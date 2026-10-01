# spc-spectra

[![CI](https://github.com/bsenkel/spc-spectra/actions/workflows/ci.yml/badge.svg)](https://github.com/bsenkel/spc-spectra/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/spc-spectra.svg)](https://crates.io/crates/spc-spectra)
[![docs.rs](https://img.shields.io/docsrs/spc-spectra)](https://docs.rs/spc-spectra)

Rust reader and writer for SPC spectroscopy files, the binary format of
Galactic Industries and Thermo's GRAMS software that FT-IR, Raman, NIR, UV-VIS,
NMR and MS instruments export. No dependencies, no `unsafe`:

- The new format (`fversn = 0x4B`, little-endian), holding one spectrum or a
  series (`TMULTI`) on one evenly spaced x axis
- 32-bit y values, as IEEE floats or Galactic fixed-point integers
- The log block, as raw binary and text
- Reading from and writing to bytes, so the data need not come from a file
- A specific error for unsupported variants and for files that contradict
  themselves, never a guess

Readers for this format exist in Python, R, JavaScript and Julia. This crate
fills the gap in Rust.

## Installation

```sh
cargo add spc-spectra
```

## Examples

Writing a spectrum:

```rust
use spc_spectra::{SpcBuilder, Technique, XType, YType};

fn main() -> Result<(), spc_spectra::SpcError> {
    // A stand-in for a measurement: one absorbance value per nm, 900 to 1700.
    let absorbance: Vec<f64> = (0..801).map(|i| 0.1 + f64::from(i) * 0.001).collect();

    // An SPC file does not store the x axis; readers regenerate it from the
    // two end points. The builder therefore takes a range, not x values.
    SpcBuilder::new(900.0, 1700.0, absorbance)
        .x_type(XType::Nanometers)
        .y_type(YType::Absorbance)
        .technique(Technique::Nir)
        .source("NIR probe")
        .scans(32)
        .log_text("Integration time=100 ms\nDetector=InGaAs")
        .build()?
        .to_path("spectrum.spc")?;
    Ok(())
}
```

Reading it back:

```rust
use spc_spectra::Spc;

fn main() -> Result<(), spc_spectra::SpcError> {
    let spc = Spc::from_path("spectrum.spc")?;

    // Header fields carry the names the format gives them: fexper is the
    // technique, fsource the instrument, ffirst and flast the x axis ends.
    let header = &spc.header;
    println!("{} ({})", header.fexper, header.fsource);
    println!("{} .. {} {}", header.ffirst, header.flast, spc.x_label());

    // One subfile per spectrum: a single measurement has one, a series many.
    for sub in &spc.subfiles {
        println!("{} points", sub.y.len());
        for (x, y) in sub.points().take(3) {
            println!("{x:8.1}  {y:.3}");
        }
    }

    // The log block holds what the format has no field for, under whatever
    // keys the acquiring software chose.
    if let Some(log) = &spc.log {
        for (key, value) in log.entries() {
            println!("{key}: {value}");
        }
    }
    Ok(())
}
```

```text
NIR (NIR probe)
900 .. 1700 Nanometers (nm)
801 points
   900.0  0.100
   901.0  0.101
   902.0  0.102
Integration time: 100 ms
Detector: InGaAs
```

`Spc::from_bytes` and `to_bytes` do the same without a file, and
`SpcBuilder::series` writes several spectra that share one x axis.

`cargo run --example write -- spectrum.spc` and
`cargo run --example dump -- spectrum.spc` are the same round trip on the
command line, and the [API documentation](https://docs.rs/spc-spectra)
describes the byte layout and every field.

## Conventions

- Not supported yet: big-endian files (`0x4C`), the old format (`0x4D`),
  per-subfile x axes (`TXYXYS`), explicit x values (`TXVALS`), 16-bit y values
  (`TSPREC`) and multi-plane data cubes (`fwplanes > 1`). Each is refused with
  its own `Unsupported` variant; a file that needs one is worth an issue.
- The writer runs the reader's validation, so a file this crate writes is one
  it can read back, and a variant it refuses to read it refuses to write.
- Every parsed field survives a round trip, and a file this crate wrote is
  byte-stable. A foreign file is not reproduced byte for byte: reserved areas
  are written as nulls and the log text is normalized, as `Spc::to_bytes`
  documents.
- Header text fields keep the bytes the file held. `text()` decodes them as
  UTF-8 and does not guess a code page; text in Windows-1252 has to be decoded
  from `as_bytes()`.

## Validation

The tests assemble their SPC files byte by byte in memory and check three
things:

- The writer produces exactly those hand-assembled bytes. A round trip alone
  would not notice a mistake that reader and writer share.
- The reader never panics: tested with every single-bit flip in the header and
  subheader, and with 100 000 random mutations.
- Every mutated file the reader still accepts can be written and reads back
  unchanged.

Instrument files cannot live in this repository. To check against your own:

```sh
SPC_SAMPLE_DIR=/path/to/spc/files cargo test --test real_files
```

## Minimum supported Rust version

Rust 1.85. Tested on Linux, macOS and Windows, in debug and release.

## Related work

- [`spc`](https://github.com/rohanisaac/spc) and
  [`spc-spectra`](https://pypi.org/project/spc-spectra/) — Python
- [`hyperSpec`](https://github.com/r-hyperspec/hyperSpec) /
  [`hySpc.read.spc`](https://github.com/r-hyperspec/hySpc.read.spc) — R
- [`spc-parser`](https://github.com/cheminfo/spc-parser) — JavaScript
- [`SPCSpectra.jl`](https://github.com/hhaensel/SPCSpectra.jl) — Julia

## License

[MIT](LICENSE)

## Trademarks

This project is not affiliated with, endorsed by, or sponsored by Thermo Fisher
Scientific. "Thermo", "Galactic" and "GRAMS" are trademarks of their respective
owners and are used only to describe which files this crate reads and writes.
The format specification is copyrighted and not included in this repository;
this implementation describes the format in its own words.
