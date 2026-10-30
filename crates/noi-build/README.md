# noi-build

`build.rs` helper for [`noi`](https://docs.rs/noi). It runs `nargo export` for your Noir package,
stages the JSON artifacts in `$OUT_DIR/noi_exports`, sets `NOI_EXPORT_DIR` for the `nrg!` macro, and
re-runs when your Noir sources change.

```rust
// build.rs
fn main() {
    noi_build::BuildOptions::from_env("noir") // Noir package dir, relative to Cargo.toml
        .expect("OUT_DIR is set by cargo")
        .export()
        .expect("failed to export Noir artifacts");
}
```

Without `nargo` installed it falls back to the artifacts committed in `<package>/export/` and emits a
`cargo:warning`. Knobs: `NOI_NARGO_BIN`, `NOI_SKIP_NARGO=1`, `NOI_VERBOSE=1`. See the
[docs](https://docs.rs/noi-build).
