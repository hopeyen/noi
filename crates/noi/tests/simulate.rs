use noi::nrg;

const PROGRAM_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/demo/noir");

nrg!(module = demo);

#[test]
fn runs_add_function() {
    let client = demo::Client::new(PROGRAM_DIR);
    let result = demo::add::simulate(&client, demo::add::Args { a: 5, b: 6 })
        .expect("simulate should succeed");
    assert_eq!(result, 11);
}

#[test]
fn runs_mix_function() {
    let client = demo::Client::new(PROGRAM_DIR);
    let args = demo::mix::Args {
        point: demo::Point { x: 2, y: 9 },
        toggles: (true, [5, 7, 3]),
    };

    let point = demo::mix::simulate(&client, args).expect("simulate should succeed");
    assert_eq!(point.x, 7);
    assert_eq!(point.y, 12);
}
