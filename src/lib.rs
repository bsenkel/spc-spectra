//! Reader and writer for SPC spectroscopy files, the binary format introduced
//! by Galactic Industries and later carried on in Thermo's GRAMS software. It
//! is still the everyday interchange format for FT-IR, Raman, NIR, UV-VIS, NMR
//! and MS data.
//!
//! It works on bytes, so the data need not come from a file at all: an
//! instrument driver, a network stream or a database blob does just as well.
//! [`Spc::from_path`] and [`Spc::to_path`] are conveniences over
//! [`Spc::from_bytes`] and [`Spc::to_bytes`].
//!
//! ```no_run
//! use spc_spectra::Spc;
//!
//! let spc = Spc::from_path("spectrum.spc")?;
//! // or, when the bytes are already in hand: Spc::from_bytes(&raw)?
//!
//! println!("{} ({})", spc.header.fexper, spc.header.fsource);
//! println!("{} points, {} .. {} {}", spc.subfiles[0].y.len(), spc.header.ffirst,
//!          spc.header.flast, spc.x_label());
//!
//! // One subfile per spectrum: a single measurement has one, a series many.
//! for sub in &spc.subfiles {
//!     println!("z = {}", sub.subheader.subtime);
//!     for (x, y) in sub.points().take(5) {
//!         println!("{x:10.3}  {y:12.6}");
//!     }
//! }
//! # Ok::<(), spc_spectra::SpcError>(())
//! ```
//!
//! # Writing
//!
//! [`SpcBuilder`] turns a spectrum into a file, and [`Spc::to_bytes`] or
//! [`Spc::to_path`] serialises one that was read or built:
//!
//! ```no_run
//! use spc_spectra::{SpcBuilder, Technique, XType, YType};
//!
//! let y: Vec<f64> = (0..801).map(|i| 0.1 + f64::from(i) * 0.001).collect();
//!
//! SpcBuilder::new(900.0, 1700.0, y)
//!     .x_type(XType::Nanometers)
//!     .y_type(YType::Absorbance)
//!     .technique(Technique::Nir)
//!     .source("NIR probe")
//!     .log_text("Channel=1\nIntegration=100ms")
//!     .build()?
//!     .to_path("spectrum.spc")?;
//! # Ok::<(), spc_spectra::SpcError>(())
//! ```
//!
//! A file holding a series of spectra is one x axis and many curves, each with
//! the z value that places it in the series:
//!
//! ```no_run
//! use spc_spectra::{SpcBuilder, XType, ZSpacing};
//!
//! let spectra: Vec<(f32, Vec<f64>)> = vec![
//!     (16.57, vec![0.11, 0.12, 0.14]),
//!     (17.42, vec![0.10, 0.13, 0.15]),
//! ];
//!
//! SpcBuilder::series(900.0, 1700.0, spectra)
//!     .z_type(XType::Seconds)
//!     .z_spacing(ZSpacing::Uneven)
//!     .build()?
//!     .to_path("series.spc")?;
//! # Ok::<(), spc_spectra::SpcError>(())
//! ```
//!
//! Reading and writing cover exactly the same ground, and deliberately so: the
//! writer runs the reader's own validation before it writes a byte, so a file
//! this crate produces is always one it can read back. The x axis is not stored
//! in the format — it is regenerated from the two end points — which is why the
//! builder takes a range rather than x values.
//!
//! Every field this crate parses survives a read/write round trip, and a file
//! it wrote is byte-stable. Byte-for-byte fidelity to a *foreign* file is not
//! promised, because the reader does not model everything a file may hold: the
//! reserved tails of the header and subheader and the log block's `logdsks`
//! area are written as nulls, log entries separated by nulls come back
//! separated by newlines, and trailing whitespace in the log text is trimmed.
//! See [`Spc::to_bytes`] for the full list. The header's text fields are not on
//! it: they are kept as the bytes the file held, since decoding them loses
//! everything past the first null and mangles what is not UTF-8. See
//! [`TextField`].
//!
//! # What this version reads and writes
//!
//! Version 0.4 deliberately covers only the most common variants, which is what
//! a typical export from a modern instrument looks like — a single spectrum, or
//! a series of them sharing one x axis:
//!
//! | Aspect | Supported |
//! |---|---|
//! | Version byte | `0x4B` (new format, little-endian) |
//! | Subfiles | one or many (`TMULTI`), sharing one x axis |
//! | x axis | evenly spaced, generated from `ffirst`/`flast` |
//! | y values | IEEE floats (`fexp = 0x80`) and Galactic fixed-point |
//! | Log block | yes, passed through raw |
//!
//! Everything else is rejected with a specific [`Unsupported`] value rather
//! than parsed on a guess — and, since the writer runs the same checks, rather
//! than written on a guess either: big-endian files (`0x4C`), the old format (`0x4D`),
//! `TXYXYS`, explicit x values (`TXVALS`), 16-bit y values
//! (`TSPREC`) and multi-plane data cubes (`fwplanes > 1`).
//!
//! That choice is the point of the crate. For measurement data, a loud error is
//! far more useful than a spectrum that looks plausible and is quietly wrong,
//! and it makes clear which files would benefit from support being added next.
//!
//! # Self-consistency
//!
//! The same principle applies to fields that must agree with each other. An SPC
//! file states some things twice, and where it does, both statements are
//! checked:
//!
//! - Under `TMULTI` the subheader's `subexp` governs the y values, so it must
//!   not announce fixed-point while the file-wide `fexp` announces floats.
//!   Without `TMULTI` the field is not consulted and cannot contradict.
//! - The y values must end at or before `flogoff`, since the log block follows
//!   the data. A point count that would overrun it means one of the two fields
//!   is wrong, and there is no way to tell which.
//! - A `fdate` that unpacks to an impossible date, such as month 15, yields
//!   `None` rather than a plausible-looking timestamp. The raw word stays
//!   available as [`Header::fdate`].
//! - A subfile count of zero, or a non-finite `ffirst`/`flast`, is rejected as
//!   a [`SpcError::MalformedHeader`]: the first still produces points, the
//!   second would fill the x axis with `NaN`.
//!
//! Each of these was, at some point during development, a way to get silently
//! wrong numbers out of a file that parsed without complaint.
//!
//! Writing applies it in the other direction, refusing to put a contradiction
//! into a file rather than to take one out of it: a point count that disagrees
//! with the y values, an x axis that is not the evenly spaced one the end
//! points describe (writing it would substitute that axis silently), an `fnsub`
//! that disagrees with the number of subfiles (a reader loops over `fnsub`, so
//! the rest would be unreachable), and a y value the file's own encoding cannot
//! carry. A text field too long for its slot is refused earlier still, when the
//! field is built, so it can never reach a header at all.
//!
//! # Handling the unsupported cases
//!
//! ```
//! use spc_spectra::{Spc, SpcError};
//!
//! match Spc::from_bytes(&[]) {
//!     Ok(spc) => println!("{} points", spc.subfiles[0].y.len()),
//!     Err(SpcError::Unsupported(u)) => eprintln!("known variant, not decoded yet: {u}"),
//!     Err(SpcError::BadVersion(_)) => eprintln!("this is not an SPC file"),
//!     Err(e) => eprintln!("{e}"),
//! }
//! ```
//!
//! # Trademarks
//!
//! This project is not affiliated with, endorsed by, or sponsored by Thermo
//! Fisher Scientific. Product names are mentioned only to describe which files
//! this crate reads.

#![forbid(unsafe_code)]

mod builder;
mod bytes;
mod error;
mod header;
mod log;
mod spc;
mod subheader;
mod text;
mod write;

pub use builder::SpcBuilder;
pub use error::{SpcError, Unsupported};
pub use header::{
    FEXP_IEEE_FLOAT, Header, SpcDate, TFlags, Technique, VERSION_NEW_BE, VERSION_NEW_LE,
    VERSION_OLD, XType, YType, ZSpacing,
};
pub use log::LogBlock;
pub use spc::{Spc, Subfile};
pub use subheader::{SubFlags, SubHeader};
pub use text::TextField;
