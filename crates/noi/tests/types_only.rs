//! With `runner` disabled, `nrg!` still generates the schema types (no execution functions).
#![cfg(not(feature = "runner"))]

noi::nrg!(module = demo, export_dir = "tests/fixtures/demo");

#[test]
fn generated_types_are_usable() {
    let args = demo::move_point::Args {
        p: demo::P { x: 1, y: 2 },
        dx: -1,
    };
    let inputs: demo::move_point::Inputs = args.clone().into();
    assert_eq!(demo::move_point::Args::from(inputs), args);
    assert_eq!(
        demo::move_point::SIGNATURE,
        "fn move_point(p: P, dx: i32) -> P"
    );
}
