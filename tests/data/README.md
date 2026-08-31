# Test data

This directory is intentionally almost empty.

The test suite builds SPC files synthetically in memory (see
`tests/common/mod.rs`), so no third-party sample files are needed to run
`cargo test`.

## If you add files here

Only add files whose licensing you have actually checked:

- **Your own instrument exports** are fine to commit copyright-wise, but check
  `fcmnt`, `fsource` and the log block first, instruments routinely write
  operator names, sample/project labels or acquisition file paths in there.
  Run `cargo run --example dump` on the file and read the output before
  committing.

`Cargo.toml` currently excludes this directory from the published package via
`exclude = ["tests/data/*", ".github/"]`. Revisit that line once real files are
committed and their licensing is settled.

## Verifying against real files

Point the optional suite at a directory of instrument exports. It never reads
from this directory by default, so the files can stay wherever they are:

```sh
SPC_SAMPLE_DIR=/path/to/spc/files cargo test --test real_files
```

Each file must parse or name the feature it needs, every modelled field must
survive a round trip, and every byte that differs from the original must fall in
a region the README documents. Failures name byte offsets and field names, never
field contents.

To look at a single file:

```sh
cargo run --example dump -- your-file.spc
```
