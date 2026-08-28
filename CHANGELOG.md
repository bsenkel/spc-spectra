# Changelog

Notable changes to this crate, from the point of view of someone using it.
Internal work — refactoring, test infrastructure, documentation wording — is
left to the commit history.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/). Note
what that means below 1.0: `0.2.1` is compatible with `0.2.0`, while `0.3.0`
is not. Every public struct and enum is `#[non_exhaustive]`, so new fields,
variants and builder methods can arrive in a patch release without breaking
anyone.

## [Unreleased]

## [0.2.1] - 2026-08-28

### Fixed

- **The log block's `logsizm` and `logdsks` now survive a round trip.** Writing
  recomputed all five of the block's size fields. That is right for `logsizd`,
  `logtxto` and `logbins`, which are offsets and lengths into the block being
  written, but wrong for the other two: `logsizm` records how much memory the
  acquiring software reserved, `logdsks` a vendor-reserved area. Neither
  describes our bytes, so both were information the file carried and writing
  threw away. This contradicted the round-trip guarantee the crate makes, and no
  test looked at the log block, so nothing caught it.

- **`logsizd` now measures the way other readers of this format interpret it.**
  It spans from the start of the log block to the end of the text, so the text
  is `logsizd - logtxto` bytes long. It previously counted the text's
  terminating null as well, putting that null inside the range a reader takes as
  text. Readers strip nulls, so nothing was broken by it; this is one byte of
  slop removed, not a rescue.

### Added

- `LogBlock::stored_size` records how many bytes the block occupied in the file
  it was read from. Instruments reserve the block in whole allocation units and
  pad the remainder with nulls — 4096 bytes for 153 bytes of text is typical —
  and writing now restores that padding. The target comes from the file the
  block was read from, never from `logsizm`, which is a number out of the file
  like any other. `None` for a block built with `LogBlock::new()`.

## [0.2.0] - 2026-08-26

### Added

- **Writing.** `Spc::to_bytes()` and `Spc::to_path()` serialise a file, mirroring
  `from_bytes()` and `from_path()`. The scope is deliberately the reader's own:
  `fversn = 0x4B`, one subfile, IEEE float y values, an evenly spaced x axis and
  an optional log block. Writing runs the reader's validation before it emits a
  byte, so a file this crate produces is always one it can read back, and a
  variant it refuses to read is one it refuses to write.
- `SpcBuilder`, which turns a spectrum and its metadata into an `Spc`. It is the
  way to build a file from a measurement rather than from another file, since the
  public types are `#[non_exhaustive]` and cannot be constructed by hand. Fields
  without a setter are set on the `Spc` it returns.
- `LogBlock::new()`, to attach a log block to a file that was read.
- Three `SpcError` variants for failures only writing can hit: `FieldTooLong`
  for a text value that does not fit its fixed-width slot, `ValueNotRepresentable`
  for a finite y value with no `f32` equivalent, and `NotWritable` for parts that
  contradict each other — a point count that disagrees with the y values, or an x
  axis that is not the one `ffirst`/`flast` describe. None of these truncates or
  substitutes silently.
- `SpcBuilder::scans()` records the number of co-added scans in `subscan`, the
  field the format has for it, rather than leaving it to a `key=value` line in
  the log text.
- `examples/write.rs` writes a synthetic spectrum; `examples/dump.rs` now reports
  the scan count.

### Notes on round trips

Every field this crate parses survives a read/write round trip, and a file it
wrote is byte-stable. Byte-for-byte fidelity to a *foreign* file is not promised:
reserved areas are written as nulls, log entries separated by nulls come back
separated by newlines, and text that was not valid UTF-8 is decoded lossily. See
`Spc::to_bytes` for the complete list.

## [0.1.0] - 2026-08-25

### Added

- Initial release. Reads `fversn = 0x4B` files with a single subfile, IEEE float
  y values, an evenly spaced x axis generated from `ffirst`/`flast`, a bit-packed
  date and an optional log block. Every other format variant is refused with a
  named `Unsupported` value rather than parsed on a guess. No dependencies, no
  `unsafe`.

[Unreleased]: https://github.com/bsenkel/spc-spectra/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/bsenkel/spc-spectra/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/bsenkel/spc-spectra/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/bsenkel/spc-spectra/releases/tag/v0.1.0
