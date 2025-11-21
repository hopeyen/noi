# Demo Example (`noi-example-app`)

This crate mirrors how a downstream project would adopt `noi`:

- `build.rs` calls `noi-build::export`, dropping JSON into `OUT_DIR` and emitting `NOI_EXPORT_DIR`.
- `src/main.rs` invokes `nrg!(module = demo)` and exercises the generated `Args`/`Inputs` structs.
- `./noir` holds the actual Noir sources (the same `add`/`mix` functions described in
  `examples/README.md`).

Run it with:

```bash
cargo run -p noi-example-app
```

You’ll see log output describing the simulated calls. Until the Noir runner work lands (milestone 4),
the `simulate` helpers return friendly `todo!()` errors—expected behavior that’s also mentioned in the
root README.
