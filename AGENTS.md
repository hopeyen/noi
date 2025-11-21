# noi contributor guide

This repository hosts **`noi`**, a Rust workspace that generates Alloy-style, strongly typed bindings for Noir `#[export]` functions. All implementation work should align with the milestones described in `milestone.md` and the upstream Noir documentation at [https://noir-lang.org/docs](https://noir-lang.org/docs).

## Required references
- Read `milestone.md` before starting any task to understand scope, deliverables, and acceptance criteria for the current milestone.
- Use the official Noir docs (`https://noir-lang.org/docs`) as the source of truth for language features, `nargo` behavior, and export schema expectations.

## Repository expectations
- Maintain the workspace layout described in the milestone file (`crates/`, `examples/demo/` with its embedded `noir/`, etc.).
- Keep crate metadata, feature flags, and public APIs consistent with the specification; do not introduce additional crates without coordinating in the milestone plan.
- Prefer `anyhow` for error handling across crates and keep macro diagnostics user friendly.

## Workflow rules
- Start work by restating the relevant milestone goal and outlining the file-level changes you plan to make.
- Implement the smallest vertical slices that satisfy a milestone task; leave clear `todo!()` comments only when the milestone explicitly defers work.
- When touching Noir artifacts, confirm type names, visibility, and backend behavior against the Noir docs.
- Document assumptions directly in code comments or module-level docs when behavior is inferred from Noir tooling.

## Coding guidelines
- Target stable Rust and keep abstractions minimal, ergonomic, and testable.
- Follow the existing module organization: `noi-core` for runtime/export schema, `noi-build` for build helpers, `noi-macro` for proc macros, and `noi` for re-exports.
- Keep code generation deterministic; avoid randomness or environment-dependent ordering.
- Ensure new features gate optional dependencies with Cargo features (`runner`, `prove`, `field`) as specified in the milestones.
- Backwards compatibility is not a constraint—feel free to refactor public APIs, file layout, and config knobs between milestones if that improves delivery speed or quality.

## Rust standards (efficient, usable, robust)
- **Frictionless APIs:** Favor explicit, typed builders and `Result`-returning functions; no hidden global state. Every public API must have a doctest or example showing canonical usage.
- **Zero-cost ergonomics:** Reuse buffers, iterate without allocations, and pass slices/`&str` references whenever data ownership is not required. Avoid `clone()` unless justified by comments.
- **Deterministic error handling:** Use `anyhow::Context` (libraries) or `thiserror` (typed errors) so callers get actionable diagnostics. Never swallow errors—propagate with context.
- **Thread-safe by default:** Types that may cross threads should implement `Send`/`Sync` (when sound) or document why not. Use `Arc` + `RwLock`/`Mutex` only when contention analysis is included in the PR description.
- **Visibility hygiene:** Keep modules private unless exporting is necessary. Public structs must `#[non_exhaustive]` when future fields are expected; otherwise, document stability expectations.
- **Resilience:** Guard file/network/process I/O with retries or validation where Noir tooling is involved. Validate user inputs (paths, feature flags) before invoking expensive operations.
- **Performance proofs:** For non-trivial data structures or codegen loops, include benchmark notes or complexity comments referencing the relevant section of `milestone.md` or `https://noir-lang.org/docs` when the Noir schema drives costs.

## Testing & quality
- Add unit tests for new modules and cover export-schema parsing with golden fixtures under `noi-core/tests/fixtures/`.
- Write integration or doc tests when milestones call for end-to-end coverage (e.g., macro invocation, example app usage).
- Before submitting changes, run `cargo fmt`, `cargo clippy --all --all-features -D warnings`, and the most specific `cargo test` targets affected by your changes. Run `cargo test --workspace` prior to milestone hand-off.
- Keep example Noir programs and Rust clients building; update fixtures when Noir schema changes (verify against Noir docs).

## Documentation
- Update `README.md`, crate-level docs, and example comments whenever public APIs or expected workflows change.
- Note any required tool versions (Rust toolchain, `nargo`, Noir backend) in the relevant docs or configuration files.

## Communication
- Surface blockers early, especially when Noir tooling diverges from the documented behavior.
- When unsure about Noir semantics, cite the relevant section from `https://noir-lang.org/docs` in your PR or code comments to aid review.
