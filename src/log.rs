//! The optional log block at the end of an SPC file.
//!
//! The log block holds whatever the acquiring software wanted to record. Its
//! contents are vendor-specific: some instruments write plain `key=value`
//! lines, others write a binary blob. This crate therefore passes both parts
//! through untouched instead of imposing a schema on them.

use crate::bytes::{Cursor, decode_log_text};
use crate::error::SpcError;
use crate::write::Sink;

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

    /// A log block holding `text`, and optionally a vendor binary blob.
    ///
    /// The four size fields are computed from the content, and recomputed again
    /// when the block is written, so they can never drift out of step with what
    /// is actually in the file. `logdsks` is zero: this crate does not carry a
    /// vendor-reserved disk area.
    ///
    /// ```
    /// let log = spc_spectra::LogBlock::new("Channel=1\nIntegration=100ms", Vec::new());
    /// assert_eq!(log.get("Channel"), Some("1"));
    /// ```
    pub fn new(text: impl Into<String>, binary: Vec<u8>) -> Self {
        let text = text.into();
        let (logtxto, logsizd) = Self::sizes(&binary, &text);
        Self {
            logsizd,
            logsizm: logsizd,
            logtxto,
            logbins: binary.len() as u32,
            logdsks: 0,
            binary,
            text,
        }
    }

    /// The text offset and on-disk size a block with this content will have.
    ///
    /// The text is measured trimmed, because that is how it is written: see
    /// [`Self::write`].
    fn sizes(binary: &[u8], text: &str) -> (u32, u32) {
        let logtxto = (Self::HEADER_SIZE + binary.len()) as u32;
        (logtxto, logtxto + text.trim().len() as u32 + 1)
    }

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

    /// Writes the log block: its own 64 byte header, the binary area, then the
    /// text area with a terminating null.
    ///
    /// The four size fields are recomputed from what is actually written rather
    /// than copied from the struct — they are offsets into the block being
    /// built, and a stale one would send a reader looking in the wrong place.
    /// `logdsks` is written as zero: it describes a vendor-reserved disk area
    /// this crate does not carry.
    ///
    /// The text is written trimmed, exactly as [`crate::bytes::decode_log_text`]
    /// would hand it back, so that writing a file that was just read reproduces
    /// the same bytes instead of growing or shrinking a trailing newline.
    pub(crate) fn write(&self, s: &mut Sink) {
        let start = s.pos();
        let text = self.text.trim();
        let (logtxto, logsizd) = Self::sizes(&self.binary, &self.text);

        s.u32(logsizd);
        s.u32(logsizd); // logsizm: the block is the same size in memory
        s.u32(logtxto);
        s.u32(self.binary.len() as u32);
        s.u32(0); // logdsks
        s.pad_to(start + Self::HEADER_SIZE);

        s.bytes(&self.binary);
        s.bytes(text.as_bytes());
        s.u8(0);
    }

    /// Best-effort iterator over `key=value` pairs in the text area.
    ///
    /// Many instruments write one `key=value` per line. That is a convention
    /// of the acquiring software, not part of the format, so treat the result
    /// as a hint: lines without an `=` are skipped, no key is guaranteed to
    /// exist, and another instrument may use a layout of its own. Use
    /// [`LogBlock::text`] when you need the exact bytes.
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
