//! `build.rs` helper for [`noi`](https://docs.rs/noi): runs `nargo export` and points the `nrg!`
//! macro at the result.
//!
//! # Usage
//!
//! ```no_run
//! // build.rs
//! fn main() {
//!     noi_build::BuildOptions::from_env("noir") // path to the Noir package, relative to Cargo.toml
//!         .expect("OUT_DIR is set by cargo")
//!         .export()
//!         .expect("failed to export Noir artifacts");
//! }
//! ```
//!
//! # What [`BuildOptions::export`] does
//!
//! 1. Checks that `program_dir` contains a `Nargo.toml`.
//! 2. Runs `nargo export` there, which writes one JSON artifact per `#[export]` function to
//!    `<program_dir>/export/`.
//! 3. Copies those artifacts to `$OUT_DIR/noi_exports/` and emits
//!    `cargo:rustc-env=NOI_EXPORT_DIR=…`, which is where `nrg!` looks by default.
//! 4. Emits `cargo:rerun-if-changed` for `Nargo.toml` and `src/`, so editing Noir code (including
//!    adding files) re-runs the export.
//!
//! # Building without `nargo`
//!
//! If `nargo` is not installed or fails, the artifacts already in `<program_dir>/export/` are
//! staged instead and a `cargo:warning` explains why. Committing `export/` therefore keeps
//! `cargo build` working on machines and CI runners without the Noir toolchain (and on docs.rs).
//! Set `NOI_SKIP_NARGO=1` to always use the committed artifacts.
//!
//! # Environment variables
//!
//! | Variable | Effect |
//! |---|---|
//! | `NOI_NARGO_BIN` | `nargo` executable to run (default: `nargo` on `PATH`). |
//! | `NOI_SKIP_NARGO=1` | Do not run `nargo`; use committed artifacts. |
//! | `NOI_VERBOSE=1` | Log each step to the build script's stderr (`cargo build -vv` shows it). |
#![warn(missing_docs)]
// Doc snippets show complete `build.rs` files, which need `fn main`.
#![allow(clippy::needless_doctest_main)]

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};
use fs_err as fs;

/// Directory, relative to the Noir package root, where `nargo export` writes artifacts.
pub const NARGO_EXPORT_DIR: &str = "export";
/// Directory inside `OUT_DIR` where artifacts are staged for the macro.
pub const STAGE_SUBDIR: &str = "noi_exports";

/// Where the Noir package lives and where to stage its exports.
///
/// Construct with [`BuildOptions::from_env`] inside `build.rs`, or [`BuildOptions::new`] when you
/// control the output directory yourself.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BuildOptions {
    /// Root of the Noir package (the directory containing `Nargo.toml`).
    pub program_dir: PathBuf,
    /// Directory to stage artifacts under; they end up in `<out_dir>/noi_exports/`.
    pub out_dir: PathBuf,
    /// `nargo` executable. Defaults to `$NOI_NARGO_BIN`, then `nargo`.
    pub nargo_bin: PathBuf,
    /// Skip running `nargo` and stage committed artifacts. Defaults to `NOI_SKIP_NARGO=1`.
    pub skip_nargo: bool,
}

impl BuildOptions {
    /// Options for `program_dir`, staging into `out_dir`.
    pub fn new(program_dir: impl Into<PathBuf>, out_dir: impl Into<PathBuf>) -> Self {
        Self {
            program_dir: program_dir.into(),
            out_dir: out_dir.into(),
            nargo_bin: env::var_os("NOI_NARGO_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| "nargo".into()),
            skip_nargo: env::var("NOI_SKIP_NARGO").as_deref() == Ok("1"),
        }
    }

    /// Options for `program_dir`, staging into cargo's `OUT_DIR`. For use in `build.rs`.
    ///
    /// # Errors
    ///
    /// Fails if `OUT_DIR` is unset, i.e. when not called from a build script.
    pub fn from_env(program_dir: impl Into<PathBuf>) -> Result<Self> {
        let out_dir = env::var_os("OUT_DIR")
            .context("OUT_DIR is not set; BuildOptions::from_env must be called from build.rs")?;
        Ok(Self::new(program_dir, out_dir))
    }

    /// Overrides the `nargo` executable.
    pub fn nargo_bin(mut self, bin: impl Into<PathBuf>) -> Self {
        self.nargo_bin = bin.into();
        self
    }

    /// Skips running `nargo` and stages the committed `export/` artifacts.
    pub fn skip_nargo(mut self, skip: bool) -> Self {
        self.skip_nargo = skip;
        self
    }

    /// Runs the export and staging steps. Shorthand for [`export(&self)`](export).
    ///
    /// # Errors
    ///
    /// See [`export`].
    pub fn export(&self) -> Result<ExportSummary> {
        export(self)
    }
}

/// What [`export`] staged.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExportSummary {
    /// Directory the macro will read (`<out_dir>/noi_exports`).
    pub staged_dir: PathBuf,
    /// Staged artifact paths, sorted.
    pub artifacts: Vec<PathBuf>,
    /// Whether the artifacts are fresh from `nargo` or were already on disk.
    pub source: ArtifactSource,
}

/// Origin of the staged artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArtifactSource {
    /// `nargo export` ran successfully during this build.
    Nargo,
    /// `nargo` was skipped or failed; artifacts already in `export/` were used.
    Committed {
        /// Why `nargo export` did not produce the artifacts.
        reason: String,
    },
}

/// Runs `nargo export` for `opts.program_dir` and stages the artifacts for `nrg!`.
///
/// Prints `cargo:` directives to stdout, so call it from `build.rs`.
///
/// # Errors
///
/// Fails if `program_dir` has no `Nargo.toml`, or if `nargo export` did not run and there are no
/// committed artifacts to fall back on. The error includes the `nargo` failure.
pub fn export(opts: &BuildOptions) -> Result<ExportSummary> {
    let program_dir = &opts.program_dir;
    if !program_dir.join("Nargo.toml").is_file() {
        bail!(
            "`{}` is not a Noir package (no Nargo.toml); point BuildOptions at the directory \
             created by `nargo new --lib`",
            program_dir.display()
        );
    }
    emit_rerun_directives(program_dir);

    let source = if opts.skip_nargo {
        ArtifactSource::Committed {
            reason: "NOI_SKIP_NARGO=1".into(),
        }
    } else {
        match run_nargo_export(&opts.nargo_bin, program_dir) {
            Ok(()) => ArtifactSource::Nargo,
            Err(err) => ArtifactSource::Committed {
                reason: format!("{err:#}"),
            },
        }
    };

    let export_dir = program_dir.join(NARGO_EXPORT_DIR);
    let stage_dir = opts.out_dir.join(STAGE_SUBDIR);
    let artifacts = stage(&export_dir, &stage_dir).with_context(|| match &source {
        ArtifactSource::Nargo => "`nargo export` succeeded but produced no artifacts; are any \
                                  functions marked `#[export]`?"
            .to_owned(),
        ArtifactSource::Committed { reason } => {
            format!("`nargo export` did not run ({reason}) and no committed artifacts exist")
        }
    })?;

    if let ArtifactSource::Committed { reason } = &source {
        if !opts.skip_nargo {
            println!(
                "cargo:warning=noi-build: using committed Noir artifacts from {} because `nargo \
                 export` failed: {reason}. Install nargo {} with `noirup --version {}` to \
                 regenerate them.",
                export_dir.display(),
                noi_core::NOIR_VERSION,
                noi_core::NOIR_VERSION,
            );
        }
    }
    check_versions(&artifacts);

    println!("cargo:rustc-env=NOI_EXPORT_DIR={}", stage_dir.display());
    verbose(&format!(
        "staged {} artifact(s) into {}",
        artifacts.len(),
        stage_dir.display()
    ));

    Ok(ExportSummary {
        staged_dir: stage_dir,
        artifacts,
        source,
    })
}

fn run_nargo_export(bin: &Path, program_dir: &Path) -> Result<()> {
    verbose(&format!(
        "running `{} export` in {}",
        bin.display(),
        program_dir.display()
    ));
    let output = Command::new(bin)
        .arg("export")
        .current_dir(program_dir)
        .output()
        .with_context(|| format!("could not run `{}`", bin.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`{} export` exited with {}: {}",
            bin.display(),
            output.status,
            stderr.trim()
        );
    }
    Ok(())
}

/// Replaces `stage_dir` with copies of the `*.json` files in `export_dir`. Returns them sorted.
fn stage(export_dir: &Path, stage_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut sources = Vec::new();
    for entry in fs::read_dir(export_dir)? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "json") {
            sources.push(path);
        }
    }
    if sources.is_empty() {
        bail!("no *.json artifacts in `{}`", export_dir.display());
    }
    sources.sort();

    // Clear old artifacts so removed `#[export]`s disappear from the bindings.
    if stage_dir.exists() {
        fs::remove_dir_all(stage_dir)?;
    }
    fs::create_dir_all(stage_dir)?;

    sources
        .iter()
        .map(|src| {
            // `read_dir` only yields entries with a file name.
            let dest = stage_dir.join(src.file_name().unwrap_or_default());
            fs::copy(src, &dest)?;
            Ok(dest)
        })
        .collect()
}

/// Warns when artifacts were built by a Noir release the runtime cannot execute.
fn check_versions(artifacts: &[PathBuf]) {
    for path in artifacts {
        let Some(found) = fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
            .and_then(|v| v.get("noir_version")?.as_str().map(str::to_owned))
        else {
            continue;
        };
        if !found.starts_with(noi_core::NOIR_VERSION) {
            println!(
                "cargo:warning=noi-build: {} was built by Noir {found}, but noi executes Noir {} \
                 artifacts; run `noirup --version {}` and rebuild",
                path.display(),
                noi_core::NOIR_VERSION,
                noi_core::NOIR_VERSION,
            );
            // One warning is enough; every artifact comes from the same `nargo`.
            return;
        }
    }
}

fn emit_rerun_directives(program_dir: &Path) {
    // A directory path makes cargo rescan it recursively, so new `.nr` files are picked up too.
    println!(
        "cargo:rerun-if-changed={}",
        program_dir.join("Nargo.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        program_dir.join("src").display()
    );
    for var in ["NOI_NARGO_BIN", "NOI_SKIP_NARGO", "NOI_VERBOSE"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
}

fn verbose(msg: &str) {
    if env::var("NOI_VERBOSE").as_deref() == Ok("1") {
        eprintln!("[noi-build] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_with_exports() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Nargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        std::fs::create_dir(dir.path().join("export")).unwrap();
        std::fs::write(dir.path().join("export/add.json"), "{}").unwrap();
        std::fs::write(dir.path().join("export/notes.txt"), "ignored").unwrap();
        dir
    }

    #[test]
    fn falls_back_to_committed_artifacts() {
        let pkg = package_with_exports();
        let out = tempfile::tempdir().unwrap();
        let summary = BuildOptions::new(pkg.path(), out.path())
            .nargo_bin("/definitely/not/nargo")
            .export()
            .unwrap();
        assert!(matches!(summary.source, ArtifactSource::Committed { .. }));
        assert_eq!(summary.artifacts, [out.path().join("noi_exports/add.json")]);
    }

    #[test]
    fn stale_artifacts_are_removed() {
        let pkg = package_with_exports();
        let out = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(out.path().join("noi_exports")).unwrap();
        std::fs::write(out.path().join("noi_exports/old.json"), "{}").unwrap();
        BuildOptions::new(pkg.path(), out.path())
            .skip_nargo(true)
            .export()
            .unwrap();
        assert!(!out.path().join("noi_exports/old.json").exists());
    }

    #[test]
    fn missing_manifest_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let err = BuildOptions::new(dir.path(), dir.path())
            .export()
            .unwrap_err();
        assert!(err.to_string().contains("Nargo.toml"), "{err}");
    }

    #[test]
    fn no_artifacts_explains_why() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Nargo.toml"), "").unwrap();
        let err = BuildOptions::new(dir.path(), dir.path())
            .skip_nargo(true)
            .export()
            .unwrap_err();
        assert!(format!("{err:#}").contains("NOI_SKIP_NARGO"), "{err:#}");
    }
}
