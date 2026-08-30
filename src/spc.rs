//! The parsed file and its subfiles.

use crate::bytes::Cursor;
use crate::error::SpcError;
use crate::header::{FEXP_IEEE_FLOAT, Header, fixed_point_scale};
use crate::log::LogBlock;
use crate::subheader::SubHeader;
use crate::write::Sink;
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
/// println!("{} points from {} to {}", spc.subfiles[0].y.len(), spc.header.ffirst, spc.header.flast);
/// # Ok::<(), spc_spectra::SpcError>(())
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Spc {
    /// The 512 byte main header.
    pub header: Header,
    /// The subfiles, in file order. A single-spectrum file has exactly one.
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

        // `fnsub` is a count out of the file, so it is bounded before it turns
        // into an allocation. A subfile cannot cost less than its subheader,
        // which makes that a floor from the format rather than a guess.
        let nsub = header.fnsub as usize;
        let least = nsub.saturating_mul(SubHeader::SIZE);
        if least > c.remaining() {
            return Err(SpcError::TooShort {
                context: "the subfiles",
                needed: least,
                available: c.remaining(),
            });
        }

        let mut subfiles = Vec::with_capacity(nsub);
        for _ in 0..nsub {
            let subheader = SubHeader::parse(&mut c)?;
            subheader.validate(&header)?;

            let npts = subheader.npts(&header);
            if npts == 0 {
                return Err(SpcError::InvalidPointCount(npts));
            }
            let npts = npts as usize;

            // The log block sits after the data, so `flogoff` is also a
            // statement about where the y values end. Cross-checking the two
            // catches a corrupted point count that would otherwise silently
            // pull log block bytes into the spectrum as extra "measurements".
            // Applied per subfile: every one of them ends before the block.
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
            let needed = npts.saturating_mul(size_of::<f32>());
            if needed > c.remaining() {
                return Err(SpcError::TooShort {
                    context: "the y values",
                    needed,
                    available: c.remaining(),
                });
            }

            // How those four bytes per point are to be read is the one thing
            // `fexp` and `subexp` say; which of the two speaks is
            // `effective_exponent`. Either way it is four bytes, so nothing
            // above this line depends on the answer.
            let exp = subheader.effective_exponent(&header);
            let mut y = Vec::with_capacity(npts);
            if exp == FEXP_IEEE_FLOAT {
                for _ in 0..npts {
                    y.push(f64::from(c.f32("the y values")?));
                }
            } else {
                let scale = fixed_point_scale(exp);
                for _ in 0..npts {
                    y.push(f64::from(c.i32("the y values")?) * scale);
                }
            }
            let x = generate_x(header.ffirst, header.flast, npts);

            subfiles.push(Subfile { subheader, x, y });
        }

        let log = LogBlock::parse_at(&mut c, header.flogoff)?;

        Ok(Self {
            header,
            subfiles,
            log,
        })
    }

    /// x values of the first subfile.
    #[deprecated(
        since = "0.3.0",
        note = "reads the first subfile only, which silently hides the rest of a multifile record; use the subfiles field, whose every entry has its own x"
    )]
    pub fn x(&self) -> &[f64] {
        self.subfiles.first().map_or(&[], |s| &s.x)
    }

    /// y values of the first subfile.
    #[deprecated(
        since = "0.3.0",
        note = "reads the first subfile only, which silently hides the rest of a multifile record; use the subfiles field, whose every entry has its own y"
    )]
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

    /// Serialises the file back into SPC bytes.
    ///
    /// Everything this crate can write, it can read back: the same checks that
    /// guard [`Spc::from_bytes`] run here first, so a successful call cannot
    /// produce a file that [`Spc::from_bytes`] would reject.
    ///
    /// # What is preserved, and what is not
    ///
    /// Every field this crate parses survives a read/write round trip, and a
    /// file written here is byte-stable: writing it, reading it back and
    /// writing it again yields identical bytes. Byte-for-byte fidelity to a
    /// *foreign* file is not promised, because the reader does not model
    /// everything a file may contain:
    ///
    /// - the reserved tails of the main header and the subheader, and the
    ///   `logdsks` area of the log block, are written as nulls;
    /// - log entries separated by nulls come back separated by newlines, since
    ///   that is how [`LogBlock::text`] presents them;
    /// - y values pass back through the file's own encoding, so a float file
    ///   narrows the [`f64`] values to 32 bits exactly as reading widened them,
    ///   and a fixed-point one rounds them to its scale.
    ///
    /// The header's text fields are *not* on that list: they are kept as the
    /// bytes the file held, so whatever was in them comes back unchanged. See
    /// [`TextField`](crate::TextField).
    ///
    /// # Errors
    ///
    /// Besides the variants [`Spc::from_bytes`] can return, this reports
    /// [`SpcError::ValueNotRepresentable`] for a y value the file's encoding
    /// cannot carry, and [`SpcError::NotWritable`] when the parts contradict
    /// each other — a point count that disagrees with the y values, or an x
    /// axis that is not the evenly spaced one `ffirst`/`flast` describe.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let spc = spc_spectra::Spc::from_path("in.spc")?;
    /// std::fs::write("out.spc", spc.to_bytes()?)?;
    /// # Ok::<(), spc_spectra::SpcError>(())
    /// ```
    pub fn to_bytes(&self) -> Result<Vec<u8>, SpcError> {
        let subfiles = self.writable_subfiles()?;

        // Where the y values end, which is also where the log block goes.
        let data_end = Header::SIZE
            + subfiles
                .iter()
                .map(|sub| SubHeader::SIZE + sub.y.len() * size_of::<f32>())
                .sum::<usize>();
        let flogoff = if self.log.is_some() {
            data_end as u32
        } else {
            0
        };

        let mut s = Sink::with_capacity(data_end);
        self.header.write(&mut s, flogoff);
        for subfile in subfiles {
            subfile.subheader.write(&mut s);
            let exp = subfile.subheader.effective_exponent(&self.header);
            if exp == FEXP_IEEE_FLOAT {
                for &v in &subfile.y {
                    s.f32(v as f32);
                }
            } else {
                let scale = fixed_point_scale(exp);
                for &v in &subfile.y {
                    // `validate_subfile` has already established that this
                    // lands inside i32, so the cast cannot saturate here.
                    s.i32((v / scale).round() as i32);
                }
            }
        }
        debug_assert!(flogoff == 0 || s.pos() == flogoff as usize);
        if let Some(log) = &self.log {
            log.write(&mut s);
        }

        Ok(s.finish())
    }

    /// Serialises the file and writes it to disk.
    ///
    /// The counterpart to [`Spc::from_path`]. See [`Spc::to_bytes`] for what a
    /// round trip preserves and what it does not.
    pub fn to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), SpcError> {
        std::fs::write(path, self.to_bytes()?)?;
        Ok(())
    }

    /// Checks that this file can be written as a readable SPC file, and returns
    /// the subfiles to write.
    ///
    /// The header and subheader checks are the *reader's* own
    /// ([`Header::validate`], [`SubHeader::validate`]), which is what makes
    /// "written by this crate" and "readable by this crate" the same set. The
    /// rest are contradictions that only a hand-assembled [`Spc`] can have: the
    /// reader cannot produce them, but it must still be impossible to write one
    /// out and discover it later.
    pub(crate) fn writable_subfiles(&self) -> Result<&[Subfile], SpcError> {
        self.header.validate()?;

        if self.subfiles.is_empty() {
            return Err(SpcError::NotWritable {
                detail: "there is no subfile to write",
            });
        }
        // `fnsub` is the count a reader loops over, so it has to be the number
        // of subfiles actually written. The TMULTI flag is deliberately not
        // checked against it: the flag is an observation the file carried, and
        // one that contradicts its own count still has to come back unchanged.
        if u32::try_from(self.subfiles.len()) != Ok(self.header.fnsub) {
            return Err(SpcError::NotWritable {
                detail: "fnsub does not match the number of subfiles",
            });
        }

        for subfile in &self.subfiles {
            self.validate_subfile(subfile)?;
        }
        Ok(&self.subfiles)
    }

    /// The per-subfile half of [`Self::writable_subfiles`].
    fn validate_subfile(&self, subfile: &Subfile) -> Result<(), SpcError> {
        subfile.subheader.validate(&self.header)?;

        let npts = subfile.y.len();
        if npts == 0 {
            return Err(SpcError::NotWritable {
                detail: "the spectrum has no points",
            });
        }
        if u32::try_from(npts).is_err() {
            return Err(SpcError::NotWritable {
                detail: "more points than the format's 32-bit count can hold",
            });
        }
        if subfile.subheader.npts(&self.header) as usize != npts {
            return Err(SpcError::NotWritable {
                detail: "the declared point count does not match the number of y values",
            });
        }
        if subfile.x.len() != npts {
            return Err(SpcError::NotWritable {
                detail: "the x and y axes have different lengths",
            });
        }

        // The x axis is never stored: a reader regenerates it from ffirst and
        // flast. An x that is not that axis would therefore be silently
        // replaced by it, which is precisely the kind of quiet substitution
        // this crate refuses to make.
        let expected = generate_x(self.header.ffirst, self.header.flast, npts);
        if !subfile
            .x
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.to_bits() == b.to_bits())
        {
            return Err(SpcError::NotWritable {
                detail: "the x axis is not the evenly spaced one ffirst and flast describe",
            });
        }

        // Losing precision is inherent to the format and is accepted, whether
        // that is narrowing to f32 or rounding to the nearest step of a
        // fixed-point scale. What is refused is a value that would not come
        // back at all.
        let exp = subfile.subheader.effective_exponent(&self.header);
        if exp == FEXP_IEEE_FLOAT {
            for (index, &value) in subfile.y.iter().enumerate() {
                if value.is_finite() && !(value as f32).is_finite() {
                    return Err(SpcError::ValueNotRepresentable { index, value });
                }
            }
        } else {
            let scale = fixed_point_scale(exp);
            for (index, &value) in subfile.y.iter().enumerate() {
                let raw = (value / scale).round();
                // An `as` cast saturates silently, which would store a
                // different number than the caller asked for without saying so.
                let out_of_range =
                    !raw.is_finite() || raw < f64::from(i32::MIN) || raw > f64::from(i32::MAX);
                // Fixed-point precision is absolute rather than relative: a
                // value below half a step does not round, it disappears. That
                // is a different loss from the rounding above and is refused.
                let collapsed = raw == 0.0 && value != 0.0;
                if out_of_range || collapsed {
                    return Err(SpcError::ValueNotRepresentable { index, value });
                }
            }
        }

        Ok(())
    }
}

/// Builds an evenly spaced x axis with exact end points.
///
/// The last value is assigned directly rather than computed, so that it equals
/// `last` bit for bit instead of drifting by an ulp or two.
pub(crate) fn generate_x(first: f64, last: f64, npts: usize) -> Vec<f64> {
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
