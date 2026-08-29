# spc-spectra

[![CI](https://github.com/bsenkel/spc-spectra/actions/workflows/ci.yml/badge.svg)](https://github.com/bsenkel/spc-spectra/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/spc-spectra.svg)](https://crates.io/crates/spc-spectra)
[![docs.rs](https://img.shields.io/docsrs/spc-spectra)](https://docs.rs/spc-spectra)
[![License](https://img.shields.io/crates/l/spc-spectra.svg)](LICENSE-MIT)

A Rust reader and writer for **SPC spectroscopy files** — the binary format
introduced by Galactic Industries and carried on in Thermo's GRAMS software. It
is still the everyday interchange format for FT-IR, Raman, NIR, UV-VIS, NMR and
MS data.

Readers for this format exist in Python, R, JavaScript and Julia. This crate
fills the gap in Rust.

- **No dependencies.** Standard library only.
- **No `unsafe`.** Enforced with `#![forbid(unsafe_code)]`.
- **Refuses rather than guesses.** Format variants that are not decoded yet are
  reported as a specific error, never parsed on a hunch — and never written on
  one either, since the writer runs the reader's checks. The same applies to a
  file that contradicts itself: where the format states an invariant, it is
  checked rather than assumed.

```toml
[dependencies]
spc-spectra = "0.3"
```

## Usage

```rust
use spc_spectra::Spc;

let spc = Spc::from_path("spectrum.spc")?;

println!("{} ({})", spc.header.fexper, spc.header.fsource);
println!("{} points, {} .. {} {}",
         spc.subfiles[0].y.len(), spc.header.ffirst, spc.header.flast, spc.x_label());

// One subfile per spectrum: a single measurement has one, a series has many.
for (i, sub) in spc.subfiles.iter().enumerate() {
    println!("spectrum {i}, z = {}", sub.subheader.subtime);
    for (x, y) in sub.points().take(5) {
        println!("{x:10.3}  {y:12.6}");
    }
}

// The log block is vendor-specific and passed through raw.
if let Some(log) = &spc.log {
    if let Some(channel) = log.get("Channel") {
        println!("channel {channel}");
    }
}
# Ok::<(), spc_spectra::SpcError>(())
```

## Writing

`SpcBuilder` turns a spectrum into a file; `to_path()` and `to_bytes()` write a
file that was read or built.

```rust
use spc_spectra::{SpcBuilder, Technique, XType, YType};

let y: Vec<f64> = vec![/* your measurement */];

SpcBuilder::new(900.0, 1700.0, y)   // first x, last x, the data
    .x_type(XType::Nanometers)
    .y_type(YType::Absorbance)
    .technique(Technique::Nir)
    .source("NIR probe")
    .scans(32)                      // the format has a field for this
    .log_text("Channel=1\nIntegration=100ms")
    .build()?
    .to_path("spectrum.spc")?;
# Ok::<(), spc_spectra::SpcError>(())
```

The x axis is not stored in an SPC file — every reader regenerates it from the
two end points — which is why the builder takes a range rather than x values.
Every spectrum in a file shares it, so a series is one range and many curves:

```rust
use spc_spectra::{SpcBuilder, XType, ZSpacing};

// One (z, y) pair per spectrum: z places it in the series, y is the curve.
let spectra: Vec<(f32, Vec<f64>)> = vec![
    (16.57, vec![/* ... */]),
    (17.42, vec![/* ... */]),
];

SpcBuilder::series(900.0, 1700.0, spectra)
    .z_type(XType::Seconds)        // what the z values mean
    .z_spacing(ZSpacing::Uneven)   // and that they are not evenly spaced
    .build()?
    .to_path("series.spc")?;
# Ok::<(), spc_spectra::SpcError>(())
```

Reading and writing cover exactly the same ground, on purpose: **the writer runs
the reader's own validation before it writes a byte**, so a file this crate
produces is always one it can read back, and a variant it refuses to read is one
it refuses to write.

Every parsed field survives a round trip, and a file this crate wrote is
byte-stable. That includes the log block's own bookkeeping: instruments reserve
the block in whole allocation units and pad the rest with nulls — 4096 bytes for
153 bytes of text is typical — and both the reservation size and the padding are
written back as they were.

Byte-for-byte fidelity to a *foreign* file is still not promised, because the
reader does not model everything a file may hold:

- the reserved tails of the header, the subheader and the log block header are
  written as nulls, as is anything a log block held after its text area;
- log entries separated by nulls come back separated by newlines, and trailing
  whitespace in the log text is trimmed, which shrinks `logsizd` to match;
- text that was not valid UTF-8 is decoded lossily, and a field that grows past
  its slot that way is reported rather than truncated.

Two command-line examples, which are also the round trip in the small:

```sh
cargo run --example write -- spectrum.spc   # write a synthetic spectrum
cargo run --example dump  -- spectrum.spc   # read it back
cargo run --example dump  -- series.spc --sub 7   # one spectrum of a series
```

## What version 0.3 reads and writes

| Aspect | Supported |
| --- | --- |
| Version byte | `0x4B` — new format, little-endian |
| Subfiles | one or many (`TMULTI`), sharing one x axis |
| x axis | evenly spaced, generated from `ffirst`/`flast` |
| y values | IEEE floats (`fexp = 0x80`) |
| Log block | yes, raw binary and text |
| Bit-packed date (`fdate`) | yes |
| `subnpts = 0` shorthand | yes, and kept as the shorthand when written |
| Per-subfile `subtime`, `subindx`, `subscan` | yes |
| z spacing (`TORDRD`, `TRANDM`) | read and written, via `ZSpacing` |

### Not yet supported

Each of these is rejected with its own `Unsupported` variant — on reading and,
by the same check, on writing — so you always learn exactly which feature a file
needs:

| Variant | Error |
| --- | --- |
| Big-endian, `fversn = 0x4C` | `Unsupported::BigEndian` |
| Old format, `fversn = 0x4D` | `Unsupported::OldFormat` |
| Per-subfile x axes (`TXYXYS`) | `Unsupported::XyxySubfiles` |
| Explicit x values (`TXVALS`) | `Unsupported::ExplicitXValues` |
| 16-bit y values (`TSPREC`) | `Unsupported::SixteenBitY` |
| Multi-plane data cubes (`fwplanes > 1`) | `Unsupported::WPlanes` |
| Galactic fixed-point y values | `Unsupported::FixedPointY` |
| A subfile whose `subexp` contradicts `fexp` | `Unsupported::FixedPointSubfileY` |

This narrow scope is deliberate. For measurement data, an error you can act on
beats a spectrum that looks plausible and is quietly wrong. If you have a file
that hits one of these, an issue with the details is the fastest way to get it
supported — the main thing missing for the remaining variants is real-world
sample files to validate against.

## Roadmap

Widening the supported set, in the order the table above lists it: explicit x
values (`TXVALS`) and per-subfile x axes (`TXYXYS`) are the two that come up
most in practice. Each needs a real-world sample file to validate against —
see above.

## Testing

The test suite builds SPC files byte by byte in memory
(`tests/common/mod.rs`), so it needs no sample data of uncertain provenance:

```sh
cargo test
```

Seven suites, with different jobs:

- `roundtrip.rs` — parses a known-good file and checks every field matches.
- `unsupported.rs` — each refused variant is refused for the *right* stated
  reason, so an unreadable file never masquerades as a readable one.
- `robustness.rs` — the parser never panics. Exhaustive single-bit flips
  through the header and subheader, 100 000 random mutations, arbitrary
  garbage, and every plausible `flogoff`. Deterministic, so a failure is
  reproducible. It also holds the two properties the writer rests on: whatever
  the reader accepts, however mangled, must be writable and parse back the
  same — run over a single-spectrum and a four-spectrum file, so that `fnsub`
  is itself among the mutated fields; and whatever `SpcBuilder` can express —
  2 000 generated spectra, point counts from 1 to 1 000, magnitudes from
  `1e-15` to `1e14`, text fields up to their exact limit — must survive a
  round trip unchanged.
- `write.rs` — a parsed file, written back, must be **byte-identical** to the
  hand-assembled fixture. Checking the writer against the reader alone would
  pass happily if both shared a mistake about the layout. Run over four dozen
  named shapes — point counts, subfile counts, axis directions, log block
  combinations, text fields at their limits — because most layout mistakes
  survive any single geometry.
- `write_refuses.rs` — the writing counterpart to `unsupported.rs`: every way a
  file can fail to be written, refused for the right stated reason.
- `build.rs` — `SpcBuilder`, checked the same way: a file built from a spectrum
  and its metadata must come out byte-identical to the fixture, for a single
  spectrum and for a timed series alike.
- `real_files.rs` — the one check the others cannot make. A reader and a writer
  that share a mistake about the format agree with each other perfectly; only a
  file from foreign software settles it. Instrument exports cannot live in this
  repository, so the suite reads from a directory you point it at and skips when
  you do not:

  ```sh
  SPC_SAMPLE_DIR=/path/to/spc/files cargo test --test real_files
  ```

  Each file must parse or name the feature it needs, every modelled field must
  survive a round trip, and every byte that differs from the original must fall
  in a region this README lists above. Failures name byte offsets and field
  names, never field contents.

## Related work

- [`spc`](https://github.com/rohanisaac/spc) and
  [`spc-spectra`](https://pypi.org/project/spc-spectra/) — Python
- [`hyperSpec`](https://github.com/r-hyperspec/hyperSpec) /
  `hySpc.read.spc` — R
- [`spc-parser`](https://github.com/cheminfo/spc-parser) — JavaScript
- [`SPCSpectra.jl`](https://github.com/hhaensel/SPCSpectra.jl) — Julia

## License

Licensed under the MIT license ([LICENSE-MIT](LICENSE-MIT)).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you shall be licensed as above, without any
additional terms or conditions.

## Trademarks and affiliation

This project is **not affiliated with, endorsed by, or sponsored by Thermo
Fisher Scientific**. "Thermo", "Galactic" and "GRAMS" are trademarks of their
respective owners and are used here only descriptively, to say which files this
crate reads and writes.

The format specification document is copyrighted and is neither included in nor
distributed with this repository. This implementation describes the format's
structure and behaviour in its own words.
