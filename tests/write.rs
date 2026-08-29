//! Writing tests: what is written must be readable, and must be the same file.
//!
//! The reference for the byte layout is `tests/common/mod.rs`, which assembles
//! an SPC file by hand and was written before the writer existed. Checking the
//! writer against it, rather than only against this crate's own reader, is what
//! makes these tests capable of catching a field written in the wrong place —
//! a reader and a writer that share a mistake would round-trip happily.

mod common;

use common::{DEFAULT_FIRST, DEFAULT_LAST, DEFAULT_NPTS, SpcBuilder as RawSpc};
use spc_spectra::{Spc, SpcDate, TFlags, Technique, XType, YType};

/// The default fixture, minus the trailing newline in the log text.
///
/// Writing normalises that away — the reader trims it, so keeping it would mean
/// a file could never be written back as itself. Everything else in the fixture
/// is byte-comparable as it stands.
fn reference() -> RawSpc {
    RawSpc::new().log_text("Channel=1\nIntegration=100ms")
}

fn parse(raw: &[u8]) -> Spc {
    Spc::from_bytes(raw).expect("the fixture must parse")
}

#[test]
fn writes_byte_for_byte_what_the_reference_builder_assembled() {
    let raw = reference().build();
    let written = parse(&raw)
        .to_bytes()
        .expect("the fixture must be writable");

    assert_eq!(written.len(), raw.len(), "file length changed");
    if let Some(i) = written.iter().zip(&raw).position(|(a, b)| a != b) {
        panic!(
            "byte {i} differs: wrote {:#04X}, expected {:#04X}",
            written[i], raw[i]
        );
    }
}

/// The configurations the byte comparison runs over.
///
/// One default fixture proves the layout is right for one geometry. Most
/// layout mistakes — an offset computed from the wrong base, a field written
/// before the one it should follow, a size that happens to match — survive that
/// single case and show up only when the shape changes. Each entry is named so
/// a failure says which shape broke it.
fn byte_comparison_cases() -> Vec<(&'static str, RawSpc)> {
    let ramp = |n: usize| -> Vec<f32> { (0..n).map(|i| 0.1 + i as f32 * 0.001).collect() };

    vec![
        ("default", reference()),
        // Point counts, including the ones where the x arithmetic degenerates.
        ("one point", reference().y(vec![0.42]).range(1000.0, 1000.0)),
        ("two points", reference().y(ramp(2)).range(900.0, 1700.0)),
        ("three points", reference().y(ramp(3)).range(900.0, 1700.0)),
        ("odd count", reference().y(ramp(407)).range(900.0, 1700.0)),
        (
            "large count",
            reference().y(ramp(5000)).range(900.0, 1700.0),
        ),
        // Axis directions and magnitudes.
        (
            "descending axis",
            reference().y(ramp(4)).range(4000.0, 400.0),
        ),
        (
            "negative range",
            reference().y(ramp(10)).range(-500.0, -100.0),
        ),
        ("crosses zero", reference().y(ramp(10)).range(-5.0, 5.0)),
        (
            "fractional range",
            reference().y(ramp(10)).range(0.001, 0.002),
        ),
        (
            "huge magnitudes",
            reference().y(ramp(10)).range(1e100, 2e100),
        ),
        // The subnpts shorthand against an explicit count.
        ("subnpts shorthand", reference().subnpts(0)),
        ("subnpts explicit", reference().subnpts(DEFAULT_NPTS)),
        // Scan counts, including the format's "not recorded" zero.
        ("subscan zero", reference().subscan(0)),
        ("subscan many", reference().subscan(1024)),
        // Log block shapes. The block is the only part whose size is computed
        // rather than fixed, so every combination is worth one case.
        ("no log block", reference().no_log()),
        ("log text only", reference().log_text("k=v")),
        ("log empty text", reference().log_text("")),
        (
            "log binary only",
            RawSpc::new().no_log().log_binary(vec![0xDE, 0xAD]),
        ),
        (
            "log text and binary",
            reference().log_binary(vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01]),
        ),
        (
            "log binary odd length",
            reference().log_binary(vec![0x01, 0x02, 0x03]),
        ),
        (
            "log long text",
            reference().log_text("key=value\n".repeat(200).trim_end()),
        ),
        // How instruments store the block: reserved in whole allocation
        // units, the remainder padded with nulls, `logsizm` describing the
        // reservation rather than the text.
        (
            "log padded to 4096",
            reference().log_sizm(4096).log_pad_to(4096),
        ),
        (
            "log padded, odd total",
            reference().log_sizm(999).log_pad_to(999),
        ),
        (
            "log padded by one byte",
            reference().log_sizm(97).log_pad_to(97),
        ),
        (
            "log padded with binary area",
            reference()
                .log_binary(vec![0xDE, 0xAD, 0xBE, 0xEF])
                .log_sizm(512)
                .log_pad_to(512),
        ),
        // logsizm larger than the block is: nothing to pad, but the field
        // still has to come back as it was.
        ("logsizm without padding", reference().log_sizm(4096)),
        ("logdsks not zero", reference().log_dsks(0x2A)),
        (
            "logsizm and logdsks together",
            reference().log_sizm(4096).log_dsks(7).log_pad_to(4096),
        ),
        // Multifile: the geometry a wrong flogoff or a subheader written from
        // the wrong base survives with one subfile and cannot survive here.
        ("two subfiles", reference().spectra(2)),
        ("many subfiles", reference().spectra(9)),
        (
            "multifile without a log block",
            reference().spectra(4).no_log(),
        ),
        (
            "multifile, short spectra",
            reference().y(ramp(3)).spectra(5).range(900.0, 1700.0),
        ),
        (
            "multifile with a padded log block",
            reference().spectra(3).log_sizm(4096).log_pad_to(4096),
        ),
        // TMULTI on a single subfile: a contradiction the format allows, and
        // the flag is written back as found rather than corrected.
        (
            "multifile flag without multiple subfiles",
            reference().ftflgs(TFlags::TMULTI),
        ),
        // Custom axis labels: the 30 byte field, filled to different depths.
        (
            "one custom label",
            reference().custom_axis_labels(&["Wellenlaenge"]),
        ),
        (
            "two custom labels",
            reference().custom_axis_labels(&["Wellenlaenge", "Extinktion"]),
        ),
        (
            "three custom labels",
            reference().custom_axis_labels(&["nm", "AU", "s"]),
        ),
        // Text fields at their limits. `fsource` is nine bytes wide, and a value
        // that fills it leaves no terminator — the case that once made a
        // readable file unwritable.
        ("empty source", reference().source("")),
        ("source fills field", reference().source("123456789")),
        ("comment fills field", reference().comment(&"x".repeat(130))),
        // Dates, including the zero word that means "no date recorded".
        ("no date", reference().fdate(0)),
        ("leap day", reference().date(2024, 2, 29, 8, 5)),
        ("end of day", reference().date(2000, 12, 31, 23, 59)),
        // Unit codes this crate has no name for must survive as raw bytes.
        ("unknown unit codes", reference().axis_types(200, 201)),
        ("unknown technique", reference().fexper(99)),
        // y values that stress the float encoding.
        (
            "extreme y values",
            reference().y(vec![0.0, -0.0, f32::MIN, f32::MAX, f32::MIN_POSITIVE, -1.5]),
        ),
        (
            "infinities",
            reference().y(vec![f32::INFINITY, f32::NEG_INFINITY, 1.0]),
        ),
    ]
}

/// Every shape in the matrix, written back byte for byte.
///
/// The reference is `tests/common/mod.rs`, which assembles the bytes by hand
/// and knows nothing about the writer.
#[test]
fn every_shape_is_written_back_byte_for_byte() {
    for (name, builder) in byte_comparison_cases() {
        let raw = builder.build();
        let spc = Spc::from_bytes(&raw)
            .unwrap_or_else(|e| panic!("case {name:?}: the fixture must parse: {e}"));
        let written = spc
            .to_bytes()
            .unwrap_or_else(|e| panic!("case {name:?}: must be writable: {e}"));

        assert_eq!(
            written.len(),
            raw.len(),
            "case {name:?}: file length changed"
        );
        if let Some(i) = written.iter().zip(&raw).position(|(a, b)| a != b) {
            panic!(
                "case {name:?}: byte {i} differs: wrote {:#04X}, expected {:#04X}",
                written[i], raw[i]
            );
        }
    }
}

/// The same shapes, but checking the values rather than the bytes.
///
/// Identical bytes already imply this. It is kept separate because a failure
/// here says *which field* was lost, while a byte comparison only says where.
#[test]
fn every_shape_keeps_its_values_through_a_round_trip() {
    for (name, builder) in byte_comparison_cases() {
        let before = parse(&builder.build());
        let after = parse(&before.to_bytes().unwrap());

        assert_eq!(
            after.subfiles[0].y.len(),
            before.subfiles[0].y.len(),
            "case {name:?}: y length"
        );
        for (i, (a, b)) in before.subfiles[0]
            .y
            .iter()
            .zip(&after.subfiles[0].y)
            .enumerate()
        {
            assert!(
                a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
                "case {name:?}: y[{i}] changed from {a} to {b}"
            );
        }
        for (i, (a, b)) in before.subfiles[0]
            .x
            .iter()
            .zip(&after.subfiles[0].x)
            .enumerate()
        {
            assert_eq!(a.to_bits(), b.to_bits(), "case {name:?}: x[{i}] changed");
        }
        assert_eq!(after.header.fnpts, before.header.fnpts, "case {name:?}");
        assert_eq!(after.header.fdate, before.header.fdate, "case {name:?}");
        assert_eq!(after.header.fcmnt, before.header.fcmnt, "case {name:?}");
        assert_eq!(after.header.fsource, before.header.fsource, "case {name:?}");
        assert_eq!(after.header.fcatxt, before.header.fcatxt, "case {name:?}");
        assert_eq!(
            after.subfiles[0].subheader.subscan, before.subfiles[0].subheader.subscan,
            "case {name:?}"
        );
        assert_eq!(
            after.log.as_ref().map(|l| (&l.text, &l.binary)),
            before.log.as_ref().map(|l| (&l.text, &l.binary)),
            "case {name:?}: log block"
        );
    }
}

#[test]
fn a_file_without_a_log_block_is_also_reproduced_exactly() {
    let raw = RawSpc::new().no_log().build();
    let written = parse(&raw).to_bytes().unwrap();
    assert_eq!(written, raw);
    assert_eq!(parse(&written).header.flogoff, 0);
}

#[test]
fn a_binary_log_area_is_reproduced_exactly() {
    let raw = reference()
        .log_binary(vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01])
        .build();
    assert_eq!(parse(&raw).to_bytes().unwrap(), raw);
}

#[test]
fn every_parsed_header_field_survives_the_round_trip() {
    let before = parse(&reference().build());
    let after = parse(&before.to_bytes().unwrap());
    let (a, b) = (&before.header, &after.header);

    assert_eq!(a.ftflgs, b.ftflgs);
    assert_eq!(a.fversn, b.fversn);
    assert_eq!(a.fexper, b.fexper);
    assert_eq!(a.fexp, b.fexp);
    assert_eq!(a.fnpts, b.fnpts);
    assert_eq!(a.ffirst, b.ffirst);
    assert_eq!(a.flast, b.flast);
    assert_eq!(a.fnsub, b.fnsub);
    assert_eq!(a.fxtype, b.fxtype);
    assert_eq!(a.fytype, b.fytype);
    assert_eq!(a.fztype, b.fztype);
    assert_eq!(a.fdate, b.fdate);
    assert_eq!(a.date, b.date);
    assert_eq!(a.fres, b.fres);
    assert_eq!(a.fsource, b.fsource);
    assert_eq!(a.fcmnt, b.fcmnt);
    assert_eq!(a.fcatxt, b.fcatxt);
    assert_eq!(a.flogoff, b.flogoff);
    assert_eq!(a.ffactor, b.ffactor);
    assert_eq!(a.fmethod, b.fmethod);
    assert_eq!(a.fwplanes, b.fwplanes);
}

#[test]
fn the_subheader_survives_the_round_trip() {
    let before = parse(&reference().subnpts(DEFAULT_NPTS).build());
    let after = parse(&before.to_bytes().unwrap());
    let (a, b) = (&before.subfiles[0].subheader, &after.subfiles[0].subheader);

    assert_eq!(a.subflgs, b.subflgs);
    assert_eq!(a.subexp, b.subexp);
    assert_eq!(a.subindx, b.subindx);
    assert_eq!(a.subnpts, b.subnpts);
    assert_eq!(a.subscan, b.subscan);
}

#[test]
fn the_inherit_from_fnpts_shorthand_is_kept_rather_than_expanded() {
    let before = parse(&reference().subnpts(0).build());
    let after = parse(&before.to_bytes().unwrap());

    assert_eq!(
        after.subfiles[0].subheader.subnpts, 0,
        "subnpts = 0 must stay the shorthand it was"
    );
    assert_eq!(after.subfiles[0].y.len(), DEFAULT_NPTS as usize);
}

#[test]
fn y_values_survive_bit_for_bit() {
    let values: Vec<f32> = vec![0.0, -0.0, -1.5, 1e-30, 3.402_823_5e38, 0.123_456_79];
    let before = parse(&reference().y(values.clone()).build());
    let after = parse(&before.to_bytes().unwrap());

    assert_eq!(after.subfiles[0].y.len(), values.len());
    for (got, want) in after.subfiles[0].y.iter().zip(&values) {
        assert_eq!(got.to_bits(), f64::from(*want).to_bits(), "changed: {want}");
    }
}

#[test]
fn non_finite_y_values_come_back_unchanged() {
    // The reader accepts these, so the writer has to keep them: refusing here
    // would mean a file this crate reads is one it cannot write.
    let raw = reference()
        .y(vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0])
        .build();
    let after = parse(&parse(&raw).to_bytes().unwrap());

    assert!(after.subfiles[0].y[0].is_nan());
    assert_eq!(after.subfiles[0].y[1], f64::INFINITY);
    assert_eq!(after.subfiles[0].y[2], f64::NEG_INFINITY);
    assert_eq!(after.subfiles[0].y[3], 1.0);
}

#[test]
fn the_log_block_survives_the_round_trip() {
    let before = parse(&reference().build());
    let after = parse(&before.to_bytes().unwrap());

    let log = after.log.expect("the log block must survive");
    assert_eq!(log.get("Channel"), Some("1"));
    assert_eq!(log.get("Integration"), Some("100ms"));
    assert_eq!(log.text, before.log.unwrap().text);
}

#[test]
fn writing_is_idempotent_even_when_the_text_needed_normalising() {
    // The fixture's log text ends in a newline, which the reader trims. The
    // first write therefore loses that byte; every write after it must not
    // change anything further.
    let once = parse(&RawSpc::new().build()).to_bytes().unwrap();
    let twice = parse(&once).to_bytes().unwrap();
    assert_eq!(once, twice, "writing must reach a fixed point immediately");

    let entries: Vec<_> = parse(&twice)
        .log
        .unwrap()
        .entries()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    assert_eq!(entries.len(), 2, "no entry may be lost to normalisation");
}

#[test]
fn custom_axis_labels_survive_the_round_trip() {
    let before = parse(
        &reference()
            .custom_axis_labels(&["Wellenlaenge", "Extinktion"])
            .build(),
    );
    let after = parse(&before.to_bytes().unwrap());

    assert_eq!(after.x_label(), "Wellenlaenge");
    assert_eq!(after.y_label(), "Extinktion");
}

#[test]
fn the_date_survives_the_round_trip() {
    let before = parse(&reference().date(2026, 2, 29, 8, 5).build());
    let after = parse(&before.to_bytes().unwrap());
    assert_eq!(
        after.header.date,
        Some(SpcDate {
            year: 2026,
            month: 2,
            day: 29,
            hour: 8,
            minute: 5
        })
    );
}

#[test]
fn unusual_axis_shapes_are_written_correctly() {
    // One point (no interval to divide by) and a descending axis (flast below
    // ffirst) are the two shapes where generated-x arithmetic tends to break.
    let single = parse(&reference().y(vec![0.42]).range(1000.0, 1000.0).build());
    let single = parse(&single.to_bytes().unwrap());
    assert_eq!(single.subfiles[0].x, &[1000.0]);

    let descending = parse(
        &reference()
            .y(vec![1.0, 2.0, 3.0, 4.0])
            .range(4000.0, 400.0)
            .build(),
    );
    let descending = parse(&descending.to_bytes().unwrap());
    assert_eq!(descending.subfiles[0].x, &[4000.0, 2800.0, 1600.0, 400.0]);
}

#[test]
fn writes_a_file_that_from_path_reads_back() {
    let path = std::env::temp_dir().join("spc-spectra-to-path-test.spc");
    let before = parse(&reference().build());

    before.to_path(&path).expect("to_path must write the file");
    let after = Spc::from_path(&path).expect("from_path must read what to_path wrote");
    std::fs::remove_file(&path).ok();

    assert_eq!(after.subfiles[0].y, before.subfiles[0].y);
    assert_eq!(after.subfiles[0].x[0], DEFAULT_FIRST);
    assert_eq!(
        after.subfiles[0].x[after.subfiles[0].x.len() - 1],
        DEFAULT_LAST
    );
    assert_eq!(after.header.fexper, Technique::Nir);
    assert_eq!(after.header.fxtype, XType::Nanometers);
    assert_eq!(after.header.fytype, YType::Absorbance);
}
