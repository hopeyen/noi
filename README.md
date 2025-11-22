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
- Noir toolchain with `nargo` ≥ `1.0.0-beta.15`. If `nargo` isn’t on PATH, `noi-build` quietly
  copies the pre-exported fixtures under `examples/*/noir/export`, so `cargo check` keeps
  working (but you won’t pick up schema changes until you run `nargo export`).

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
  and future runtime hooks (`simulate`, `prove`, `verify`).
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
    let opts = noi_build::BuildOptions {
        program_dir: PathBuf::from("./noir"),
        out_dir: PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")),
    };
    noi_build::export(&opts).expect("nargo export failed");
}
```

### `noi-macro` / `nrg!`

```rust
use noi::nrg;

nrg!(module = voting, export_dir = "./target/noi_exports");

fn main() -> anyhow::Result<()> {
    let client = voting::Client::new("./noir");
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

fn check() -> anyhow::Result<u64> {
    let client = finance::Client::new("./noir");
    let out = finance::interest::simulate(&client, finance::interest::Args { principal: 1_000, rate: 350 })?;
    Ok(out)
}
```

### Pattern B — Backend proves, chain verifies

```
[User inputs]
     ↓
[Rust backend + noi]
     │ prove()
     ↓
[Proof + public inputs]
     ↓
[Smart contract verifier]
```

```rust
use noi::nrg;
nrg!(module = auth);

fn prove_login(req: LoginRequest) -> anyhow::Result<ProofPayload> {
    let client = auth::Client::new("./noir");
    let inputs = auth::login::Inputs {
        public: auth::login::PublicInputs { app_id: req.app_id },
        private: auth::login::PrivateInputs { secret: req.secret },
    };
    let proof = auth::login::prove(&client, inputs.clone())?;
    Ok(ProofPayload { proof, public_inputs: inputs.public })
}
```

### Pattern C — Rust verifies incoming proofs

```
[Browser/wallet prover] → (proof, public_inputs) → [Rust backend + noi] → verify() → decision
```

```rust
use noi::nrg;
nrg!(module = attest);

fn verify_submission(proof: Vec<u8>, public: attest::attest::PublicInputs) -> anyhow::Result<()> {
    let client = attest::Client::new("./noir");
    attest::attest::verify(&client, &proof, public)
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

fn validate_public_inputs(data: &risk::score::PublicInputs) -> anyhow::Result<()> {
    if data.score > 10_000 { anyhow::bail!("score out of bounds") }
    Ok(())
}
```

## Production & publication status
`noi` is **pre-release/alpha**. Codegen and the `simulate` path are wired through the
[`noir-runner`](https://github.com/noir-lang/noir-runner) crate, so you can exercise Noir exports
from Rust today. The proving/verification helpers are still placeholders until milestone 7 lands.
Publishing to crates.io will happen once those runtime APIs stabilize.

## Using `noi` in your project
1. Add dependencies (path-based until the crates are published):
   ```bash
   cargo add noi --path crates/noi
   cargo add noi-build --path crates/noi-build
   ```
2. Create or update your project’s `build.rs`:
   ```rust
   use std::{env, path::PathBuf};

   fn main() {
       let opts = noi_build::BuildOptions {
           program_dir: PathBuf::from("path/to/noir/project"),
           out_dir: PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")),
       };
       noi_build::export(&opts).expect("nargo export failed");
   }
   ```
3. Invoke `nrg!` inside your crate:
   ```rust
   use noi::nrg;

   nrg!(module = demo); // reads NOI_EXPORT_DIR emitted by build.rs

   fn main() -> anyhow::Result<()> {
       let noir_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("path/to/noir/project");
       let client = demo::Client::new(noir_dir);
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
- `NOI_EXPORT_DIR`: automatically set by `noi-build`, but you can override it manually when calling
  `nrg!` (e.g., `nrg!(module = demo, export_dir = "../export")`).
- `NOI_DEBUG=1`: when set, the macro dumps the expanded Rust code into `target/noi/expanded.rs` for
  inspection and IDE completion debugging.
- `NOI_NARGO_BIN`: optional path to a custom `nargo` binary for environments that pin toolchains.
- Visibility metadata: parameters tagged `public` go into the generated `PublicInputs`, while
  everything else lands in `PrivateInputs`. The macro also provides `From<Args> for Inputs` and the
  reverse, so you can ergonomically switch representations.

## Limitations & open work
- `simulate` already talks to Noir via `noir-runner`; `prove`/`verify` will be implemented in
  the proving milestones.
- Type coverage currently includes primitives, field elements, fixed arrays, tuples, and structs.
  Extend `TypeRepr` + fixtures before relying on additional Noir features.
- `noi-build` expects a `Nargo.toml` with sources under `src/`. Custom layouts must copy JSON into
  their own `noi_exports` directories before invoking `nrg!`.

## Development workflow
- Format: `cargo fmt`
- Lint: `cargo clippy --workspace --all-features -D warnings`
- Test: `cargo test --workspace`
- Review the stricter engineering standards in `AGENTS.md` before opening a PR. The milestones
  listed in `milestone.md` define the scope and acceptance tests for each iteration.

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
- Milestone 4 (done): integrate the Noir runner crate so `simulate` produces real outputs.
- Milestone 5+: expand type coverage, add public/private input ergonomics, and wire proving paths.
- Milestone 9+: ship documentation + examples polished enough for crates.io publication.
