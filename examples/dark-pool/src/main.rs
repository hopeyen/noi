use noi::nrg;

nrg!(module = dark_pool);

fn main() -> noi::Result<()> {
    let client = dark_pool::Client::new();

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
