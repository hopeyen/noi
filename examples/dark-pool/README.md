# Dark-pool example (`noi-dark-pool-example`)

A slightly more realistic scenario inspired by the [Mezcal](https://github.com/nemi-fi/mezcal)
project: match/settle batched orders off-chain, then ship proofs + public inputs elsewhere.

- `build.rs` runs `noi-build::export` against the embedded Noir program under `./noir`.
- `src/main.rs` exercises both exported functions:
  - `match_orders` takes two `Order` structs plus a `min_size` threshold and returns whether the fill
    is valid.
  - `settle_batch` aggregates three fills plus a reserve and returns the signed delta so you know if
    inventory needs topping up.

Run it with:

```bash
cargo run -p noi-dark-pool-example
```

As with the other examples, the runtime hooks still emit TODO-style errors until milestone 4 wires in
`noir-runner`, but the generated bindings highlight how structs, arrays, and public/private inputs
map into Rust.
