//! Dark-pool flavored example inspired by https://github.com/nemi-fi/mezcal .
//! It shows how a backend might batch trades off-chain, simulate Noir logic,
//! and (eventually) produce proofs before relaying public inputs on-chain.

use std::fmt::Debug;
use std::path::Path;

use anyhow::Result;
use noi::nrg;

nrg!(module = dark_pool);

fn main() -> Result<()> {
    let noir_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("noir");
    let client = dark_pool::Client::new(noir_dir);

    report("match_orders", || {
        dark_pool::match_orders::simulate(
            &client,
            dark_pool::match_orders::Args {
                bid: dark_pool::Order {
                    price: 1_005,
                    size: 50,
                },
                ask: dark_pool::Order {
                    price: 1_000,
                    size: 40,
                },
                min_size: 25,
            },
        )
    });

    report("settle_batch", || {
        let args = dark_pool::settle_batch::Args {
            filled: [12, 18, 10],
            reserve: 45,
        };
        let inputs: dark_pool::settle_batch::Inputs = args.clone().into();
        let result = dark_pool::settle_batch::simulate(&client, args);
        println!("settle_batch public inputs = {:?}", inputs.public);
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
