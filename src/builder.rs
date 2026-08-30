//! Building an SPC file from measured data.

use crate::error::SpcError;
use crate::header::{
    FEXP_IEEE_FLOAT, Header, SpcDate, TFlags, Technique, VERSION_NEW_LE, XType, YType, ZSpacing,
};
use crate::log::LogBlock;
use crate::spc::{Spc, Subfile};
use crate::subheader::{SubFlags, SubHeader};
use crate::text::TextField;

/// One spectrum and the z value that places it in the series.
#[derive(Debug, Clone)]
struct Spectrum {
    subtime: f32,
    y: Vec<f64>,
}

/// Builds a writable [`Spc`] from one or more spectra and their metadata.
///
/// The x axis is not stored in an SPC file; it is described by its two end
/// points and the number of y values, and every reader regenerates it from
/// those. That is why the axis is given as a range here rather than as values.
/// Every spectrum in the file shares it: [`Self::series`] takes a whole series
/// at once, [`Self::add_spectrum_at`] appends one more.
///
/// Everything else has a defensible default, so a minimal file needs only the
/// range and the data. What [`Self::build`] produces is always writable: it runs
/// the same checks [`Spc::to_bytes`] does, so a mistake surfaces here rather
/// than at the filesystem.
///
/// The setters cover what describes a measurement. The format has a good many
/// further fields — `ffactor`, `fpeakpt`, `fsampin`, `subtime` — that a
/// single-spectrum export leaves at zero; rather than a setter each, they are
/// set on the [`Spc`] the builder returns, whose fields are public:
///
/// ```
/// # use spc_spectra::SpcBuilder;
/// let mut spc = SpcBuilder::new(900.0, 1700.0, vec![0.1, 0.2]).build()?;
/// spc.header.ffactor = 2.0;
/// spc.subfiles[0].subheader.subtime = 12.5;
/// # assert_eq!(spc.to_bytes()?.len(), 552);
/// # Ok::<(), spc_spectra::SpcError>(())
/// ```
///
/// ```
/// use spc_spectra::{SpcBuilder, SpcDate, Technique, XType, YType};
///
/// let y: Vec<f64> = (0..801).map(|i| 0.1 + f64::from(i) * 0.001).collect();
///
/// let spc = SpcBuilder::new(900.0, 1700.0, y)
///     .x_type(XType::Nanometers)
///     .y_type(YType::Absorbance)
///     .technique(Technique::Nir)
///     .source("NIR probe")
///     .resolution("2nm")
///     .scans(32)
///     .comment("cellulose, 32 scans")
///     .date(SpcDate { year: 2026, month: 8, day: 26, hour: 9, minute: 15 })
///     .log_text("Channel=1\nIntegration=100ms")
///     .build()?;
///
/// assert_eq!(spc.subfiles[0].x[0], 900.0);
/// assert_eq!(spc.subfiles[0].x[800], 1700.0);
///
/// let bytes = spc.to_bytes()?;
/// let read_back = spc_spectra::Spc::from_bytes(&bytes)?;
///
/// assert_eq!(read_back.subfiles[0].y.len(), spc.subfiles[0].y.len());
/// // y values are stored as 32-bit floats, so the round trip costs precision.
/// assert!((read_back.subfiles[0].y[0] - spc.subfiles[0].y[0]).abs() < 1e-7);
/// # Ok::<(), spc_spectra::SpcError>(())
/// ```
#[derive(Debug, Clone)]
pub struct SpcBuilder {
    ffirst: f64,
    flast: f64,
    spectra: Vec<Spectrum>,
    fxtype: XType,
    fytype: YType,
    fztype: XType,
    z_spacing: ZSpacing,
    fexper: Technique,
    date: Option<SpcDate>,
    fres: String,
    fsource: String,
    fcmnt: String,
    fmethod: String,
    fcatxt: [u8; 30],
    talabs: bool,
    subscan: u32,
    log_text: Option<String>,
    log_binary: Vec<u8>,
}

impl SpcBuilder {
    /// Starts a file covering `first ..= last` on the x axis, with these y
    /// values.
    ///
    /// `first` may be larger than `last`: a descending wavenumber axis is
    /// ordinary in FT-IR and Raman data.
    pub fn new(first: f64, last: f64, y: Vec<f64>) -> Self {
        Self {
            ffirst: first,
            flast: last,
            spectra: vec![Spectrum { subtime: 0.0, y }],
            fxtype: XType::Arbitrary,
            fytype: YType::ArbitraryIntensity,
            fztype: XType::Arbitrary,
            z_spacing: ZSpacing::Even,
            fexper: Technique::General,
            date: None,
            fres: String::new(),
            fsource: String::new(),
            fcmnt: String::new(),
            fmethod: String::new(),
            fcatxt: [0; 30],
            talabs: false,
            subscan: 1,
            log_text: None,
            log_binary: Vec::new(),
        }
    }

    /// Starts a file holding a series of spectra, each with the z value that
    /// places it in the series — an acquisition time, a temperature, a
    /// position.
    ///
    /// The counterpart to [`Self::new`] for a multifile record. Every spectrum
    /// shares the one x axis `first ..= last` describes, so they all need the
    /// same number of points; [`Self::build`] refuses a set that does not, and
    /// an empty series.
    ///
    /// Unlike [`Self::new`], this records the first spectrum's z value as well.
    /// Instrument files routinely have one: a series is timed from when the
    /// instrument started, not from when this particular run began.
    ///
    /// ```
    /// use spc_spectra::{SpcBuilder, XType};
    ///
    /// let spectra = vec![
    ///     (16.57, vec![0.11, 0.12, 0.14]),
    ///     (17.42, vec![0.10, 0.13, 0.15]),
    ///     (18.30, vec![0.12, 0.11, 0.16]),
    /// ];
    ///
    /// let spc = SpcBuilder::series(900.0, 1700.0, spectra)
    ///     .z_type(XType::Seconds)
    ///     .build()?;
    ///
    /// assert_eq!(spc.subfiles.len(), 3);
    /// assert_eq!(spc.subfiles[0].subheader.subtime, 16.57);
    /// # Ok::<(), spc_spectra::SpcError>(())
    /// ```
    pub fn series(first: f64, last: f64, spectra: Vec<(f32, Vec<f64>)>) -> Self {
        Self {
            spectra: spectra
                .into_iter()
                .map(|(subtime, y)| Spectrum { subtime, y })
                .collect(),
            ..Self::new(first, last, Vec::new())
        }
    }

    /// Sets the x axis unit (`fxtype`).
    #[must_use]
    pub fn x_type(mut self, t: XType) -> Self {
        self.fxtype = t;
        self
    }

    /// Sets the y axis unit (`fytype`).
    #[must_use]
    pub fn y_type(mut self, t: YType) -> Self {
        self.fytype = t;
        self
    }

    /// Sets the unit of the z axis (`fztype`), which is what the per-spectrum
    /// z values mean — a time, a temperature, a position.
    ///
    /// Only meaningful with more than one spectrum. [`Self::z_spacing`] states
    /// how those values are spaced. See also [`Self::add_spectrum_at`].
    #[must_use]
    pub fn z_type(mut self, t: XType) -> Self {
        self.fztype = t;
        self
    }

    /// States how the z values are spaced, which sets `TORDRD` or `TRANDM`.
    ///
    /// Defaults to [`ZSpacing::Even`], which is what the format means when
    /// neither flag is set — and what another program will assume, computing
    /// each spectrum's z from the first one and a constant step instead of
    /// reading it. An instrument that cannot hold an exact interval should say
    /// [`ZSpacing::Uneven`], or its spectra land where they roughly ought to be
    /// rather than where they were measured.
    ///
    /// Not derived from the values themselves: deciding whether two `f32`
    /// intervals are "the same" needs a tolerance, and that is a guess this
    /// crate leaves to the caller who knows the instrument.
    ///
    /// [`Self::z_type`] states what the values mean.
    #[must_use]
    pub fn z_spacing(mut self, s: ZSpacing) -> Self {
        self.z_spacing = s;
        self
    }

    /// Appends a further spectrum, sharing the x axis and every header field.
    ///
    /// Its z value stays zero. Only the caller knows what the series is ordered
    /// by, and numbering the spectra 0, 1, 2 … would be data this crate made up;
    /// [`Self::add_spectrum_at`] records the real value.
    #[must_use]
    pub fn add_spectrum(self, y: Vec<f64>) -> Self {
        self.add_spectrum_at(0.0, y)
    }

    /// Appends a further spectrum together with the z value that places it in
    /// the series (`subtime`), such as an acquisition time.
    ///
    /// Every spectrum needs the same number of points, since they all share the
    /// one x axis the range describes; [`Self::build`] refuses a set that does
    /// not. Whether the z values count as evenly spaced is [`Self::z_spacing`]:
    /// that is a claim about the measurement, not something to read off the
    /// numbers.
    ///
    /// ```
    /// use spc_spectra::{SpcBuilder, XType};
    ///
    /// let spc = SpcBuilder::new(900.0, 1700.0, vec![0.1, 0.2])
    ///     .z_type(XType::Seconds)
    ///     .add_spectrum_at(1.5, vec![0.3, 0.4])
    ///     .add_spectrum_at(3.0, vec![0.5, 0.6])
    ///     .build()?;
    ///
    /// assert_eq!(spc.subfiles.len(), 3);
    /// assert_eq!(spc.subfiles[2].subheader.subtime, 3.0);
    /// # Ok::<(), spc_spectra::SpcError>(())
    /// ```
    #[must_use]
    pub fn add_spectrum_at(mut self, z: f32, y: Vec<f64>) -> Self {
        self.spectra.push(Spectrum { subtime: z, y });
        self
    }

    /// Sets the measurement technique (`fexper`).
    #[must_use]
    pub fn technique(mut self, t: Technique) -> Self {
        self.fexper = t;
        self
    }

    /// Sets the acquisition date and time (`fdate`).
    ///
    /// Left unset, the file records no date, which is what a zero `fdate` means.
    #[must_use]
    pub fn date(mut self, date: SpcDate) -> Self {
        self.date = Some(date);
        self
    }

    /// Sets the instrument description (`fsource`, at most 9 bytes).
    #[must_use]
    pub fn source(mut self, s: impl Into<String>) -> Self {
        self.fsource = s.into();
        self
    }

    /// Sets the resolution description (`fres`, at most 9 bytes).
    #[must_use]
    pub fn resolution(mut self, s: impl Into<String>) -> Self {
        self.fres = s.into();
        self
    }

    /// Sets the memo field (`fcmnt`, at most 130 bytes).
    #[must_use]
    pub fn comment(mut self, s: impl Into<String>) -> Self {
        self.fcmnt = s.into();
        self
    }

    /// Sets the number of co-added scans (`subscan`).
    ///
    /// The format has a field for this, so a spectrum averaged over 32 scans
    /// should say so there rather than only in the log text: another program
    /// reads `subscan`, while `Averages=32` in the log is a convention it has
    /// no reason to know. Defaults to 1, a single scan, and applies to every
    /// spectrum in the file.
    #[must_use]
    pub fn scans(mut self, n: u32) -> Self {
        self.subscan = n;
        self
    }

    /// Sets the method file name (`fmethod`, at most 48 bytes).
    #[must_use]
    pub fn method(mut self, s: impl Into<String>) -> Self {
        self.fmethod = s.into();
        self
    }

    /// Overrides the axis labels with custom text, and sets [`TFlags::TALABS`].
    ///
    /// The format stores x, y and z labels in that order, null-separated, in a
    /// single 30 byte field. Labels that do not fit are dropped rather than cut
    /// in half, and passing none clears the flag again.
    #[must_use]
    pub fn custom_axis_labels(mut self, labels: &[&str]) -> Self {
        let mut raw = [0u8; 30];
        let mut at = 0;
        for label in labels {
            let bytes = label.as_bytes();
            // The label plus its separator has to fit, or it is not written at
            // all: half a label is worse than none.
            if at + bytes.len() + 1 > raw.len() {
                break;
            }
            raw[at..at + bytes.len()].copy_from_slice(bytes);
            at += bytes.len() + 1;
        }
        self.fcatxt = raw;
        self.talabs = at > 0;
        self
    }

    /// Sets the text area of the log block.
    ///
    /// Instruments conventionally write one `key=value` per line, which is what
    /// [`LogBlock::entries`] expects, but the area is free-form. Leading and
    /// trailing whitespace is trimmed when the file is written.
    #[must_use]
    pub fn log_text(mut self, s: impl Into<String>) -> Self {
        self.log_text = Some(s.into());
        self
    }

    /// Sets the binary area of the log block, which is vendor-specific.
    #[must_use]
    pub fn log_binary(mut self, b: Vec<u8>) -> Self {
        self.log_binary = b;
        self
    }

    /// Assembles the [`Spc`], checking that it can actually be written.
    ///
    /// # Errors
    ///
    /// [`SpcError::NotWritable`] for a spectrum with no points or more points
    /// than the format's 32 bit count can hold, [`SpcError::MalformedHeader`]
    /// for a non-finite end point, [`SpcError::ValueNotRepresentable`] for a
    /// finite y value with no `f32` equivalent, and [`SpcError::FieldTooLong`]
    /// for a text field that does not fit its slot.
    pub fn build(self) -> Result<Spc, SpcError> {
        // `series` accepts any list, the empty one included.
        let Some(head) = self.spectra.first() else {
            return Err(SpcError::NotWritable {
                detail: "there is no spectrum to write",
            });
        };

        // Without TXYXYS every subfile shares the one x axis the range
        // describes, so spectra of different lengths would each get a
        // differently spaced axis over the same range. Refused rather than
        // written: it is a mistake far more often than an intention.
        let npts = head.y.len();
        if self.spectra.iter().any(|s| s.y.len() != npts) {
            return Err(SpcError::NotWritable {
                detail: "the spectra do not all have the same number of points",
            });
        }
        let fnpts = u32::try_from(npts).map_err(|_| SpcError::NotWritable {
            detail: "more points than the format's 32-bit count can hold",
        })?;
        // `subindx` numbers the subfiles and is only 16 bits wide.
        if self.spectra.len() > u16::MAX as usize + 1 {
            return Err(SpcError::NotWritable {
                detail: "more spectra than the format's 16-bit subfile index can number",
            });
        }
        let fnsub = self.spectra.len() as u32;

        let mut ftflgs = TFlags::default();
        if self.talabs {
            ftflgs.0 |= TFlags::TALABS;
        }
        if fnsub > 1 {
            ftflgs.0 |= TFlags::TMULTI;
        }
        // Assigned from a fresh `ftflgs`, so `Even` leaves both bits clear
        // rather than merely not setting them.
        ftflgs.0 |= self.z_spacing.flags();

        let header = Header {
            ftflgs,
            fversn: VERSION_NEW_LE,
            fexper: self.fexper,
            fexp: FEXP_IEEE_FLOAT,
            fnpts,
            ffirst: self.ffirst,
            flast: self.flast,
            fnsub,
            fxtype: self.fxtype,
            fytype: self.fytype,
            fztype: self.fztype,
            fpost: 0,
            fdate: self.date.map_or(0, SpcDate::to_packed),
            date: self.date,
            fres: TextField::new("fres", &self.fres)?,
            fsource: TextField::new("fsource", &self.fsource)?,
            fpeakpt: 0,
            fspare: [0.0; 8],
            fcmnt: TextField::new("fcmnt", &self.fcmnt)?,
            fcatxt: TextField::from_bytes(self.fcatxt),
            // A byte offset into a file that does not exist yet; the writer
            // fills it in from the geometry it actually produces.
            flogoff: 0,
            fmods: 0,
            fprocs: 0,
            flevel: 0,
            fsampin: 0,
            ffactor: 1.0,
            fmethod: TextField::new("fmethod", &self.fmethod)?,
            fzinc: 0.0,
            fwplanes: 0,
            fwinc: 0.0,
            fwtype: XType::Arbitrary,
        };

        let x = crate::spc::generate_x(self.ffirst, self.flast, npts);
        let subscan = self.subscan;
        let subfiles = self
            .spectra
            .into_iter()
            .enumerate()
            .map(|(i, spectrum)| Subfile {
                subheader: SubHeader {
                    subflgs: SubFlags::default(),
                    subexp: FEXP_IEEE_FLOAT,
                    subindx: i as u16,
                    subtime: spectrum.subtime,
                    subnext: 0.0,
                    subnois: 0.0,
                    // The "same count as the main header" shorthand, which is
                    // what every subfile here holds.
                    subnpts: 0,
                    subscan,
                    subwlevel: 0.0,
                },
                x: x.clone(),
                y: spectrum.y,
            })
            .collect();

        let log = match (self.log_text, self.log_binary) {
            (None, b) if b.is_empty() => None,
            (text, binary) => Some(LogBlock::new(text.unwrap_or_default(), binary)),
        };

        let spc = Spc {
            header,
            subfiles,
            log,
        };

        // Fail here rather than at the filesystem: a builder that hands back an
        // Spc which cannot be written has only postponed the error.
        spc.writable_subfiles()?;
        Ok(spc)
    }
}
