//! A minimal byte sink for building an SPC file in memory.
//!
//! The mirror image of [`crate::bytes::Cursor`]: this is the only place in the
//! crate that appends raw bytes. Nothing here can fail — every value handed to
//! it has already been checked, and a text field arrives as the fixed number of
//! bytes it occupies, see [`crate::text::TextField`].

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
    write_number!(i32, i32);
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
    fn writes_a_negative_32_bit_integer() {
        let mut s = Sink::with_capacity(0);
        s.i32(-2);
        assert_eq!(s.finish(), vec![0xFE, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn padding_fills_the_reserved_tail() {
        let mut s = Sink::with_capacity(0);
        s.u8(0xAB);
        s.pad_to(4);
        assert_eq!(s.finish(), vec![0xAB, 0, 0, 0]);
    }
}
