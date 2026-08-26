//! The 512 byte main header that starts every new-format SPC file.

use crate::bytes::Cursor;
use crate::error::{SpcError, Unsupported};
use crate::write::Sink;
use std::fmt;

/// Version byte of the new format with little-endian numbers.
pub const VERSION_NEW_LE: u8 = 0x4B;
/// Version byte of the new format with big-endian numbers.
pub const VERSION_NEW_BE: u8 = 0x4C;
/// Version byte of the old DOS-era format, which uses a 256 byte header.
pub const VERSION_OLD: u8 = 0x4D;

/// The value of `fexp` that marks the y values as IEEE floats.
pub const FEXP_IEEE_FLOAT: i8 = -128; // 0x80 read as a signed byte

/// Layout flags from the `ftflgs` byte.
///
/// These bits combine, and together they decide how the data after the header
/// is laid out. Use the associated constants with [`TFlags::contains`].
///
/// ```
/// use spc_spectra::TFlags;
///
/// let flags = TFlags(TFlags::TMULTI | TFlags::TXVALS);
/// assert!(flags.contains(TFlags::TMULTI));
/// assert!(!flags.contains(TFlags::TSPREC));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TFlags(pub u8);

impl TFlags {
    /// y values are stored as 16 bit rather than 32 bit.
    pub const TSPREC: u8 = 0x01;
    /// Experiment-specific exponent handling is enabled.
    pub const TCGRAM: u8 = 0x02;
    /// The file holds multiple subfiles.
    pub const TMULTI: u8 = 0x04;
    /// Subfile z values are arbitrary rather than evenly spaced.
    pub const TRANDM: u8 = 0x08;
    /// Subfiles are ordered but unevenly spaced in z.
    pub const TORDRD: u8 = 0x10;
    /// Axis labels come from the `fcatxt` field instead of the type codes.
    pub const TALABS: u8 = 0x20;
    /// Every subfile carries its own x axis, plus a directory at the end.
    pub const TXYXYS: u8 = 0x40;
    /// The x axis is stored explicitly instead of being evenly spaced.
    pub const TXVALS: u8 = 0x80;

    /// Returns true if every bit in `mask` is set.
    pub const fn contains(self, mask: u8) -> bool {
        self.0 & mask == mask
    }
}

/// A date and time decoded from the bit-packed `fdate` field.
///
/// SPC does not store a plain timestamp: minute, hour, day, month and year are
/// packed into the bits of a single 32 bit word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpcDate {
    /// Full year, for example 2026.
    pub year: u16,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day of month, 1 to 31.
    pub day: u8,
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
}

impl SpcDate {
    /// Earliest year accepted as a real date. The format itself dates from
    /// 1986, so anything below this is bit garbage rather than a timestamp.
    const MIN_YEAR: u16 = 1900;

    /// Unpacks the bit fields of `fdate`, if they describe a possible date.
    ///
    /// Returns `None` when the word is zero — meaning the writing software
    /// recorded no date — and also when the unpacked fields cannot be a real
    /// date, such as month 15 or minute 63. The bit fields are wider than the
    /// values they hold, so a corrupt or misaligned word unpacks into something
    /// that looks like a date but is not one; reporting `None` keeps that from
    /// being passed off as a measurement time.
    ///
    /// The raw word remains available as `Header::fdate` for anyone who wants
    /// to inspect what was actually stored.
    pub const fn from_packed(fdate: u32) -> Option<Self> {
        if fdate == 0 {
            return None;
        }
        let date = Self {
            year: (fdate >> 20) as u16 & 0x0FFF,
            month: (fdate >> 16) as u8 & 0x0F,
            day: (fdate >> 11) as u8 & 0x1F,
            hour: (fdate >> 6) as u8 & 0x1F,
            minute: fdate as u8 & 0x3F,
        };
        if date.is_plausible() {
            Some(date)
        } else {
            None
        }
    }

    /// Checks the calendar ranges, without worrying about month lengths.
    ///
    /// Deliberately does not reject 31 February: the aim is to catch garbage,
    /// not to audit the instrument's clock, and a real file with an odd but
    /// well-formed date should keep its metadata.
    const fn is_plausible(&self) -> bool {
        self.year >= Self::MIN_YEAR
            && self.month >= 1
            && self.month <= 12
            && self.day >= 1
            && self.day <= 31
            && self.hour <= 23
            && self.minute <= 59
    }

    /// Packs the fields back into the `fdate` representation.
    ///
    /// Mostly useful for tests and for writing files later on.
    pub const fn to_packed(self) -> u32 {
        ((self.year as u32 & 0x0FFF) << 20)
            | ((self.month as u32 & 0x0F) << 16)
            | ((self.day as u32 & 0x1F) << 11)
            | ((self.hour as u32 & 0x1F) << 6)
            | (self.minute as u32 & 0x3F)
    }
}

impl fmt::Display for SpcDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

/// Generates an enum of unit/technique codes with a catch-all variant.
macro_rules! code_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($code:literal => $variant:ident, $label:literal;)* }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum $name {
            $(
                #[doc = $label]
                $variant,
            )*
            /// A code this crate does not know a name for.
            Other(u8),
        }

        impl $name {
            /// Maps a raw code byte to its variant; unknown codes become
            /// [`Self::Other`] rather than an error.
            pub const fn from_code(code: u8) -> Self {
                match code {
                    $($code => Self::$variant,)*
                    other => Self::Other(other),
                }
            }

            /// The raw code byte this variant came from.
            pub const fn code(self) -> u8 {
                match self {
                    $(Self::$variant => $code,)*
                    Self::Other(c) => c,
                }
            }

            /// A short human-readable label, suitable for axis annotation.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)*
                    Self::Other(_) => "Unknown",
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Other(c) => write!(f, "Unknown ({c})"),
                    known => f.write_str(known.as_str()),
                }
            }
        }
    };
}

code_enum! {
    /// Unit of the x axis, from the `fxtype` code.
    XType {
        0 => Arbitrary, "Arbitrary";
        1 => Wavenumber, "Wavenumber (cm-1)";
        2 => Micrometers, "Micrometers (um)";
        3 => Nanometers, "Nanometers (nm)";
        4 => Seconds, "Seconds";
        5 => Minutes, "Minutes";
        6 => Hertz, "Hertz (Hz)";
        7 => Kilohertz, "Kilohertz (kHz)";
        8 => Megahertz, "Megahertz (MHz)";
        9 => MassPerCharge, "Mass (m/z)";
        10 => PartsPerMillion, "Parts per million (ppm)";
        11 => Days, "Days";
        12 => Years, "Years";
        13 => RamanShift, "Raman shift (cm-1)";
        14 => ElectronVolts, "Electron volts (eV)";
        16 => Diode, "Diode number";
        17 => Channel, "Channel";
        18 => Degrees, "Degrees";
        19 => TemperatureF, "Temperature (F)";
        20 => TemperatureC, "Temperature (C)";
        21 => TemperatureK, "Temperature (K)";
        22 => DataPoints, "Data points";
        23 => Milliseconds, "Milliseconds (ms)";
        24 => Microseconds, "Microseconds (us)";
        25 => Nanoseconds, "Nanoseconds (ns)";
        26 => Gigahertz, "Gigahertz (GHz)";
        27 => Centimeters, "Centimeters (cm)";
        28 => Meters, "Meters (m)";
        29 => Millimeters, "Millimeters (mm)";
        30 => Hours, "Hours";
        255 => DoubleInterferogram, "Double interferogram";
    }
}

code_enum! {
    /// Unit of the y axis, from the `fytype` code.
    YType {
        0 => ArbitraryIntensity, "Arbitrary intensity";
        1 => Interferogram, "Interferogram";
        2 => Absorbance, "Absorbance";
        3 => KubelkaMunk, "Kubelka-Munk";
        4 => Counts, "Counts";
        5 => Volts, "Volts";
        6 => Degrees, "Degrees";
        7 => Milliamps, "Milliamps";
        8 => Millimeters, "Millimeters";
        9 => Millivolts, "Millivolts";
        10 => LogOneOverR, "Log(1/R)";
        11 => Percent, "Percent";
        12 => Intensity, "Intensity";
        13 => RelativeIntensity, "Relative intensity";
        14 => Energy, "Energy";
        16 => Decibel, "Decibel";
        19 => TemperatureF, "Temperature (F)";
        20 => TemperatureC, "Temperature (C)";
        21 => TemperatureK, "Temperature (K)";
        22 => IndexOfRefraction, "Index of refraction";
        23 => ExtinctionCoefficient, "Extinction coefficient";
        24 => Real, "Real";
        25 => Imaginary, "Imaginary";
        26 => Complex, "Complex";
        128 => Transmission, "Transmission";
        129 => Reflectance, "Reflectance";
        130 => ArbitraryOrSingleBeam, "Arbitrary or single beam";
        131 => Emission, "Emission";
    }
}

code_enum! {
    /// Measurement technique, from the `fexper` code.
    Technique {
        0 => General, "General or unspecified";
        1 => GasChromatogram, "Gas chromatogram";
        2 => GeneralChromatogram, "General chromatogram";
        3 => HplcChromatogram, "HPLC chromatogram";
        4 => FtirOrRaman, "FT-IR, FT-NIR or FT-Raman";
        5 => Nir, "NIR";
        7 => UvVis, "UV-VIS";
        8 => XRayDiffraction, "X-ray diffraction";
        9 => MassSpectrum, "Mass spectrum";
        10 => Nmr, "NMR";
        11 => Raman, "Raman";
        12 => Fluorescence, "Fluorescence";
        13 => Atomic, "Atomic";
        14 => ChromatographyDiodeArray, "Chromatography diode array";
    }
}

/// The 512 byte main header of a new-format SPC file.
///
/// Field names follow the original format documentation so that values can be
/// compared against other implementations without a translation table.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Header {
    /// Layout flags, see [`TFlags`].
    pub ftflgs: TFlags,
    /// Version byte: `0x4B`, `0x4C` or `0x4D`.
    pub fversn: u8,
    /// Measurement technique code.
    pub fexper: Technique,
    /// `0x80` for IEEE floats, otherwise the shared fixed-point exponent.
    pub fexp: i8,
    /// Number of points per subfile.
    pub fnpts: u32,
    /// First x value.
    pub ffirst: f64,
    /// Last x value.
    pub flast: f64,
    /// Number of subfiles.
    pub fnsub: u32,
    /// x axis unit.
    pub fxtype: XType,
    /// y axis unit.
    pub fytype: YType,
    /// z axis unit, used for multifile records.
    pub fztype: XType,
    /// Posting disposition byte.
    pub fpost: u8,
    /// Raw bit-packed date word.
    pub fdate: u32,
    /// The decoded date, or `None` if `fdate` was zero or held impossible
    /// values. See [`SpcDate::from_packed`].
    pub date: Option<SpcDate>,
    /// Free-text resolution description.
    pub fres: String,
    /// Free-text source instrument description.
    pub fsource: String,
    /// Index of the peak point, for interferograms.
    pub fpeakpt: u16,
    /// Eight spare floats reserved by the format.
    pub fspare: [f32; 8],
    /// Free-text memo field.
    pub fcmnt: String,
    /// Custom axis labels, null-separated, used when `TALABS` is set.
    pub fcatxt: [u8; 30],
    /// Byte offset of the log block, or zero if there is none.
    pub flogoff: u32,
    /// Bit flags recording how the data was modified after acquisition.
    pub fmods: u32,
    /// Processing code.
    pub fprocs: u8,
    /// Calibration level plus one.
    pub flevel: u8,
    /// Sub-method sample injection number.
    pub fsampin: u16,
    /// Multiplier applied to the data by the writing software.
    pub ffactor: f32,
    /// Method file name.
    pub fmethod: String,
    /// z increment between subfiles.
    pub fzinc: f32,
    /// Number of w planes.
    pub fwplanes: u32,
    /// w plane increment.
    pub fwinc: f32,
    /// w axis unit.
    pub fwtype: XType,
}

impl Header {
    /// Size of the main header in bytes.
    pub const SIZE: usize = 512;

    /// The header's fixed-width text fields: name, and width in bytes.
    ///
    /// Stated once so that reading, writing and validating cannot drift apart.
    /// A value may fill its field completely; the width is what ends it.
    const TEXT_FIELDS: [(&'static str, usize); 4] =
        [("fres", 9), ("fsource", 9), ("fcmnt", 130), ("fmethod", 48)];

    /// Reads the header from the current cursor position.
    pub(crate) fn parse(c: &mut Cursor<'_>) -> Result<Self, SpcError> {
        const CTX: &str = "the main header";
        let start = c.pos();

        let ftflgs = TFlags(c.u8(CTX)?);
        let fversn = c.u8(CTX)?;
        let fexper = Technique::from_code(c.u8(CTX)?);
        let fexp = c.i8(CTX)?;
        let fnpts = c.u32(CTX)?;
        let ffirst = c.f64(CTX)?;
        let flast = c.f64(CTX)?;
        let fnsub = c.u32(CTX)?;
        let fxtype = XType::from_code(c.u8(CTX)?);
        let fytype = YType::from_code(c.u8(CTX)?);
        let fztype = XType::from_code(c.u8(CTX)?);
        let fpost = c.u8(CTX)?;
        let fdate = c.u32(CTX)?;
        let fres = c.text(9, CTX)?;
        let fsource = c.text(9, CTX)?;
        let fpeakpt = c.u16(CTX)?;

        let mut fspare = [0f32; 8];
        for slot in &mut fspare {
            *slot = c.f32(CTX)?;
        }

        let fcmnt = c.text(130, CTX)?;
        let mut fcatxt = [0u8; 30];
        fcatxt.copy_from_slice(c.bytes(30, CTX)?);

        let flogoff = c.u32(CTX)?;
        let fmods = c.u32(CTX)?;
        let fprocs = c.u8(CTX)?;
        let flevel = c.u8(CTX)?;
        let fsampin = c.u16(CTX)?;
        let ffactor = c.f32(CTX)?;
        let fmethod = c.text(48, CTX)?;
        let fzinc = c.f32(CTX)?;
        let fwplanes = c.u32(CTX)?;
        let fwinc = c.f32(CTX)?;
        let fwtype = XType::from_code(c.u8(CTX)?);

        // Skip the reserved tail so the cursor lands exactly on the next
        // structure regardless of how many fields were read above.
        c.skip(Self::SIZE - (c.pos() - start), CTX)?;

        Ok(Self {
            ftflgs,
            fversn,
            fexper,
            fexp,
            fnpts,
            ffirst,
            flast,
            fnsub,
            fxtype,
            fytype,
            fztype,
            fpost,
            fdate,
            date: SpcDate::from_packed(fdate),
            fres,
            fsource,
            fpeakpt,
            fspare,
            fcmnt,
            fcatxt,
            flogoff,
            fmods,
            fprocs,
            flevel,
            fsampin,
            ffactor,
            fmethod,
            fzinc,
            fwplanes,
            fwinc,
            fwtype,
        })
    }

    /// Writes the header back out, mirroring [`Self::parse`] field for field.
    ///
    /// `flogoff` is passed in rather than taken from the struct: it is a byte
    /// offset into the file being built, so only the writer knows it, and a
    /// stale value would point the log block into the middle of the spectrum.
    ///
    /// Everything after `fwtype` is the format's reserved tail and is written
    /// as nulls. A header that came from another program may have had something
    /// in there; this crate does not model it, so it cannot preserve it.
    pub(crate) fn write(&self, s: &mut Sink, flogoff: u32) -> Result<(), SpcError> {
        let start = s.pos();

        s.u8(self.ftflgs.0);
        s.u8(self.fversn);
        s.u8(self.fexper.code());
        s.i8(self.fexp);
        s.u32(self.fnpts);
        s.f64(self.ffirst);
        s.f64(self.flast);
        s.u32(self.fnsub);
        s.u8(self.fxtype.code());
        s.u8(self.fytype.code());
        s.u8(self.fztype.code());
        s.u8(self.fpost);
        s.u32(self.fdate);
        s.text(&self.fres, Self::TEXT_FIELDS[0].1, "fres")?;
        s.text(&self.fsource, Self::TEXT_FIELDS[1].1, "fsource")?;
        s.u16(self.fpeakpt);
        for v in self.fspare {
            s.f32(v);
        }
        s.text(&self.fcmnt, Self::TEXT_FIELDS[2].1, "fcmnt")?;
        s.bytes(&self.fcatxt);
        s.u32(flogoff);
        s.u32(self.fmods);
        s.u8(self.fprocs);
        s.u8(self.flevel);
        s.u16(self.fsampin);
        s.f32(self.ffactor);
        s.text(&self.fmethod, Self::TEXT_FIELDS[3].1, "fmethod")?;
        s.f32(self.fzinc);
        s.u32(self.fwplanes);
        s.f32(self.fwinc);
        s.u8(self.fwtype.code());

        s.pad_to(start + Self::SIZE);
        Ok(())
    }

    /// Rejects every file variant this version cannot decode correctly.
    ///
    /// Each rejection is a distinct [`Unsupported`] value, so callers can tell
    /// "I need a newer version of this crate" apart from "this is not an SPC
    /// file".
    pub(crate) fn validate(&self) -> Result<(), SpcError> {
        match self.fversn {
            VERSION_NEW_LE => {}
            VERSION_NEW_BE => return Err(Unsupported::BigEndian.into()),
            VERSION_OLD => return Err(Unsupported::OldFormat.into()),
            other => return Err(SpcError::BadVersion(other)),
        }

        // Checked before TXVALS/TMULTI because a TXYXYS file is also flagged
        // as both, and this is the more specific diagnosis.
        if self.ftflgs.contains(TFlags::TXYXYS) {
            return Err(Unsupported::XyxySubfiles.into());
        }
        if self.ftflgs.contains(TFlags::TXVALS) {
            return Err(Unsupported::ExplicitXValues.into());
        }
        if self.ftflgs.contains(TFlags::TMULTI) || self.fnsub > 1 {
            return Err(Unsupported::MultiFile.into());
        }
        if self.ftflgs.contains(TFlags::TSPREC) {
            return Err(Unsupported::SixteenBitY.into());
        }
        if self.fwplanes > 1 {
            return Err(Unsupported::WPlanes {
                fwplanes: self.fwplanes,
            }
            .into());
        }
        if self.fexp != FEXP_IEEE_FLOAT {
            return Err(Unsupported::FixedPointY { fexp: self.fexp }.into());
        }

        // Contradictions that would otherwise pass silently: a zero subfile
        // count still produces points, and a non-finite endpoint poisons the
        // generated x axis with NaN without any complaint.
        if self.fnsub == 0 {
            return Err(SpcError::MalformedHeader {
                detail: "fnsub is 0 (no subfiles)",
            });
        }
        if !self.ffirst.is_finite() {
            return Err(SpcError::MalformedHeader {
                detail: "ffirst is not a finite number",
            });
        }
        if !self.flast.is_finite() {
            return Err(SpcError::MalformedHeader {
                detail: "flast is not a finite number",
            });
        }
        Ok(())
    }

    /// Checks that every text field still fits the slot it came from.
    ///
    /// Called before writing, so that an over-long comment is reported where it
    /// was set rather than halfway through building the file. A value read from
    /// a file normally fits by construction — with one exception worth knowing
    /// about: text that was not valid UTF-8 is decoded lossily, and each
    /// replaced byte becomes a three byte `U+FFFD`. Such a field can grow past
    /// its slot, and then the file can be read but not written back.
    pub(crate) fn validate_text_fields(&self) -> Result<(), SpcError> {
        let values = [&self.fres, &self.fsource, &self.fcmnt, &self.fmethod];
        for (&(field, max), value) in Self::TEXT_FIELDS.iter().zip(values) {
            if value.len() > max {
                return Err(SpcError::FieldTooLong {
                    field,
                    max,
                    len: value.len(),
                });
            }
        }
        Ok(())
    }

    /// The custom axis labels from `fcatxt`, split at null bytes.
    ///
    /// The format stores x, y and z labels in that order. Only meaningful when
    /// [`TFlags::TALABS`] is set.
    pub fn custom_axis_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = self
            .fcatxt
            .split(|&b| b == 0)
            .map(crate::bytes::decode_text)
            .collect();
        // The field is null-padded, so drop the empty tail but keep any blank
        // label that sits between two filled ones.
        while labels.last().is_some_and(|s| s.is_empty()) {
            labels.pop();
        }
        labels
    }

    /// Label for the x axis, honouring `TALABS` when present.
    pub fn x_label(&self) -> String {
        self.custom_label(0)
            .unwrap_or_else(|| self.fxtype.as_str().to_string())
    }

    /// Label for the y axis, honouring `TALABS` when present.
    pub fn y_label(&self) -> String {
        self.custom_label(1)
            .unwrap_or_else(|| self.fytype.as_str().to_string())
    }

    fn custom_label(&self, index: usize) -> Option<String> {
        if !self.ftflgs.contains(TFlags::TALABS) {
            return None;
        }
        self.custom_axis_labels()
            .into_iter()
            .nth(index)
            .filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_bit_fields_round_trip() {
        let d = SpcDate {
            year: 2026,
            month: 7,
            day: 21,
            hour: 14,
            minute: 37,
        };
        assert_eq!(SpcDate::from_packed(d.to_packed()), Some(d));
    }

    #[test]
    fn a_zero_date_word_means_no_date() {
        assert_eq!(SpcDate::from_packed(0), None);
    }

    #[test]
    fn date_fields_do_not_bleed_into_each_other() {
        // Pure bit-layout check: every field saturated must fill the word
        // exactly, with no overlap and no gap.
        let packed = SpcDate {
            year: 4095,
            month: 15,
            day: 31,
            hour: 31,
            minute: 63,
        }
        .to_packed();
        assert_eq!(packed, u32::MAX);

        // And the largest word that is actually a date survives unpacking.
        let max = SpcDate {
            year: 4095,
            month: 12,
            day: 31,
            hour: 23,
            minute: 59,
        };
        assert_eq!(SpcDate::from_packed(max.to_packed()), Some(max));
    }

    #[test]
    fn impossible_dates_are_reported_as_no_date() {
        // The bit fields are wider than the values they hold, so garbage
        // unpacks into something date-shaped. It must not be handed back as a
        // measurement time.
        assert_eq!(SpcDate::from_packed(u32::MAX), None, "month 15, minute 63");

        let valid = SpcDate {
            year: 2026,
            month: 7,
            day: 21,
            hour: 14,
            minute: 37,
        };
        assert!(SpcDate::from_packed(valid.to_packed()).is_some());

        for broken in [
            SpcDate { month: 0, ..valid }, // months count from 1
            SpcDate { month: 13, ..valid },
            SpcDate { day: 0, ..valid }, // so do days
            SpcDate { day: 32, ..valid },
            SpcDate { hour: 24, ..valid },
            SpcDate {
                minute: 60,
                ..valid
            },
            SpcDate {
                year: 1899,
                ..valid
            }, // predates the format itself
            SpcDate { year: 12, ..valid },
        ] {
            assert_eq!(
                SpcDate::from_packed(broken.to_packed()),
                None,
                "should have been rejected: {broken:?}"
            );
        }
    }

    #[test]
    fn flags_test_all_bits_of_the_mask() {
        let f = TFlags(TFlags::TMULTI);
        assert!(f.contains(TFlags::TMULTI));
        assert!(!f.contains(TFlags::TMULTI | TFlags::TXVALS));
    }

    #[test]
    fn unknown_unit_codes_survive_as_other() {
        assert_eq!(XType::from_code(3), XType::Nanometers);
        assert_eq!(XType::from_code(200), XType::Other(200));
        assert_eq!(XType::from_code(200).code(), 200);
        assert_eq!(YType::from_code(2).as_str(), "Absorbance");
    }
}
