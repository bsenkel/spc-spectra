//! Every way a file can fail to be written, refused for the right stated
//! reason.
//!
//! The counterpart to `unsupported.rs`. These `Spc` values are assembled by
//! hand — read a valid file, then break one thing — because the parser cannot
//! produce most of them. That is the point: the writer must not depend on its
//! input having come from the reader.

mod common;

use common::{DEFAULT_NPTS, SpcBuilder as RawSpc};
use spc_spectra::{Spc, SpcError, TFlags, Unsupported};

fn valid() -> Spc {
    Spc::from_bytes(&RawSpc::new().build()).expect("the fixture must parse")
}

#[track_caller]
fn refusal(spc: &Spc) -> SpcError {
    spc.to_bytes()
        .expect_err("this file should not have been writable")
}

/// A fixed-point file this crate read always divides back out exactly, so
/// these two only arise once a caller has computed new y values. Both destroy
/// the value outright, which is what separates them from ordinary rounding.
fn fixed_point_file() -> Spc {
    // Exponent 2: a step of 2^-30, and a largest value just under 2.
    Spc::from_bytes(&RawSpc::new().fixed_point(2, vec![1, 2, 3]).build())
        .expect("the fixed-point fixture must parse")
}

#[test]
fn a_y_value_too_large_for_the_fixed_point_scale_is_named() {
    let mut spc = fixed_point_file();
    spc.subfiles[0].y[1] = 4.0; // the scale tops out just under 2

    match refusal(&spc) {
        SpcError::ValueNotRepresentable { index, value } => {
            assert_eq!(index, 1);
            assert_eq!(value, 4.0);
        }
        other => panic!("expected ValueNotRepresentable, got {other:?}"),
    }
}

#[test]
fn a_y_value_that_the_fixed_point_scale_would_erase_is_named() {
    let mut spc = fixed_point_file();
    // Below half a step. Rounding would silently make this a zero, which is
    // not a rounded measurement but the absence of one.
    spc.subfiles[0].y[2] = 1e-12;

    match refusal(&spc) {
        SpcError::ValueNotRepresentable { index, value } => {
            assert_eq!(index, 2);
            assert_eq!(value, 1e-12);
        }
        other => panic!("expected ValueNotRepresentable, got {other:?}"),
    }
}

#[test]
fn the_edges_of_the_fixed_point_range_are_where_they_belong() {
    // Exponent 0 gives a step of 2^-32, so i32::MIN lands exactly on -0.5 and
    // one step further is out. The range check compares against that same
    // bound, which is precisely where an off-by-one would let a value through
    // that the cast then saturates, or refuse one the reader would hand back.
    let scale = f64::exp2(-32.0);
    let mut spc = Spc::from_bytes(&RawSpc::new().fixed_point(0, vec![0, 0]).build())
        .expect("the fixed-point fixture must parse");

    spc.subfiles[0].y[0] = f64::from(i32::MIN) * scale;
    spc.subfiles[0].y[1] = f64::from(i32::MAX) * scale;
    let bytes = spc
        .to_bytes()
        .expect("both extremes are representable and must be written");
    let back = Spc::from_bytes(&bytes).expect("and must read back");
    assert_eq!(back.subfiles[0].y[0], f64::from(i32::MIN) * scale);
    assert_eq!(back.subfiles[0].y[1], f64::from(i32::MAX) * scale);

    // One step beyond either end is not.
    for (index, value) in [
        (0usize, (f64::from(i32::MIN) - 1.0) * scale),
        (1, (f64::from(i32::MAX) + 1.0) * scale),
    ] {
        let mut spc = spc.clone();
        spc.subfiles[0].y[index] = value;
        match refusal(&spc) {
            SpcError::ValueNotRepresentable { index: i, .. } => assert_eq!(i, index),
            other => panic!("expected ValueNotRepresentable, got {other:?}"),
        }
    }
}

#[test]
fn rounding_to_the_nearest_fixed_point_step_is_not_a_refusal() {
    // Losing precision is inherent to the format, exactly as narrowing to f32
    // is on the float path. Only total loss is refused.
    let mut spc = fixed_point_file();
    spc.subfiles[0].y[0] = 0.1234567890123;

    let bytes = spc.to_bytes().expect("mere rounding must not be refused");
    let back = Spc::from_bytes(&bytes).expect("and must read back");
    assert!((back.subfiles[0].y[0] - 0.1234567890123).abs() < 1e-9);
}

#[test]
fn a_y_value_that_is_finite_but_not_representable_is_named() {
    let mut spc = valid();
    spc.subfiles[0].y[42] = 1e300;

    match refusal(&spc) {
        SpcError::ValueNotRepresentable { index, value } => {
            assert_eq!(index, 42);
            assert_eq!(value, 1e300);
        }
        other => panic!("expected ValueNotRepresentable, got {other:?}"),
    }
}

#[test]
fn an_x_axis_that_is_not_the_generated_one_is_refused() {
    // The x axis is never stored, so writing this file would replace the axis
    // with the evenly spaced one — silently, and with no way to notice.
    let mut spc = valid();
    spc.subfiles[0].x[100] += 0.5;

    match refusal(&spc) {
        SpcError::NotWritable { detail } => assert!(detail.contains("x axis"), "{detail}"),
        other => panic!("expected NotWritable, got {other:?}"),
    }
}

#[test]
fn an_x_axis_of_the_wrong_length_is_refused() {
    let mut spc = valid();
    spc.subfiles[0].x.pop();

    match refusal(&spc) {
        SpcError::NotWritable { detail } => assert!(detail.contains("length"), "{detail}"),
        other => panic!("expected NotWritable, got {other:?}"),
    }
}

#[test]
fn a_point_count_that_disagrees_with_the_data_is_refused() {
    // fnpts says 800, but 801 y values follow. Writing either number would
    // produce a file that contradicts itself.
    let mut spc = valid();
    spc.header.fnpts = DEFAULT_NPTS - 1;

    match refusal(&spc) {
        SpcError::NotWritable { detail } => assert!(detail.contains("point count"), "{detail}"),
        other => panic!("expected NotWritable, got {other:?}"),
    }
}

#[test]
fn an_empty_spectrum_is_refused() {
    let mut spc = valid();
    spc.subfiles[0].x.clear();
    spc.subfiles[0].y.clear();
    spc.header.fnpts = 0;

    match refusal(&spc) {
        SpcError::NotWritable { detail } => assert!(detail.contains("no points"), "{detail}"),
        other => panic!("expected NotWritable, got {other:?}"),
    }
}

#[test]
fn a_file_with_no_subfile_at_all_is_refused() {
    let mut spc = valid();
    spc.subfiles.clear();

    match refusal(&spc) {
        SpcError::NotWritable { detail } => assert!(detail.contains("no subfile"), "{detail}"),
        other => panic!("expected NotWritable, got {other:?}"),
    }
}

#[test]
fn a_subfile_count_that_contradicts_fnsub_is_refused() {
    // `fnsub` is what a reader loops over, so writing more subfiles than it
    // announces would produce a file whose tail is unreachable.
    let mut spc = valid();
    let extra = spc.subfiles[0].clone();
    spc.subfiles.push(extra);

    assert!(
        matches!(refusal(&spc), SpcError::NotWritable { .. }),
        "a subfile count that disagrees with fnsub must be refused"
    );
}

#[test]
fn the_multifile_flag_is_written_back_however_it_was_found() {
    // TMULTI is an observation the file carried, not a fact about the subfile
    // count. A file that contradicts itself has to come back unchanged rather
    // than corrected, or reading and writing would cover different sets.
    let mut spc = valid();
    spc.header.ftflgs.0 |= TFlags::TMULTI;

    let bytes = spc
        .to_bytes()
        .expect("one subfile flagged TMULTI is writable");
    let again = Spc::from_bytes(&bytes).expect("and readable again");
    assert!(again.header.ftflgs.contains(TFlags::TMULTI));
    assert_eq!(again.subfiles.len(), 1);
}

#[test]
fn header_variants_the_reader_refuses_cannot_be_written_either() {
    // The writer runs the reader's own validation, so the set of writable files
    // and the set of readable ones stay the same by construction.
    /// Name of the variant, how to introduce it, and the error it must give.
    type Case = (&'static str, fn(&mut Spc), Unsupported);

    let cases: [Case; 4] = [
        (
            "TXVALS",
            |s| s.header.ftflgs.0 |= TFlags::TXVALS,
            Unsupported::ExplicitXValues,
        ),
        (
            "TSPREC",
            |s| s.header.ftflgs.0 |= TFlags::TSPREC,
            Unsupported::SixteenBitY,
        ),
        (
            "fwplanes",
            |s| s.header.fwplanes = 4,
            Unsupported::WPlanes { fwplanes: 4 },
        ),
        (
            "subexp contradicting fexp under TMULTI",
            |s| {
                s.header.ftflgs.0 |= TFlags::TMULTI;
                s.subfiles[0].subheader.subexp = 3;
            },
            Unsupported::FixedPointSubfileY {
                subexp: 3,
                fexp: -128,
            },
        ),
    ];

    for (name, break_it, expected) in cases {
        let mut spc = valid();
        break_it(&mut spc);
        match refusal(&spc) {
            SpcError::Unsupported(got) => assert_eq!(got, expected, "{name}"),
            other => panic!("{name} should be refused as unsupported, got {other:?}"),
        }
    }
}

#[test]
fn a_non_finite_end_point_is_refused() {
    let mut spc = valid();
    spc.header.flast = f64::NAN;

    assert!(
        matches!(refusal(&spc), SpcError::MalformedHeader { .. }),
        "a NaN end point would fill the whole x axis with NaN"
    );
}

#[test]
fn a_refused_write_leaves_no_file_behind_a_partial_one() {
    // to_path serialises first and writes second, so a rejected file must not
    // half-overwrite whatever was there before.
    let path = std::env::temp_dir().join("spc-spectra-refused-write-test.spc");
    std::fs::write(&path, b"previous contents").unwrap();

    let mut spc = valid();
    spc.header.flast = f64::NAN;
    assert!(spc.to_path(&path).is_err());

    let after = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(after, b"previous contents", "the file was touched anyway");
}
