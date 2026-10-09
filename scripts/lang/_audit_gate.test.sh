#!/usr/bin/env bash
# _audit_gate.test.sh — hermetic tests for the dep-change gate (_audit_gate.sh), the
# heart of the ADR-0033 §3 reclassification (task #47, §I item 5). A wrong gate = silent
# denial of scan coverage, so this covers the full tri-state matrix + force-run + both
# langs + a wrapper-level "scanner not invoked on a proven skip" proof, and (Part 5) which
# Cargo graphs the rust wrapper audits and how it reports each.
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
# * crosses / — the workspace-EXCLUDED nested fuzz manifest still MATCHES, so the wrapper
# runs and audits that fuzz graph (Part 5). See _changed_helpers.test.sh for the semantics.
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
# Shared hermetic wrapper repo for Parts 3-5: a throwaway git repo holding
# lang/rust/audit.sh and every helper it sources, a small Cargo tree, and a PATH-stubbed
# cargo that appends `<cwd>|<argv>` to <dir>/cargo_calls. Add any new helper audit.sh
# sources HERE, once — a missing helper makes the wrapper abort early and look like a result.
#
# No real cargo runs (Layer 3 invokes cargo zero times — docs/runbooks/devloop-validation.md).
# The stub fakes every subcommand the wrapper uses and exits 97 on anything else:
#   locate-project   answers from <dir>/stub_locate (`<manifest>\t<root path>` or
#                    `<manifest>\tFAIL`); an UNMAPPED manifest exits 97, so a new fixture
#                    manifest cannot silently map anywhere. The mapping is written by hand,
#                    so Part 5 tests what the WRAPPER does with it (dedup, `.` vs relative
#                    roots, failure propagation, tracked/untracked routing) — never cargo's
#                    membership semantics. Those run on the real tree in Layer 6 every
#                    time, behind the root-discovered positive control (Lead ruling).
#   generate-lockfile writes a fresh lock, or fails for a manifest in <dir>/stub_resolve_fail;
#                    with <dir>/stub_abort it first deletes the wrapper's backup dir (so the
#                    restore's `cp` fails: a set -e abort), with <dir>/stub_sigterm it then
#                    SIGTERMs the wrapper.
#   audit -f <lock>  fails for a lock listed in <dir>/stub_audit_fail.
#
# The Cargo tree: root workspace with members m, c and independent roots fz, fd; root
# lock tracked once a part commits, fz/fd locks untracked. m maps to `<dir>/m/../Cargo.toml`
# so the canonical-path dedup is exercised.
# -----------------------------------------------------------------------------
make_wrapper_repo() {
  local dir="$1"
  (
    cd "$dir"
    git init -q; git config user.email t@t; git config user.name t
    mkdir -p scripts/lang/rust stubbin tmp m c fz fd
    cp "${REPO_ROOT}/scripts/lang/_common.sh" scripts/lang/_common.sh
    cp "${REPO_ROOT}/scripts/lang/_changed_helpers.sh" scripts/lang/_changed_helpers.sh
    cp "${REPO_ROOT}/scripts/lang/_get_base_ref.sh" scripts/lang/_get_base_ref.sh
    cp "${REPO_ROOT}/scripts/lang/_audit_gate.sh" scripts/lang/_audit_gate.sh
    cp "${REPO_ROOT}/scripts/lang/_audit_suppressions_lib.sh" scripts/lang/_audit_suppressions_lib.sh
    cp "${REPO_ROOT}/scripts/lang/rust/audit.sh" scripts/lang/rust/audit.sh
    for m in Cargo.toml m/Cargo.toml c/Cargo.toml fz/Cargo.toml fd/Cargo.toml; do
      printf '# fixture manifest\n' > "$m"
    done
    printf '%s\t%s\n' \
      Cargo.toml "${dir}/Cargo.toml" \
      m/Cargo.toml "${dir}/m/../Cargo.toml" \
      c/Cargo.toml "${dir}/Cargo.toml" \
      fz/Cargo.toml "${dir}/fz/Cargo.toml" \
      fd/Cargo.toml "${dir}/fd/Cargo.toml" > stub_locate
    echo "seed" > Cargo.lock
    printf 'stub_*\ncargo_calls\nstubbin/\ntmp/\n' > .git/info/exclude
    cat > stubbin/cargo <<STUB
#!/usr/bin/env bash
d=$(printf '%q' "$dir")
printf '%s|%s\\n' "\$PWD" "\$*" >> "\$d/cargo_calls"
case "\$1" in
  locate-project)
    [[ \$# -eq 7 && "\$2 \$3 \$5 \$6 \$7" == "--workspace --manifest-path --message-format plain --offline" ]] \\
      || { echo "stub: unexpected: \$*" >&2; exit 97; }
    root="\$(awk -F'\\t' -v m="\$4" '\$1 == m { print \$2; f = 1 } END { exit !f }' "\$d/stub_locate")" \\
      || { echo "stub: no locate mapping for \$4" >&2; exit 97; }
    [[ "\$root" == FAIL ]] && { echo "error: current package believes it's in a workspace when it's not" >&2; exit 101; }
    echo "\$root" ;;
  generate-lockfile)
    [[ \$# -eq 3 && "\$2" == --manifest-path ]] || { echo "stub: unexpected: \$*" >&2; exit 97; }
    grep -qxF -- "\$3" "\$d/stub_resolve_fail" 2>/dev/null && { echo "stub: cannot resolve \$3" >&2; exit 101; }
    [[ -e "\$d/stub_abort" ]] && rm -rf "\$TMPDIR"/tmp.*
    printf 'fresh\\n' > "\$(dirname "\$3")/Cargo.lock"
    [[ -e "\$d/stub_sigterm" ]] && kill -TERM "\$PPID"
    exit 0 ;;
  audit)
    [[ \$# -eq 3 && "\$2" == -f ]] || { echo "stub: unexpected: \$*" >&2; exit 97; }
    grep -qxF -- "\$3" "\$d/stub_audit_fail" 2>/dev/null && { echo "stub: advisory in \$3"; exit 1; }
    exit 0 ;;
  *) echo "stub: unexpected cargo subcommand: \$*" >&2; exit 97 ;;
esac
STUB
    chmod +x stubbin/cargo
  )
}

# Run the wrapper hermetically in <dir>; sets WRAP_OUT / WRAP_RC. DEVLOOP_TMP is per-repo:
# the default (/tmp/devloop) holds the changed-files cache every concurrent layer run and
# test shares, so a neighbour's cache could flip this run's gate (an intermittent
# "cargo was called on a skip" / missing scan).
run_wrapper() {
  local dir="$1"
  WRAP_OUT="$(cd "$dir" && env -i PATH="${dir}/stubbin:$PATH" HOME="$dir" TMPDIR="${dir}/tmp" \
        DEVLOOP_TMP="${dir}/tmp/devloop" \
        bash scripts/lang/rust/audit.sh 2>&1)"; WRAP_RC=$?
}

# 1 iff <dir>/cargo_calls holds exactly the line "<dir>|<argv>" (cwd = the repo root).
called_with() {
  local dir="$1" argv="$2"
  grep -qxF -- "${dir}|${argv}" "${dir}/cargo_calls" 2>/dev/null && echo 1 || echo 0
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
if [[ -f "${wrap}/cargo_calls" ]]; then
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
  # 1 only if the scanner itself ran over the root lock.
  WRAP_CALLED="$(called_with "$dir" "audit -f Cargo.lock")"
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

# -----------------------------------------------------------------------------
# Part 5 — what the wrapper does with the discovered graphs, and how it reports each
# (fuzz-graph coverage). Discovery answers are the stub's hand-written table (see the
# fixture header); cargo's membership semantics are covered by Layer 6 on the real tree.
# Every case is a dep change (the gate runs) in a committed repo: root Cargo.lock tracked,
# fz/fd locks untracked. scan_case <setup-fn> sets WRAP_OUT / WRAP_RC and leaves the repo
# in $CASE_DIR for call assertions; the caller removes it.
# -----------------------------------------------------------------------------
scan_case() {
  local setup="$1"
  CASE_DIR="$(mktemp -d)"
  make_wrapper_repo "$CASE_DIR"
  (
    cd "$CASE_DIR"
    mkdir -p .cargo; printf '[advisories]\nignore = []\n' > .cargo/audit.toml
    git add -A; git commit -qm seed
    echo "bumped" > Cargo.lock   # dep-manifest change -> gate must RUN
    "$setup"
  )
  run_wrapper "$CASE_DIR"
  # Every Part-5 case is a dep change: a skipped gate would make its assertions vacuous.
  if [[ "$WRAP_OUT" == *SKIPPED-NO-DIFF* ]]; then
    FAIL=$((FAIL+1)); FAILURES+=("[${setup}: gate must RUN] got SKIPPED-NO-DIFF:"$'\n'"${WRAP_OUT}")
  fi
}
case_plain() { :; }

# Five manifests -> three graphs: manifests mapping to the root (one via a non-canonical
# `m/../` path) dedup to `.`, the others become repo-relative roots, each routed by whether
# its lock is tracked; all scans run from the repo root with exactly `-f <lock>`.
scan_case case_plain
assert_status "graphs: clean -> OK" "STATUS=OK REASON=cargo-audit-passed" "$WRAP_OUT"
assert_rc "graphs: clean -> exit 0" 0 "$WRAP_RC"
assert_status "graphs: discovered set is exactly root + fd + fz" "AUDIT_GRAPHS=3 . fd fz" "$WRAP_OUT"
assert_rc "graphs: root audited as-is" 1 "$(called_with "$CASE_DIR" "audit -f Cargo.lock")"
assert_rc "graphs: tracked root lock never regenerated" 0 "$(grep -c 'generate-lockfile --manifest-path Cargo.toml' "${CASE_DIR}/cargo_calls")"
assert_rc "graphs: untracked-lock root fz resolved fresh" 1 "$(called_with "$CASE_DIR" "generate-lockfile --manifest-path fz/Cargo.toml")"
assert_rc "graphs: untracked-lock root fz audited" 1 "$(called_with "$CASE_DIR" "audit -f fz/Cargo.lock")"
assert_rc "graphs: untracked-lock root fd resolved fresh" 1 "$(called_with "$CASE_DIR" "generate-lockfile --manifest-path fd/Cargo.toml")"
assert_rc "graphs: untracked-lock root fd audited" 1 "$(called_with "$CASE_DIR" "audit -f fd/Cargo.lock")"
assert_rc "graphs: manifest mapped to root via m/../ dedups (no m graph)" 0 "$(grep -c 'm/Cargo.lock' "${CASE_DIR}/cargo_calls")"
assert_rc "graphs: second manifest mapped to root dedups (no c graph)" 0 "$(grep -c 'c/Cargo.lock' "${CASE_DIR}/cargo_calls")"
# Suppression parity: every scan from the repo root (where .cargo/audit.toml lives), with
# no argument beyond `-f <lock>` — no line runs elsewhere or carries anything else.
assert_rc "graphs: every audit from the repo root, -f only" 0 \
  "$(grep '|audit' "${CASE_DIR}/cargo_calls" | grep -cvxE "$(printf '%s' "$CASE_DIR" | sed 's/[][\.*^$/]/\\&/g')\|audit -f (Cargo|fd/Cargo|fz/Cargo)\.lock")"
assert_rc "graphs: no fresh lock left behind (fz)" 1 "$([[ ! -e "${CASE_DIR}/fz/Cargo.lock" ]] && echo 1 || echo 0)"
rm -rf "$CASE_DIR"

# An advisory in the FIRST resolved graph: the later graph is still scanned (no
# short-circuit), the failing graph is named, the token is the advisory one.
case_fd_advisory() { echo "fd/Cargo.lock" > stub_audit_fail; }
scan_case case_fd_advisory
assert_status "advisory -> cargo-audit-failed" "STATUS=FAIL REASON=cargo-audit-failed" "$WRAP_OUT"
assert_rc "advisory -> exit 1" 1 "$WRAP_RC"
assert_status "advisory -> failing graph named" "AUDIT_GRAPH=fd/Cargo.lock LOCK=resolved RESULT=audit-failed" "$WRAP_OUT"
assert_rc "advisory -> later graph still audited" 1 "$(called_with "$CASE_DIR" "audit -f fz/Cargo.lock")"
rm -rf "$CASE_DIR"

# A resolve failure is "could not evaluate", never an advisory — and it outranks one.
case_fz_unresolvable() { echo "fz/Cargo.toml" > stub_resolve_fail; echo "Cargo.lock" > stub_audit_fail; }
scan_case case_fz_unresolvable
assert_status "resolve failure -> cargo-lockfile-resolve-failed" "STATUS=FAIL REASON=cargo-lockfile-resolve-failed" "$WRAP_OUT"
assert_rc "resolve failure -> exit 1" 1 "$WRAP_RC"
assert_status "resolve failure -> graph named" "AUDIT_GRAPH=fz/Cargo.lock LOCK=resolved RESULT=resolve-failed" "$WRAP_OUT"
assert_rc "resolve failure -> unresolved lock not audited" 0 "$(called_with "$CASE_DIR" "audit -f fz/Cargo.lock")"
rm -rf "$CASE_DIR"

# A developer's local untracked lock comes back byte-identical: clean, advisory, resolve failure.
for mode in clean advisory unresolvable; do
  case "$mode" in
    clean) want=ok ;; advisory) want=audit-failed ;; unresolvable) want=resolve-failed ;;
  esac
  case_dev_lock() {
    printf 'my local lock\x00\nfrom a crash repro\n' > fz/Cargo.lock
    cp fz/Cargo.lock dev-lock.orig
    case "$mode" in
      advisory) echo "fz/Cargo.lock" > stub_audit_fail ;;
      unresolvable) echo "fz/Cargo.toml" > stub_resolve_fail ;;
    esac
  }
  scan_case case_dev_lock
  assert_rc "dev lock ($mode) -> byte-identical afterwards" 0 \
    "$(cmp -s "${CASE_DIR}/fz/Cargo.lock" "${CASE_DIR}/dev-lock.orig"; echo $?)"
  assert_status "dev lock ($mode) -> backup path printed" "AUDIT_LOCK_BACKUP=" "$WRAP_OUT"
  # The scan really reached fz, so byte-identical is a restore, not an untouched file.
  assert_status "dev lock ($mode) -> fz scanned" "AUDIT_GRAPH=fz/Cargo.lock LOCK=resolved RESULT=${want}" "$WRAP_OUT"
  rm -rf "$CASE_DIR"
done

# Discovery failures: each is its own check, all -> cargo-audit-discovery-failed, and
# nothing is scanned.
assert_discovery_failed() {
  local label="$1" cause="$2"
  assert_status "${label} -> cargo-audit-discovery-failed" "STATUS=FAIL REASON=cargo-audit-discovery-failed" "$WRAP_OUT"
  assert_rc "${label} -> exit 1" 1 "$WRAP_RC"
  assert_status "${label} -> cause named" "$cause" "$WRAP_OUT"
  assert_rc "${label} -> nothing scanned" 0 "$(cat "${CASE_DIR}/cargo_calls" 2>/dev/null | grep -c '|audit')"
}
# A tracked manifest cargo rejects (neither a member nor its own root).
case_orphan() {
  mkdir -p orphan; printf '# fixture manifest\n' > orphan/Cargo.toml
  printf 'orphan/Cargo.toml\tFAIL\n' >> stub_locate
  git add orphan
}
scan_case case_orphan
assert_discovery_failed "orphan manifest" "AUDIT_DISCOVERY: cargo locate-project failed for orphan/Cargo.toml"
rm -rf "$CASE_DIR"
# A workspace root outside the repo.
case_root_outside() {
  local elsewhere; elsewhere="$(mktemp -d)"
  sed -i "s|^fz/Cargo.toml\t.*|fz/Cargo.toml\t${elsewhere}/Cargo.toml|" stub_locate
}
scan_case case_root_outside
assert_discovery_failed "root outside repo" "AUDIT_DISCOVERY: workspace root of fz/Cargo.toml is outside the repo"
rm -rf "$CASE_DIR"
# Only the independent workspaces are discoverable: no tracked manifest maps to the root.
case_root_untracked_manifest() { git rm -q --cached Cargo.toml m/Cargo.toml c/Cargo.toml; }
scan_case case_root_untracked_manifest
assert_discovery_failed "root not discovered" "AUDIT_DISCOVERY: the repo-root workspace was not discovered"
rm -rf "$CASE_DIR"
# The root lock untracked.
case_root_lock_untracked() { git rm -q --cached Cargo.lock; }
scan_case case_root_lock_untracked
assert_discovery_failed "root lock untracked" "AUDIT_DISCOVERY: the repo-root Cargo.lock is not tracked"
rm -rf "$CASE_DIR"
# No tracked manifest at all: its own message, not "root not discovered".
case_no_manifests() { git rm -q --cached Cargo.toml m/Cargo.toml c/Cargo.toml fz/Cargo.toml fd/Cargo.toml; }
scan_case case_no_manifests
assert_discovery_failed "no tracked manifests" "AUDIT_DISCOVERY: no tracked Cargo.toml"
rm -rf "$CASE_DIR"

# Abort paths. (1) A set -e abort mid-scan (the restore's `cp` fails because the backup is
# gone — independent of uid): the cleanup hook runs and the task-#50 STATUS fallback still
# carries the original rc; no later graph is scanned. (2) SIGTERM mid-scan: the developer's
# lock is restored byte-for-byte by the EXIT trap.
case_abort_mid_scan() {
  printf 'my local lock\n' > fd/Cargo.lock
  touch stub_abort
}
scan_case case_abort_mid_scan
assert_status "set -e abort -> STATUS fallback kept" "STATUS=FAIL REASON=wrapper-aborted-early-exit-1" "$WRAP_OUT"
assert_rc "set -e abort -> exit 1" 1 "$WRAP_RC"
assert_status "set -e abort -> backup path printed" "AUDIT_LOCK_BACKUP=" "$WRAP_OUT"
assert_rc "set -e abort -> no later graph scanned" 0 "$(called_with "$CASE_DIR" "audit -f fz/Cargo.lock")"
rm -rf "$CASE_DIR"

# fd is the FIRST resolved graph (sorted), so the signal lands while ITS lock is replaced.
case_sigterm_mid_scan() {
  printf 'my local lock\nfrom a crash repro\n' > fd/Cargo.lock
  cp fd/Cargo.lock dev-lock.orig
  touch stub_sigterm
}
scan_case case_sigterm_mid_scan
assert_rc "SIGTERM -> fd was being resolved" 1 "$(called_with "$CASE_DIR" "generate-lockfile --manifest-path fd/Cargo.toml")"
assert_rc "SIGTERM -> dev lock byte-identical" 0 \
  "$(cmp -s "${CASE_DIR}/fd/Cargo.lock" "${CASE_DIR}/dev-lock.orig"; echo $?)"
assert_status "SIGTERM -> a STATUS=FAIL line" "STATUS=FAIL" "$WRAP_OUT"
assert_rc "SIGTERM -> nonzero exit" 1 "$(( WRAP_RC != 0 ))"
rm -rf "$CASE_DIR"

report_results "scripts/lang/_audit_gate.test.sh"
