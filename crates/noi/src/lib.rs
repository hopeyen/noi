//! Typed Rust bindings for [Noir](https://noir-lang.org) `#[export]` functions, in the spirit of
//! Alloy's `sol!`.
//!
//! You write a Noir library, mark functions with `#[export]`, and `noi` generates a Rust module per
//! function with a typed `Args` struct, a typed `Output`, and a `simulate` function that runs the
//! circuit in-process. With the `prove` feature it also proves and verifies with Barretenberg. A
//! mismatch between your Rust code and the Noir ABI is a compile error, not a runtime
//! witness-encoding failure.
//!
//! # Quickstart
//!
//! **1. Noir side.** A library package (`type = "lib"` in `Nargo.toml`) with exported functions:
//!
//! ```text
//! // noir/src/lib.nr
//! struct P { x: u32, y: u32 }
//!
//! #[export]
//! fn add(a: u32, b: u32) -> u32 { a + b }
//!
//! #[export]
//! fn move_point(p: P, dx: i32) -> P { /* … */ }
//! ```
//!
//! **2. Cargo side.**
//!
//! ```toml
//! [dependencies]
//! noi = "0.0.2"
//!
//! [build-dependencies]
//! noi-build = "0.0.2"
//! ```
//!
//! ```no_run
//! // build.rs: runs `nargo export` (into OUT_DIR) and tells `nrg!` where the artifacts are.
//! fn main() {
//!     noi_build::BuildOptions::from_env("noir")
//!         .and_then(|opts| opts.export())
//!         .unwrap_or_else(|err| panic!("failed to export Noir artifacts: {err:#}"));
//! }
//! ```
//!
//! **3. Call it.**
//!
//! ```
//! # noi::nrg!(module = demo, export_dir = "tests/fixtures/demo");
//! // noi::nrg!(module = demo);   // reads NOI_EXPORT_DIR, set by noi-build
//!
//! # #[cfg(feature = "runner")]
//! # fn main() -> noi::Result<()> {
//! let client = demo::Client::new();
//!
//! let sum = demo::add::simulate(&client, &demo::add::Args { a: 2, b: 3 })?;
//! assert_eq!(sum, 5);
//!
//! // Noir structs become Rust structs; signed integers are handled for you.
//! let p = demo::move_point::simulate(
//!     &client,
//!     &demo::move_point::Args { p: demo::P { x: 10, y: 3 }, dx: -4 },
//! )?;
//! assert_eq!(p, demo::P { x: 6, y: 3 });
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "runner"))]
//! # fn main() {}
//! ```
//!
//! # What `nrg!` generates
//!
//! For `nrg!(module = demo)` and an export `fn add(a: u32, b: u32) -> u32`:
//!
//! ```ignore
//! pub mod demo {
//!     pub use noi::Client;                       // execution settings, see `Client`
//!     pub struct P { pub x: u32, pub y: u32 }    // one Rust struct per Noir struct
//!
//!     pub mod add {
//!         pub const NAME: &str = "add";
//!         pub const SIGNATURE: &str = "fn add(a: u32, b: u32) -> u32";
//!         pub const ARTIFACT_JSON: &str = /* embedded nargo export artifact */;
//!
//!         pub struct Args { pub a: u32, pub b: u32 }     // every parameter, in order
//!         pub type Output = u32;
//!
//!         pub fn simulate(client: &Client, args: &Args) -> noi::Result<Output>;
//!         pub fn execute(client: &Client, args: &Args) -> noi::Result<noi::Execution>;
//!         pub fn program() -> noi::Result<&'static noi::Program>;
//!
//!         // feature `prove`:
//!         pub fn prove(client: &Client, args: &Args) -> noi::Result<noi::Proof>;
//!         pub fn verify(client: &Client, proof: &noi::Proof) -> noi::Result<bool>;
//!         pub fn verified_output(client: &Client, proof: &noi::Proof) -> noi::Result<Output>;
//!         pub fn verification_key(client: &Client) -> noi::Result<Vec<u8>>;
//!         pub fn solidity_verifier(client: &Client) -> noi::Result<String>;
//!     }
//! }
//! ```
//!
//! Generated structs derive `Clone`, `Debug`, `PartialEq`, and (when every field allows it)
//! `Default`. Run with `NOI_DEBUG=1` to write the expanded code to
//! `target/noi/<crate>/<module>.rs`.
//!
//! ## Type mapping
//!
//! | Noir | Rust |
//! |---|---|
//! | `bool`, `u1` | `bool` |
//! | `u8` … `u128`, `i8` … `i64` | `u8` … `u128`, `i8` … `i64` |
//! | `Field` | [`FieldElement`] |
//! | `[T; N]` | `[T; N]` |
//! | `(A, B, …)`, up to 12 elements | `(A, B, …)` |
//! | `str<N>` | `String` (must be exactly `N` bytes at execution) |
//! | `struct S { … }` | `demo::S` |
//!
//! Struct naming: a struct is named after the last segment of its Noir path. If structs from
//! different paths share that name, the full path is used (`orders::Order` → `OrdersOrder`). Names
//! that would clash with `Client` or a prelude type get a `Noir` prefix (`std::option::Option` →
//! `NoirOption`). Each instantiation of a generic struct is its own Rust struct, numbered in
//! order of first use (`NoirOption`, `NoirOption2`), because Noir's ABI does not record generic
//! arguments.
//!
//! Function, parameter, and field names are converted to `snake_case`, and Rust keywords are
//! escaped (`move` → `r#move`, `self` → `self_`). Two Noir names that map to the same Rust name
//! (`fooBar` and `foo_bar`) are a compile error naming both.
//!
//! # Errors
//!
//! Every fallible function returns [`noi::Result`](Result). When a Noir `assert` fails, the error
//! contains an [`ExecutionError`] carrying the assertion message, with format strings filled in:
//!
//! ```
//! # noi::nrg!(module = demo, export_dir = "tests/fixtures/demo");
//! # #[cfg(feature = "runner")] {
//! # let client = demo::Client::new();
//! // Noir: fn checked_div(a: u32, b: u32) -> u32 { assert(b != 0, "division by zero"); a / b }
//! let err = demo::checked_div::simulate(&client, &demo::checked_div::Args { a: 1, b: 0 })
//!     .unwrap_err();
//! let exec = err.downcast_ref::<noi::ExecutionError>().unwrap();
//! assert_eq!(exec.function(), "checked_div");
//! assert_eq!(exec.message(), Some("division by zero"));
//! # }
//! ```
//!
//! # Oracles
//!
//! Unconstrained Noir code can ask the host for data through oracles. Answer them with
//! [`Client::with_oracle`]; unhandled oracles fail the execution, and `println` output is
//! discarded.
//!
//! ```
//! # noi::nrg!(module = demo, export_dir = "tests/fixtures/demo");
//! # #[cfg(feature = "runner")]
//! # fn main() -> noi::Result<()> {
//! use noi::{FieldElement, OracleValue};
//!
//! // Noir: #[oracle(get_price)] unconstrained fn get_price_oracle(item: Field) -> Field {}
//! let client = demo::Client::new().with_oracle("get_price", |_args| {
//!     Ok(vec![OracleValue::Field(FieldElement::from(250u128))])
//! });
//! let total = demo::order_total::simulate(
//!     &client,
//!     &demo::order_total::Args { item: FieldElement::from(7u128), qty: 4 },
//! )?;
//! assert_eq!(total, FieldElement::from(1000u128));
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "runner"))]
//! # fn main() {}
//! ```
//!
//! # Proving
//!
//! With the `prove` feature, each function also gets `prove`, `verify`, `verified_output`,
//! `verification_key`, and `solidity_verifier`, backed by Aztec's Barretenberg `bb` CLI
//! (UltraHonk). Install the matching release (`noi::BB_VERSION`, currently 6.0.0) with
//! `bbup -v 6.0.0` or the repository's `scripts/install_bb.sh`, and point `NOI_BB_BIN` at it if
//! it is not on `PATH`.
//!
//! **The return value is the public statement.** Noir only accepts `pub` on a program's `main`,
//! so `#[export]` functions have no public parameters, and a proof's public inputs are exactly the
//! function's return value. Return any input the proof must commit to:
//!
//! ```text
//! #[export]
//! fn above_threshold(threshold: u32, secret: u32) -> (u32, bool) {
//!     (threshold, secret > threshold)   // `secret` stays private
//! }
//! ```
//!
//! ```no_run
//! # #[cfg(feature = "prove")]
//! # fn main() -> noi::Result<()> {
//! # noi::nrg!(module = demo, export_dir = "tests/fixtures/demo");
//! let client = demo::Client::new()
//!     .with_prover(noi::Prover::new().with_target(noi::VerifierTarget::Evm));
//!
//! // Prover side.
//! let args = demo::above_threshold::Args { threshold: 100, secret: 250 };
//! let proof = demo::above_threshold::prove(&client, &args)?;
//!
//! // Verifier side: checks the proof and decodes what it proves.
//! let (threshold, above) = demo::above_threshold::verified_output(&client, &proof)?;
//! assert_eq!((threshold, above), (100, true));
//!
//! // Deploy this contract to verify the same proofs on-chain.
//! let solidity = demo::above_threshold::solidity_verifier(&client)?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "prove"))]
//! # fn main() {}
//! ```
//!
//! Verification keys are computed once per circuit, verifier target, and `bb` release, and cached
//! on disk (`$NOI_CACHE_DIR`, default: the per-user cache directory). The cache is trusted: on
//! Unix `noi` creates it private (`0700`) and refuses one other users can write; other platforms
//! are not checked, so never point `NOI_CACHE_DIR` at a shared directory. Proofs from a browser
//! (`bb.js`) can be checked with `noi::Proof::from_parts` and `verify`.
//!
//! bb's UltraHonk prover only accepts single-circuit programs, so a function that calls `#[fold]`
//! functions can be simulated but not proved; `prove` reports this before invoking `bb`.
//!
//! # Performance
//!
//! The first call to a function decodes its embedded artifact (decompression plus bytecode
//! deserialization) and caches it in a `static`. Later calls only ABI-encode the inputs and solve
//! the circuit; nothing is read from disk, and `nargo` is not needed at run time. Generated
//! functions are safe to call from many threads at once.
//!
//! # Features
//!
//! - `runner` *(default)*: generate `simulate`/`execute` and the value conversions. Disable it to
//!   use the generated types only (for example to share the schema with a WASM prover), which
//!   avoids compiling the Noir execution crates.
//! - `field`: use Noir's `FieldElement` for `Field` without enabling `runner`.
//! - `prove`: generate the proving functions described under [Proving](#proving). Implies
//!   `runner`; needs the `bb` CLI at run time.
//!
//! # Versions
//!
//! Artifacts must come from Noir [`NOIR_VERSION`] (`nargo --version`); `noi` rejects artifacts
//! from other releases when they are first loaded, naming both versions.
#![warn(missing_docs)]
// Doc snippets show complete `build.rs` files, which need `fn main`.
#![allow(clippy::needless_doctest_main)]

#[cfg(feature = "prove")]
pub use noi_core::prove::{self, Proof, Prover, VerifierTarget, BB_VERSION};
#[cfg(feature = "runner")]
pub use noi_core::runtime::{
    self, Client, Execution, ExecutionError, FromValue, InputMap, OracleValue, Program, ToValue,
    Value,
};
pub use noi_core::{
    export::{self, ExportFunction, Param, StructField, StructType, TypeRepr, Visibility},
    load_export_dir, noir_version_compatible,
    types::FieldElement,
    NOIR_VERSION,
};
pub use noi_macro::nrg;

/// Result type returned by generated bindings (an alias for [`anyhow::Result`]).
pub type Result<T, E = anyhow::Error> = std::result::Result<T, E>;

// `nrg!` runs on the host and cannot see which features *this* crate was built with, so the
// generated code wraps feature-dependent items in these passthrough macros, which expand to
// their input only when the feature is enabled here.
#[cfg(feature = "runner")]
#[doc(hidden)]
#[macro_export]
macro_rules! __noi_if_runner {
    ($($tokens:tt)*) => { $($tokens)* };
}
#[cfg(not(feature = "runner"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __noi_if_runner {
    ($($tokens:tt)*) => {};
}
#[cfg(feature = "prove")]
#[doc(hidden)]
#[macro_export]
macro_rules! __noi_if_prove {
    ($($tokens:tt)*) => { $($tokens)* };
}
#[cfg(not(feature = "prove"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __noi_if_prove {
    ($($tokens:tt)*) => {};
}

/// Implementation details used by code generated with [`nrg!`]. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use crate::{__noi_if_prove as if_prove, __noi_if_runner as if_runner};
    pub use anyhow;
    #[cfg(feature = "runner")]
    pub use noi_core::runtime;
}
