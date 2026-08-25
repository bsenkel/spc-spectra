//! Reader for SPC spectroscopy files, the binary format introduced by Galactic
//! Industries and later carried on in Thermo's GRAMS software. It is still the
//! everyday interchange format for FT-IR, Raman, NIR, UV-VIS, NMR and MS data.
//!
//! ```no_run
//! use spc_spectra::Spc;
//!
//! let spc = Spc::from_path("spectrum.spc")?;
//!
//! println!("{} ({})", spc.header.fexper, spc.header.fsource);
//! println!("{} points, {} .. {} {}", spc.y().len(), spc.header.ffirst,
//!          spc.header.flast, spc.x_label());
//!
//! for (x, y) in spc.subfiles[0].points().take(5) {
//!     println!("{x:10.3}  {y:12.6}");
//! }
//! # Ok::<(), spc_spectra::SpcError>(())
//! ```
//!
//! # What this version reads
//!
//! Version 0.1 deliberately covers only the most common variant, which is what
//! a typical single-spectrum export from a modern instrument looks like:
//!
//! | Aspect | Supported |
//! |---|---|
//! | Version byte | `0x4B` (new format, little-endian) |
//! | Subfiles | exactly one |
//! | x axis | evenly spaced, generated from `ffirst`/`flast` |
//! | y values | IEEE floats (`fexp = 0x80`) |
//! | Log block | yes, passed through raw |
//!
//! Everything else is rejected with a specific [`Unsupported`] value rather
//! than parsed on a guess: big-endian files (`0x4C`), the old format (`0x4D`),
//! multifile records, `TXYXYS`, explicit x values (`TXVALS`), 16-bit y values
//! (`TSPREC`), multi-plane data cubes (`fwplanes > 1`) and Galactic fixed-point
//! y values — including the case where the subheader's own `subexp` contradicts
//! the file-wide `fexp`.
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
//! - The subheader's `subexp` must not contradict the file-wide `fexp`, or the
//!   y values are encoded differently from what the main header advertises.
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
//! # Handling the unsupported cases
//!
//! ```
//! use spc_spectra::{Spc, SpcError};
//!
//! match Spc::from_bytes(&[]) {
//!     Ok(spc) => println!("{} points", spc.y().len()),
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

mod bytes;
mod error;
mod header;
mod log;
mod spc;
mod subheader;

pub use error::{SpcError, Unsupported};
pub use header::{
    FEXP_IEEE_FLOAT, Header, SpcDate, TFlags, Technique, VERSION_NEW_BE, VERSION_NEW_LE,
    VERSION_OLD, XType, YType,
};
pub use log::LogBlock;
pub use spc::{Spc, Subfile};
pub use subheader::{SubFlags, SubHeader};
