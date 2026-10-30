//! Generated proving API, end to end. Skipped without `bb` unless `NOI_REQUIRE_BB=1`.
#![cfg(feature = "prove")]

use std::path::PathBuf;

noi::nrg!(module = demo, export_dir = "tests/fixtures/demo");

fn client() -> Option<(demo::Client, tempfile::TempDir)> {
    let bb = std::env::var_os("NOI_BB_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/bb-bin")
                    .join(noi::BB_VERSION)
                    .join("bb"),
            )
        })
        .filter(|p| p.is_file());
    let Some(bb) = bb else {
        assert!(
            std::env::var("NOI_REQUIRE_BB").as_deref() != Ok("1"),
            "NOI_REQUIRE_BB=1 but no bb found; run scripts/install_bb.sh"
        );
        eprintln!("skipping: no bb found (run scripts/install_bb.sh)");
        return None;
    };
    let cache = tempfile::tempdir().unwrap();
    let prover = noi::Prover::new().with_bb(bb).with_cache_dir(cache.path());
    Some((demo::Client::new().with_prover(prover), cache))
}

#[test]
fn generated_prove_and_verify() {
    let Some((client, _cache)) = client() else {
        return;
    };
    let args = demo::above_threshold::Args {
        threshold: 100,
        secret: 250,
    };
    let proof = demo::above_threshold::prove(&client, &args).unwrap();
    assert!(demo::above_threshold::verify(&client, &proof).unwrap());
    assert_eq!(
        demo::above_threshold::verified_output(&client, &proof).unwrap(),
        (100, true)
    );
    assert!(!demo::above_threshold::verification_key(&client)
        .unwrap()
        .is_empty());
}

#[test]
fn proving_a_failing_assertion_is_an_execution_error() {
    let Some((client, _cache)) = client() else {
        return;
    };
    let err =
        demo::checked_div::prove(&client, &demo::checked_div::Args { a: 1, b: 0 }).unwrap_err();
    assert!(
        err.downcast_ref::<noi::ExecutionError>().is_some(),
        "{err:#}"
    );
}
