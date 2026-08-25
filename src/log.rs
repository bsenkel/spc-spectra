//! The optional log block at the end of an SPC file.
//!
//! The log block holds whatever the acquiring software wanted to record. Its
//! contents are vendor-specific: some instruments write plain `key=value`
//! lines, others write a binary blob. This crate therefore passes both parts
//! through untouched instead of imposing a schema on them.

use crate::bytes::{Cursor, decode_log_text};
use crate::error::SpcError;

/// The log block, split into its raw binary and text areas.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LogBlock {
    /// Size of the block on disk.
    pub logsizd: u32,
    /// Size of the block in memory.
    pub logsizm: u32,
    /// Offset of the text area, relative to the start of the block.
    pub logtxto: u32,
    /// Size of the binary area.
    pub logbins: u32,
    /// Size of the vendor-reserved disk area.
    pub logdsks: u32,
    /// The binary area, passed through as-is.
    pub binary: Vec<u8>,
    /// The text area, decoded lossily and passed through as-is.
    pub text: String,
}

impl LogBlock {
    /// Size of the log block's own header in bytes.
    pub const HEADER_SIZE: usize = 64;

    /// Reads the log block starting at `offset`.
    ///
    /// Returns `Ok(None)` when the offset or the recorded sizes do not fit the
    /// file. A damaged log block says nothing about the validity of the
    /// spectrum itself, so it is dropped rather than turned into an error.
    pub(crate) fn parse_at(c: &mut Cursor<'_>, offset: u32) -> Result<Option<Self>, SpcError> {
        const CTX: &str = "the log block";
        let offset = offset as usize;
        if offset == 0 || offset >= c.len() {
            return Ok(None);
        }
        if c.seek(offset, CTX).is_err() || c.remaining() < Self::HEADER_SIZE {
            return Ok(None);
        }

        let start = c.pos();
        let logsizd = c.u32(CTX)?;
        let logsizm = c.u32(CTX)?;
        let logtxto = c.u32(CTX)?;
        let logbins = c.u32(CTX)?;
        let logdsks = c.u32(CTX)?;
        c.skip(Self::HEADER_SIZE - (c.pos() - start), CTX)?;

        // The binary area sits between the block header and the text area.
        let binary = match c.bytes(logbins as usize, CTX) {
            Ok(raw) => raw.to_vec(),
            // A size field larger than the file means the writer got it wrong;
            // keep the text, drop the unreadable binary part.
            Err(_) => Vec::new(),
        };

        // logtxto is relative to the start of the block. Fall back to whatever
        // follows the header if it points outside the file.
        let text_start = offset.saturating_add(logtxto as usize);
        let text = if logtxto == 0 || text_start >= c.len() {
            String::new()
        } else {
            c.seek(text_start, CTX)?;
            let rest = c.remaining();
            decode_log_text(c.bytes(rest, CTX)?)
        };

        Ok(Some(Self {
            logsizd,
            logsizm,
            logtxto,
            logbins,
            logdsks,
            binary,
            text,
        }))
    }

    /// Best-effort iterator over `key=value` pairs in the text area.
    ///
    /// Many instruments, including the ones this crate was developed against,
    /// write one `key=value` per line. That is a convention, not part of the
    /// format, so treat the result as a hint: lines without an `=` are skipped
    /// and no key is guaranteed to exist. Use [`LogBlock::text`] when you need
    /// the exact bytes.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.text.lines().filter_map(|line| {
            let (k, v) = line.split_once('=')?;
            let k = k.trim();
            if k.is_empty() {
                None
            } else {
                Some((k, v.trim()))
            }
        })
    }

    /// Looks up the first value for `key`, comparing case-insensitively.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }
}
