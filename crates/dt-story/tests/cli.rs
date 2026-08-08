// Per clippy.toml: allow-expect-in-tests covers #[test] fns, but
// integration-test helper functions need crate-level overrides
// (rust-clippy#13981). Test-only code — panics are the failure mode.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

//! Binary-surface tests for the dt-story exit-code contract.
//!
//! Drives `dt-story` via `assert_cmd` against fixture stories under
//! `tests/fixtures/`, covering: `next`'s four exit paths (0 runnable /
//! 3 all-complete / 4 blocked / 2 malformed), `complete` + `escalate`
//! round-trips that preserve all non-manifest bytes exactly, and
//! `validate` catching each violation class.

use assert_cmd::Command;
use dt_story::manifest::{Manifest, Status, MARKER};
use dt_story::markdown;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn dt_story() -> Command {
    Command::cargo_bin("dt-story").expect("dt-story binary")
}

/// Wrap a manifest YAML body in a realistic story markdown skeleton.
fn story_md(yaml_body: &str) -> String {
    format!("# Story\n\nintro prose\n\n```yaml\n{MARKER}\n{yaml_body}```\n\noutro prose\n")
}

/// Parse the manifest embedded in a story file's text.
fn parse_manifest(text: &str) -> Manifest {
    let span = markdown::find_manifest_block(text).expect("manifest block");
    let yaml = markdown::manifest_yaml(text, &span).expect("manifest yaml");
    Manifest::from_yaml(yaml).expect("manifest parse")
}

// ---------------------------------------------------------------- next --

#[test]
fn next_runnable_prints_json_and_exits_zero() {
    let assert = dt_story().arg("next").arg(fixture("runnable.md")).assert();
    let output = assert.success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    assert_eq!(json["id"], 2);
    assert_eq!(json["specialist"], "global-controller");
    assert_eq!(json["env_tests"], true);
    let prompt = json["prompt"].as_str().expect("prompt string");
    assert_eq!(
        prompt, "Implement the join endpoint.\nReturn a meeting token on success.\n",
        "multiline prompt must round-trip with real newlines"
    );
}

#[test]
fn next_all_complete_exits_three_with_empty_stdout() {
    let assert = dt_story()
        .arg("next")
        .arg(fixture("all_complete.md"))
        .assert();
    let output = assert.code(3).get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "exit 3 must print nothing on stdout"
    );
}

#[test]
fn next_blocked_exits_four_with_stderr_diagnosis() {
    let assert = dt_story().arg("next").arg(fixture("blocked.md")).assert();
    let output = assert.code(4).get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "exit 4 must print nothing on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("task 1")
            && stderr.contains(
                "dep 2 is escalated and blocks dependents until it is selected and retried"
            ),
        "diagnosis must name the blocked task and explain the escalated dep, got: {stderr}"
    );
}

#[test]
fn next_escalated_task_with_unmet_deps_still_exits_four() {
    // blocked.md's task 2 is escalated with an unmet dep — being escalated
    // must not make it runnable, and the blocked path must not rewrite the
    // story file.
    let orig = fs::read_to_string(fixture("blocked.md")).expect("read fixture");
    dt_story()
        .arg("next")
        .arg(fixture("blocked.md"))
        .assert()
        .code(4);
    let after = fs::read_to_string(fixture("blocked.md")).expect("re-read fixture");
    assert_eq!(
        after, orig,
        "blocked next must leave the story file untouched"
    );
}

#[test]
fn next_malformed_exits_two_with_stderr_diagnosis() {
    let assert = dt_story().arg("next").arg(fixture("malformed.md")).assert();
    let output = assert.code(2).get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "exit 2 must print nothing on stdout"
    );
    assert!(!output.stderr.is_empty(), "exit 2 must diagnose on stderr");
}

#[test]
fn next_missing_file_exits_two() {
    dt_story()
        .arg("next")
        .arg(fixture("does-not-exist.md"))
        .assert()
        .code(2);
}

// ---------------------------------------------- complete / escalate -----

/// Assert every byte outside the manifest block content is unchanged.
fn assert_non_manifest_bytes_preserved(orig: &str, mutated: &str) {
    let orig_span = markdown::find_manifest_block(orig).expect("orig block");
    let new_span = markdown::find_manifest_block(mutated).expect("mutated block");
    assert_eq!(
        &orig[..orig_span.content_start],
        &mutated[..new_span.content_start],
        "bytes before the manifest content must be preserved exactly"
    );
    assert_eq!(
        &orig[orig_span.content_end..],
        &mutated[new_span.content_end..],
        "bytes after the manifest content must be preserved exactly"
    );
}

#[test]
fn complete_round_trip_preserves_non_manifest_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    let orig = fs::read_to_string(fixture("runnable.md")).expect("read fixture");
    fs::write(&story, &orig).expect("write story copy");

    dt_story()
        .arg("complete")
        .arg(&story)
        .arg("2")
        .arg("--commit")
        .arg("abc1234")
        .assert()
        .success();

    let mutated = fs::read_to_string(&story).expect("read mutated story");
    assert_non_manifest_bytes_preserved(&orig, &mutated);

    let new_span = markdown::find_manifest_block(&mutated).expect("mutated block");
    let new_yaml = markdown::manifest_yaml(&mutated, &new_span).expect("mutated yaml");
    assert!(
        new_yaml.starts_with(MARKER),
        "rewritten block must keep the marker comment as its first line"
    );
    assert!(
        new_yaml.contains("prompt: |"),
        "multiline prompts must re-serialize as YAML block scalars, got:\n{new_yaml}"
    );

    let orig_manifest = parse_manifest(&orig);
    let manifest = parse_manifest(&mutated);
    let task2 = manifest.task(2).expect("task 2");
    assert_eq!(task2.status, Status::Completed);
    assert_eq!(task2.commit.as_deref(), Some("abc1234"));
    assert_eq!(
        manifest.task(3).expect("task 3").prompt,
        orig_manifest.task(3).expect("orig task 3").prompt,
        "untouched multiline prompt must round-trip losslessly"
    );

    // Task 3's dep is now satisfied — next must advance to it.
    let assert = dt_story().arg("next").arg(&story).assert();
    let output = assert.success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    assert_eq!(json["id"], 3);
}

#[test]
fn complete_is_idempotent_on_already_completed_task() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    let orig = fs::read_to_string(fixture("runnable.md")).expect("read fixture");
    fs::write(&story, &orig).expect("write story copy");

    // Task 1 is already completed — completing it again is a success no-op
    // (task #60 double-writer collision, 2026-08-06), file untouched.
    let output = dt_story()
        .arg("complete")
        .arg(&story)
        .arg("1")
        .output()
        .expect("run");
    assert!(output.status.success(), "idempotent complete must exit 0");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("already completed"),
        "expected already-completed note, got: {stderr}"
    );
    let untouched = fs::read_to_string(&story).expect("read story");
    assert_eq!(
        untouched, orig,
        "idempotent complete must not modify the file"
    );
}

#[test]
fn complete_warns_on_commit_mismatch_but_succeeds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    let orig = fs::read_to_string(fixture("runnable.md")).expect("read fixture");
    fs::write(&story, &orig).expect("write story copy");

    // Task 1 already completed; re-completing with a different commit warns
    // (genuine conflict) but still exits 0 and leaves the file unchanged.
    let output = dt_story()
        .arg("complete")
        .arg(&story)
        .arg("1")
        .arg("--commit")
        .arg("deadbeefcafe")
        .output()
        .expect("run");
    assert!(output.status.success(), "mismatch complete must exit 0");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("not overwriting"),
        "expected commit-mismatch warning, got: {stderr}"
    );
    let untouched = fs::read_to_string(&story).expect("read story");
    assert_eq!(untouched, orig, "mismatch warning must not modify the file");
}

#[test]
fn escalate_round_trip_records_state_and_writes_out_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    let out = dir.path().join("escalation.json");
    let orig = fs::read_to_string(fixture("runnable.md")).expect("read fixture");
    fs::write(&story, &orig).expect("write story copy");

    dt_story()
        .arg("escalate")
        .arg(&story)
        .arg("2")
        .arg("--reason")
        .arg("env-tests-failed")
        .arg("--log")
        .arg("escalations/task-2.log")
        .arg("--out")
        .arg(&out)
        .assert()
        .success();

    let mutated = fs::read_to_string(&story).expect("read mutated story");
    assert_non_manifest_bytes_preserved(&orig, &mutated);
    let manifest = parse_manifest(&mutated);
    let task2 = manifest.task(2).expect("task 2");
    assert_eq!(task2.status, Status::Escalated);
    assert_eq!(task2.escalation.as_deref(), Some("escalations/task-2.log"));

    let out_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&out).expect("read out json"))
            .expect("out JSON parses");
    assert_eq!(out_json["story"], "client-join-meeting");
    assert_eq!(out_json["task_id"], 2);
    assert_eq!(out_json["reason"], "env-tests-failed");
    assert_eq!(out_json["log"], "escalations/task-2.log");

    // Escalated tasks are retryable: next reopens task 2 instead of
    // reporting task 3 as blocked.
    let assert = dt_story().arg("next").arg(&story).assert();
    let output = assert.success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    assert_eq!(json["id"], 2);
}

#[test]
fn next_reopens_escalated_task_for_retry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    let orig = fs::read_to_string(fixture("runnable.md")).expect("read fixture");
    fs::write(&story, &orig).expect("write story copy");

    dt_story()
        .arg("escalate")
        .arg(&story)
        .arg("2")
        .arg("--reason")
        .arg("env-tests-failed")
        .arg("--log")
        .arg("escalations/task-2.log")
        .assert()
        .success();

    // next must return the SAME task, flip the file back to pending with
    // the escalation cleared, and announce the reopen on stderr.
    let assert = dt_story().arg("next").arg(&story).assert();
    let output = assert.success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    assert_eq!(json["id"], 2);
    assert_eq!(json["specialist"], "global-controller");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("dt-story: reopening escalated task 2 for retry"),
        "reopen must be announced on stderr, got: {stderr}"
    );

    let mutated = fs::read_to_string(&story).expect("read mutated story");
    assert_non_manifest_bytes_preserved(&orig, &mutated);
    let manifest = parse_manifest(&mutated);
    let task2 = manifest.task(2).expect("task 2");
    assert_eq!(task2.status, Status::Pending);
    assert_eq!(
        task2.escalation, None,
        "reopen must clear the escalation field"
    );

    // A second next selects the (now plain pending) task again without a
    // reopen notice and without rewriting the file.
    let assert = dt_story().arg("next").arg(&story).assert();
    let output = assert.success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    assert_eq!(json["id"], 2);
    assert!(
        output.stderr.is_empty(),
        "no reopen happened, so stderr must be silent"
    );
    let unchanged = fs::read_to_string(&story).expect("re-read story");
    assert_eq!(
        unchanged, mutated,
        "a non-reopening next must not rewrite the story file"
    );
}

// ------------------------------------------------------------ validate --

fn run_validate(yaml_body: &str) -> (i32, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    fs::write(&story, story_md(yaml_body)).expect("write story");
    let output = dt_story()
        .arg("validate")
        .arg(&story)
        .assert()
        .get_output()
        .clone();
    let code = output.status.code().expect("exit code");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    (code, stderr)
}

#[test]
fn validate_accepts_well_formed_fixture() {
    dt_story()
        .arg("validate")
        .arg(fixture("runnable.md"))
        .assert()
        .success();
}

#[test]
fn validate_rejects_unknown_field() {
    let (code, stderr) =
        run_validate("story: s\nbranch: b\ntasks:\n- id: 1\n  status: completed\n  owner: bob\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("unknown field"), "got: {stderr}");
}

#[test]
fn validate_rejects_dangling_dep() {
    let (code, stderr) = run_validate(
        "story: s\nbranch: b\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  deps: [9]\n  prompt: p\n",
    );
    assert_eq!(code, 1);
    assert!(
        stderr.contains("dep 9 references no existing task"),
        "got: {stderr}"
    );
}

#[test]
fn validate_rejects_dependency_cycle() {
    let (code, stderr) = run_validate(
        "story: s\nbranch: b\ntasks:\n\
         - id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  deps: [2]\n  prompt: p\n\
         - id: 2\n  status: pending\n  specialist: test\n  env_tests: false\n  deps: [1]\n  prompt: p\n",
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("cycle"), "got: {stderr}");
}

#[test]
fn validate_rejects_duplicate_id() {
    let (code, stderr) = run_validate(
        "story: s\nbranch: b\ntasks:\n- id: 1\n  status: completed\n- id: 1\n  status: completed\n",
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("duplicate task id 1"), "got: {stderr}");
}

#[test]
fn validate_rejects_pending_task_missing_prompt() {
    let (code, stderr) = run_validate(
        "story: s\nbranch: b\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: true\n",
    );
    assert_eq!(code, 1);
    assert!(
        stderr.contains("pending task 1 has no prompt"),
        "got: {stderr}"
    );
}
