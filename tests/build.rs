//! Tests for `SpcBuilder`, the way to make a file without reading one first.
//!
//! The central test is again a byte comparison against `tests/common/mod.rs`:
//! a file built from a spectrum and its metadata has to come out identical to
//! the hand-assembled fixture. That checks the builder's defaults, the header
//! layout and the writer in one go, against something written independently of
//! all three.

mod common;

use common::{DEFAULT_FIRST, DEFAULT_LAST, DEFAULT_NPTS, SpcBuilder as RawSpc};
use spc_spectra::{Spc, SpcBuilder, SpcDate, SpcError, TFlags, Technique, XType, YType, ZSpacing};

/// The fixture's y values, as `f64` but bit-identical to its `f32` ones.
///
/// The builder takes `f64` and narrows on write, so starting from the exact
/// `f32` values is what makes a byte comparison meaningful rather than a test
/// of rounding.
fn fixture_y() -> Vec<f64> {
    (0..DEFAULT_NPTS)
        .map(|i| f64::from(0.1 + i as f32 * 0.001))
        .collect()
}

/// The fixture's y values for spectrum `i` of a series, bit-identical to the
/// `f32` ramp `common::SpcBuilder::spectra` writes.
fn series_y(i: usize) -> Vec<f64> {
    (0..DEFAULT_NPTS)
        .map(|j| f64::from(i as f32 + j as f32 * 0.001))
        .collect()
}

/// The fixture's y values for spectrum `i`, as the `f32` the raw builder writes.
fn series_y32(i: usize) -> Vec<f32> {
    (0..DEFAULT_NPTS)
        .map(|j| i as f32 + j as f32 * 0.001)
        .collect()
}

/// A builder configured to match `common::SpcBuilder::new()` exactly.
fn fixture_builder() -> SpcBuilder {
    fixture_builder_from(fixture_y())
}

/// The same metadata, over any first spectrum.
fn fixture_builder_from(y: Vec<f64>) -> SpcBuilder {
    fixture_metadata(SpcBuilder::new(DEFAULT_FIRST, DEFAULT_LAST, y))
}

/// The metadata the fixture carries, applied to any constructor's output.
fn fixture_metadata(b: SpcBuilder) -> SpcBuilder {
    b.x_type(XType::Nanometers)
        .y_type(YType::Absorbance)
        .technique(Technique::Nir)
        .resolution("2nm")
        .source("NIR probe")
        .comment("synthetic test spectrum")
        .date(SpcDate {
            year: 2026,
            month: 7,
            day: 21,
            hour: 14,
            minute: 37,
        })
        .log_text("Channel=1\nIntegration=100ms")
}

#[test]
fn builds_byte_for_byte_what_the_reference_builder_assembled() {
    let expected = RawSpc::new()
        .log_text("Channel=1\nIntegration=100ms")
        .build();
    let built = fixture_builder().build().unwrap().to_bytes().unwrap();

    assert_eq!(built.len(), expected.len(), "file length differs");
    if let Some(i) = built.iter().zip(&expected).position(|(a, b)| a != b) {
        panic!(
            "byte {i} differs: built {:#04X}, fixture has {:#04X}",
            built[i], expected[i]
        );
    }
}

#[test]
fn builds_byte_for_byte_what_the_reference_builder_assembled_for_a_series() {
    // The same cross-check for a multifile record: subheader order, `subindx`,
    // `subtime` and the TMULTI flag all have to land where the independently
    // written fixture puts them.
    let expected = RawSpc::new()
        .ftflgs(TFlags::TMULTI)
        .spectra(3)
        .log_text("Channel=1\nIntegration=100ms")
        .build();

    let built = fixture_builder_from(series_y(0))
        .add_spectrum_at(1.0, series_y(1))
        .add_spectrum_at(2.0, series_y(2))
        .build()
        .unwrap()
        .to_bytes()
        .unwrap();

    assert_eq!(built.len(), expected.len(), "file length differs");
    if let Some(i) = built.iter().zip(&expected).position(|(a, b)| a != b) {
        panic!(
            "byte {i} differs: built {:#04X}, fixture has {:#04X}",
            built[i], expected[i]
        );
    }
}

#[test]
fn a_series_is_byte_for_byte_what_the_reference_builder_assembled() {
    // The z values are the ones a real instrument writes: a series timed from
    // when the instrument started, so the first one is not zero. That is the
    // case `new` cannot express at all.
    let z = [16.57f32, 17.42, 18.30];

    let expected = RawSpc::new()
        .ftflgs(TFlags::TMULTI)
        .series(
            z.iter()
                .enumerate()
                .map(|(i, &t)| (t, series_y32(i)))
                .collect(),
        )
        .log_text("Channel=1\nIntegration=100ms")
        .build();

    let spectra = z
        .iter()
        .enumerate()
        .map(|(i, &t)| (t, series_y(i)))
        .collect();
    let built = fixture_metadata(SpcBuilder::series(DEFAULT_FIRST, DEFAULT_LAST, spectra))
        .build()
        .unwrap()
        .to_bytes()
        .unwrap();

    assert_eq!(built.len(), expected.len(), "file length differs");
    if let Some(i) = built.iter().zip(&expected).position(|(a, b)| a != b) {
        panic!(
            "byte {i} differs: built {:#04X}, fixture has {:#04X}",
            built[i], expected[i]
        );
    }
}

#[test]
fn a_series_records_the_first_spectrum_s_z_value_too() {
    let spc = SpcBuilder::series(
        0.0,
        1.0,
        vec![(16.57, vec![1.0, 2.0]), (17.42, vec![3.0, 4.0])],
    )
    .z_type(XType::Seconds)
    .build()
    .unwrap();
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

    let times: Vec<f32> = read.subfiles.iter().map(|s| s.subheader.subtime).collect();
    assert_eq!(times, vec![16.57, 17.42], "the first z value must survive");
}

#[test]
fn a_one_spectrum_series_is_the_same_file_as_a_plain_one() {
    // Nothing about `series` should change the ordinary single-spectrum case.
    let plain = fixture_builder().build().unwrap().to_bytes().unwrap();
    let as_series = fixture_metadata(SpcBuilder::series(
        DEFAULT_FIRST,
        DEFAULT_LAST,
        vec![(0.0, fixture_y())],
    ))
    .build()
    .unwrap()
    .to_bytes()
    .unwrap();

    assert_eq!(plain, as_series);
}

#[test]
fn the_z_spacing_reaches_the_file_as_the_flag_it_is() {
    let cases = [
        (ZSpacing::Even, 0u8),
        (ZSpacing::Uneven, TFlags::TORDRD),
        (ZSpacing::Unordered, TFlags::TRANDM),
    ];

    for (spacing, bit) in cases {
        let spc = SpcBuilder::series(0.0, 1.0, vec![(0.0, vec![1.0, 2.0]), (1.0, vec![3.0, 4.0])])
            .z_spacing(spacing)
            .build()
            .unwrap();
        let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

        let z = read.header.ftflgs.0 & (TFlags::TORDRD | TFlags::TRANDM);
        assert_eq!(z, bit, "{spacing:?} set the wrong flags");
        assert_eq!(
            read.header.z_spacing(),
            spacing,
            "{spacing:?} did not survive"
        );
    }
}

#[test]
fn setting_the_z_spacing_back_to_even_clears_the_flags_again() {
    // The builder is a value that can be reconfigured before build(); a flag
    // that could only ever be added would leave a stale claim behind.
    let spc = SpcBuilder::series(0.0, 1.0, vec![(0.0, vec![1.0, 2.0])])
        .z_spacing(ZSpacing::Uneven)
        .z_spacing(ZSpacing::Even)
        .build()
        .unwrap();

    assert_eq!(spc.header.ftflgs.0 & (TFlags::TORDRD | TFlags::TRANDM), 0);
    assert_eq!(spc.header.z_spacing(), ZSpacing::Even);
}

#[test]
fn evenly_spaced_is_the_default_and_says_so() {
    let spc = SpcBuilder::new(0.0, 1.0, vec![1.0, 2.0]).build().unwrap();
    assert_eq!(spc.header.z_spacing(), ZSpacing::Even);
    assert_eq!(spc.header.ftflgs.0 & (TFlags::TORDRD | TFlags::TRANDM), 0);
}

#[test]
fn an_empty_series_is_refused_rather_than_a_panic() {
    let err = SpcBuilder::series(900.0, 1700.0, Vec::new())
        .build()
        .unwrap_err();
    assert!(matches!(err, SpcError::NotWritable { .. }), "got {err:?}");
}

#[test]
fn a_series_carries_its_z_values_and_flags_itself_as_one() {
    let spc = SpcBuilder::new(0.0, 1.0, vec![1.0, 2.0])
        .z_type(XType::Seconds)
        .add_spectrum_at(1.5, vec![3.0, 4.0])
        .add_spectrum_at(3.0, vec![5.0, 6.0])
        .build()
        .expect("three spectra of equal length are writable");
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).expect("must be readable");

    assert_eq!(read.header.fnsub, 3);
    assert_eq!(read.subfiles.len(), 3);
    assert!(read.header.ftflgs.contains(TFlags::TMULTI));
    assert_eq!(read.header.fztype, XType::Seconds);

    let times: Vec<f32> = read.subfiles.iter().map(|s| s.subheader.subtime).collect();
    assert_eq!(times, vec![0.0, 1.5, 3.0]);
    let indices: Vec<u16> = read.subfiles.iter().map(|s| s.subheader.subindx).collect();
    assert_eq!(indices, vec![0, 1, 2]);
    assert_eq!(
        read.subfiles[2].y,
        vec![5.0, 6.0],
        "wrong data for spectrum 3"
    );
}

#[test]
fn a_spectrum_added_without_a_z_value_leaves_it_at_zero() {
    // Numbering them 0, 1, 2 would be data the crate invented.
    let spc = SpcBuilder::new(0.0, 1.0, vec![1.0, 2.0])
        .add_spectrum(vec![3.0, 4.0])
        .build()
        .unwrap();

    assert_eq!(spc.subfiles.len(), 2);
    assert!(spc.subfiles.iter().all(|s| s.subheader.subtime == 0.0));
}

#[test]
fn spectra_of_different_lengths_are_refused() {
    // They all share the one x axis the range describes, so a shorter one
    // would silently get a different step over the same span.
    let err = SpcBuilder::new(0.0, 1.0, vec![1.0, 2.0])
        .add_spectrum(vec![3.0, 4.0, 5.0])
        .build()
        .unwrap_err();

    // The message matters, not just the refusal: without the builder's own
    // check the file is still rejected, but by the writer's point-count
    // cross-check, whose wording is about a field the caller never set.
    assert!(matches!(err, SpcError::NotWritable { .. }), "got {err:?}");
    let msg = err.to_string();
    assert!(
        msg.contains("same number of points"),
        "the refusal should name the real problem, got: {msg}"
    );
}

#[test]
fn a_single_spectrum_is_not_flagged_as_a_series() {
    let spc = SpcBuilder::new(0.0, 1.0, vec![1.0, 2.0]).build().unwrap();
    assert_eq!(spc.header.fnsub, 1);
    assert!(!spc.header.ftflgs.contains(TFlags::TMULTI));
}

#[test]
fn the_defaults_alone_produce_a_readable_file() {
    let spc = SpcBuilder::new(0.0, 10.0, vec![1.0, 2.0, 3.0])
        .build()
        .expect("range and data must be enough");
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).expect("must be readable");

    assert_eq!(read.subfiles[0].y, &[1.0, 2.0, 3.0]);
    assert_eq!(read.subfiles[0].x, &[0.0, 5.0, 10.0]);
    assert_eq!(read.header.fnsub, 1);
    assert_eq!(read.header.fversn, 0x4B);
    assert_eq!(read.header.date, None, "no date was set");
    assert!(read.log.is_none(), "no log block was set");
}

#[test]
fn a_descending_axis_is_ordinary_and_supported() {
    let spc = SpcBuilder::new(4000.0, 400.0, vec![1.0, 2.0, 3.0, 4.0])
        .x_type(XType::Wavenumber)
        .technique(Technique::FtirOrRaman)
        .build()
        .unwrap();
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

    assert_eq!(read.subfiles[0].x, &[4000.0, 2800.0, 1600.0, 400.0]);
    assert_eq!(read.x_label(), "Wavenumber (cm-1)");
}

#[test]
fn custom_axis_labels_set_the_flag_and_survive_a_round_trip() {
    let spc = SpcBuilder::new(900.0, 1700.0, vec![1.0, 2.0])
        .custom_axis_labels(&["Wellenlaenge", "Extinktion"])
        .build()
        .unwrap();
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

    assert_eq!(read.x_label(), "Wellenlaenge");
    assert_eq!(read.y_label(), "Extinktion");
}

#[test]
fn a_label_that_does_not_fit_is_dropped_rather_than_cut_in_half() {
    // The field holds 30 bytes for all three labels together.
    let spc = SpcBuilder::new(0.0, 1.0, vec![1.0, 2.0])
        .custom_axis_labels(&["short", &"x".repeat(40)])
        .build()
        .unwrap();
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

    assert_eq!(read.x_label(), "short");
    assert_eq!(
        read.y_label(),
        "Arbitrary intensity",
        "the oversized label must not appear at all"
    );
}

#[test]
fn the_scan_count_goes_into_the_field_the_format_has_for_it() {
    // Not only into the log text: another program looks in subscan, and has no
    // reason to know that "Averages=32" in the log means the same thing.
    let spc = SpcBuilder::new(900.0, 1700.0, vec![1.0, 2.0])
        .scans(32)
        .build()
        .unwrap();
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

    assert_eq!(read.subfiles[0].subheader.subscan, 32);
}

#[test]
fn a_single_scan_is_the_default() {
    let spc = SpcBuilder::new(900.0, 1700.0, vec![1.0, 2.0])
        .build()
        .unwrap();
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

    assert_eq!(read.subfiles[0].subheader.subscan, 1);
}

#[test]
fn fields_without_a_setter_are_set_on_the_result() {
    // The builder covers what describes a measurement; the format's remaining
    // fields are reached through the public fields of the Spc it returns. If
    // that stops working, the builder's documentation is a lie.
    let mut spc = SpcBuilder::new(900.0, 1700.0, vec![1.0, 2.0])
        .build()
        .unwrap();
    spc.header.ffactor = 2.0;
    spc.header.fpeakpt = 7;
    spc.subfiles[0].subheader.subtime = 12.5;

    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();
    assert_eq!(read.header.ffactor, 2.0);
    assert_eq!(read.header.fpeakpt, 7);
    assert_eq!(read.subfiles[0].subheader.subtime, 12.5);
}

#[test]
fn a_log_block_can_be_binary_only() {
    let spc = SpcBuilder::new(0.0, 1.0, vec![1.0, 2.0])
        .log_binary(vec![0xDE, 0xAD, 0xBE, 0xEF])
        .build()
        .unwrap();
    let read = Spc::from_bytes(&spc.to_bytes().unwrap()).unwrap();

    let log = read
        .log
        .expect("a binary-only log block is still a log block");
    assert_eq!(log.binary, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(log.text, "");
}

#[test]
fn a_built_file_can_be_written_to_disk_and_read_back() {
    let path = std::env::temp_dir().join("spc-spectra-build-to-path-test.spc");
    let spc = fixture_builder().build().unwrap();

    spc.to_path(&path).unwrap();
    let read = Spc::from_path(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(read.subfiles[0].y.len(), DEFAULT_NPTS as usize);
    assert_eq!(read.header.fsource, "NIR probe");
    assert_eq!(read.log.unwrap().get("Channel"), Some("1"));
}

#[test]
fn an_empty_spectrum_is_refused_at_build_time() {
    // Not at to_bytes() time, and certainly not at the filesystem.
    match SpcBuilder::new(0.0, 1.0, Vec::new()).build() {
        Err(SpcError::NotWritable { detail }) => assert!(detail.contains("no points"), "{detail}"),
        other => panic!("an empty spectrum must be refused, got {other:?}"),
    }
}

#[test]
fn a_text_field_that_does_not_fit_is_refused_at_build_time() {
    for (field, builder) in [
        ("fsource", fixture_builder().source("x".repeat(10))),
        ("fres", fixture_builder().resolution("x".repeat(10))),
        ("fcmnt", fixture_builder().comment("x".repeat(131))),
        ("fmethod", fixture_builder().method("x".repeat(49))),
    ] {
        match builder.build() {
            Err(SpcError::FieldTooLong { field: got, .. }) => assert_eq!(got, field),
            other => panic!("{field} is over-long but was accepted: {other:?}"),
        }
    }

    // The exact width still fits: these fields need no terminator.
    assert!(fixture_builder().source("x".repeat(9)).build().is_ok());
    assert!(fixture_builder().comment("x".repeat(130)).build().is_ok());
}

#[test]
fn a_non_finite_end_point_is_refused_at_build_time() {
    for bad in [f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                SpcBuilder::new(bad, 1700.0, vec![1.0, 2.0]).build(),
                Err(SpcError::MalformedHeader { .. })
            ),
            "ffirst = {bad} would poison the whole x axis"
        );
    }
}

#[test]
fn a_y_value_with_no_32_bit_equivalent_is_refused() {
    match SpcBuilder::new(0.0, 1.0, vec![1.0, 1e300]).build() {
        Err(SpcError::ValueNotRepresentable { index, value }) => {
            assert_eq!(index, 1);
            assert_eq!(value, 1e300);
        }
        other => panic!("1e300 does not fit in an f32, got {other:?}"),
    }
}
