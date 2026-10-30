
# `noi` – Milestone Specifications

## Product one‑liner

> **`noi`**: a Rust crate that reads **`nargo export`** artifacts and generates **strongly‑typed Rust bindings** (like `sol!`) to **call, simulate, prove, and verify** Noir `#[export]` functions.

### High‑level goals

* Zero‑boilerplate Rust interface to Noir programs (per‑function modules, typed `Args`/`Output`).
* Ergonomic surface like:

  ```rust
  noi::nrg!(module = math);       // reads export dir
  let client = math::Client::new(program_dir)?;
  let out = math::add::simulate(&client, math::add::Args{ a: 2u32, b: 3u32 })?;
  ```
* Optional proving (`prove`/`verify`) behind a feature flag.

### Out of scope (for v0)

* Compiling `.nr` sources directly (we rely on `nargo export`).
* Editing/transforming Noir code.
* Multi‑backend abstraction beyond a single proving backend (add later).

---

## Workspace layout (target)

```
noi/
├─ Cargo.toml                 # [workspace]
├─ crates/
│  ├─ noi-core/               # runtime glue, export schema, conversions
│  ├─ noi-build/              # build helper: run nargo export, stage artifacts
│  ├─ noi-macro/              # proc-macro: nrg!(...)
│  └─ noi/                    # umbrella crate: re-exports macro/core + helpers
└─ examples/
   └─ demo/
      ├─ Cargo.toml
      ├─ build.rs             # uses noi-build
      ├─ src/main.rs          # uses noi::nrg!
      └─ noir/
         ├─ Nargo.toml
         └─ src/lib.nr
```

---

## Milestone 0 — Repository bootstrap

**Deliverables**

* Workspace `Cargo.toml`.
* Crates created with minimal metadata.
* Root `.editorconfig`, `.gitignore`, `rust-toolchain.toml` (pin stable), `clippy.toml`.
* LICENSE + README stub.

**Acceptance**

* `cargo build --all` succeeds (empty crates).
* `cargo clippy --all -D warnings` passes (empty).

---

## Milestone 1 — Export schema & loader (`noi-core`)

**Why**: Decode `nargo export` JSON (per‑function) into Rust structs we can codegen from.

**Tasks**

* Implement `noi_core::export` module:

  * `ExportFunction { name, parameters: Vec<Param>, return_type, visibility info, … }`
  * `Param { name, ty: TypeRepr, visibility }`
  * `TypeRepr` enum with *minimum viable* variants:

    * primitives: `Bool`, `U{8,16,32,64,128}`, `I{8,16,32,64,128}`, `Field`
    * aggregates: `Array(Box<TypeRepr>, usize)`, `Tuple(Vec<TypeRepr>)`, `Struct { name, fields }`
    * (add others later)
  * `fn load_export_dir(path: &Path) -> anyhow::Result<Vec<ExportFunction>>`
* Add **golden JSON fixtures** in `noi-core/tests/fixtures/` to lock schema expectations.
* Unit tests: round‑trip deserialize fixtures; assert shapes.

**Acceptance**

* `cargo test -p noi-core` passes.
* The loader returns a vector of functions with correct names and type shapes from fixtures.

---

## Milestone 2 — Build helper (`noi-build`)

**Why**: Make it trivial for users to refresh exports in `build.rs`.

**Tasks**

* Expose:

  * `struct BuildOptions { program_dir: PathBuf, out_dir: PathBuf }`
  * `fn export(opts: &BuildOptions) -> anyhow::Result<ExportSummary>`

    * Runs `nargo export` in `program_dir` (via `Command`).
    * Stages **all JSON exports** into `<out_dir>/noi_exports/`.
    * Emits `cargo:rustc-env=NOI_EXPORT_DIR=<out_dir>/noi_exports`.
    * Emits `cargo:rerun-if-changed=<program_dir>/src/**` and `Nargo.toml`.
* Write concise logging to `stderr` with `NOI_VERBOSE` env opt.

**Acceptance**

* Example `examples/demo/build.rs` can call `noi_build::export(...)` and produce a staged `noi_exports/` with JSON.
* On rebuild, changing `src/lib.nr` triggers re‑export.

---

## Milestone 3 — Proc‑macro skeleton (`noi-macro`)

**Why**: Generate user‑facing, typed modules at compile time (like `sol!`).

**Tasks**

* Create `nrg!(module = <ident>, export_dir = "<path>"?)`.

  * If `export_dir` omitted, use `std::env::var("NOI_EXPORT_DIR")`.
  * Read all JSON in the directory with `noi_core::load_export_dir`.
  * For each function:

    * Generate `pub mod <fn_name> { pub struct Args { ... } pub type Output = ... }` with type mapping.
* Generate a shared:

  ```rust
  pub struct Client { program_dir: std::path::PathBuf }
  impl Client { pub fn new(program_dir: impl Into<PathBuf>) -> Self { ... } }
  ```
* Implement **type mapping** (Noir → Rust) for:

  * `bool`, signed/unsigned ints up to 128, `Field` → `acvm::FieldElement` (behind a feature; else map to opaque newtype).
  * Fixed arrays `[T; N]`, tuples `(A,B,...)`, and structs.
* Generate `simulate` **stub** per function (no runtime yet), returning `todo!()`.

**Acceptance**

* `examples/demo` can compile a project that uses `nrg!(module = demo)` and sees generated types in IDE.
* The macro fails gracefully (compile error) for unsupported types with a clear message.

---

## Milestone 4 — Execution path (`noi-core` + `noi-macro`)

**Why**: Make `.simulate()` actually evaluate the function using the exported program.

**Tasks**

* Add optional feature `runner`:

  * Dependency: `noir-runner` (exact version pinned).
* In `noi-core`:

  * Implement `ToInputValue` / `FromOutputValue` traits for supported types.
  * `fn run_function(program_dir: &Path, fn_name: &str, inputs: InputMap) -> anyhow::Result<Option<OutputValue>>`
    using `noir_runner::NoirRunner`.
* In `noi-macro`:

  * Emit real `simulate(client, args) -> Result<Output>`:

    * Build `InputMap` from `Args` using generated `ToInputValue`.
    * Call into `noi_core::run_function`.
    * Map output back to typed `Output`.

**Acceptance**

* Update `examples/demo/noir` with:

  ```noir
  #[export] fn add(a: u32, b: u32) -> u32 { a + b }
  ```
* `examples/demo`:

  ```rust
  let client = demo::Client::new("./noir");
  assert_eq!(demo::add::simulate(&client, demo::add::Args { a: 5, b: 6 })?, 11);
  ```
* `cargo test -p noi` runs an integration test invoking `.simulate()`.

---

## Milestone 5 — Type coverage & nesting

**Why**: Handle realistic Noir types.

**Tasks**

* Extend `TypeRepr` and mapper to support:

  * Nested tuples, nested arrays.
  * `struct` nesting (dedupe type declarations by structural hash; generate stable names).
  * Visibility tags (record which inputs are public vs private).
* Generate doc comments on `Args`/`Output` with original Noir type signatures.

**Acceptance**

* Add Noir tests:

  * `#[export] fn mix(t: (u32,bool), arr: [u16; 3]) -> (u32,bool)`
  * `struct P { x: u32, y: u32 } #[export] fn move(p: P, dx: i32) -> P`
* Verify `.simulate()` works for both.

---

## Milestone 6 — Public/private inputs model

**Why**: Prepare for proof APIs and improve clarity.

**Tasks**

* In codegen, if visibility metadata is available:

  * Generate

    ```rust
    pub struct Inputs { pub public: PublicInputs, pub private: PrivateInputs }
    ```

    and overload `.simulate(client, Inputs)` (with `From<Args>` fallback for functions with all‑private or all‑public).
* Keep `.simulate` working without proofs, but structure inputs for easy proving later.

**Acceptance**

* Example function with both public and private parameters compiles and runs simulation.

---

## Milestone 7 — Proving & verification (feature: `prove`)

**Why**: End‑to‑end proving from Rust.

**Tasks**

* Add optional dependency on a Noir backend wrapper (e.g., Barretenberg via `noir_rs`) behind `features = ["prove"]`.
* In `noi-core`:

  * `fn prove(program_dir, fn_name, inputs) -> anyhow::Result<ProofBytes>`
  * `fn verify(program_dir, fn_name, proof: &[u8]) -> anyhow::Result<()>`
  * Manage SRS discovery/caching (configurable env var).
* In `noi-macro`, generate for each function:

  ```rust
  #[cfg(feature="prove")]
  pub fn prove(client: &Client, inputs: Inputs) -> anyhow::Result<Vec<u8>> { ... }
  #[cfg(feature="prove")]
  pub fn verify(client: &Client, proof: &[u8]) -> anyhow::Result<()> { ... }
  ```

**Acceptance**

* Noir demo adds a trivial verifiable function.
* Example app proves and verifies once under `--features prove`.

---

## Milestone 8 — Diagnostics & UX polish

**Tasks**

* Macro errors: pretty, actionable (missing env/dir, unreadable JSON, unsupported types).
* `cargo:warning=` lines from `noi-build` when `nargo` missing, with guidance.
* Optional `NOI_DEBUG=1` env to dump generated Rust into `target/noi/expanded.rs`.

**Acceptance**

* Hand‑crafted failures (missing export dir, malformed JSON) show focused errors.

---

## Milestone 9 — Documentation & examples

**Tasks**

* Top‑level `README.md` (Quickstart, Concepts, Features, Limitations).
* `examples/demo` expanded with two functions and public/private inputs.
* Doc‑tests on the macro usage.
* `CHANGELOG.md` and `CONTRIBUTING.md`.

**Acceptance**

* `cargo doc --all --no-deps` builds cleanly.
* Examples compile and run from README snippets.

---

## Milestone 10 — CI & versioning

**Tasks**

* CI: build + test matrix for `default`, `runner`, and `prove` features.
* Enforce `clippy -D warnings` and `fmt --check`.
* Pin Noir toolchain version in CI; print `nargo --version` in logs.
* Crate features:

  * `default = ["runner"]`
  * optional `prove`
  * optional `field` (maps `Field` to `acvm::FieldElement`)
* Semantic versioning plan documented.

**Acceptance**

* CI passes on all jobs.
* Feature flags build independently.

---

## API surface (initial)

```rust
// macro (noi-macro)
noi::nrg!(module = mymod);                 // reads from NOI_EXPORT_DIR
noi::nrg!(module = mymod, export_dir = "path/to/exports");

// generated (per function)
pub mod mymod {
    #[derive(Clone)]
    pub struct Client { /* program_dir, cache, … */ }
    impl Client { pub fn new<P: Into<PathBuf>>(program_dir: P) -> Self { ... } }

    pub mod add {
        #[derive(Clone, Debug)] pub struct Args { pub a: u32, pub b: u32 }
        pub type Output = u32;

        pub fn simulate(client: &Client, args: Args) -> anyhow::Result<Output>;
        #[cfg(feature="prove")]
        pub fn prove(client: &Client, inputs: Inputs) -> anyhow::Result<Vec<u8>>;
        #[cfg(feature="prove")]
        pub fn verify(client: &Client, proof: &[u8]) -> anyhow::Result<()>;
    }
}
```

---

## Example usage (end‑to‑end)

**`examples/demo/build.rs`**

```rust
use std::{env, path::PathBuf};
fn main() {
    let program_dir = PathBuf::from("noir");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    noi_build::export(&noi_build::BuildOptions { program_dir, out_dir })
        .expect("nargo export failed");
}
}
```

**`examples/demo/src/main.rs`**

```rust
use noi::nrg; // re-exported proc macro

nrg!(module = demo); // reads NOI_EXPORT_DIR set by build.rs

fn main() -> anyhow::Result<()> {
    let client = demo::Client::new("./noir");
    let out = demo::add::simulate(&client, demo::add::Args { a: 5, b: 6 })?;
    assert_eq!(out, 11);
    Ok(())
}
```

---

## Engineering checklist per milestone (for Codex)

* [ ] Create files exactly as specified (paths matter).
* [ ] Add unit tests where called out.
* [ ] Keep public APIs `pub` and stable names (no randomized identifiers).
* [ ] Use `anyhow` for errors across crates; convert to nice macro diagnostics in `noi-macro`.
* [ ] Gate optional deps with features (`runner`, `prove`, `field`).
* [ ] Avoid panics in library code (return errors); panics allowed in proc‑macro on unrecoverable generation failures.
* [ ] Document every `todo!()` with a short comment explaining the intended implementation.

---

## Backlog (post‑M10, nice‑to‑have)

* Multiple Noir programs in one binding (namespace support).
* Cache compiled ACIR and VK/PK under `target/noi/` with content‑hash keys.
* `macro_rules!` sugar:

  ```rust
  noi::auto!(module = mymod, program_dir = "./noir"); // runs export internally
  ```
* Derive `serde::{Serialize,Deserialize}` on generated `Args`/`Output`.
* Pretty printers for `Field` & big integers.

---

## v0.0.2 — Runtime hardening, DX, and docs

**Done**

* Upgrade from Noir 0.36 to 1.0.0-beta.26 (crates, `nargo`, regenerated `export/` artifacts).
* Replace `noir-runner` with an in-process executor on the Noir crates (`nargo`, `noirc_abi`,
  `noirc_artifacts`, `bn254_blackbox_solver`); artifacts are embedded by `nrg!` and decoded once.
* `noi::ExecutionError` (assertion messages), `execute()` + `Execution::witness_bytes()` as the
  bridge to external provers.
* ABI coverage: `databus` visibility, `str<N>`, `u1`; serde mirrors of `noirc_abi` types.
* Codegen: only `::noi` paths, keyword escaping, path-based struct naming on collisions,
  conditional `Default`, relative `export_dir` resolved against `CARGO_MANIFEST_DIR`.
* `noi-build`: stage from `export/`, builder API, `NOI_SKIP_NARGO`, version-mismatch warnings.
* Docs on every public item with runnable doctests; CI (fmt, clippy, tests per feature set, docs).
* Milestone 7 proving (feature `prove`) via the `bb` CLI: `Prover`/`Proof`/`VerifierTarget`,
  generated `prove`/`verify`/`verified_output`/`verification_key`/`solidity_verifier`, VK cache,
  demo `above_threshold` export proved and verified by the example app. Noir does not allow `pub`
  on `#[export]` parameters, so proofs expose the return value only.

**Deferred to v0.0.3**

* In-process proving (no `bb` subprocess) and recursion helpers.
* Optional `serde` derives on generated types; source locations for failed assertions.
