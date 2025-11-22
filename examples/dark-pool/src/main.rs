use std::path::Path;

use anyhow::Result;
use noi::nrg;

nrg!(module = dark_pool);

fn main() -> Result<()> {
    let noir_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("noir");
    let client = dark_pool::Client::new(&noir_dir);

    let matched = dark_pool::match_orders::simulate(
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
    )?;
    println!("match_orders => {matched}");
    assert!(matched);

    let delta = dark_pool::settle_batch::simulate(
        &client,
        dark_pool::settle_batch::Args {
            filled: [12, 18, 10],
            reserve: 45,
        },
    )?;
    println!("settle_batch delta = {delta}");
    assert_eq!(delta, 5);

    Ok(())
}
