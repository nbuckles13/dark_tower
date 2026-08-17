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

// ------------------------------------------------------------ add-task --

#[test]
fn add_task_appends_runnable_pending_task() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    let orig = fs::read_to_string(fixture("runnable.md")).expect("read fixture");
    fs::write(&story, &orig).expect("write story copy");

    let prompt_path = dir.path().join("prompt.txt");
    let prompt_body = "Fix the audit advisory.\nPrefer a version bump over suppression.\n";
    fs::write(&prompt_path, prompt_body).expect("write prompt");

    let output = dt_story()
        .arg("add-task")
        .arg(&story)
        .arg("--specialist")
        .arg("infrastructure")
        .arg("--prompt-file")
        .arg(&prompt_path)
        .arg("--tag")
        .arg("audit-remediation-rust")
        .arg("--env-tests")
        .output()
        .expect("run");
    assert!(output.status.success(), "add-task must exit 0 on append");
    let new_id: u32 = String::from_utf8(output.stdout)
        .expect("utf-8 stdout")
        .trim()
        .parse()
        .expect("stdout is the new id");
    // runnable.md has tasks 1,2,3 → fresh id is 4.
    assert_eq!(new_id, 4);

    let mutated = fs::read_to_string(&story).expect("read mutated story");
    assert_non_manifest_bytes_preserved(&orig, &mutated);
    let manifest = parse_manifest(&mutated);
    let added = manifest.task(new_id).expect("added task");
    assert_eq!(added.status, Status::Pending);
    assert_eq!(added.specialist.as_deref(), Some("infrastructure"));
    assert_eq!(added.env_tests, Some(true));
    assert_eq!(added.tag.as_deref(), Some("audit-remediation-rust"));
    // Trailing newline trimmed, internal newline preserved.
    assert_eq!(
        added.prompt.as_deref(),
        Some("Fix the audit advisory.\nPrefer a version bump over suppression.")
    );

    // The re-serialized multi-line prompt is a YAML block scalar.
    let new_span = markdown::find_manifest_block(&mutated).expect("mutated block");
    let new_yaml = markdown::manifest_yaml(&mutated, &new_span).expect("mutated yaml");
    assert!(
        new_yaml.contains("prompt: |"),
        "multiline prompt must re-serialize as a block scalar, got:\n{new_yaml}"
    );
}

#[test]
fn add_task_with_existing_tag_is_idempotent_noop() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    let orig = fs::read_to_string(fixture("runnable.md")).expect("read fixture");
    fs::write(&story, &orig).expect("write story copy");

    let prompt_path = dir.path().join("prompt.txt");
    fs::write(&prompt_path, "Do the thing.\n").expect("write prompt");

    let add = || {
        dt_story()
            .arg("add-task")
            .arg(&story)
            .arg("--specialist")
            .arg("client")
            .arg("--prompt-file")
            .arg(&prompt_path)
            .arg("--tag")
            .arg("dup-tag")
            .output()
            .expect("run")
    };

    // First append: exit 0, new id 4, env_tests defaults to false (flag absent).
    let first = add();
    assert!(first.status.success());
    let first_id: u32 = String::from_utf8(first.stdout)
        .expect("utf-8")
        .trim()
        .parse()
        .expect("id");
    assert_eq!(first_id, 4);
    let after_first = fs::read_to_string(&story).expect("read story");
    assert_eq!(
        parse_manifest(&after_first)
            .task(first_id)
            .expect("added task")
            .env_tests,
        Some(false),
        "absent --env-tests flag must default to false"
    );

    // Second append with the same tag: exit 4, prints the existing id, no write.
    let second = add();
    assert_eq!(
        second.status.code(),
        Some(4),
        "duplicate tag must exit 4 (EXIT_TAG_EXISTS)"
    );
    let echoed_id: u32 = String::from_utf8(second.stdout)
        .expect("utf-8")
        .trim()
        .parse()
        .expect("existing id");
    assert_eq!(echoed_id, first_id);
    let stderr = String::from_utf8(second.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("already exists"),
        "duplicate must announce on stderr, got: {stderr}"
    );
    let after_second = fs::read_to_string(&story).expect("re-read story");
    assert_eq!(
        after_second, after_first,
        "duplicate-tag add-task must not modify the file"
    );
}

#[test]
fn add_task_missing_prompt_file_exits_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    fs::write(
        &story,
        fs::read_to_string(fixture("runnable.md")).expect("read fixture"),
    )
    .expect("write story");

    dt_story()
        .arg("add-task")
        .arg(&story)
        .arg("--specialist")
        .arg("client")
        .arg("--prompt-file")
        .arg(dir.path().join("nope.txt"))
        .arg("--tag")
        .arg("t")
        .assert()
        .code(2);
}

#[test]
fn validate_accepts_manifest_with_tagged_task() {
    let (code, stderr) = run_validate(
        "story: s\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  prompt: p\n  tag: audit-remediation-ts\n",
    );
    assert_eq!(
        code, 0,
        "tagged pending task must validate, stderr: {stderr}"
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
        run_validate("story: s\ntasks:\n- id: 1\n  status: completed\n  owner: bob\n");
    assert_eq!(code, 1);
    assert!(stderr.contains("unknown field"), "got: {stderr}");
}

#[test]
fn validate_rejects_dangling_dep() {
    let (code, stderr) = run_validate(
        "story: s\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  deps: [9]\n  prompt: p\n",
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
        "story: s\ntasks:\n\
         - id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  deps: [2]\n  prompt: p\n\
         - id: 2\n  status: pending\n  specialist: test\n  env_tests: false\n  deps: [1]\n  prompt: p\n",
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("cycle"), "got: {stderr}");
}

#[test]
fn validate_rejects_duplicate_id() {
    let (code, stderr) = run_validate(
        "story: s\ntasks:\n- id: 1\n  status: completed\n- id: 1\n  status: completed\n",
    );
    assert_eq!(code, 1);
    assert!(stderr.contains("duplicate task id 1"), "got: {stderr}");
}

#[test]
fn validate_rejects_pending_task_missing_prompt() {
    let (code, stderr) = run_validate(
        "story: s\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: true\n",
    );
    assert_eq!(code, 1);
    assert!(
        stderr.contains("pending task 1 has no prompt"),
        "got: {stderr}"
    );
}

// ---------------------------------------------------------- list-tasks --
//
// `list-tasks` is a read-only projection consumed by `run-story.sh`'s
// `--stop-after` validation and probed by `preflight-story.sh`. Its contract
// (see the module doc in `src/main.rs`): exit 0 with a single-line JSON array
// on stdout, exit 2 with EMPTY stdout on any error, no other exit code, and
// never a write to the story file.

/// Write `yaml_body` as a story file in a fresh tempdir; return both.
fn story_in_tempdir(yaml_body: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    fs::write(&story, story_md(yaml_body)).expect("write story");
    (dir, story)
}

/// Task ids deliberately out of ascending order, all three statuses present,
/// and every non-projected field populated — `specialist`, `env_tests`,
/// `prompt`, `commit`, `escalation`. Any of those appearing in the output is
/// a widening of the projection. Task 1 carries a `slug`, which IS projected
/// (rule 1: its named consumer is `/close-story` Phase 1/4), and tasks 2/3 do
/// not, so the `null`-not-absent property below is exercised too.
const MIXED_MANIFEST: &str = "story: s\ntasks:\n\
- id: 3\n  status: pending\n  specialist: protocol\n  env_tests: false\n  deps:\n  - 1\n  prompt: |\n    third\n\
- id: 1\n  status: completed\n  commit: abc1234\n  slug: 2026-08-13-some-devloop\n\
- id: 2\n  status: escalated\n  specialist: test\n  env_tests: true\n  deps:\n  - 1\n  prompt: |\n    second\n  escalation: /tmp/devloop/story-runner/s/task-2.log\n";

#[test]
fn list_tasks_projects_exactly_four_keys_in_manifest_order() {
    let (_dir, story) = story_in_tempdir(MIXED_MANIFEST);

    let assert = dt_story().arg("list-tasks").arg(&story).assert();
    let output = assert.success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");

    assert_eq!(
        stdout.trim_end().lines().count(),
        1,
        "output must be a single compact line, got:\n{stdout}"
    );

    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    let array = json.as_array().expect("top level must be a JSON array");
    assert_eq!(array.len(), 3);

    // Closed-world key assertion. Asserting each expected field individually
    // is open-world: it passes unchanged when a key is ADDED, which is the
    // one thing this test exists to catch (e.g. someone swapping the
    // projection for a serialization of `manifest::Task`, which would drag in
    // `prompt`, `commit` and `escalation`).
    for element in array {
        let obj = element.as_object().expect("element must be a JSON object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["deps", "id", "slug", "status"],
            "projection key set must be exactly {{id, status, deps, slug}}, got: {element}"
        );
    }

    // Manifest order, NOT sorted by id — re-sorting would be dt-story forming
    // a second opinion about the file's own task order.
    let ids: Vec<i64> = array
        .iter()
        .map(|e| e["id"].as_i64().expect("id must be a number"))
        .collect();
    assert_eq!(
        ids,
        vec![3, 1, 2],
        "tasks must be emitted in manifest order"
    );

    // Pins the lowercase status token set the runner's jq matches on.
    let statuses: Vec<&str> = array
        .iter()
        .map(|e| e["status"].as_str().expect("status must be a string"))
        .collect();
    assert_eq!(statuses, vec!["pending", "completed", "escalated"]);

    // `deps` is total: always present, `[]` rather than absent, so consumers
    // never need `// []`.
    assert_eq!(array[0]["deps"], serde_json::json!([1]));
    assert_eq!(
        array[1]["deps"],
        serde_json::json!([]),
        "a task with no deps must emit an empty array, not omit the key"
    );

    // `slug` is likewise TOTAL: present on every element, `null` when unset,
    // so `/close-story` never has to distinguish absent from empty.
    assert_eq!(
        array[1]["slug"],
        serde_json::json!("2026-08-13-some-devloop")
    );
    assert_eq!(
        array[0]["slug"],
        serde_json::Value::Null,
        "a task with no slug must emit null, not omit the key"
    );

    // The projection must NOT be a serialization of `manifest::Task`: the
    // uncontrolled/non-durable fields stay off the wire (rules 2 and 3).
    for element in array {
        for banned in [
            "prompt",
            "commit",
            "escalation",
            "specialist",
            "env_tests",
            "tag",
        ] {
            assert!(
                element.get(banned).is_none(),
                "{banned} must not be projected: {element}"
            );
        }
    }
}

/// The escalated task is the lowest-id candidate with satisfied deps and no
/// pending task ahead of it, so `engine::next` is GUARANTEED to select it and
/// flip it back to pending — i.e. `next` writes the file for this input.
/// Without that guarantee the read-only assertion below would pass against a
/// `list-tasks` that simply delegated to `next`.
const REOPEN_MANIFEST: &str = "story: s\ntasks:\n\
- id: 1\n  status: escalated\n  specialist: test\n  env_tests: false\n  prompt: |\n    only task\n  escalation: /tmp/devloop/story-runner/s/task-1.log\n\
- id: 2\n  status: pending\n  specialist: test\n  env_tests: false\n  deps:\n  - 1\n  prompt: |\n    blocked on 1\n";

#[test]
fn list_tasks_never_writes_the_story_file() {
    let (_dir, story) = story_in_tempdir(REOPEN_MANIFEST);
    let orig = fs::read_to_string(&story).expect("read story");

    // Precondition: prove the trap can spring. `next` on an identical fixture
    // MUST mutate it — otherwise this fixture cannot distinguish a read-only
    // `list-tasks` from one that delegates to `engine::next`, and the
    // assertion below would be vacuous. Selection order lives in
    // `engine::next` and could change; this fails loudly if it does.
    let (_probe_dir, probe_story) = story_in_tempdir(REOPEN_MANIFEST);
    dt_story().arg("next").arg(&probe_story).assert().success();
    let after_next = fs::read_to_string(&probe_story).expect("read probe story");
    assert_ne!(
        orig, after_next,
        "fixture precondition broken: `next` must reopen the escalated task \
         and rewrite the file, or the read-only assertion proves nothing"
    );

    dt_story().arg("list-tasks").arg(&story).assert().success();

    // Whole-file equality, not `assert_non_manifest_bytes_preserved` — the
    // latter ignores the manifest block, which is exactly the region a
    // delegating implementation would rewrite.
    let after = fs::read_to_string(&story).expect("re-read story");
    assert_eq!(
        orig, after,
        "list-tasks must leave the story file byte-identical"
    );
}

/// The one input where "exit 0 implies non-empty stdout" is closest to
/// failing. `preflight-story.sh`'s `[ -n "$out" ]` check treats rc-0-with-
/// empty-stdout as a contract violation, so an empty manifest MUST still
/// emit the two bytes `[]` rather than nothing — and if the projection ever
/// became "emit nothing when there is nothing", that check would silently
/// start passing an unfounded state.
///
/// The input is reachable **for this verb**: `list-tasks` goes through
/// `load()` and never calls `engine::validate_manifest`, whereas `validate`
/// now REJECTS a zero-task manifest (it asserts the story is complete —
/// `next` would exit 3 AllDone). That split is deliberate and load-bearing:
/// it is what lets `/user-story` emit a `tasks: []` skeleton and populate it
/// with `add-task`, while a skeleton left on disk still stops the runner and
/// reds CI. So do NOT "fix" this test to match `validate`'s behaviour — the
/// two verbs disagree on this input on purpose.
#[test]
fn list_tasks_empty_manifest_emits_empty_array_not_empty_stdout() {
    let (_dir, story) = story_in_tempdir("story: s\ntasks: []\n");

    let assert = dt_story().arg("list-tasks").arg(&story).assert();
    let output = assert.success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");

    assert!(
        !stdout.trim().is_empty(),
        "exit 0 must never carry empty stdout — preflight reads that as a \
         contract violation"
    );
    assert_eq!(stdout.trim(), "[]");
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    assert_eq!(
        json.as_array().expect("top level must be an array").len(),
        0
    );
}

#[test]
fn list_tasks_malformed_exits_two_with_empty_stdout() {
    let assert = dt_story()
        .arg("list-tasks")
        .arg(fixture("malformed.md"))
        .assert();
    let output = assert.code(2).get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "exit 2 must print nothing on stdout — a consumer's jq must never \
         see partial output, and preflight reads rc-0-with-empty-stdout as a \
         contract violation"
    );
    assert!(!output.stderr.is_empty(), "exit 2 must diagnose on stderr");
}

#[test]
fn list_tasks_missing_file_exits_two_with_empty_stdout() {
    let assert = dt_story()
        .arg("list-tasks")
        .arg(fixture("does-not-exist.md"))
        .assert();
    let output = assert.code(2).get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "exit 2 must print nothing on stdout"
    );
}

// ===========================================================================
// Story task 4 (R-8): schema change, slug lifecycle, and the fence-collision
// controls. Grouped because they share one property: every pre-existing
// control in this crate compares TASK IDS, and several failures below leave
// the id set intact while changing what the manifest MEANS.
// ===========================================================================

/// `branch` was a required field nothing read. Removing it from the struct
/// makes every manifest still carrying it fail `deny_unknown_fields`.
///
/// Editing the fixtures proves the NEW shape parses and nothing about the
/// OLD one, so this asserts the rejection directly — otherwise a
/// reintroduced `pub branch` would go unnoticed by the whole suite.
#[test]
fn validate_rejects_removed_branch_field() {
    let (code, stderr) =
        run_validate("story: s\nbranch: b\ntasks:\n- id: 1\n  status: completed\n");
    assert_eq!(code, 1, "a manifest still carrying `branch` must fail");
    assert!(
        stderr.contains("branch"),
        "the violation must name the offending field so a stale-binary vs \
         stale-manifest diagnosis is possible, got: {stderr}"
    );
}

// --- Slug: class enforcement, round-trip, last-writer-wins ----------------

#[test]
fn slug_round_trips_through_complete() {
    let (dir, story) = story_in_tempdir(
        "story: s\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  prompt: p\n",
    );
    dt_story()
        .args(["complete"])
        .arg(&story)
        .args(["1", "--slug", "2026-08-17-manifest-single-home"])
        .assert()
        .success();

    let text = fs::read_to_string(&story).expect("read back");
    assert!(
        text.contains("slug: 2026-08-17-manifest-single-home"),
        "slug must be persisted, got:\n{text}"
    );
    let manifest = parse_manifest(&text);
    assert_eq!(manifest.tasks[0].status, Status::Completed);
    assert_eq!(
        manifest.tasks[0].slug.as_ref().map(|s| s.as_str()),
        Some("2026-08-17-manifest-single-home")
    );
    drop(dir);
}

/// An out-of-class slug fails the WHOLE FILE to parse rather than yielding a
/// per-task violation. That blast radius is a deliberate design choice (it
/// matches how illegal `Status` values and unknown fields already behave), so
/// it is pinned rather than left to be discovered.
#[test]
fn invalid_slug_fails_the_whole_manifest_to_parse() {
    let (_dir, story) = story_in_tempdir(
        "story: s\ntasks:\n- id: 1\n  status: completed\n  slug: Not_A_Valid_Slug\n",
    );
    let assert = dt_story().arg("next").arg(&story).assert();
    let output = assert.get_output().clone();
    assert_eq!(
        output.status.code(),
        Some(2),
        "an out-of-class slug is a malformed manifest, not a validation warning"
    );
}

#[test]
fn complete_rejects_out_of_class_slug_at_the_cli_boundary() {
    let (_dir, story) = story_in_tempdir(
        "story: s\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  prompt: p\n",
    );
    // Enforced at the clap boundary as well as in the deserializer, because
    // `--slug` is a SECOND producer (a model on a command line), less
    // constrained than the runner's derived value.
    dt_story()
        .args(["complete"])
        .arg(&story)
        .args(["1", "--slug", "../escape"])
        .assert()
        .failure();
    let text = fs::read_to_string(&story).expect("read back");
    assert!(
        !text.contains("escape"),
        "a rejected slug must not reach the file at all"
    );
}

/// LAST-WRITER-WINS. Reds under a first-writer-wins implementation, which is
/// the rule `commit` uses one field above — the two are deliberately opposite
/// and each direction needs its own test.
#[test]
fn slug_is_last_writer_wins_on_an_already_completed_task() {
    let (_dir, story) = story_in_tempdir(
        "story: s\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  prompt: p\n",
    );
    for slug in ["2026-08-16-first-attempt", "2026-08-17-second-attempt"] {
        dt_story()
            .args(["complete"])
            .arg(&story)
            .args(["1", "--slug", slug])
            .assert()
            .success();
    }
    let manifest = parse_manifest(&fs::read_to_string(&story).expect("read back"));
    assert_eq!(
        manifest.tasks[0].slug.as_ref().map(|s| s.as_str()),
        Some("2026-08-17-second-attempt"),
        "the attempt that completed LAST must win"
    );
}

/// `commit` keeps FIRST-writer-wins. Pinned alongside the test above so the
/// asymmetry cannot be "tidied" into consistency without reddening one of them.
#[test]
fn commit_is_not_overwritten_on_an_already_completed_task() {
    let (_dir, story) =
        story_in_tempdir("story: s\ntasks:\n- id: 1\n  status: completed\n  commit: aaa1111\n");
    dt_story()
        .args(["complete"])
        .arg(&story)
        .args(["1", "--commit", "bbb2222"])
        .assert()
        .success();
    let manifest = parse_manifest(&fs::read_to_string(&story).expect("read back"));
    assert_eq!(manifest.tasks[0].commit.as_deref(), Some("aaa1111"));
}

/// The runner executing story task 4 runs the PRE-EDIT script, so it calls
/// the two-arg `complete` against the new binary on a task whose slug was
/// hand-written. That path must flip status and leave the slug alone — it is
/// not the path the `AlreadyComplete` logic exercises.
#[test]
fn complete_without_slug_preserves_an_existing_slug() {
    let (_dir, story) = story_in_tempdir(
        "story: s\ntasks:\n- id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  prompt: p\n  slug: 2026-08-17-hand-written\n",
    );
    dt_story()
        .args(["complete"])
        .arg(&story)
        .arg("1")
        .assert()
        .success();
    let manifest = parse_manifest(&fs::read_to_string(&story).expect("read back"));
    assert_eq!(manifest.tasks[0].status, Status::Completed);
    assert_eq!(
        manifest.tasks[0].slug.as_ref().map(|s| s.as_str()),
        Some("2026-08-17-hand-written"),
        "omitting --slug must not clear a slug that is already recorded"
    );
}

/// "No change written" is only tested by comparing BYTES. Exit 0 alone would
/// pass against an implementation that rewrote the file identically-ish.
#[test]
fn already_complete_without_slug_change_writes_nothing() {
    let (_dir, story) = story_in_tempdir(
        "story: s\ntasks:\n- id: 1\n  status: completed\n  slug: 2026-08-17-same\n",
    );
    let before = fs::read_to_string(&story).expect("read before");
    dt_story()
        .args(["complete"])
        .arg(&story)
        .args(["1", "--slug", "2026-08-17-same"])
        .assert()
        .success();
    let after = fs::read_to_string(&story).expect("read after");
    assert_eq!(before, after, "an unchanged slug must not rewrite the file");
}

// --- Zero-task manifest ---------------------------------------------------

/// BOTH halves are the invariant. A zero-task manifest is a valid document
/// that ASSERTS COMPLETENESS (`next` exits 3 = AllDone), so `validate` must
/// reject it — while `add-task` must still accept it, or the emission
/// skeleton is unusable by the one tool whose job is to fill it.
#[test]
fn validate_rejects_zero_task_manifest_but_add_task_accepts_it() {
    let (code, stderr) = run_validate("story: s\ntasks: []\n");
    assert_eq!(code, 1, "an empty task list asserts the story is complete");
    assert!(stderr.contains("no tasks"), "got: {stderr}");

    let (dir, story) = story_in_tempdir("story: s\ntasks: []\n");
    let prompt = dir.path().join("p.txt");
    fs::write(&prompt, "first task").expect("write prompt");
    dt_story()
        .args(["add-task"])
        .arg(&story)
        .args(["--specialist", "operations"])
        .arg("--prompt-file")
        .arg(&prompt)
        .args(["--tag", "story-s-task-1"])
        .assert()
        .success();
}

// --- add-task: deps, id assignment, weak idempotency ----------------------

#[test]
fn add_task_records_deps_and_returns_the_assigned_id() {
    let (dir, story) = story_in_tempdir("story: s\ntasks:\n- id: 1\n  status: completed\n");
    let prompt = dir.path().join("p.txt");
    fs::write(&prompt, "second task").expect("write prompt");
    let assert = dt_story()
        .args(["add-task"])
        .arg(&story)
        .args(["--specialist", "operations"])
        .arg("--prompt-file")
        .arg(&prompt)
        .args(["--tag", "story-s-task-2", "--deps", "1"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8");
    assert_eq!(stdout.trim(), "2", "the assigned id must be on stdout");
    let manifest = parse_manifest(&fs::read_to_string(&story).expect("read back"));
    assert_eq!(manifest.tasks[1].deps, vec![1]);
}

/// Dep validation is DELTA-form: an append that would introduce a dangling
/// dep is refused, so the runner never writes a manifest its own gate rejects.
#[test]
fn add_task_refuses_a_dangling_dep() {
    let (dir, story) = story_in_tempdir("story: s\ntasks:\n- id: 1\n  status: completed\n");
    let prompt = dir.path().join("p.txt");
    fs::write(&prompt, "second").expect("write prompt");
    dt_story()
        .args(["add-task"])
        .arg(&story)
        .args(["--specialist", "operations"])
        .arg("--prompt-file")
        .arg(&prompt)
        .args(["--tag", "t", "--deps", "99"])
        .assert()
        .failure();
    let manifest = parse_manifest(&fs::read_to_string(&story).expect("read back"));
    assert_eq!(
        manifest.tasks.len(),
        1,
        "the refused task must not be appended"
    );
}

#[test]
fn add_task_rejects_empty_and_malformed_deps() {
    let (dir, story) = story_in_tempdir("story: s\ntasks:\n- id: 1\n  status: completed\n");
    let prompt = dir.path().join("p.txt");
    fs::write(&prompt, "x").expect("write prompt");
    // `--deps ''` is what a caller with an unset variable emits; a silently
    // dep-free task would be a masked failure.
    for bad in ["", "1,,2", "1,x", "1,1"] {
        dt_story()
            .args(["add-task"])
            .arg(&story)
            .args(["--specialist", "operations"])
            .arg("--prompt-file")
            .arg(&prompt)
            .args(["--tag", "t", "--deps", bad])
            .assert()
            .failure();
    }
}

/// WEAK idempotency: the tag short-circuit returns before any field is
/// written, so a re-run with corrected values discards them. The rc-4 notice
/// must say so — a silent discard reads as convergence and is not.
#[test]
fn add_task_existing_tag_discards_supplied_fields_and_says_so() {
    let (dir, story) = story_in_tempdir("story: s\ntasks:\n- id: 1\n  status: completed\n");
    let prompt = dir.path().join("p.txt");
    fs::write(&prompt, "original").expect("write prompt");
    dt_story()
        .args(["add-task"])
        .arg(&story)
        .args(["--specialist", "operations"])
        .arg("--prompt-file")
        .arg(&prompt)
        .args(["--tag", "story-s-task-2"])
        .assert()
        .success();

    let revised = dir.path().join("p2.txt");
    fs::write(&revised, "REVISED PROMPT").expect("write revised");
    let assert = dt_story()
        .args(["add-task"])
        .arg(&story)
        .args(["--specialist", "database"])
        .arg("--prompt-file")
        .arg(&revised)
        .args(["--tag", "story-s-task-2", "--deps", "1"])
        .assert()
        .code(4);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("utf-8");
    assert!(
        stderr.contains("NOT UPDATED"),
        "the discard must be loud, got: {stderr}"
    );

    let text = fs::read_to_string(&story).expect("read back");
    assert!(
        !text.contains("REVISED PROMPT"),
        "the revision was discarded"
    );
    let manifest = parse_manifest(&text);
    assert_eq!(manifest.tasks.len(), 2, "no duplicate append");
    assert!(
        manifest.tasks[1].deps.is_empty(),
        "supplied deps not applied"
    );
}

// --- Fence collision: the four controls, and their measured coverage ------
//
// A prompt containing a markdown fence closes the manifest block early. The
// truncation is SILENT: the surviving prefix parses, deps point backwards so
// nothing dangles, and `validate_manifest` returns zero violations on a
// manifest that has lost its tail. `next` then reports the story complete.
//
// Whether the EOF check fires depends on the parity of fence lines after the
// truncation point, which depends on whether the model tagged its fence and
// how many fenced blocks follow — i.e. on formatting accidents. So the cases
// below deliberately cover BOTH parities; a suite with only the odd one
// ships the fix for the shape that almost never occurs.

/// Build a story whose task-1 prompt carries `injected`, with `after`
/// appended as ordinary document text following the manifest block.
fn story_with_injected_prompt(injected: &str, after: &str) -> String {
    format!(
        "# Title\n\nprose\n\n```yaml\n# task-metadata (dt-story manifest v1)\nstory: s\ntasks:\n\
         - id: 1\n  status: pending\n  specialist: test\n  env_tests: false\n  prompt: |\n\
         {injected}\n- id: 2\n  status: pending\n  specialist: test\n  env_tests: false\n  prompt: p\n\
         ```\n\n{after}\n"
    )
}

fn validate_text(text: &str) -> (i32, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let story = dir.path().join("story.md");
    fs::write(&story, text).expect("write");
    let assert = dt_story().arg("validate").arg(&story).assert();
    let out = assert.get_output().clone();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (a) Control: an ordinary prompt with no fence still validates. Guards the
/// two checks below against over-rejecting every normal story.
#[test]
fn fence_control_ordinary_prompt_validates() {
    let (code, stderr) = validate_text(&story_with_injected_prompt(
        "    an ordinary prompt with `inline backticks`",
        "trailing prose",
    ));
    assert_eq!(
        code, 0,
        "an ordinary manifest must still validate: {stderr}"
    );
}

/// (b) EVEN parity — a properly closed bare fence, which is what a model
/// actually writes. Fence lines pair up, so nothing dangles at EOF and the
/// unterminated-fence check CANNOT fire. Only the orphan scan sees this.
/// This case must red against the EOF check alone.
#[test]
fn fence_even_parity_truncation_is_still_caught() {
    let (code, stderr) = validate_text(&story_with_injected_prompt(
        "    example:\n    ```\n    some code\n    ```",
        "trailing prose",
    ));
    assert_eq!(
        code, 1,
        "an even-parity truncation must be caught: {stderr}"
    );
    // Assert the ORPHAN SCAN specifically, not "some check fired". The EOF
    // check provably cannot see this input (fence lines pair up), so
    // accepting its message here would let the orphan scan be deleted while
    // this test kept passing — which is the exact coverage this case exists
    // to hold. Measured: this input yields the orphan message naming `- id: 2`.
    assert!(
        stderr.contains("outside the manifest block"),
        "the orphan scan must be what catches even parity, got: {stderr}"
    );
    assert!(
        stderr.contains("- id: 2"),
        "must name the lost task: {stderr}"
    );
}

/// (c) ODD parity — a language-tagged fence. The tagged line is swallowed as
/// content, the example's bare close ends the block, and the manifest's real
/// close is left dangling at EOF.
#[test]
fn fence_odd_parity_truncation_is_caught() {
    let (code, stderr) = validate_text(&story_with_injected_prompt(
        "    example:\n    ```bash\n    ls -la\n    ```",
        "trailing prose",
    ));
    assert_eq!(
        code, 1,
        "a tagged-fence truncation must be caught: {stderr}"
    );
}

/// (d) Tagged fence PLUS an ordinary fenced block later in the document —
/// the parity flips back to even, so again only the orphan scan sees it.
#[test]
fn fence_truncation_with_a_later_code_block_is_caught() {
    let (code, stderr) = validate_text(&story_with_injected_prompt(
        "    example:\n    ```bash\n    ls -la\n    ```",
        "prose\n\n```\nan ordinary later code block\n```\n",
    ));
    assert_eq!(code, 1, "must not depend on fence parity: {stderr}");
}

/// (e) The EOF check must not reject a story merely for having other fenced
/// blocks — this story file itself carries several.
#[test]
fn ordinary_fenced_blocks_elsewhere_do_not_trip_the_checks() {
    let (code, stderr) = validate_text(&story_with_injected_prompt(
        "    an ordinary prompt",
        "prose\n\n```bash\nrun something\n```\n\nmore prose\n\n```\nplain block\n```\n",
    ));
    assert_eq!(code, 0, "legitimate fenced blocks must be fine: {stderr}");
}

/// (f) The ORPHAN detector must not over-reject: a story discussing task ids
/// in prose is normal documentation, not a truncation. The end anchor is what
/// makes this pass, and the leading-whitespace tolerance is what makes (b)
/// fail — both halves are load-bearing and neither alone is enough.
#[test]
fn prose_discussing_task_ids_does_not_trip_the_orphan_detector() {
    let (code, stderr) = validate_text(&story_with_injected_prompt(
        "    an ordinary prompt",
        "Notes:\n\n- id: 3 is the one that broke\n- id: 5 (see above) was struck\n",
    ));
    assert_eq!(
        code, 0,
        "prose mentioning ids must not be read as truncation: {stderr}"
    );
}

/// The WRITE path is closed by construction: `to_block_body` refuses to emit
/// a fence, so `add-task` cannot corrupt a story file it merely round-trips.
/// This is the control that makes `/user-story`'s emission route safe.
#[test]
fn add_task_refuses_a_prompt_containing_a_fence() {
    let (dir, story) = story_in_tempdir("story: s\ntasks:\n- id: 1\n  status: completed\n");
    let prompt = dir.path().join("p.txt");
    fs::write(&prompt, "Run this:\n\n```bash\nls -la\n```\n").expect("write prompt");
    let assert = dt_story()
        .args(["add-task"])
        .arg(&story)
        .args(["--specialist", "operations"])
        .arg("--prompt-file")
        .arg(&prompt)
        .args(["--tag", "t"])
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).expect("utf-8");
    assert!(
        stderr.contains("fence"),
        "must name the mechanism: {stderr}"
    );

    // The refusal must leave the file untouched, not half-written.
    let manifest = parse_manifest(&fs::read_to_string(&story).expect("read back"));
    assert_eq!(manifest.tasks.len(), 1);
}
