#![doc = r#"
`noi-core` is the shared foundation for the `noi` workspace. It knows how to load Noir export
artifacts, represent ABI information with strongly typed Rust structs, and hosts the runtime helpers
that other crates build upon (the runner glue behind `simulate`, with proving hooks coming later).

Most downstream users only need the high-level `noi` crate, but if you want low-level access to the
Noir export schema (e.g., to inspect function signatures or build custom tooling), this crate is the
place to start. The primary entry point is [`load_export_dir`](crate::load_export_dir).
"#]

pub mod export;
#[cfg(feature = "runner")]
pub mod runtime;
pub mod types;

pub use export::{load_export_dir, ExportFunction, Param, StructField, TypeRepr, Visibility};
#[cfg(feature = "runner")]
pub use runtime::{run_function, FromOutputValue, InputMap, InputValue, ToInputValue};
