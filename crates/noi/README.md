# noi

Typed Rust bindings for [Noir](https://noir-lang.org) `#[export]` functions, in the spirit of
Alloy's `sol!`. `noi::nrg!` reads the artifacts written by `nargo export` and generates a module per
function with a typed `Args` struct, an `Output` type, and a `simulate` function that runs the
circuit in-process. With the `prove` feature it also proves and verifies with Barretenberg.

```rust
noi::nrg!(module = demo); // reads NOI_EXPORT_DIR, set by noi-build in build.rs

fn main() -> noi::Result<()> {
    let client = demo::Client::new();
    let sum = demo::add::simulate(&client, &demo::add::Args { a: 5, b: 6 })?;
    assert_eq!(sum, 11);
    Ok(())
}
```

```rust
// build.rs (with `noi-build` as a build-dependency)
fn main() {
    noi_build::BuildOptions::from_env("noir")
        .and_then(|opts| opts.export())
        .unwrap_or_else(|err| panic!("failed to export Noir artifacts: {err:#}"));
}
```

Requires artifacts from a stable Noir 1.0.x (`noi::NOIR_VERSION` = 1.0.0 recommended), Rust
1.89+, and, for proving, `bb` 6.0.0. See the [API documentation](https://docs.rs/noi) for the generated code, type mapping,
errors, oracles, and proving.
