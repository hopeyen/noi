//! # noi
//!
//! Public facade that re-exports the `nrg!` proc macro and runtime helpers from the rest of the
//! workspace. Downstream crates usually just depend on this crate:
//!
//! ```no_run
//! use noi::nrg;
//!
//! nrg!(module = demo);
//!
//! fn main() -> anyhow::Result<()> {
//!     let client = demo::Client::new("./noir");
//!     let result = demo::add::simulate(&client, demo::add::Args { a: 1_000, b: 350 })?;
//!     println!("sum = {result}");
//!     Ok(())
//! }
//! ```
//!
//! For full context (design goals, architecture, and runnable examples) see the workspace README in
//! the repository.

pub use noi_core::{
    load_export_dir, types::FieldElement, ExportFunction, Param, StructField, TypeRepr, Visibility,
};
pub use noi_macro::nrg;
