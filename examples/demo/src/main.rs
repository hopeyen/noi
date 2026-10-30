use noi::nrg;

nrg!(module = demo);

fn main() -> noi::Result<()> {
    let client = demo::Client::new();

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

    #[cfg(feature = "prove")]
    prove_threshold(&client)?;

    Ok(())
}

/// Proves `secret > threshold` without revealing `secret`, then verifies it as a third party would.
#[cfg(feature = "prove")]
fn prove_threshold(client: &demo::Client) -> noi::Result<()> {
    let proof = demo::above_threshold::prove(
        client,
        demo::above_threshold::Args {
            threshold: 100,
            secret: 250,
        },
    )?;
    println!("proof: {} bytes", proof.proof.len());

    // The verifier only sees the proof and learns the public output `(threshold, above)`.
    let (threshold, above) = demo::above_threshold::verified_output(client, &proof)?;
    println!("verified: secret > {threshold} is {above}");
    assert!(above);
    Ok(())
}
