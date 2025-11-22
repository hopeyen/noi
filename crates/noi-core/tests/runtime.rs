#![cfg(feature = "runner")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use noi_core::runtime::{run_function, FromOutputValue, InputMap, InputValue, ToInputValue};

fn demo_program_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo/noir")
}

#[test]
fn run_function_add_returns_sum() {
    let mut inputs = InputMap::new();
    inputs.insert("a".into(), 5u32.to_input_value());
    inputs.insert("b".into(), 6u32.to_input_value());

    let result = run_function(demo_program_dir().as_path(), "add", inputs)
        .expect("runner to succeed")
        .expect("function should return value");
    let sum = u32::from_output_value(result).expect("decode u32");
    assert_eq!(sum, 11);
}

#[test]
fn run_function_mix_handles_structs() {
    let mut inputs = InputMap::new();
    let mut point = BTreeMap::new();
    point.insert("x".into(), 2u32.to_input_value());
    point.insert("y".into(), 9u32.to_input_value());
    inputs.insert("point".into(), InputValue::Struct(point));

    let mut toggle = Vec::new();
    toggle.push(true.to_input_value());
    toggle.push([5u16, 7, 3].to_input_value());
    inputs.insert("toggles".into(), InputValue::Vec(toggle));

    let result = run_function(demo_program_dir().as_path(), "mix", inputs)
        .expect("runner to succeed")
        .expect("function should return value");
    if let InputValue::Struct(mut fields) = result {
        let x = fields
            .remove("x")
            .and_then(|value| u32::from_output_value(value).ok())
            .expect("x field");
        let y = fields
            .remove("y")
            .and_then(|value| u32::from_output_value(value).ok())
            .expect("y field");
        assert_eq!(x, 7);
        assert_eq!(y, 12);
    } else {
        panic!("expected struct output");
    }
}
