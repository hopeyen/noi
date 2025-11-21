use std::path::PathBuf;

use noi_core::{load_export_dir, TypeRepr, Visibility};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn parses_export_fixtures() {
    let exports = load_export_dir(fixtures()).expect("fixtures should load");
    assert_eq!(exports.len(), 2);

    let add = exports
        .iter()
        .find(|f| f.name == "add")
        .expect("add function");
    assert_eq!(add.parameters.len(), 2);
    assert_eq!(add.parameters[0].visibility, Visibility::Public);
    match &add.return_type {
        Some(TypeRepr::Unsigned(bits)) => assert_eq!(*bits, 32),
        other => panic!("unexpected return type: {other:?}"),
    }

    let mix = exports
        .iter()
        .find(|f| f.name == "mix")
        .expect("mix function");
    assert!(mix.parameters[0].ty.is_struct());
}
