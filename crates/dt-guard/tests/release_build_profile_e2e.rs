//! End-to-end tier for `dt-guard release-build-profile`.
//!
//! The in-module unit tests exercise the matchers. This tier exercises the
//! **operator-facing surface** — the exact bytes an operator sees on a red
//! Layer 3 — which the unit tests cannot reach because it is produced by
//! `run()`'s printing path.
//!
//! Two properties are pinned here that nothing else pins:
//!
//! 1. **The REASON token appears inside the `VIOLATION:` / `ERROR:` line**, not
//!    only in the `STATUS=` line. Verified 2026-09-01 against
//!    `scripts/guards/run-guards.sh`: Layer 3 runs the guard runner
//!    non-verbose (`scripts/layer3.sh` passes no flag; `run-guards.sh:45`
//!    defaults `VERBOSE=false`), the non-verbose branch **captures** guard
//!    stdout+stderr at `run-guards.sh:217`, and `classify_guard_exit`'s
//!    failure arm re-emits only lines matching
//!    `VIOLATION|violation|ERROR|error|WARN`, capped at `head -5`. The
//!    runner's own `STATUS=FAIL REASON=guard-violations` is a **fused
//!    constant** carrying no guard name and no token — so the re-emitted
//!    violation text is the ONLY channel by which anyone learns which class
//!    fired. A token that lives only in the guard's `STATUS=` line is
//!    decorative.
//!
//! 2. **Discovery/precondition lines are printed FIRST.** With `head -5`
//!    truncating, a coverage refusal buried at line six is precisely the
//!    failure this guard exists to prevent.
//!
//! Mirrors `binary_status_surface.rs` for the STATUS-surface tier.

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
use std::path::Path;
use tempfile::TempDir;

/// A workspace manifest with a clean `[profile.release]`, so fixtures isolate
/// the channel under test instead of tripping the manifest vacuity class
/// (which outranks every violation class by design).
const CLEAN_MANIFEST: &str = r#"
[workspace]
members = ["crates/common"]

[profile.release]
opt-level = 3
lto = true
"#;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

/// Build a minimal fixture tree: workspace manifest + one service Dockerfile.
fn fixture(manifest: &str, dockerfile: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    write(&tmp.path().join("Cargo.toml"), manifest);
    write(
        &tmp.path().join("infra/docker/mh-service/Dockerfile"),
        dockerfile,
    );
    tmp
}

fn run_guard(root: &Path) -> (bool, String) {
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["release-build-profile", "--root"])
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

/// The re-emission filter `run-guards.sh` applies in non-verbose mode. A line
/// that does not match this is invisible to an operator on a red Layer 3.
fn survives_run_guards_filter(line: &str) -> bool {
    ["VIOLATION", "violation", "ERROR", "error", "WARN"]
        .iter()
        .any(|t| line.contains(t))
}

fn first_surviving_line(output: &str) -> Option<&str> {
    output.lines().find(|l| survives_run_guards_filter(l))
}

// ---------------------------------------------------------------------------
// Negative direction — the guard must FAIL on crafted bad input
// ---------------------------------------------------------------------------

#[test]
fn dockerfile_dropping_release_fails_with_its_own_token() {
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust AS builder\nRUN cargo build --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "guard must fail when a Dockerfile drops --release");
    assert!(
        out.contains("STATUS=FAIL REASON=dockerfile-build-not-release-1"),
        "STATUS token must name the class; got:\n{out}"
    );
    // The property that actually matters operationally.
    assert!(
        out.lines()
            .any(|l| l.starts_with("VIOLATION:") && l.contains("dockerfile-build-not-release")),
        "the REASON token must ride inside the VIOLATION line — it is the only \
         channel that survives run-guards.sh's non-verbose capture; got:\n{out}"
    );
}

#[test]
fn dockerfile_with_release_passes() {
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust AS builder\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(ok, "a --release build must pass; got:\n{out}");
    assert!(out.contains("STATUS=OK"));
}

#[test]
fn dockerfile_line_continuation_does_not_false_fail() {
    // A guard that false-fails gets bypassed, so the continuation join is a
    // correctness requirement rather than a nicety.
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust AS builder\nRUN cargo build \\\n    --release \\\n    --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(
        ok,
        "`--release` after a continuation must pass; got:\n{out}"
    );
}

#[test]
fn profile_release_debug_assertions_fails() {
    let manifest = "[workspace]\nmembers = [\"crates/common\"]\n\
                    [profile.release]\nopt-level = 3\ndebug-assertions = true\n";
    let tmp = fixture(
        manifest,
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok);
    assert!(
        out.contains("REASON=release-profile-debug-assertions-enabled-1"),
        "got:\n{out}"
    );
    assert!(out
        .lines()
        .any(|l| l.starts_with("VIOLATION:") && l.contains("release-profile-debug-assertions")));
}

#[test]
fn profile_release_package_override_fails() {
    // @security row 8, and the most precisely-aimed form of this attack: it
    // re-arms the compile-time control in exactly the crate ADR-0036 §11's
    // control protects, while `[profile.release]` stays byte-identical.
    let manifest = "[workspace]\nmembers = [\"crates/common\"]\n\
                    [profile.release]\nopt-level = 3\n\
                    [profile.release.package.mh-service]\ndebug-assertions = true\n";
    let tmp = fixture(
        manifest,
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "per-package override must fail; got:\n{out}");
    assert!(
        out.contains("profile.release.package.mh-service"),
        "got:\n{out}"
    );
}

#[test]
fn cargo_config_profile_table_fails() {
    // Row 7: defeats the premise with Cargo.toml AND every Dockerfile
    // byte-identical.
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    write(
        &tmp.path().join(".cargo/config.toml"),
        "[profile.release]\ndebug-assertions = true\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "config-profile spelling must fail; got:\n{out}");
    assert!(
        out.contains("REASON=cargo-config-release-profile-debug-assertions-1"),
        "got:\n{out}"
    );
}

#[test]
fn legacy_cargo_config_without_extension_fails() {
    // Row 9: cargo still honours the unsuffixed filename, so globbing only
    // `config.toml` would leave a silent bypass.
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    write(
        &tmp.path().join(".cargo/config"),
        "[build]\nrustflags = [\"-C\", \"debug-assertions=on\"]\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "legacy .cargo/config must be scanned; got:\n{out}");
    assert!(
        out.contains("REASON=rustflags-debug-assertions-1"),
        "got:\n{out}"
    );
}

#[test]
fn cargo_profile_env_in_dockerfile_fails() {
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust\nARG CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true\n\
         RUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok);
    assert!(
        out.contains("REASON=cargo-profile-env-debug-assertions-1"),
        "got:\n{out}"
    );
}

#[test]
fn cook_build_profile_mismatch_says_the_binary_is_unaffected() {
    // Distinct class from a premise break, and the message must say so — the
    // person triaging a red Layer 3 at 3am does not know cargo-chef's caching
    // model, and a token that reads alarming for a correct binary is a lie.
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust\nRUN cargo chef cook --recipe-path recipe.json\n\
         RUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok);
    assert!(
        out.contains("REASON=dockerfile-cook-profile-mismatch-1"),
        "cook drift gets its OWN token, distinct from a premise break; got:\n{out}"
    );
    assert!(
        out.contains("THE SHIPPED BINARY IS UNAFFECTED"),
        "the message must state affirmatively that this is cache-layer drift; got:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// Vacuity — "found nothing" must never read as "checked and clean"
// ---------------------------------------------------------------------------

#[test]
fn zero_dockerfiles_discovered_fails_and_leads_the_output() {
    let tmp = TempDir::new().unwrap();
    write(&tmp.path().join("Cargo.toml"), CLEAN_MANIFEST);
    // No infra/docker at all.
    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "a walk that matched nothing must FAIL, never report clean; got:\n{out}"
    );
    assert!(
        out.contains("REASON=release-build-profile-no-dockerfiles-discovered-1"),
        "got:\n{out}"
    );
    let first = first_surviving_line(&out).expect("a re-emittable line must exist");
    assert!(
        first.contains("release-build-profile-no-dockerfiles-discovered"),
        "the discovery failure must be the FIRST line surviving run-guards.sh's \
         head -5 cap, or it can be truncated away; got first: {first}"
    );
    assert!(
        first.contains("NOT a diff defect"),
        "the line must name its lane — classify_guard_exit cannot route it; got: {first}"
    );
}

#[test]
fn missing_workspace_manifest_fails() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp.path().join("infra/docker/mh-service/Dockerfile"),
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "an absent Cargo.toml must FAIL, not skip; got:\n{out}");
    assert!(
        out.contains("REASON=release-build-profile-workspace-manifest-unreadable-1"),
        "got:\n{out}"
    );
}

#[test]
fn unparseable_workspace_manifest_fails() {
    // Totality is the entire justification for taking a real TOML parser: an
    // unparseable manifest must FAIL, never silently skip.
    let tmp = fixture(
        "[workspace\nmembers = [ this is not toml\n",
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(!ok, "unparseable manifest must FAIL, not skip; got:\n{out}");
    assert!(
        out.contains("REASON=release-build-profile-workspace-manifest-unreadable-1"),
        "got:\n{out}"
    );
}

#[test]
fn manifest_without_release_profile_passes_via_cargo_defaults() {
    // FLIPPED at Gate 3 (@observability + @test, settled by measurement rather
    // than argument). Cargo's built-in release profile already sets
    // `debug-assertions = false`, so a manifest with NO `[profile.release]`
    // table builds `--release` with the premise HOLDING. Verified twice
    // independently, once with a throwaway crate carrying ADR-0036 §11's exact
    // control: it compiles clean under `--release` with no profile table, and
    // trips under the dev profile.
    //
    // The guard used to fail closed here. That was a false positive on a
    // premise-holding config, and this test cemented it as intended behaviour —
    // the same failure mode `dockerfile_line_continuation_does_not_false_fail`
    // exists to defend against, pointed the other way.
    let tmp = fixture(
        "[workspace]\nmembers = [\"crates/common\"]\n",
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(
        ok,
        "cargo defaults debug-assertions off for release, so the premise holds; got:\n{out}"
    );
    assert!(out.contains("STATUS=OK"), "got:\n{out}");
}

#[test]
fn cargo_config_parse_failure_gets_its_own_token() {
    // @observability Finding 1: this used to share
    // `workspace-manifest-unreadable`, which sends a triager to Cargo.toml — a
    // file that is fine. It is also CAUSABLE by an ordinary edit, unlike the
    // manifest classes, so it must not inherit their "you cannot cause these by
    // editing a file" triage clause in devloop-validation.md §8.
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    write(
        &tmp.path().join(".cargo/config.toml"),
        "[build\nrustflags = not toml\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "an unparseable cargo config must FAIL, not skip; got:\n{out}"
    );
    assert!(
        out.contains("REASON=release-build-profile-cargo-config-unparseable-1"),
        "got:\n{out}"
    );
    assert!(
        out.contains(".cargo/config.toml"),
        "the message must name the file that is actually broken; got:\n{out}"
    );
}

#[test]
fn glob_workspace_members_fail_rather_than_voiding_the_floor() {
    // End-to-end twin of the module-level regression. Reproduced on identical
    // trees differing ONLY in how members are spelled: enumerated members
    // FAILed with two precondition hits, glob members reported STATUS=OK.
    let tmp = fixture(
        "[workspace]\nmembers = [\"crates/*\"]\n[profile.release]\nopt-level = 3\n",
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "glob members must not silently void the service-coverage floor; got:\n{out}"
    );
    assert!(
        out.contains("REASON=release-build-profile-service-roster-underivable-1"),
        "got:\n{out}"
    );
}

#[test]
fn service_crate_without_dockerfile_fails() {
    let manifest = "[workspace]\nmembers = [\"crates/mh-service\", \"crates/xx-service\"]\n\
                    [profile.release]\nopt-level = 3\n";
    let tmp = fixture(
        manifest,
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    let (ok, out) = run_guard(tmp.path());
    assert!(
        !ok,
        "a service crate outside the gate must FAIL — otherwise the floor is \
         vacuous for exactly the newly-added service; got:\n{out}"
    );
    assert!(out.contains("xx-service"), "got:\n{out}");
}

// ---------------------------------------------------------------------------
// The premise assertion itself, against the real tree
// ---------------------------------------------------------------------------

#[test]
fn real_tree_is_clean_and_discovery_is_non_vacuous() {
    // Guards the always-fails direction (the negative tests above all pass on
    // an accidentally-always-red guard), AND asserts discovery actually found
    // something — a green produced by scanning nothing is the vacuous pass.
    let repo_root = {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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
        "the release premise must hold on the real tree; got:\n{out}"
    );
    assert!(
        out.contains("STATUS=OK REASON=release-build-profile-enumerated-inputs-clean"),
        "the OK token must name the ENUMERATION, not the conclusion — a reader \
         must not be able to derive \"the release premise holds\" from a green; got:\n{out}"
    );
    let scope = out
        .lines()
        .find(|l| l.starts_with("SCOPE:"))
        .expect("a SCOPE counts line must be emitted");
    assert!(
        !scope.contains("SCOPE: 0 "),
        "discovery scanned zero Dockerfiles — the green is vacuous; got: {scope}"
    );
    // WARN-free-run contract: a clean real-tree run must skip nothing.
    assert!(
        !out.contains("WARN dt-guard auxiliary skip"),
        "a clean run must skip no file; a skip means a channel checked less \
         than the SCOPE line claims; got:\n{out}"
    );
}

#[test]
fn unreadable_workflow_warns_but_does_not_fail() {
    // The deliberate asymmetry with `no_insecure_browser_flags`, pinned so it
    // reads as a decision rather than an oversight. This module's PRIMARY
    // premise surface is strict (Dockerfiles/manifest/cargo-config reads
    // propagate or record a hit); the CI-workflow sweep is the secondary
    // belt-and-suspenders half of the value-keyed RUSTFLAGS rule, so losing one
    // workflow degrades a backstop, not the premise. It must WARN — never
    // vanish silently, which is what F-DRY-F was — and must not fail.
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust\nRUN cargo build --release --package mh-service\n",
    );
    std::fs::create_dir_all(tmp.path().join(".github/workflows")).unwrap();
    std::fs::write(
        tmp.path().join(".github/workflows/bad.yml"),
        [0xff, 0xfe, 0x00],
    )
    .unwrap();

    let (ok, out) = run_guard(tmp.path());
    assert!(
        ok,
        "a secondary-surface read failure must not fail the guard; got:\n{out}"
    );
    assert!(
        out.contains("WARN dt-guard auxiliary skip (release-build-profile workflow read)"),
        "it must still be VISIBLE — silence is the defect F-DRY-F named; got:\n{out}"
    );
}

#[test]
fn explain_mode_emits_policy_qualified_findings() {
    // `--explain` is a mandatory per-subcommand surface (ADR-0034 §7) and is
    // otherwise untested.
    let tmp = fixture(
        CLEAN_MANIFEST,
        "FROM rust\nRUN cargo build --package mh-service\n",
    );
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["release-build-profile", "--root"])
        .arg(tmp.path())
        .arg("--explain")
        .output()
        .unwrap();
    let combined = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        combined.contains("policy=release-build-profile::dockerfile_build_not_release"),
        "EXPLAIN lines must be policy-qualified per rule_id; got:\n{combined}"
    );
    assert!(
        combined.contains("reason=dockerfile-build-not-release"),
        "EXPLAIN lines carry the REASON token as an extra; got:\n{combined}"
    );
}
