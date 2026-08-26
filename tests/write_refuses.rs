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

#[test]
fn a_text_field_that_no_longer_fits_is_named() {
    let mut spc = valid();
    spc.header.fcmnt = "x".repeat(131);

    match refusal(&spc) {
        SpcError::FieldTooLong { field, max, len } => {
            assert_eq!((field, max, len), ("fcmnt", 130, 131));
        }
        other => panic!("expected FieldTooLong, got {other:?}"),
    }
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
fn a_second_subfile_is_refused_as_the_unsupported_variant_it_is() {
    // Not a NotWritable: multifile records are a real part of the format that
    // this version does not do yet, and the error should say so.
    let mut spc = valid();
    let extra = spc.subfiles[0].clone();
    spc.subfiles.push(extra);

    assert!(
        matches!(refusal(&spc), SpcError::Unsupported(Unsupported::MultiFile)),
        "a second subfile must report MultiFile"
    );
}

#[test]
fn header_variants_the_reader_refuses_cannot_be_written_either() {
    // The writer runs the reader's own validation, so the set of writable files
    // and the set of readable ones stay the same by construction.
    /// Name of the variant, how to introduce it, and the error it must give.
    type Case = (&'static str, fn(&mut Spc), Unsupported);

    let cases: [Case; 5] = [
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
            "TMULTI",
            |s| s.header.ftflgs.0 |= TFlags::TMULTI,
            Unsupported::MultiFile,
        ),
        (
            "fwplanes",
            |s| s.header.fwplanes = 4,
            Unsupported::WPlanes { fwplanes: 4 },
        ),
        (
            "fexp",
            |s| s.header.fexp = 3,
            Unsupported::FixedPointY { fexp: 3 },
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
    spc.header.fcmnt = "x".repeat(200);
    assert!(spc.to_path(&path).is_err());

    let after = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(after, b"previous contents", "the file was touched anyway");
}
