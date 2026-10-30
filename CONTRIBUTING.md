# Contributing to noi

Thanks for helping build the Noir-to-Rust bridge! Please read `AGENTS.md` and `milestone.md` for the current engineering plan and coding conventions.

## Prerequisites
- Rust 1.89+ (`rust-toolchain.toml` tracks stable)
- `nargo` 1.0.0 to re-export example artifacts (`scripts/refresh_exports.sh` fetches it)
- `bb` 6.0.0 to run the proving tests (`scripts/install_bb.sh` fetches it)
- `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
  `cargo test --workspace` must pass before opening a pull request.

## Workflow
1. Choose the milestone or task you want to tackle and restate it in your PR description.
2. Implement the smallest vertical slice that satisfies the acceptance criteria.
3. Update docs, examples, and fixtures that reference your changes.
4. Add or update tests; prefer targeted unit tests plus doc tests for the macro API.
5. Run the full hygiene stack before submission.

## Reporting issues
If `nargo export` changes its JSON shape or runner behavior, please file an issue with:
- Noir version (`nargo --version` output)
- The failing artifact (zip or gist)
- Observed vs expected behavior referencing the Noir docs (<https://noir-lang.org/docs>)
