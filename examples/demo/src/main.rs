use std::path::Path;

use anyhow::Result;
use noi::nrg;

nrg!(module = demo);

fn main() -> Result<()> {
    let program_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("noir");
    let client = demo::Client::new(&program_dir);

    let sum = demo::add::simulate(&client, demo::add::Args { a: 5, b: 6 })?;
    println!("5 + 6 = {sum}");
    assert_eq!(sum, 11);

    let mixed = demo::mix::simulate(
        &client,
        demo::mix::Args {
            t: (12, true),
            arr: [5, 7, 3],
        },
    )?;
    println!("mix output = ({}, {})", mixed.0, mixed.1);
    assert_eq!(mixed.0, 17);
    assert!(mixed.1);

    let moved = demo::move_point::simulate(
        &client,
        demo::move_point::Args {
            p: demo::P { x: 9, y: 4 },
            dx: -3,
        },
    )?;
    println!("move_point => P {{ x: {}, y: {} }}", moved.x, moved.y);
    assert_eq!(moved.x, 6);
    assert_eq!(moved.y, 4);

    Ok(())
}
