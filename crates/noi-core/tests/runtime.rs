#![cfg(feature = "runner")]
//! Executes real `nargo export` artifacts (Noir 1.0) from `tests/fixtures/demo/`.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use noi_core::runtime::{
    Client, ExecutionError, FieldElement, FromValue, InputMap, OracleValue, Program, ToValue, Value,
};

fn demo(name: &str) -> Program {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/demo")
        .join(format!("{name}.json"));
    Program::from_file(path).expect("artifact should decode")
}

fn inputs<const N: usize>(pairs: [(&str, Value); N]) -> InputMap {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn call(program: &Program, client: &Client, inputs: &InputMap) -> Value {
    program
        .execute(client, inputs)
        .expect("execution should succeed")
        .into_return_value()
        .expect("function should return a value")
}

fn exec_error(err: &anyhow::Error) -> &ExecutionError {
    err.downcast_ref::<ExecutionError>()
        .unwrap_or_else(|| panic!("expected an ExecutionError, got {err:#}"))
}

#[test]
fn add_returns_sum() {
    let program = demo("add");
    assert_eq!(program.name(), "add");
    assert!(noi_core::noir_version_compatible(program.noir_version()));
    let out = call(
        &program,
        &Client::new(),
        &inputs([("a", 5u32.to_value()), ("b", 6u32.to_value())]),
    );
    assert_eq!(u32::from_value(out).unwrap(), 11);
}

#[test]
fn program_is_reusable_across_threads() {
    let program = Arc::new(demo("add"));
    let client = Client::new();
    let handles: Vec<_> = (0..4u32)
        .map(|i| {
            let (program, client) = (program.clone(), client.clone());
            std::thread::spawn(move || {
                let out = call(
                    &program,
                    &client,
                    &inputs([("a", i.to_value()), ("b", 1u32.to_value())]),
                );
                u32::from_value(out).unwrap()
            })
        })
        .collect();
    let sums: Vec<u32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(sums, [1, 2, 3, 4]);
}

#[test]
fn tuples_arrays_structs_and_signed_values() {
    let out = call(
        &demo("mix"),
        &Client::new(),
        &inputs([
            ("t", (4u32, true).to_value()),
            ("arr", [5u16, 7, 3].to_value()),
        ]),
    );
    assert_eq!(<(u32, bool)>::from_value(out).unwrap(), (9, true));

    let p = Value::Struct(BTreeMap::from([
        ("x".to_string(), 12u32.to_value()),
        ("y".to_string(), 4u32.to_value()),
    ]));
    let Value::Struct(fields) = call(
        &demo("move_point"),
        &Client::new(),
        &inputs([("p", p), ("dx", (-2i32).to_value())]),
    ) else {
        panic!("expected a struct");
    };
    assert_eq!(u32::from_value(fields["x"].clone()).unwrap(), 10);
    assert_eq!(u32::from_value(fields["y"].clone()).unwrap(), 4);
}

#[test]
fn strings_round_trip() {
    let out = call(
        &demo("greet"),
        &Client::new(),
        &inputs([("name", "alice".to_value())]),
    );
    assert_eq!(String::from_value(out).unwrap(), "alice");
}

#[test]
fn folded_functions_run_as_separate_circuits() {
    let program = demo("sum_of_squares");
    let execution = program
        .execute(
            &Client::new(),
            &inputs([
                ("a", FieldElement::from(3u128).to_value()),
                ("b", FieldElement::from(4u128).to_value()),
            ]),
        )
        .unwrap();
    assert!(
        execution.witness_stack().length() > 1,
        "#[fold] should produce one witness per executed circuit"
    );
    let out = FieldElement::from_value(execution.into_return_value().unwrap()).unwrap();
    assert_eq!(out, FieldElement::from(25u128));
}

#[test]
fn oracles_are_answered_by_registered_handlers() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let client = Client::new().with_oracle("get_price", move |args| {
        seen.fetch_add(1, Ordering::SeqCst);
        assert_eq!(args, [OracleValue::Field(FieldElement::from(7u128))]);
        Ok(vec![OracleValue::Field(FieldElement::from(250u128))])
    });
    let out = call(
        &demo("order_total"),
        &client,
        &inputs([
            ("item", FieldElement::from(7u128).to_value()),
            ("qty", 4u32.to_value()),
        ]),
    );
    assert_eq!(
        FieldElement::from_value(out).unwrap(),
        FieldElement::from(1000u128)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn unhandled_oracles_fail_with_their_name() {
    let err = demo("order_total")
        .execute(
            &Client::new(),
            &inputs([
                ("item", FieldElement::from(7u128).to_value()),
                ("qty", 4u32.to_value()),
            ]),
        )
        .unwrap_err();
    let exec = exec_error(&err);
    assert!(exec.detail().contains("get_price"), "{}", exec.detail());
}

#[test]
fn assertion_messages_are_decoded() {
    let err = demo("checked_div")
        .execute(
            &Client::new(),
            &inputs([("a", 1u32.to_value()), ("b", 0u32.to_value())]),
        )
        .unwrap_err();
    let exec = exec_error(&err);
    assert_eq!(exec.function(), "checked_div");
    assert_eq!(exec.message(), Some("division by zero"));
    assert_eq!(err.to_string(), "`checked_div` failed: division by zero");
}

#[test]
fn format_string_assertions_are_filled_in() {
    let client =
        Client::new().with_oracle("get_price", |_| Ok(vec![OracleValue::Field(1u128.into())]));
    let err = demo("order_total")
        .execute(
            &client,
            &inputs([
                ("item", FieldElement::from(1u128).to_value()),
                ("qty", 5000u32.to_value()),
            ]),
        )
        .unwrap_err();
    assert_eq!(exec_error(&err).message(), Some("qty 5000 too large"));
}

#[test]
fn execution_exposes_witness_for_proving() {
    let execution = demo("add")
        .execute(
            &Client::new(),
            &inputs([("a", 1u32.to_value()), ("b", 2u32.to_value())]),
        )
        .unwrap();
    assert_eq!(execution.witness_stack().length(), 1);
    assert!(!execution.witness_bytes().unwrap().is_empty());
}

#[test]
fn invalid_inputs_name_the_problem() {
    let add = demo("add");
    let client = Client::new();
    let msg = |inputs: InputMap| format!("{:#}", add.execute(&client, &inputs).unwrap_err());

    let missing = msg(inputs([("a", 1u32.to_value())]));
    assert!(missing.contains("missing input `b`"), "{missing}");

    let extra = msg(inputs([
        ("a", 1u32.to_value()),
        ("b", 1u32.to_value()),
        ("c", 1u32.to_value()),
    ]));
    assert!(extra.contains("unexpected input `c`"), "{extra}");

    let too_big = msg(inputs([("a", u64::MAX.to_value()), ("b", 1u32.to_value())]));
    assert!(
        too_big.contains("`a`") && too_big.contains("u32"),
        "{too_big}"
    );

    let wrong_len = format!(
        "{:#}",
        demo("greet")
            .execute(&client, &inputs([("name", "bob".to_value())]))
            .unwrap_err()
    );
    assert!(wrong_len.contains("exactly 5 bytes"), "{wrong_len}");
}

#[test]
fn incompatible_artifacts_name_both_versions() {
    for version in ["0.36.0+abc", "1.0.0-beta.26+40d6574f"] {
        let json =
            format!(r#"{{"noir_version":"{version}","hash":"1","abi":{{}},"bytecode":"x"}}"#);
        let msg = Program::from_json("f", &json).unwrap_err().to_string();
        assert!(
            msg.contains(version) && msg.contains(noi_core::NOIR_VERSION),
            "{msg}"
        );
    }
}
