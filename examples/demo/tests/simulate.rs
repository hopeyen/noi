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
    let point = demo::mix::simulate(
        &client,
        demo::mix::Args {
            point: demo::Point { x: 2, y: 9 },
            toggles: (true, [5, 7, 3]),
        },
    )
    .expect("simulate should succeed");
    assert_eq!(point.x, 7);
    assert_eq!(point.y, 12);
}
