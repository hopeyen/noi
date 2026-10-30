# noi

`noi` takes the JSON artifacts produced by [`nargo export`](https://noir-lang.org/docs/noir/cli/nargo/#exporting-artifacts)
and turns them into strongly typed Rust bindings—think Alloy’s `sol!`, but for Noir circuits. The
workspace ships with runtime helpers, a build-script tool, a proc macro, and a runnable example so a
backend team can call, simulate, and eventually prove Noir `#[export]` functions without copy/paste
witness maps.

## Why `noi`?
- **Strong typing:** every Noir function produces an `Args` struct, an `Output` alias, and a
  visibility-aware `Inputs` wrapper.
- **Cargo-native build flow:** `noi-build` stages exports inside `build.rs`, copies JSON into
  `OUT_DIR`, and sets `NOI_EXPORT_DIR` for the macro.
- **Single macro integration:** `nrg!` emits per-function modules and a shared `Client` that
  captures the Noir program directory.
- **Deterministic codegen:** macro output depends solely on checked-in artifacts, producing
  reviewable diffs and reproducible builds.
- **Self-contained demo:** the repository ships with a demo Rust app + Noir program in one place so
  newcomers can read and run the full workflow with a single `cargo run`.

## Design goals & non-goals

**Goals**
- Give Rust devs Alloy-style, type-safe bindings over Noir `#[export]` functions.
- Make simulation/proving/verification calls feel like normal Rust method calls instead of manual
  witness plumbing.
- Treat the exported schema as the single source of truth that backend, frontend, and smart-contract
  teams can share.
- Keep codegen deterministic so schema changes show up as compile errors and clean diffs.

**Not trying to do (yet)**
- Replace the Noir toolchain or compile `.nr` files—`noi` assumes `nargo export` already ran.
- Support every backend permutation out of the gate; milestones start with the common runner and a
  single proving backend.
- Preserve backwards compatibility at all costs. If an API/layout tweak makes things easier, we’ll
  take it.

## Requirements
- Rust stable toolchain (pinned via `rust-toolchain.toml`).
- Rust 1.89+ (required by the Noir 1.0 crates).
- For proving (feature `prove`): Barretenberg `bb` **5.0.0** (`bbup -v 5.0.0`, or
  `scripts/install_bb.sh`, which does not touch your shell config).
- Noir toolchain with `nargo` **1.0.0-beta.26** (`noirup --version 1.0.0-beta.26`). The runtime
  decodes artifacts with the Noir crates at the same tag, so the `nargo` you export with must match
  `noi::NOIR_VERSION`; mismatches are reported at build time (warning) and when an artifact loads.
  Nargo rejects prerelease versions in `compiler_version`, so the pin lives in `noi` rather than
  in `Nargo.toml`.
- If `nargo` isn’t on PATH, `noi-build` stages the committed artifacts in `<package>/export/` and
  prints a `cargo:warning`, so `cargo check` keeps working (but you won’t pick up schema changes
  until you run `nargo export`). Set `NOI_SKIP_NARGO=1` to do this on purpose.

## Workspace layout

```
crates/
  noi-core     # export schema + runtime glue
  noi-build    # build.rs helper that stages exports into OUT_DIR
  noi-macro    # `nrg!` proc macro
  noi          # facade re-exporting runtime helpers + macro
examples/
  demo/
    Cargo.toml
    build.rs
    src/main.rs
    noir/       # embedded Noir project used by the demo (with fallback exports)
  dark-pool/
    Cargo.toml
    build.rs
    src/main.rs
    noir/       # structs/arrays inspired by Mezcal’s dark-pool prototype
```

Bundling the Noir sources inside each example crate means `cargo run -p ...` has everything it needs
without juggling extra paths. The `dark-pool` variant nods to
[Mezcal](https://github.com/nemi-fi/mezcal) and shows how realistic structs/arrays look after
codegen.

## Architecture snapshot
- `noi-core`: owns the export schema (`ExportFunction`, `Param`, `TypeRepr`), filesystem loader,
  and the runtime (`Program`, `Client`, value conversions) that executes artifacts in-process.
- `noi-build`: intended for `build.rs`; runs `nargo export`, mirrors JSON into
  `target/.../noi_exports/`, and sets `NOI_EXPORT_DIR`.
- `noi-macro`: implements `nrg!`, reading staged JSON to generate typed modules, structs, and
  client helpers.
- `noi`: thin facade that re-exports runtime types and the macro.

### Why multiple crates instead of a monolith?
- Proc macros (`noi-macro`) must live in their own crate and can’t depend on the runtime crate, so
  the macro and the facade (`noi`) stay separate.
- Build scripts can’t link against proc-macro crates, so `noi-build` is a lightweight helper that
  downstream `build.rs` files can depend on without weird coupling.
- `noi-core` holds the shared schema/runtime types so both the macro and the facade reuse the same
  logic without circular deps.
Collapsing everything into one crate would run into Cargo rules immediately, so the “multi-crate
but colocated” structure keeps things navigable and practical.

## Crate usage examples

Most folks will just `cargo add noi`, but if you need to hook into a specific layer, here’s how each
crate looks in isolation.

### `noi-build` (build.rs helper)

```rust
// build.rs
use std::{env, path::PathBuf};

fn main() {
    noi_build::BuildOptions::from_env("noir") // relative to Cargo.toml
        .expect("OUT_DIR is set by cargo")
        .export()
        .expect("nargo export failed");
}
```

### `noi-macro` / `nrg!`

```rust
use noi::nrg;

nrg!(module = voting, export_dir = "noir/export"); // relative to CARGO_MANIFEST_DIR

fn main() -> noi::Result<()> {
    let client = voting::Client::new();
    let result = voting::tally::simulate(&client, voting::tally::Args { yes: 10, no: 4 })?;
    println!("tally = {result}");
    Ok(())
}
```

### `noi-core` (raw schema loader)

```rust
use noi_core::load_export_dir;

fn print_exports() -> anyhow::Result<()> {
    let exports = load_export_dir("./target/noi_exports")?;
    for function in exports {
        println!("{}", function.signature());
    }
    Ok(())
}
```

### `noi` facade

The `noi` crate re-exports both `nrg!` and the runtime helpers so downstream code usually depends on
it exclusively: `cargo add noi --path crates/noi`.

## Interaction patterns with Noir

The diagrams below highlight where `noi` sits when Rust talks to Noir, frontends, and verifiers.
Each pattern lines up with the design goals above and includes a short snippet from `nrg!` output.

### Pattern A — Pure simulation (no proof)

```
[Client request] → [Rust backend + noi::nrg!] → [Noir runner] → [Result JSON]
```

```rust
use noi::nrg;
nrg!(module = finance);

fn check() -> noi::Result<u64> {
    let client = finance::Client::new();
    let out = finance::interest::simulate(&client, finance::interest::Args { principal: 1_000, rate: 350 })?;
    Ok(out)
}
```

### Pattern B — Backend proves, chain verifies *(feature `prove`)*

```
[User inputs] → [Rust backend + noi] → prove() → [proof + public output] → [Solidity verifier]
```

`#[export]` functions cannot take `pub` parameters (Noir only allows `pub` on `main`), so a
proof's public inputs are the function's **return value**. Return whatever the verifier must see:

```noir
#[export]
fn login(app_id: Field, secret: Field) -> Field {
    assert(hash(secret) == REGISTERED); // `hash`: e.g. Poseidon2 from the noir-lang/poseidon library
    app_id // public: the proof is bound to this app
}
```

```rust
use noi::{nrg, Prover, VerifierTarget};
nrg!(module = auth);

fn prove_login(app_id: noi::FieldElement, secret: noi::FieldElement) -> noi::Result<noi::Proof> {
    let client = auth::Client::new().with_prover(Prover::new().with_target(VerifierTarget::Evm));
    auth::login::prove(&client, auth::login::Args { app_id, secret })
}

// Once, at deploy time:
// std::fs::write("Verifier.sol", auth::login::solidity_verifier(&client)?)?;
```

### Pattern C — Rust verifies incoming proofs *(feature `prove`)*

```
[Browser/wallet prover (bb.js)] → (proof, public_inputs) → [Rust backend + noi] → verified_output()
```

```rust
use noi::{nrg, Proof, VerifierTarget};
nrg!(module = attest);

fn accept(proof: Vec<u8>, public_inputs: Vec<u8>) -> noi::Result<attest::attest::Output> {
    let client = attest::Client::new();
    let proof = Proof::from_parts(proof, public_inputs, VerifierTarget::Default);
    // Fails unless the proof is valid; returns the proven statement, typed.
    attest::attest::verified_output(&client, &proof)
}
```

### Pattern D — Coordinator only (schema sharing)

```
[Noir prover in WASM] ← schema → [Rust backend + noi] → validates public inputs / relays proofs
```

Even if Rust never calls `simulate`/`prove`, the generated structs describe the Noir interface:

```rust
use noi::nrg;
nrg!(module = risk);

fn validate_public_inputs(data: &risk::score::PublicInputs) -> noi::Result<()> {
    if data.score > 10_000 { anyhow::bail!("score out of bounds") }
    Ok(())
}
```

## Production & publication status
`noi` is **pre-release/alpha**. Codegen and `simulate` work today: the macro embeds each
artifact in your binary and `noi-core` executes it in-process with the ACVM, so neither `nargo` nor
the Noir sources are needed at run time. With the `prove` feature, each function also gets
`prove`/`verify`/`verified_output`/`verification_key`/`solidity_verifier`, backed by Barretenberg's
`bb` CLI (UltraHonk).

## Using `noi` in your project
1. Add dependencies (path-based until the crates are published):
   ```bash
   cargo add noi --path crates/noi
   cargo add noi-build --path crates/noi-build
   ```
2. Create or update your project’s `build.rs`:
   ```rust
   fn main() {
       noi_build::BuildOptions::from_env("path/to/noir/project")
           .expect("OUT_DIR is set by cargo")
           .export()
           .expect("nargo export failed");
   }
   ```
3. Invoke `nrg!` inside your crate:
   ```rust
   use noi::nrg;

   nrg!(module = demo); // reads NOI_EXPORT_DIR emitted by build.rs

   fn main() -> noi::Result<()> {
       let client = demo::Client::new(); // or .with_output(true) to see Noir println!
       let args = demo::add::Args { a: 5, b: 6 };
       let sum = demo::add::simulate(&client, args)?;
       assert_eq!(sum, 11);
       Ok(())
   }
   ```

## Running the bundled demo
1. Install `nargo` (optional but recommended for fresh exports).
2. From the repo root, run:
   ```bash
   cargo run -p noi-example-app
   ```
   The example crate lives at `examples/demo/` and embeds its Noir project under `examples/demo/noir`.
   The build script stages exports from that directory, the macro generates bindings, and the binary
   executes real Noir code (falling back to the checked-in JSON fixtures when `nargo` is missing).
   See `examples/README.md` for a deeper walkthrough of the demo.

Want something closer to production? Try the dark-pool flavored example:

```bash
cargo run -p noi-dark-pool-example
```
It follows the same structure but uses Noir circuits inspired by the
[Mezcal](https://github.com/nemi-fi/mezcal) project to showcase structs, arrays, and public/private
inputs.

## Detailed behavior & knobs
- `NOI_EXPORT_DIR`: automatically set by `noi-build`. Bypass it with
  `nrg!(module = demo, export_dir = "noir/export")`; relative paths resolve against the invoking
  crate's `CARGO_MANIFEST_DIR`.
- `NOI_DEBUG=1`: the macro writes the expanded code of each invocation to
  `target/noi/<module>.rs` for inspection.
- `NOI_NARGO_BIN`: path to a specific `nargo` binary for environments that pin toolchains.
- `NOI_SKIP_NARGO=1`: never run `nargo`; stage the committed `export/` artifacts.
- `NOI_BB_BIN`: the `bb` executable for proving (default: `bb` on `PATH`).
- `NOI_CACHE_DIR`: where verification keys are cached (default: per-user cache dir, e.g.
  `~/.cache/noi`). It must be private: anyone who can write it can make forged proofs verify, so
  `noi` refuses directories that are group/world-writable or owned by another user.
- `NOI_VERBOSE=1`: log `noi-build` steps to stderr (visible with `cargo build -vv`).
- Errors: a failing Noir `assert(cond, "msg")` surfaces as `noi::ExecutionError` with `message()`
  returning `"msg"`; downcast the `noi::Result` error to match on it.
- Visibility metadata: parameters tagged `public` go into the generated `PublicInputs`, while
  everything else lands in `PrivateInputs`. The macro also provides `From<Args> for Inputs` and the
  reverse, so you can ergonomically switch representations.

## Limitations & open work
- Proving shells out to the `bb` CLI (Barretenberg 5.0.0, paired with Noir 1.0.0-beta.26). It must
  be installed where proofs are made or verified; `scripts/install_bb.sh` fetches it into
  `target/bb-bin/`.
- Exported functions have no public parameters (a Noir rule), so `PublicInputs` is normally empty;
  proofs are public only in their return value.
- Type coverage: `bool`, `u1`–`u128`, `i8`–`i64`, `Field`, `str<N>`, fixed arrays, tuples, and
  structs (nested). Noir structs with the same short name in different modules get path-based Rust
  names (`orders::Order` → `OrdersOrder`).
- `noi-build` expects a `Nargo.toml` with sources under `src/`. For Nargo workspaces, point
  `BuildOptions` at the member package.

## Development workflow
- Format: `cargo fmt`
- Lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Test: `cargo test --workspace`
- Review the stricter engineering standards in `AGENTS.md` before opening a PR. The milestones
  listed in `milestone.md` define the scope and acceptance tests for each iteration.

### Refreshing Noir exports
- Run `scripts/refresh_exports.sh` whenever you change the Noir sources under `examples/*/noir/`.
  The script downloads a prebuilt `nargo` release from GitHub (or reuses an existing binary) and
  runs `nargo export` for the demo and dark-pool programs so the checked-in `export/` artifacts stay
  in sync with the Rust fixtures. Set `NARGO_BIN=/path/to/nargo` if you already have a binary handy.
- The script also copies the demo artifacts into `crates/noi/tests/fixtures/demo/`, which the
  `noi` crate's doctests and integration tests use.
- CI executes the same script (see `.github/workflows/refresh-exports.yml`) and fails when the
  committed exports drift, so keeping `export/` up to date locally avoids review churn.

## Publishing crates
- `noi-core`, `noi-build`, and `noi-macro` must be published before `noi` because the facade depends
  on them.
- Use the helper script to publish in order (pass `--dry-run` while testing):

  ```bash
  chmod +x scripts/publish.sh
  scripts/publish.sh --dry-run
  scripts/publish.sh          # real publish once satisfied
  ```

  The script calls `cargo publish -p <crate>` for each crate in dependency order, so the final crate
  on crates.io is `noi`—the user-facing package name you asked for.

## Troubleshooting
- **`nargo` missing:** builds succeed using the fallback exports but log a warning. Install `nargo`
  to regenerate fresh JSON.
- **`NOI_EXPORT_DIR` not set:** ensure `noi-build::export` ran inside your `build.rs`. It emits the
  proper env var automatically.
- **Unsupported Noir type:** extend `TypeRepr`, add fixtures under `crates/noi-core/tests/fixtures/`,
  and expand the macro’s type mapping.

## Roadmap
- **v0.0.1:** export schema, `nrg!` codegen, `noi-build`, `simulate` via `noir-runner`.
- **v0.0.2 (this release):** Noir 1.0.0-beta.26, in-process runtime built on the Noir crates (no `noir-runner`, no disk
  I/O per call, cached artifacts), typed `ExecutionError` with assertion messages, witness access
  for external provers, `databus`/`str<N>`/`u1` support, keyword-safe and collision-free codegen,
  generated code that only needs `noi` as a dependency, `noi-build` fixes, full API docs, CI, and
  proving (`prove`/`verify`/`verified_output`/Solidity verifiers via `bb`, cached verification keys).
- **Next:**
  1. Proving without the CLI (link Barretenberg directly or via `bb`'s msgpack API) and
     recursive-proof helpers.
  2. Optional `serde` derives on generated types, and Noir `assert` source locations in errors.
