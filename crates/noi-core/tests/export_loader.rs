use std::path::PathBuf;

use noi_core::export::{collect_structs, load_export_dir, load_export_file, TypeRepr, Visibility};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn parses_export_fixtures() {
    let exports = load_export_dir(fixtures()).expect("fixtures should load");
    let names: Vec<_> = exports.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["add", "mix"]);

    let add = &exports[0];
    assert_eq!(add.parameters.len(), 2);
    assert_eq!(add.parameters[0].visibility, Visibility::Public);
    assert_eq!(add.return_type, Some(TypeRepr::Unsigned(32)));
    assert_eq!(add.return_visibility, Visibility::Public);
    assert_eq!(add.signature(), "fn add(pub a: u32, b: u32) -> pub u32");

    let mix = &exports[1];
    assert!(mix.parameters[0].ty.is_struct());
    assert_eq!(
        mix.signature(),
        "fn mix(point: Point, pub toggle: (bool, [u16; 3])) -> Delta"
    );
}

#[test]
fn collects_structs_by_path() {
    let exports = load_export_dir(fixtures()).unwrap();
    let structs = collect_structs(&exports);
    let paths: Vec<&str> = structs.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["Delta", "Point"]);
}

#[test]
fn parses_real_nargo_export() {
    let path = fixtures().join("demo/move_point.json");
    let f = load_export_file(path).unwrap();
    assert_eq!(f.name, "move_point");
    assert_eq!(f.signature(), "fn move_point(p: P, dx: i32) -> P");
    assert!(f
        .noir_version
        .as_deref()
        .unwrap()
        .starts_with(noi_core::NOIR_VERSION));
}

#[test]
fn missing_dir_has_actionable_error() {
    let err = load_export_dir(fixtures().join("nope")).unwrap_err();
    assert!(format!("{err:#}").contains("nargo export"), "{err:#}");
}

#[test]
fn empty_dir_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = load_export_dir(dir.path()).unwrap_err();
    assert!(err.to_string().contains("no export artifacts"), "{err}");
}

#[test]
fn malformed_json_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("bad.json"), "{ not json").unwrap();
    let err = load_export_dir(dir.path()).unwrap_err();
    assert!(format!("{err:#}").contains("bad.json"), "{err:#}");
}

#[test]
fn generic_instantiations_are_distinct_structs() {
    // `Option<u8>` and `Option<u32>` share an ABI path but not a layout.
    let option = |width: u16| {
        format!(
            r#"{{"kind":"struct","path":"std::option::Option","fields":[
                {{"name":"_is_some","type":{{"kind":"boolean"}}}},
                {{"name":"_value","type":{{"kind":"integer","sign":"unsigned","width":{width}}}}}]}}"#
        )
    };
    let json = format!(
        r#"{{"abi":{{"parameters":[{{"name":"a","type":{}}},{{"name":"b","type":{}}},{{"name":"c","type":{}}}]}}}}"#,
        option(8),
        option(32),
        option(8)
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.json"), json).unwrap();
    let exports = load_export_dir(dir.path()).unwrap();
    let structs = collect_structs(&exports);
    assert_eq!(structs.len(), 2, "two instantiations, deduplicated");
    assert_eq!(
        structs[0].fields[1].ty,
        TypeRepr::Unsigned(8),
        "first-seen order"
    );
    assert_eq!(structs[1].fields[1].ty, TypeRepr::Unsigned(32));
}
