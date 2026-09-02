//! End-to-end tier for `dt-guard no-insecure-browser-flags`.
//!
//! **This file deliberately contains no prohibited literal.** Every fixture is
//! built from the vocabulary constants exported by the policy module, so the
//! test suite does not itself become a file the guard must exempt — each
//! exemption is a hole, and the allowlist's whole value is that it stays at
//! two reviewable entries. The sanctioned fake-media flags ARE spelled out,
//! because they are sanctioned and pinning them literally is the point.
//!
//! Same operator-surface reasoning as `release_build_profile_e2e.rs`: Layer 3
//! runs the guard runner non-verbose, guard stdout is captured at
//! `run-guards.sh:217`, and only `VIOLATION|violation|ERROR|error|WARN` text is
//! re-emitted (capped at `head -5`). `run-guards.sh:253`'s own
//! `REASON=guard-violations` is a fused constant carrying no guard name and no
//! token — so the re-emitted violation text is the only channel telling an
//! operator which class fired.

// Crate-level test override, per the note in `clippy.toml` and the precedent at
// `tests/ts_retained_credentials_fixtures.rs`: `allow-unwrap-in-tests` /
// `allow-expect-in-tests` only apply inside `#[cfg(test)]` modules and `#[test]`
// fns, so this file's non-`#[test]` HELPER functions (`write`, `fixture`,
// `run_guard`, …) still trip `unwrap_used`. (`expect_used` is deliberately NOT
// listed: `#![expect]` reported it unfulfilled, i.e. every `.expect()` here is
// already inside a `#[test]` fn and covered by `allow-expect-in-tests`. Listing
// it anyway would be an escape hatch wider than the actual need.)
//
// This is the ADR-0002 TEST carve-out, and it is deliberately NOT the escape used
// for the three `indexing_slicing` denials in the library at Gate 2 — those were
// restructured to total accessors, because production guard code that panics on
// malformed input reads as a crashed pipeline rather than a finding. Here the
// opposite holds: a fixture helper that swallowed an IO error and continued would
// yield a harness that passes while proving nothing — the exact vacuous-pass
// failure this suite exists to lock down. Aborting loudly is correct.
#![expect(
    clippy::unwrap_used,
    reason = "integration-test fixture helpers outside #[test] fns; a broken fixture MUST abort loudly rather than degrade to a vacuous pass (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use assert_cmd::Command;
use dt_guard::no_insecure_browser_flags::{
    CERT_VALIDATION_FLAGS, INSECURE_ORIGIN_FLAGS, INSECURE_PROPERTIES,
};
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Sanctioned, spelled literally on purpose: these inject synthesized capture
/// and auto-grant the microphone permission. They disable nothing, and story
/// task 20 requires the runbook to name them verbatim.
const SANCTIONED_FAKE_MEDIA: &[&str] = &[
    "--use-fake-device-for-media-stream",
    "--use-fake-ui-for-media-stream",
];

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// The guard scans tracked files only, so a fixture must be a git repo with
/// its files staged.
fn git_fixture(files: &[(&str, String)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let run = |args: &[&str]| {
        StdCommand::new("git")
            .current_dir(tmp.path())
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    for (rel, content) in files {
        write(&tmp.path().join(rel), content);
    }
    run(&["add", "-A"]);
    tmp
}

fn run_guard(root: &Path) -> (bool, String) {
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["no-insecure-browser-flags", "--root"])
        .arg(root)
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), combined)
}

fn survives_run_guards_filter(line: &str) -> bool {
    ["VIOLATION", "violation", "ERROR", "error", "WARN"]
        .iter()
        .any(|t| line.contains(t))
}

// ---------------------------------------------------------------------------
// One positive case per family
// ---------------------------------------------------------------------------

#[test]
fn cert_validation_flag_in_a_script_fails() {
    let flag = CERT_VALIDATION_FLAGS[0];
    let tmp = git_fixture(&[(
        "scripts/launch-demo.sh",
        format!("#!/usr/bin/env bash\nchrome {flag} https://demo.localhost\n"),
    )]);
    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "a cert-validation flag in a script must fail; got:\n{out}"
    );
    assert!(
        out.contains("REASON=insecure-browser-cert-validation-flag-1"),
        "got:\n{out}"
    );
    assert!(
        out.lines()
            .any(|l| l.starts_with("VIOLATION:")
                && l.contains("insecure-browser-cert-validation-flag")),
        "the token must ride inside the VIOLATION line — it is the only channel \
         that survives run-guards.sh's non-verbose capture; got:\n{out}"
    );
}

#[test]
fn insecure_origin_flag_in_a_runbook_fails() {
    // Docs and runbooks are IN scope, deliberately and unlike the RUSTFLAGS
    // rule: a runbook telling a developer to paste this flag IS the leak.
    let flag = INSECURE_ORIGIN_FLAGS[0];
    let tmp = git_fixture(&[(
        "docs/runbooks/example.md",
        format!("If the origin is not trusted, run:\n\n```bash\nchrome {flag}=http://192.168.1.5:5173\n```\n"),
    )]);
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "a runbook naming the flag must fail; got:\n{out}");
    assert!(
        out.contains("REASON=insecure-browser-origin-or-websecurity-flag-1"),
        "got:\n{out}"
    );
}

#[test]
fn playwright_config_property_fails_with_zero_flags_present() {
    // The cheap bypass @security flagged: identical effect, no flag involved,
    // and a flag-only matcher is silent on it.
    let (name, unsafe_value) = INSECURE_PROPERTIES[0];
    let tmp = git_fixture(&[(
        "packages/web-app/playwright.config.ts",
        format!("export default {{\n  use: {{\n    {name}: {unsafe_value},\n  }},\n}};\n"),
    )]);
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "a config property must fail; got:\n{out}");
    assert!(
        out.contains("REASON=insecure-browser-config-property-1"),
        "distinct token per class — an args array and a config property are \
         different remedies in different places; got:\n{out}"
    );
}

#[test]
fn webdriver_capability_fails() {
    let (name, unsafe_value) = INSECURE_PROPERTIES[1];
    let tmp = git_fixture(&[(
        "crates/env-tests/tests/example.rs",
        format!("let caps = json!({{ \"{name}\": {unsafe_value} }});\n"),
    )]);
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "a WebDriver capability must fail; got:\n{out}");
}

#[test]
fn node_tls_opt_out_fails() {
    let (name, unsafe_value) = INSECURE_PROPERTIES[2];
    let tmp = git_fixture(&[(
        "scripts/run-e2e.sh",
        format!("#!/usr/bin/env bash\nexport {name}={unsafe_value}\npnpm test\n"),
    )]);
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "a Node TLS opt-out must fail; got:\n{out}");
}

// ---------------------------------------------------------------------------
// The sanctioned/prohibited boundary — non-membership, NOT exemption
// ---------------------------------------------------------------------------

#[test]
fn sanctioned_fake_media_flags_pass_without_any_allowlist_entry() {
    // @semantic-guard's pin. These must pass because they are NOT IN THE
    // VOCABULARY — never because a path was exempted.
    //
    // The regression this prevents: if the vocabulary is ever widened to a
    // `--use-fake-*` shape, the guard starts firing on the real
    // playwright.config.ts AND on story task 20's required runbook text; the
    // tempting fix is a broad allowlist entry for those files; and that entry
    // then masks a real cert-bypass setting added to the same file later,
    // with every other test still green.
    let args = SANCTIONED_FAKE_MEDIA
        .iter()
        .map(|f| format!("'{f}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let tmp = git_fixture(&[
        (
            "packages/web-app/playwright.config.ts",
            format!("  launchOptions: {{ args: [{args}] }},\n"),
        ),
        (
            "docs/runbooks/client-dev-local.md",
            format!("On a machine with no microphone, launch Chrome with {args}.\n"),
        ),
    ]);
    let (ok, out) = run_guard(tmp.path());
    assert!(
        ok,
        "sanctioned fake-media flags must pass on vocabulary NON-MEMBERSHIP; got:\n{out}"
    );
    assert!(
        out.contains("0 allowlisted mentions"),
        "they must pass without ANY allowlist entry — passing via exemption would \
         mean the vocabulary had become shape-based; got:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// Allowlist: it works, and it is narrow
// ---------------------------------------------------------------------------

#[test]
fn allowlisted_path_is_exempt_but_its_sibling_is_not() {
    let flag = CERT_VALIDATION_FLAGS[0];
    let body = format!("This harness deliberately does NOT use `{flag}`.\n");

    // Exemption WORKS on the exact literal path...
    let tmp = git_fixture(&[(
        "docs/devloop-outputs/2026-08-03-playwright-e2e-harness/main.md",
        body.clone(),
    )]);
    let (ok, out) = run_guard(tmp.path());
    assert!(
        ok,
        "the allowlisted negative mention must pass; got:\n{out}"
    );
    assert!(out.contains("1 allowlisted mentions"), "got:\n{out}");

    // ...and is NARROW: a sibling in the SAME directory still fails. This is
    // the widening vector — a prefix allowlist would swallow it silently.
    let tmp = git_fixture(&[(
        "docs/devloop-outputs/2026-08-03-playwright-e2e-harness/sibling.md",
        body,
    )]);
    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "a sibling path must NOT inherit the exemption; got:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// Vacuity — this guard's green today literally IS "found nothing"
// ---------------------------------------------------------------------------

#[test]
fn zero_candidate_files_fails_and_leads_the_output() {
    let tmp = git_fixture(&[("infra/docker/certs/ca.pem", "not scannable\n".to_string())]);
    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "a walk matching zero scannable files must FAIL — this guard's passing \
         state and its broken state otherwise look identical; got:\n{out}"
    );
    assert!(
        out.contains("REASON=no-insecure-browser-flags-no-candidate-files-1"),
        "got:\n{out}"
    );
    let first = out
        .lines()
        .find(|l| survives_run_guards_filter(l))
        .expect("a re-emittable line must exist");
    assert!(
        first.contains("no-candidate-files") && first.contains("NOT a diff defect"),
        "the discovery refusal must be the FIRST surviving line and must name its \
         lane; got: {first}"
    );
}

// ---------------------------------------------------------------------------
// Unreadable candidate — coverage loss must not read as clean (F-DRY-F)
// ---------------------------------------------------------------------------

#[test]
fn unreadable_candidate_file_fails_and_warns_rather_than_dropping_silently() {
    // @dry-reviewer F-DRY-F. A candidate that cannot be read leaves the scanned
    // set; before this fix it did so via a bare `continue` while the `SCOPE:`
    // line still COUNTED it — so the count and the coverage claim disagreed
    // with nothing saying so. Same "could-not-evaluate reads as a pass" shape
    // as the zero-files vacuity bail, one branch lower.
    //
    // Invalid UTF-8 in a `.ts` file is the realistic form (a corrupt checkout,
    // a mis-encoded fixture); it reproduces without needing chmod, which would
    // be a no-op for a root-run container.
    let tmp = git_fixture(&[(
        "packages/web-app/src/ok.ts",
        "export const a = 1;\n".to_string(),
    )]);
    std::fs::write(
        tmp.path().join("packages/web-app/src/bad.ts"),
        [0xff, 0xfe, 0x00],
    )
    .unwrap();
    StdCommand::new("git")
        .current_dir(tmp.path())
        .args(["add", "-A"])
        .output()
        .unwrap();

    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "an unreadable candidate silently shrinks the scanned set that this \
         guard's green is a claim about; got:\n{out}"
    );
    assert!(
        out.contains("REASON=no-insecure-browser-flags-unreadable-candidate-file-1"),
        "got:\n{out}"
    );
    // Shared-home WARN reaches the operator with the underlying IO error.
    assert!(
        out.contains("WARN dt-guard auxiliary skip (no-insecure-browser-flags candidate read)"),
        "warn_skip must name the scan and the file; got:\n{out}"
    );
    // And it leads, so run-guards.sh's head -5 cap cannot truncate it away.
    let first = out
        .lines()
        .find(|l| survives_run_guards_filter(l) && l.starts_with("ERROR:"))
        .expect("a precondition line must be emitted");
    assert!(first.contains("NOT a diff defect"), "got: {first}");
}

// ---------------------------------------------------------------------------
// Real tree
// ---------------------------------------------------------------------------

#[test]
fn real_tree_is_clean_today() {
    // The differential that goes red the day someone widens the vocabulary
    // and reaches for a broad allowlist entry: the real tree carries both
    // sanctioned fake-media flags in playwright.config.ts.
    let repo_root = {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            if dir.join(".git").exists() {
                break dir;
            }
            assert!(
                dir.pop(),
                "could not locate repo root — this test would otherwise pass \
                 vacuously by checking nothing"
            );
        }
    };
    let (ok, out) = run_guard(&repo_root);
    assert!(
        ok,
        "the prohibition must hold on the real tree; got:\n{out}"
    );
    assert!(
        out.contains("STATUS=OK REASON=no-insecure-browser-flags-enumerated-settings-clean"),
        "the OK token must name the ENUMERATION, not the conclusion; got:\n{out}"
    );
    let scope = out
        .lines()
        .find(|l| l.starts_with("SCOPE:"))
        .expect("a SCOPE counts line must be emitted");
    assert!(
        !scope.contains("SCOPE: 0 "),
        "discovery scanned zero files — the green is vacuous; got: {scope}"
    );
    // WARN-FREE-RUN CONTRACT (the shape `ts_retained_credentials` documents at
    // its module doc): a clean real-tree run must emit NO `warn_skip`. A WARN
    // here would mean some candidate silently left the scanned set, so the
    // `SCOPE:` count and the coverage claim would disagree — which is exactly
    // what F-DRY-F was. Asserting its ABSENCE is what keeps the count honest.
    assert!(
        !out.contains("WARN dt-guard auxiliary skip"),
        "a clean run must skip nothing; a skip means SCOPE over-counts the \
         scanned set; got:\n{out}"
    );
}
