//! End-to-end proving with a real `bb`. Skipped when no `bb` is found, unless `NOI_REQUIRE_BB=1`
//! (set in CI), in which case a missing `bb` fails the test.
#![cfg(feature = "prove")]

use std::path::PathBuf;

use noi_core::{
    prove::{Prover, VerifierTarget},
    runtime::{Client, FromOutputValue, InputMap, Program, ToInputValue},
    types::FieldElement,
};

fn bb() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("NOI_BB_BIN").map(PathBuf::from),
        Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/bb-bin")
                .join(noi_core::prove::BB_VERSION)
                .join("bb"),
        ),
    ];
    let found = candidates.into_iter().flatten().find(|p| p.is_file());
    if found.is_none() {
        assert!(
            std::env::var("NOI_REQUIRE_BB").as_deref() != Ok("1"),
            "NOI_REQUIRE_BB=1 but no bb found; run scripts/install_bb.sh"
        );
        eprintln!("skipping: no bb found (run scripts/install_bb.sh)");
    }
    found
}

fn setup() -> Option<(Prover, Program, tempfile::TempDir)> {
    let bb = bb()?;
    let cache = tempfile::tempdir().unwrap();
    let prover = Prover::new().with_bb(bb).with_cache_dir(cache.path());
    let program = Program::from_file(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/demo/noir/export/above_threshold.json"),
    )
    .unwrap();
    Some((prover, program, cache))
}

fn inputs(threshold: u32, secret: u32) -> InputMap {
    InputMap::from([
        ("threshold".into(), threshold.to_input_value()),
        ("secret".into(), secret.to_input_value()),
    ])
}

#[test]
fn prove_and_verify_round_trip() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    let execution = program.execute(&Client::new(), &inputs(10, 42)).unwrap();
    let proof = prover.prove(&program, &execution).unwrap();

    // `#[export]` functions cannot mark parameters `pub`; their return values are the public
    // inputs. `above_threshold` returns `(threshold, secret > threshold)`; `secret` stays private.
    assert_eq!(
        proof.public_inputs().unwrap(),
        [FieldElement::from(10u128), FieldElement::from(1u128)]
    );
    assert!(prover.verify(&program, &proof).unwrap());

    // A verifier reads the proven statement back as the function's return type.
    let (threshold, above): (u32, bool) =
        FromOutputValue::from_output_value(proof.return_value(&program).unwrap().unwrap()).unwrap();
    assert_eq!((threshold, above), (10, true));

    // The verification key was cached on first use and is reused.
    let vk = prover.verification_key(&program).unwrap();
    assert!(!vk.is_empty());
}

#[test]
fn tampered_public_inputs_fail_verification() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    let execution = program.execute(&Client::new(), &inputs(10, 42)).unwrap();
    let mut proof = prover.prove(&program, &execution).unwrap();
    // Claim a higher threshold than the one proven.
    proof.public_inputs[31] = 99;
    assert!(!prover.verify(&program, &proof).unwrap());
}

#[test]
fn evm_target_produces_solidity_verifier() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    let prover = prover.with_target(VerifierTarget::Evm);
    let execution = program.execute(&Client::new(), &inputs(1, 2)).unwrap();
    let proof = prover.prove(&program, &execution).unwrap();
    assert!(prover.verify(&program, &proof).unwrap());

    let contract = prover.solidity_verifier(&program).unwrap();
    assert!(contract.contains("contract"), "unexpected verifier output");

    // A proof for one target is rejected by a prover configured for another.
    let default_prover = Prover::new().with_bb(bb().unwrap());
    assert!(default_prover.verify(&program, &proof).is_err());
}
