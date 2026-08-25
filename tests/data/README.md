# Test data

This directory is intentionally almost empty.

The test suite builds SPC files synthetically in memory (see
`tests/common/mod.rs`), so no third-party sample files are needed to run
`cargo test`.

## If you add files here

Only add files whose licensing you have actually checked:

- **Your own instrument exports** are fine to commit copyright-wise, but check
  `fcmnt`, `fsource` and the log block first — instruments routinely write
  operator names, sample/project labels or acquisition file paths in there.
  Run `cargo run --example dump` on the file and read the output before
  committing.
- **hyperSpec (R)**, **cheminfo/spc-parser (JS)** and similar collections are
  useful for local verification, but at least one hyperSpec dataset is credited
  to Thermo Galactic as co-author. Use those locally; do not commit them.
- The format specification PDF is copyrighted. Link to it, never vendor it.

`Cargo.toml` currently excludes this directory from the published package via
`exclude = ["tests/data/*"]`. Revisit that line once real files are committed
and their licensing is settled.

## Verifying against a real file

```sh
cargo run --example dump -- tests/data/your-file.spc
```
