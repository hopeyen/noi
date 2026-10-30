# noi

Typed Rust bindings for [Noir](https://noir-lang.org) `#[export]` functions, in the spirit of
Alloy's `sol!`. `noi::nrg!` reads the artifacts written by `nargo export` and generates a module per
function with typed `Args`, `Inputs` (split into public/private), an `Output` type, and a
`simulate` function that executes the circuit in-process.

```rust
noi::nrg!(module = demo); // reads NOI_EXPORT_DIR, set by noi-build in build.rs

fn main() -> noi::Result<()> {
    let client = demo::Client::new();
    let sum = demo::add::simulate(&client, demo::add::Args { a: 5, b: 6 })?;
    assert_eq!(sum, 11);
    Ok(())
}
```

```rust
// build.rs (with `noi-build` as a build-dependency)
fn main() {
    noi_build::BuildOptions::from_env("noir")
        .expect("OUT_DIR is set by cargo")
        .export()
        .expect("failed to export Noir artifacts");
}
```

See the [API documentation](https://docs.rs/noi) for the generated code shape, type mapping, error
handling, and features. Requires artifacts from Noir 1.0.0-beta.26 (`noi::NOIR_VERSION`) and Rust 1.89+.
