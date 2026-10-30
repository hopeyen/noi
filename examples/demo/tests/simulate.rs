use noi::nrg;

nrg!(module = demo);

#[test]
fn add_function_runs() {
    let client = demo::Client::new();
    let sum = demo::add::simulate(&client, &demo::add::Args { a: 5, b: 6 })
        .expect("simulate should succeed");
    assert_eq!(sum, 11);
}

#[test]
fn mix_function_runs() {
    let client = demo::Client::new();
    let result = demo::mix::simulate(
        &client,
        &demo::mix::Args {
            t: (2, false),
            arr: [5, 7, 3],
        },
    )
    .expect("simulate should succeed");
    assert_eq!(result, (9, false));
}

#[test]
fn move_point_runs() {
    let client = demo::Client::new();
    let moved = demo::move_point::simulate(
        &client,
        &demo::move_point::Args {
            p: demo::P { x: 10, y: 3 },
            dx: -4,
        },
    )
    .expect("simulate should succeed");
    assert_eq!(moved.x, 6);
    assert_eq!(moved.y, 3);
}

#[test]
fn failed_assertion_surfaces_message() {
    let client = demo::Client::new();
    let err = demo::checked_div::simulate(&client, &demo::checked_div::Args { a: 1, b: 0 })
        .expect_err("dividing by zero must fail");
    let exec = err
        .downcast_ref::<noi::ExecutionError>()
        .expect("error should be an ExecutionError");
    assert_eq!(exec.function(), "checked_div");
    assert_eq!(exec.message(), Some("division by zero"));
}

#[test]
fn strings_round_trip() {
    let client = demo::Client::new();
    let out = demo::greet::simulate(
        &client,
        &demo::greet::Args {
            name: "alice".into(),
        },
    )
    .expect("simulate should succeed");
    assert_eq!(out, "alice");
}

#[test]
fn wrong_string_length_is_rejected() {
    let client = demo::Client::new();
    let err = demo::greet::simulate(&client, &demo::greet::Args { name: "bob".into() })
        .expect_err("str<5> requires exactly 5 bytes");
    assert!(format!("{err:#}").contains("invalid inputs"), "{err:#}");
}
