# Changelog

All notable changes to this project will be documented in this file.

## [0.0.2] - Unreleased

The first release whose runtime works from crates.io: 0.0.1 shipped only the schema, because the
runtime depended on unpublished Noir crates.

### Changed (breaking)
- Targets **Noir 1.0.0**: artifacts must be exported with `nargo` 1.0.0; others are rejected with
  a message naming both versions. MSRV is Rust 1.89.
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
- Generated code references only `::noi`; crates using `nrg!` need no other direct dependency.
- `noi-build`: `BuildOptions::from_env`/`new` builders, `ExportSummary::source`. `nargo export`
  runs on a temporary copy of the package, so the build never modifies the source tree;
  committed `export/` artifacts are only read, as a fallback.
- Struct naming: path-based names when short names collide, a `Noir` prefix for names that
  would shadow `Client` or prelude types (`NoirOption`), numbered names for generic
  instantiations. `collect_structs` returns every distinct instantiation.
- Relative `export_dir` paths in `nrg!` resolve against the invoking crate's `CARGO_MANIFEST_DIR`.

### Added
- Proving (feature `prove`) with Barretenberg 6.0.0: `noi::Prover`, `noi::Proof` (with typed
  `return_value`), `VerifierTarget` (default, EVM, recursive Noir), generated `prove`, `verify`,
  `verified_output`, `verification_key`, and `solidity_verifier`. Verification keys are cached in
  a private per-user directory keyed by a SHA-256 of the artifact, target, and `bb` version.
- `noi::ExecutionError` with the assertion message (format strings filled in) and solver detail.
- Execution of multi-circuit programs (`#[fold]`), Brillig, and oracles; `str<N>`, `u1`, and
  `databus` ABI support; tuples up to 12 elements.
- `noi::noir_version_compatible`, `NOIR_VERSION`, `BB_VERSION`.
- `scripts/install_bb.sh`, reproducible example artifacts, `scripts/check_exports.py`, and CI for
  fmt, clippy, tests per feature set, proving, MSRV, packaging, and docs.
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
- Proving: `bb` environment variables could silently switch the verifier target; JSON-shaped
  public inputs could be verified as a different statement than the one decoded; `bb` tooling
  errors were reported as invalid proofs; cached keys were trusted by a non-cryptographic hash.
- `scripts/refresh_exports.sh` could not download `nargo` (renamed release assets, Rosetta, a
  stdin bug), forced a full Rust toolchain download, and accepted any cached `nargo` version.

## [0.0.1] - 2025-11-21
### Added
- Initial workspace bootstrap with `noi-core`, `noi-build`, `noi-macro`, and `noi` crates.
- Noir demo program and Rust example app demonstrating generated bindings.
- Renamed the proc macro to `nrg!` (dropping all legacy aliases).
- Reorganized the workspace so the demo Rust app and Noir project live together under `examples/demo/`.
- Added a dark-pool-inspired example (`examples/dark-pool/`) that mirrors more realistic structs and arrays.
