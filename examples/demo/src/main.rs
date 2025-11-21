//! Companion example demonstrating how `nrg!` generates typed bindings for Noir exports.

use std::fmt::Debug;
use std::path::Path;

use anyhow::Result;
use noi::nrg;

nrg!(module = demo);

fn main() -> Result<()> {
    let program_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("noir");
    let client = demo::Client::new(program_dir);

    report("add", || {
        demo::add::simulate(&client, demo::add::Args { a: 5, b: 6 })
    });
    report("mix", || {
        let args = demo::mix::Args {
            point: demo::Point { x: 2, y: 9 },
            toggle: (true, [5, 7, 3]),
        };
        let inputs: demo::mix::Inputs = args.clone().into();
        let result = demo::mix::simulate(&client, args);
        println!(
            "mix public inputs = {:?}, private inputs = {:?}",
            inputs.public, inputs.private
        );
        result
    });

    Ok(())
}

fn report<F, T>(label: &str, call: F)
where
    F: FnOnce() -> Result<T>,
    T: Debug,
{
    match call() {
        Ok(value) => println!("{label} => {value:?}"),
        Err(err) => println!("{label} pending runner integration: {err:#}"),
    }
}
