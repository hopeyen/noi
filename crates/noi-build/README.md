# noi-build

`noi-build` is a tiny helper crate meant to be used inside `build.rs`. It runs `nargo export` for you,
mirrors the resulting JSON artifacts into `$OUT_DIR/noi_exports`, and emits the `NOI_EXPORT_DIR`
environment variable so the `nrg!` macro knows where to look.

## Example build script
```rust
use std::{env, path::PathBuf};

fn main() {
    let opts = noi_build::BuildOptions {
        program_dir: PathBuf::from("./noir"),
        out_dir: PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")),
    };
    noi_build::export(&opts).expect("nargo export failed");
}
```

Ship this crate with your own Noir project so any downstream crate can regenerate exports during
`cargo build` without hand-rolling the `nargo` invocation.
