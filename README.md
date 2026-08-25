# spc-spectra

A Rust reader for **SPC spectroscopy files** — the binary format introduced by
Galactic Industries and carried on in Thermo's GRAMS software. It is still the
everyday interchange format for FT-IR, Raman, NIR, UV-VIS, NMR and MS data.

Readers for this format exist in Python, R and Julia. This crate fills the gap
in Rust.

- **No dependencies.** Standard library only.
- **No `unsafe`.** Enforced with `#![forbid(unsafe_code)]`.
- **Refuses rather than guesses.** Format variants that are not decoded yet are
  reported as a specific error, never parsed on a hunch. The same applies to a
  file that contradicts itself: where the format states an invariant, it is
  checked rather than assumed.

```toml
[dependencies]
spc-spectra = "0.1"
```

## Usage

```rust
use spc_spectra::Spc;

let spc = Spc::from_path("spectrum.spc")?;

println!("{} ({})", spc.header.fexper, spc.header.fsource);
println!("{} points, {} .. {} {}",
         spc.y().len(), spc.header.ffirst, spc.header.flast, spc.x_label());

for (x, y) in spc.subfiles[0].points().take(5) {
    println!("{x:10.3}  {y:12.6}");
}

// The log block is vendor-specific and passed through raw.
if let Some(log) = &spc.log {
    if let Some(channel) = log.get("Channel") {
        println!("channel {channel}");
    }
}
# Ok::<(), spc_spectra::SpcError>(())
```

There is also a command-line dump for inspecting a file:

```sh
cargo run --example dump -- spectrum.spc
```

## What version 0.1 reads

| Aspect | Supported |
| --- | --- |
| Version byte | `0x4B` — new format, little-endian |
| Subfiles | exactly one |
| x axis | evenly spaced, generated from `ffirst`/`flast` |
| y values | IEEE floats (`fexp = 0x80`) |
| Log block | yes, raw binary and text |
| Bit-packed date (`fdate`) | yes |
| `subnpts = 0` shorthand | yes |

### Not yet supported

Each of these is rejected with its own `Unsupported` variant, so you always
learn exactly which feature a file needs:

| Variant | Error |
| --- | --- |
| Big-endian, `fversn = 0x4C` | `Unsupported::BigEndian` |
| Old format, `fversn = 0x4D` | `Unsupported::OldFormat` |
| Multiple subfiles (`TMULTI`) | `Unsupported::MultiFile` |
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

- **Writing.** `Spc::to_bytes()` / `Spc::to_path()` to serialize a `Spc` back
  to a valid `.spc` file, mirroring `from_bytes()`/`from_path()`. Not started
  yet; would begin with the same scope as the current reader (single
  subfile, IEEE float y values) before widening.

## Testing

The test suite builds SPC files byte by byte in memory
(`tests/common/mod.rs`), so it needs no sample data of uncertain provenance:

```sh
cargo test
```

Three suites, with different jobs:

- `roundtrip.rs` — parses a known-good file and checks every field matches.
- `unsupported.rs` — each refused variant is refused for the *right* stated
  reason, so an unreadable file never masquerades as a readable one.
- `robustness.rs` — the parser never panics. Exhaustive single-bit flips
  through the header and subheader, 100 000 random mutations, arbitrary
  garbage, and every plausible `flogoff`. Deterministic, so a failure is
  reproducible.

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
crate reads.

The format specification document is copyrighted and is neither included in nor
distributed with this repository. This implementation describes the format's
structure and behaviour in its own words.
