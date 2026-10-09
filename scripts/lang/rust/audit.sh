#!/usr/bin/env bash
# Rust audit: cargo audit over EVERY Cargo workspace in the tree, DEP-CHANGE-GATED
# (ADR-0033 §3 amendment, task #47).
#
# GRAPHS: every git-tracked Cargo.toml is mapped to its workspace root by
# `cargo locate-project --workspace --offline` (cargo's own parser decides membership
# and `exclude`; no second encoding here). Today: the root workspace plus the two
# root-`exclude` fuzz workspaces (crates/{ac-service,media-protocol}/fuzz). A workspace
# whose Cargo.lock is TRACKED is audited as-is, never regenerated. One whose lock is
# UNTRACKED (the fuzz crates: no Dependabot block can serve them, see
# .github/dependabot.yml) is resolved fresh with `cargo generate-lockfile` — the graph
# every unlocked build of it compiles; it runs no build scripts — and that lock is
# audited. A developer's existing local lock is saved first and restored byte-for-byte
# afterwards, also on an abort (the install_wrapper_exit_trap cleanup hook).
#
# Suppression is sourced ONLY from the tracked, generated .cargo/audit.toml (which
# cargo-audit reads NATIVELY from the CURRENT DIRECTORY's .cargo/ — not from the
# lockfile's directory; that is why every scan runs from the repo root, which the
# wrapper cd's to). That derived file is regenerated from audit-suppressions.toml by
# audit-suppressions-check.sh --fix; it is never hand-edited (sync-check enforces this).
#
# IMPORTANT (security finding 1, PRESERVED + EXTENDED): we deliberately do NOT pass
# "$@" through to cargo audit. Threshold/allowlist edits and `--ignore` flags are
# security's domain (ADR-0033 §11). The ONLY sanctioned suppression channel is
# audit-suppressions.toml -> generated .cargo/audit.toml — never an ad-hoc CLI flag,
# never a hand-edited derived file, never a Dependabot alert dismissal. Allowing CLI
# pass-through would let a caller silence advisories at runtime without leaving a
# trace in the tracked manifest. The only argument added is `-f <discovered lockfile>`.
#
# GATE (fail-closed tri-state, _audit_gate.sh): runs the scans if deps changed OR the
# diff is indeterminate; emits SKIPPED-NO-DIFF only on a PROVEN no-dep-change. The gate
# matches ONLY true dep manifests (root Cargo.toml/Cargo.lock + crates/*/Cargo.toml,
# which also matches crates/*/fuzz/Cargo.toml) — a crates/-SOURCE-only edit SKIPS (it
# cannot move a resolved dep graph); an indeterminate diff still RUNS.
# DEVLOOP_AUDIT_FORCE_RUN=1 forces a run (weekly scheduled scan); force-RUN-only.
#
# OUTCOME (one STATUS line; every graph is scanned, no short-circuit):
#   cargo-audit-discovery-failed   the graph set could not be established (nothing scanned),
#                                  or not every discovered graph was scanned (`scanned N of M`:
#                                  the AUDIT_GRAPH lines that DID print are real results — read them)
#   cargo-lockfile-resolve-failed  an untracked graph did not resolve: unevaluated, a REPAIR
#   cargo-audit-failed             cargo-audit failed on some graph: an advisory (REMEDIATE),
#                                  or the tool itself (e.g. advisory-DB fetch; exit 1 too) —
#                                  its output just above the AUDIT_GRAPH line says which
# Each graph prints `AUDIT_GRAPH=<lock> LOCK=tracked|resolved RESULT=ok|audit-failed|resolve-failed`
# to stderr, so the failing graph is named. Triage: docs/runbooks/devloop-validation.md §6.6.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_audit_gate.sh"

# The ONE pending lock restore: "<lock>|<backup or empty>". It is cleared BEFORE the
# restore is attempted, so a failed restore is not retried by the EXIT trap — the
# AUDIT_LOCK_BACKUP= path on stderr is the recovery.
__pending_lock=""
restore_pending_lock() {
  [[ -n "$__pending_lock" ]] || return 0
  local lock="${__pending_lock%%|*}" backup="${__pending_lock#*|}"
  __pending_lock=""
  if [[ -n "$backup" ]]; then
    cp -p "$backup" "$lock"
    rm -rf "$(dirname "$backup")"
  else
    rm -f "$lock"
  fi
}
# task #50: emit STATUS=FAIL if we abort before emitting — and restore a developer's
# lock first, on every exit path (the handler tolerates a failing cleanup).
install_wrapper_exit_trap restore_pending_lock
__repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd -P)"
# Every scan runs from here: cargo-audit reads .cargo/audit.toml from the cwd.
cd "$__repo_root"

gate_rc=0
audit_gate audit_dep_changed_rust || gate_rc=$?
if [[ "$gate_rc" -eq 1 ]]; then
  # Proven no dep-manifest change. Non-dominating skip (SKIPPED-NO-DIFF ranks below
  # OK so a sibling's real OK still wins). NOT N/A, which is reserved for the intentional-gap
  # placeholders ("the verb does not apply"; allow-listed in _dispatch.test.sh). See
  # ADR-0033 §3 amendment + docs/runbooks/devloop-validation.md §6.6.
  emit_status SKIPPED-NO-DIFF no-dep-changes
  exit 0
fi

# Deps changed (0) or indeterminate (>=2): RUN. cargo-audit applies the tracked
# .cargo/audit.toml ignore list NATIVELY; suppressed advisories drop out of its
# failure decision. Surface the configured suppression set for 3am legibility
# (observability): cargo-audit does not report which ignores it hit, so this is the
# tracked ignore list in effect this run, not a per-hit list — but it makes "an
# advisory is being silenced here, here's its id" greppable without re-deriving from
# the manifest. No line is emitted when nothing is configured to suppress.
__cargo_audit_toml="${__repo_root}/.cargo/audit.toml"
if [[ -f "$__cargo_audit_toml" ]]; then
  # grep exit 1 (no match) is a valid state: an empty ignore list, i.e. no
  # suppressions. Any other grep failure (e.g. 2, unreadable file) still aborts
  # the wrapper under `set -o pipefail` — never mask it.
  applied="$({ grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' "$__cargo_audit_toml" || [[ $? -eq 1 ]]; } | sort -u | paste -sd, -)"
  [[ -n "$applied" ]] && echo "SUPPRESSED=${applied}" >&2
fi

# -----------------------------------------------------------------------------
# Discovery: repo-relative workspace roots ("." = the repo root), one per line,
# deduplicated on CANONICAL paths. Any failure names its cause on stderr.
# -----------------------------------------------------------------------------
discover_cargo_workspaces() {
  local manifests m root
  local -a roots=()
  manifests="$(git ls-files -- 'Cargo.toml' '*/Cargo.toml')" \
    || { echo "AUDIT_DISCOVERY: git ls-files failed" >&2; return 1; }
  [[ -n "$manifests" ]] || { echo "AUDIT_DISCOVERY: no tracked Cargo.toml" >&2; return 1; }
  while IFS= read -r m; do
    root="$(cargo locate-project --workspace --manifest-path "$m" --message-format plain --offline </dev/null)" \
      || { echo "AUDIT_DISCOVERY: cargo locate-project failed for ${m}" >&2; return 1; }
    root="$(cd "$(dirname "$root")" && pwd -P)" \
      || { echo "AUDIT_DISCOVERY: workspace root of ${m} is not a directory" >&2; return 1; }
    if [[ "$root" == "$__repo_root" ]]; then
      roots+=(".")
    elif [[ "$root" == "$__repo_root"/* ]]; then
      roots+=("${root#"$__repo_root"/}")
    else
      echo "AUDIT_DISCOVERY: workspace root of ${m} is outside the repo (${root})" >&2
      return 1
    fi
  done <<<"$manifests"
  printf '%s\n' "${roots[@]}" | sort -u
}

# Audit one workspace ("." or a repo-relative dir) and set __result. Called directly,
# never in $(…): a subshell would lose __pending_lock and the EXIT trap. A failing scan
# never aborts, so every graph is scanned. Every cargo call reads /dev/null, never the
# caller's loop input: a tool that read stdin would swallow the remaining graphs.
audit_workspace() {
  local ws="$1" manifest lock result backup=""
  if [[ "$ws" == "." ]]; then manifest="Cargo.toml"; lock="Cargo.lock"
  else manifest="${ws}/Cargo.toml"; lock="${ws}/Cargo.lock"; fi
  if git ls-files --error-unmatch -- "$lock" >/dev/null 2>&1; then
    if cargo audit -f "$lock" </dev/null >&2; then result=ok; else result=audit-failed; fi
    echo "AUDIT_GRAPH=${lock} LOCK=tracked RESULT=${result}" >&2
  else
    # Untracked: resolve fresh into place. A developer's lock (e.g. one reproducing a
    # nightly fuzz crash) is saved and restored byte-for-byte; the backup path is
    # printed so even a SIGKILL mid-scan is recoverable.
    if [[ -f "$lock" ]]; then
      backup="$(mktemp -d)/Cargo.lock"
      cp -p "$lock" "$backup"
      echo "AUDIT_LOCK_BACKUP=${backup} (of ${lock})" >&2
    fi
    __pending_lock="${lock}|${backup}"
    if cargo generate-lockfile --manifest-path "$manifest" </dev/null >&2; then
      if cargo audit -f "$lock" </dev/null >&2; then result=ok; else result=audit-failed; fi
    else
      result=resolve-failed
    fi
    restore_pending_lock
    echo "AUDIT_GRAPH=${lock} LOCK=resolved RESULT=${result}" >&2
  fi
  __result="$result"
}

if ! workspaces="$(discover_cargo_workspaces)"; then
  emit_status FAIL cargo-audit-discovery-failed
  exit 1
fi
# Positive control: the root workspace, with its tracked lock, is always a graph. A
# discovery that lost it (wrong cwd, a broken locate) must not scan the rest as "clean".
if ! grep -qx '\.' <<<"$workspaces"; then
  echo "AUDIT_DISCOVERY: the repo-root workspace was not discovered" >&2
  emit_status FAIL cargo-audit-discovery-failed
  exit 1
fi
if ! git ls-files --error-unmatch -- Cargo.lock >/dev/null 2>&1; then
  echo "AUDIT_DISCOVERY: the repo-root Cargo.lock is not tracked" >&2
  emit_status FAIL cargo-audit-discovery-failed
  exit 1
fi
graphs="$(wc -l <<<"$workspaces")"
echo "AUDIT_GRAPHS=${graphs} $(paste -sd' ' <<<"$workspaces")" >&2

resolve_failed=0
audit_failed=0
scanned=0
while IFS= read -r ws; do
  audit_workspace "$ws"
  scanned=$((scanned + 1))
  case "$__result" in
    ok) ;;
    audit-failed) audit_failed=1 ;;
    *) resolve_failed=1 ;;
  esac
done <<<"$workspaces"

# Every discovered graph was scanned (nothing ate the loop input).
if [[ "$scanned" -ne "$graphs" ]]; then
  echo "AUDIT_DISCOVERY: scanned ${scanned} of ${graphs} discovered graphs" >&2
  emit_status FAIL cargo-audit-discovery-failed
  exit 1
fi
if [[ "$resolve_failed" -eq 1 ]]; then
  emit_status FAIL cargo-lockfile-resolve-failed
  exit 1
elif [[ "$audit_failed" -eq 1 ]]; then
  emit_status FAIL cargo-audit-failed
  exit 1
fi
emit_status OK cargo-audit-passed
