#![cfg(feature = "runner")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use noi_core::runtime::{Client, FromOutputValue, InputMap, InputValue, Program, ToInputValue};

fn demo_artifact(name: &str) -> Program {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo/noir/export")
        .join(format!("{name}.json"));
    Program::from_file(path).expect("artifact should decode")
}

fn call(program: &Program, inputs: &InputMap) -> InputValue {
    program
        .execute(&Client::new(), inputs)
        .expect("execution should succeed")
        .into_return_value()
        .expect("function should return a value")
}

#[test]
fn add_returns_sum() {
    let program = demo_artifact("add");
    assert_eq!(program.name(), "add");
    assert!(program.noir_version().starts_with(noi_core::NOIR_VERSION));

    let mut inputs = InputMap::new();
    inputs.insert("a".into(), 5u32.to_input_value());
    inputs.insert("b".into(), 6u32.to_input_value());
    assert_eq!(u32::from_output_value(call(&program, &inputs)).unwrap(), 11);
}

#[test]
fn program_is_reusable_and_thread_safe() {
    let program = std::sync::Arc::new(demo_artifact("add"));
    let handles: Vec<_> = (0..4u32)
        .map(|i| {
            let program = program.clone();
            std::thread::spawn(move || {
                let mut inputs = InputMap::new();
                inputs.insert("a".into(), i.to_input_value());
                inputs.insert("b".into(), 1u32.to_input_value());
                u32::from_output_value(call(&program, &inputs)).unwrap()
            })
        })
        .collect();
    let sums: Vec<u32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(sums, [1, 2, 3, 4]);
}

#[test]
fn mix_handles_tuples_and_arrays() {
    let mut inputs = InputMap::new();
    inputs.insert("t".into(), (4u32, true).to_input_value());
    inputs.insert("arr".into(), [5u16, 7, 3].to_input_value());
    let out: (u32, bool) =
        FromOutputValue::from_output_value(call(&demo_artifact("mix"), &inputs)).unwrap();
    assert_eq!(out, (9, true));
}

#[test]
fn move_point_handles_structs_and_signed() {
    let point = BTreeMap::from([
        ("x".to_string(), 12u32.to_input_value()),
        ("y".to_string(), 4u32.to_input_value()),
    ]);
    let mut inputs = InputMap::new();
    inputs.insert("p".into(), InputValue::Struct(point));
    inputs.insert("dx".into(), (-2i32).to_input_value());

    let InputValue::Struct(fields) = call(&demo_artifact("move_point"), &inputs) else {
        panic!("expected struct output");
    };
    assert_eq!(u32::from_output_value(fields["x"].clone()).unwrap(), 10);
    assert_eq!(u32::from_output_value(fields["y"].clone()).unwrap(), 4);
}

#[test]
fn execution_exposes_witness_for_proving() {
    let mut inputs = InputMap::new();
    inputs.insert("a".into(), 1u32.to_input_value());
    inputs.insert("b".into(), 2u32.to_input_value());
    let execution = demo_artifact("add")
        .execute(&Client::new(), &inputs)
        .unwrap();
    assert!(execution.witness_stack().length() >= 1);
    assert!(!execution.witness_bytes().unwrap().is_empty());
}

#[test]
fn missing_input_is_reported() {
    let mut inputs = InputMap::new();
    inputs.insert("a".into(), 1u32.to_input_value());
    let err = demo_artifact("add")
        .execute(&Client::new(), &inputs)
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("invalid inputs for `add`"),
        "{err:#}"
    );
}

#[test]
fn incompatible_artifact_names_versions() {
    let json = r#"{"noir_version":"0.36.0+abc","hash":"1","abi":{},"bytecode":"x"}"#;
    let err = Program::from_json("f", json).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("0.36.0") && msg.contains(noi_core::NOIR_VERSION),
        "{msg}"
    );
}
