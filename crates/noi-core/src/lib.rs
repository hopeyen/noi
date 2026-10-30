//! Schema and runtime layer behind [`noi`](https://docs.rs/noi).
//!
//! Most applications should depend on the `noi` crate and use its `nrg!` macro. Reach for
//! `noi-core` directly when you are writing tooling around Noir exports or need to call functions
//! whose names are only known at run time.
//!
//! | Module | What it does | Extra dependencies |
//! |---|---|---|
//! | [`export`] | Parses the ABI of `nargo export` artifacts into [`ExportFunction`]s. Used by the macro. | none |
//! | [`runtime`] | Executes artifacts and converts values (feature `runner`, default). | `acvm`, `bn254_blackbox_solver` |
//! | `prove` | Proves and verifies with the `bb` CLI (feature `prove`). | `bb` 6.0.0 at run time |
//! | [`types`] | [`FieldElement`](types::FieldElement), with or without `runner`. | `acir_field` with `runner`/`field` |
//!
//! Every dependency is published on crates.io.
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
//! - `runner` *(default)*: enables [`runtime`], pulling in `acvm` and `bn254_blackbox_solver`
//!   (plus `acir_field` through `field`), all from crates.io at Noir [`NOIR_VERSION`]. The parts of
//!   Noir's unpublished `nargo`/`noirc_abi` that are needed are reimplemented in [`runtime`].
//! - `field`: exposes Noir's `FieldElement` as [`types::FieldElement`] without the runtime.
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
    Client, Execution, ExecutionError, FromValue, InputMap, OracleValue, Program, ToValue, Value,
};

/// The Noir release whose artifacts this build of `noi-core` decodes and executes.
///
/// `nargo export` output from other releases may use a different bytecode encoding. Export with
/// this `nargo` (`noirup --version 1.0.0`); see [`noir_version_compatible`] for what is accepted.
pub const NOIR_VERSION: &str = "1.0.0";

/// Whether an artifact's `noir_version` (e.g. `1.0.0+275d3ab1…`) is one `noi` can execute.
///
/// The release part (before `+`) must be a stable `1.0.x` release; prereleases such as
/// `1.0.0-beta.26` are rejected because their bytecode encoding differs.
///
/// ```
/// assert!(noi_core::noir_version_compatible("1.0.0+275d3ab1a5c916c6"));
/// assert!(noi_core::noir_version_compatible("1.0.3"));
/// assert!(!noi_core::noir_version_compatible("1.0.0-beta.26+40d6574f"));
/// assert!(!noi_core::noir_version_compatible("0.36.0"));
/// ```
pub fn noir_version_compatible(found: &str) -> bool {
    let release = found.split('+').next().unwrap_or_default();
    let mut ours = NOIR_VERSION.split('.');
    let mut theirs = release.split('.');
    // Same major.minor, and a numeric (non-prerelease) patch.
    ours.next() == theirs.next()
        && ours.next() == theirs.next()
        && theirs
            .next()
            .is_some_and(|patch| !patch.is_empty() && patch.bytes().all(|b| b.is_ascii_digit()))
        && theirs.next().is_none()
}
