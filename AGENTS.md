# Intro

`spc-spectra` reads and writes SPC spectroscopy files (Thermo Galactic /
GRAMS): one crate, no dependencies, no `unsafe`, MSRV 1.85 on edition 2024.
`README.md` carries the format, the byte layout and the supported variants.

Its purpose is to refuse rather than guess. For measurement data an error beats
a spectrum that looks plausible and is quietly wrong, so an unsupported variant
gets a named `Unsupported` value and a self-contradicting file is rejected, 
neither is ever parsed on a hunch.

## Constraints

- Never add sample `.spc` files to the repository, and never copy sample,
  operator, company or device names out of an instrument file into code, tests,
  comments or output. Instruments write them into `fcmnt`, `fsource` and the
  log block.
- No dependencies. Adding one is a decision about the crate's identity.

## Rust Coding Guidelines

- Prioritize code correctness and clarity. Speed and efficiency are secondary priorities 
  unless otherwise specified.
- Do not write organizational or comments that summarize the code. Comments should only 
  be written in order to explain "why" the code is written in some way in the case there 
  is a reason that is tricky or non-obvious.
- Prefer implementing functionality in existing files unless it is a new logical component. 
  Avoid creating many small files.
- Avoid using functions that panic like `unwrap()`, instead use mechanisms like `?` to 
  propagate errors.

## Commands

```sh
cargo test              # also compiles the examples and runs the doctests
cargo test --release    # a second run, not a repeat: no debug_assert!, no overflow checks
cargo test --test roundtrip -- <name>            # one suite, one test
cargo fmt --check
cargo clippy --all-targets -- -D warnings        # what makes missing_docs fatal
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps   # catches [links] that resolve to nothing
cargo +1.85 test                                 # MSRV, enforced by CI
SPC_SAMPLE_DIR=/path/to/spc/files cargo test --test real_files   # inert without the variable
cargo package           # compiles what publish would upload; catches an exclude line
cargo run --example dump -- spectrum.spc [--sub N]
```

## Invariants that span files

- **The writer runs the reader's validation.** `Spc::to_bytes` goes through
  `writable_subfiles` (`src/spc.rs:281`), which calls the same
  `Header::validate` and `SubHeader::validate` that `from_bytes` calls, so
  "writable by this crate" and "readable by this crate" are the same set by
  construction. A new read-side check belongs in `validate()`, not inline in
  `from_bytes`, or the writer silently diverges.
- **`src/bytes.rs` is the only place that slices input bytes, `src/write.rs`
  the only one that appends them.** Every `Cursor` accessor is bounds-checked
  and returns `SpcError::TooShort` rather than panicking. Parsing that bypasses
  it voids the never-panics property `tests/robustness.rs` exists to hold.
- **`tests/common/mod.rs` assembles SPC files by hand and predates the
  writer.** Never make a failing byte comparison pass by having the fixture
  call the writer: a reader and a writer sharing a layout mistake round-trip
  happily, and that comparison is the only thing that catches it.
- **Two fields do not mean what they look like.** `subnpts == 0` means "as many
  points as `fnpts`", not an empty subfile. An exponent of `0x80` means IEEE
  floats and every other value, `0` included, is a real fixed-point exponent —
  which of `subexp` and `fexp` governs is `SubHeader::effective_exponent`
  (`src/subheader.rs:115`).

## The decision rule

Decide from what the file states, never from inference. Values **bound to the
bytes being written** (`fnsub`, `flogoff`, `logsizd`, `logtxto`, `logbins`) are
recomputed and checked; values that are **observations the file carried**
(`ftflgs`, `logsizm`, `logdsks`) pass through unchanged, even when they
contradict something else. A contradiction is reported or preserved, never
silently corrected. This settles most new format questions on its own.

## Conventions

- Follow the [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) format.
- Use conventional commits. 
- Keep commits focused.
- Update `CHANGELOG.md` after every meaningful change (new features, bug fixes, 
  breaking changes, deprecations, removals).
- The `CHANGELOG.md` is user-facing only. Refactoring, test infrastructure and
  documentation wording are deliberately left to the commit history.

## Security

- Never commit credentials, generated build products, or user data. 
- Never expose personally identifiable machine or user information. 
- Never override the configured Git author or committer identity.
