# Demo example (`noi-example-app`)

This crate mirrors how a downstream project adopts `noi`:

- `build.rs` calls `noi_build::BuildOptions::from_env("noir")?.export()`, which stages the
  artifacts in `OUT_DIR` and sets `NOI_EXPORT_DIR`.
- `src/main.rs` invokes `nrg!(module = demo)` and calls `simulate` with the generated `Args`
  structs. With `--features prove` it also proves `above_threshold` and checks the proof with
  `verified_output`.
- `./noir` holds the Noir sources; `examples/README.md` lists every exported function.

```bash
cargo run -p noi-example-app
NOI_BB_BIN="$(scripts/install_bb.sh)" cargo run -p noi-example-app --features prove   # or bbup -v 6.0.0
```

With `nargo` 1.0.x installed, the build re-exports from `./noir` whenever the Noir code changes.
Without it, the build falls back to the committed artifacts in `./noir/export`, so the example
still builds anywhere.
