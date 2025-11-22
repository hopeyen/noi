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

    let point = demo::mix::simulate(
        &client,
        demo::mix::Args {
            point: demo::Point { x: 2, y: 9 },
            toggle: (true, [5, 7, 3]),
        },
    )?;
    println!("mix output = Point {{ x: {}, y: {} }}", point.x, point.y);
    assert_eq!(point.x, 7);
    assert_eq!(point.y, 12);

    Ok(())
}
