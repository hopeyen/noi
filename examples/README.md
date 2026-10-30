# Examples

Runnable crates that use `noi` end to end. Both embed their Noir package under `noir/` and commit
its `export/` artifacts, so they build without `nargo`.

## `demo/` (`noi-example-app`)

The Noir library exports:

| Function | Shows |
|---|---|
| `add(a: u32, b: u32) -> u32` | the basics |
| `mix(t: (u32, bool), arr: [u16; 3]) -> (u32, bool)` | tuples and arrays |
| `move_point(p: P, dx: i32) -> P` | structs and signed integers |
| `checked_div(a: u32, b: u32) -> u32` | `assert(b != 0, "division by zero")` surfacing as `noi::ExecutionError` |
| `greet(name: str<5>) -> str<5>` | strings (exactly 5 bytes) |
| `sum_of_squares(a: Field, b: Field) -> Field` | a `#[fold]` function, executed as a separate ACIR circuit |
| `order_total(item: Field, qty: u32) -> Field` | an oracle (`get_price`) answered by `Client::with_oracle`, and a format-string assertion |
| `pick(i: u32, points: [P; 2]) -> u32` | an out-of-range `i` fails with nargo's message, in the program's own array size and index |
| `above_threshold(threshold: u32, secret: u32) -> (u32, bool)` | proving: returns `threshold` so the proof commits to it, while `secret` stays private |

```bash
cargo run -p noi-example-app
NOI_BB_BIN="$(scripts/install_bb.sh)" cargo run -p noi-example-app --features prove   # proves and verifies
```

`src/main.rs` calls `simulate` for the first functions and, with `--features prove`, proves
`above_threshold` and verifies it as a third party would with `verified_output`.
`tests/simulate.rs` covers errors and strings.

## `dark-pool/` (`noi-dark-pool-example`)

Order matching and batch settlement inspired by the [Mezcal](https://github.com/nemi-fi/mezcal)
dark-pool prototype: `match_orders(bid: Order, ask: Order, min_size: u64) -> bool` and
`settle_batch(filled: [u64; 3], reserve: u64) -> i64`.

```bash
cargo run -p noi-dark-pool-example
```

## Changing the Noir code

Edit `noir/src/lib.nr`. With `nargo` 1.0.0 installed, the next `cargo build` re-exports
automatically. To update the committed artifacts, run `scripts/refresh_exports.sh` from the
repository root and commit the result.
