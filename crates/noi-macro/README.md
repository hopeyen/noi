# noi-macro

`noi-macro` implements the `nrg!` proc macro. Point it at a directory of `nargo export` JSON files
and it will emit strongly typed Rust modules: `Args` structs, `Inputs` wrappers, `Output` aliases, and
helper methods like `simulate`, `prove`, and `verify` (once the runtime hooks are wired up).

## Example
```rust
use noi::nrg;

nrg!(module = my_circuit);

fn main() -> anyhow::Result<()> {
    let client = my_circuit::Client::new("./noir");
    let out = my_circuit::adder::simulate(&client, my_circuit::adder::Args { a: 1, b: 2 })?;
    println!("result = {out}");
    Ok(())
}
```

`nrg!` can also take an explicit `export_dir = "..."` override if you want to point at a custom
location.
