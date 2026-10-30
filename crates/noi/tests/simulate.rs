#![cfg(feature = "runner")]

use noi::{FieldElement, OracleValue};

noi::nrg!(module = demo, export_dir = "tests/fixtures/demo");

#[test]
fn runs_add_function() {
    let client = demo::Client::new();
    let result = demo::add::simulate(&client, &demo::add::Args { a: 5, b: 6 }).unwrap();
    assert_eq!(result, 11);
}

#[test]
fn runs_mix_function() {
    let client = demo::Client::new();
    let args = demo::mix::Args {
        t: (4, true),
        arr: [5, 7, 3],
    };
    assert_eq!(demo::mix::simulate(&client, &args).unwrap(), (9, true));
}

#[test]
fn runs_move_point() {
    let client = demo::Client::new();
    let moved = demo::move_point::simulate(
        &client,
        &demo::move_point::Args {
            p: demo::P { x: 12, y: 8 },
            dx: -2,
        },
    )
    .unwrap();
    assert_eq!(moved, demo::P { x: 10, y: 8 });
}

#[test]
fn folded_and_oracle_functions() {
    let client = demo::Client::new().with_oracle("get_price", |_| {
        Ok(vec![OracleValue::Field(FieldElement::from(3u128))])
    });
    let squares = demo::sum_of_squares::simulate(
        &client,
        &demo::sum_of_squares::Args {
            a: FieldElement::from(3u128),
            b: FieldElement::from(4u128),
        },
    )
    .unwrap();
    assert_eq!(squares, FieldElement::from(25u128));

    let total = demo::order_total::simulate(
        &client,
        &demo::order_total::Args {
            item: FieldElement::from(1u128),
            qty: 5,
        },
    )
    .unwrap();
    assert_eq!(total, FieldElement::from(15u128));
}

#[test]
fn constants_describe_the_function() {
    assert_eq!(demo::add::NAME, "add");
    assert_eq!(demo::add::SIGNATURE, "fn add(a: u32, b: u32) -> u32");
    assert!(demo::add::program()
        .unwrap()
        .noir_version()
        .starts_with(noi::NOIR_VERSION));
}
