//! `noi-build` against a real `nargo`. Skipped when nargo 1.0.x is not found, unless
//! `NOI_REQUIRE_NARGO=1`, in which case a missing nargo fails the tests.

use std::path::{Path, PathBuf};

use noi_build::{ArtifactSource, BuildOptions};

fn nargo() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("NOI_NARGO_BIN").map(PathBuf::from),
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/nargo-bin/1.0.0/nargo")),
    ];
    let found = candidates.into_iter().flatten().find(|p| p.is_file());
    if found.is_none() {
        assert!(
            std::env::var("NOI_REQUIRE_NARGO").as_deref() != Ok("1"),
            "NOI_REQUIRE_NARGO=1 but no nargo found; run scripts/refresh_exports.sh"
        );
        eprintln!("skipping: no nargo 1.0.x found");
    }
    found
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A library with a custom `entry` outside `src/` that depends on a sibling via a relative path.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("dep/Nargo.toml"),
        "[package]\nname = \"dep\"\ntype = \"lib\"\n",
    );
    write(
        &dir.path().join("dep/src/lib.nr"),
        "pub fn double(x: u32) -> u32 { x * 2 }\n",
    );
    write(
        &dir.path().join("app/Nargo.toml"),
        "[package]\nname = \"app\"\ntype = \"lib\"\nentry = \"circuits/main.nr\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(
        &dir.path().join("app/circuits/main.nr"),
        "mod helpers;\n#[export]\nfn quad(x: u32) -> u32 { helpers::twice(dep::double(x)) }\n",
    );
    write(
        &dir.path().join("app/circuits/helpers.nr"),
        "pub fn twice(x: u32) -> u32 { x * 2 }\n",
    );
    dir
}

fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn exports_custom_entry_with_path_dependency_without_touching_sources() {
    let Some(nargo) = nargo() else { return };
    let ws = workspace();
    let before = snapshot(ws.path());
    let out = tempfile::tempdir().unwrap();

    let summary = BuildOptions::new(ws.path().join("app"), out.path())
        .nargo_bin(&nargo)
        .skip_nargo(false)
        .export()
        .unwrap();
    assert_eq!(summary.source, ArtifactSource::Nargo);
    assert_eq!(
        summary.artifacts,
        [out.path().join("noi_exports/quad.json")]
    );
    assert_eq!(
        snapshot(ws.path()),
        before,
        "the source tree must not change"
    );

    // Paths in the artifact are relative to the package, so the output is reproducible.
    let json = std::fs::read_to_string(&summary.artifacts[0]).unwrap();
    let root = std::fs::canonicalize(ws.path()).unwrap();
    assert!(
        !json.contains(&*root.to_string_lossy()),
        "absolute path leaked"
    );
    assert!(
        json.contains("circuits/main.nr"),
        "entry file should be in file_map"
    );
    let again = tempfile::tempdir().unwrap();
    let second = BuildOptions::new(ws.path().join("app"), again.path())
        .nargo_bin(&nargo)
        .skip_nargo(false)
        .export()
        .unwrap();
    assert_eq!(
        std::fs::read(&second.artifacts[0]).unwrap(),
        json.as_bytes()
    );
}

#[test]
fn noir_compile_errors_fail_the_build() {
    let Some(nargo) = nargo() else { return };
    let ws = workspace();
    write(
        &ws.path().join("app/circuits/helpers.nr"),
        "pub fn twice(x: u32) -> u32 { x * }\n",
    );
    // A committed fallback must not hide the compile error.
    write(
        &ws.path().join("app/export/quad.json"),
        r#"{"noir_version":"1.0.0","abi":{}}"#,
    );
    let out = tempfile::tempdir().unwrap();
    let err = BuildOptions::new(ws.path().join("app"), out.path())
        .nargo_bin(&nargo)
        .skip_nargo(false)
        .export()
        .unwrap_err();
    assert!(format!("{err:#}").contains("export` failed"), "{err:#}");
}
