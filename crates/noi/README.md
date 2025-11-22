# noi

The `noi` crate is the public entrypoint: it re-exports the `nrg!` macro and the runtime helpers so
most downstream projects only need this single dependency.

## Example
```rust
use noi::nrg;

nrg!(module = finance);

fn main() -> anyhow::Result<()> {
    let client = finance::Client::new("./noir");
    let args = finance::interest::Args { principal: 1_000, rate: 350 };
    let _ = finance::interest::simulate(&client, args)?;
    Ok(())
}
```

See the workspace README for a deeper dive into design goals, interaction patterns, and the
companion examples.
