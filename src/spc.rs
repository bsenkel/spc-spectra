//! The parsed file and its subfiles.

use crate::bytes::Cursor;
use crate::error::SpcError;
use crate::header::Header;
use crate::log::LogBlock;
use crate::subheader::SubHeader;
use std::path::Path;

/// A single spectrum: one subheader plus its x and y arrays.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Subfile {
    /// The 32 byte header that introduced this subfile.
    pub subheader: SubHeader,
    /// x values, generated from the header's `ffirst`/`flast` range.
    pub x: Vec<f64>,
    /// y values, widened to `f64` regardless of how they were stored.
    pub y: Vec<f64>,
}

impl Subfile {
    /// Number of points, identical for `x` and `y`.
    pub fn len(&self) -> usize {
        self.y.len()
    }

    /// True if the subfile holds no points.
    pub fn is_empty(&self) -> bool {
        self.y.is_empty()
    }

    /// Iterates over the spectrum as `(x, y)` pairs.
    pub fn points(&self) -> impl Iterator<Item = (f64, f64)> {
        self.x.iter().copied().zip(self.y.iter().copied())
    }
}

/// A parsed SPC file.
///
/// # Example
///
/// ```no_run
/// let spc = spc_spectra::Spc::from_path("spectrum.spc")?;
/// println!("{} points from {} to {}", spc.y().len(), spc.header.ffirst, spc.header.flast);
/// # Ok::<(), spc_spectra::SpcError>(())
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Spc {
    /// The 512 byte main header.
    pub header: Header,
    /// The subfiles. Version 0.1 always yields exactly one.
    pub subfiles: Vec<Subfile>,
    /// The log block, if the file has one and it could be read.
    pub log: Option<LogBlock>,
}

impl Spc {
    /// Reads and parses an SPC file from disk.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, SpcError> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Parses an SPC file that is already in memory.
    pub fn from_bytes(data: &[u8]) -> Result<Self, SpcError> {
        let mut c = Cursor::new(data);

        let header = Header::parse(&mut c)?;
        header.validate()?;

        let subheader = SubHeader::parse(&mut c)?;
        subheader.validate(&header)?;

        let npts = subheader.npts(&header);
        if npts == 0 {
            return Err(SpcError::InvalidPointCount(npts));
        }
        let npts = npts as usize;

        // The log block sits after the data, so `flogoff` is also a statement
        // about where the y values end. Cross-checking the two catches a
        // corrupted point count that would otherwise silently pull log block
        // bytes into the spectrum as extra "measurements".
        let data_end = c
            .pos()
            .saturating_add(npts.saturating_mul(size_of::<f32>()));
        if header.flogoff != 0 && data_end > header.flogoff as usize {
            return Err(SpcError::DataOverrunsLogBlock {
                data_end,
                log_offset: header.flogoff,
            });
        }

        // Reject an absurd count before allocating for it.
        if npts.saturating_mul(size_of::<f32>()) > c.remaining() {
            return Err(SpcError::TooShort {
                context: "the y values",
                needed: npts * size_of::<f32>(),
                available: c.remaining(),
            });
        }

        let mut y = Vec::with_capacity(npts);
        for _ in 0..npts {
            y.push(f64::from(c.f32("the y values")?));
        }
        let x = generate_x(header.ffirst, header.flast, npts);

        let log = LogBlock::parse_at(&mut c, header.flogoff)?;

        Ok(Self {
            header,
            subfiles: vec![Subfile { subheader, x, y }],
            log,
        })
    }

    /// x values of the first subfile.
    pub fn x(&self) -> &[f64] {
        self.subfiles.first().map_or(&[], |s| &s.x)
    }

    /// y values of the first subfile.
    pub fn y(&self) -> &[f64] {
        self.subfiles.first().map_or(&[], |s| &s.y)
    }

    /// Label for the x axis, honouring custom labels when the file sets them.
    pub fn x_label(&self) -> String {
        self.header.x_label()
    }

    /// Label for the y axis, honouring custom labels when the file sets them.
    pub fn y_label(&self) -> String {
        self.header.y_label()
    }
}

/// Builds an evenly spaced x axis with exact end points.
///
/// The last value is assigned directly rather than computed, so that it equals
/// `last` bit for bit instead of drifting by an ulp or two.
fn generate_x(first: f64, last: f64, npts: usize) -> Vec<f64> {
    if npts == 0 {
        return Vec::new();
    }
    if npts == 1 {
        return vec![first];
    }
    let divisor = (npts - 1) as f64;
    let mut x = Vec::with_capacity(npts);
    for i in 0..npts - 1 {
        x.push(first + (last - first) * (i as f64 / divisor));
    }
    x.push(last);
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_axis_hits_both_end_points_exactly() {
        let x = generate_x(900.0, 1700.0, 801);
        assert_eq!(x.len(), 801);
        assert_eq!(x[0], 900.0);
        assert_eq!(x[800], 1700.0);
        assert_eq!(x[1], 901.0);
    }

    #[test]
    fn a_single_point_does_not_divide_by_zero() {
        assert_eq!(generate_x(500.0, 500.0, 1), vec![500.0]);
        assert!(generate_x(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn a_descending_axis_works_too() {
        // Wavenumber axes commonly run from high to low.
        let x = generate_x(4000.0, 400.0, 4);
        assert_eq!(x, vec![4000.0, 2800.0, 1600.0, 400.0]);
    }
}
