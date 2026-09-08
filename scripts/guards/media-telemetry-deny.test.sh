#!/usr/bin/env bash
#
# Self-test for scripts/guards/simple/media-telemetry-deny.sh
# (dt-guard subcommand `media-telemetry-deny`, ADR-0036 §11).
#
# ---------------------------------------------------------------------------
# WHY THIS FILE EXISTS
# ---------------------------------------------------------------------------
# The guard PASSES on every real run — `crates/mh-service/src/media/` is clean
# and is meant to stay that way. A passing guard exercises none of its failure
# branches, and those branches are its entire value: a check that walks
# nothing, compares nothing and reports success is the exact bug ADR-0036 §11
# was written to prevent ("alive, never applied").
#
# This drives, against synthetic roots: every scope/parse failure token, one
# hit per denied family, the allow-list and comment negatives, the REASON
# precedence ladder with two conditions live at once, the no-suppression
# property, the output-shape invariants, and — the direction nobody tests —
# that the deny does NOT leak outside its configured scope.
#
# ---------------------------------------------------------------------------
# WHY IT IS NOT UNDER guards/simple/
# ---------------------------------------------------------------------------
# `run-guards.sh` discovers guards with `find … -name '*.sh'`, which matches
# `*.test.sh` too — a self-test placed there would be auto-run AS A PRODUCTION
# GUARD in addition to running here. Same reasoning as
# validate-subdomain-regex-sync.test.sh, validate-slug-class-sync.test.sh,
# validate-frame-vectors.test.sh and run-guards.test.sh.
#
# ---------------------------------------------------------------------------
# NO TEST SEAM, ON PURPOSE
# ---------------------------------------------------------------------------
# There is no `DEVLOOP_TEST`-gated override of the manifest path or the scan
# root, and none is needed: `--root` is a production clap flag and the manifest
# is read relative to it, so this suite builds throwaway roots and points the
# real binary at them. That is strictly better than a gated seam, because an
# env var that relocates a guard's scope is a disarm switch that has to be
# gated; an absent one cannot be misused.
#
# Hermetic: mktemp -d + EXIT trap, no cluster, no network, no cargo. The
# binary comes from Layer 1 (`scripts/lang/rust/compile.sh`); a missing binary
# is a LOUD failure here, never a skip.
set -euo pipefail
IFS=$'\n\t'

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

DT_GUARD="${DT_GUARD:-${REPO_ROOT}/target/release/dt-guard}"
if [[ ! -x "$DT_GUARD" ]]; then
  # Fail loudly. A skip here would make the whole suite a no-op exactly when
  # the binary is broken, which is the failure class this file exists for.
  printf '  - [precondition] dt-guard binary missing or not executable at %s\n' "$DT_GUARD"
  printf '\n%s: 0 passed, 1 failed\n' "$0"
  exit 1
fi

MANIFEST_REL="scripts/guards/simple/media-telemetry-deny.yaml"
REAL_MANIFEST="${REPO_ROOT}/${MANIFEST_REL}"
REAL_MEDIA="${REPO_ROOT}/crates/mh-service/src/media"
SENTINEL="SENTINEL_MUST_NOT_APPEAR_IN_GUARD_OUTPUT_participant_7f3a"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# One pristine copy of the real media tree, re-used per case via `cp -r`.
# Deliberately NOT one fresh copy of a large tree per case: that shape costs
# validate-frame-vectors.test.sh 23-27s and is the single largest item in
# Layer 3. This tree is ~64 KB.
PRISTINE="${WORK}/pristine"
mkdir -p "$PRISTINE"
cp -r "$REAL_MEDIA" "${PRISTINE}/media"

# How many `.rs` files the real scope holds, DERIVED rather than hardcoded.
#
# Two cases below assert the guard reports the right scan size. Writing the
# current count (7) as a literal would red this suite the next time anyone adds
# a file under `crates/mh-service/src/media/` — a test failing for a change it
# has no opinion about, which is how suites get weakened rather than fixed. The
# property under test is "the guard reports the size of the REAL configured
# scope", not "the scope has exactly N files"; deriving expresses that, and the
# `.rs`-tree-is-non-empty half is asserted in media_telemetry_deny_e2e.rs
# without a count for the same reason.
REAL_RS_COUNT="$(find "$REAL_MEDIA" -name '*.rs' -type f | wc -l | tr -d ' ')"
if [[ "$REAL_RS_COUNT" -lt 1 ]]; then
  printf '  - [precondition] real media scope holds zero .rs files; the fires cases would be vacuous\n'
  printf '\n%s: 0 passed, 1 failed\n' "$0"
  exit 1
fi

# Build a synthetic repo root.
#   $1 = root dir
#   $2 = manifest mode: `real` (verbatim copy of the shipped manifest),
#        `custom` (caller writes it), `none` (no manifest at all)
# In `real` mode the media tree is placed at the SAME relative path the
# shipped manifest names. That is the strong form: it proves the ACTUAL
# configured path is what gets scanned. A synthetic manifest re-pointed at a
# convenient temp path would only prove "the matcher works".
mk_root() {
  local root="$1" mode="$2"
  mkdir -p "${root}/scripts/guards/simple"
  case "$mode" in
    real)
      cp "$REAL_MANIFEST" "${root}/${MANIFEST_REL}"
      mkdir -p "${root}/crates/mh-service/src"
      cp -r "${PRISTINE}/media" "${root}/crates/mh-service/src/media"
      ;;
    custom|none) : ;;
  esac
}

# Run the guard against a root. Sets the globals OUT (combined stdout+stderr)
# and RC (exit code).
#
# Deliberately NOT invoked via command substitution. That runs the function in
# a SUBSHELL, so an `RC=$?` inside it never reaches the caller and every
# exit-code assertion silently reads a stale 0 — the assertions would all pass
# while testing nothing. Assign to globals and read them after the call.
RC=0
OUT=""
run_guard() {
  local root="$1"; shift
  set +e
  OUT="$("$DT_GUARD" media-telemetry-deny --root "$root" "$@" 2>&1)"
  RC=$?
  set -e
}

# Show captured guard output INDENTED.
#
# The indent is load-bearing, not cosmetic. `tee_collect_statuses` votes on
# any line matching `^STATUS=` anywhere in layer-3 stdout, and this suite's
# whole job is driving FAIL branches — so its captured output contains
# `STATUS=FAIL REASON=media-telemetry-deny-…` lines. Echoed at column zero, a
# PASSING self-test would vote FAIL into layer 3: a red pipeline whose cause is
# invisible from both ends. Same control as validate-frame-vectors.test.sh.
show() { printf '%s\n' "$1" | sed 's/^/      /' | head -6; }

# Extract the single STATUS line's REASON token.
reason_of() { printf '%s\n' "$1" | sed -n 's/^STATUS=[A-Z-]* REASON=\(.*\)$/\1/p'; }

# ---------------------------------------------------------------------------
# A. FIRES — on the REAL configured path, one plant per denied family
# ---------------------------------------------------------------------------
# The fixture catalog in media_telemetry_deny_e2e.rs proves the MATCHER works.
# These prove the CONFIGURED PATH is what gets scanned: real manifest, real
# tree, real relative location, plant, expect red.
fires_case() {
  local label="$1" plant="$2"
  local root="${WORK}/fires_${label}"
  mk_root "$root" real
  printf '\n%s\n' "$plant" >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  local out="$OUT"
  assert_exit "fires:${label}:rc" 1 "$RC"
  assert_status "fires:${label}:token" "REASON=media-telemetry-deny-macro-in-media-path" "$out"
  assert_status "fires:${label}:violation-line" "VIOLATION: [media-telemetry-deny-macro-in-media-path]" "$out"
}

fires_case "metrics"        'pub fn planted_a() { counter!("m").increment(1); }'
fires_case "metrics-describe" 'pub fn planted_b() { describe_gauge!("m", "d"); }'
fires_case "tracing-level"  'pub fn planted_c() { tracing::info!("x"); }'
fires_case "log-crate"      'pub fn planted_d() { log::error!("x"); }'
fires_case "event"          'pub fn planted_e() { event!(Level::INFO, "x"); }'
fires_case "span-bare"      'pub fn planted_f() { span!("x"); }'
fires_case "span-suffixed"  'pub fn planted_g() { debug_span!("x"); }'
fires_case "print"          'pub fn planted_h() { println!("x"); }'
fires_case "dbg"            'pub fn planted_i() { dbg!(1); }'
fires_case "instrument"     '#[tracing::instrument(skip_all)]
pub fn planted_j() {}'

# The import deny is its own token.
{
  root="${WORK}/fires_import"
  mk_root "$root" real
  printf '\nuse tracing::info as note;\n' >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  out="$OUT"
  assert_exit "fires:import:rc" 1 "$RC"
  assert_status "fires:import:token" "REASON=media-telemetry-deny-telemetry-crate-import" "$out"
}

# ---------------------------------------------------------------------------
# B. APPLIES — the real tree, unmodified, must be GREEN
# ---------------------------------------------------------------------------
{
  root="${WORK}/clean_real"
  mk_root "$root" real
  run_guard "$root"
  out="$OUT"
  assert_exit "applies:real-tree-clean:rc" 0 "$RC"
  assert_status "applies:real-tree-clean:ok" "STATUS=OK REASON=media-telemetry-deny-clean-" "$out"
  # The allow-list and the comment negatives are BOTH exercised by this case:
  # the real tree calls .increment/.set/.record on cached handles AND
  # documents its own ban in prose. If either half regressed, this reds.
  assert_absent "applies:real-tree-clean:no-violation" "VIOLATION:" "$out"
  [[ "$RC" -ne 0 ]] && show "$out"
}

# The guard must also pass through its WRAPPER against the real repo.
{
  set +e
  wrapper_out="$("${REPO_ROOT}/scripts/guards/simple/media-telemetry-deny.sh" 2>&1)"
  wrapper_rc=$?
  set -e
  assert_exit "applies:wrapper:rc" 0 "$wrapper_rc"
  assert_status "applies:wrapper:ok" "STATUS=OK REASON=media-telemetry-deny-clean-" "$wrapper_out"
  [[ "$wrapper_rc" -ne 0 ]] && show "$wrapper_out"
}

# ---------------------------------------------------------------------------
# C. SCOPE BOUNDARY — the deny must NOT leak outside the configured list
# ---------------------------------------------------------------------------
# The direction nobody tests. Every case above asks "does it fire when it
# should"; this asks "does it stay quiet when it should". Without it, a
# regression widening the walk root from the resolved directory to the crate
# root — or the repo root — passes this ENTIRE suite, because every fires case
# still fires and every scope case still fails correctly. It would surface
# only as a mystery red on an unrelated sibling in someone else's diff.
#
# TWO siblings, not one: one inside `crates/mh-service/src/` catches a widening
# to the crate root; one outside the crate catches a widening to the repo root.
# The planted text is IDENTICAL in both, so location is the only variable.
{
  root="${WORK}/boundary"
  mk_root "$root" real
  leak='pub fn sibling_leak() { counter!("mh_media_frames_forwarded_total").increment(1); println!("x"); }'
  mkdir -p "${root}/crates/mh-service/src/session" "${root}/crates/other-crate/src"
  printf '%s\n' "$leak" > "${root}/crates/mh-service/src/session/leak.rs"
  printf '%s\n' "$leak" > "${root}/crates/other-crate/src/leak.rs"
  run_guard "$root"
  out="$OUT"
  assert_exit "boundary:stays-green:rc" 0 "$RC"
  # Exit 0 alone is NOT sufficient: a guard redding for a different reason, or
  # staying quiet for the wrong reason, satisfies an exit-code-only assertion.
  # Assert the ABSENCE of any finding line.
  assert_absent "boundary:no-violation-line" "VIOLATION:" "$out"
  assert_absent "boundary:no-error-line" "ERROR:" "$out"
  assert_status "boundary:still-scanned-the-real-scope" "${REAL_RS_COUNT} .rs files" "$out"
  [[ "$RC" -ne 0 ]] && show "$out"
}

# ---------------------------------------------------------------------------
# D. SCOPE LIVENESS — every failure token, and their DISTINCTNESS
# ---------------------------------------------------------------------------
# These are the "alive, never applied" guard. A rename must not silently
# disarm the control.
#
# THESE CASES ARE NOT PARANOIA, AND THE PRECEDENT IS IN THIS REPOSITORY.
# A guard silently disarmed by a directory rename reports clean forever, and
# reads as coverage while covering nothing. That has already happened here:
# `crates/dt-guard/src/env_config.rs:23-27` records the env-config guard
# warn-skipping `mc-service` and `mh-service` while emitting
#   STATUS=OK REASON=env-config-clean-4-services
# a clean verdict carrying a confident count that INCLUDED the two services it
# had not checked. The WARNING went to stderr; the STATUS line said clean;
# nobody noticed until someone went looking.
#
# So the assertions below check the SPECIFIC token and a non-zero exit, never
# merely "did not report OK" — a test that accepts any failure cannot tell a
# working control from a broken one. Anyone tempted to thin this section
# should read that entry first.
write_manifest() { printf 'denied_directories:\n%s\n' "$1" > "${2}/${MANIFEST_REL}"; }

# (d1) configured directory does not exist.
{
  root="${WORK}/scope_missing"
  mk_root "$root" custom
  write_manifest '  - crates/mh-service/src/media/' "$root"
  run_guard "$root"
  out="$OUT"
  assert_exit "scope:missing:rc" 1 "$RC"
  assert_status "scope:missing:token" "STATUS=FAIL REASON=media-telemetry-deny-scope-directory-missing" "$out"
  assert_status "scope:missing:diff-defect" "THIS IS A DIFF DEFECT, not a machine fault" "$out"
  MISSING_TOKEN="$(reason_of "$out")"
}

# (d2) configured directory exists but holds zero .rs files.
{
  root="${WORK}/scope_empty"
  mk_root "$root" custom
  write_manifest '  - crates/mh-service/src/media/' "$root"
  mkdir -p "${root}/crates/mh-service/src/media"
  printf 'not rust\n' > "${root}/crates/mh-service/src/media/README.txt"
  run_guard "$root"
  out="$OUT"
  assert_exit "scope:empty:rc" 1 "$RC"
  assert_status "scope:empty:token" "STATUS=FAIL REASON=media-telemetry-deny-scope-directory-empty" "$out"
  assert_status "scope:empty:diff-defect" "THIS IS A DIFF DEFECT, not a machine fault" "$out"
  EMPTY_TOKEN="$(reason_of "$out")"
}

# (d2c) configured directory exists and is LITERALLY EMPTY (zero entries).
# Distinct from (d2), which holds a non-.rs file. Both are one equivalence
# class -- zero `.rs` files -- and must therefore emit the SAME token; a
# different token for empty-vs-non-rs would itself be a smell, since the
# operator's next action is identical. The case exists because "empty
# directory" and "exists but holds no Rust source" are different filesystem
# states reaching the same code path, and only one of them was covered at any
# level before 2026-09-08.
{
  root="${WORK}/scope_truly_empty"
  mk_root "$root" custom
  write_manifest '  - crates/mh-service/src/media/' "$root"
  mkdir -p "${root}/crates/mh-service/src/media"
  run_guard "$root"
  out="$OUT"
  assert_exit "scope:truly-empty:rc" 1 "$RC"
  assert_status "scope:truly-empty:token" "STATUS=FAIL REASON=media-telemetry-deny-scope-directory-empty" "$out"
  assert_status "scope:truly-empty:diff-defect" "THIS IS A DIFF DEFECT, not a machine fault" "$out"
  TRULY_EMPTY_TOKEN="$(reason_of "$out")"
}

# (d2d) The two zero-.rs states are ONE equivalence class and must not have
# forked into two tokens. Asserted rather than assumed: a future walk that
# distinguished them would send an operator hunting a difference that has no
# bearing on the fix.
if [[ "$EMPTY_TOKEN" == "$TRULY_EMPTY_TOKEN" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[scope:empty-class-forked] no-.rs-file gave '${EMPTY_TOKEN}' but empty-dir gave '${TRULY_EMPTY_TOKEN}'; they are one equivalence class")
fi

# (d3) The two tokens must be DISTINGUISHABLE by a reader of the STATUS line.
# Asserting "both failed" would pass against a single overloaded token, which
# is the `gsa_sync::CANON_MISSING_RULE_ID` defect this design avoids.
if [[ "$MISSING_TOKEN" != "$EMPTY_TOKEN" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[scope:tokens-distinct] missing and empty produced the same token '${MISSING_TOKEN}'")
fi

# (d4) manifest present but configuring zero directories — a one-line total
# disarm that the per-directory tokens cannot see.
{
  root="${WORK}/scope_none"
  mk_root "$root" custom
  printf 'denied_directories: []\n' > "${root}/${MANIFEST_REL}"
  run_guard "$root"
  out="$OUT"
  assert_exit "scope:none:rc" 1 "$RC"
  assert_status "scope:none:token" "REASON=media-telemetry-deny-scope-no-directories-configured" "$out"
  assert_status "scope:none:diff-defect" "THIS IS A DIFF DEFECT, not a machine fault" "$out"
}

# (d5) configured directory escapes the repo root via a symlink. Must NOT
# report as the benign "missing" case: that row's remediation is "restore the
# directory or update the manifest path", which applied here walks the
# operator into re-pointing the manifest at the symlink's target and COMPLETES
# the evasion.
{
  root="${WORK}/scope_escape"
  outside="${WORK}/outside_tree"
  mkdir -p "$outside" && printf 'pub fn x() {}\n' > "${outside}/x.rs"
  mk_root "$root" custom
  write_manifest '  - crates/mh-service/src/media/' "$root"
  mkdir -p "${root}/crates/mh-service/src"
  ln -s "$outside" "${root}/crates/mh-service/src/media"
  run_guard "$root"
  out="$OUT"
  assert_exit "scope:escape:rc" 1 "$RC"
  assert_status "scope:escape:token" "REASON=media-telemetry-deny-scope-directory-escapes-root" "$out"
  assert_absent "scope:escape:not-missing" "REASON=media-telemetry-deny-scope-directory-missing" "$out"
  assert_status "scope:escape:diff-defect" "THIS IS A DIFF DEFECT, not a machine fault" "$out"
}

# (d6) manifest missing entirely, and (d7) manifest that will not deserialize.
{
  root="${WORK}/manifest_missing"
  mk_root "$root" none
  mkdir -p "${root}/scripts/guards/simple"
  run_guard "$root"
  out="$OUT"
  assert_exit "manifest:missing:rc" 1 "$RC"
  assert_status "manifest:missing:token" "REASON=media-telemetry-deny-manifest-missing" "$out"
  # The prefix-collision pin: `ERROR: PRECONDITION` means the ENVIRONMENT for
  # `release-build-profile-*` and `env-config-*`, and a DIFF DEFECT here. That
  # inversion is held by hand-maintained prose in docs/runbooks/devloop-validation.md.
  # Pinning the string the prose is ABOUT is cheaper than another copy and
  # cannot rot silently. (Deliberately no count of the prose sites here: a
  # hand-copied tally inside a comment that justifies deleting a hand-copied
  # tally is the same defect one layer up. @operations OPS-3.)
  assert_status "manifest:missing:diff-defect" "THIS IS A DIFF DEFECT, not a machine fault" "$out"
}
{
  root="${WORK}/manifest_bad"
  mk_root "$root" custom
  printf 'denied_directories: "not a list"\nunknown_key: 1\n' > "${root}/${MANIFEST_REL}"
  run_guard "$root"
  out="$OUT"
  assert_exit "manifest:bad:rc" 1 "$RC"
  assert_status "manifest:bad:token" "REASON=media-telemetry-deny-manifest-unparseable" "$out"
  # Sixth of the SEVEN `Rule::is_content() == false` tokens. `unparseable-use`
  # is the seventh and is deliberately NOT pinned here: it routes through
  # `print_finding_line`, not `print_scope_failure_line`, so it carries the
  # `ERROR: PRECONDITION` prefix on an ordinary finding line and no banner.
  # The runbook prose was corrected to say so rather than the emitter being
  # changed -- emitted guard output is infrastructure machinery. (@operations
  # OPS-3.)
  assert_status "manifest:bad:diff-defect" "THIS IS A DIFF DEFECT, not a machine fault" "$out"
}

# ---------------------------------------------------------------------------
# E. REASON PRECEDENCE — with TWO conditions live at once
# ---------------------------------------------------------------------------
# A precedence asserted only in prose is a precedence the first real
# multi-token run gets to disagree with. Two configured directories, one
# missing and one empty: `missing` must win the STATUS line.
#
# Asserting only that the winner is PRESENT would pass an implementation
# emitting several `STATUS=` lines — which would feed `tee_collect_statuses`
# multiple votes. So assert the loser is ABSENT from the STATUS line too, and
# that exactly one STATUS line is emitted.
{
  root="${WORK}/precedence"
  mk_root "$root" custom
  printf 'denied_directories:\n  - crates/a/gone/\n  - crates/b/empty/\n' > "${root}/${MANIFEST_REL}"
  mkdir -p "${root}/crates/b/empty"
  printf 'x\n' > "${root}/crates/b/empty/README.txt"
  run_guard "$root"
  out="$OUT"
  assert_exit "precedence:rc" 1 "$RC"
  status_lines="$(printf '%s\n' "$out" | grep -c '^STATUS=' || true)"
  assert_exit "precedence:exactly-one-status-line" 1 "$status_lines"
  status_only="$(printf '%s\n' "$out" | grep '^STATUS=' || true)"
  assert_status "precedence:winner" "REASON=media-telemetry-deny-scope-directory-missing" "$status_only"
  assert_absent "precedence:loser-absent-from-status" "scope-directory-empty" "$status_only"
  # Both conditions must still be REPORTED in the body — a precedence that
  # hides the co-firing condition trades one blindness for another.
  assert_status "precedence:loser-still-reported" "media-telemetry-deny-scope-directory-empty" "$out"
}

# ---------------------------------------------------------------------------
# F. NO SUPPRESSION
# ---------------------------------------------------------------------------
{
  root="${WORK}/no_suppression"
  mk_root "$root" real
  printf '\npub fn planted() { counter!("m").increment(1); } // guard:ignore(a sufficiently long reason)\n' \
    >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  out="$OUT"
  assert_exit "suppression:ignored-annotation-still-reds" 1 "$RC"
  assert_status "suppression:token" "REASON=media-telemetry-deny-macro-in-media-path" "$out"
}

# ---------------------------------------------------------------------------
# G. OUTPUT SHAPE
# ---------------------------------------------------------------------------
# (g1) The guard must NEVER echo the matched macro's ARGUMENT text.
# The identifiers §11 keeps out of logs live in the argument list, so echoing
# them moves the leak into CI output and its retention window rather than
# closing it. Asserted on stdout, stderr AND --explain — separately, never
# inferred from the hit count.
{
  root="${WORK}/sentinel"
  mk_root "$root" real
  printf '\npub fn planted() { info!(stream = "%s", bytes = 1200); }\n' "$SENTINEL" \
    >> "${root}/crates/mh-service/src/media/forward.rs"

  set +e
  s_stdout="$("$DT_GUARD" media-telemetry-deny --root "$root" 2>/dev/null)"
  s_stderr="$("$DT_GUARD" media-telemetry-deny --root "$root" 2>&1 >/dev/null)"
  s_explain="$("$DT_GUARD" media-telemetry-deny --root "$root" --explain 2>&1)"
  set -e
  # The plant must actually fire, or absence proves nothing.
  assert_status "sentinel:fired" "REASON=media-telemetry-deny-macro-in-media-path" "$s_stdout"
  assert_status "sentinel:explain-emitted" "EXPLAIN:" "$s_explain"
  assert_absent "sentinel:absent-stdout" "$SENTINEL" "$s_stdout"
  assert_absent "sentinel:absent-stderr" "$SENTINEL" "$s_stderr"
  assert_absent "sentinel:absent-explain" "$SENTINEL" "$s_explain"
  # `SecretFinding` has no `matched=` field; assert the wire form too, since a
  # future switch to `print_finding` would reintroduce the span.
  assert_absent "sentinel:explain-has-no-matched-field" 'matched="' "$s_explain"
}

# (g2) One PHYSICAL line per record. `run-guards.sh` surfaces guard output via
# `grep -E "(VIOLATION|ERROR|WARN)" | head -5`, so a record wrapped for
# readability loses its continuation lines silently or burns the five-line cap
# on one finding. Invisible from inside the module — which is how it regresses
# the first time someone makes a message friendlier.
{
  root="${WORK}/one_line"
  mk_root "$root" real
  printf '\npub fn planted() { println!("x"); }\n' >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  out="$OUT"
  # Every VIOLATION/ERROR record starts a line; count records vs. lines that
  # look like continuations (a continuation would not start with a known
  # prefix and would follow one).
  bad="$(printf '%s\n' "$out" \
        | awk '/^(VIOLATION|ERROR): / {rec=1; next} rec && !/^(VIOLATION|ERROR): |^STATUS=|^SCOPE: |^EXPLAIN: |^$/ {print; rec=0}' \
        | head -3)"
  if [[ -z "$bad" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[output:one-physical-line] a VIOLATION/ERROR record spilled onto a continuation line")
  fi
}

# (g3) Content tokens carry the `-<n>-of-<m>-findings` suffix so truncation is
# visible from the STATUS line alone; scope tokens stay bare.
{
  root="${WORK}/findings_suffix"
  mk_root "$root" real
  printf '\npub fn planted() { println!("a"); dbg!(1); info!("c"); }\n' \
    >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  out="$OUT"
  assert_status "output:findings-suffix" "-findings" "$(printf '%s\n' "$out" | grep '^STATUS=' || true)"
}

# (g4) The SCOPE line reports what was actually examined.
{
  root="${WORK}/scope_line"
  mk_root "$root" real
  run_guard "$root"
  out="$OUT"
  assert_status "output:scope-line" "SCOPE: 1 configured directory, ${REAL_RS_COUNT} .rs files" "$out"
}

# ---------------------------------------------------------------------------
# G5. FILTER SURVIVAL — every record class must reach an operator
# ---------------------------------------------------------------------------
# `run-guards.sh` runs guards non-verbose and re-emits ONLY lines matching
#   grep -E "(VIOLATION|violation|ERROR|error|WARN)"
# A record that is emitted but carries none of those substrings is captured and
# DISCARDED. Asserting a line is emitted proves nothing about whether anyone
# sees it — and that gap is exactly what let `ERROR: MIXED_CONDITIONS:` be
# ruled in, agreed by two reviewers, and then ship missing.
#
# So every record class is piped through the REAL filter and asserted to
# survive. The cases are derived from `Rule::ORDER` (via the token list below,
# which `rule_order_contains_every_variant_exactly_once` pins on the Rust side)
# rather than hand-listed per record type, so a new record class cannot skip
# the check by being forgotten here.
REAL_FILTER='(VIOLATION|violation|ERROR|error|WARN)'

# $1=label $2=text that must survive the filter
assert_survives_filter() {
  local label="$1" text="$2"
  if printf '%s\n' "$text" | grep -qE "$REAL_FILTER"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] record is DROPPED by run-guards.sh's re-emission filter — emitted but never seen")
  fi
}

# (g5a) A content record (VIOLATION arm).
{
  root="${WORK}/filter_content"
  mk_root "$root" real
  printf '\npub fn planted() { println!("x"); }\n' >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    assert_survives_filter "filter:content-record" "$line"
  done < <(printf '%s\n' "$OUT" | grep -E '^(VIOLATION|ERROR)' || true)
}

# (g5b) A scope record (ERROR: PRECONDITION arm).
{
  root="${WORK}/filter_scope"
  mk_root "$root" custom
  write_manifest '  - crates/mh-service/src/media/' "$root"
  run_guard "$root"
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    assert_survives_filter "filter:scope-record" "$line"
  done < <(printf '%s\n' "$OUT" | grep -E '^(VIOLATION|ERROR)' || true)
}

# (g5c) The MIXED_CONDITIONS banner — the record this test class exists for.
# Two rule classes live at once: an unparseable `use` (tier C) wins the STATUS
# line while content findings sit unshown, which the `-<n>-of-<m>-findings`
# suffix cannot report because it is gated on `winner.is_content()`.
{
  root="${WORK}/filter_mixed"
  mk_root "$root" real
  printf '\nuse {;\npub fn planted() { println!("x"); tracing::info!("y"); }\n' \
    >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  assert_exit "filter:mixed:rc" 1 "$RC"
  assert_status "filter:mixed:banner-emitted" "ERROR: MIXED_CONDITIONS:" "$OUT"
  banner="$(printf '%s\n' "$OUT" | grep '^ERROR: MIXED_CONDITIONS:' || true)"
  assert_survives_filter "filter:mixed:banner-survives" "$banner"
  # It must print FIRST, ahead of every other record — a banner buried at line
  # six is dropped by the same `head -5` cap it exists to warn about.
  first_record="$(printf '%s\n' "$OUT" | grep -E '^(VIOLATION|ERROR)' | head -1 || true)"
  assert_status "filter:mixed:printed-first" "MIXED_CONDITIONS" "$first_record"
  # And it must name the winner and a non-zero co-firing count.
  assert_status "filter:mixed:names-winner" "winner=media-telemetry-deny-unparseable-use" "$banner"
}

# (g5d) A SINGLE-class run must NOT emit the banner — otherwise it becomes
# noise on every run and gets ignored, which is the same end state as absent.
{
  root="${WORK}/filter_single"
  mk_root "$root" real
  printf '\npub fn planted() { println!("x"); }\n' >> "${root}/crates/mh-service/src/media/forward.rs"
  run_guard "$root"
  assert_absent "filter:single-class:no-banner" "MIXED_CONDITIONS" "$OUT"
}

# (g5e) The `SCOPE:` line is DROPPED by the real filter — asserted, because
# `emit_scope`'s doc must not claim pipeline visibility it does not have.
# This is a characterisation test of a known gap, not an endorsement: see
# docs/TODO.md. Widening the filter touches two other guards and is out of
# scope here.
{
  root="${WORK}/filter_scope_line"
  mk_root "$root" real
  run_guard "$root"
  scope_line="$(printf '%s\n' "$OUT" | grep '^SCOPE: ' || true)"
  assert_status "filter:scope-line-emitted" "SCOPE: " "$scope_line"
  if printf '%s\n' "$scope_line" | grep -qE "$REAL_FILTER"; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[filter:scope-line-known-gap] SCOPE: now survives the filter — good news, but emit_scope's doc-comment and the docs/TODO.md entry both say it does not; update them")
  else
    PASS=$((PASS + 1))
  fi
}

# ---------------------------------------------------------------------------
# H. NO BARE `^STATUS=` MAY ESCAPE THIS SUITE
# ---------------------------------------------------------------------------
# `tee_collect_statuses` votes on ANY line matching `^STATUS=` anywhere in
# layer-3 stdout. This suite's whole job is driving FAIL branches, so its
# captured output is full of `STATUS=FAIL` lines. If one reached column zero, a
# PASSING self-test would vote FAIL into layer 3 — a red pipeline whose cause is
# invisible from both ends. Every `show` call indents; this asserts it.
{
  leaked="$(printf '%s\n' "${FAILURES[@]:-}" | grep -c '^STATUS=' || true)"
  assert_exit "harness:no-bare-status-in-failures" 0 "$leaked"
}

# The case count, pinned.
#
# Without this the harness has NO FLOOR: `report_results` exits 0 on
# "87 passed, 0 failed" exactly as happily as on the full set, so silently
# dropping a case is invisible from both ends. A suite whose entire subject is
# "a control that reads as coverage while covering nothing" must not itself be
# able to shrink quietly — and `docs/runbooks/devloop-validation.md` now tells
# an operator to read this suite's pass count, which is only meaningful if
# something floors it.
#
# `PASS + FAIL` is the right tally rather than `PASS`: the (d2d) equivalence
# check and (d3) token-distinctness check increment the counters directly
# without going through an `assert_` helper, and they still count as cases.
#
# Shape and rationale follow `scripts/guards/validate-frame-vectors.test.sh`,
# which is the in-repo SSoT precedent. When you legitimately add or remove a
# case, change this number in the same commit; this is the ONLY home for it.
EXPECTED_CASES=92
if [[ $((PASS + FAIL)) -ne "$EXPECTED_CASES" ]]; then
  echo "FAIL: ran $((PASS + FAIL)) cases, expected ${EXPECTED_CASES}. A case was added or dropped;" \
       "update EXPECTED_CASES deliberately rather than letting the tally float."
  exit 1
fi

report_results "$0"
