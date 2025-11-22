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
        t: (4, true),
        arr: [5, 7, 3],
    };

    let tuple = demo::mix::simulate(&client, args).expect("simulate should succeed");
    assert_eq!(tuple, (9, true));
}

#[test]
fn runs_move_point() {
    let client = demo::Client::new(PROGRAM_DIR);
    let moved = demo::move_point::simulate(
        &client,
        demo::move_point::Args {
            p: demo::P { x: 12, y: 8 },
            dx: -2,
        },
    )
    .expect("simulate should succeed");
    assert_eq!(moved.x, 10);
    assert_eq!(moved.y, 8);
}
