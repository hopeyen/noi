use std::{env, path::PathBuf};

fn main() {
    let opts = noi_build::BuildOptions {
        program_dir: PathBuf::from("noir"),
        out_dir: PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set")),
    };
    if let Err(err) = noi_build::export(&opts) {
        panic!("failed to export Noir artifacts: {err:#}");
    }
}
