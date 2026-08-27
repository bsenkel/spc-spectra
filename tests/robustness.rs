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
//!
//! The two properties with teeth are at the bottom: whatever the reader
//! accepts, the writer must be able to write back — and whatever the builder
//! can express must survive a round trip. The second one reaches inputs the
//! first cannot, because it does not start from a file at all.

mod common;

use common::{Rng, SpcBuilder as RawSpc};
use spc_spectra::{Header, LogBlock, Spc, SpcBuilder, SpcDate, SpcError};

/// Corrupts a valid file in a few random places, then truncates it randomly.
#[test]
fn random_mutations_never_panic() {
    let base = RawSpc::new().build();
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
    let base = RawSpc::new().build();

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
    let base = RawSpc::new().build();

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
    let base = RawSpc::new().no_log().build();

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
    let mut data = RawSpc::new().build();
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
        let mut data = RawSpc::new().build();
        data[offset..offset + 8].copy_from_slice(&bytes.to_le_bytes());
        let err = Spc::from_bytes(&data);
        assert!(
            matches!(err, Err(SpcError::MalformedHeader { .. })),
            "non-finite endpoint at byte {offset} should be rejected, got {err:?}"
        );
    }

    // A finite spectrum still reads, so the guard is not over-eager.
    assert!(Spc::from_bytes(&RawSpc::new().build()).is_ok());
}

/// A log offset that lands *inside* the measurements is a different matter.
///
/// Then the header contradicts itself, and there is no way to tell whether the
/// point count or the offset is the wrong one — so the y values cannot be
/// trusted and are refused rather than served up.
#[test]
fn a_log_offset_inside_the_data_is_refused() {
    let base = RawSpc::new().no_log().build();

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

/// Everything the reader accepts, the writer must be able to write back.
///
/// This is the invariant the whole design rests on — "readable" and "writable"
/// are meant to be the same set — and a corrupted file is the sharpest way to
/// test it, because it reaches header combinations no instrument would produce.
/// The round trip has to survive them all, not just the tidy ones.
#[test]
fn whatever_parses_can_be_written_back_and_parsed_again() {
    let base = RawSpc::new().build();
    let mut rng = Rng::new();
    let mut round_tripped = 0usize;

    for _ in 0..20_000 {
        let mut data = base.clone();
        for _ in 0..1 + rng.below(6) {
            let i = rng.below(data.len());
            data[i] = rng.next() as u8;
        }
        let len = data.len() - rng.below(data.len());

        let Ok(spc) = Spc::from_bytes(&data[..len]) else {
            continue;
        };
        let bytes = match spc.to_bytes() {
            Ok(bytes) => bytes,
            // The one documented exception: text that was not valid UTF-8 is
            // decoded lossily, and each replaced byte becomes a three byte
            // U+FFFD, which can push the field past the slot it came from.
            Err(SpcError::FieldTooLong { .. }) => continue,
            Err(e) => panic!("a file that parsed could not be written back: {e}"),
        };

        let again = Spc::from_bytes(&bytes).expect("what the writer produced must parse");
        assert_same(spc.y(), again.y(), "y values");
        assert_same(spc.x(), again.x(), "x values");
        assert_eq!(again.header.fnpts, spc.header.fnpts);
        assert_eq!(again.header.ffirst, spc.header.ffirst);
        assert_eq!(again.header.fcmnt, spc.header.fcmnt);
        assert_log_survives(spc.log.as_ref(), again.log.as_ref());
        round_tripped += 1;
    }

    assert!(
        round_tripped > 0,
        "no mutated file survived to be written — is the corruption too aggressive?"
    );
}

/// Compares two log blocks field by field.
///
/// Three of the block's five numbers are recomputed on write, being offsets
/// into the block being built. The other two are what the acquiring software
/// recorded, and have to come back unchanged like any other parsed field.
#[track_caller]
fn assert_log_survives(before: Option<&LogBlock>, after: Option<&LogBlock>) {
    let (before, after) = match (before, after) {
        (None, None) => return,
        (a, b) => (
            a.expect("a log block appeared out of nowhere"),
            b.expect("the log block disappeared"),
        ),
    };
    assert_eq!(before.logsizm, after.logsizm, "logsizm changed");
    assert_eq!(before.logdsks, after.logdsks, "logdsks changed");
    assert_eq!(before.text, after.text, "log text changed");
    assert_eq!(before.binary, after.binary, "log binary area changed");
}

/// Compares two axes bit for bit, counting NaN as equal to NaN.
///
/// Bit equality rather than `==` because a spectrum may legitimately contain a
/// signed zero, and those compare equal while being different numbers. NaN is
/// the mirror image: identical in meaning, never equal to itself.
#[track_caller]
fn assert_same(before: &[f64], after: &[f64], what: &str) {
    assert_eq!(before.len(), after.len(), "{what}: length changed");
    for (i, (a, b)) in before.iter().zip(after).enumerate() {
        assert!(
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
            "{what}: index {i} changed from {a} to {b}"
        );
    }
}

/// Everything the builder can express must survive a round trip.
///
/// The property above starts from a file, so it only ever hands the writer
/// values a *reader* produced: point counts bounded by the file it came from,
/// text that had already fitted its field, an x axis the reader generated
/// itself. A caller with a measurement in hand is under no such constraint.
///
/// This generates that side instead — magnitudes from `1e-15` to `1e14`, point
/// counts from one to a thousand, text fields up to and including their exact
/// limit, log blocks of every shape — and checks that what comes back out of
/// the bytes is what went in.
#[test]
fn whatever_the_builder_can_express_survives_a_round_trip() {
    let mut rng = Rng::new();

    for i in 0..2_000 {
        let spec = Spectrum::random(&mut rng);
        let case = format!("iteration {i} ({} points)", spec.y.len());

        let built = spec
            .build()
            .unwrap_or_else(|e| panic!("{case}: the builder refused its own input: {e}"));
        let bytes = built
            .to_bytes()
            .unwrap_or_else(|e| panic!("{case}: a built file must be writable: {e}"));
        let read = Spc::from_bytes(&bytes)
            .unwrap_or_else(|e| panic!("{case}: what was written must parse: {e}"));

        spec.assert_survived(&read, &case);

        // And once more around, which must not move a single byte.
        assert_eq!(
            read.to_bytes().unwrap(),
            bytes,
            "{case}: writing is not a fixed point"
        );
    }
}

/// One randomly generated spectrum, kept so the result can be compared to it.
struct Spectrum {
    first: f64,
    last: f64,
    y: Vec<f64>,
    source: String,
    comment: String,
    scans: u32,
    date: Option<SpcDate>,
    log_text: Option<String>,
    log_binary: Vec<u8>,
}

impl Spectrum {
    fn random(rng: &mut Rng) -> Self {
        let npts = 1 + rng.below(1_000);
        let (first, last) = (coordinate(rng), coordinate(rng));

        Self {
            first,
            last,
            y: (0..npts).map(|_| y_value(rng)).collect(),
            // Nine bytes is the whole field, so the longest value here has no
            // room for a terminating null — deliberately included.
            source: text(rng, 9),
            comment: text(rng, 130),
            scans: match rng.below(3) {
                0 => 0, // the format's "not recorded"
                1 => 1,
                _ => rng.next() as u32,
            },
            date: rng.one_in(4).then(|| random_date(rng)),
            log_text: (!rng.one_in(3)).then(|| text(rng, 300)),
            log_binary: (0..rng.below(64)).map(|_| rng.next() as u8).collect(),
        }
    }

    fn build(&self) -> Result<Spc, SpcError> {
        let mut b = SpcBuilder::new(self.first, self.last, self.y.clone())
            .source(self.source.clone())
            .comment(self.comment.clone())
            .scans(self.scans)
            .log_binary(self.log_binary.clone());
        if let Some(date) = self.date {
            b = b.date(date);
        }
        if let Some(text) = &self.log_text {
            b = b.log_text(text.clone());
        }
        b.build()
    }

    #[track_caller]
    fn assert_survived(&self, read: &Spc, case: &str) {
        assert_eq!(read.y().len(), self.y.len(), "{case}: point count");
        assert_eq!(read.header.ffirst, self.first, "{case}: ffirst");
        assert_eq!(read.header.flast, self.last, "{case}: flast");
        assert_eq!(read.header.fsource, self.source, "{case}: fsource");
        assert_eq!(read.header.fcmnt, self.comment, "{case}: fcmnt");
        assert_eq!(
            read.subfiles[0].subheader.subscan, self.scans,
            "{case}: subscan"
        );
        assert_eq!(read.header.date, self.date, "{case}: date");

        // y values pass through a 32 bit float, so what comes back is the
        // narrowed value — not the f64 that went in. Anything else would mean
        // the writer changed a number it was only supposed to store.
        for (i, (got, want)) in read.y().iter().zip(&self.y).enumerate() {
            assert_eq!(
                got.to_bits(),
                f64::from(*want as f32).to_bits(),
                "{case}: y[{i}] is {got}, expected {want} narrowed to f32"
            );
        }

        match (&read.log, &self.log_text, self.log_binary.is_empty()) {
            (None, None, true) => {}
            (Some(log), text, _) => {
                assert_eq!(log.binary, self.log_binary, "{case}: log binary");
                assert_eq!(
                    log.text,
                    text.as_deref().unwrap_or("").trim(),
                    "{case}: log text"
                );
            }
            (None, text, empty) => {
                panic!("{case}: log block lost (text {text:?}, binary empty: {empty})")
            }
        }
    }
}

/// An x axis end point: finite, signed, across many orders of magnitude.
fn coordinate(rng: &mut Rng) -> f64 {
    let magnitude = 10f64.powi(rng.below(20) as i32 - 6);
    let sign = if rng.one_in(4) { -1.0 } else { 1.0 };
    sign * magnitude * rng.fraction()
}

/// A y value, always inside the f32 range so narrowing costs precision but
/// never turns the number into an infinity.
fn y_value(rng: &mut Rng) -> f64 {
    let magnitude = 10f64.powi(rng.below(30) as i32 - 15);
    let sign = if rng.one_in(3) { -1.0 } else { 1.0 };
    sign * magnitude * rng.fraction()
}

/// Text for a fixed-width field: no NUL, and no leading or trailing space,
/// since the reader trims those and the comparison would be about trimming
/// rather than about writing.
fn text(rng: &mut Rng, max: usize) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.=";
    let n = rng.below(max + 1);
    (0..n)
        .map(|_| CHARS[rng.below(CHARS.len())] as char)
        .collect()
}

fn random_date(rng: &mut Rng) -> SpcDate {
    SpcDate {
        year: 1900 + rng.below(200) as u16,
        month: 1 + rng.below(12) as u8,
        day: 1 + rng.below(31) as u8,
        hour: rng.below(24) as u8,
        minute: rng.below(60) as u8,
    }
}
