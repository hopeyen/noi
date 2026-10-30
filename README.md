# noi

`noi` turns the JSON artifacts produced by
[`nargo export`](https://noir-lang.org/docs/reference/nargo_commands#nargo-export) into strongly
typed Rust bindings, much as Alloy's `sol!` does for Solidity. You call, simulate, prove, and verify
Noir `#[export]` functions from Rust as ordinary function calls, without hand-built witness maps.

```rust
noi::nrg!(module = demo); // generated from noir/export/*.json by noi-build

fn main() -> noi::Result<()> {
    let client = demo::Client::new();
    let sum = demo::add::simulate(&client, &demo::add::Args { a: 5, b: 6 })?;
    assert_eq!(sum, 11);
    Ok(())
}
```

The full API guide is in the [`noi` crate docs](https://docs.rs/noi).

## Why `noi`?
- **Strong typing:** every exported function gets an `Args` struct and an `Output` type, and every
  Noir struct a Rust struct. ABI drift is a compile error.
- **Runs in-process:** `simulate` solves the circuit with the ACVM inside your process. The
  artifact is embedded in your binary, so neither `nargo` nor the Noir sources are needed at run
  time.
- **Proving built in:** with the `prove` feature, `prove`, `verify`, `verified_output`, and Solidity
  verifier generation drive Barretenberg's `bb` CLI (UltraHonk).
- **Cargo-native build flow:** `noi-build` runs `nargo export` from `build.rs` without touching your
  source tree, and falls back to committed artifacts when `nargo` is unavailable.
- **Useful errors:** a failing Noir `assert(cond, "message")` comes back as `noi::ExecutionError`
  with the message, format strings filled in.
- **Published on crates.io:** the runtime depends only on Noir crates that are on crates.io.

## Requirements
- Rust 1.89 or newer (the MSRV of the Noir 1.0 crates). The repository pins 1.92 for development
  (`rust-toolchain.toml`).
- To export artifacts: a stable Noir **1.0.x** `nargo` (`noirup --version 1.0.0` recommended).
  Artifacts from prereleases (such as `1.0.0-beta.N`) or other major/minor releases are rejected
  with a message naming both versions; see `noi::noir_version_compatible`.
- To prove (feature `prove`): Barretenberg `bb` **6.0.0**, the release paired with Noir 1.0.0. Run
  `bbup -v 6.0.0`, or `scripts/install_bb.sh`, which installs into `target/bb-bin/` without
  editing your shell configuration.

Without `nargo`, `noi-build` stages the artifacts committed in `<package>/export/` and prints a
`cargo:warning`, so `cargo build` still works (it uses whatever you last committed).

## Using `noi` in your project
1. Create a Noir **library** package (`nargo new --lib noir`) and mark functions with `#[export]`.
2. Add the dependencies:
   ```toml
   [dependencies]
   noi = "0.0.2"                       # features: "prove" for proving

   [build-dependencies]
   noi-build = "0.0.2"
   ```
3. Add a `build.rs`:
   ```rust
   fn main() {
       noi_build::BuildOptions::from_env("noir") // relative to Cargo.toml
           .and_then(|opts| opts.export())
           .unwrap_or_else(|err| panic!("failed to export Noir artifacts: {err:#}"));
   }
   ```
4. Generate and call the bindings:
   ```rust
   noi::nrg!(module = demo); // reads NOI_EXPORT_DIR, set by noi-build

   fn main() -> noi::Result<()> {
       let client = demo::Client::new();
       let sum = demo::add::simulate(&client, &demo::add::Args { a: 5, b: 6 })?;
       assert_eq!(sum, 11);
       Ok(())
   }
   ```
5. Optionally run `nargo export` in the Noir package and commit `export/`, so machines without
   `nargo` (CI, docs.rs, teammates) can still build.

Without a build script, point the macro at a directory instead:
`noi::nrg!(module = demo, export_dir = "noir/export")`. Relative paths are resolved against the
crate's `Cargo.toml`.

## What gets generated

For `fn add(a: u32, b: u32) -> u32`, `nrg!(module = demo)` produces `demo::add::{Args, Output,
simulate, execute, program}`, plus `prove`, `verify`, `verified_output`, `verification_key`, and
`solidity_verifier` with the `prove` feature. Each Noir struct becomes `demo::<Name>`, and
`demo::Client` holds execution settings such as oracle handlers. See
[the crate docs](https://docs.rs/noi) for the exact signatures, the Noir-to-Rust type table, and
naming rules for keywords, collisions, and generic structs.

## Interaction patterns

### Pattern A — Simulation in a backend

```
[request] → [Rust backend + noi] → simulate() → [typed result]
```

```rust
noi::nrg!(module = finance);

fn interest(principal: u64, rate: u64) -> noi::Result<u64> {
    let client = finance::Client::new();
    finance::interest::simulate(&client, &finance::interest::Args { principal, rate })
}
```

### Pattern B — Backend proves, chain verifies *(feature `prove`)*

```
[user inputs] → [Rust backend + noi] → prove() → [proof + public output] → [Solidity verifier]
```

Noir only allows `pub` on a program's `main`, so `#[export]` functions have no public parameters:
**a proof's public inputs are the function's return value**. Return whatever the verifier must
see:

```noir
#[export]
fn login(app_id: Field, secret: Field) -> Field {
    assert(hash(secret) == REGISTERED); // `hash`: e.g. Poseidon2 from the noir-lang/poseidon library
    app_id // public: the proof is bound to this app
}
```

```rust
use noi::{Prover, VerifierTarget};
noi::nrg!(module = auth);

fn prove_login(app_id: noi::FieldElement, secret: noi::FieldElement) -> noi::Result<noi::Proof> {
    let client = auth::Client::new().with_prover(Prover::new().with_target(VerifierTarget::Evm));
    auth::login::prove(&client, &auth::login::Args { app_id, secret })
}

// Once, at deploy time:
// std::fs::write("Verifier.sol", auth::login::solidity_verifier(&client)?)?;
```

### Pattern C — Rust verifies incoming proofs *(feature `prove`)*

```
[browser prover (bb.js)] → (proof, public_inputs) → [Rust backend + noi] → verified_output()
```

```rust
use noi::{Proof, VerifierTarget};
noi::nrg!(module = attest);

fn accept(proof: Vec<u8>, public_inputs: Vec<u8>) -> noi::Result<attest::attest::Output> {
    let client = attest::Client::new();
    let proof = Proof::from_parts(proof, public_inputs, VerifierTarget::Default);
    // Fails unless the proof is valid; returns the proven statement, typed.
    attest::attest::verified_output(&client, &proof)
}
```

### Pattern D — Schema sharing only

```
[Noir prover elsewhere] ← schema → [Rust backend + noi (default-features = false)]
```

With `default-features = false`, `noi` generates only the types, without the Noir execution
crates. Use them to validate and route data shaped like your circuit's inputs and outputs:

```rust
noi::nrg!(module = risk);

fn check_score(score: &risk::score::Output) -> Result<(), String> {
    if *score > 10_000 { Err("score out of bounds".into()) } else { Ok(()) }
}
```

## Configuration

| Variable | Used by | Effect |
|---|---|---|
| `NOI_EXPORT_DIR` | `nrg!` | Artifact directory; set automatically by `noi-build`. |
| `NOI_NARGO_BIN` | `noi-build` | `nargo` to run (default: `nargo` on `PATH`). |
| `NOI_SKIP_NARGO=1` | `noi-build` | Never run `nargo`; stage the committed `export/` artifacts. |
| `NOI_VERBOSE=1` | `noi-build` | Log each step (visible with `cargo build -vv`). |
| `NOI_DEBUG=1` | `nrg!` | Write the expanded code to `target/noi/<crate>/<module>.rs`. |
| `NOI_BB_BIN` | `prove` | `bb` to run (default: `bb` on `PATH`). |
| `NOI_CACHE_DIR` | `prove` | Verification-key cache (default: the per-user cache directory). |

The verification-key cache is trusted: anyone who can write it can make forged proofs verify. On
Unix, `noi` creates it with mode `0700` and refuses a directory that is not owned by the current
user or that group/others can write. Other platforms get no such check, so keep `NOI_CACHE_DIR` in
a per-user directory there.

## Limitations
- Proving shells out to the `bb` CLI, which must be installed wherever proofs are made or
  verified.
- bb's UltraHonk prover only accepts single-circuit programs: functions that call `#[fold]`
  functions can be simulated but not proved (the prover says so before running `bb`).
- `println` output from Noir is discarded; handle the `print` oracle with `Client::with_oracle` to
  capture it raw.
- Each instantiation of a generic Noir struct (`Option<u8>`, `Option<u32>`) is a separate Rust
  struct (`NoirOption`, `NoirOption2`), because Noir's ABI does not record generic arguments.
- Tuples are limited to 12 elements (the standard library's trait coverage).
- Adding a new artifact to an explicit `export_dir` is not noticed by cargo; use `noi-build`, or
  touch the file that invokes `nrg!`.
- `noi-build` exports one Noir library package; for a Nargo workspace, point it at the member.

## Workspace layout

```
crates/
  noi-core     # export schema, runtime (ABI codec + ACVM executor), prover
  noi-build    # build.rs helper that runs nargo export and stages artifacts
  noi-macro    # nrg! proc macro
  noi          # facade: re-exports + the user guide
examples/
  demo/        # noi-example-app: every feature, including proving (`--features prove`)
  dark-pool/   # noi-dark-pool-example: structs and arrays from an order-matching circuit
scripts/
  refresh_exports.sh   # re-export example artifacts with the pinned nargo
  install_bb.sh        # install the pinned bb into target/bb-bin/
  check_exports.py     # CI: fail if committed artifacts are stale
  publish.sh           # publish all crates in dependency order
```

A proc-macro crate can export only procedural macros, so `nrg!` lives in `noi-macro`. The build
helper (`noi-build`) is an ordinary library that depends only on the schema part of `noi-core`, so
`build.rs` does not compile the ACVM runtime or the prover. `noi` re-exports everything.

## Running the examples

```bash
cargo run -p noi-example-app                   # simulate add/mix/move_point
cargo run -p noi-example-app --features prove  # also prove and verify (needs bb)
cargo run -p noi-dark-pool-example
```

See [`examples/README.md`](examples/README.md) for what each example covers.

## Development
- Format: `cargo fmt --all`
- Lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Test: `cargo test --workspace`; with proving: `NOI_BB_BIN=$(scripts/install_bb.sh) cargo test --workspace --features noi/prove,noi-core/prove`
- Package: `cargo package --workspace --exclude noi-example-app --exclude noi-dark-pool-example`
  builds every crate from its packaged sources, as `cargo publish` does; then
  `scripts/test_packaged.sh` runs each packaged crate's own tests.
- MSRV: `cargo +1.89 check --workspace --all-targets --all-features`.

Read `AGENTS.md` for engineering standards and `milestone.md` for scope.

### Refreshing Noir exports
Run `scripts/refresh_exports.sh` after changing Noir code under `examples/*/noir/`. It fetches
`nargo` 1.0.0 (prebuilt release, or `cargo install` as a fallback), re-exports both examples,
makes the artifacts reproducible (repository-relative source paths), and copies the demo artifacts
into the `noi` and `noi-core` test fixtures. CI runs it followed by `scripts/check_exports.py` and
fails if the committed artifacts differ.

### Releasing
1. Bump `version` in the root `Cargo.toml` and the internal dependency versions.
2. Update `CHANGELOG.md`.
3. Run the package command above, then `scripts/publish.sh --dry-run`, then `scripts/publish.sh`.

## Troubleshooting
- **"artifact … was built by Noir X"**: re-export with `nargo` 1.0.0.
- **"no handler for oracle `name`"**: the circuit calls an oracle; register it with
  `Client::with_oracle`.
- **"`bb` is bb X, but noi requires bb 6.0.0"**: install the paired release, or opt out with
  `Prover::allow_any_bb_version(true)`.
- **"nrg!: no export directory"**: call `noi_build` from `build.rs` (it sets `NOI_EXPORT_DIR`),
  or pass `export_dir = "path/to/noir/export"` to `nrg!`.
- **"… calls `#[fold]` functions … only supports single-circuit programs"**: remove `#[fold]` from
  functions you need to prove.

## Roadmap
- **v0.0.1:** export schema, `nrg!` codegen, `noi-build` (schema only on crates.io).
- **v0.0.2:** Noir 1.0.0; an in-process runtime on published crates only, with oracle handlers
  and assertion messages; proving and verification with bb 6.0.0; generic-struct and
  collision-safe codegen; a `noi-build` that never writes the source tree; full API docs and CI.
- **Next:** in-process proving (Barretenberg bindings instead of the CLI), recursion helpers,
  optional `serde` derives, mapping Noir `Option<T>` to Rust `Option<T>`, and source locations for
  failed assertions.
