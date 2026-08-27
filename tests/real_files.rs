//! Optional cross-check against real instrument files.
//!
//! Every other suite builds its own SPC files, so the reader is only checked
//! against layouts this project also wrote — and a reader and writer that share
//! a mistake agree with each other perfectly. Only a file from foreign software
//! settles it.
//!
//! Such files cannot live here: instrument exports carry sample labels,
//! operator names and file paths in `fcmnt`, `fsource` and the log block. So
//! these tests read from a directory you point them at, and pass trivially when
//! you do not:
//!
//! ```text
//! SPC_SAMPLE_DIR=/path/to/spc/files cargo test --test real_files
//! ```
//!
//! Failures name byte offsets, field names and lengths, never field contents. A
//! test log is the last place a sample name should turn up.

use spc_spectra::{Header, Spc, SpcError, SubHeader};
use std::path::PathBuf;

/// The sample files to check, or an empty list if none were pointed at.
fn samples() -> Vec<PathBuf> {
    let Some(dir) = std::env::var_os("SPC_SAMPLE_DIR") else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("SPC_SAMPLE_DIR {dir:?} cannot be read: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("spc")))
        .collect();
    found.sort();
    assert!(
        !found.is_empty(),
        "SPC_SAMPLE_DIR {dir:?} holds no .spc files"
    );
    found
}

/// A short label for a file. Not its name: that holds the sample and the
/// instrument, and this string can end up in a failure log.
fn label(index: usize, len: usize) -> String {
    format!("sample {} ({len} bytes)", index + 1)
}

/// Reading must end in one of two places, never a third: the file parses, or it
/// is refused with a named `Unsupported` variant. Any other error, or a panic,
/// on a file a real instrument wrote is a bug here, not a property of the file.
#[test]
fn every_sample_file_either_parses_or_names_the_feature_it_needs() {
    for (i, path) in samples().into_iter().enumerate() {
        let raw = std::fs::read(&path).expect("sample file must be readable");
        let name = label(i, raw.len());
        match Spc::from_bytes(&raw) {
            Ok(spc) => assert!(!spc.subfiles.is_empty(), "{name}: parsed with no subfiles"),
            Err(SpcError::Unsupported(_)) => {}
            Err(other) => panic!(
                "{name}: a real instrument file must parse or name an unsupported \
                 feature, but reading it gave {other:?}"
            ),
        }
    }
}

/// Every modelled field must come back unchanged through write-then-read — the
/// guarantee the crate states, checked against files it did not write.
/// `logsizd`, `logtxto` and `logbins` are excluded: they are recomputed on
/// purpose.
#[test]
fn every_modelled_field_survives_a_round_trip() {
    for (i, path) in samples().into_iter().enumerate() {
        let raw = std::fs::read(&path).expect("sample file must be readable");
        let Ok(spc) = Spc::from_bytes(&raw) else {
            continue; // Refusals are the other test's business.
        };
        let name = label(i, raw.len());
        let written = spc
            .to_bytes()
            .unwrap_or_else(|e| panic!("{name}: parsed but could not be written: {e}"));
        let again = Spc::from_bytes(&written)
            .unwrap_or_else(|e| panic!("{name}: what the writer produced does not parse: {e}"));

        // Debug output holds every field, including ones no accessor exposes,
        // so comparing it catches fields this test does not know about. Only
        // ever compared, never printed.
        assert!(
            format!("{:?}", spc.header) == format!("{:?}", again.header),
            "{name}: a header field changed"
        );
        assert_eq!(
            spc.subfiles.len(),
            again.subfiles.len(),
            "{name}: subfile count changed"
        );
        for (i, (before, after)) in spc.subfiles.iter().zip(&again.subfiles).enumerate() {
            assert!(
                format!("{:?}", before.subheader) == format!("{:?}", after.subheader),
                "{name}: a subheader field changed in subfile {i}"
            );
            assert_same(&before.y, &after.y, &format!("{name}: subfile {i} y"));
            assert_same(&before.x, &after.x, &format!("{name}: subfile {i} x"));
        }
        match (&spc.log, &again.log) {
            (None, None) => {}
            (Some(before), Some(after)) => {
                assert_eq!(before.logsizm, after.logsizm, "{name}: logsizm changed");
                assert_eq!(before.logdsks, after.logdsks, "{name}: logdsks changed");
                assert_eq!(
                    before.stored_size, after.stored_size,
                    "{name}: the block's stored size changed"
                );
                assert_eq!(
                    before.binary, after.binary,
                    "{name}: the log binary area changed"
                );
                assert!(
                    before.text == after.text,
                    "{name}: the log text changed, {} bytes became {}",
                    before.text.len(),
                    after.text.len()
                );
            }
            _ => panic!("{name}: the log block appeared or vanished"),
        }
    }
}

/// Every byte that differs from the original must fall in a region this crate
/// documents as not reproduced. Byte identity is not promised for a foreign
/// file, so this checks that the set of differences is the *known* set: a
/// difference elsewhere means the reader dropped something it should model.
#[test]
fn every_difference_from_the_original_bytes_is_one_this_crate_documents() {
    for (i, path) in samples().into_iter().enumerate() {
        let raw = std::fs::read(&path).expect("sample file must be readable");
        let Ok(spc) = Spc::from_bytes(&raw) else {
            continue;
        };
        let name = label(i, raw.len());

        // A log block too damaged to read is dropped rather than carried, so
        // the file legitimately comes out shorter. Nothing to compare.
        if spc.header.flogoff != 0 && spc.log.is_none() {
            continue;
        }

        let written = spc.to_bytes().expect("already shown to be writable");
        let log_offset = spc.header.flogoff as usize;

        assert_eq!(
            written.len(),
            raw.len(),
            "{name}: file length changed; a log block's padding should be \
             restored, not dropped"
        );

        let unexplained: Vec<usize> = raw
            .iter()
            .zip(&written)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(at, _)| at)
            .filter(|&at| documented(at, log_offset).is_none())
            .collect();
        assert!(
            unexplained.is_empty(),
            "{name}: {} byte(s) differ outside every documented region, first \
             at offset {} — the reader is dropping something it should model",
            unexplained.len(),
            unexplained.first().copied().unwrap_or(0)
        );
    }
}

/// The regions the README lists as not reproduced byte for byte.
fn documented(offset: usize, log_offset: usize) -> Option<&'static str> {
    const HEADER_TAIL: usize = 325;
    const SUBHEADER_TAIL: usize = 28;

    if (HEADER_TAIL..Header::SIZE).contains(&offset) {
        return Some("the header's reserved tail");
    }
    let sub = Header::SIZE;
    if (sub + SUBHEADER_TAIL..sub + SubHeader::SIZE).contains(&offset) {
        return Some("the subheader's reserved tail");
    }
    if log_offset != 0 && offset >= log_offset {
        // Spelled out rather than left to a catch-all: `logsizm` (4..8) and
        // `logdsks` (16..20) are preserved, so a difference there is a
        // regression and must fall through to None.
        return match offset - log_offset {
            0..=3 => Some("logsizd, recomputed from the bytes written"),
            8..=15 => Some("logtxto and logbins, recomputed"),
            20..=63 => Some("the log block header's reserved tail"),
            4..=7 | 16..=19 => None,
            _ => Some("the log text: trimmed, nulls turned into newlines, lossy UTF-8"),
        };
    }
    None
}

/// Compares two axes bit for bit, counting NaN as equal to NaN.
#[track_caller]
fn assert_same(before: &[f64], after: &[f64], what: &str) {
    assert_eq!(before.len(), after.len(), "{what}: length changed");
    for (i, (a, b)) in before.iter().zip(after).enumerate() {
        assert!(
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
            "{what}: value at index {i} changed"
        );
    }
}
