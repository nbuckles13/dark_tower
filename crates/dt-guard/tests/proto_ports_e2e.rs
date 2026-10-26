//! End-to-end tier for the two ported proto guards, `dt-guard proto-no-any`
//! and `dt-guard internal-proto-no-key-material` (ported from bash
//! 2026-10-26): the binary, its clap arms and `run()`. The in-module tests call
//! `check_with` directly; this tier proves each subcommand is wired, that its
//! STATUS line carries the byte-identical rule token, and that a run with no
//! override reads the real repository clean.

#![expect(
    clippy::unwrap_used,
    reason = "integration-test fixture helpers outside #[test] fns; a broken fixture MUST abort loudly rather than degrade to a vacuous pass (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use assert_cmd::Command;
use std::path::Path;
use tempfile::TempDir;

const INTERNAL: &str = "proto/dark_tower/internal/v1/internal.proto";
const SIGNALING: &str = "proto/dark_tower/signaling/v1/signaling.proto";

fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn run_guard(sub: &str, root: &Path) -> (bool, String) {
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args([sub, "--root"])
        .arg(root)
        .output()
        .unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// A copy of the real internal + signaling protos, `internal` transformed.
fn tree(transform: impl Fn(String) -> String) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for rel in [INTERNAL, SIGNALING] {
        let src = std::fs::read_to_string(repo_root().join(rel)).unwrap();
        let body = if rel == INTERNAL { transform(src) } else { src };
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    dir
}

#[test]
fn both_guards_pass_on_the_real_repository() {
    for (sub, ok_token) in [
        ("proto-no-any", "STATUS=OK REASON=proto-no-any-clean"),
        (
            "internal-proto-no-key-material",
            "STATUS=OK REASON=internal-proto-no-key-material-clean",
        ),
    ] {
        let (ok, out) = run_guard(sub, repo_root());
        assert!(ok, "{sub}: {out}");
        assert!(out.contains(ok_token), "{sub}: {out}");
    }
}

#[test]
fn an_any_import_fails_with_its_token() {
    let dir = tree(|s| {
        s.replacen(
            "import \"dark_tower/signaling/v1/signaling.proto\";",
            "import \"dark_tower/signaling/v1/signaling.proto\";\nimport \"google/protobuf/any.proto\";",
            1,
        )
    });
    let (ok, out) = run_guard("proto-no-any", dir.path());
    assert!(!ok, "{out}");
    assert!(out.contains("STATUS=FAIL REASON=proto-any-import"), "{out}");
    assert!(out.contains("VIOLATION [proto-any-import]"), "{out}");
}

#[test]
fn a_bytes_field_fails_with_its_token() {
    let dir = tree(|s| {
        s.replacen(
            "  repeated SourceStream source_streams = 8;",
            "  repeated SourceStream source_streams = 8;\n  bytes k = 90;",
            1,
        )
    });
    let (ok, out) = run_guard("internal-proto-no-key-material", dir.path());
    assert!(!ok, "{out}");
    assert!(
        out.contains("STATUS=FAIL REASON=internal-proto-bytes-field"),
        "{out}"
    );
}

#[test]
fn a_tree_without_protos_is_vacuous_for_both() {
    let dir = tempfile::tempdir().unwrap();
    for (sub, token) in [
        ("proto-no-any", "proto-any-scan-vacuous"),
        (
            "internal-proto-no-key-material",
            "internal-proto-scan-vacuous",
        ),
    ] {
        let (ok, out) = run_guard(sub, dir.path());
        assert!(!ok, "{sub}: {out}");
        assert!(
            out.contains(&format!("STATUS=FAIL REASON={token}")),
            "{sub}: {out}"
        );
    }
}
