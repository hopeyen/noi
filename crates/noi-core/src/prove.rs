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
//! Verification keys depend only on the circuit and the [`VerifierTarget`], so they are computed
//! once with `bb write_vk` and cached on disk, keyed by the artifact hash.
//!
//! **The cache is trusted.** `verify` accepts any proof that checks out against the cached key,
//! so whoever can write the cache can make forged proofs verify. The default location is
//! per-user, it is created with mode `0700`, and on Unix `noi` refuses to use a cache directory
//! that is not owned by the current user or is writable by group or others. Do not point
//! `NOI_CACHE_DIR` at a shared directory. `bb` itself is pinned to
//! [`BB_VERSION`], the release paired with [`crate::NOIR_VERSION`]; other versions may reject the
//! bytecode.
//!
//! # Example
//!
//! ```no_run
//! use noi_core::{prove::Prover, runtime::{Client, InputMap, Program, ToInputValue}};
//!
//! let program = Program::from_file("noir/export/add.json")?;
//! let mut inputs = InputMap::new();
//! inputs.insert("a".into(), 2u32.to_input_value());
//! inputs.insert("b".into(), 3u32.to_input_value());
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
//! | `NOI_BB_BIN` | `bb` executable (default: `bb` on `PATH`). `scripts/install_bb.sh` installs the pinned release. |
//! | `NOI_CACHE_DIR` | Verification-key cache (default: `$XDG_CACHE_HOME/noi`, `~/Library/Caches/noi` on macOS, or `~/.cache/noi`). |
//! | `BB_CRS_PATH`-style setups | Use [`Prover::with_crs_path`]; otherwise `bb` downloads the CRS on first use. |

use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

use acvm::AcirField;
use anyhow::{anyhow, bail, Context, Result};
use fs_err as fs;

use crate::runtime::{Execution, FieldElement, InputValue, Program};

/// The Barretenberg release this crate is tested against (paired with [`crate::NOIR_VERSION`]).
pub const BB_VERSION: &str = "5.0.0";

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
    /// Recursive verification without zero knowledge.
    NoirRecursiveNoZk,
    /// Starknet verifier generated with Garaga (ZK).
    Starknet,
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
            VerifierTarget::Starknet => Some("starknet"),
        }
    }

    fn cache_tag(self) -> &'static str {
        self.as_bb_arg().unwrap_or("default")
    }
}

/// A proof and the public inputs it commits to, in `bb`'s binary encoding.
///
/// Both are sequences of 32-byte big-endian field elements.
///
/// # What is public
///
/// Noir only allows `pub` on a program's `main`, so `#[export]` functions have no public
/// parameters: **the return value is the public input**. To commit a proof to an input, return
/// it, e.g. `fn above(threshold: u32, secret: u32) -> (u32, bool)`.
/// [`Proof::return_value`] decodes the public inputs back into the function's return type, which
/// is what a verifier should check before trusting a proof.
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
    /// Fails if the byte length is not a multiple of 32.
    pub fn public_inputs(&self) -> Result<Vec<FieldElement>> {
        if self.public_inputs.len() % 32 != 0 {
            bail!(
                "public inputs are {} bytes, expected a multiple of 32",
                self.public_inputs.len()
            );
        }
        Ok(self
            .public_inputs
            .chunks_exact(32)
            .map(FieldElement::from_be_bytes_reduce)
            .collect())
    }
}

impl Proof {
    /// Decodes the public inputs as `program`'s return value, i.e. the statement this proof
    /// attests to. Returns `None` for functions that return nothing.
    ///
    /// # Errors
    ///
    /// Fails if the number of public inputs does not match the return type.
    pub fn return_value(&self, program: &Program) -> Result<Option<InputValue>> {
        let fields = self.public_inputs()?;
        let Some(ret) = &program.abi().return_type else {
            if fields.is_empty() {
                return Ok(None);
            }
            bail!(
                "`{}` returns nothing but the proof has public inputs",
                program.name()
            );
        };
        let expected = ret.abi_type.field_count() as usize;
        if fields.len() != expected {
            bail!(
                "proof has {} public inputs but `{}` returns {expected} field(s)",
                fields.len(),
                program.name()
            );
        }
        noirc_abi::decode_value(&mut fields.into_iter(), &ret.abi_type, "return")
            .map(Some)
            .map_err(|err| anyhow!("decoding public inputs of `{}`: {err}", program.name()))
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

/// Proves and verifies with the Barretenberg `bb` CLI.
///
/// Cheap to clone; holds only configuration. Every call runs `bb` in a fresh temporary directory,
/// so a `Prover` can be shared across threads. The verification-key cache is written atomically,
/// so concurrent provers (even in different processes) can share a cache directory.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Prover {
    bb: PathBuf,
    target: VerifierTarget,
    crs_path: Option<PathBuf>,
    cache_dir: PathBuf,
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
                .map(PathBuf::from)
                .unwrap_or_else(|| "bb".into()),
            target: VerifierTarget::Default,
            crs_path: None,
            cache_dir: default_cache_dir(),
        }
    }

    /// Uses a specific `bb` executable.
    pub fn with_bb(mut self, bb: impl Into<PathBuf>) -> Self {
        self.bb = bb.into();
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
    /// Fails if `bb` is missing or rejects the bytecode.
    pub fn verification_key(&self, program: &Program) -> Result<Vec<u8>> {
        let path = self.vk_path(program)?;
        fs::read(&path).context("reading cached verification key")
    }

    /// Proves `execution`, which must come from `program.execute(..)`.
    ///
    /// # Errors
    ///
    /// Fails if `bb` is missing, the witness does not satisfy the circuit, or `bb` fails.
    pub fn prove(&self, program: &Program, execution: &Execution) -> Result<Proof> {
        let vk = self.vk_path(program)?;
        let work = tempfile::tempdir().context("creating a temporary directory for bb")?;
        let bytecode = write_bytecode(program, work.path())?;
        let witness = work.path().join("witness.gz");
        fs::write(&witness, execution.witness_bytes()?)?;

        self.run(
            "prove",
            [
                "-b".as_ref(),
                bytecode.as_os_str(),
                "-w".as_ref(),
                witness.as_os_str(),
                "-k".as_ref(),
                vk.as_os_str(),
                "-o".as_ref(),
                work.path().as_os_str(),
            ],
        )
        .with_context(|| format!("proving `{}`", program.name()))?;

        Ok(Proof {
            proof: fs::read(work.path().join("proof"))?,
            public_inputs: fs::read(work.path().join("public_inputs"))?,
            target: self.target,
        })
    }

    /// Verifies `proof` against `program`'s verification key.
    ///
    /// Returns `Ok(false)` when `bb` rejects the proof (including malformed proofs) and `Err` only
    /// when verification could not run.
    ///
    /// # Errors
    ///
    /// Fails if `bb` is missing, crashes, or `proof` was made for a different
    /// [`VerifierTarget`] than this prover's.
    pub fn verify(&self, program: &Program, proof: &Proof) -> Result<bool> {
        if proof.target != self.target {
            bail!(
                "proof was made for verifier target {:?} but this prover verifies {:?}",
                proof.target,
                self.target
            );
        }
        let vk = self.vk_path(program)?;
        let work = tempfile::tempdir().context("creating a temporary directory for bb")?;
        let proof_path = work.path().join("proof");
        let inputs_path = work.path().join("public_inputs");
        fs::write(&proof_path, &proof.proof)?;
        fs::write(&inputs_path, &proof.public_inputs)?;

        let output = self
            .command(
                "verify",
                [
                    "-p".as_ref(),
                    proof_path.as_os_str(),
                    "-i".as_ref(),
                    inputs_path.as_os_str(),
                    "-k".as_ref(),
                    vk.as_os_str(),
                ],
            )?
            .output()
            .with_context(|| self.missing_bb_hint())?;
        // `bb verify` exits 0 for a valid proof and 1 for an invalid one; anything else (e.g. a
        // signal) is a tooling failure.
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => bail!(
                "`bb verify` failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        }
    }

    /// Generates a Solidity verifier contract for `program`.
    ///
    /// # Errors
    ///
    /// Fails unless the target is [`VerifierTarget::Evm`] or [`VerifierTarget::EvmNoZk`].
    pub fn solidity_verifier(&self, program: &Program) -> Result<String> {
        if !matches!(self.target, VerifierTarget::Evm | VerifierTarget::EvmNoZk) {
            bail!("Solidity verifiers require VerifierTarget::Evm or VerifierTarget::EvmNoZk");
        }
        let vk = self.vk_path(program)?;
        let work = tempfile::tempdir()?;
        let out = work.path().join("Verifier.sol");
        self.run(
            "write_solidity_verifier",
            [
                "-k".as_ref(),
                vk.as_os_str(),
                "-o".as_ref(),
                out.as_os_str(),
            ],
        )?;
        Ok(fs::read_to_string(out)?)
    }

    /// Path of the cached verification key, computing it if needed.
    fn vk_path(&self, program: &Program) -> Result<PathBuf> {
        // The artifact hash identifies the compiled circuit; the bb version and target change
        // the key's format and contents.
        let dir = self.cache_dir.join(format!(
            "{}-{:016x}-bb{}-{}",
            program.name(),
            program.artifact_hash(),
            BB_VERSION,
            self.target.cache_tag()
        ));
        let vk = dir.join("vk");
        // Validate before trusting anything already in the cache.
        ensure_private_dir(&self.cache_dir)?;
        if vk.is_file() {
            return Ok(vk);
        }

        let work = tempfile::tempdir_in(&self.cache_dir)
            .context("creating a temporary directory in the noi cache")?;
        let bytecode = write_bytecode(program, work.path())?;
        let out = work.path().join("out");
        fs::create_dir(&out)?;
        self.run(
            "write_vk",
            [
                "-b".as_ref(),
                bytecode.as_os_str(),
                "-o".as_ref(),
                out.as_os_str(),
            ],
        )
        .with_context(|| format!("computing the verification key of `{}`", program.name()))?;

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

    fn command<'a>(
        &self,
        subcommand: &str,
        args: impl IntoIterator<Item = &'a OsStr>,
    ) -> Result<Command> {
        let mut cmd = Command::new(&self.bb);
        cmd.arg(subcommand).args(args);
        if let Some(target) = self.target.as_bb_arg() {
            cmd.args(["-t", target]);
        }
        if let Some(crs) = &self.crs_path {
            cmd.arg("-c").arg(crs);
        }
        Ok(cmd)
    }

    fn run<'a>(&self, subcommand: &str, args: impl IntoIterator<Item = &'a OsStr>) -> Result<()> {
        let output = self
            .command(subcommand, args)?
            .output()
            .with_context(|| self.missing_bb_hint())?;
        if !output.status.success() {
            bail!(
                "`bb {subcommand}` failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    fn missing_bb_hint(&self) -> String {
        format!(
            "could not run `{}`; install Barretenberg {BB_VERSION} (`bbup -v {BB_VERSION}` or \
             `scripts/install_bb.sh`) and/or set NOI_BB_BIN",
            self.bb.display()
        )
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

/// Creates `dir` (mode `0700` on Unix) and, on Unix, rejects it unless it is owned by the current
/// user and not writable by group or others. Otherwise another local user could plant a
/// verification key that makes forged proofs verify.
fn ensure_private_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};

        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .with_context(|| format!("creating the noi cache at {}", dir.display()))?;
        let meta = fs::metadata(dir)?;
        // std has no `getuid`; a file we just created carries our effective uid.
        let probe = tempfile::NamedTempFile::new_in(dir)
            .with_context(|| format!("the noi cache at {} is not writable", dir.display()))?;
        let our_uid = probe.as_file().metadata()?.uid();
        if meta.uid() != our_uid || meta.mode() & 0o022 != 0 {
            bail!(
                "refusing to use the noi cache at {}: it must be owned by the current user and not                  writable by group or others (run `chmod 700` on it, or set NOI_CACHE_DIR)",
                dir.display()
            );
        }
    }
    #[cfg(not(unix))]
    fs::create_dir_all(dir)?;
    Ok(())
}

fn write_bytecode(program: &Program, dir: &Path) -> Result<PathBuf> {
    let path = dir.join("program.json");
    let json = program
        .to_json()
        .map_err(|err| anyhow!("re-encoding `{}`: {err}", program.name()))?;
    fs::write(&path, json)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn misaligned_public_inputs_are_rejected() {
        let proof = Proof::from_parts(vec![], vec![0; 33], VerifierTarget::Default);
        assert!(proof.public_inputs().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn shared_cache_dir_is_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        let err = ensure_private_dir(dir.path()).unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");

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
        let program = Program::from_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/demo/noir/export/add.json"
        ))
        .unwrap();
        let err = prover.verification_key(&program).unwrap_err();
        assert!(
            format!("{err:#}").contains("install Barretenberg"),
            "{err:#}"
        );
    }
}
