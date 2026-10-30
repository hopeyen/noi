# Changelog

All notable changes to this project will be documented in this file.

## [0.0.2] - Unreleased
### Changed (breaking)
- Upgraded from Noir 0.36.0 to **1.0.0-beta.26**: artifacts must be exported with the matching
  `nargo`, and the MSRV is now Rust 1.89. Witness bytes use the new msgpack encoding. The
  `proptest` patch is gone.
- `simulate` now runs in-process from the artifact embedded by `nrg!`, decoded once and cached in a
  `static`. The `noir-runner` dependency is gone: no per-call file I/O or manifest resolution, and
  no panic on undecodable artifacts.
- `Client` is now `noi::Client` (re-exported in each generated module), built with `Client::new()`
  plus `with_output`, `with_oracle_resolver`, and `with_program_dir`. It no longer needs the Noir
  package path.
- Generated code only references `::noi`, so crates using `nrg!` no longer need `noi-core` or
  `anyhow` as direct dependencies. Generated functions return `noi::Result`.
- `noi-build`: new `BuildOptions::from_env(dir)` / `BuildOptions::new(dir, out)` builders and
  `ExportSummary::source`. Artifacts are staged from `<package>/export/`, where `nargo export`
  writes them (previously `target/` was preferred, which could pick up unrelated JSON).
- Export schema: types are decoded with serde mirrors of `noirc_abi`. `StructType::name` is now
  `path`. Public structs are `#[non_exhaustive]`.
- Relative `export_dir` paths in `nrg!` are resolved against the invoking crate's
  `CARGO_MANIFEST_DIR`.

### Added
- Proving with Barretenberg (feature `prove`): `noi::Prover` (pinned to bb 5.0.0, with
  `VerifierTarget`s for EVM, recursive Noir, and Starknet), `noi::Proof` with typed decoding of
  the proven return value, cached verification keys, and Solidity verifier generation. `nrg!`
  generates `prove`, `verify`, `verified_output`, `verification_key`, and `solidity_verifier` per
  function. `scripts/install_bb.sh` installs `bb` (handling Rosetta shells on Apple Silicon).
- `noi::ExecutionError` carrying the Noir `assert` message; `execute()` and `program()` per
  generated function; `Execution::witness_bytes()` for use with `bb prove -w`.
- ABI support for `databus` visibility, `str<N>`, and `u1`; `ExportFunction::return_visibility`,
  `noir_version`, `public_parameters()`; `collect_structs`; `load_export_file`.
- Codegen: Rust keywords are escaped, Noir structs with the same short name get path-based names,
  `Default` is only derived when every field supports it (arrays longer than 32 broke the build),
  invalid integer widths produce a compile error, doc comments carry Noir types and visibility.
- `noi-build`: `NOI_SKIP_NARGO`, rerun on new files under `src/` and on env changes, warnings when
  artifacts come from a different Noir version, and `nargo` stderr in fallback warnings.
- `noi::NOIR_VERSION`; full API documentation with runnable doctests; CI for fmt, clippy, tests
  (all feature sets), and docs.

### Fixed
- `noi-build` printed a "pre-exported artifacts" warning on every build, even when `nargo export`
  succeeded.
- `scripts/refresh_exports.sh` could not find `nargo` after installing it with `cargo install`,
  and accepted a `nargo` on `PATH` of any version.
- The export loader silently skipped unreadable directory entries and functions without parameters
  or a return value.

## [0.0.1] - 2025-11-21
### Added
- Initial workspace bootstrap with `noi-core`, `noi-build`, `noi-macro`, and `noi` crates.
- Noir demo program and Rust example app demonstrating generated bindings.
- Renamed the proc macro to `nrg!` (dropping all legacy aliases).
- Reorganized the workspace so the demo Rust app and Noir project live together under `examples/demo/`.
- Added a dark-pool-inspired example (`examples/dark-pool/`) that mirrors more realistic structs and arrays.
