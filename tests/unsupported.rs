//! Every format variant this version cannot decode must be refused with a
//! specific reason. These tests are the guarantee that the crate never returns
//! a plausible-looking but wrong spectrum.

mod common;

use common::SpcBuilder;
use spc_spectra::{Spc, SpcError, TFlags, Unsupported};

/// Asserts that the built file is rejected for exactly the expected reason.
fn assert_rejected(builder: SpcBuilder, expected: Unsupported) {
    match Spc::from_bytes(&builder.build()) {
        Err(SpcError::Unsupported(got)) => assert_eq!(got, expected),
        Err(other) => panic!("expected {expected:?}, got {other:?}"),
        Ok(_) => panic!("expected {expected:?}, but the file parsed successfully"),
    }
}

#[test]
fn big_endian_files_are_refused() {
    assert_rejected(SpcBuilder::new().fversn(0x4C), Unsupported::BigEndian);
}

#[test]
fn the_old_format_is_refused() {
    assert_rejected(SpcBuilder::new().fversn(0x4D), Unsupported::OldFormat);
}

#[test]
fn multifile_records_are_refused_via_the_flag() {
    assert_rejected(
        SpcBuilder::new().ftflgs(TFlags::TMULTI),
        Unsupported::MultiFile,
    );
}

#[test]
fn multifile_records_are_refused_via_fnsub() {
    // Some writers set fnsub without setting TMULTI; both must be caught.
    assert_rejected(SpcBuilder::new().fnsub(3), Unsupported::MultiFile);
}

#[test]
fn per_subfile_x_axes_are_refused() {
    // A TXYXYS file also carries TXVALS and TMULTI; the most specific
    // diagnosis has to win, otherwise the message misleads.
    let flags = TFlags::TXYXYS | TFlags::TXVALS | TFlags::TMULTI;
    assert_rejected(SpcBuilder::new().ftflgs(flags), Unsupported::XyxySubfiles);
}

#[test]
fn explicit_x_values_are_refused() {
    assert_rejected(
        SpcBuilder::new().ftflgs(TFlags::TXVALS),
        Unsupported::ExplicitXValues,
    );
}

#[test]
fn sixteen_bit_y_values_are_refused() {
    assert_rejected(
        SpcBuilder::new().ftflgs(TFlags::TSPREC),
        Unsupported::SixteenBitY,
    );
}

#[test]
fn multi_plane_data_cubes_are_refused() {
    // fwplanes > 1 changes the data layout; 0 and 1 are both the ordinary case.
    assert_rejected(
        SpcBuilder::new().fwplanes(5),
        Unsupported::WPlanes { fwplanes: 5 },
    );
    for planes in [0u32, 1] {
        assert!(
            Spc::from_bytes(&SpcBuilder::new().fwplanes(planes).build()).is_ok(),
            "fwplanes {planes} is the normal single-plane case"
        );
    }
}

#[test]
fn fixed_point_y_values_are_refused_and_report_the_exponent() {
    for fexp in [-3i8, 0, 12, 127] {
        assert_rejected(
            SpcBuilder::new().fexp(fexp),
            Unsupported::FixedPointY { fexp },
        );
    }
}

/// The subheader carries its own exponent. If nobody cross-checks it against
/// the header's `fexp`, a file that says "floats" up front and "fixed-point,
/// exponent -3" in the subheader parses into a perfectly plausible, perfectly
/// wrong spectrum. That is precisely the failure mode this crate exists to
/// avoid, so it gets its own set of tests.
#[test]
fn a_subfile_exponent_contradicting_the_header_is_refused() {
    for subexp in [-3i8, 12, 127, -1] {
        assert_rejected(
            SpcBuilder::new().subexp(subexp),
            Unsupported::FixedPointSubfileY { subexp, fexp: -128 },
        );
    }
}

#[test]
fn a_zero_subfile_exponent_is_refused_rather_than_assumed_unset() {
    // 0 is a legal fixed-point exponent, not an obvious "not filled in".
    assert_rejected(
        SpcBuilder::new().subexp(0),
        Unsupported::FixedPointSubfileY {
            subexp: 0,
            fexp: -128,
        },
    );
}

#[test]
fn a_subfile_exponent_agreeing_with_the_header_reads_normally() {
    // 0x80 in both places is the ordinary IEEE-float case.
    let spc = Spc::from_bytes(&SpcBuilder::new().subexp(-128).build())
        .expect("matching exponents must not be rejected");
    assert_eq!(spc.y().len(), common::DEFAULT_NPTS as usize);
    assert_eq!(spc.subfiles[0].subheader.subexp, -128);
}

#[test]
fn an_unknown_version_byte_is_not_an_spc_file() {
    match Spc::from_bytes(&SpcBuilder::new().fversn(0x99).build()) {
        Err(SpcError::BadVersion(0x99)) => {}
        other => panic!("expected BadVersion(0x99), got {other:?}"),
    }
}

#[test]
fn harmless_flags_do_not_trigger_a_rejection() {
    // These bits describe z spacing and label handling, not the byte layout of
    // the data, so they must not block reading.
    let flags = TFlags::TRANDM | TFlags::TORDRD | TFlags::TALABS | TFlags::TCGRAM;
    let spc = Spc::from_bytes(&SpcBuilder::new().ftflgs(flags).build())
        .expect("layout-neutral flags must not be rejected");
    assert_eq!(spc.y().len(), common::DEFAULT_NPTS as usize);
}

#[test]
fn every_rejection_explains_itself() {
    let cases = [
        SpcBuilder::new().fversn(0x4C),
        SpcBuilder::new().fversn(0x4D),
        SpcBuilder::new().ftflgs(TFlags::TMULTI),
        SpcBuilder::new().ftflgs(TFlags::TXYXYS),
        SpcBuilder::new().ftflgs(TFlags::TXVALS),
        SpcBuilder::new().ftflgs(TFlags::TSPREC),
        SpcBuilder::new().fexp(4),
        SpcBuilder::new().subexp(4),
        SpcBuilder::new().fwplanes(2),
    ];
    for builder in cases {
        let err = Spc::from_bytes(&builder.build()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.starts_with("not supported yet: ") && msg.len() > 25,
            "unhelpful message: {msg}"
        );
    }
}
