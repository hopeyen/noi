use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};
use fs_err as fs;
use walkdir::WalkDir;

const EXPORT_SUBDIR: &str = "noi_exports";

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub program_dir: PathBuf,
    pub out_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ExportSummary {
    pub staged_dir: PathBuf,
    pub artifacts: Vec<PathBuf>,
}

pub fn export(opts: &BuildOptions) -> Result<ExportSummary> {
    ensure_program_dir(&opts.program_dir)?;
    let stage_dir = opts.out_dir.join(EXPORT_SUBDIR);
    if stage_dir.exists() {
        fs::remove_dir_all(&stage_dir).with_context(|| {
            format!(
                "failed to clear previous exports at `{}`",
                stage_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&stage_dir)?;

    let run_result = run_nargo(&opts.program_dir);
    emit_cargo_metadata(&opts.program_dir, &stage_dir);

    let staged = match stage_exports(&opts.program_dir, &stage_dir) {
        Ok(result) => {
            if let StageSource::Fallback(ref path) = result.source {
                println!(
                    "cargo:warning=Using pre-exported Noir artifacts from {}",
                    path.display()
                );
                if let Err(err) = &run_result {
                    verbose(&format!("nargo export skipped: {err}"));
                }
            }
            result
        }
        Err(stage_err) => {
            if let Err(run_err) = run_result {
                return Err(stage_err.context(run_err));
            }
            return Err(stage_err);
        }
    };

    Ok(ExportSummary {
        staged_dir: stage_dir,
        artifacts: staged.artifacts,
    })
}

fn ensure_program_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!(
            "program directory `{}` does not exist",
            path.display()
        ));
    }
    if !path.join("Nargo.toml").exists() {
        return Err(anyhow!(
            "`{}` is missing Nargo.toml; point BuildOptions::program_dir at a Noir workspace",
            path.display()
        ));
    }
    Ok(())
}

fn run_nargo(program_dir: &Path) -> Result<()> {
    let bin = env::var("NOI_NARGO_BIN").unwrap_or_else(|_| "nargo".into());
    verbose(&format!(
        "running `{bin} export` in {}",
        program_dir.display()
    ));

    let status = Command::new(&bin)
        .arg("export")
        .current_dir(program_dir)
        .status()
        .with_context(|| format!("failed to start `{bin}`"))?;

    if !status.success() {
        return Err(anyhow!("`{bin} export` failed with status {status}"));
    }

    Ok(())
}

fn stage_exports(program_dir: &Path, dest_dir: &Path) -> Result<StageResult> {
    let target_root = program_dir.join("target");
    if target_root.exists() {
        let staged = copy_artifacts(&target_root, dest_dir)?;
        if !staged.is_empty() {
            verbose(&format!(
                "staged {} export(s) into {}",
                staged.len(),
                dest_dir.display()
            ));
            return Ok(StageResult {
                artifacts: staged,
                source: StageSource::Target,
            });
        }
    }

    let fallback = program_dir.join("exports");
    if fallback.exists() {
        let staged = copy_artifacts(&fallback, dest_dir)?;
        if !staged.is_empty() {
            verbose(&format!(
                "staged {} fallback export(s) into {}",
                staged.len(),
                dest_dir.display()
            ));
            return Ok(StageResult {
                artifacts: staged,
                source: StageSource::Fallback(fallback),
            });
        }
    }

    Err(anyhow!(
        "no JSON artifacts found in `{}` (expected either `target/` or `exports/`)",
        program_dir.display()
    ))
}

fn copy_artifacts(root: &Path, dest_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut staged = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext != "json")
            .unwrap_or(true)
        {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or_else(|_| entry.path());
        let dest = dest_dir.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &dest).with_context(|| {
            format!(
                "failed to copy `{}` to `{}`",
                entry.path().display(),
                dest.display()
            )
        })?;
        staged.push(dest);
    }

    Ok(staged)
}

struct StageResult {
    artifacts: Vec<PathBuf>,
    source: StageSource,
}

enum StageSource {
    Target,
    Fallback(PathBuf),
}

fn emit_cargo_metadata(program_dir: &Path, stage_dir: &Path) {
    println!("cargo:rustc-env=NOI_EXPORT_DIR={}", stage_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        program_dir.join("Nargo.toml").display()
    );

    let src_dir = program_dir.join("src");
    if src_dir.exists() {
        for entry in WalkDir::new(&src_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                println!("cargo:rerun-if-changed={}", entry.path().display());
            }
        }
    }
}

fn verbose(msg: &str) {
    if env::var("NOI_VERBOSE").as_deref() == Ok("1") {
        eprintln!("[noi-build] {msg}");
    }
}
