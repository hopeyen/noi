# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2025-11-21
### Added
- Initial workspace bootstrap with `noi-core`, `noi-build`, `noi-macro`, and `noi` crates.
- Noir demo program and Rust example app demonstrating generated bindings.
- Renamed the proc macro to `nrg!` (dropping all legacy aliases).
- Reorganized the workspace so the demo Rust app and Noir project live together under `examples/demo/`.
- Added a dark-pool-inspired example (`examples/dark-pool/`) that mirrors more realistic structs and arrays.
