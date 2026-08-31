//! Error types returned when reading or writing an SPC file.

use std::fmt;

/// Anything that can go wrong while reading or writing an SPC file.
#[derive(Debug)]
#[non_exhaustive]
pub enum SpcError {
    /// The file could not be opened, read or written.
    Io(std::io::Error),

    /// The data ended before a structure could be read completely.
    ///
    /// `context` names the structure that was being read, `needed` is how many
    /// bytes it still wanted and `available` how many were left.
    TooShort {
        /// What was being read when the data ran out.
        context: &'static str,
        /// Number of bytes required.
        needed: usize,
        /// Number of bytes actually left.
        available: usize,
    },

    /// The version byte (`fversn`) is not one of the three known values.
    ///
    /// This usually means the data is not an SPC file at all.
    BadVersion(u8),

    /// The file is a valid SPC file, but uses a variant this version of the
    /// crate does not decode yet.
    ///
    /// Reading is refused rather than guessed: a spectrum that silently
    /// contains wrong numbers is far worse than a loud error.
    Unsupported(Unsupported),

    /// The point count is zero, which no valid file declares.
    ///
    /// A count too large for the file is reported as [`Self::TooShort`] or
    /// [`Self::DataOverrunsLogBlock`] instead, because those say what ran out.
    InvalidPointCount(u32),

    /// The point count and the log block offset contradict each other: reading
    /// that many y values would run past the start of the log block.
    ///
    /// The format places the log block after the data, so `flogoff` doubles as
    /// a statement of where the y values end. When the two disagree, one of
    /// them is wrong — and since there is no way to tell which, the y values
    /// cannot be trusted either way. Refused rather than quietly truncated.
    DataOverrunsLogBlock {
        /// Byte offset at which the y values would end.
        data_end: usize,
        /// Byte offset at which the log block claims to start (`flogoff`).
        log_offset: u32,
    },

    /// A header field holds a value that cannot describe a real file, so no
    /// meaningful spectrum can be built from it.
    ///
    /// Covers a subfile count of zero and non-finite x endpoints (`NaN` or
    /// infinity in `ffirst`/`flast`), either of which would otherwise pass
    /// silently — a zero count still yields points, and a `NaN` endpoint
    /// poisons the generated x axis without any complaint.
    MalformedHeader {
        /// Which field was wrong, and what it held.
        detail: &'static str,
    },

    /// A text field holds more bytes than its fixed-width slot in the file.
    ///
    /// Truncating instead would put a silently shortened comment or instrument
    /// name into the file. A value that fills the slot exactly is fine: the
    /// field's width ends it, no terminating null required.
    FieldTooLong {
        /// Name of the header field, as the format documentation spells it.
        field: &'static str,
        /// Number of bytes the field can hold.
        max: usize,
        /// Number of bytes the value actually needs.
        len: usize,
    },

    /// The `Spc` in hand describes something this version cannot serialise.
    ///
    /// Unlike [`Self::Unsupported`], which is about a file that was read, this
    /// is about a value that would have been written: more than one subfile, an
    /// empty spectrum, a point count that disagrees with the y values, or an x
    /// axis that is not the evenly spaced one the format implies.
    NotWritable {
        /// What made it unwritable.
        detail: &'static str,
    },

    /// A finite y value that the file's own y encoding cannot carry.
    ///
    /// Some precision is always lost, and that is not an error: the format
    /// rounds by design. This is about a value that would be destroyed
    /// outright, which happens in three ways. On the float path, a finite
    /// number that turns into an infinity when narrowed to `f32`. On the
    /// fixed-point path, a value beyond the range of `i32` at the file's
    /// scale, or one smaller than half a step, which would quantise to zero.
    ValueNotRepresentable {
        /// Index of the offending point.
        index: usize,
        /// The value the encoding cannot carry.
        value: f64,
    },
}

/// The specific format variant that is not supported yet.
///
/// See the crate-level documentation for the roadmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unsupported {
    /// `fversn = 0x4C`: new format, but all multi-byte fields are big-endian.
    BigEndian,
    /// `fversn = 0x4D`: the old DOS-era format with a 256 byte header.
    OldFormat,
    /// More than one subfile (`TMULTI`, or `fnsub` greater than one).
    ///
    /// No longer produced as of 0.3.0, which reads and writes these files. The
    /// variant stays so that existing `match` arms keep compiling.
    MultiFile,
    /// `TXYXYS`: every subfile carries its own x axis plus a directory.
    XyxySubfiles,
    /// `TXVALS`: the x axis is stored explicitly instead of being evenly spaced.
    ExplicitXValues,
    /// `fexp` is not `0x80`, so the y values are Galactic fixed-point numbers
    /// scaled by this shared exponent.
    ///
    /// No longer produced as of 0.4.0, which reads and writes these files. The
    /// variant stays so that existing `match` arms keep compiling.
    FixedPointY {
        /// The shared scaling exponent read from the header.
        fexp: i8,
    },
    /// `TMULTI` is set, so the subfile's own `subexp` governs its y values and
    /// says fixed-point, while the file-wide `fexp` announces IEEE floats.
    ///
    /// Two fields then disagree about how the same four bytes are read, and
    /// nothing else in the file settles it. Picking a winner would be a guess,
    /// and the wrong guess yields a plausible-looking, entirely wrong spectrum.
    ///
    /// Without `TMULTI` this cannot arise: `subexp` is not consulted at all,
    /// so whatever it holds passes through untouched.
    FixedPointSubfileY {
        /// The subfile's own exponent.
        subexp: i8,
        /// The file-wide exponent it contradicts.
        fexp: i8,
    },
    /// `TSPREC`: y values are stored as 16 bit instead of 32 bit.
    SixteenBitY,
    /// `fwplanes` is greater than one: the file is a multi-plane data cube
    /// (for example a depth stack), whose data is laid out differently.
    WPlanes {
        /// The number of w planes the header declares.
        fwplanes: u32,
    },
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BigEndian => f.write_str("big-endian files (fversn 0x4C)"),
            Self::OldFormat => f.write_str("the old format (fversn 0x4D)"),
            Self::MultiFile => f.write_str("multifile records (more than one subfile)"),
            Self::XyxySubfiles => f.write_str("subfiles with their own x axis (TXYXYS)"),
            Self::ExplicitXValues => f.write_str("explicitly stored x values (TXVALS)"),
            Self::FixedPointY { fexp } => {
                write!(
                    f,
                    "fixed-point y values (fexp {fexp}, expected 0x80 for IEEE floats)"
                )
            }
            Self::FixedPointSubfileY { subexp, fexp } => write!(
                f,
                "fixed-point y values in a subfile (its subexp {subexp} contradicts the \
                 file-wide fexp {fexp})"
            ),
            Self::SixteenBitY => f.write_str("16-bit y values (TSPREC)"),
            Self::WPlanes { fwplanes } => {
                write!(f, "multi-plane data cubes (fwplanes {fwplanes})")
            }
        }
    }
}

impl fmt::Display for SpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "could not read or write the file: {e}"),
            Self::TooShort {
                context,
                needed,
                available,
            } => write!(
                f,
                "file ended while reading {context}: needed {needed} more byte(s), {available} left"
            ),
            Self::BadVersion(v) => write!(
                f,
                "unknown SPC version byte {v:#04X} (expected 0x4B, 0x4C or 0x4D) \
                 - this is probably not an SPC file"
            ),
            Self::Unsupported(u) => write!(f, "not supported yet: {u}"),
            Self::InvalidPointCount(n) => write!(f, "implausible point count: {n}"),
            Self::DataOverrunsLogBlock {
                data_end,
                log_offset,
            } => write!(
                f,
                "the header contradicts itself: the y values would end at byte {data_end}, \
                 but the log block starts at {log_offset}"
            ),
            Self::MalformedHeader { detail } => write!(f, "malformed header: {detail}"),
            Self::FieldTooLong { field, max, len } => write!(
                f,
                "{field} is {len} bytes long, but the field holds only {max}"
            ),
            Self::NotWritable { detail } => write!(f, "cannot be written: {detail}"),
            Self::ValueNotRepresentable { index, value } => write!(
                f,
                "y value {value} at index {index} is finite, but this file's y \
                 encoding cannot carry it"
            ),
        }
    }
}

impl std::error::Error for SpcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SpcError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<Unsupported> for SpcError {
    fn from(u: Unsupported) -> Self {
        Self::Unsupported(u)
    }
}
