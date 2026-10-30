//! End-to-end proving with a real `bb`. Skipped when no `bb` is found, unless `NOI_REQUIRE_BB=1`
//! (set in CI), in which case a missing `bb` fails the test.
#![cfg(feature = "prove")]

use std::path::PathBuf;

use noi_core::{
    prove::{Proof, Prover, VerifierTarget, BB_VERSION},
    runtime::{Client, FieldElement, FromValue, InputMap, Program, ToValue},
};

fn bb() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("NOI_BB_BIN").map(PathBuf::from),
        Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/bb-bin")
                .join(BB_VERSION)
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/demo/above_threshold.json"),
    )
    .unwrap();
    Some((prover, program, cache))
}

fn prove(prover: &Prover, program: &Program, threshold: u32, secret: u32) -> Proof {
    let inputs = InputMap::from([
        ("threshold".to_string(), threshold.to_value()),
        ("secret".to_string(), secret.to_value()),
    ]);
    let execution = program.execute(&Client::new(), &inputs).unwrap();
    prover.prove(program, &execution).unwrap()
}

#[test]
fn prove_and_verify_round_trip() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    let proof = prove(&prover, &program, 10, 42);

    // `#[export]` functions cannot mark parameters `pub`; their return values are the public
    // inputs. `above_threshold` returns `(threshold, secret > threshold)`; `secret` stays private.
    assert_eq!(
        proof.public_inputs().unwrap(),
        [FieldElement::from(10u128), FieldElement::from(1u128)]
    );
    assert!(prover.verify(&program, &proof).unwrap());

    let statement: (u32, bool) =
        FromValue::from_value(proof.return_value(&program).unwrap().unwrap()).unwrap();
    assert_eq!(statement, (10, true));
    assert!(!prover.verification_key(&program).unwrap().is_empty());
}

#[test]
fn tampered_public_inputs_fail_verification() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    let mut proof = prove(&prover, &program, 10, 42);
    // Claim a higher threshold than the one proven.
    proof.public_inputs[31] = 99;
    assert!(!prover.verify(&program, &proof).unwrap());
}

#[test]
fn json_public_inputs_are_not_passed_to_bb() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    let proof = prove(&prover, &program, 10, 42);
    // bb would parse this as the (valid) statement [10, 1]; noi must not accept bytes that bb and
    // `Proof::return_value` would read differently.
    let json = br#"{"public_inputs":["0x0a","0x01"]}"#;
    let mut padded = vec![b' '; 64 - json.len()];
    padded.extend_from_slice(json);
    let forged = Proof::from_parts(proof.proof.clone(), padded, VerifierTarget::Default);
    assert!(!prover.verify(&program, &forged).unwrap());
}

#[test]
fn bb_environment_overrides_are_ignored() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    // Would otherwise make a Default prover silently produce non-ZK Keccak proofs.
    std::env::set_var("BB_VERIFIER_TARGET", "evm-no-zk");
    let proof = prove(&prover, &program, 1, 2);
    std::env::remove_var("BB_VERIFIER_TARGET");
    assert!(prover.verify(&program, &proof).unwrap());
    let evm = prover.clone().with_target(VerifierTarget::EvmNoZk);
    assert!(evm
        .verify(
            &program,
            &Proof::from_parts(proof.proof, proof.public_inputs, VerifierTarget::EvmNoZk)
        )
        .map_or(true, |ok| !ok));
}

#[test]
fn evm_target_produces_solidity_verifier() {
    let Some((prover, program, _cache)) = setup() else {
        return;
    };
    let prover = prover.with_target(VerifierTarget::Evm);
    let proof = prove(&prover, &program, 1, 2);
    assert!(prover.verify(&program, &proof).unwrap());

    let contract = prover.solidity_verifier(&program).unwrap();
    assert!(contract.contains("contract"), "unexpected verifier output");

    // A proof for one target is rejected by a prover configured for another.
    let default_prover = Prover::new().with_bb(bb().unwrap());
    assert!(default_prover.verify(&program, &proof).is_err());
}

#[test]
fn wrong_bb_release_is_refused() {
    let Some(bb) = bb() else { return };
    let dir = tempfile::tempdir().unwrap();
    // A wrapper that reports a different release.
    let fake = dir.path().join("bb");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 0.0.1; else exec {} \"$@\"; fi\n",
            bb.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let program = setup().unwrap().1;
    let prover = Prover::new()
        .with_bb(&fake)
        .with_cache_dir(dir.path().join("cache"));
    let err = prover.verification_key(&program).unwrap_err();
    assert!(err.to_string().contains("requires bb"), "{err}");
    assert!(prover
        .allow_any_bb_version(true)
        .verification_key(&program)
        .is_ok());
}
