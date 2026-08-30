//! The 32 byte header that precedes each subfile's data block.

use crate::bytes::Cursor;
use crate::error::{SpcError, Unsupported};
use crate::header::{FEXP_IEEE_FLOAT, Header, TFlags};
use crate::write::Sink;

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

    /// Writes the subheader back out, mirroring [`Self::parse`] field for field.
    ///
    /// `subnpts` is written as it stands, so a file that used the
    /// "inherit from `fnpts`" shorthand keeps it. The caller has already
    /// checked that the two agree with the number of y values.
    pub(crate) fn write(&self, s: &mut Sink) {
        let start = s.pos();

        s.u8(self.subflgs.0);
        s.i8(self.subexp);
        s.u16(self.subindx);
        s.f32(self.subtime);
        s.f32(self.subnext);
        s.f32(self.subnois);
        s.u32(self.subnpts);
        s.u32(self.subscan);
        s.f32(self.subwlevel);

        s.pad_to(start + Self::SIZE);
    }

    /// The exponent that actually governs this subfile's y values.
    ///
    /// Without `TMULTI` the file-wide `fexp` governs and `subexp` is not
    /// consulted at all; with `TMULTI` the subfile's own `subexp` does. That is
    /// the rule the Python, Julia and R readers implement, and the R one cites
    /// the format documentation for it.
    ///
    /// A value of `0x80` means IEEE floats. Every other value is a fixed-point
    /// exponent, including `0` — which is why this crate does not treat `0` as
    /// an unfilled field the way one other reader does.
    pub(crate) const fn effective_exponent(&self, header: &Header) -> i8 {
        if header.ftflgs.contains(TFlags::TMULTI) {
            self.subexp
        } else {
            header.fexp
        }
    }

    /// Rejects a subfile whose own exponent contradicts the file-wide one in a
    /// way that cannot be resolved from what the file states.
    ///
    /// Only one combination is genuinely ambiguous: `TMULTI` is set, so
    /// `subexp` governs and says fixed-point, while `fexp` announces floats for
    /// the file. Two fields then disagree about how four bytes are to be read,
    /// and nothing in the file settles it. Another reader resolves this in
    /// favour of the header and says so in a message; this one refuses, because
    /// picking a winner would be a guess and the wrong guess produces a
    /// spectrum that looks entirely plausible and is entirely wrong.
    ///
    /// Without `TMULTI` there is nothing to check: `subexp` is not read, so
    /// whatever it holds is an observation that travels through unchanged.
    pub(crate) fn validate(&self, header: &Header) -> Result<(), SpcError> {
        let contradicts = header.ftflgs.contains(TFlags::TMULTI)
            && header.fexp == FEXP_IEEE_FLOAT
            && self.subexp != FEXP_IEEE_FLOAT;
        if contradicts {
            return Err(Unsupported::FixedPointSubfileY {
                subexp: self.subexp,
                fexp: header.fexp,
            }
            .into());
        }
        Ok(())
    }

    /// Number of points in this subfile.
    ///
    /// A `subnpts` of zero is not an empty subfile: it is the common shorthand
    /// for "the same count as the main header", used whenever a subfile shares
    /// the file-wide x axis — which, without `TXYXYS`, they all do.
    pub const fn npts(&self, header: &Header) -> u32 {
        if self.subnpts == 0 {
            header.fnpts
        } else {
            self.subnpts
        }
    }
}
