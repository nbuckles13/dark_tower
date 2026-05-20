//! WARN-emit helper for auxiliary-scan skip points.
//!
//! Per @team-lead F-SG-2 fold-in 2026-05-19: subcommands walk secondary
//! index trees (catalog `.md`, dashboards `.json`, alert-rule `.yaml`, etc.)
//! with `let Ok(...) = ... else { continue };` to keep one bad file from
//! black-holing the whole subcommand. The cheap mitigation is a one-line
//! `eprintln!` on the skip branch so an operator running with `--explain`
//! (or reading non-verbose CI logs) sees what got skipped. The
//! `run-guards.sh` non-verbose classifier's grep pattern is widened in the
//! same change to surface `WARN` lines alongside `VIOLATION` / `ERROR`.
//!
//! Heavier "collect-and-fail-loudly" restructure stays as a Wave 2+
//! tech-debt entry; the cheap mitigation buys oncall visibility now.

use std::fmt::Display;
use std::path::Path;

/// Emit a WARN line to stderr when an auxiliary scan loop skips a file
/// because of an IO or parse failure. The first argument should describe
/// the scan context (e.g. `"catalog read"`, `"dashboard parse"`); the
/// path identifies the offending file; the error is the underlying cause.
pub fn warn_skip(context: &str, path: &Path, err: &dyn Display) {
    eprintln!(
        "WARN dt-guard auxiliary skip ({context}): {} ({err})",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn warn_skip_formats_one_line() {
        // Smoke: function is total (no panic) and string-format coverage.
        // Output goes to stderr; we don't capture it here — the
        // production-data smoke run in run-guards.sh validates the wire form.
        let path = PathBuf::from("/tmp/example.json");
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        warn_skip("catalog read", &path, &err);
    }
}
