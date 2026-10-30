# Changelog

All notable changes to this project will be documented in this file.

## [0.0.2] - 2026-10-30

The first release whose runtime works from crates.io: 0.0.1 shipped only the schema, because the
runtime depended on unpublished Noir crates.

### Changed (breaking)
- Targets **Noir 1.0**: artifacts must come from a stable `nargo` 1.0.x (1.0.0 recommended);
  prereleases (e.g. `1.0.0-beta.N`) and other major/minor releases are rejected with a message
  naming both versions. MSRV is Rust 1.89.
- The runtime depends only on Noir crates published on crates.io (`acvm`, `acir_field`,
  `bn254_blackbox_solver` 1.0.0). `noir-runner` is gone; `simulate` runs from the artifact
  embedded by `nrg!`, decoded once and cached, with no per-call disk I/O.
- Value conversions are `noi::Value`, `ToValue`, and `FromValue` (were `InputValue`,
  `ToInputValue`, `FromOutputValue`).
- `Client::new()` takes no arguments. Oracles are answered by `Client::with_oracle(name, handler)`
  (replacing the JSON-RPC resolver); unhandled oracles fail with their name, and `print` output
  is discarded.
- Generated functions take `&Args`. `Inputs`, `PublicInputs`, and `PrivateInputs` were removed:
  Noir 1.0 forbids `pub` on `#[export]` parameters, so they were always empty.
- Generated code references only `::noi`, and which items exist follows the features of the
  `noi` crate it is compiled against (not the proc macro's host build). Crates using `nrg!` need
  no other direct dependency. `noi-macro` no longer has features.
- `noi-build`: `BuildOptions::from_env`/`new` builders, `ExportSummary::source`. `nargo export` runs from a temporary directory with a generated `Nargo.toml` that references the package's entry file and path dependencies by absolute path (no source copy), so the build never modifies the source tree;
  committed `export/` artifacts are only read, as a fallback.
- Struct naming: path-based names when short names collide, a `Noir` prefix for names that
  would shadow `Client` or prelude types (`NoirOption`), numbered names for generic
  instantiations. `collect_structs` returns every distinct instantiation.
- Relative `export_dir` paths in `nrg!` resolve against the invoking crate's `CARGO_MANIFEST_DIR`.

### Added
- Proving (feature `prove`) with Barretenberg 6.0.0: `noi::Prover`, `noi::Proof` (whose
  `return_value` decodes the public inputs; generated `verified_output` returns the typed
  `Output`), `VerifierTarget` (default, EVM, recursive Noir), and generated `prove`, `verify`,
  `verified_output`, `verification_key`, and `solidity_verifier`. `verify` hands `bb` an explicit
  encoding of the statement it decoded, so `bb` and `return_value` always agree; `bb` environment
  overrides are ignored; tooling failures are errors rather than "invalid proof". Verification
  keys are cached in a private per-user directory keyed by a SHA-256 of the artifact, target, and
  `bb` version. Single-circuit programs only (no `#[fold]`), a limit of bb's UltraHonk prover.
- `noi::ExecutionError` with the assertion message as nargo prints it (format strings filled in,
  out-of-bounds errors in the program's own coordinates) and the solver detail.
- Execution of multi-circuit programs (`#[fold]`), Brillig, and oracles; `str<N>` (non-UTF-8
  bytes decode lossily in simulation, as in nargo, and strictly in proven statements) and
  `databus` ABI support; tuples up to 12 elements.
- Artifacts are validated on load: invalid integer widths and encodings larger than `u32::MAX`
  field elements are rejected instead of overflowing later.
- `noi::noir_version_compatible`, `NOIR_VERSION`, `BB_VERSION`.
- `noi-build`: Noir packages with a custom `[package] entry`, symlinked path dependencies, and
  re-export after installing or upgrading `nargo`; staged artifacts have package-relative paths,
  so builds are reproducible.
- `scripts/install_bb.sh`, reproducible example artifacts, `scripts/check_exports.py`,
  `scripts/test_packaged.sh`, a pinned development toolchain (1.92), a committed `Cargo.lock`, and
  CI for fmt, clippy, tests per feature set, proving, real `nargo`, MSRV 1.89, packaging (including
  the packaged crates' own tests), and docs.
- Each published crate ships the Apache-2.0 `LICENSE`.
- Crate-level guides and API docs on every public item, with runnable doctests.

### Fixed
- `noi-build` rewrote committed `export/*.json` in the source tree whenever `nargo` was
  installed, which broke `cargo publish` for downstream crates; it also preferred stale `target/`
  JSON, warned on every build, missed new Noir files and path dependencies, ran any `nargo`
  version, and silently fell back to stale artifacts on Noir compile errors.
- Internal dependencies now carry both `path` and `version`, so `noi` works as a path or git
  dependency.
- Codegen failed for Noir structs named `Client`, fields named `fields`, generic structs used with
  two type arguments, arrays longer than 32, and names that collide after `snake_case`.
- `scripts/refresh_exports.sh` could not download `nargo` (renamed release assets, Rosetta, a
  stdin bug), forced a full Rust toolchain download, and accepted any cached `nargo` version.

## [0.0.1] - 2025-11-21
### Added
- Initial workspace bootstrap with `noi-core`, `noi-build`, `noi-macro`, and `noi` crates.
- Noir demo program and Rust example app demonstrating generated bindings.
- Renamed the proc macro to `nrg!` (dropping all legacy aliases).
- Reorganized the workspace so the demo Rust app and Noir project live together under `examples/demo/`.
- Added a dark-pool-inspired example (`examples/dark-pool/`) that mirrors more realistic structs and arrays.
