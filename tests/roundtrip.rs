//! Happy-path tests: everything the builder writes must come back unchanged.

mod common;

use common::{DEFAULT_FIRST, DEFAULT_LAST, DEFAULT_NPTS, SpcBuilder};
use spc_spectra::{Header, Spc, SpcDate, SpcError, SubHeader, Technique, XType, YType};

fn parse(builder: &SpcBuilder) -> Spc {
    Spc::from_bytes(&builder.build()).expect("the default builder must produce a readable file")
}

#[test]
fn reads_the_same_file_from_disk() {
    let path = std::env::temp_dir().join("spc-spectra-from-path-test.spc");
    std::fs::write(&path, SpcBuilder::new().build()).expect("could not write the temp file");

    let from_disk = Spc::from_path(&path).expect("from_path must read what from_bytes accepts");
    std::fs::remove_file(&path).ok();

    assert_eq!(from_disk.y(), parse(&SpcBuilder::new()).y());
}

#[test]
fn a_missing_file_reports_an_io_error() {
    let err = Spc::from_path("/nonexistent/definitely-not-here.spc").unwrap_err();
    assert!(matches!(err, SpcError::Io(_)), "got {err:?}");
    assert!(
        std::error::Error::source(&err).is_some(),
        "the io error should be chained"
    );
}

#[test]
fn reads_every_header_field_back() {
    let spc = parse(&SpcBuilder::new());
    let h = &spc.header;

    assert_eq!(h.fversn, 0x4B);
    assert_eq!(h.fexp, -128);
    assert_eq!(h.fnpts, DEFAULT_NPTS);
    assert_eq!(h.ffirst, DEFAULT_FIRST);
    assert_eq!(h.flast, DEFAULT_LAST);
    assert_eq!(h.fnsub, 1);
    assert_eq!(h.fexper, Technique::Nir);
    assert_eq!(h.fxtype, XType::Nanometers);
    assert_eq!(h.fytype, YType::Absorbance);
    assert_eq!(h.fres, "2nm");
    assert_eq!(h.fsource, "NIR probe");
    assert_eq!(h.fcmnt, "synthetic test spectrum");
    assert_eq!(h.ffactor, 1.0);
}

#[test]
fn the_log_block_lands_where_the_size_arithmetic_says() {
    let builder = SpcBuilder::new();
    let expected = Header::SIZE + SubHeader::SIZE + DEFAULT_NPTS as usize * 4;
    assert_eq!(
        expected, 3748,
        "this is the geometry a real 801-point export has"
    );

    let spc = parse(&builder);
    assert_eq!(spc.header.flogoff as usize, expected);
}

#[test]
fn y_values_survive_bit_for_bit() {
    let values: Vec<f32> = vec![0.0, -1.5, 1e-30, 3.402_823_5e38, -0.0, 0.123_456_79];
    let builder = SpcBuilder::new().y(values.clone());
    let spc = parse(&builder);

    assert_eq!(spc.y().len(), values.len());
    for (got, want) in spc.y().iter().zip(&values) {
        assert_eq!(
            got.to_bits(),
            f64::from(*want).to_bits(),
            "y value changed: {want}"
        );
    }
}

#[test]
fn the_x_axis_spans_the_declared_range() {
    let spc = parse(&SpcBuilder::new());
    let x = spc.x();

    assert_eq!(x.len(), DEFAULT_NPTS as usize);
    assert_eq!(x[0], DEFAULT_FIRST, "first x must be exactly ffirst");
    assert_eq!(x[x.len() - 1], DEFAULT_LAST, "last x must be exactly flast");
    assert_eq!(x[1], 901.0, "801 points over 900..1700 nm is a 1 nm step");
    assert_eq!(x.len(), spc.y().len(), "x and y must be the same length");
}

#[test]
fn subnpts_zero_inherits_the_count_from_the_main_header() {
    let spc = parse(&SpcBuilder::new().subnpts(0));
    assert_eq!(
        spc.subfiles[0].subheader.subnpts, 0,
        "the raw field stays zero"
    );
    assert_eq!(
        spc.y().len(),
        DEFAULT_NPTS as usize,
        "but the count comes from fnpts"
    );
}

#[test]
fn an_explicit_subnpts_gives_the_same_result() {
    let inherited = parse(&SpcBuilder::new().subnpts(0));
    let explicit = parse(&SpcBuilder::new().subnpts(DEFAULT_NPTS));
    assert_eq!(inherited.y(), explicit.y());
}

#[test]
fn dates_round_trip_through_the_packed_field() {
    for (y, mo, d, h, mi) in [
        (2026, 7, 21, 14, 37),
        (1996, 1, 1, 0, 0),
        (2000, 12, 31, 23, 59),
        (2024, 2, 29, 8, 5),
    ] {
        let spc = parse(&SpcBuilder::new().date(y, mo, d, h, mi));
        assert_eq!(
            spc.header.date,
            Some(SpcDate {
                year: y,
                month: mo,
                day: d,
                hour: h,
                minute: mi
            })
        );
    }
}

#[test]
fn a_zero_date_word_yields_none() {
    let spc = parse(&SpcBuilder::new().fdate(0));
    assert_eq!(spc.header.date, None);
    assert_eq!(spc.header.fdate, 0);
}

#[test]
fn a_log_block_using_nulls_as_separators_keeps_every_entry() {
    // Some instruments separate log entries with NUL instead of newline.
    // Stopping at the first NUL, as a fixed-width header field would, throws
    // away everything after the first entry — here nothing may be lost.
    let spc = parse(&SpcBuilder::new().log_text("Channel=1\0Integration=100ms\0Averages=32"));
    let log = spc.log.expect("the default builder writes a log block");

    let pairs: Vec<_> = log.entries().collect();
    assert_eq!(
        pairs,
        vec![
            ("Channel", "1"),
            ("Integration", "100ms"),
            ("Averages", "32")
        ],
        "NUL-separated entries must all survive; got text {:?}",
        log.text
    );
}

#[test]
fn the_log_block_text_comes_through_and_splits_into_pairs() {
    let spc =
        parse(&SpcBuilder::new().log_text("Channel=1\nIntegration=100ms\nOperator=day shift\n"));
    let log = spc.log.expect("the default builder writes a log block");

    assert!(log.text.contains("Integration=100ms"));

    let pairs: Vec<_> = log.entries().collect();
    assert_eq!(
        pairs,
        vec![
            ("Channel", "1"),
            ("Integration", "100ms"),
            ("Operator", "day shift")
        ]
    );
    assert_eq!(log.get("channel"), Some("1"), "lookup ignores case");
    assert_eq!(log.get("missing"), None);
}

#[test]
fn lines_without_an_equals_sign_are_skipped_not_fatal() {
    let spc = parse(&SpcBuilder::new().log_text("header line\nChannel=1\n\n=novalue\n"));
    let log = spc.log.unwrap();
    assert_eq!(log.entries().collect::<Vec<_>>(), vec![("Channel", "1")]);
}

#[test]
fn a_binary_log_area_is_passed_through_untouched() {
    let blob = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];
    let spc = parse(&SpcBuilder::new().log_binary(blob.clone()).log_text("k=v\n"));
    let log = spc.log.unwrap();
    assert_eq!(log.binary, blob);
    assert_eq!(log.get("k"), Some("v"));
}

#[test]
fn a_file_without_a_log_block_parses_fine() {
    let spc = parse(&SpcBuilder::new().no_log());
    assert_eq!(spc.header.flogoff, 0);
    assert!(spc.log.is_none());
    assert_eq!(spc.y().len(), DEFAULT_NPTS as usize);
}

#[test]
fn a_log_offset_past_the_end_is_dropped_rather_than_fatal() {
    let mut raw = SpcBuilder::new().no_log().build();
    // Point flogoff far beyond the file; the spectrum itself is still valid.
    raw[248..252].copy_from_slice(&999_999u32.to_le_bytes());

    let spc = Spc::from_bytes(&raw).expect("a broken log block must not invalidate the spectrum");
    assert!(spc.log.is_none());
    assert_eq!(spc.y().len(), DEFAULT_NPTS as usize);
}

#[test]
fn a_single_point_spectrum_works() {
    let spc = parse(&SpcBuilder::new().y(vec![0.42]).range(1000.0, 1000.0));
    assert_eq!(spc.x(), &[1000.0]);
    assert_eq!(spc.y(), &[f64::from(0.42f32)]);
}

#[test]
fn a_descending_wavenumber_axis_works() {
    let spc = parse(
        &SpcBuilder::new()
            .y(vec![1.0, 2.0, 3.0, 4.0])
            .range(4000.0, 400.0),
    );
    assert_eq!(spc.x(), &[4000.0, 2800.0, 1600.0, 400.0]);
}

#[test]
fn axis_labels_default_to_the_unit_codes() {
    let spc = parse(&SpcBuilder::new());
    assert_eq!(spc.x_label(), "Nanometers (nm)");
    assert_eq!(spc.y_label(), "Absorbance");
}

#[test]
fn custom_axis_labels_win_when_talabs_is_set() {
    let spc = parse(&SpcBuilder::new().custom_axis_labels(&["Wellenlaenge", "Extinktion"]));
    assert_eq!(spc.x_label(), "Wellenlaenge");
    assert_eq!(spc.y_label(), "Extinktion");
}

#[test]
fn unknown_unit_codes_do_not_fail_the_parse() {
    let spc = parse(&SpcBuilder::new().axis_types(200, 201));
    assert_eq!(spc.header.fxtype, XType::Other(200));
    assert_eq!(spc.x_label(), "Unknown");
}

#[test]
fn points_pairs_up_x_and_y() {
    let spc = parse(&SpcBuilder::new().y(vec![1.0, 2.0, 3.0]).range(0.0, 2.0));
    let pts: Vec<_> = spc.subfiles[0].points().collect();
    assert_eq!(pts, vec![(0.0, 1.0), (1.0, 2.0), (2.0, 3.0)]);
    assert_eq!(spc.subfiles[0].len(), 3);
    assert!(!spc.subfiles[0].is_empty());
}

#[test]
fn every_truncation_of_the_spectrum_is_an_error_and_never_a_panic() {
    let builder = SpcBuilder::new();
    let full = builder.build();
    // Everything up to the end of the y values is required data. Step by a
    // prime so the cut lands mid-field as often as mid-structure.
    for len in (0..builder.log_offset() as usize).step_by(7) {
        match Spc::from_bytes(&full[..len]) {
            Err(SpcError::TooShort { .. }) => {}
            Err(other) => panic!("prefix of {len} bytes gave the wrong error: {other:?}"),
            Ok(_) => panic!("prefix of {len} bytes must not parse as a complete file"),
        }
    }
    assert!(
        Spc::from_bytes(&full).is_ok(),
        "the complete file must parse"
    );
}

#[test]
fn a_truncated_log_block_still_yields_the_spectrum() {
    let builder = SpcBuilder::new();
    let full = builder.build();
    // Cut anywhere at or after the end of the y values: the spectrum is
    // complete, only the vendor log is damaged, so reading must still succeed.
    for len in (builder.log_offset() as usize..full.len()).step_by(3) {
        let spc = Spc::from_bytes(&full[..len])
            .unwrap_or_else(|e| panic!("prefix of {len} bytes should still parse: {e}"));
        assert_eq!(spc.y().len(), DEFAULT_NPTS as usize);
    }
}

#[test]
fn a_zero_point_count_is_rejected() {
    let raw = SpcBuilder::new().y(Vec::new()).fnpts(0).no_log().build();
    assert!(matches!(
        Spc::from_bytes(&raw),
        Err(SpcError::InvalidPointCount(0))
    ));
}

#[test]
fn a_point_count_larger_than_the_file_is_rejected_without_allocating() {
    let raw = SpcBuilder::new()
        .fnpts(u32::MAX)
        .subnpts(u32::MAX)
        .no_log()
        .build();
    assert!(matches!(
        Spc::from_bytes(&raw),
        Err(SpcError::TooShort { .. })
    ));
}

#[test]
fn errors_render_a_useful_message() {
    let err = Spc::from_bytes(&[]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("main header"), "unhelpful message: {msg}");
}
