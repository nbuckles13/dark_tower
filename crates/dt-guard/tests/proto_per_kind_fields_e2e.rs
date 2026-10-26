//! End-to-end tier for `dt-guard proto-per-kind-fields`: the binary, its clap
//! arm and `run()`, over fixture trees (test finding 7). The in-module tests
//! call `check_with` directly; this tier proves the subcommand is wired and
//! that its STATUS line and violation text reach an operator.

#![expect(
    clippy::unwrap_used,
    reason = "integration-test fixture helpers outside #[test] fns; a broken fixture MUST abort loudly rather than degrade to a vacuous pass (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use assert_cmd::Command;
use std::path::Path;
use tempfile::TempDir;

const KINDS: &str = "syntax = \"proto3\";\npackage t.v1;\n\
    enum MediaKind {\n  MEDIA_KIND_UNSPECIFIED = 0;\n  MEDIA_KIND_AUDIO = 1;\n\
    \x20 MEDIA_KIND_VIDEO_CAMERA = 2;\n}\n\
    message Holder {\n  reserved 9;\n  int32 ok = 1;\n}\n";

fn tree(body: Option<&str>) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    if let Some(body) = body {
        let path = dir.path().join("proto/t/v1/t.proto");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("{KINDS}{body}")).unwrap();
    }
    dir
}

fn run_guard(root: &Path) -> (bool, String) {
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["proto-per-kind-fields", "--root"])
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

#[test]
fn a_kind_named_field_fails_with_its_token_and_names_the_field() {
    let dir = tree(Some("message M {\n  bool audio_muted = 1;\n}\n"));
    let (ok, out) = run_guard(dir.path());
    assert!(!ok, "{out}");
    assert!(
        out.contains("STATUS=FAIL REASON=proto-per-kind-field"),
        "{out}"
    );
    assert!(
        out.contains("VIOLATION [proto-per-kind-field]") && out.contains("t.v1.M.audio_muted"),
        "{out}"
    );
}

#[test]
fn a_clean_tree_passes() {
    let dir = tree(Some("message M {\n  bool muted = 1;\n}\n"));
    let (ok, out) = run_guard(dir.path());
    assert!(ok, "{out}");
    assert!(
        out.contains("STATUS=OK REASON=proto-per-kind-fields-clean"),
        "{out}"
    );
}

#[test]
fn a_tree_without_protos_is_vacuous_not_clean() {
    let dir = tree(None);
    let (ok, out) = run_guard(dir.path());
    assert!(!ok, "{out}");
    assert!(
        out.contains("STATUS=FAIL REASON=proto-per-kind-scan-vacuous"),
        "{out}"
    );
}
