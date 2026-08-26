//! A minimal byte sink for building an SPC file in memory.
//!
//! The mirror image of [`crate::bytes::Cursor`]: this is the only place in the
//! crate that appends raw bytes. Fixed-width text fields are the one thing that
//! can fail here — a value too long for its slot is refused rather than
//! truncated, because a silently shortened comment or instrument name is
//! exactly the kind of quiet damage this crate exists to avoid.

use crate::error::SpcError;

/// A write cursor that grows a `Vec<u8>`.
pub(crate) struct Sink {
    out: Vec<u8>,
}

macro_rules! write_number {
    ($name:ident, $ty:ty) => {
        /// Appends one little-endian value.
        pub(crate) fn $name(&mut self, v: $ty) {
            self.out.extend_from_slice(&v.to_le_bytes());
        }
    };
}

impl Sink {
    /// A sink with room for `capacity` bytes reserved up front.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            out: Vec::with_capacity(capacity),
        }
    }

    /// Number of bytes written so far.
    pub(crate) fn pos(&self) -> usize {
        self.out.len()
    }

    /// Appends raw bytes.
    pub(crate) fn bytes(&mut self, raw: &[u8]) {
        self.out.extend_from_slice(raw);
    }

    /// Pads with null bytes until the sink holds exactly `len` bytes.
    ///
    /// Used for the reserved tails of the header, the subheader and the log
    /// block header, so their sizes stay stated in one place: the `SIZE`
    /// constants the parser already uses.
    pub(crate) fn pad_to(&mut self, len: usize) {
        debug_assert!(self.out.len() <= len, "structure overran its own size");
        self.out.resize(len, 0);
    }

    write_number!(u16, u16);
    write_number!(u32, u32);
    write_number!(f32, f32);
    write_number!(f64, f64);

    /// Appends one byte.
    pub(crate) fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    /// Appends one signed byte.
    pub(crate) fn i8(&mut self, v: i8) {
        self.out.push(v as u8);
    }

    /// Writes a fixed-width, null-padded text field.
    ///
    /// A value that fills the slot exactly is written without a terminating
    /// null. Real files do that — `fsource` is only nine bytes wide, and
    /// instrument names run right up to the end of it — and the field's width
    /// is what ends the value, so [`crate::bytes::decode_text`] reads it back
    /// correctly. Insisting on a spare byte here would make a readable file
    /// unwritable, so only a value longer than the slot is refused.
    pub(crate) fn text(
        &mut self,
        s: &str,
        width: usize,
        field: &'static str,
    ) -> Result<(), SpcError> {
        let raw = s.as_bytes();
        if raw.len() > width {
            return Err(SpcError::FieldTooLong {
                field,
                max: width,
                len: raw.len(),
            });
        }
        let end = self.out.len() + width;
        self.out.extend_from_slice(raw);
        self.out.resize(end, 0);
        Ok(())
    }

    /// Hands back the finished file.
    pub(crate) fn finish(self) -> Vec<u8> {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_numbers_in_little_endian_order() {
        let mut s = Sink::with_capacity(0);
        s.u16(0x0201);
        s.u32(0x0807_0605);
        assert_eq!(s.finish(), vec![0x01, 0x02, 0x05, 0x06, 0x07, 0x08]);
    }

    #[test]
    fn text_fields_are_null_padded_to_their_full_width() {
        let mut s = Sink::with_capacity(0);
        s.text("2nm", 9, "fres").unwrap();
        assert_eq!(s.pos(), 9);
        assert_eq!(s.finish(), b"2nm\0\0\0\0\0\0");
    }

    #[test]
    fn a_value_that_fills_the_slot_exactly_is_written_without_a_terminator() {
        // A nine byte name in a nine byte field, which is what real exports
        // look like. Rejecting it would make a readable file unwritable.
        let mut s = Sink::with_capacity(0);
        s.text("NIR probe", 9, "fsource").unwrap();
        assert_eq!(s.finish(), b"NIR probe");
    }

    #[test]
    fn a_value_longer_than_its_slot_is_refused_rather_than_truncated() {
        let mut s = Sink::with_capacity(0);
        match s.text("NIR probes", 9, "fsource") {
            Err(SpcError::FieldTooLong { field, max, len }) => {
                assert_eq!((field, max, len), ("fsource", 9, 10));
            }
            other => panic!("expected FieldTooLong, got {other:?}"),
        }
        assert_eq!(s.pos(), 0, "a refused write must leave nothing behind");
    }

    #[test]
    fn an_empty_text_field_is_all_nulls() {
        let mut s = Sink::with_capacity(0);
        s.text("", 4, "fmethod").unwrap();
        assert_eq!(s.finish(), vec![0, 0, 0, 0]);
    }

    #[test]
    fn padding_fills_the_reserved_tail() {
        let mut s = Sink::with_capacity(0);
        s.u8(0xAB);
        s.pad_to(4);
        assert_eq!(s.finish(), vec![0xAB, 0, 0, 0]);
    }
}
