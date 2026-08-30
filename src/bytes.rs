//! A minimal bounds-checked reader over a byte slice.
//!
//! This is the only place in the crate that slices raw bytes. Every accessor
//! checks the remaining length first and returns [`SpcError::TooShort`] instead
//! of panicking, so a truncated or corrupt file can never bring down a caller.

use crate::error::SpcError;
use crate::text::TextField;

/// A read cursor over an in-memory SPC file.
pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

macro_rules! read_number {
    ($name:ident, $ty:ty) => {
        /// Reads one little-endian value and advances the cursor.
        pub(crate) fn $name(&mut self, context: &'static str) -> Result<$ty, SpcError> {
            const N: usize = size_of::<$ty>();
            let raw = self.bytes(N, context)?;
            // The slice is exactly N bytes long, so the conversion cannot fail.
            Ok(<$ty>::from_le_bytes(raw.try_into().unwrap()))
        }
    };
}

impl<'a> Cursor<'a> {
    /// Wraps a byte slice, starting at offset zero.
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current byte offset from the start of the file.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Total length of the underlying data.
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    /// Number of bytes left after the cursor.
    pub(crate) fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Moves the cursor to an absolute offset.
    pub(crate) fn seek(&mut self, pos: usize, context: &'static str) -> Result<(), SpcError> {
        if pos > self.data.len() {
            return Err(SpcError::TooShort {
                context,
                needed: pos - self.data.len(),
                available: 0,
            });
        }
        self.pos = pos;
        Ok(())
    }

    /// Skips `n` bytes, typically reserved or padding fields.
    pub(crate) fn skip(&mut self, n: usize, context: &'static str) -> Result<(), SpcError> {
        self.bytes(n, context).map(|_| ())
    }

    /// Borrows the next `n` bytes and advances the cursor.
    pub(crate) fn bytes(&mut self, n: usize, context: &'static str) -> Result<&'a [u8], SpcError> {
        let end = self.pos.checked_add(n).ok_or(SpcError::TooShort {
            context,
            needed: n,
            available: self.remaining(),
        })?;
        if end > self.data.len() {
            return Err(SpcError::TooShort {
                context,
                needed: n,
                available: self.remaining(),
            });
        }
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    read_number!(u8, u8);
    read_number!(i8, i8);
    read_number!(u16, u16);
    read_number!(u32, u32);
    read_number!(i32, i32);
    read_number!(f32, f32);
    read_number!(f64, f64);

    /// Reads a fixed-width text field, keeping its bytes as they stand.
    ///
    /// The decoding happens in [`TextField`], on demand, so that nothing the
    /// file held is lost between reading and writing it back.
    pub(crate) fn text_field<const N: usize>(
        &mut self,
        context: &'static str,
    ) -> Result<TextField<N>, SpcError> {
        let raw = self.bytes(N, context)?;
        let mut field = [0u8; N];
        field.copy_from_slice(raw);
        Ok(TextField::from_bytes(field))
    }
}

/// Decodes a fixed-width, null-padded text field.
///
/// These fields (comment, source, method, …) treat the first null as the end
/// of the string and pad the rest, so truncating there is correct.
pub(crate) fn decode_text(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).trim().to_string()
}

/// Decodes a run of text that may contain embedded null bytes.
///
/// The log block's text area is not a fixed-width field: some instruments
/// separate their `key=value` entries with nulls rather than newlines, so
/// stopping at the first null would throw away everything after the first
/// entry. Here nulls are turned into newlines and only trailing padding is
/// dropped, keeping every entry intact.
pub(crate) fn decode_log_text(raw: &[u8]) -> String {
    // Drop trailing null padding, but keep nulls that sit between entries.
    let end = raw.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    let mapped: Vec<u8> = raw[..end]
        .iter()
        .map(|&b| if b == 0 { b'\n' } else { b })
        .collect();
    String::from_utf8_lossy(&mapped).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbers_in_little_endian_order() {
        let data = [0x01u8, 0x02, 0x03, 0x04];
        let mut c = Cursor::new(&data);
        assert_eq!(c.u16("t").unwrap(), 0x0201);
        assert_eq!(c.u16("t").unwrap(), 0x0403);
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn reads_a_negative_32_bit_integer() {
        // Fixed-point y values are signed, so the sign bit has to survive the
        // read rather than turning a trough into a very large peak.
        let data = (-2i32).to_le_bytes();
        let mut c = Cursor::new(&data);
        assert_eq!(c.i32("the y values").unwrap(), -2);
    }

    #[test]
    fn reports_how_much_was_missing() {
        let data = [0x01u8, 0x02];
        let mut c = Cursor::new(&data);
        let err = c.u32("the header").unwrap_err();
        match err {
            SpcError::TooShort {
                context,
                needed,
                available,
            } => {
                assert_eq!(context, "the header");
                assert_eq!(needed, 4);
                assert_eq!(available, 2);
            }
            other => panic!("expected TooShort, got {other:?}"),
        }
        // A failed read must not move the cursor.
        assert_eq!(c.pos(), 0);
    }

    #[test]
    fn text_stops_at_the_first_null() {
        assert_eq!(decode_text(b"NIR probe\0\0\0"), "NIR probe");
        assert_eq!(decode_text(b"  padded  "), "padded");
        assert_eq!(decode_text(b""), "");
        // Invalid UTF-8 is replaced rather than rejected.
        assert_eq!(decode_text(&[b'a', 0xFF, b'b']).chars().count(), 3);
    }

    #[test]
    fn log_text_keeps_embedded_nulls_as_separators() {
        // Trailing null padding is dropped, but nulls between entries become
        // newlines so nothing after the first one is lost.
        assert_eq!(decode_log_text(b"A=1\0B=2\0\0\0"), "A=1\nB=2");
        assert_eq!(decode_log_text(b"only=one"), "only=one");
        assert_eq!(decode_log_text(b"\0\0\0"), "");
        assert_eq!(decode_log_text(b""), "");
    }

    #[test]
    fn seek_past_the_end_is_an_error_not_a_panic() {
        let data = [0u8; 4];
        let mut c = Cursor::new(&data);
        assert!(c.seek(5, "log block").is_err());
        assert!(c.seek(4, "log block").is_ok());
    }
}
