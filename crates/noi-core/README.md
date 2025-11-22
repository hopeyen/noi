# noi-core

`noi-core` contains the shared data structures and helpers that power the rest of the `noi`
workspace. It knows how to parse `nargo export` JSON, exposes strongly typed representations of Noir
functions, and will eventually host the runtime helpers used by the facade crate.

## Highlights
- `noi_core::load_export_dir(path)` parses every JSON artifact in a directory and returns a typed
  list of `ExportFunction`s.
- Provides `TypeRepr`, `StructType`, `Param`, and related building blocks that `noi-macro` uses when
  it emits Rust code.
- Acts as the single source of truth for Noir ABI information so both proc-macro and runtime layers
  stay in sync.

## Example
```rust
use noi_core::load_export_dir;

fn dump_signature() -> anyhow::Result<()> {
    for function in load_export_dir("./target/noi_exports")? {
        println!("{}", function.signature());
    }
    Ok(())
}
```
