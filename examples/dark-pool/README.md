# Dark-pool example (`noi-dark-pool-example`)

A slightly more realistic scenario inspired by the [Mezcal](https://github.com/nemi-fi/mezcal)
project: simulate matching and settling a batch of orders, using Noir structs (`Order`) and
fixed-size arrays. It only simulates; the circuits do not commit to the orders, so proving them
would attest to little. For an end-to-end prove/verify flow, see the demo's `above_threshold`.

- `build.rs` runs `noi_build::BuildOptions::from_env("noir")` against the embedded Noir program.
- `src/main.rs` exercises both exported functions:
  - `match_orders` takes two `Order` structs plus a `min_size` threshold and returns whether the fill
    is valid.
  - `settle_batch` aggregates three fills plus a reserve and returns the signed delta so you know if
    inventory needs topping up.

Run it with:

```bash
cargo run -p noi-dark-pool-example
```

With `nargo` 1.0.x installed, the build re-exports from `./noir`; otherwise it uses the committed
artifacts under `./noir/export`, so the example builds without the Noir toolchain.
