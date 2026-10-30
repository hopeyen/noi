//! Proving and verifying exported functions with Barretenberg (feature `prove`).
//!
//! Noir is backend-agnostic; the de facto backend is Aztec's Barretenberg, whose `bb` CLI proves
//! with UltraHonk (see <https://noir-lang.org/docs/getting_started/quick_start>). [`Prover`] drives
//! that CLI:
//!
//! 1. [`Program::execute`] solves the circuit in-process and yields the witness.
//! 2. [`Prover::prove`] hands the artifact and witness to `bb prove`, reusing a cached
//!    verification key, and returns a [`Proof`] (proof bytes + public inputs).
//! 3. [`Prover::verify`] checks a proof with `bb verify`.
//!
//! # What a proof makes public
//!
//! Noir only allows `pub` on a program's `main`, so `#[export]` functions have no public
//! parameters: **a proof's public inputs are exactly the function's return value**. Return any
//! input the proof must commit to, and read the proven statement back with
//! [`Proof::return_value`].
//!
//! # Verification keys and trust
//!
//! A verification key depends on the circuit, the [`VerifierTarget`], and the `bb` release, so it
//! is computed once with `bb write_vk` and cached on disk under a SHA-256 of the artifact plus the
//! target and `bb --version`.
//!
//! **The cache is trusted.** `verify` accepts any proof that checks out against the cached key,
//! so whoever can write the cache can make forged proofs verify. The default location is
//! per-user. On Unix it is created with mode `0700`, and `noi` refuses a cache directory that is
//! not owned by the current user or is writable by group or others; other platforms get no such
//! check. Never point `NOI_CACHE_DIR` at a shared directory.
//!
//! # Limitations
//!
//! bb's UltraHonk prover only accepts single-circuit programs, so functions that call `#[fold]`
//! functions can be simulated but not proved; the prover reports this before invoking `bb`.
//!
//! `bb` must be release [`BB_VERSION`], the one paired with [`crate::NOIR_VERSION`]; the prover
//! checks `bb --version` once and refuses other releases unless
//! [`Prover::allow_any_bb_version`] is set.
//!
//! # Example
//!
//! ```no_run
//! use noi_core::{prove::Prover, runtime::{Client, InputMap, Program, ToValue}};
//!
//! let program = Program::from_file("noir/export/above_threshold.json")?;
//! let inputs = InputMap::from([
//!     ("threshold".to_string(), 100u32.to_value()),
//!     ("secret".to_string(), 250u32.to_value()),
//! ]);
//! let execution = program.execute(&Client::new(), &inputs)?;
//!
//! let prover = Prover::new();
//! let proof = prover.prove(&program, &execution)?;
//! assert!(prover.verify(&program, &proof)?);
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # Configuration
//!
//! | Variable | Effect |
//! |---|---|
//! | `NOI_BB_BIN` | `bb` executable (default: `bb` on `PATH`). The repository's `scripts/install_bb.sh` installs the pinned release and prints its path for this variable. |
//! | `NOI_CACHE_DIR` | Verification-key cache (default: `$XDG_CACHE_HOME/noi`, `~/Library/Caches/noi` on macOS, or `~/.cache/noi`). |
//!
//! `bb` downloads its CRS on first use; pass [`Prover::with_crs_path`] to use a local copy.
//! `BB_VERIFIER_TARGET` and `BB_SCHEME` in the environment are ignored, so a [`Prover`] always
//! proves for the target it was configured with.

use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{Arc, OnceLock},
};

use acvm::AcirField;
use anyhow::{bail, Context, Result};
use fs_err as fs;
use sha2::{Digest, Sha256};

use crate::runtime::{abi, Execution, FieldElement, Program, Value};

/// The Barretenberg release paired with [`crate::NOIR_VERSION`] (Noir's `@aztec-foundation/bb.js`
/// dependency).
pub const BB_VERSION: &str = "6.0.0";

/// Upper bound on the size of a proof accepted by [`Prover::verify`]. bb 6 UltraHonk proofs are
/// at most 458 words (14,656 bytes); anything far larger cannot verify.
const MAX_PROOF_BYTES: usize = 64 * 1024;

/// Where a proof will be verified. Selects the transcript hash and zero-knowledge setting; proofs
/// and verification keys are only valid for the target they were made for.
///
/// Mirrors `bb --verifier_target`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VerifierTarget {
    /// `bb`'s default (UltraHonk, Poseidon2 transcript, ZK). Use for off-chain verification.
    #[default]
    Default,
    /// Solidity verifier contract (Keccak transcript, ZK). Required for
    /// [`Prover::solidity_verifier`].
    Evm,
    /// Solidity verifier without zero knowledge (cheaper to verify, reveals more).
    EvmNoZk,
    /// Recursive verification inside another Noir circuit (Poseidon2, ZK).
    NoirRecursive,
    /// Recursive verification inside another Noir circuit, without zero knowledge.
    NoirRecursiveNoZk,
}

impl VerifierTarget {
    /// The `--verifier_target` value, or `None` for [`VerifierTarget::Default`].
    pub fn as_bb_arg(self) -> Option<&'static str> {
        match self {
            VerifierTarget::Default => None,
            VerifierTarget::Evm => Some("evm"),
            VerifierTarget::EvmNoZk => Some("evm-no-zk"),
            VerifierTarget::NoirRecursive => Some("noir-recursive"),
            VerifierTarget::NoirRecursiveNoZk => Some("noir-recursive-no-zk"),
        }
    }

    fn cache_tag(self) -> &'static str {
        self.as_bb_arg().unwrap_or("default")
    }
}

/// A proof and the public inputs it commits to, in `bb`'s binary encoding.
///
/// Both are sequences of 32-byte big-endian field elements. For an `#[export]` function the
/// public inputs are its return value; [`Proof::return_value`] decodes them.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Proof {
    /// The proof, as written by `bb prove` to `proof`.
    pub proof: Vec<u8>,
    /// The public inputs, as written by `bb prove` to `public_inputs`.
    pub public_inputs: Vec<u8>,
    /// The verifier target the proof was made for.
    pub target: VerifierTarget,
}

impl Proof {
    /// Reassembles a proof received from elsewhere (e.g. a browser prover using `bb.js`).
    pub fn from_parts(proof: Vec<u8>, public_inputs: Vec<u8>, target: VerifierTarget) -> Self {
        Self {
            proof,
            public_inputs,
            target,
        }
    }

    /// Decodes the public inputs into field elements.
    ///
    /// # Errors
    ///
    /// Fails unless the bytes are a whole number of canonical 32-byte big-endian field elements
    /// (each below the BN254 scalar modulus).
    pub fn public_inputs(&self) -> Result<Vec<FieldElement>> {
        decode_canonical_fields(&self.public_inputs, "public inputs")
    }

    /// Decodes the public inputs as `program`'s return value: the statement this proof attests
    /// to. Returns `None` for functions that return nothing.
    ///
    /// This does not verify the proof; call [`Prover::verify`] first (generated
    /// `verified_output` functions do both).
    ///
    /// # Errors
    ///
    /// Fails if the public inputs are malformed or do not match the return type.
    pub fn return_value(&self, program: &Program) -> Result<Option<Value>> {
        let fields = self.public_inputs()?;
        let Some(ty) = &program.function().return_type else {
            if fields.is_empty() {
                return Ok(None);
            }
            bail!(
                "`{}` returns nothing but the proof has public inputs",
                program.name()
            );
        };
        let expected = abi::field_count(ty);
        if fields.len() != expected {
            bail!(
                "proof has {} public inputs but `{}` returns {expected} field(s)",
                fields.len(),
                program.name()
            );
        }
        // Strict: a proven `str<N>` with non-UTF-8 bytes must not be collapsed into the same
        // lossy string as a different statement.
        abi::decode_value_strict(&mut fields.into_iter(), ty, "return")
            .map(Some)
            .with_context(|| format!("decoding the public inputs of `{}`", program.name()))
    }
}

impl fmt::Debug for Proof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Proof")
            .field("proof_len", &self.proof.len())
            .field("public_inputs", &self.public_inputs().unwrap_or_default())
            .field("target", &self.target)
            .finish()
    }
}

/// Splits `bytes` into canonical 32-byte big-endian field elements.
fn decode_canonical_fields(bytes: &[u8], what: &str) -> Result<Vec<FieldElement>> {
    let (chunks, rest) = bytes.as_chunks::<32>();
    if !rest.is_empty() {
        bail!(
            "{what} are {} bytes, expected a multiple of 32",
            bytes.len()
        );
    }
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let field = FieldElement::from_be_bytes_reduce(chunk);
            // Reject values >= the modulus: they would be read differently by different tools.
            if field.to_be_bytes() != chunk {
                bail!("{what}: element {i} is not a canonical field element");
            }
            Ok(field)
        })
        .collect()
}

/// Proves and verifies with the Barretenberg `bb` CLI.
///
/// Cheap to clone (configuration plus a shared, lazily checked `bb` version). Every call runs
/// `bb` in a fresh temporary directory, so a `Prover` can be shared across threads, and the
/// verification-key cache is written atomically, so concurrent provers (even in different
/// processes) can share a cache directory.
#[derive(Clone)]
pub struct Prover {
    bb: PathBuf,
    target: VerifierTarget,
    crs_path: Option<PathBuf>,
    cache_dir: PathBuf,
    any_bb_version: bool,
    bb_version: Arc<OnceLock<String>>,
}

impl fmt::Debug for Prover {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prover")
            .field("bb", &self.bb)
            .field("target", &self.target)
            .field("crs_path", &self.crs_path)
            .field("cache_dir", &self.cache_dir)
            .finish_non_exhaustive()
    }
}

impl Default for Prover {
    fn default() -> Self {
        Self::new()
    }
}

impl Prover {
    /// A prover using `$NOI_BB_BIN` (or `bb` on `PATH`), the default verifier target, and the
    /// per-user verification-key cache (`$NOI_CACHE_DIR`, else the platform cache directory).
    pub fn new() -> Self {
        Self {
            bb: std::env::var_os("NOI_BB_BIN")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| "bb".into()),
            target: VerifierTarget::Default,
            crs_path: None,
            cache_dir: default_cache_dir(),
            any_bb_version: false,
            bb_version: Arc::default(),
        }
    }

    /// Uses a specific `bb` executable.
    pub fn with_bb(mut self, bb: impl Into<PathBuf>) -> Self {
        self.bb = bb.into();
        self.bb_version = Arc::default();
        self
    }

    /// Sets where proofs will be verified (default: [`VerifierTarget::Default`]).
    pub fn with_target(mut self, target: VerifierTarget) -> Self {
        self.target = target;
        self
    }

    /// Uses a local CRS directory instead of letting `bb` download it.
    pub fn with_crs_path(mut self, dir: impl Into<PathBuf>) -> Self {
        self.crs_path = Some(dir.into());
        self
    }

    /// Stores verification keys under `dir`. It must be private to the current user; see the
    /// [module docs](self) for why.
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = dir.into();
        self
    }

    /// Accepts any `bb` release instead of requiring [`BB_VERSION`]. Proofs and keys from
    /// different releases are generally incompatible; use this only for testing newer releases.
    pub fn allow_any_bb_version(mut self, allow: bool) -> Self {
        self.any_bb_version = allow;
        self
    }

    /// The configured verifier target.
    pub fn target(&self) -> VerifierTarget {
        self.target
    }

    /// Returns the verification key for `program`, computing it with `bb write_vk` on first use.
    ///
    /// The key is what an off-chain verifier, a Solidity verifier, or a recursive Noir circuit
    /// needs; share it alongside proofs.
    ///
    /// # Errors
    ///
    /// Fails if `bb` is missing, is the wrong release, or rejects the bytecode.
    pub fn verification_key(&self, program: &Program) -> Result<Vec<u8>> {
        let path = self.vk_path(program)?;
        read_vk(&path)
    }

    /// Proves `execution`, which must come from `program.execute(..)`.
    ///
    /// # Errors
    ///
    /// Fails if `bb` is missing or the wrong release, or if `bb prove` fails.
    pub fn prove(&self, program: &Program, execution: &Execution) -> Result<Proof> {
        let vk = self.vk_path(program)?;
        let work = tempfile::tempdir().context("creating a temporary directory for bb")?;
        let bytecode = write_bytecode(program, work.path())?;
        let witness = work.path().join("witness.gz");
        fs::write(&witness, execution.witness_bytes()?)?;
        let out = work.path().join("out");
        fs::create_dir(&out)?;

        let output = self.run(
            "prove",
            [
                "-b".as_ref(),
                bytecode.as_os_str(),
                "-w".as_ref(),
                witness.as_os_str(),
                "-k".as_ref(),
                vk.as_os_str(),
                "-o".as_ref(),
                out.as_os_str(),
            ],
        )?;
        ensure_success("prove", &output)
            .with_context(|| format!("proving `{}`", program.name()))?;

        Ok(Proof {
            proof: fs::read(out.join("proof"))?,
            public_inputs: fs::read(out.join("public_inputs"))?,
            target: self.target,
        })
    }

    /// Verifies `proof` against `program`'s verification key.
    ///
    /// Returns `Ok(false)` when the proof is invalid: `bb` rejects it, or its bytes are not in
    /// `bb`'s canonical binary encoding (for example JSON, which `bb` would otherwise interpret
    /// differently from [`Proof::return_value`]).
    ///
    /// # Errors
    ///
    /// Fails if verification could not run: `bb` is missing or the wrong release, the cached key
    /// is unreadable, `bb` reports an error other than a rejected proof, or `proof` was made for
    /// a different [`VerifierTarget`] than this prover's.
    pub fn verify(&self, program: &Program, proof: &Proof) -> Result<bool> {
        if proof.target != self.target {
            bail!(
                "proof was made for verifier target {:?} but this prover verifies {:?}",
                proof.target,
                self.target
            );
        }
        let expected = program
            .function()
            .return_type
            .as_ref()
            .map_or(0, abi::field_count);
        // Bound untrusted input before decoding it: honest proofs are a few hundred words, and the
        // JSON handed to bb is about ten times the input size.
        if proof.proof.len() > MAX_PROOF_BYTES || proof.public_inputs.len() != 32 * expected {
            return Ok(false);
        }
        // Decode the statement exactly as `Proof::return_value` does, and require canonical
        // proof words as bb does; anything else is an invalid proof.
        let Ok(public_inputs) = proof.public_inputs() else {
            return Ok(false);
        };
        let (proof_words, rest) = proof.proof.as_chunks::<32>();
        if !rest.is_empty()
            || proof_words.is_empty()
            || decode_canonical_fields(&proof.proof, "proof").is_err()
        {
            return Ok(false);
        }
        // Read the key first so a missing or unreadable cache entry is an error, not "invalid".
        let vk = read_vk(&self.vk_path(program)?)?;

        // `bb` reads a file as JSON when its first non-space byte is `{` (and stops at a NUL), so
        // passing the caller's raw bytes could make it check a different statement than the one
        // decoded above. Always hand it explicit JSON built from the decoded values instead.
        let hex = |word: &[u8]| {
            let digits: String = word.iter().map(|b| format!("{b:02x}")).collect();
            serde_json::Value::String(format!("0x{digits}"))
        };
        let proof_json =
            serde_json::json!({ "proof": proof_words.iter().map(|w| hex(w)).collect::<Vec<_>>() });
        let inputs_json = serde_json::json!({
            "public_inputs": public_inputs.iter().map(|f| hex(&f.to_be_bytes())).collect::<Vec<_>>()
        });

        let work = tempfile::tempdir().context("creating a temporary directory for bb")?;
        let (proof_path, inputs_path, vk_path) = (
            work.path().join("proof.json"),
            work.path().join("public_inputs.json"),
            work.path().join("vk"),
        );
        fs::write(&proof_path, proof_json.to_string())?;
        fs::write(&inputs_path, inputs_json.to_string())?;
        fs::write(&vk_path, vk)?;

        let output = self.run(
            "verify",
            [
                "-v".as_ref(),
                "-p".as_ref(),
                proof_path.as_os_str(),
                "-i".as_ref(),
                inputs_path.as_os_str(),
                "-k".as_ref(),
                vk_path.as_os_str(),
            ],
        )?;
        classify_verify(&output)
    }

    /// Generates a Solidity verifier contract for `program`.
    ///
    /// # Errors
    ///
    /// Fails unless the target is [`VerifierTarget::Evm`] or [`VerifierTarget::EvmNoZk`], or if
    /// `bb` fails.
    pub fn solidity_verifier(&self, program: &Program) -> Result<String> {
        if !matches!(self.target, VerifierTarget::Evm | VerifierTarget::EvmNoZk) {
            bail!("Solidity verifiers require VerifierTarget::Evm or VerifierTarget::EvmNoZk");
        }
        let vk = self.vk_path(program)?;
        let work = tempfile::tempdir()?;
        let out = work.path().join("Verifier.sol");
        let output = self.run(
            "write_solidity_verifier",
            [
                "-k".as_ref(),
                vk.as_os_str(),
                "-o".as_ref(),
                out.as_os_str(),
            ],
        )?;
        ensure_success("write_solidity_verifier", &output)?;
        Ok(fs::read_to_string(out)?)
    }

    /// Checks `bb --version`. A successful answer is cached for this prover and its clones; a
    /// failure (e.g. `bb` not installed yet) is retried on the next call.
    fn checked_bb_version(&self) -> Result<&str> {
        let version = match self.bb_version.get() {
            Some(version) => version.as_str(),
            None => {
                let output = Command::new(&self.bb)
                    .arg("--version")
                    .output()
                    .with_context(|| self.missing_bb_hint())?;
                let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                if !output.status.success() || version.is_empty() {
                    bail!("`{} --version` failed", self.bb.display());
                }
                // A concurrent caller may have stored it first; both ran the same binary.
                self.bb_version.get_or_init(|| version).as_str()
            }
        };
        if !self.any_bb_version && version != BB_VERSION {
            bail!(
                "`{}` is bb {version}, but noi requires bb {BB_VERSION} (paired with Noir {}); \
                 install it with `bbup -v {BB_VERSION}` or scripts/install_bb.sh, or opt out \
                 with Prover::allow_any_bb_version",
                self.bb.display(),
                crate::NOIR_VERSION
            );
        }
        Ok(version)
    }

    /// Path of the cached verification key, computing it if needed.
    fn vk_path(&self, program: &Program) -> Result<PathBuf> {
        let circuits = program.circuit_count();
        if circuits > 1 {
            bail!(
                "`{}` compiles to {circuits} ACIR circuits because it calls `#[fold]` functions; \
                 bb {BB_VERSION}'s UltraHonk prover only supports single-circuit programs \
                 (remove `#[fold]` to prove it)",
                program.name()
            );
        }
        let bb_version = self.checked_bb_version()?;
        // Key on the exact artifact bytes (not the self-reported `hash`, which does not cover
        // codegen), the target, and the bb release. The name is only a readable, sanitized prefix.
        let mut hasher = Sha256::new();
        hasher.update(program.artifact_json().as_bytes());
        hasher.update([0]);
        hasher.update(self.target.cache_tag().as_bytes());
        hasher.update([0]);
        hasher.update(bb_version.as_bytes());
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        // Validate before trusting anything already in the cache, and use the resolved directory
        // from here on so it cannot be swapped out underneath us.
        let cache_dir = ensure_private_dir(&self.cache_dir)?;
        let dir = cache_dir.join(format!("{}-{}", sanitize_name(program.name()), &hex[..32]));
        let vk = dir.join("vk");
        if vk.is_file() {
            return Ok(vk);
        }

        let work = tempfile::tempdir_in(&cache_dir)
            .context("creating a temporary directory in the noi cache")?;
        let bytecode = write_bytecode(program, work.path())?;
        let out = work.path().join("out");
        fs::create_dir(&out)?;
        let output = self.run(
            "write_vk",
            [
                "-b".as_ref(),
                bytecode.as_os_str(),
                "-o".as_ref(),
                out.as_os_str(),
            ],
        )?;
        ensure_success("write_vk", &output)
            .with_context(|| format!("computing the verification key of `{}`", program.name()))?;
        read_vk(&out.join("vk"))?;

        // Publish atomically: if another process won the race, keep its (identical) key.
        match std::fs::rename(&out, &dir) {
            Ok(()) => {}
            Err(_) if vk.is_file() => {}
            Err(err) => {
                return Err(err).with_context(|| format!("caching the key in {}", dir.display()))
            }
        }
        Ok(vk)
    }

    fn run<'a>(
        &self,
        subcommand: &str,
        args: impl IntoIterator<Item = &'a OsStr>,
    ) -> Result<Output> {
        let mut cmd = Command::new(&self.bb);
        cmd.arg(subcommand)
            .args(["--scheme", "ultra_honk"])
            .args(args)
            // These would silently override the target and scheme chosen above.
            .env_remove("BB_VERIFIER_TARGET")
            .env_remove("BB_SCHEME");
        if let Some(target) = self.target.as_bb_arg() {
            cmd.args(["-t", target]);
        }
        if let Some(crs) = &self.crs_path {
            cmd.arg("-c").arg(crs);
        }
        cmd.output().with_context(|| self.missing_bb_hint())
    }

    fn missing_bb_hint(&self) -> String {
        format!(
            "could not run `{}`; install Barretenberg {BB_VERSION} (`bbup -v {BB_VERSION}` or \
             scripts/install_bb.sh) and/or set NOI_BB_BIN",
            self.bb.display()
        )
    }
}

fn ensure_success(subcommand: &str, output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "`bb {subcommand}` failed with {}: {}",
        output.status,
        bb_messages(output)
    )
}

/// `bb`'s diagnostic lines, without the memory-usage suffixes it appends.
fn bb_messages(output: &Output) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    text.lines()
        .map(|line| line.split(" (mem:").next().unwrap_or(line).trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Interprets `bb verify -v`.
///
/// bb 6 exits 1 both for invalid proofs and for tooling errors, so the output decides:
/// - `verified: 0` (sometimes with `Proof verification failed: invalid proof size …`) is a
///   rejected proof;
/// - after the key loaded (`vk hash in Oink verifier`), a proof that cannot be deserialized
///   (`Deserialized point is not on the curve`, `… bad proof serde or parsing`) is also rejected;
/// - anything else, such as `Proof verification failed: invalid VK size …`, is an error.
fn classify_verify(output: &Output) -> Result<bool> {
    let messages = bb_messages(output);
    let lines: Vec<&str> = messages.split("; ").collect();
    let tooling_error = lines.iter().any(|line| {
        line.starts_with("Proof verification failed:") && !line.contains("invalid proof size")
    });
    let vk_loaded = lines
        .iter()
        .any(|line| line.starts_with("vk hash in Oink verifier"));
    let bad_proof_encoding = lines.iter().any(|line| {
        line.contains("Deserialized point is not on the curve")
            || line.contains("usually implies some bad proof serde or parsing")
    });
    match output.status.code() {
        Some(0) if messages.contains("verified: 1") => Ok(true),
        Some(1) if messages.contains("verified: 0") && !tooling_error => Ok(false),
        Some(1) if vk_loaded && bad_proof_encoding => Ok(false),
        _ => bail!("`bb verify` failed with {}: {messages}", output.status),
    }
}

fn read_vk(path: &Path) -> Result<Vec<u8>> {
    let vk = fs::read(path).context("reading the verification key")?;
    if vk.is_empty() {
        bail!("the verification key at {} is empty", path.display());
    }
    Ok(vk)
}

/// Keeps `[A-Za-z0-9_]`, so a caller-chosen program name cannot escape the cache directory.
fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "program".to_owned()
    } else {
        cleaned
    }
}

/// `$NOI_CACHE_DIR`, else the per-user platform cache directory. Falls back to the system temp dir
/// only when no home directory is known; [`ensure_private_dir`] still guards that case.
fn default_cache_dir() -> PathBuf {
    let env_dir = |var: &str| {
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = env_dir("NOI_CACHE_DIR") {
        return dir;
    }
    if let Some(dir) = env_dir("XDG_CACHE_HOME") {
        return dir.join("noi");
    }
    match env_dir("HOME") {
        Some(home) if cfg!(target_os = "macos") => home.join("Library/Caches/noi"),
        Some(home) => home.join(".cache/noi"),
        None => std::env::temp_dir().join("noi"),
    }
}

/// Creates `dir` (mode `0700` on Unix) and returns it with symlinks resolved. On Unix, the
/// resolved directory must be owned by the current user and not writable by group or others;
/// otherwise another local user could plant a verification key that makes forged proofs verify.
/// A symlink to a private directory (a cache relocated to another disk) is fine. Other platforms
/// are not checked; keep `NOI_CACHE_DIR` in a per-user location there.
fn ensure_private_dir(dir: &Path) -> Result<PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};

        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .with_context(|| format!("creating the noi cache at {}", dir.display()))?;
        let real = fs::canonicalize(dir)
            .with_context(|| format!("resolving the noi cache at {}", dir.display()))?;
        let meta = fs::metadata(&real)?;
        // SAFETY: `geteuid` has no preconditions and cannot fail. Reading the uid this way (rather
        // than creating a probe file) keeps a read-only, pre-populated cache usable.
        let our_uid = unsafe { libc::geteuid() };
        if !meta.is_dir() || meta.uid() != our_uid || meta.mode() & 0o022 != 0 {
            bail!(
                "refusing to use the noi cache at {} (resolved to {}): it must be a directory \
                 owned by the current user and not writable by group or others (run `chmod 700` \
                 on it, or set NOI_CACHE_DIR)",
                dir.display(),
                real.display()
            );
        }
        Ok(real)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)?;
        Ok(std::path::absolute(dir)?)
    }
}

fn write_bytecode(program: &Program, dir: &Path) -> Result<PathBuf> {
    let path = dir.join("program.json");
    fs::write(&path, program.artifact_json())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Program {
        Program::from_file(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/demo")
                .join(format!("{name}.json")),
        )
        .unwrap()
    }

    #[test]
    fn public_inputs_decode_big_endian() {
        let mut bytes = vec![0u8; 64];
        bytes[31] = 7;
        bytes[63] = 1;
        let proof = Proof::from_parts(vec![], bytes, VerifierTarget::Default);
        assert_eq!(
            proof.public_inputs().unwrap(),
            [FieldElement::from(7u128), FieldElement::from(1u128)]
        );
    }

    #[test]
    fn non_canonical_inputs_are_rejected() {
        let misaligned = Proof::from_parts(vec![], vec![0; 33], VerifierTarget::Default);
        assert!(misaligned.public_inputs().is_err());
        let above_modulus = Proof::from_parts(vec![], vec![0xff; 32], VerifierTarget::Default);
        assert!(above_modulus.public_inputs().is_err());
    }

    #[test]
    fn folded_programs_are_refused_before_running_bb() {
        let prover = Prover::new()
            .with_bb("/definitely/not/bb")
            .with_cache_dir(tempfile::tempdir().unwrap().path());
        let err = prover
            .verification_key(&fixture("sum_of_squares"))
            .unwrap_err();
        assert!(err.to_string().contains("single-circuit"), "{err}");
    }

    #[test]
    fn names_cannot_escape_the_cache() {
        assert_eq!(sanitize_name("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_name("/abs"), "abs");
        assert_eq!(sanitize_name("..."), "program");
    }

    #[cfg(unix)]
    #[test]
    fn verify_output_is_classified() {
        use std::os::unix::process::ExitStatusExt;
        let out = |code: i32, stderr: &str| Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        };
        assert!(classify_verify(&out(0, "verified: 1 (mem: 1 MiB)")).unwrap());
        assert!(!classify_verify(&out(1, "Proof verification failed\nverified: 0")).unwrap());
        assert!(!classify_verify(&out(
            1,
            "verified: 0\nProof verification failed: invalid proof size. Expected 457, got 3"
        ))
        .unwrap());
        assert!(classify_verify(&out(
            1,
            "verified: 0\nProof verification failed: invalid VK size. Expected 3680 bytes, got 100"
        ))
        .is_err());
        assert!(classify_verify(&out(2, "")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn shared_cache_dir_is_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        let err = ensure_private_dir(dir.path()).unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");
        assert!(
            !err.to_string().contains("  "),
            "message has stray spaces: {err}"
        );

        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        ensure_private_dir(dir.path()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn new_cache_dir_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("a/b/noi");
        ensure_private_dir(&dir).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn missing_bb_mentions_install() {
        let prover = Prover::new()
            .with_bb("/definitely/not/bb")
            .with_cache_dir(tempfile::tempdir().unwrap().path());
        let err = prover.verification_key(&fixture("add")).unwrap_err();
        assert!(
            format!("{err:#}").contains("install Barretenberg"),
            "{err:#}"
        );
    }
}
