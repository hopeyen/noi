use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let is_workspace_crate = manifest_dir.file_name().is_some_and(|name| name == "noi")
        && manifest_dir
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|name| name == "crates");
    if !is_workspace_crate {
        return;
    }

    let exports_dir = manifest_dir.join("../../examples/demo/noir/export");
    if !exports_dir.exists() {
        return;
    }

    let canonical = fs::canonicalize(&exports_dir).unwrap_or(exports_dir);
    println!("cargo:rustc-env=NOI_EXPORT_DIR={}", canonical.display());
    println!("cargo:rerun-if-changed={}", canonical.display());
}
