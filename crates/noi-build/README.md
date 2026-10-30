# noi-build

`build.rs` helper for [`noi`](https://docs.rs/noi). It runs `nargo export` from a temporary directory whose `Nargo.toml` points at your Noir library package's entry file and path dependencies by absolute path (sources are read in place; your source tree is never written), stages the JSON artifacts in
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

If a compatible `nargo` (stable 1.0.x) is not available, it uses the artifacts committed in `<package>/export/`
(for a Nargo workspace member, copy them there from `<workspace>/export/`) and emits a
`cargo:warning`; a Noir compile error fails the build. Knobs: `NOI_NARGO_BIN`,
`NOI_SKIP_NARGO=1`, `NOI_VERBOSE=1`. See the [docs](https://docs.rs/noi-build).
