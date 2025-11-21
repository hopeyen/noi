use std::{env, path::PathBuf};

fn main() {
    let program_dir = PathBuf::from("noir");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let opts = noi_build::BuildOptions {
        program_dir,
        out_dir,
    };
    if let Err(err) = noi_build::export(&opts) {
        panic!("failed to export Noir artifacts: {err:#}");
    }
}
