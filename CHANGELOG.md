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

## [0.4.1] - 2026-08-31

### Fixed

- **`SpcError::ValueNotRepresentable` renders a different message**, because
  the old one blamed `f32` even where the governing exponent was the limit.

## [0.4.0] - 2026-08-30

### Added

- **Galactic fixed-point y values.** Files whose `fexp` is not `0x80` store
  their y values as 32-bit integers sharing one exponent
  (`y = raw · 2^(fexp - 32)`); they are now read and written like any other.
  The scale is a power of two, so the round trip is exact in both directions,
  unlike the float path, where narrowing to `f32` rounds. Which exponent
  governs is stated rather than assumed: without `TMULTI` the file-wide `fexp`,
  with `TMULTI` each subfile's own `subexp`. A `subexp` of `0` is a legal
  exponent and never means "not filled in".

- **The header's text fields now round-trip byte for byte.** Two things real
  instrument files do that this crate could not reproduce: an `fcmnt` holding
  two null-separated entries lost the second one, and an `fres` that was not
  UTF-8 grew past its own nine byte slot on the way to a `String`, so the file
  could be read but not written back. The fields keep their bytes now, and
  decode on demand.

- `Header::set_fres` and its four siblings set a text field without the caller
  restating its name and width, the read, adjust, write back path. Each
  refuses over-long text under its own field name and leaves the field
  unchanged. `set_fcatxt` takes the axis labels as a list; it does not set
  `TFlags::TALABS`, which stays the caller's decision.

### Changed

- **Breaking:** `fres`, `fsource`, `fcmnt`, `fmethod` and `fcatxt` change from
  `String` and `[u8; 30]` to the new `TextField<N>`. **Migration:** add
  `.text()` where a `String` was expected. `Display` is implemented, so
  printing a field is unchanged, and `Header::custom_axis_labels` keeps
  working. `TextField` also offers `entries()` for a field holding several
  null-separated values, `as_bytes()` and `is_empty()`.
- `SpcError::FieldTooLong` no longer comes out of `Spc::to_bytes`. It is
  reported where the field is built — `TextField::new`, `Header::set_*`,
  `SpcBuilder::build`, so an over-long value can never reach a header at all.
  The variant itself is unchanged and still names the field.
- `Unsupported::FixedPointSubfileY` names a narrower case: `TMULTI` is set, so
  `subexp` governs and says fixed-point, while `fexp` announces floats. Nothing
  in the file settles which is right, so it is refused rather than decided.
  Without `TMULTI` a `subexp` that differs from `fexp` is no longer an error,
  nobody reads it, and it travels through unchanged.
- Writing a y value that a fixed-point scale cannot hold is refused with
  `SpcError::ValueNotRepresentable`, for either way the value would be lost
  outright: it exceeds the range of `i32`, or it is under half a step and would
  quantise to zero. Ordinary rounding to the nearest step is not refused, just
  as narrowing to `f32` is not.

### Notes

- `Unsupported::FixedPointY` is no longer produced. The variant stays so that
  existing `match` arms keep compiling.
- Reading a fixed-point file needs no new API: y values remain `f64`.

## [0.3.0] - 2026-08-28

### Added

- **Multifile records.** Files holding more than one spectrum — `TMULTI`, or an
  `fnsub` greater than one — are read and written like any other. All subfiles
  share the one x axis `ffirst`/`flast` describes; `TXYXYS`, where each carries
  its own, is still refused. `Spc::subfiles` holds them in file order, each with
  its own `subtime`, `subindx` and `subscan`.
- `SpcBuilder::series` builds such a file from one `(z, y)` pair per spectrum.
  Unlike `SpcBuilder::new` it records the first spectrum's z value, which
  instrument files routinely have: a series is timed from when the instrument
  started, not from when the run began.
- `SpcBuilder::add_spectrum` and `add_spectrum_at` append one further spectrum
  to a builder. `add_spectrum` leaves the z value at zero rather than numbering
  the spectra, which would be data this crate invented.
- `SpcBuilder::z_type` sets `fztype`, the unit the per-spectrum z values are in.
- `ZSpacing`, with `SpcBuilder::z_spacing` and `Header::z_spacing`, states
  whether the z values are evenly spaced, ordered but uneven (`TORDRD`) or in no
  order at all (`TRANDM`). A file that says nothing claims even spacing, so
  another program may compute each z from the first one and a constant step
  instead of reading it. Never derived from the values: deciding whether two
  `f32` intervals are "the same" needs a tolerance, which is the caller's call.
- `examples/dump.rs` reports the number of subfiles and takes `--sub N` to
  tabulate one of them.

### Deprecated

- `Spc::y()` and `Spc::x()`. They return the first subfile and say nothing about
  the rest, which was harmless while a file could only hold one. Use
  `Spc::subfiles`, whose every entry has its own `x` and `y`. Both still work.

### Notes

- `Unsupported::MultiFile` is no longer produced. The variant stays, so existing
  `match` arms keep compiling.
- The `TMULTI` flag is not cross-checked against the number of subfiles, on
  reading or on writing: `fnsub` is the count, the flag an observation the file
  carried. A file that contradicts itself is read as its count says and written
  back unchanged, rather than corrected.

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

[Unreleased]: https://github.com/bsenkel/spc-spectra/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/bsenkel/spc-spectra/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/bsenkel/spc-spectra/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/bsenkel/spc-spectra/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/bsenkel/spc-spectra/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/bsenkel/spc-spectra/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/bsenkel/spc-spectra/releases/tag/v0.1.0
