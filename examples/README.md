# Examples

Small, runnable crates live here so you can try `noi` without wiring your own project first.

## `demo/`
- Location: `examples/demo/`
- Rust package: `noi-example-app`
- Noir sources: embedded under `examples/demo/noir/`

### What the Noir program does
Two functions are exported via `#[export]`:

1. `add(a: u32, b: u32) -> u32` – simple addition.
2. `mix(point: Point, toggles: (bool, [u16; 3])) -> Point` – nudges the point by one of two deltas
   based on the boolean; the third array slot always bumps `y`.

These simple circuits give the demo enough structure to showcase structs, tuples, arrays, and public
vs private inputs.

### What the Rust example showcases
- `nrg!(module = demo)` gives you `Args`, `Inputs`, and the `Client` glue for free.
- `build.rs` runs `noi-build::export` so the macro knows where to read JSON from (falling back to the
  committed fixtures when `nargo` is missing).
- `src/main.rs` shows how to call `simulate` and flip between `Args` and `Inputs`.

### Running the demo
```bash
cargo run -p noi-example-app
```
Expect log lines similar to:
```
5 + 6 = 11
mix output = Point { x: 7, y: 12 }
```
`nargo` isn’t required for this example—the committed exports under `examples/demo/noir/export`
keep `cargo run` working everywhere.

## `dark-pool/`

- Location: `examples/dark-pool/`
- Rust package: `noi-dark-pool-example`
- Noir sources: embedded under `examples/dark-pool/noir/`

This crate borrows the matching/settlement flavor from the
[Mezcal](https://github.com/nemi-fi/mezcal) dark-pool prototype. The Noir program exports two
functions (`match_orders`, `settle_batch`), which the Rust side calls via `nrg!`. It’s handy for
seeing how structs, fixed arrays, and public/private inputs look in a more realistic workflow.

```
cargo run -p noi-dark-pool-example
```
