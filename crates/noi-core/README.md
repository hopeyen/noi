# noi-core

Schema and runtime layer behind [`noi`](https://docs.rs/noi). Use it directly for tooling around
Noir `nargo export` artifacts, or to execute functions whose names are only known at run time.

- `noi_core::export`: parse artifact ABIs into `ExportFunction` / `TypeRepr` (no Noir compiler
  dependency).
- `noi_core::runtime` (feature `runner`, default): `Program::from_json(..).execute(..)` solves the
  circuit in-process with the ACVM and returns the decoded output plus the witness.

```rust
use noi_core::runtime::{Client, InputMap, Program, ToInputValue, FromOutputValue};

let program = Program::from_file("noir/export/add.json")?;
let mut inputs = InputMap::new();
inputs.insert("a".into(), 2u32.to_input_value());
inputs.insert("b".into(), 3u32.to_input_value());
let out = program.execute(&Client::new(), &inputs)?.into_return_value().unwrap();
assert_eq!(u32::from_output_value(out)?, 5);
```

See the [docs](https://docs.rs/noi-core).
