//! The 32 byte header that precedes each subfile's data block.

use crate::bytes::Cursor;
use crate::error::{SpcError, Unsupported};
use crate::header::{FEXP_IEEE_FLOAT, Header};

/// Flags from the `subflgs` byte of a subheader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubFlags(pub u8);

impl SubFlags {
    /// The subfile has been changed since it was created.
    pub const SUBCHGD: u8 = 0x01;
    /// The subfile has no peak table entry.
    pub const SUBNOPT: u8 = 0x08;
    /// The subfile has been modified by arithmetic.
    pub const SUBMODF: u8 = 0x80;

    /// Returns true if every bit in `mask` is set.
    pub const fn contains(self, mask: u8) -> bool {
        self.0 & mask == mask
    }
}

/// The 32 byte header in front of one subfile's y values.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SubHeader {
    /// Subfile flags, see [`SubFlags`].
    pub subflgs: SubFlags,
    /// Per-subfile fixed-point exponent, or `0x80` for IEEE floats.
    pub subexp: i8,
    /// Index of this subfile within the file.
    pub subindx: u16,
    /// z value of this subfile, often an acquisition time.
    pub subtime: f32,
    /// z value of the following subfile.
    pub subnext: f32,
    /// Noise value used for peak picking.
    pub subnois: f32,
    /// Number of points in this subfile, or zero to inherit `fnpts`.
    pub subnpts: u32,
    /// Number of co-added scans.
    pub subscan: u32,
    /// Floating w axis value for this subfile.
    pub subwlevel: f32,
}

impl SubHeader {
    /// Size of a subheader in bytes.
    pub const SIZE: usize = 32;

    /// Reads a subheader from the current cursor position.
    pub(crate) fn parse(c: &mut Cursor<'_>) -> Result<Self, SpcError> {
        const CTX: &str = "a subheader";
        let start = c.pos();

        let subflgs = SubFlags(c.u8(CTX)?);
        let subexp = c.i8(CTX)?;
        let subindx = c.u16(CTX)?;
        let subtime = c.f32(CTX)?;
        let subnext = c.f32(CTX)?;
        let subnois = c.f32(CTX)?;
        let subnpts = c.u32(CTX)?;
        let subscan = c.u32(CTX)?;
        let subwlevel = c.f32(CTX)?;

        c.skip(Self::SIZE - (c.pos() - start), CTX)?;

        Ok(Self {
            subflgs,
            subexp,
            subindx,
            subtime,
            subnext,
            subnois,
            subnpts,
            subscan,
            subwlevel,
        })
    }

    /// Rejects a subfile whose own exponent contradicts the file-wide one.
    ///
    /// `subexp` may legitimately repeat `fexp`, or be `0x80` to mean "IEEE
    /// floats regardless". Anything else says this subfile is encoded
    /// differently from what the main header advertises — and since version 0.1
    /// only decodes floats, carrying on would produce a spectrum that looks
    /// entirely plausible and is entirely wrong.
    ///
    /// This is strict on purpose. A `subexp` of `0` is a valid fixed-point
    /// exponent, not an obvious "field not filled in" marker, so it is refused
    /// rather than waved through. If some instrument turns out to zero the
    /// field as a matter of course, that is worth finding out from a loud error
    /// on a known file rather than from silently shifted numbers later.
    pub(crate) fn validate(&self, header: &Header) -> Result<(), SpcError> {
        if self.subexp == FEXP_IEEE_FLOAT || self.subexp == header.fexp {
            return Ok(());
        }
        Err(Unsupported::FixedPointSubfileY {
            subexp: self.subexp,
            fexp: header.fexp,
        }
        .into())
    }

    /// Number of points in this subfile.
    ///
    /// A `subnpts` of zero is not an empty subfile: it is the common shorthand
    /// for "the same count as the main header", used whenever a single subfile
    /// shares the file-wide x axis.
    pub const fn npts(&self, header: &Header) -> u32 {
        if self.subnpts == 0 {
            header.fnpts
        } else {
            self.subnpts
        }
    }
}
