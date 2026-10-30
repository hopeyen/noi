//! `build.rs` helper for [`noi`](https://docs.rs/noi): runs `nargo export` and points the `nrg!`
//! macro at the result.
//!
//! # Usage
//!
//! ```no_run
//! // build.rs
//! fn main() {
//!     noi_build::BuildOptions::from_env("noir") // Noir package dir, relative to Cargo.toml
//!         .and_then(|opts| opts.export())
//!         .unwrap_or_else(|err| panic!("failed to export Noir artifacts: {err:#}"));
//! }
//! ```
//!
//! # What [`BuildOptions::export`] does
//!
//! 1. Checks that `program_dir` is a Noir **library** package (`type = "lib"` in `Nargo.toml`);
//!    `nargo export` only exports from libraries.
//! 2. Checks that the `nargo` it would run is a stable Noir 1.0.x release (see
//!    [`noi_core::noir_version_compatible`]; 1.0.0 is recommended).
//! 3. Runs `nargo export` in a temporary directory, with a `Nargo.toml` that points at your
//!    package's entry file and path dependencies by absolute path. The build script therefore
//!    never writes to your source tree. `file_map` paths in the artifacts are made relative to the
//!    package, so builds are reproducible across machines.
//! 4. Copies the resulting artifacts to `$OUT_DIR/noi_exports/` and emits
//!    `cargo:rustc-env=NOI_EXPORT_DIR=…`, which is where `nrg!` looks by default.
//! 5. Emits `cargo:rerun-if-changed` for `Nargo.toml` and the source directory (`src/`, or the
//!    directory of `[package] entry`) of the package and every Noir path dependency, and for
//!    `nargo` and its directory (or the `PATH` directories while it is missing), plus
//!    `rerun-if-env-changed` for `PATH` and the variables below. Editing Noir code (including
//!    adding files) or installing or upgrading `nargo` re-runs the export.
//!
//! A Noir compile error fails the build with `nargo`'s full output.
//!
//! # Building without `nargo`
//!
//! If `nargo` is not installed, or is not a compatible release (a prerelease, or another
//! major/minor version), the artifacts committed in
//! `<program_dir>/export/` are staged instead and a `cargo:warning` explains why. To support
//! machines, CI runners, and docs.rs builds without the Noir toolchain, run `nargo export` in the
//! package yourself and commit `export/`. Set `NOI_SKIP_NARGO=1` to always use the committed
//! artifacts.
//!
//! # Environment variables
//!
//! | Variable | Effect |
//! |---|---|
//! | `NOI_NARGO_BIN` | `nargo` executable to run (default: `nargo` on `PATH`). Relative paths are resolved against the crate root. |
//! | `NOI_SKIP_NARGO=1` | Do not run `nargo`; use committed artifacts. |
//! | `NOI_VERBOSE=1` | Log each step to the build script's stderr (`cargo build -vv` shows it). |
#![warn(missing_docs)]
// Doc snippets show complete `build.rs` files, which need `fn main`.
#![allow(clippy::needless_doctest_main)]

use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use fs_err as fs;

/// Directory, relative to the Noir package root, where `nargo export` writes artifacts and where
/// committed fallback artifacts are read from.
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
    /// Root of the Noir library package (the directory containing its `Nargo.toml`).
    pub program_dir: PathBuf,
    /// Directory to stage artifacts under; they end up in `<out_dir>/noi_exports/`.
    pub out_dir: PathBuf,
    /// `nargo` executable. Defaults to `$NOI_NARGO_BIN`, then `nargo` on `PATH`.
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
                .filter(|v| !v.is_empty())
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
    /// Absolute directory the macro will read (`<out_dir>/noi_exports`).
    pub staged_dir: PathBuf,
    /// Staged artifact paths, sorted.
    pub artifacts: Vec<PathBuf>,
    /// Whether the artifacts are fresh from `nargo` or were committed.
    pub source: ArtifactSource,
}

/// Origin of the staged artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArtifactSource {
    /// `nargo export` ran successfully during this build.
    Nargo,
    /// `nargo` was skipped, missing, or an incompatible release; artifacts committed in
    /// `export/` were used.
    Committed {
        /// Why `nargo export` did not run.
        reason: String,
    },
}

/// Runs `nargo export` for `opts.program_dir` and stages the artifacts for `nrg!`.
///
/// Prints `cargo:` directives to stdout, so call it from `build.rs`.
///
/// # Errors
///
/// Fails if `program_dir` is not a Noir library package, if `nargo export` fails (for example on
/// a Noir compile error; the error contains `nargo`'s output), or if `nargo` could not run and
/// there are no compatible committed artifacts to fall back on.
pub fn export(opts: &BuildOptions) -> Result<ExportSummary> {
    // Build scripts run with the crate root as cwd; make every path absolute so the paths we emit
    // mean the same thing to rustc, which runs from the workspace root.
    let program_dir = physical(&opts.program_dir)?;
    let stage_dir = std::path::absolute(opts.out_dir.join(STAGE_SUBDIR))?;
    let nargo_bin = resolve_program(&opts.nargo_bin)?;

    let manifest = read_manifest(&program_dir)?;
    emit_rerun_directives(
        &program_dir,
        &manifest,
        nargo_bin.as_deref(),
        &opts.nargo_bin,
    )?;

    let fresh = if opts.skip_nargo {
        Err("NOI_SKIP_NARGO=1".to_owned())
    } else {
        match &nargo_bin {
            None => Err(format!("`{}` was not found", opts.nargo_bin.display())),
            Some(bin) => match nargo_version(bin) {
                Err(err) => Err(format!("{err:#}")),
                Ok(version) if !noi_core::noir_version_compatible(&version) => Err(format!(
                    "`{}` is nargo {version}, but noi needs {}",
                    bin.display(),
                    noi_core::NOIR_VERSION
                )),
                // From here on, failures are real errors (e.g. Noir compile errors), not fallbacks.
                Ok(_) => Ok(run_nargo_export(bin, &program_dir, &manifest)?),
            },
        }
    };

    let (artifacts, source) = match fresh {
        // `_work` keeps the temporary copy alive until the artifacts are staged.
        Ok((_work, exported)) => {
            let artifacts = stage(&exported, &stage_dir, Some(&program_dir)).with_context(|| {
                "`nargo export` succeeded but produced no artifacts; mark functions with `#[export]`"
            })?;
            (artifacts, ArtifactSource::Nargo)
        }
        Err(reason) => {
            let committed = program_dir.join(NARGO_EXPORT_DIR);
            // Pick up `nargo export` runs and `git pull`s that update the committed artifacts.
            println!("cargo:rerun-if-changed={}", committed.display());
            let artifacts = stage(&committed, &stage_dir, None).with_context(|| {
                format!(
                    "`nargo export` did not run ({reason}) and there are no committed artifacts \
                     in {}",
                    committed.display()
                )
            })?;
            if !opts.skip_nargo {
                println!(
                    "cargo:warning=noi-build: using the committed Noir artifacts in {} because \
                     {reason}. Install nargo {v} (`noirup --version {v}`) to export fresh ones.",
                    committed.display(),
                    v = noi_core::NOIR_VERSION,
                );
            }
            check_committed_versions(&artifacts)?;
            (artifacts, ArtifactSource::Committed { reason })
        }
    };

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

/// Resolves `bin` like a shell would: a bare name is looked up on `PATH`, a relative path with
/// directories is taken relative to the current directory. `None` if it does not exist.
fn resolve_program(bin: &Path) -> Result<Option<PathBuf>> {
    if bin.components().count() > 1 || bin.is_absolute() {
        let path = std::path::absolute(bin)?;
        return Ok(path.is_file().then_some(path));
    }
    let Some(paths) = env::var_os("PATH") else {
        return Ok(None);
    };
    // On Windows try `nargo.exe` first, but accept a name that already has an extension as is.
    let mut names = vec![bin.as_os_str().to_owned()];
    if !env::consts::EXE_SUFFIX.is_empty() && bin.extension().is_none() {
        let mut with_suffix = bin.as_os_str().to_owned();
        with_suffix.push(env::consts::EXE_SUFFIX);
        names.insert(0, with_suffix);
    }
    Ok(env::split_paths(&paths)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file()))
}

/// An absolute path with symlinks resolved (on Unix), so relative `..` dependency paths mean what
/// they mean when nargo runs in place. Windows keeps the plain absolute path, avoiding `\\?\`
/// verbatim paths in the generated `Nargo.toml`.
fn physical(path: &Path) -> Result<PathBuf> {
    if cfg!(unix) {
        if let Ok(path) = std::fs::canonicalize(path) {
            return Ok(path);
        }
    }
    Ok(std::path::absolute(path)?)
}

fn nargo_version(bin: &Path) -> Result<String> {
    let output = Command::new(bin)
        .arg("--version")
        .output()
        .with_context(|| format!("could not run `{}`", bin.display()))?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("nargo version = "))
        .map(|v| v.trim().to_owned())
        .ok_or_else(|| {
            anyhow!(
                "`{} --version` did not report a nargo version",
                bin.display()
            )
        })
}

/// The parts of `Nargo.toml` noi-build relies on.
struct Manifest {
    path: PathBuf,
    toml: toml::Table,
}

fn read_manifest(program_dir: &Path) -> Result<Manifest> {
    let path = program_dir.join("Nargo.toml");
    let text = fs::read_to_string(&path).with_context(|| {
        format!(
            "`{}` is not a Noir package (no readable Nargo.toml); point BuildOptions at the \
             directory created by `nargo new --lib`",
            program_dir.display()
        )
    })?;
    let toml: toml::Table = text
        .parse()
        .with_context(|| format!("invalid {}", path.display()))?;
    if toml.contains_key("workspace") {
        bail!(
            "{} is a Nargo workspace; point BuildOptions at the member library package instead",
            path.display()
        );
    }
    let kind = toml
        .get("package")
        .and_then(|p| p.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("bin");
    if kind != "lib" {
        bail!(
            "{} declares `type = \"{kind}\"`, but `nargo export` only exports from library \
             packages; set `type = \"lib\"`",
            path.display()
        );
    }
    Ok(Manifest { path, toml })
}

/// The package's entry file: `[package] entry` if set, else `src/lib.nr` (`src/main.nr` for
/// binaries), as nargo resolves it. nargo reads every module relative to this file's directory.
fn entry_file(dir: &Path, toml: &toml::Table) -> PathBuf {
    let package = toml.get("package");
    match package
        .and_then(|p| p.get("entry"))
        .and_then(|e| e.as_str())
    {
        Some(entry) => dir.join(entry),
        None => {
            let is_lib =
                package.and_then(|p| p.get("type")).and_then(|t| t.as_str()) == Some("lib");
            dir.join(if is_lib { "src/lib.nr" } else { "src/main.nr" })
        }
    }
}

/// Physical directories of the package's `path` dependencies, as declared in `Nargo.toml`.
fn path_dependencies(manifest: &Manifest) -> Vec<PathBuf> {
    let dir = manifest.path.parent().unwrap_or(Path::new("."));
    manifest
        .toml
        .get("dependencies")
        .and_then(|d| d.as_table())
        .into_iter()
        .flat_map(|deps| deps.values())
        .filter_map(|dep| dep.get("path").and_then(|p| p.as_str()))
        .map(|p| {
            let joined = dir.join(p);
            // Keep the joined path if it does not exist, so nargo reports the missing dependency.
            physical(&joined).unwrap_or(joined)
        })
        .collect()
}

/// Runs `nargo export` against a temporary package whose `Nargo.toml` points at the original
/// sources by absolute path, and returns the directory (deleted on drop) together with the path
/// holding the fresh artifacts. Nothing is written next to the original sources, and the
/// temporary directory lives outside the project so nargo cannot mistake an ancestor `Nargo.toml`
/// for a workspace root.
fn run_nargo_export(
    bin: &Path,
    program_dir: &Path,
    manifest: &Manifest,
) -> Result<(tempfile::TempDir, PathBuf)> {
    let work = tempfile::Builder::new()
        .prefix("noi-build-")
        .tempdir()
        .context("creating a temporary directory for nargo")?;
    let package = work.path().join("package");
    fs::create_dir(&package)?;

    let mut toml = manifest.toml.clone();
    let entry = entry_file(program_dir, &manifest.toml);
    if let Some(table) = toml.get_mut("package").and_then(|p| p.as_table_mut()) {
        table.insert(
            "entry".into(),
            toml::Value::String(entry.to_string_lossy().into_owned()),
        );
    }
    // Relative path dependencies must keep pointing at the original locations.
    if let Some(deps) = toml.get_mut("dependencies").and_then(|d| d.as_table_mut()) {
        for (_, dep) in deps.iter_mut() {
            if let Some(path) = dep.get("path").and_then(|p| p.as_str()) {
                let joined = program_dir.join(path);
                let absolute = physical(&joined).unwrap_or(joined);
                if let Some(dep) = dep.as_table_mut() {
                    dep.insert(
                        "path".into(),
                        toml::Value::String(absolute.to_string_lossy().into_owned()),
                    );
                }
            }
        }
    }
    fs::write(package.join("Nargo.toml"), toml.to_string())?;

    verbose(&format!(
        "running `{} export` for {}",
        bin.display(),
        program_dir.display()
    ));
    let output = Command::new(bin)
        .arg("export")
        .current_dir(&package)
        .output()
        .with_context(|| format!("could not run `{}`", bin.display()))?;
    if !output.status.success() {
        bail!(
            "`{} export` failed for {} with {}:\n{}{}",
            bin.display(),
            program_dir.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
    let exported = package.join(NARGO_EXPORT_DIR);
    Ok((work, exported))
}

/// `path` relative to `base` (with `..` as needed), or `path` unchanged if they share no prefix.
fn relative_to(path: &Path, base: &Path) -> PathBuf {
    let (path_parts, base_parts): (Vec<_>, Vec<_>) =
        (path.components().collect(), base.components().collect());
    let common = path_parts
        .iter()
        .zip(&base_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return path.to_path_buf();
    }
    let mut out = PathBuf::new();
    for _ in common..base_parts.len() {
        out.push("..");
    }
    for part in &path_parts[common..] {
        out.push(part);
    }
    out
}

/// nargo records absolute source paths in `file_map` (debug info only). Make them relative to the
/// Noir package so the embedded `ARTIFACT_JSON`, and therefore the compiled binary, is identical
/// across machines and checkouts. Relative entries (the embedded stdlib) are left alone.
fn normalize_file_map(artifact: &Path, package_dir: &Path) -> Result<()> {
    let text = fs::read_to_string(artifact)?;
    let mut json: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", artifact.display()))?;
    let Some(files) = json.get_mut("file_map").and_then(|m| m.as_object_mut()) else {
        return Ok(());
    };
    for entry in files.values_mut() {
        let Some(path) = entry
            .get("path")
            .and_then(|p| p.as_str())
            .map(PathBuf::from)
        else {
            continue;
        };
        if path.is_absolute() {
            let relative = relative_to(&path, package_dir);
            entry["path"] = serde_json::Value::String(relative.to_string_lossy().into_owned());
        }
    }
    fs::write(artifact, serde_json::to_string(&json)?)?;
    Ok(())
}

/// Replaces `stage_dir` with copies of the `*.json` files in `export_dir`. Returns them sorted.
/// With `normalize_from`, fresh artifacts get package-relative `file_map` paths.
fn stage(
    export_dir: &Path,
    stage_dir: &Path,
    normalize_from: Option<&Path>,
) -> Result<Vec<PathBuf>> {
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
            if let Some(package_dir) = normalize_from {
                normalize_file_map(&dest, package_dir)?;
            }
            Ok(dest)
        })
        .collect()
}

/// Committed artifacts from another Noir release would only fail later, at run time.
fn check_committed_versions(artifacts: &[PathBuf]) -> Result<()> {
    for path in artifacts {
        let found = fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
            .and_then(|v| v.get("noir_version")?.as_str().map(str::to_owned));
        if let Some(found) = found {
            if !noi_core::noir_version_compatible(&found) {
                bail!(
                    "the committed artifact {} was built by Noir {found}, but noi needs {}; \
                     re-run `nargo export` with nargo {} and commit the result",
                    path.display(),
                    noi_core::NOIR_VERSION,
                    noi_core::NOIR_VERSION
                );
            }
        }
    }
    Ok(())
}

fn emit_rerun_directives(
    program_dir: &Path,
    manifest: &Manifest,
    nargo_bin: Option<&Path>,
    requested_bin: &Path,
) -> Result<()> {
    // Directory paths make cargo rescan them recursively, so new `.nr` files are picked up too.
    let mut visited = BTreeSet::new();
    let mut pending = vec![(program_dir.to_path_buf(), manifest.path.clone())];
    while let Some((dir, manifest_path)) = pending.pop() {
        if !visited.insert(dir.clone()) {
            continue;
        }
        println!("cargo:rerun-if-changed={}", manifest_path.display());
        let text = fs::read_to_string(&manifest_path).unwrap_or_default();
        let toml = text.parse::<toml::Table>().unwrap_or_default();
        let sources = entry_file(&dir, &toml);
        if let Some(source_dir) = sources.parent().filter(|d| d.exists()) {
            println!("cargo:rerun-if-changed={}", source_dir.display());
        }
        // Path dependencies change the circuit too; follow them transitively.
        let dep_manifest = Manifest {
            path: manifest_path.clone(),
            toml,
        };
        for dep in path_dependencies(&dep_manifest) {
            pending.push((dep.clone(), dep.join("Nargo.toml")));
        }
    }

    // Re-run when nargo is installed, upgraded, or replaced. Installers unpack a fresh file that
    // may keep the archive's old mtime, but the directory's mtime changes, so watch directories.
    let mut watch = BTreeSet::new();
    match nargo_bin {
        Some(bin) => {
            watch.insert(bin.to_path_buf());
            watch.extend(bin.parent().map(Path::to_path_buf));
            if let Ok(real) = std::fs::canonicalize(bin) {
                watch.extend(real.parent().map(Path::to_path_buf));
            }
        }
        None if requested_bin.components().count() > 1 || requested_bin.is_absolute() => {
            let requested = std::path::absolute(requested_bin)?;
            watch.extend(
                requested
                    .parent()
                    .filter(|d| d.is_dir())
                    .map(Path::to_path_buf),
            );
        }
        None => {
            if let Some(paths) = env::var_os("PATH") {
                watch.extend(env::split_paths(&paths).filter(|d| d.is_dir()));
            }
        }
    }
    for path in watch {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    for var in ["PATH", "NOI_NARGO_BIN", "NOI_SKIP_NARGO", "NOI_VERBOSE"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    Ok(())
}

fn verbose(msg: &str) {
    if env::var("NOI_VERBOSE").as_deref() == Ok("1") {
        eprintln!("[noi-build] {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTIFACT: &str = r#"{"noir_version":"1.0.0+abc","abi":{"parameters":[]}}"#;

    fn package(kind: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Nargo.toml"),
            format!("[package]\nname = \"t\"\ntype = \"{kind}\"\n"),
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("export")).unwrap();
        std::fs::write(dir.path().join("export/add.json"), ARTIFACT).unwrap();
        std::fs::write(dir.path().join("export/notes.txt"), "ignored").unwrap();
        dir
    }

    #[test]
    fn missing_nargo_falls_back_to_committed_artifacts() {
        let pkg = package("lib");
        let out = tempfile::tempdir().unwrap();
        let summary = BuildOptions::new(pkg.path(), out.path())
            .nargo_bin("/definitely/not/nargo")
            .export()
            .unwrap();
        assert!(matches!(summary.source, ArtifactSource::Committed { .. }));
        assert_eq!(summary.artifacts, [out.path().join("noi_exports/add.json")]);
        assert!(summary.staged_dir.is_absolute());
    }

    #[test]
    fn stale_staged_artifacts_are_removed() {
        let pkg = package("lib");
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
    fn bin_packages_and_workspaces_are_rejected() {
        let pkg = package("bin");
        let err = BuildOptions::new(pkg.path(), pkg.path())
            .export()
            .unwrap_err();
        assert!(err.to_string().contains("type = \"lib\""), "{err}");

        let ws = tempfile::tempdir().unwrap();
        std::fs::write(
            ws.path().join("Nargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n",
        )
        .unwrap();
        let err = BuildOptions::new(ws.path(), ws.path())
            .export()
            .unwrap_err();
        assert!(err.to_string().contains("workspace"), "{err}");
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
        let pkg = package("lib");
        std::fs::remove_file(pkg.path().join("export/add.json")).unwrap();
        let err = BuildOptions::new(pkg.path(), pkg.path())
            .skip_nargo(true)
            .export()
            .unwrap_err();
        assert!(format!("{err:#}").contains("NOI_SKIP_NARGO"), "{err:#}");
    }

    #[test]
    fn incompatible_committed_artifacts_fail_the_build() {
        let pkg = package("lib");
        std::fs::write(
            pkg.path().join("export/add.json"),
            r#"{"noir_version":"1.0.0-beta.26+x","abi":{}}"#,
        )
        .unwrap();
        let err = BuildOptions::new(pkg.path(), pkg.path())
            .skip_nargo(true)
            .export()
            .unwrap_err();
        assert!(err.to_string().contains("1.0.0-beta.26"), "{err}");
    }

    #[test]
    fn relative_paths_are_computed() {
        assert_eq!(
            relative_to(Path::new("/a/b/c.nr"), Path::new("/a/b")),
            Path::new("c.nr")
        );
        assert_eq!(
            relative_to(Path::new("/a/dep/x.nr"), Path::new("/a/app")),
            Path::new("../dep/x.nr")
        );
    }

    #[test]
    fn relative_and_bare_programs_resolve() {
        assert_eq!(
            resolve_program(Path::new("definitely-not-a-binary")).unwrap(),
            None
        );
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("nargo");
        std::fs::write(&bin, "").unwrap();
        assert_eq!(resolve_program(&bin).unwrap(), Some(bin));
    }
}
