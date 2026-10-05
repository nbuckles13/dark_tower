#!/usr/bin/env bash
# _audit_gate.test.sh — hermetic tests for the dep-change gate (_audit_gate.sh), the
# heart of the ADR-0033 §3 reclassification (task #47, §I item 5). A wrong gate = silent
# denial of scan coverage, so this covers the full tri-state matrix + force-run + both
# langs + a wrapper-level "scanner not invoked on a proven skip" proof.
#
# No network: predicate cases drive off an injected changed-files cache; the
# wrapper-level proof uses a throwaway git repo with a PATH-stubbed scanner.
# Wired into scripts/layer3.sh (no *.test.sh auto-runner — an unrun test is untested).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_test_helpers.sh
source "${__here}/_test_helpers.sh"
set +e  # the gate returns non-zero (1=skip, 2=indeterminate) by design; assert, don't abort

REPO_ROOT="$(cd "${__here}/../.." && pwd)"

# -----------------------------------------------------------------------------
# Part 1 — PREDICATE logic (audit_dep_changed_rust / _ts) against an injected cache.
# These determine the 0(run) vs 1(skip) the gate returns when base-ref resolution
# succeeds. Hermetic: synthetic DEVLOOP_TMP cache + env -i, sourcing _audit_gate.sh.
# pred_rc <lang> <cache-content> -> echoes predicate exit code (0=dep-changed, 1=no-dep).
# -----------------------------------------------------------------------------
pred_rc() {
  local lang="$1" content="$2"
  local tmp; tmp="$(mktemp -d)"
  printf '%s\n' "$content" > "${tmp}/changed-files.layer-locality"
  local rc=0
  env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$tmp" DEVLOOP_LAYER=locality \
      bash -c "source '${__here}/_audit_gate.sh'; audit_dep_changed_${lang}" >/dev/null 2>&1 || rc=$?
  rm -rf "$tmp"
  printf '%s\n' "$rc"
}

# rust dep-manifest changes -> 0 (run). NARROWED 2026-08-05: matches ONLY true dep
# manifests (root Cargo.toml/Cargo.lock + crates/*/Cargo.toml), not the old crates/ PREFIX.
assert_rc "rust: Cargo.lock changed -> run"   0 "$(pred_rc rust 'Cargo.lock')"
assert_rc "rust: Cargo.toml changed -> run"   0 "$(pred_rc rust 'Cargo.toml')"
assert_rc "rust: crate Cargo.toml -> run"     0 "$(pred_rc rust 'crates/ac-service/Cargo.toml')"
# * crosses / — the workspace-EXCLUDED nested fuzz manifest still MATCHES (fail-safe: run
# the scan against the unchanged root lock). See _changed_helpers.test.sh for the semantics.
assert_rc "rust: nested/excluded fuzz Cargo.toml -> run (* crosses /)" 0 "$(pred_rc rust 'crates/ac-service/fuzz/Cargo.toml')"
# NARROWED: a crate-SOURCE-only edit now SKIPS (was "run" as a fail-safe over-trigger; the
# over-trigger is gone — a source edit cannot move the resolved dep graph).
assert_rc "rust: crate source only -> skip (narrowed)"   1 "$(pred_rc rust 'crates/ac-service/src/lib.rs')"
# negative glob: shares crates/ prefix but suffix differs -> skip.
assert_rc "rust: crates/ suffix-differing sibling (.bak) -> skip" 1 "$(pred_rc rust 'crates/ac-service/Cargo.toml.bak')"
# rust no dep -> 1 (skip)
assert_rc "rust: docs only -> skip"           1 "$(pred_rc rust 'docs/x.md')"
assert_rc "rust: scripts only -> skip"        1 "$(pred_rc rust 'scripts/foo.sh')"

# ts dep-manifest changes -> 0 (run). NARROWED: root package.json/pnpm-lock.yaml/
# pnpm-workspace.yaml + packages/*/package.json, not the old packages/ PREFIX.
assert_rc "ts: package.json -> run"           0 "$(pred_rc ts 'package.json')"
assert_rc "ts: pnpm-lock.yaml -> run"         0 "$(pred_rc ts 'pnpm-lock.yaml')"
assert_rc "ts: pnpm-workspace.yaml -> run"    0 "$(pred_rc ts 'pnpm-workspace.yaml')"
assert_rc "ts: packages/*/package.json -> run" 0 "$(pred_rc ts 'packages/proto-gen/package.json')"
# NARROWED: a package-SOURCE-only edit now SKIPS (was a fail-safe over-trigger).
assert_rc "ts: packages source only -> skip (narrowed)" 1 "$(pred_rc ts 'packages/proto-gen/src/i.ts')"
# ts no dep -> 1 (skip)
assert_rc "ts: docs only -> skip"             1 "$(pred_rc ts 'docs/x.md')"

# -----------------------------------------------------------------------------
# Part 1b — SSoT-DRIFT GUARD (task narrowing, 2026-08-05). The narrowed predicates couple
# to the workspace layout: root Cargo.toml [workspace].members/exclude (matched by
# crates/*/Cargo.toml) and pnpm-workspace.yaml `packages: - 'packages/*'` (matched by
# packages/*/package.json). A FUTURE workspace member outside crates/* — or a new pnpm
# workspace glob outside packages/* — would silently open a false-SKIP hole: an edit to
# that manifest would NOT trigger the audit. This guard fails LOUDLY on that drift.
#
# git ls-files is the manifest enumerator (SSoT for "which manifests exist"; no TOML/YAML
# parse): every tracked Cargo.toml MUST fire audit_dep_changed_rust and every tracked
# package.json MUST fire audit_dep_changed_ts. Unlike Part 1 (hermetic, injected cache),
# this section deliberately reads the REAL tracked paths; skips gracefully if git is absent.
#
# SAFE-BIAS LIMITATION (@security, non-blocking): this asserts EVERY tracked manifest is
# audit-covered. A tracked manifest OUTSIDE the cargo/pnpm workspace (e.g. a fixture
# docs/examples/package.json or a non-member Cargo.toml) is legitimately NOT audited, so
# this guard would fail LOUDLY on it even though skipping it is correct. That is the safe
# bias (loud + human-in-loop, never a silent SKIP), and there are ZERO such paths today.
# If one is ever added, scope the git ls-files enumeration below to the workspace roots
# (crates/, packages/) OR add a documented exclusion here at that time.
if git -C "$REPO_ROOT" rev-parse --git-dir >/dev/null 2>&1; then
  while IFS= read -r m; do
    [[ -z "$m" ]] && continue
    assert_rc "drift: tracked manifest '$m' covered by rust audit predicate" 0 "$(pred_rc rust "$m")"
  done < <(git -C "$REPO_ROOT" ls-files '*Cargo.toml')
  while IFS= read -r m; do
    [[ -z "$m" ]] && continue
    assert_rc "drift: tracked manifest '$m' covered by ts audit predicate" 0 "$(pred_rc ts "$m")"
  done < <(git -C "$REPO_ROOT" ls-files '*package.json')
else
  echo "# audit-gate-test: git unavailable — SSoT-drift guard skipped (non-fatal)" >&2
fi

# -----------------------------------------------------------------------------
# Part 2 — audit_gate COMPOSITION (tri-state wrapper): force-run + indeterminate.
# gate_rc <env-prefix...> -- runs audit_gate against a stub predicate, echoes rc.
# -----------------------------------------------------------------------------

# FORCE-RUN: DEVLOOP_AUDIT_FORCE_RUN=1 -> 0 (run) regardless of predicate, before any
# base-ref resolution. Use a predicate that would say "skip" to prove force wins.
rc=0
env -i PATH="$PATH" HOME="$HOME" DEVLOOP_AUDIT_FORCE_RUN=1 \
    bash -c "source '${__here}/_audit_gate.sh'; pred_skip(){ return 1; }; audit_gate pred_skip" >/dev/null 2>&1 || rc=$?
assert_rc "force-run overrides skip -> run(0)" 0 "$rc"
# force-run is RUN-ONLY: there is no value/sibling that forces SKIP. Any set value runs.
rc=0
env -i PATH="$PATH" HOME="$HOME" DEVLOOP_AUDIT_FORCE_RUN=anything \
    bash -c "source '${__here}/_audit_gate.sh'; pred_skip(){ return 1; }; audit_gate pred_skip" >/dev/null 2>&1 || rc=$?
assert_rc "force-run any value -> run(0)" 0 "$rc"

# INDETERMINATE: base-ref resolution fails (run in a throwaway NON-git dir with minimal
# env) -> audit_gate returns 2 (fail-closed: run on doubt), regardless of predicate.
nogit="$(mktemp -d)"
rc=0
( cd "$nogit" && env -i PATH="$PATH" HOME="$nogit" \
    bash -c "source '${REPO_ROOT}/scripts/lang/_audit_gate.sh'; pred_skip(){ return 1; }; audit_gate pred_skip" >/dev/null 2>&1 ) || rc=$?
assert_rc "indeterminate base-ref -> run(2) fail-closed" 2 "$rc"
rm -rf "$nogit"

# -----------------------------------------------------------------------------
# Shared hermetic wrapper repo for Parts 3-4: a throwaway git repo holding
# lang/rust/audit.sh and every helper it sources, plus a PATH-stubbed cargo that
# records its argv to <dir>/cargo_was_called. Add any new helper audit.sh sources
# HERE, once — a missing helper makes the wrapper abort early and look like a result.
# -----------------------------------------------------------------------------
make_wrapper_repo() {
  local dir="$1"
  (
    cd "$dir"
    git init -q; git config user.email t@t; git config user.name t
    mkdir -p scripts/lang/rust stubbin
    cp "${REPO_ROOT}/scripts/lang/_common.sh" scripts/lang/_common.sh
    cp "${REPO_ROOT}/scripts/lang/_changed_helpers.sh" scripts/lang/_changed_helpers.sh
    cp "${REPO_ROOT}/scripts/lang/_get_base_ref.sh" scripts/lang/_get_base_ref.sh
    cp "${REPO_ROOT}/scripts/lang/_audit_gate.sh" scripts/lang/_audit_gate.sh
    cp "${REPO_ROOT}/scripts/lang/_audit_suppressions_lib.sh" scripts/lang/_audit_suppressions_lib.sh
    cp "${REPO_ROOT}/scripts/lang/rust/audit.sh" scripts/lang/rust/audit.sh
    printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" > "%s/cargo_was_called"\n' "$dir" > stubbin/cargo
    chmod +x stubbin/cargo
  )
}

# Run the wrapper hermetically in <dir>; sets WRAP_OUT / WRAP_RC.
run_wrapper() {
  local dir="$1"
  WRAP_OUT="$(cd "$dir" && env -i PATH="${dir}/stubbin:$PATH" HOME="$dir" \
        bash scripts/lang/rust/audit.sh 2>&1)"; WRAP_RC=$?
}

# -----------------------------------------------------------------------------
# Part 3 — WRAPPER-level proof: on a PROVEN no-dep change, lang/rust/audit.sh emits
# SKIPPED-NO-DIFF AND does NOT invoke the scanner (cargo).
# -----------------------------------------------------------------------------
wrap="$(mktemp -d)"
make_wrapper_repo "$wrap"
(
  cd "$wrap"
  # Seed a commit, then make a NO-DEP change (a docs file) so the gate proves "no dep".
  mkdir -p docs
  echo "seed" > docs/seed.md
  git add -A; git commit -qm seed
  echo "change" > docs/changed.md  # untracked no-dep change -> gate should SKIP
)
run_wrapper "$wrap"
assert_status "wrapper no-dep -> SKIPPED-NO-DIFF" "STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes" "$WRAP_OUT"
assert_rc "wrapper no-dep -> exit 0" 0 "$WRAP_RC"
if [[ -f "${wrap}/cargo_was_called" ]]; then
  FAIL=$((FAIL+1)); FAILURES+=("[wrapper no-dep -> scanner NOT invoked] cargo was called on a skip")
else
  PASS=$((PASS+1))
fi
rm -rf "$wrap"

# -----------------------------------------------------------------------------
# Part 4 — WRAPPER-level proof: on a dep change, lang/rust/audit.sh RUNS the scanner
# and emits its STATUS whether the generated .cargo/audit.toml ignores something or
# nothing. Regression: an empty `ignore = []` (zero suppressions, a valid manifest
# state) made the SUPPRESSED= grep exit 1 and aborted the wrapper under pipefail
# (wrapper-aborted-early-exit-1). Only grep's no-match (1) is tolerated: an
# unreadable audit.toml (grep 2) must still abort, never be masked.
# -----------------------------------------------------------------------------
# Sets WRAP_OUT / WRAP_RC / WRAP_CALLED for a dep change with the given audit.toml
# body. Mode `unreadable` adds a PATH-stubbed grep that fails with exit 2 (a read
# error) for .cargo/audit.toml only and defers to the real grep otherwise, so the
# failure lands exactly on the SUPPRESSED= line, as any uid (chmod 000 is no barrier
# to root).
run_wrapper_on_dep_change() {
  local ignore_line="$1" mode="${2:-}" dir
  dir="$(mktemp -d)"
  make_wrapper_repo "$dir"
  (
    cd "$dir"
    mkdir -p .cargo
    printf '[advisories]\n%s\n' "$ignore_line" > .cargo/audit.toml
    echo "seed" > Cargo.lock
    git add -A; git commit -qm seed
    echo "bumped" > Cargo.lock  # dep-manifest change -> gate must RUN
    if [[ "$mode" == "unreadable" ]]; then
      real_grep="$(command -v grep)"
      printf '#!/usr/bin/env bash
for a in "$@"; do [[ "$a" == *.cargo/audit.toml ]] && { echo "grep: $a: Permission denied" >&2; exit 2; }; done
exec %q "$@"
' "$real_grep" > stubbin/grep
      chmod +x stubbin/grep
    fi
  )
  run_wrapper "$dir"
  # 1 only if the scanner itself ran: cargo was invoked with exactly `audit`.
  WRAP_CALLED=0; [[ "$(cat "${dir}/cargo_was_called" 2>/dev/null)" == "audit" ]] && WRAP_CALLED=1
  rm -rf "$dir"
}

run_wrapper_on_dep_change 'ignore = []'
assert_status "wrapper dep change, empty ignore -> scanner STATUS" "STATUS=OK REASON=cargo-audit-passed" "$WRAP_OUT"
assert_rc "wrapper dep change, empty ignore -> exit 0" 0 "$WRAP_RC"
assert_rc "wrapper dep change, empty ignore -> scanner invoked" 1 "$WRAP_CALLED"

# Positive control: a non-empty ignore list is surfaced as SUPPRESSED= and still runs.
run_wrapper_on_dep_change 'ignore = ["RUSTSEC-2000-0001"]'
assert_status "wrapper dep change, one ignore -> scanner STATUS" "STATUS=OK REASON=cargo-audit-passed" "$WRAP_OUT"
assert_status "wrapper dep change, one ignore -> SUPPRESSED surfaced" "SUPPRESSED=RUSTSEC-2000-0001" "$WRAP_OUT"
assert_rc "wrapper dep change, one ignore -> scanner invoked" 1 "$WRAP_CALLED"

# Fail-loud: an unreadable audit.toml (grep exit 2) aborts the wrapper — it is NOT
# treated as "no suppressions".
run_wrapper_on_dep_change 'ignore = []' unreadable
assert_rc "wrapper dep change, unreadable audit.toml -> nonzero exit" 1 "$(( WRAP_RC != 0 ))"
assert_rc "wrapper dep change, unreadable audit.toml -> scanner NOT invoked" 0 "$WRAP_CALLED"
assert_status "wrapper dep change, unreadable audit.toml -> STATUS=FAIL" "STATUS=FAIL" "$WRAP_OUT"

report_results "scripts/lang/_audit_gate.test.sh"
