//! End-to-end tier for the MH slot-model deploy guards (story 4 task I-3):
//! `dt-guard force-delete-ban` over a temp git repo (every argument ordering
//! is a binary-level negative; a graceful delete and prose are the positive
//! arm), and the STATUS surface of `mh-slot-model` when its inputs are absent
//! (fails closed, never a vacuous OK). The rule-level fixtures for the
//! template invariants, shareProcessNamespace, the root apply set and every
//! drift pair are the unit tests in `kustomize_mh_template.rs` and
//! `mh_slot_model.rs` (pure checks over parsed inputs); the shipped tree is
//! the Layer-3 wrappers' positive arm.
//!
//! Runs on the shared harness (`tests/common`): hermetic `git` (no global
//! excludesFile changes what `ls-files --others --exclude-standard` returns).
//! The force-delete commands are ASSEMBLED from pieces, so this file (outside
//! the guard's scope anyway) never spells one.

mod common;

use common::{assert_bail_token, assert_clean, git, new_root, run_guard_plain, write};
use tempfile::TempDir;

const SUB: &str = "force-delete-ban";
const FORCE: &str = "--force";
const ZERO: &str = "--grace-period=0";

fn repo(files: &[(&str, &str)]) -> TempDir {
    let root = new_root();
    git(root.path(), &["init", "-q"]);
    for (rel, content) in files {
        write(root.path(), rel, content);
    }
    root
}

#[test]
fn every_force_delete_ordering_fails_the_binary() {
    let orderings = [
        format!("kubectl delete pod mh-s0-x {FORCE} {ZERO}"),
        format!("kubectl delete pod mh-s0-x {ZERO} {FORCE}"),
        format!("kubectl {FORCE} delete {ZERO} pod mh-s0-x"),
        format!("kubectl delete {FORCE} pods/mh-s0-x"),
    ];
    for cmd in &orderings {
        // Untracked files count: a new script is checked before its first commit.
        let r = repo(&[(
            "infra/kind/scripts/x.sh",
            &format!("#!/usr/bin/env bash\n{cmd}\n"),
        )]);
        let out = run_guard_plain(r.path(), SUB);
        assert!(
            out.contains("infra/kind/scripts/x.sh:2 [pod_force_delete]"),
            "{cmd}: {out}"
        );
        assert!(
            out.contains("STATUS=FAIL REASON=pod-force-delete-1"),
            "{cmd}: {out}"
        );
    }
}

#[test]
fn every_scoped_root_is_scanned() {
    let cmd = format!("kubectl delete pod x {FORCE}");
    for rel in [
        "scripts/a.sh",
        "crates/env-tests/src/a.rs",
        "crates/devloop-helper/src/a.rs",
        "packages/web-app/e2e/a.ts",
    ] {
        let content = if rel.ends_with(".rs") {
            format!(
                "fn f() {{ let _ = Command::new(\"sh\").arg(\"-c\").arg(\"{cmd}\").status(); }}\n"
            )
        } else if rel.ends_with(".ts") {
            format!("execSync('{cmd}');\n")
        } else {
            format!("{cmd}\n")
        };
        let r = repo(&[(rel, &content)]);
        assert_bail_token(r.path(), SUB, "pod-force-delete-1");
    }
    // Out of scope: docs, and app code outside packages/*/e2e.
    let r = repo(&[
        ("docs/runbooks/a.md", &format!("```\n{cmd}\n```\n")),
        (
            "packages/web-app/src/a.ts",
            &format!("execSync('{cmd}');\n"),
        ),
        ("scripts/ok.sh", "kubectl delete pod x\n"),
    ]);
    assert_clean(r.path(), SUB, "force-delete-ban-clean-1-files");
}

#[test]
fn graceful_delete_and_prose_pass() {
    let r = repo(&[(
        "infra/kind/scripts/x.sh",
        &format!("#!/usr/bin/env bash\n# Never `kubectl delete pod {FORCE} {ZERO}`.\nkubectl delete pod x {ZERO}\n"),
    )]);
    assert_clean(r.path(), SUB, "force-delete-ban-clean-1-files");
}

#[test]
fn an_empty_scope_fails_closed() {
    let r = repo(&[("README.md", "x\n")]);
    assert_bail_token(
        r.path(),
        SUB,
        "STATUS=FAIL REASON=force-delete-ban-no-candidate-files-1",
    );
}

#[test]
fn mh_slot_model_without_its_inputs_fails_closed() {
    let r = repo(&[]);
    assert_bail_token(
        r.path(),
        "mh-slot-model",
        "STATUS=FAIL REASON=mh-slot-model-unverifiable",
    );
}
