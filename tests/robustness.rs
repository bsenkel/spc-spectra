//! The parser must never panic, whatever bytes it is handed.
//!
//! This crate reads files produced by other people's software, so hostile or
//! merely corrupt input is the normal case, not an edge case. Every accessor
//! goes through the bounds-checked cursor in `src/bytes.rs`; these tests are
//! what keeps that true as the code grows.
//!
//! Any outcome is acceptable here — `Ok`, or any `Err` — except a panic, an
//! abort, or a hang. The assertions are deliberately about *not crashing*
//! rather than about specific errors; the exact diagnosis is `unsupported.rs`'s
//! job.

mod common;

use common::SpcBuilder;
use spc_spectra::{Header, Spc, SpcError};

/// xorshift64. Deterministic on purpose: a fuzz failure you cannot reproduce
/// is a rumour, not a bug report. No `rand` dependency either — this crate has
/// none and the test suite should not smuggle one in.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        Self(0x2545_F491_4F6C_DD1D)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Corrupts a valid file in a few random places, then truncates it randomly.
#[test]
fn random_mutations_never_panic() {
    let base = SpcBuilder::new().build();
    let mut rng = Rng::new();
    let mut parsed = 0usize;

    for _ in 0..100_000 {
        let mut data = base.clone();
        for _ in 0..1 + rng.below(6) {
            let i = rng.below(data.len());
            data[i] = rng.next() as u8;
        }
        let len = data.len() - rng.below(data.len());
        if Spc::from_bytes(&data[..len]).is_ok() {
            parsed += 1;
        }
    }

    // Not a correctness assertion, a sanity check on the test itself: if
    // nothing at all still parsed, the mutations would be so destructive that
    // the success path never gets exercised.
    assert!(
        parsed > 0,
        "every single mutation was rejected — is the corruption too aggressive?"
    );
}

/// Every single-bit flip in the 512 byte header, exhaustively.
///
/// Cheaper and stricter than random sampling for the part that matters most:
/// the header decides how everything after it is interpreted, so this is where
/// a missing bounds check turns into a panic.
#[test]
fn every_single_bit_flip_in_the_header_is_handled() {
    let base = SpcBuilder::new().build();

    for byte in 0..Header::SIZE {
        for bit in 0..8 {
            let mut data = base.clone();
            data[byte] ^= 1 << bit;
            // Must not panic. Success or failure are both fine.
            let _ = Spc::from_bytes(&data);
        }
    }
}

/// The same, for the subheader that follows it.
#[test]
fn every_single_bit_flip_in_the_subheader_is_handled() {
    let base = SpcBuilder::new().build();

    for byte in Header::SIZE..Header::SIZE + spc_spectra::SubHeader::SIZE {
        for bit in 0..8 {
            let mut data = base.clone();
            data[byte] ^= 1 << bit;
            let _ = Spc::from_bytes(&data);
        }
    }
}

/// Pure noise of every plausible length, including the empty slice.
#[test]
fn arbitrary_garbage_never_panics() {
    let mut rng = Rng::new();

    for len in 0..600 {
        let data: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        let _ = Spc::from_bytes(&data);
    }

    // A valid-looking header in front of nothing at all is the nastiest shape:
    // it gets past the version check and then asks for data that is not there.
    for len in 0..64 {
        let mut data = vec![0u8; Header::SIZE];
        data[1] = 0x4B; // fversn
        data[3] = 0x80; // fexp, IEEE floats
        data[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // absurd fnpts
        data.extend(std::iter::repeat_n(0xAB, len));
        let _ = Spc::from_bytes(&data);
    }
}

/// Byte at which the default file's y values end: 512 + 32 + 801 * 4.
const DATA_END: u32 = 3748;

/// A log block that cannot be read must not take the spectrum down with it.
///
/// The log block is optional vendor data sitting after the measurements, so an
/// offset that merely points past the end of the file says nothing about
/// whether the y values are sound.
#[test]
fn an_unreachable_log_offset_still_yields_the_spectrum() {
    let base = SpcBuilder::new().no_log().build();

    for offset in [0u32, DATA_END, DATA_END + 1, u32::MAX / 2, u32::MAX] {
        let mut data = base.clone();
        data[248..252].copy_from_slice(&offset.to_le_bytes());

        let spc = Spc::from_bytes(&data)
            .unwrap_or_else(|e| panic!("flogoff {offset} should have been survivable: {e}"));
        assert_eq!(spc.y().len(), common::DEFAULT_NPTS as usize);
    }
}

/// Header fields that are self-contradictory rather than merely truncated.
///
/// Each of these once passed silently: a zero subfile count still produced
/// points, and a non-finite endpoint filled the x axis with NaN. They must now
/// be named, not waved through.
#[test]
fn self_contradictory_header_fields_are_named() {
    // fnsub = 0 while a subfile plainly follows.
    let mut data = SpcBuilder::new().build();
    data[24..28].copy_from_slice(&0u32.to_le_bytes());
    assert!(
        matches!(
            Spc::from_bytes(&data),
            Err(SpcError::MalformedHeader { .. })
        ),
        "fnsub = 0 should be rejected"
    );

    // Non-finite x endpoints. ffirst at bytes 8..16, flast at 16..24.
    for (offset, bytes) in [
        (8usize, f64::NAN),
        (8, f64::INFINITY),
        (16, f64::NEG_INFINITY),
        (16, f64::NAN),
    ] {
        let mut data = SpcBuilder::new().build();
        data[offset..offset + 8].copy_from_slice(&bytes.to_le_bytes());
        let err = Spc::from_bytes(&data);
        assert!(
            matches!(err, Err(SpcError::MalformedHeader { .. })),
            "non-finite endpoint at byte {offset} should be rejected, got {err:?}"
        );
    }

    // A finite spectrum still reads, so the guard is not over-eager.
    assert!(Spc::from_bytes(&SpcBuilder::new().build()).is_ok());
}

/// A log offset that lands *inside* the measurements is a different matter.
///
/// Then the header contradicts itself, and there is no way to tell whether the
/// point count or the offset is the wrong one — so the y values cannot be
/// trusted and are refused rather than served up.
#[test]
fn a_log_offset_inside_the_data_is_refused() {
    let base = SpcBuilder::new().no_log().build();

    for offset in [1u32, 511, 512, 545, DATA_END - 1] {
        let mut data = base.clone();
        data[248..252].copy_from_slice(&offset.to_le_bytes());

        match Spc::from_bytes(&data) {
            Err(SpcError::DataOverrunsLogBlock {
                data_end,
                log_offset,
            }) => {
                assert_eq!(data_end, DATA_END as usize);
                assert_eq!(log_offset, offset);
            }
            other => panic!("flogoff {offset} contradicts the data, but got {other:?}"),
        }
    }
}
