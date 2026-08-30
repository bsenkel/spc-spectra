# spc-spectra

[![CI](https://github.com/bsenkel/spc-spectra/actions/workflows/ci.yml/badge.svg)](https://github.com/bsenkel/spc-spectra/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/spc-spectra.svg)](https://crates.io/crates/spc-spectra)
[![docs.rs](https://img.shields.io/docsrs/spc-spectra)](https://docs.rs/spc-spectra)

A Rust reader and writer for **SPC spectroscopy files** — the binary format
introduced by Galactic Industries and carried on in Thermo's GRAMS software. It
is still the everyday interchange format for FT-IR, Raman, NIR, UV-VIS, NMR and
MS data.

It works on bytes, so the data need not come from a file at all: an instrument
driver, a network stream or a database blob does just as well. `from_path` and
`to_path` are conveniences over `from_bytes` and `to_bytes`.

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
spc-spectra = "0.4"
```

Rust 1.85 or newer. Tested on Linux, macOS and Windows, in debug and release.

## Usage

```rust
use spc_spectra::Spc;

let spc = Spc::from_path("spectrum.spc")?;
// or, when the bytes are already in hand:
// let spc = Spc::from_bytes(&raw)?;

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
```

## What survives a round trip

Reading and writing cover exactly the same ground, on purpose: **the writer runs
the reader's own validation before it writes a byte**, so a file this crate
produces is always one it can read back, and a variant it refuses to read is one
it refuses to write.

Every parsed field survives a round trip, and a file this crate wrote is
byte-stable. That includes the log block's own bookkeeping: instruments reserve
the block in whole allocation units and pad the rest with nulls, 4096 bytes for
153 bytes of text is typical and both the reservation size and the padding are
written back as they were.

Byte-for-byte fidelity to a *foreign* file is still not promised, because the
reader does not model everything a file may hold:

- the reserved tails of the header, the subheader and the log block header are
  written as nulls, as is anything a log block held after its text area;
- log entries separated by nulls come back separated by newlines, and trailing
  whitespace in the log text is trimmed, which shrinks `logsizd` to match.

## Header text fields

The header's text fields are reproduced exactly, and that took a deliberate
design. They are modelled as `TextField`, which keeps the bytes the file held,
because decoding them is not reversible: everything past the first null is
dropped, invalid UTF-8 becomes a replacement character three bytes wide, and the
edges are trimmed. Real files run into all three. Some instruments pack two
null-separated entries into `fcmnt`, and a `fres` that is not UTF-8 at all used
to grow past its own slot and make a readable file unwritable. Read them with
`text()`, or with `entries()` where a field holds several values.

What `text()` does *not* do is guess a code page. SPC has no field naming one,
and the bytes are in practice Windows-1252 or similar. The file itself stays
correct, the bytes are written back untouched, but for example a caller who 
needs an umlaut has to decode `as_bytes()` itself for now.

## Command-line examples

Two examples, which are also the round trip in the small:

```sh
cargo run --example write -- spectrum.spc   # write a synthetic spectrum
cargo run --example dump  -- spectrum.spc   # read it back
cargo run --example dump  -- series.spc --sub 7   # one spectrum of a series
```

## How an SPC file is laid out

The new format, `fversn = 0x4B`, which is the one this crate reads. The old
`0x4D` uses a 256 byte header, and `TSPREC` stores two bytes per point instead
of four; neither is supported yet.

```text
offset  0  +---------------------------------------------+
           | Main header - 512 bytes                     |
           | ftflgs . fexp . fnpts . ffirst/flast        |
      512  +---------------------------------------------+  --+
           | Subheader - 32 bytes                        |    |
           | subexp . subtime . subnpts                  |    | once per
      544  +---------------------------------------------+    | subfile
           | y values - 4 bytes per point                |    |
           | IEEE float, or fixed-point i32              |    |
           +---------------------------------------------+  --+
  flogoff  +---------------------------------------------+
           | Log block - optional                        |
           | 64-byte header . binary area . text         |
           +---------------------------------------------+

the x axis is stored nowhere: it is regenerated from ffirst,
flast and the point count
```

## What version 0.4 reads and writes

An orientation, not a description of the format — for that, see the layout
above. The list of refusals below it *is* complete: it names every reason this
version can turn a file away.

| Aspect | Supported |
| --- | --- |
| Version byte | `0x4B` — new format, little-endian |
| Subfiles | one or many (`TMULTI`), sharing one x axis |
| x axis | evenly spaced, generated from `ffirst`/`flast` |
| y values | IEEE floats (`fexp = 0x80`) and Galactic fixed-point integers |
| Log block | yes, raw binary and text |
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
| A `TMULTI` subfile whose `subexp` contradicts `fexp` | `Unsupported::FixedPointSubfileY` |

This narrow scope is deliberate. For measurement data, an error you can act on
beats a spectrum that looks plausible and is quietly wrong. If you have a file
that hits one of these, an issue with the details is the fastest way to get it
supported. For some of these a real-world sample file is the main thing still
missing.

The list shrinks from the top. `TSPREC` is the nearest, since it shares the
fixed-point arithmetic already in place and differs only in storing two bytes
per point; `TXVALS` and `TXYXYS` need more, and `TXYXYS` needs a real sample
file most of all.

## Testing

The suite builds SPC files byte by byte in memory (`tests/common/mod.rs`), so it
needs no sample data of uncertain provenance:

```sh
cargo test
```

Four properties carry most of the weight:

- **The writer is checked against hand-assembled bytes, not against the
  reader** — both sharing a mistake about the layout would pass any round trip.
  Over fifty named shapes: point counts, subfile counts, axis directions, y
  encodings and their exponents, log block combinations, text fields at their
  limits.
- **The parser never panics.** Exhaustive single-bit flips through the header
  and subheader, 100 000 random mutations, arbitrary garbage and every plausible
  `flogoff` — deterministic, so a failure is reproducible.
- **Whatever the reader accepts must be writable**, and must parse back the
  same. 20 000 mutations each over a single-spectrum, a four-spectrum and a
  fixed-point file, so that `fnsub` and the y exponents are themselves among the
  mutated fields.
- **Every refusal names the right reason**, on reading and on writing alike, so
  an unreadable file never masquerades as a readable one.

Each suite states its own job at the top of its file. One is optional: the
cross-check against foreign instrument files, which cannot live in this
repository, and which is the only test the others cannot stand in for.

```sh
SPC_SAMPLE_DIR=/path/to/spc/files cargo test --test real_files
```

Failures there name byte offsets and field names, never field contents.

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
