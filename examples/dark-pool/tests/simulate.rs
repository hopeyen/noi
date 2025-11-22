use noi::nrg;

const NOIR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/noir");

nrg!(module = dark_pool);

#[test]
fn match_orders_executes() {
    let client = dark_pool::Client::new(NOIR_DIR);
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
    )
    .expect("runner should execute");
    assert!(matched);
}

#[test]
fn settle_batch_executes() {
    let client = dark_pool::Client::new(NOIR_DIR);
    let delta = dark_pool::settle_batch::simulate(
        &client,
        dark_pool::settle_batch::Args {
            filled: [12, 18, 10],
            reserve: 45,
        },
    )
    .expect("runner should execute");
    assert_eq!(delta, 5);
}
