# noi-build

`build.rs` helper for [`noi`](https://docs.rs/noi). It runs `nargo export` on a temporary copy of
your Noir library package (your source tree is never modified), stages the JSON artifacts in
`$OUT_DIR/noi_exports`, sets `NOI_EXPORT_DIR` for the `nrg!` macro, and re-runs when your Noir
sources, their path dependencies, or `nargo` change.

```rust
// build.rs
fn main() {
    noi_build::BuildOptions::from_env("noir") // Noir package dir, relative to Cargo.toml
        .and_then(|opts| opts.export())
        .unwrap_or_else(|err| panic!("failed to export Noir artifacts: {err:#}"));
}
```

If `nargo` 1.0.0 is not available, it uses the artifacts committed in `<package>/export/` and
emits a `cargo:warning`; a Noir compile error fails the build. Knobs: `NOI_NARGO_BIN`,
`NOI_SKIP_NARGO=1`, `NOI_VERBOSE=1`. See the [docs](https://docs.rs/noi-build).
