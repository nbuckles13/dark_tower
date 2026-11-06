//! The ONE place dt-guard SPAWNS cargo, POLICY-FREE.
//!
//! The guards that need the resolver (`service_dep_exclusion`'s `cargo tree`,
//! `resolved_features`'s `cargo metadata`) share the binary lookup, working
//! directory and failure-diagnostic shape here; each keeps its own flags
//! (`--locked`, `--offline`, ...), because those legitimately differ.
//!
//! Deliberately separate from `common::cargo_cli`, which parses cargo command
//! TEXT (Dockerfiles, CI, instruction files) and spawns nothing.

use std::path::Path;
use std::process::Command;

/// Run `cargo <args>` against the tree at `repo_root` and return its stdout.
///
/// The binary is `$CARGO` when set (cargo exports it to build scripts and
/// `cargo run`, so a guard launched from cargo uses the same toolchain),
/// otherwise `cargo` on `PATH`. The working directory is `repo_root`, never the
/// caller's cwd. A spawn failure or non-zero exit is `Err` carrying the exit
/// status and the first five stderr lines — callers map it to their own
/// PRECONDITION token, never to a pass.
pub fn run_cargo(repo_root: &Path, args: &[&str]) -> Result<String, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(repo_root)
        // The devloop sets CARGO_TERM_COLOR=always; escapes in a PRECONDITION
        // line are noise to whoever reads it.
        .env("CARGO_TERM_COLOR", "never")
        .args(args)
        .output()
        .map_err(|e| format!("spawning cargo: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let head: Vec<&str> = stderr.lines().take(5).collect();
        return Err(format!("{}: {}", output.status, head.join(" | ")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
