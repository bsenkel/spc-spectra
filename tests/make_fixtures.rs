//! Development tool: writes example SPC files for eyeballing in a viewer.
//!
//! These are not assertions, which is why they are all `#[ignore]`d. Run them
//! explicitly:
//!
//! ```text
//! cargo test --test make_fixtures -- --ignored
//! ```
//!
//! Files land in the system temp directory, or in `SPC_FIXTURE_DIR` if set.
//! Each test prints the path it wrote.
//!
//! # These spectra are synthetic
//!
//! The "paper" fixtures below are **built from published band positions, not
//! measured**. They are shaped like a cellulose spectrum so that a viewer shows
//! something recognisable instead of a straight ramp — nothing more. They are
//! not reference data and must never be used to validate anything about the
//! sample they are named after. Only the file *structure* is authoritative
//! here, and that is what this crate parses.

mod common;

use common::SpcBuilder;
use std::path::PathBuf;

/// Where fixtures are written. Override with `SPC_FIXTURE_DIR`.
fn out_dir() -> PathBuf {
    std::env::var_os("SPC_FIXTURE_DIR").map_or_else(std::env::temp_dir, PathBuf::from)
}

fn write(name: &str, bytes: Vec<u8>) {
    let path = out_dir().join(name);
    std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("could not write {path:?}: {e}"));
    println!(
        "wrote {} ({} bytes)",
        path.display(),
        std::fs::metadata(&path).unwrap().len()
    );
}

/// The plain ramp: easiest possible file to reason about when a value looks off.
#[test]
#[ignore = "developer tool"]
fn ramp() {
    write("spc-demo-ramp.spc", SpcBuilder::new().build());
}

/// A cellulose-shaped absorbance spectrum over the 900-1700 nm NIR range.
#[test]
#[ignore = "developer tool"]
fn paper_absorbance() {
    let y = paper_absorbance_curve();
    let spc = SpcBuilder::new()
        .y(y)
        .range(FIRST_NM, LAST_NM)
        .axis_types(3, 2) // nanometers / absorbance
        .source("NIR probe")
        .comment("SYNTHETIC cellulose-shaped spectrum - not a measurement")
        .date(2026, 7, 21, 15, 4)
        // The scan count belongs in subscan, the field the format has for it —
        // not in the log text, where only software sharing the key=value
        // convention would ever find it.
        .subscan(32)
        .log_text(
            "Sample=paper, synthetic\n\
             Origin=generated from published band positions\n\
             Channel=1\n\
             Integration=100ms\n\
             Reference=internal white\n",
        )
        .build();
    write("spc-paper-absorbance.spc", spc);
}

/// The same spectrum expressed as transmittance, `T = 100 * 10^-A`.
///
/// Useful for checking that the y axis label follows `fytype` rather than being
/// assumed, and that a descending-looking curve plots sensibly.
#[test]
#[ignore = "developer tool"]
fn paper_transmittance() {
    let y = paper_absorbance_curve()
        .into_iter()
        .map(|a| 100.0 * 10f32.powf(-a))
        .collect::<Vec<f32>>();
    let spc = SpcBuilder::new()
        .y(y)
        .range(FIRST_NM, LAST_NM)
        .axis_types(3, 128) // nanometers / transmission
        .source("NIR probe")
        .comment("SYNTHETIC cellulose-shaped spectrum - not a measurement")
        .date(2026, 7, 21, 15, 4)
        .subscan(32) // the same acquisition, expressed differently
        .log_text("Sample=paper, synthetic\nUnit=percent transmittance\nChannel=1\n")
        .build();
    write("spc-paper-transmittance.spc", spc);
}

const FIRST_NM: f64 = 900.0;
const LAST_NM: f64 = 1700.0;
const NPTS: usize = 801;

/// Absorption bands of cellulose and its bound water in the short-wave NIR.
///
/// `(centre nm, peak absorbance, gaussian sigma nm, what it is)`. The positions
/// follow the overtone assignments commonly quoted for cellulosic material; the
/// amplitudes are chosen to look right, not to match any particular paper.
const BANDS: &[(f32, f32, f32, &str)] = &[
    (970.0, 0.05, 26.0, "water O-H second overtone"),
    (1160.0, 0.06, 30.0, "C-H second overtone"),
    (1215.0, 0.10, 30.0, "C-H second overtone, cellulose"),
    (1360.0, 0.13, 26.0, "C-H combination"),
    (
        1430.0,
        0.52,
        28.0,
        "O-H first overtone, crystalline cellulose",
    ),
    (
        1490.0,
        0.68,
        42.0,
        "O-H first overtone, amorphous cellulose and water",
    ),
    (1580.0, 0.30, 36.0, "O-H / C-H"),
    (1670.0, 0.20, 30.0, "C-H first overtone"),
];

/// Builds the absorbance curve: a scattering baseline plus Gaussian bands.
///
/// Paper is a strong diffuse scatterer, hence the sloping offset — in real
/// log(1/R) data that slope usually dwarfs the chemistry, and any preprocessing
/// step (SNV, detrend) exists mainly to remove it.
fn paper_absorbance_curve() -> Vec<f32> {
    let step = (LAST_NM - FIRST_NM) as f32 / (NPTS - 1) as f32;
    (0..NPTS)
        .map(|i| {
            let nm = FIRST_NM as f32 + i as f32 * step;
            let baseline = 0.25 + 0.000_12 * (nm - FIRST_NM as f32);
            let bands: f32 = BANDS
                .iter()
                .map(|&(centre, amp, sigma, _)| {
                    let d = (nm - centre) / sigma;
                    amp * (-0.5 * d * d).exp()
                })
                .sum();
            // Detector noise, deterministic so the file is byte-reproducible.
            baseline + bands + 0.000_6 * noise(i)
        })
        .collect()
}

/// A cheap deterministic hash in `-1.0 ..= 1.0`, standing in for detector noise.
///
/// Deliberately not `rand`: this crate has no dependencies and a fixture that
/// changes on every run is a nuisance to compare against.
fn noise(i: usize) -> f32 {
    let mut h = (i as u32).wrapping_mul(2_654_435_761);
    h ^= h >> 15;
    h = h.wrapping_mul(2_246_822_519);
    h ^= h >> 13;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[test]
fn the_synthetic_curve_stays_physically_plausible() {
    // Not a format test - a guard so a careless edit to the band table cannot
    // silently produce negative absorbance or a transmittance above 100 %.
    let a = paper_absorbance_curve();
    assert_eq!(a.len(), NPTS);
    assert!(a.iter().all(|&v| v > 0.0), "absorbance must stay positive");
    assert!(
        a.iter().all(|&v| v < 3.0),
        "absorbance must stay in a sane range"
    );

    let peak = a.iter().copied().fold(f32::MIN, f32::max);
    let peak_idx = a.iter().position(|&v| v == peak).unwrap();
    let peak_nm = FIRST_NM + peak_idx as f64;
    assert!(
        (1400.0..=1550.0).contains(&peak_nm),
        "the O-H overtone should dominate, but the maximum sits at {peak_nm} nm"
    );

    let t: Vec<f32> = a.iter().map(|&v| 100.0 * 10f32.powf(-v)).collect();
    assert!(
        t.iter().all(|&v| v > 0.0 && v <= 100.0),
        "transmittance must stay in 0..100 %"
    );
}
