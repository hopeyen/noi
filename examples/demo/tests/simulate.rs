use noi::nrg;

const NOIR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/noir");

nrg!(module = demo);

#[test]
fn add_function_runs() {
    let client = demo::Client::new(NOIR_DIR);
    let sum = demo::add::simulate(&client, demo::add::Args { a: 5, b: 6 })
        .expect("simulate should succeed");
    assert_eq!(sum, 11);
}

#[test]
fn mix_function_runs() {
    let client = demo::Client::new(NOIR_DIR);
    let result = demo::mix::simulate(
        &client,
        demo::mix::Args {
            t: (2, false),
            arr: [5, 7, 3],
        },
    )
    .expect("simulate should succeed");
    assert_eq!(result, (9, false));
}

#[test]
fn move_point_runs() {
    let client = demo::Client::new(NOIR_DIR);
    let moved = demo::move_point::simulate(
        &client,
        demo::move_point::Args {
            p: demo::P { x: 10, y: 3 },
            dx: -4,
        },
    )
    .expect("simulate should succeed");
    assert_eq!(moved.x, 6);
    assert_eq!(moved.y, 3);
}
