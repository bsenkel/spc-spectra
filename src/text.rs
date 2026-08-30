//! The header's fixed-width text fields.

use crate::bytes::decode_text;
use crate::error::SpcError;
use std::fmt;

/// A fixed-width, null-padded text field from the header.
///
/// The field is kept as the bytes the file held, not as a decoded `String`,
/// because the decoding is not reversible. Three things happen on the way to a
/// `String` and none of them can be undone: everything after the first null is
/// dropped, invalid UTF-8 becomes a three byte replacement character, and
/// leading and trailing spaces are trimmed. Real instrument files run into all
/// three — a comment field holding two null-separated entries, a resolution
/// field that is not UTF-8 at all — so a crate that promises to write back what
/// it read has to carry the bytes.
///
/// Read the text with [`text`](Self::text), which applies exactly that decoding
/// and is what [`Display`](fmt::Display) uses, or with
/// [`entries`](Self::entries) for a field that holds several null-separated
/// values.
///
/// ```
/// # use spc_spectra::TextField;
/// let field: TextField<9> = TextField::from_bytes(*b"2 cm-1\0\0\0");
/// assert_eq!(field.text(), "2 cm-1");
/// assert_eq!(field.as_bytes(), b"2 cm-1\0\0\0");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TextField<const N: usize> {
    raw: [u8; N],
}

impl<const N: usize> TextField<N> {
    /// Width of the field in bytes, as the format fixes it.
    pub const WIDTH: usize = N;

    /// Takes the field exactly as it stands in the file.
    #[must_use]
    pub const fn from_bytes(raw: [u8; N]) -> Self {
        Self { raw }
    }

    /// Builds a field from text, null-padding it to the full width.
    ///
    /// `field` names the header field and is used only to make the error
    /// legible; pass the name the format documentation uses, `"fcmnt"` say.
    /// To change a field of a header you already have, prefer the setters that
    /// know their own name — [`Header::set_fcmnt`](crate::Header::set_fcmnt)
    /// and its four siblings.
    ///
    /// Text that fills the slot exactly is accepted and written without a
    /// terminating null: the field's width is what ends it, which is how real
    /// files use the nine byte `fsource`.
    ///
    /// # Errors
    ///
    /// [`SpcError::FieldTooLong`] if the text needs more than `N` bytes.
    /// Truncating instead would put a silently shortened comment into a file.
    pub fn new(field: &'static str, text: &str) -> Result<Self, SpcError> {
        let bytes = text.as_bytes();
        if bytes.len() > N {
            return Err(SpcError::FieldTooLong {
                field,
                max: N,
                len: bytes.len(),
            });
        }
        let mut raw = [0u8; N];
        raw[..bytes.len()].copy_from_slice(bytes);
        Ok(Self { raw })
    }

    /// The field's bytes, padding included.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; N] {
        &self.raw
    }

    /// The field's text: up to the first null, lossily decoded, trimmed.
    ///
    /// This is the format's own rule for a fixed-width field, so it is the
    /// right answer for almost every file. Where it is not — a field holding
    /// several entries — [`entries`](Self::entries) gives all of them.
    #[must_use]
    pub fn text(&self) -> String {
        decode_text(&self.raw)
    }

    /// Whether the field holds anything worth showing.
    ///
    /// True for a field that is nothing but null padding and whitespace, which
    /// is what an unused slot looks like — so a caller can leave out the row
    /// or heading it would otherwise label. Equivalent to
    /// `entries().is_empty()`, without building the entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.raw.iter().all(|&b| b == 0 || b.is_ascii_whitespace())
    }

    /// Every null-separated entry in the field, with the empty padding at the
    /// end dropped.
    ///
    /// `fcatxt` is defined this way, holding the x, y and z axis labels in that
    /// order. Some programs also use it for `fcmnt`, where the format expects a
    /// single value, which is why [`text`](Self::text) sees only the first one.
    #[must_use]
    pub fn entries(&self) -> Vec<String> {
        let mut out: Vec<String> = self.raw.split(|&b| b == 0).map(decode_text).collect();
        // Keep a blank entry that sits between two filled ones; drop only the
        // run of empties that the null padding produces.
        while out.last().is_some_and(String::is_empty) {
            out.pop();
        }
        out
    }
}

impl<const N: usize> Default for TextField<N> {
    /// An empty field: `N` null bytes, which is what an unused slot holds.
    fn default() -> Self {
        Self { raw: [0u8; N] }
    }
}

impl<const N: usize> fmt::Display for TextField<N> {
    /// Writes [`text`](TextField::text).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

impl<const N: usize> fmt::Debug for TextField<N> {
    /// Shows the raw bytes escaped, not the decoded text.
    ///
    /// A field's whole point here is the bytes past the first null, so a
    /// debug view that stopped there would hide the difference it exists to
    /// preserve.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TextField(b\"{}\")", self.raw.escape_ascii())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_bytes_a_file_held() {
        let field = TextField::from_bytes(*b"ab\0cd\0\0\0\0");
        assert_eq!(field.as_bytes(), b"ab\0cd\0\0\0\0");
        assert_eq!(field.text(), "ab");
        assert_eq!(field.entries(), ["ab", "cd"]);
    }

    #[test]
    fn text_that_is_not_utf8_survives_as_bytes() {
        let field = TextField::from_bytes([b'a', 0xFF, b'b', 0, 0, 0, 0, 0, 0]);
        // Three bytes of replacement character where one byte stood: exactly
        // the growth that used to make such a field unwritable.
        assert_eq!(field.text().len(), 5);
        assert_eq!(field.as_bytes().len(), 9);
    }

    #[test]
    fn text_is_null_padded_to_the_full_width() {
        let field = TextField::<9>::new("fsource", "NIR").unwrap();
        assert_eq!(field.as_bytes(), b"NIR\0\0\0\0\0\0");
    }

    #[test]
    fn text_that_fills_the_slot_needs_no_terminating_null() {
        let field = TextField::<9>::new("fsource", "NIR probe").unwrap();
        assert_eq!(field.as_bytes(), b"NIR probe");
        assert_eq!(field.text(), "NIR probe");
    }

    #[test]
    fn text_longer_than_the_slot_names_the_field() {
        match TextField::<9>::new("fsource", "NIR probe 2") {
            Err(SpcError::FieldTooLong { field, max, len }) => {
                assert_eq!((field, max, len), ("fsource", 9, 11));
            }
            other => panic!("expected FieldTooLong, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_field_is_all_nulls() {
        let field = TextField::<4>::default();
        assert_eq!(field.as_bytes(), b"\0\0\0\0");
        assert_eq!(field.text(), "");
        assert!(field.entries().is_empty());
        assert!(field.is_empty());
    }

    /// `is_empty` is documented as agreeing with `entries`, so it has to, on
    /// the awkward shapes as well as the obvious ones.
    #[test]
    fn is_empty_agrees_with_entries() {
        for raw in [*b"\0\0\0\0", *b"  \0\0", *b"ab\0\0", *b"\0\0ab", *b"a\0\0b"] {
            let field = TextField::from_bytes(raw);
            assert_eq!(
                field.is_empty(),
                field.entries().is_empty(),
                "disagreement on {:?}",
                raw.escape_ascii().to_string()
            );
        }
    }

    #[test]
    fn debug_shows_the_bytes_rather_than_the_decoded_text() {
        let field = TextField::from_bytes(*b"ab\0cd\0\0\0\0");
        assert_eq!(
            format!("{field:?}"),
            r#"TextField(b"ab\x00cd\x00\x00\x00\x00")"#
        );
    }
}
