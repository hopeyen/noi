# noi-core

Schema and runtime layer behind [`noi`](https://docs.rs/noi). Use it directly for tooling around
Noir `nargo export` artifacts, or to execute functions whose names are only known at run time.

- `noi_core::export`: parse artifact ABIs into `ExportFunction` / `TypeRepr`, with no Noir crates
  involved.
- `noi_core::runtime` (feature `runner`, default): `Program::from_json(..).execute(..)` solves the
  circuit in-process with the ACVM (multi-circuit programs, Brillig, oracles) and returns the
  decoded output plus the witness. Depends only on Noir crates published on crates.io.
- `noi_core::prove` (feature `prove`): prove and verify with Barretenberg's `bb` CLI.

```rust
use noi_core::runtime::{Client, FromValue, InputMap, Program, ToValue};

let program = Program::from_file("noir/export/add.json")?;
let inputs = InputMap::from([("a".to_string(), 2u32.to_value()), ("b".to_string(), 3u32.to_value())]);
let out = program.execute(&Client::new(), &inputs)?.into_return_value().unwrap();
assert_eq!(u32::from_value(out)?, 5);
```

See the [docs](https://docs.rs/noi-core).
