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
fn run_function_mix_handles_tuples() {
    let mut inputs = InputMap::new();
    inputs.insert("t".into(), (4u32, true).to_input_value());
    inputs.insert("arr".into(), [5u16, 7, 3].to_input_value());

    let result = run_function(demo_program_dir().as_path(), "mix", inputs)
        .expect("runner to succeed")
        .expect("function should return value");
    let tuple: (u32, bool) = FromOutputValue::from_output_value(result).expect("tuple decode");
    assert_eq!(tuple, (9, true));
}

#[test]
fn run_function_move_point_handles_structs() {
    let mut point = BTreeMap::new();
    point.insert("x".into(), 12u32.to_input_value());
    point.insert("y".into(), 4u32.to_input_value());

    let mut inputs = InputMap::new();
    inputs.insert("p".into(), InputValue::Struct(point));
    inputs.insert("dx".into(), (-2i32).to_input_value());

    let result = run_function(demo_program_dir().as_path(), "move_point", inputs)
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
        assert_eq!(x, 10);
        assert_eq!(y, 4);
    } else {
        panic!("expected struct output");
    }
}
