//! Builds synthetic SPC files in memory so the parser can be tested without
//! shipping any third-party sample data.
//!
//! The defaults mirror a real single-spectrum NIR export: 801 points from
//! 900 to 1700 nm, one subfile with `subnpts = 0`, IEEE float y values and a
//! log block of plain `key=value` lines. That puts the log block at byte
//! 512 + 32 + 801 * 4 = 3748, the same geometry a real instrument produces.

#![allow(dead_code)] // Not every test uses every knob.

use spc_spectra::{Header, SpcDate, SubHeader, TFlags};

pub const DEFAULT_NPTS: u32 = 801;
pub const DEFAULT_FIRST: f64 = 900.0;
pub const DEFAULT_LAST: f64 = 1700.0;

/// Fluent builder for a byte-exact SPC file.
pub struct SpcBuilder {
    ftflgs: u8,
    fversn: u8,
    fexper: u8,
    fexp: i8,
    fnpts: Option<u32>,
    ffirst: f64,
    flast: f64,
    fnsub: u32,
    fwplanes: u32,
    fxtype: u8,
    fytype: u8,
    fdate: u32,
    fres: String,
    fsource: String,
    fcmnt: String,
    fcatxt: Vec<u8>,
    subnpts: u32,
    /// Written into the subheader; defaults to whatever `fexp` is.
    subexp: Option<i8>,
    y: Vec<f32>,
    log_text: Option<String>,
    log_binary: Vec<u8>,
}

impl Default for SpcBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SpcBuilder {
    /// A valid, fully supported file: 0x4B, one subfile, float y values.
    pub fn new() -> Self {
        let y = (0..DEFAULT_NPTS).map(|i| 0.1 + i as f32 * 0.001).collect();
        Self {
            ftflgs: 0,
            fversn: 0x4B,
            fexper: 5,  // NIR
            fexp: -128, // 0x80: IEEE floats
            fnpts: None,
            ffirst: DEFAULT_FIRST,
            flast: DEFAULT_LAST,
            fnsub: 1,
            fwplanes: 0,
            fxtype: 3, // nanometers
            fytype: 2, // absorbance
            fdate: SpcDate {
                year: 2026,
                month: 7,
                day: 21,
                hour: 14,
                minute: 37,
            }
            .to_packed(),
            fres: "2nm".into(),
            fsource: "SentroNIR".into(),
            fcmnt: "synthetic test spectrum".into(),
            fcatxt: Vec::new(),
            subnpts: 0, // inherit fnpts
            subexp: None,
            y,
            log_text: Some("Channel=1\nIntegration=100ms\n".into()),
            log_binary: Vec::new(),
        }
    }

    pub fn ftflgs(mut self, v: u8) -> Self {
        self.ftflgs = v;
        self
    }

    pub fn fversn(mut self, v: u8) -> Self {
        self.fversn = v;
        self
    }

    pub fn fexper(mut self, v: u8) -> Self {
        self.fexper = v;
        self
    }

    pub fn fexp(mut self, v: i8) -> Self {
        self.fexp = v;
        self
    }

    /// Overrides `fnpts` independently of how many y values are written.
    pub fn fnpts(mut self, v: u32) -> Self {
        self.fnpts = Some(v);
        self
    }

    pub fn fnsub(mut self, v: u32) -> Self {
        self.fnsub = v;
        self
    }

    pub fn fwplanes(mut self, v: u32) -> Self {
        self.fwplanes = v;
        self
    }

    pub fn range(mut self, first: f64, last: f64) -> Self {
        self.ffirst = first;
        self.flast = last;
        self
    }

    pub fn axis_types(mut self, x: u8, y: u8) -> Self {
        self.fxtype = x;
        self.fytype = y;
        self
    }

    pub fn fdate(mut self, packed: u32) -> Self {
        self.fdate = packed;
        self
    }

    pub fn date(self, year: u16, month: u8, day: u8, hour: u8, minute: u8) -> Self {
        self.fdate(
            SpcDate {
                year,
                month,
                day,
                hour,
                minute,
            }
            .to_packed(),
        )
    }

    pub fn comment(mut self, s: &str) -> Self {
        self.fcmnt = s.into();
        self
    }

    pub fn source(mut self, s: &str) -> Self {
        self.fsource = s.into();
        self
    }

    /// Sets the null-separated custom axis label field and the `TALABS` flag.
    pub fn custom_axis_labels(mut self, labels: &[&str]) -> Self {
        let mut raw = Vec::new();
        for label in labels {
            raw.extend_from_slice(label.as_bytes());
            raw.push(0);
        }
        raw.resize(30, 0);
        self.fcatxt = raw;
        self.ftflgs |= TFlags::TALABS;
        self
    }

    /// `0` keeps the "inherit from fnpts" shorthand.
    pub fn subnpts(mut self, v: u32) -> Self {
        self.subnpts = v;
        self
    }

    /// Sets the subheader's own exponent independently of the header's `fexp`.
    ///
    /// Without this the two always agree, which is exactly the case that hides
    /// a missing cross-check between them.
    pub fn subexp(mut self, v: i8) -> Self {
        self.subexp = Some(v);
        self
    }

    pub fn y(mut self, values: Vec<f32>) -> Self {
        self.y = values;
        self
    }

    pub fn log_text(mut self, s: &str) -> Self {
        self.log_text = Some(s.into());
        self
    }

    pub fn log_binary(mut self, b: Vec<u8>) -> Self {
        self.log_binary = b;
        self
    }

    pub fn no_log(mut self) -> Self {
        self.log_text = None;
        self.log_binary.clear();
        self
    }

    /// Byte offset at which the log block will be written.
    pub fn log_offset(&self) -> u32 {
        (Header::SIZE + SubHeader::SIZE + self.y.len() * 4) as u32
    }

    /// Serialises everything into a complete SPC file.
    pub fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let has_log = self.log_text.is_some() || !self.log_binary.is_empty();
        let flogoff = if has_log { self.log_offset() } else { 0 };
        let fnpts = self.fnpts.unwrap_or(self.y.len() as u32);

        // --- main header, 512 bytes ---
        out.push(self.ftflgs);
        out.push(self.fversn);
        out.push(self.fexper);
        out.push(self.fexp as u8);
        out.extend_from_slice(&fnpts.to_le_bytes());
        out.extend_from_slice(&self.ffirst.to_le_bytes());
        out.extend_from_slice(&self.flast.to_le_bytes());
        out.extend_from_slice(&self.fnsub.to_le_bytes());
        out.push(self.fxtype);
        out.push(self.fytype);
        out.push(0); // fztype
        out.push(0); // fpost
        out.extend_from_slice(&self.fdate.to_le_bytes());
        push_text(&mut out, &self.fres, 9);
        push_text(&mut out, &self.fsource, 9);
        out.extend_from_slice(&0u16.to_le_bytes()); // fpeakpt
        out.extend_from_slice(&[0u8; 32]); // fspare[8]
        push_text(&mut out, &self.fcmnt, 130);
        let mut catxt = self.fcatxt.clone();
        catxt.resize(30, 0);
        out.extend_from_slice(&catxt);
        out.extend_from_slice(&flogoff.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // fmods
        out.push(0); // fprocs
        out.push(0); // flevel
        out.extend_from_slice(&0u16.to_le_bytes()); // fsampin
        out.extend_from_slice(&1.0f32.to_le_bytes()); // ffactor
        push_text(&mut out, "", 48); // fmethod
        out.extend_from_slice(&0.0f32.to_le_bytes()); // fzinc
        out.extend_from_slice(&self.fwplanes.to_le_bytes()); // fwplanes
        out.extend_from_slice(&0.0f32.to_le_bytes()); // fwinc
        out.push(0); // fwtype
        out.resize(Header::SIZE, 0); // reserved tail
        assert_eq!(
            out.len(),
            Header::SIZE,
            "main header must be exactly 512 bytes"
        );

        // --- subheader, 32 bytes ---
        let sub_start = out.len();
        out.push(0); // subflgs
        out.push(self.subexp.unwrap_or(self.fexp) as u8); // subexp
        out.extend_from_slice(&0u16.to_le_bytes()); // subindx
        out.extend_from_slice(&0.0f32.to_le_bytes()); // subtime
        out.extend_from_slice(&0.0f32.to_le_bytes()); // subnext
        out.extend_from_slice(&0.0f32.to_le_bytes()); // subnois
        out.extend_from_slice(&self.subnpts.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes()); // subscan
        out.extend_from_slice(&0.0f32.to_le_bytes()); // subwlevel
        out.resize(sub_start + SubHeader::SIZE, 0);

        // --- y values ---
        for v in &self.y {
            out.extend_from_slice(&v.to_le_bytes());
        }

        // --- log block ---
        if has_log {
            assert_eq!(out.len() as u32, flogoff, "log block must land on flogoff");
            let logtxto = (spc_spectra::LogBlock::HEADER_SIZE + self.log_binary.len()) as u32;
            let text = self.log_text.clone().unwrap_or_default();
            let logsizd = logtxto + text.len() as u32 + 1;
            out.extend_from_slice(&logsizd.to_le_bytes());
            out.extend_from_slice(&logsizd.to_le_bytes()); // logsizm
            out.extend_from_slice(&logtxto.to_le_bytes());
            out.extend_from_slice(&(self.log_binary.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // logdsks
            out.resize(flogoff as usize + spc_spectra::LogBlock::HEADER_SIZE, 0);
            out.extend_from_slice(&self.log_binary);
            out.extend_from_slice(text.as_bytes());
            out.push(0);
        }

        out
    }
}

/// Writes a null-padded fixed-width text field.
fn push_text(out: &mut Vec<u8>, s: &str, width: usize) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(width);
    out.extend_from_slice(&bytes[..n]);
    out.resize(out.len() + (width - n), 0);
}
