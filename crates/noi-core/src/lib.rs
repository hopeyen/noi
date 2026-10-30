//! Schema and runtime layer behind [`noi`](https://docs.rs/noi).
//!
//! Most applications should depend on the `noi` crate and use its `nrg!` macro. Reach for
//! `noi-core` directly when you are writing tooling around Noir exports or need to call functions
//! whose names are only known at run time.
//!
//! | Module | What it does | Needs the Noir compiler crates? |
//! |---|---|---|
//! | [`export`] | Parses the ABI of `nargo export` artifacts into [`ExportFunction`]s. Used by the macro. | No |
//! | [`runtime`] | Executes artifacts and converts values (feature `runner`). | Yes |
//! | `prove` | Proves/verifies with the `bb` CLI (feature `prove`). | Yes, plus `bb` at run time |
//! | [`types`] | [`FieldElement`](types::FieldElement), available with or without `runner`. | Only with `runner`/`field` |
//!
//! # Inspecting exports
//!
//! ```
//! # let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
//! for function in noi_core::load_export_dir(dir)? {
//!     println!("{}", function.signature()); // e.g. `fn add(pub a: u32, b: u32) -> pub u32`
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # Executing an export dynamically
//!
//! See [`runtime`] for a complete example built on [`runtime::Program`].
//!
//! # Features
//!
//! - `runner` *(default)*: enables [`runtime`], pulling in `acvm`, `nargo`, and `noirc_abi` at
//!   Noir [`NOIR_VERSION`].
//! - `field`: exposes `acvm::FieldElement` as [`types::FieldElement`] without the full runtime.
//! - `prove`: enables the `prove` module, which proves and verifies with Barretenberg's `bb` CLI
//!   (implies `runner`).
#![warn(missing_docs)]

pub mod export;
#[cfg(feature = "prove")]
pub mod prove;
#[cfg(feature = "runner")]
pub mod runtime;
pub mod types;

pub use export::{
    load_export_dir, load_export_file, ExportFunction, Param, StructField, StructType, TypeRepr,
    Visibility,
};
#[cfg(feature = "prove")]
pub use prove::{Proof, Prover, VerifierTarget};
#[cfg(feature = "runner")]
pub use runtime::{
    Client, Execution, ExecutionError, FromOutputValue, InputMap, InputValue, Program, ToInputValue,
};

/// The Noir release whose artifacts this build of `noi-core` can execute.
///
/// `nargo export` output from other versions may use a different bytecode encoding. Keep the
/// `nargo` you export with in sync with this version (see `noirup --version`).
pub const NOIR_VERSION: &str = "1.0.0-beta.26";
