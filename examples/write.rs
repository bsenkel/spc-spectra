//! Writes a synthetic NIR spectrum to an SPC file.
//!
//! ```text
//! cargo run --example write -- spectrum.spc
//! cargo run --example dump  -- spectrum.spc
//! ```
//!
//! The pair is the round trip in the small: whatever this writes, `dump` reads.
//! The spectrum itself is a Gaussian band on a sloping baseline — shaped like
//! an absorbance measurement so a viewer shows something recognisable, and not
//! a measurement of anything.

use spc_spectra::{SpcBuilder, SpcDate, Technique, XType, YType};

const FIRST_NM: f64 = 900.0;
const LAST_NM: f64 = 1700.0;
const NPTS: usize = 801;

fn main() -> std::process::ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: write <file.spc>");
        return std::process::ExitCode::FAILURE;
    };

    let spc = match SpcBuilder::new(FIRST_NM, LAST_NM, absorbance())
        .x_type(XType::Nanometers)
        .y_type(YType::Absorbance)
        .technique(Technique::Nir)
        .source("example")
        .resolution("2nm")
        .scans(32)
        .comment("SYNTHETIC spectrum written by the spc-spectra example")
        .date(SpcDate {
            year: 2026,
            month: 8,
            day: 26,
            hour: 9,
            minute: 15,
        })
        // The scan count has a field of its own (subscan); only what the
        // format has no place for goes into the log text.
        .log_text("Channel=1\nIntegration=100ms")
        .build()
    {
        Ok(spc) => spc,
        Err(e) => {
            eprintln!("could not build the spectrum: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    if let Err(e) = spc.to_path(&path) {
        eprintln!("{path}: {e}");
        return std::process::ExitCode::FAILURE;
    }

    println!("wrote {path}: {NPTS} points, {FIRST_NM} .. {LAST_NM} nm");
    std::process::ExitCode::SUCCESS
}

/// A single O-H overtone band at 1450 nm on a scattering baseline.
fn absorbance() -> Vec<f64> {
    let step = (LAST_NM - FIRST_NM) / (NPTS - 1) as f64;
    (0..NPTS)
        .map(|i| {
            let nm = FIRST_NM + i as f64 * step;
            let baseline = 0.25 + 0.000_12 * (nm - FIRST_NM);
            let d = (nm - 1450.0) / 40.0;
            baseline + 0.6 * (-0.5 * d * d).exp()
        })
        .collect()
}
