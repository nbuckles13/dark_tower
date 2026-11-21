#!/usr/bin/env bash
# _gate2_binding.sh — shared single-source library for the Gate-2 authority gate
# (devloop task #51; design: docs/devloop-outputs/2026-06-09-gate2-authority-verdict-gate/main.md).
#
# Sourced by BOTH:
#   - the PRODUCER (scripts/layer-all.sh), which emits /tmp/devloop/gate2-verdict
#     via an EXIT trap as the pipeline's final step, and
#   - the VALIDATOR (.githooks/pre-commit), which recomputes the binding over the
#     STAGED index and enforces the verdict for devloop commits.
# Keeping the exclusion predicate / changeset enumeration / blob resolution /
# signature / slug-derivation in ONE file is the whole point: producer and hook
# can then differ ONLY in their git plumbing (worktree-vs-HEAD + hash-object on
# the producer; --cached + rev-parse :path on the hook), never in policy. A
# divergence between the two would be a silent signature gap.
#
# DEPENDENCY-LIGHT BY DESIGN: this library deliberately does NOT source
# scripts/lang/_common.sh. The pre-commit hook runs wherever a developer commits
# (incl. macOS bash-3 boxes via homebrew bash, hosts without the devloop cache,
# etc.); pulling in _common.sh would drag along lastpipe/job-control toggles,
# colour setup, and the CI-sentinel assertion the hook has no business running.
# Only coreutils + git are required here. This file carries its OWN strict-mode
# and bash-version guard so it is correct regardless of who sources it.
#
# THREAT MODEL (see docs/runbooks/devloop-validation.md): locally this gate is
# anti-drift / anti-laziness, NOT anti-forgery — an agent with filesystem write
# can hand-author a well-formed PASS verdict whose signature matches the staged
# tree, and `git commit --no-verify` bypasses the hook entirely. CI's independent
# from-scratch re-run of layer-all.sh on PRs into main/develop (.github/workflows/ci.yml
# `on:`) — sharded across parallel jobs (`--layers`), with the `Test Suite` job's
# `--aggregate` proving every layer ran exactly once — never reads the /tmp artifact and
# is the only non-bypassable enforcement point. The artifact is ephemeral in /tmp and is
# never committed. A conflict-resolved cherry-pick/rebase of an already-validated
# devloop commit needs no local verdict (REPLAY SKIP, above gate2_staged_trigger_slug);
# that relaxation stays inside this model — see its FORGERY note.

# SOURCE-SAFETY (operations source-hygiene review): this file is sourced by the
# pre-commit HOOK, which runs on EVERY commit and is plain `#!/bin/bash` + `set -e`.
# Therefore the preamble must NOT mutate the sourcer's shell:
#   - NO file-scope `set -euo pipefail` / `IFS` — that would change the hook's
#     option flags (`-u`/pipefail are stricter than the hook) for the rest of the
#     hook body (cargo fmt/clippy, the main.md loop). Each function that needs it
#     opens with `local -; set -euo pipefail` (function-local, auto-restored).
#   - NO file-scope `exit` on the bash-version check — a hard `exit 2` here would
#     kill the HOOK process on a bash-3 box for EVERY commit (not just devloop
#     ones), defeating the two-conjunct no-op-for-ordinary-commits design. The
#     check is a function (gate2_require_bash4) the bash-4-dependent entry points
#     call, returning nonzero (never exiting) so the caller decides.

# Idempotent-source guard (mirrors _common.sh:25). `return` is safe here: this file
# is only ever sourced, never executed.
[[ -n "${__DEVLOOP_GATE2_BINDING_SH:-}" ]] && return 0
readonly __DEVLOOP_GATE2_BINDING_SH=1

# LAYER_MAX / LAYER7_SUITES — the ONE home is the dependency-free
# _layers.sh (plain assignments, no shell options: source-safe for the hook). The
# emitter renders one LAYER line per layer 1..LAYER_MAX; run-story's full gate reads it.
# shellcheck source=_layers.sh
source "$(dirname "${BASH_SOURCE[0]}")/_layers.sh"

# Per-function strict mode. Functions that want -euo pipefail open with
# `local -; set -euo pipefail` directly (NOT via a helper call): `local -`
# (bash 4.4+, which we already require) makes the option change FUNCTION-LOCAL
# and auto-restored on return, so it is contained regardless of whether the
# caller invoked the function in a subshell — a sourced library never mutates
# its sourcer's options even if a future edit calls one of these fns directly.
# SCOPE NOTE (code-reviewer): `local -` saves/restores SHELL OPTIONS only, NOT
# IFS. That is fine here because these fns set only `set -euo pipefail` and never
# change IFS at function scope (they use `IFS= read ...` per-read, which is
# command-local). If a future edit changes IFS inside a strict fn, add `local IFS`
# too — `local -` will NOT restore it.
# gate2_validate_commit (called directly by the hook, NOT in a subshell) does
# NOT set strict mode — it is written to be correct under the hook's ambient
# `set -e` without -u/pipefail, so sourcing never changes the hook's behavior.

# Bash 4.4+ requirement. NOTE the floor is 4.4, NOT 4.0: `mapfile -d` (used by the
# producer + the hook validator) landed in bash 4.4 — a 4.0–4.3 box would pass a
# `>= 4.0` check and then FAIL at runtime on `mapfile -d`. (Associative arrays and
# `local -` are 4.0/4.4 respectively; 4.4 covers all features used here.) NON-FATAL:
# returns 2 instead of exiting, so SOURCING this file (e.g. into the pre-commit hook
# on a bash-3/early-4 box) can never kill the sourcer. The bash-4.4-dependent entry
# points call this and surface a clear error; ordinary commits that no-op before
# reaching them are unaffected.
gate2_require_bash4() {
  local major="${BASH_VERSINFO[0]:-0}" minor="${BASH_VERSINFO[1]:-0}"
  if [[ "$major" -lt 4 || ( "$major" -eq 4 && "$minor" -lt 4 ) ]]; then
    echo "gate2: requires bash >= 4.4 (mapfile -d, associative arrays); got ${BASH_VERSION:-unknown}" >&2
    return 2
  fi
  return 0
}

# Verdict artifact path. Lives under the pipeline cache namespace (/tmp/devloop,
# per _common.sh:44) but is referenced by literal here so the hook need not source
# _common.sh to learn it. Flat KEY=VALUE file, NOT JSON (validator is coreutils-only).
GATE2_VERDICT_FILE="${DEVLOOP_TMP:-/tmp/devloop}/gate2-verdict"

# Schema version stamped into the artifact (bump if the line grammar changes). v2
# (2026-11-21, killed-run fix): REASON/SIGNAL/MISSING keys, a LAYER line per layer, and a
# verdict decided by layer-all.sh derive_gate2 — a v1 file (whose PASS meant only "the
# trap saw $?==0", forgeable by a signal) no longer binds.
readonly GATE2_SCHEMA="gate2-verdict/v2"

# -----------------------------------------------------------------------------
# Exclusion set (the key judgment call — 3-way locked at Gate 1 with operations,
# security, code-reviewer). Mirrors crates/dt-guard/src/cross_boundary_scope.rs:64-84
# `is_symmetric_exclusion` ONE-FOR-ONE, plus one deliberate, commented widening.
#
# Files matching an exclusion are dropped from the binding's `files` set on BOTH
# sides (emit + recompute) and from the hook trigger's conjunct-2 count, so the
# emitter and validator share the identical universe.
#
# CRITICAL (security #1 landmine): the single-segment cases are matched with
# explicit prefix-strip + no-embedded-slash string ops, NOT a bash `case` glob.
# A bash `case $p in docs/.../*/INDEX.md)` would WRONGLY match nested paths
# because `*` matches `/` in pathname `case` patterns — a silent over-exclusion
# hole letting e.g. crates/x/INDEX.md or docs/user-stories/sub/evil.md escape the
# binding. Keep every matcher here as literal string ops.
# -----------------------------------------------------------------------------

# Human-readable summary of the exclusion set, stamped into the verdict as a
# single EXCLUSIONS= line for transparency (NOT parsed back — informational).
readonly GATE2_EXCLUSIONS_SUMMARY='docs/devloop-outputs/** docs/TODO.md docs/specialist-knowledge/*/INDEX.md docs/user-stories/*.md'

# gate2_is_excluded <path>
# Returns 0 (true) if <path> is exempt from the binding, 1 (false) otherwise.
# Pure string predicate — no git, no filesystem.
gate2_is_excluded() {
  local p="$1"

  # (1) docs/devloop-outputs/** — DELIBERATE widening past the precedent (which
  #     excludes only the active main.md self-referentially). Tree-wide here
  #     because Gate-3 writes verdicts/attachments into ANY devloop-output file
  #     AFTER this pipeline runs; if those writes were bound, the artifact would
  #     self-invalidate → infinite re-run loop. Prefix-anchored starts-with.
  [[ "$p" == docs/devloop-outputs/* ]] && return 0

  # (2) docs/TODO.md — post-validation bookkeeping append target (ADR-0019).
  #     EXACT equality, never a substring/contains test.
  [[ "$p" == "docs/TODO.md" ]] && return 0

  # (3) docs/specialist-knowledge/*/INDEX.md — single-segment `*`, no `/` crossing
  #     (reflection-phase artifact written after validation). Implemented as
  #     prefix-strip + assert the remainder is exactly "<one-segment>/INDEX.md"
  #     with NO further slash in the segment.
  if [[ "$p" == docs/specialist-knowledge/*/INDEX.md ]]; then
    local rest="${p#docs/specialist-knowledge/}"   # "<seg...>/INDEX.md"
    local seg="${rest%/INDEX.md}"                   # "<seg...>"
    [[ "$seg" != *"/"* ]] && return 0               # exactly one segment → exempt
  fi

  # (4) docs/user-stories/*.md — depth-2 top-level `*.md` ONLY (mirrors
  #     is_user_story_path: starts_with docs/user-stories/, ends_with .md, and the
  #     remainder contains no `/`). Subdir files (docs/user-stories/sub/x.md) stay
  #     BOUND — the safe direction.
  if [[ "$p" == docs/user-stories/*.md ]]; then
    local us="${p#docs/user-stories/}"              # "<remainder>.md"
    [[ "$us" != *"/"* ]] && return 0                # no nested slash → exempt
  fi

  return 1
}

# -----------------------------------------------------------------------------
# Changeset enumeration. Two variants, identical EXCEPT for git plumbing:
#   - worktree (producer): diff vs HEAD ∪ untracked, blobs via git hash-object
#   - staged   (hook):     diff --cached,            blobs via git rev-parse :path
# Both NUL-enumerate (`-z`) so an exotic path (spaces/TAB/newline) is read as raw
# bytes identically on both sides — NEVER parse core.quotePath quoted text, or the
# two sides would canonicalize differently and the signature would falsely differ.
# Both apply gate2_is_excluded with the SAME predicate above.
#
# Both also pass `--no-renames` (2026-10-06, security). Porcelain `git diff --name-only`
# honours diff.renames (default on), so a rename lists ONLY its destination and
# `--diff-filter=D` never sees the source: a `git mv` of a bound file into
# docs/devloop-outputs/ would delete it with zero binding visibility, and a bound→bound
# rename would drop the source's DELETED record. Every file-list diff in this file carries
# the flag, producer and hook alike, independent of the user's config.
#
# Each emits one path per line of OUTPUT via a NUL-safe printf '%s\0' stream on
# fd to the caller's `mapfile -d ''`. We expose array-returning helpers instead of
# pipelines to avoid `shopt -s lastpipe` (kept out so the hook context stays simple).
# -----------------------------------------------------------------------------

# __gate2_collect_excluded <newline-or-nul?> — internal: read NUL-delimited paths
# on stdin, print (NUL-delimited) only those NOT excluded. Pure filter.
__gate2_filter_excluded() {
  local path
  while IFS= read -r -d '' path; do
    gate2_is_excluded "$path" && continue
    printf '%s\0' "$path"
  done
}

# __gate2_capture_nul <array-name> <cmd> [args...] — run <cmd>, split its NUL-delimited
# stdout into the named array, and return <cmd>'s rc. FAIL-CLOSED READ: the exit status
# of a `< <(…)` process substitution is invisible (`mapfile < <(false)` and
# `while … done < <(false)` both return 0), so a failing enumeration read that way is
# indistinguishable from an EMPTY one — which on the trigger path reads as "not a
# devloop commit" (allow). Buffering through a temp file keeps the producer's rc.
# Every decision-bearing enumeration in this file goes through here; the two remaining
# `< <(…)` reads are deliberately exempt because they decide nothing (the emitter's
# human-readable FILE lines, and gate2_report_drift's post-block diagnostic).
__gate2_capture_nul() {
  local -n __gate2_cap_out="$1"; shift
  local __gate2_cap_tmp __gate2_cap_rc=0
  __gate2_cap_tmp="$(mktemp)" || return 1
  "$@" > "$__gate2_cap_tmp" || __gate2_cap_rc=$?
  if [[ "$__gate2_cap_rc" -eq 0 ]]; then
    mapfile -d '' __gate2_cap_out < "$__gate2_cap_tmp" || __gate2_cap_rc=$?
  fi
  rm -f "$__gate2_cap_tmp"
  return "$__gate2_cap_rc"
}

# gate2_changeset_worktree → prints NUL-delimited included paths (producer side).
# Universe: `git diff -z --name-only HEAD` ∪ `git ls-files -z --others
# --exclude-standard` (untracked), minus exclusions. Deduped, LC_ALL=C sorted.
gate2_changeset_worktree() {
  local -; set -euo pipefail   # function-local opts (bash 4.4+): auto-restored, never leaks to sourcer
  # `&&`, not a newline: callers reach this from condition contexts (if/||, via
  # __gate2_capture_nul) where errexit is inert, so a failing `git diff` must fail the
  # group explicitly or pipefail would only see ls-files' status.
  {
    git diff -z --no-renames --name-only HEAD &&
    git ls-files -z --others --exclude-standard
  } | __gate2_filter_excluded | LC_ALL=C sort -z -u
}

# gate2_changeset_staged → prints NUL-delimited included paths (hook side).
# Universe: `git diff --cached -z --name-only`, minus exclusions. The staged index
# is authoritative for what the commit will contain. Deduped, LC_ALL=C sorted.
gate2_changeset_staged() {
  local -; set -euo pipefail   # function-local opts (bash 4.4+): auto-restored, never leaks to sourcer
  git diff --cached -z --no-renames --name-only | __gate2_filter_excluded | LC_ALL=C sort -z -u
}

# -----------------------------------------------------------------------------
# Blob resolution. Both return the git blob OID for a path.
#
# OID-EQUALITY ASSUMPTION (code-reviewer): `git hash-object <worktree-path>`
# (producer) and `git rev-parse :<path>` (staged index, hook) yield IDENTICAL OIDs
# for identical content — both are the blob OID over object content. This holds
# UNLESS a content-altering clean/smudge filter (.gitattributes `text=auto`,
# `eol`, `filter=...`, `working-tree-encoding`, core.autocrlf) rewrites worktree
# bytes relative to the stored blob. The repo's `.gitattributes` declares only
# `merge=union` strategies (for docs/user-stories/*.md + docs/TODO.md) — merge
# drivers do NOT touch blob hashing, and both paths are excluded from the binding
# anyway, so no content-altering attribute applies to any bound path today. We do
# NOT switch the producer to index blobs — that would break the emit-at-worktree-
# time model (the producer runs before `git add`). Documented assumption.
#
# Symlinks: git stores a symlink as a blob of its LINK TARGET STRING (what the index,
# `rev-parse :path` and `rev-parse <tip>:path` return). `git hash-object <path>`,
# however, FOLLOWS the link and hashes the pointed-to file's contents — so the producer
# hashes the target string itself (readlink | hash-object --stdin) to stay symmetric.
# No traversal — intended. (Before 2026-11-10 the producer followed the link, so any
# changed symlink made the hook and the runner refuse; fail-closed, but wrong.)
# -----------------------------------------------------------------------------

# gate2_blob_worktree <path> → blob OID of the working-tree file (producer).
gate2_blob_worktree() {
  if [[ -L "$1" ]]; then
    local target
    target="$(readlink -n -- "$1")" || return 1
    printf '%s' "$target" | git hash-object --stdin
    return
  fi
  git hash-object -- "$1"
}

# gate2_blob_staged <path> → blob OID of the staged-index entry (hook).
# `--verify --quiet` makes git print NOTHING and EXIT NONZERO on an unresolvable
# ref, instead of echoing the literal ":path" to stdout (which a bare
# `rev-parse :path` does on a missing entry — that echo is what corrupted deletion
# records before this hardening). NOTE: do NOT append `^{blob}` — git parses
# `:path^{blob}` as the pathspec "path^{blob}", not a peel of the index entry, so it
# always fails. Belt-and-suspenders: deletions are classified explicitly below and
# never reach here; this clean-fail is the fail-closed backstop for any other
# unresolvable present path.
gate2_blob_staged() {
  git rev-parse --verify --quiet ":$1"
}

# -----------------------------------------------------------------------------
# Deletion handling (security HIGH finding — integrity bypass).
#
# A staged DELETION of a tracked file has NO blob: `git hash-object` (worktree,
# file gone) and `git rev-parse :path` (index entry removed) both FAIL. If a
# record producer naively resolves a blob per changeset path, a deletion either
# (a) truncates the signature stream when the failure aborts the loop, or
# (b) emits a garbage record (rev-parse echoes ":path") — BOTH identical on
# producer and hook, so the signatures still MATCH → false PASS, letting OTHER
# files be tampered post-validation. So deletions must be a FIRST-CLASS, bound
# record kind: we represent the deletion FACT with a sentinel "blob" token that
# can never collide with a real OID, identically on both sides.
#
# `GATE2_DELETED_OID` — the deletion sentinel placed in the blob field. The
# literal word "DELETED" cannot collide with a git OID (hex only) and keeps the
# blob-first "<token> <path>" record grammar intact (first field has no space).
readonly GATE2_DELETED_OID="DELETED"

# gate2_deletions_worktree / gate2_deletions_staged → NUL-delimited deleted paths
# (minus exclusions), via `--diff-filter=D`. Producer reads the worktree-vs-HEAD
# deletions; hook reads the staged (index-vs-HEAD) deletions.
gate2_deletions_worktree() {
  local -; set -euo pipefail
  git diff --diff-filter=D -z --no-renames --name-only HEAD | __gate2_filter_excluded | LC_ALL=C sort -z -u
}
gate2_deletions_staged() {
  local -; set -euo pipefail
  git diff --cached --diff-filter=D -z --no-renames --name-only | __gate2_filter_excluded | LC_ALL=C sort -z -u
}

# __gate2_in_nulset <path> <nul-set...> — internal: is <path> a member of the
# NUL-delimited set captured in array $2..? Used to classify a changeset path as
# deleted vs present without re-running git per path.
# (Implemented inline in the record producers via an associative lookup.)

# -----------------------------------------------------------------------------
# Signature. THE only enforced integrity field.
#
# Canonical serialization (delimiter-collision-free):
#   per-file record = "<blob> <path>" terminated by NUL.
#   blob is FIRST: a fixed-shape 40-hex (or 64-hex SHA-256) token with no spaces,
#   so the single space after it is an unambiguous field separator and the path
#   is the trailing field running to the NUL — a path may contain spaces / TAB /
#   newline without any escaping, because NUL is the ONLY byte a git path cannot
#   contain. (This supersedes an earlier TAB-delimited sketch: TAB is a legal path
#   byte, so it could not safely delimit; NUL is the only safe record separator.)
#   Records are LC_ALL=C-sorted (locale-stable byte order) then sha256summed.
#
# LOUD NOTE: the signature is computed over THIS blob-first NUL record stream —
# NOT over the human-readable `FILE <path>\t<blob>` lines rendered into the verdict
# file. Do not "helpfully" recompute the signature from the rendered FILE lines;
# they exist only for human eyes and use a different (lossy) field order.
#
# SYMMETRY CONSTRAINT (durability — @paired-protocol (f)). Every record kind in this stream must be
# derivable IDENTICALLY by BOTH sides — the producer from the WORKTREE, the hook from the staged INDEX. A
# fact known only to the producing run (e.g. "this run rewrote these files", or anything from FMT_APPLIED)
# CANNOT be signed here: it would appear in the producer's stream and never the hook's, diverging the
# recomputation on every commit and wedging the gate. This is why the deletion record (below) is a
# first-class, symmetrically-derivable "DELETED <path>" and not, say, a producer-only apply log.
#
# gate2_signature — reads NUL-delimited records "<blob> <path>" on stdin, prints
# the hex sha256 of the LC_ALL=C-sorted record stream.
# -----------------------------------------------------------------------------
gate2_signature() {
  local -; set -euo pipefail   # function-local opts (bash 4.4+): auto-restored, never leaks to sourcer
  local out
  # This sort OWNS the signature's order-canonicalization: it makes the hash invariant to the order records
  # ARRIVE in. It is NOT redundant with the upstream `LC_ALL=C sort -z -u` in gate2_records_{worktree,staged}
  # (:182/:190), which sorts to DEDUP the path universe; this sorts to canonicalize RECORD order before
  # hashing. Keep it (code-reviewer + @test ruling): cheap defense-in-depth for an authority control — do
  # NOT couple the signature's correctness to intermediate order-preservation surviving on both sides.
  out="$(LC_ALL=C sort -z | sha256sum)" || return 1   # explicit: errexit is inert in condition contexts
  # sha256sum prints "<hex>  -"; take the leading hex field via parameter expansion
  # (IFS-independent — robust regardless of the sourcer's IFS, and unaffected by the
  # space sha256sum uses as its separator).
  printf '%s' "${out%% *}"
}

# gate2_records_worktree → prints NUL-delimited "<token> <path>" records (producer).
# token = blob OID for a present file, or GATE2_DELETED_OID for a staged deletion.
# FAIL-CLOSED: a blob-resolution failure on a path that is NOT a known deletion is
# an unexpected error → abort the whole stream (an explicit `|| return 1`; errexit
# does not help here because callers run this inside `if !`/`||` conditions) rather
# than emit a short/garbage record; callers hash it under pipefail, so the abort fails
# the signature step. A truncated stream that matches on both sides is the
# exact integrity bypass this guards against.
gate2_records_worktree() {
  local -; set -euo pipefail
  local path blob
  local -A deleted=()
  local d
  local -a dels=() paths=()
  # Enumerations are rc-checked (__gate2_capture_nul), never read via `< <(…)`.
  __gate2_capture_nul dels gate2_deletions_worktree || return 1
  __gate2_capture_nul paths gate2_changeset_worktree || return 1
  for d in "${dels[@]+"${dels[@]}"}"; do deleted["$d"]=1; done
  for path in "${paths[@]+"${paths[@]}"}"; do
    if [[ -n "${deleted[$path]:-}" ]]; then
      printf '%s %s\0' "$GATE2_DELETED_OID" "$path"
      continue
    fi
    # Present file: resolve its blob. EXPLICIT check, not `set -e`: this fn is reached
    # from condition contexts (`if ! sig="$(… | gate2_signature)"`) where errexit is
    # inert even with `local -; set -e`. A failure aborts the stream → fail closed.
    blob="$(gate2_blob_worktree "$path")" || return 1
    printf '%s %s\0' "$blob" "$path"
  done
}

# gate2_records_staged → prints NUL-delimited "<token> <path>" records (hook).
# Same classification + fail-closed contract as the worktree side, over the staged
# index. Producer and hook agree byte-for-byte: a present file → its blob OID
# (hash-object == rev-parse :path for equal content), a deletion → GATE2_DELETED_OID.
gate2_records_staged() {
  local -; set -euo pipefail
  local path blob
  local -A deleted=()
  local d
  local -a dels=() paths=()
  __gate2_capture_nul dels gate2_deletions_staged || return 1
  __gate2_capture_nul paths gate2_changeset_staged || return 1
  for d in "${dels[@]+"${dels[@]}"}"; do deleted["$d"]=1; done
  for path in "${paths[@]+"${paths[@]}"}"; do
    if [[ -n "${deleted[$path]:-}" ]]; then
      printf '%s %s\0' "$GATE2_DELETED_OID" "$path"
      continue
    fi
    blob="$(gate2_blob_staged "$path")" || return 1   # explicit; see gate2_records_worktree
    printf '%s %s\0' "$blob" "$path"
  done
}

# -----------------------------------------------------------------------------
# COMMITTED variant — the story runner's post-commit check (ADR-0035 §3, 2026-11-10
# amendment). The third plumbing variant beside worktree (producer) and staged (hook):
#   - committed (runner): diff <base>..<tip>, blobs via git rev-parse <tip>:path
# <base> is the verdict's own HEAD, so this stream is byte-identical to what the
# producer emitted iff <tip>'s tree (minus exclusions) is the tree layer-all ran on.
# That is what makes it immune to the trap ADR-0035 §3 records: recomputing the
# WORKTREE stream after a commit diffs against the NEW HEAD, which is empty for any
# clean tree and so "matches" any verdict computed over an empty change. Here the
# base comes from the verdict, never from the current HEAD.
# Same exclusions, --no-renames, deletion sentinel and fail-closed contract as the
# other two variants (SYMMETRY CONSTRAINT above).
# -----------------------------------------------------------------------------
gate2_changeset_commit() {
  local -; set -euo pipefail
  git diff -z --no-renames --name-only "$1" "$2" -- | __gate2_filter_excluded | LC_ALL=C sort -z -u
}
gate2_deletions_commit() {
  local -; set -euo pipefail
  git diff --diff-filter=D -z --no-renames --name-only "$1" "$2" -- | __gate2_filter_excluded | LC_ALL=C sort -z -u
}
# gate2_blob_commit <tip> <path> — blob OID of <path> in <tip>'s tree. Equal to the
# producer's `git hash-object` for equal content (OID-EQUALITY ASSUMPTION above).
gate2_blob_commit() {
  git rev-parse --verify --quiet "$1:$2"
}

# gate2_records_commit <base> <tip> → NUL-delimited "<token> <path>" records for the
# change <base>..<tip>. Same classification and fail-closed contract as
# gate2_records_worktree.
gate2_records_commit() {
  local -; set -euo pipefail
  local base="$1" tip="$2" path blob d
  local -A deleted=()
  local -a dels=() paths=()
  __gate2_capture_nul dels gate2_deletions_commit "$base" "$tip" || return 1
  __gate2_capture_nul paths gate2_changeset_commit "$base" "$tip" || return 1
  for d in "${dels[@]+"${dels[@]}"}"; do deleted["$d"]=1; done
  for path in "${paths[@]+"${paths[@]}"}"; do
    if [[ -n "${deleted[$path]:-}" ]]; then
      printf '%s %s\0' "$GATE2_DELETED_OID" "$path"
      continue
    fi
    blob="$(gate2_blob_commit "$tip" "$path")" || return 1   # explicit; see gate2_records_worktree
    printf '%s %s\0' "$blob" "$path"
  done
}

# gate2_verdict_binds_commit <verdict-file> [tip] — does a PASSING layer-all verdict
# attest <tip>'s committed tree (default HEAD)? Returns 0 iff ALL hold:
#   1. the verdict exists and carries this SCHEMA;
#   2. GATE2=PASS (layer-all exited 0, i.e. every layer ran and passed);
#   3. its HEAD is a full commit OID that is an ancestor of (or equal to) <tip>;
#   4. its SIGNATURE equals gate2_records_commit <verdict HEAD> <tip> | gate2_signature.
# Holding 3+4 means the tree layer-all validated IS <tip>'s tree outside the exclusion
# set, however many commits (task, manifest bump, operator fixes) lie in between.
# Otherwise returns 1 and prints ONE line "<token>: <detail>" on stdout, token from the
# closed set: verdict-missing | verdict-schema | verdict-not-pass | verdict-head-invalid
# | verdict-head-not-ancestor | verdict-signature-mismatch | verdict-recompute-failed.
# FAIL-CLOSED: every git error is a non-match (return 1), never a match.
# THREAT MODEL: anti-laziness, as for the hook (header) — a hand-written verdict passes.
gate2_verdict_binds_commit() {
  local file="$1" tip_ref="${2:-HEAD}"
  local schema gate2 vhead vsig tip sig anc_rc=0
  gate2_require_bash4 >/dev/null 2>&1 || { printf 'verdict-recompute-failed: bash >= 4.4 required\n'; return 1; }
  if [[ ! -f "$file" ]]; then
    printf 'verdict-missing: no Gate-2 verdict at %s (layer-all.sh has not run in this container)\n' "$file"
    return 1
  fi
  schema="$(gate2_verdict_get SCHEMA "$file")"
  gate2="$(gate2_verdict_get GATE2 "$file")"
  vhead="$(gate2_verdict_get HEAD "$file")"
  vsig="$(gate2_verdict_get SIGNATURE "$file")"
  if [[ "$schema" != "$GATE2_SCHEMA" ]]; then
    printf 'verdict-schema: SCHEMA=%s, expected %s\n' "$(__gate2_printable "$schema")" "$GATE2_SCHEMA"
    return 1
  fi
  if [[ "$gate2" != "PASS" ]]; then
    printf 'verdict-not-pass: GATE2=%s %s\n' "$(__gate2_printable "$gate2")" "$(__gate2_verdict_cause "$file")"
    return 1
  fi
  if ! [[ "$vhead" =~ ^[0-9a-f]{40}([0-9a-f]{24})?$ ]] || ! git cat-file -e "${vhead}^{commit}" 2>/dev/null; then
    printf 'verdict-head-invalid: HEAD=%s is not a commit in this repository\n' "$(__gate2_printable "$vhead")"
    return 1
  fi
  if ! tip="$(git rev-parse --verify --quiet "${tip_ref}^{commit}")"; then
    printf 'verdict-recompute-failed: cannot resolve %s to a commit\n' "$(__gate2_printable "$tip_ref")"
    return 1
  fi
  git merge-base --is-ancestor "$vhead" "$tip" 2>/dev/null || anc_rc=$?
  if [[ "$anc_rc" -eq 1 ]]; then
    printf 'verdict-head-not-ancestor: verdict HEAD %s is not an ancestor of %s (the verdict is for another line of history)\n' "${vhead:0:12}" "${tip:0:12}"
    return 1
  elif [[ "$anc_rc" -ne 0 ]]; then
    printf 'verdict-recompute-failed: git merge-base --is-ancestor exited %s\n' "$anc_rc"
    return 1
  fi
  if ! sig="$(set -o pipefail; gate2_records_commit "$vhead" "$tip" | gate2_signature)"; then
    printf 'verdict-recompute-failed: computing the committed-tree signature over %s..%s failed\n' "${vhead:0:12}" "${tip:0:12}"
    return 1
  fi
  if [[ "$sig" != "$vsig" ]]; then
    printf 'verdict-signature-mismatch: the tree at %s differs from the tree layer-all validated (verdict HEAD %s, RUN_AT %s) — something changed after the last layer-all run\n' \
      "${tip:0:12}" "${vhead:0:12}" "$(__gate2_printable "$(gate2_verdict_get RUN_AT "$file")")"
    return 1
  fi
  return 0
}

# -----------------------------------------------------------------------------
# Slug derivation — PURE. Returns the active devloop slug, or empty.
#
# Mirrors crates/dt-guard/src/cross_boundary_scope.rs:192-202 `find_active_main_md`:
# a candidate is a path that starts_with "docs/devloop-outputs/", ends_with
# "/main.md", and does NOT start_with "docs/devloop-outputs/_" (the _template).
# The slug is the path segment between "docs/devloop-outputs/" and "/main.md".
#
# This function NEVER aborts (no exit, no nonzero return that callers must trap):
#   exactly one candidate → prints the slug;
#   zero or more-than-one  → prints empty (and on >1, a stderr diagnostic).
# Policy lives in the CALLERS, not here:
#   - PRODUCER (layer-all.sh) runs in CI / ad-hoc contexts where 0 or >1 active
#     main.md is perfectly legitimate; it stamps an empty SLUG and CONTINUES,
#     never aborting the pipeline.
#   - HOOK applies its own FAIL-CLOSED trigger logic; an empty/mismatched slug is
#     a diagnostic only (the SIGNATURE is the integrity anchor).
#
# gate2_derive_slug <path>... — caller passes the candidate path list (producer
# passes its worktree changeset; hook passes its staged changeset), so the query
# is NOT hardcoded inside and both sides stay symmetric.
# -----------------------------------------------------------------------------

# gate2_is_devloop_mainmd <path> — true iff <path> is an active (non-template)
# devloop main.md, per find_active_main_md's three rules (cross_boundary_scope.rs).
# Single source for the predicate so gate2_derive_slug + gate2_staged_trigger_slug
# can't drift (dry-reviewer slug-idiom extraction).
gate2_is_devloop_mainmd() {
  local p="$1"
  [[ "$p" == docs/devloop-outputs/_* ]] && return 1          # _template → no
  [[ "$p" == docs/devloop-outputs/*/main.md ]]
}

# gate2_mainmd_slug <path> — extract the slug (segment between the prefix and
# /main.md) from a devloop main.md path. Single source for the extraction idiom.
gate2_mainmd_slug() {
  local rest="${1#docs/devloop-outputs/}"   # "<slug...>/main.md"
  printf '%s' "${rest%/main.md}"            # "<slug...>"
}

gate2_derive_slug() {
  local p
  local -a slugs=()
  for p in "$@"; do
    gate2_is_devloop_mainmd "$p" || continue
    slugs+=("$(gate2_mainmd_slug "$p")")
  done
  if [[ ${#slugs[@]} -eq 1 ]]; then
    printf '%s' "${slugs[0]}"
  elif [[ ${#slugs[@]} -gt 1 ]]; then
    printf 'gate2: multiple active devloop main.md in changeset (%s) — slug ambiguous, leaving empty\n' \
      "${slugs[*]}" >&2
    # print nothing (empty slug)
  fi
  # zero candidates → print nothing
}

# -----------------------------------------------------------------------------
# Layer-4 target counts (RECORDED for humans, NOT hook-enforced). Sums the
# `test result: ok. N passed; M failed; K ignored; ...; L filtered out` lines
# cargo emits across all test binaries in the layer-4 log. These surface the
# vector-A/B signal (0 ignored / 0 filtered) to a human reading the verdict, but
# enforcing them is task #50's Layer-4 job, not this gate's.
#
# gate2_layer4_counts <layer4-log-path> — prints "passed=N failed=N ignored=N
# filtered=N" (zeros if the log is absent or has no test-result lines).
# -----------------------------------------------------------------------------
gate2_layer4_counts() {
  local log="$1"
  local passed=0 failed=0 ignored=0 filtered=0
  if [[ -f "$log" ]]; then
    # awk sums each cargo "test result:" line. Field positions are stable in the
    # libtest summary format: "... N passed; M failed; K ignored; J measured; L filtered out ...".
    # awk sums each category and prints one `key=value` per line; we then read
    # each line individually (IFS in this file is $'\n\t', so newline-separated
    # output is split correctly without depending on space-splitting).
    local line key val
    while IFS='=' read -r key val; do
      case "$key" in
        passed)   passed="$val" ;;
        failed)   failed="$val" ;;
        ignored)  ignored="$val" ;;
        filtered) filtered="$val" ;;
      esac
    done < <(awk '
      /test result:/ {
        for (i = 1; i <= NF; i++) {
          if ($i == "passed;")       p += $(i-1)
          else if ($i == "failed;")  f += $(i-1)
          else if ($i == "ignored;") g += $(i-1)
          else if ($i == "filtered") l += $(i-1)
        }
      }
      END { printf "passed=%d\nfailed=%d\nignored=%d\nfiltered=%d\n", p+0, f+0, g+0, l+0 }
    ' "$log")
  fi
  printf 'passed=%s failed=%s ignored=%s filtered=%s' \
    "$passed" "$failed" "$ignored" "$filtered"
}

# -----------------------------------------------------------------------------
# Producer, part 1: the tree SNAPSHOT. The ONE reader of the worktree for the verdict.
#
# layer-all.sh takes it ONCE, before Layer 1, and every verdict it writes (the run-start
# `REASON=in-progress` one and the exit one) is written FROM it — so the verdict attests the
# tree the layers started on, never the tree as it happens to be at exit. At a clean exit
# layer-all takes a second snapshot and derive_gate2 refuses PASS unless the two match
# (REASON=tree-changed-during-run): an edit made while layers ran is never attested.
#
# gate2_snapshot_worktree <dir> — write into <dir> (which the caller created, 700):
#   head       — HEAD's full OID
#   base_ref   — merge-base(origin/main, HEAD), may be empty
#   slug       — the active devloop slug from the RAW worktree changeset (pre-exclusion:
#                the active main.md lives under docs/devloop-outputs/**, which IS excluded
#                from the binding; mirror of gate2_staged_trigger_slug on the hook side)
#   records    — the canonical NUL record stream (gate2_records_worktree)
#   signature  — gate2_signature over it (authoritative; FILE lines are derived from records)
# Returns non-zero (and leaves the dir incomplete) on ANY git/hash failure: the caller
# treats that as a failed snapshot, never as an empty tree.
# -----------------------------------------------------------------------------
gate2_snapshot_worktree() {
  # Run from the work-tree TOP whatever the caller's cwd (the record stream's paths are
  # top-relative and its blobs are hashed by path); <dir> is made absolute first.
  local top dir
  top="$(git rev-parse --show-toplevel 2>/dev/null)" && [[ -n "$top" ]] || return 1
  dir="$(cd "$1" 2>/dev/null && pwd -P)" || return 1
  ( cd "$top" && __gate2_snapshot_here "$dir" )
}
__gate2_snapshot_here() {
  local -; set -euo pipefail
  local dir="$1" head sig slug
  local -a raw_paths=()
  gate2_require_bash4 || return 2
  [[ -d "$dir" ]] || return 1
  head="$(git rev-parse --verify --quiet HEAD^{commit})" || return 1
  [[ -n "$head" ]] || return 1
  __gate2_capture_nul raw_paths __gate2_raw_worktree_paths || return 1
  slug="$(gate2_derive_slug "${raw_paths[@]+"${raw_paths[@]}"}")"
  gate2_records_worktree > "${dir}/records" || return 1
  sig="$(gate2_signature < "${dir}/records")" || return 1
  [[ -n "$sig" ]] || return 1
  printf '%s' "$head" > "${dir}/head"
  printf '%s' "$(git merge-base origin/main HEAD 2>/dev/null || printf '')" > "${dir}/base_ref"
  printf '%s' "$slug" > "${dir}/slug"
  printf '%s' "$sig" > "${dir}/signature"
}

# gate2_snapshot_get <dir> <field> — print one snapshot field (empty if absent).
gate2_snapshot_get() {
  [[ -f "$1/$2" ]] || return 0
  cat "$1/$2"
}

# gate2_snapshot_identity <dir> — "<head> <signature>" (empty if either is missing): what
# two snapshots must share for the tree to count as unchanged. HEAD is part of it because
# the record stream is a diff AGAINST HEAD — a commit made mid-run changes what it means.
gate2_snapshot_identity() {
  local h s
  h="$(gate2_snapshot_get "$1" head)"; s="$(gate2_snapshot_get "$1" signature)"
  [[ -n "$h" && -n "$s" ]] || return 0
  printf '%s %s' "$h" "$s"
}

# gate2_snapshot_changed_paths <start-dir> <end-dir> [max] — the paths whose record differs
# between two snapshots (added, removed or re-blobbed), comma-separated, at most <max>
# (default 5), for the WARN GATE2_TREE_CHANGED_DURING_RUN line. Display only.
gate2_snapshot_changed_paths() {
  local a="$1/records" b="$2/records" max="${3:-5}"
  [[ -f "$a" && -f "$b" ]] || return 0
  LC_ALL=C comm -3 -z <(LC_ALL=C sort -z "$a") <(LC_ALL=C sort -z "$b") 2>/dev/null \
    | tr '\0' '\n' | sed 's/^\t//; s/^[^ ]* //' | LC_ALL=C sort -u | head -n "$max" \
    | tr -d '\r' | paste -sd, - 2>/dev/null || true
}

# __gate2_raw_worktree_paths — the producer's RAW (pre-exclusion) changed-path list,
# NUL-delimited, for slug derivation. `&&` so a failing `git diff` fails the call.
__gate2_raw_worktree_paths() {
  git diff -z --no-renames --name-only HEAD &&
  git ls-files -z --others --exclude-standard
}

# -----------------------------------------------------------------------------
# Producer, part 2: write the verdict artifact from a snapshot. DECIDES NOTHING.
#
# The PASS/FAIL decision and its REASON have ONE home: scripts/layer-all.sh
# derive_gate2 (which owns the layer record, LAYER_MAX completeness and the status
# ladder this library cannot see). This function only RENDERS it, fail-closed:
# GATE2=PASS is written iff <verdict> is the literal `PASS`; anything else — FAIL,
# empty, a typo — is written as GATE2=FAIL.
#
# Writes atomically (temp + `mv -fT`, so a partial write is never readable and the
# verdict can never land INSIDE a directory squatting at the path).
#
# Verdict lifecycle (runbook §8.5):
#   - no file            — layer-all.sh never ran its authority mode here, refused at its
#                          start, or was killed between removing the old verdict (right after
#                          taking its lock) and writing the run-start one;
#   - REASON=in-progress — written at authority-run start: a run is in flight, or the last
#                          one died uncatchably (SIGKILL/OOM) before writing its own verdict;
#   - anything else      — the verdict of the last run that reached its EXIT trap.
#
# emit_gate2_verdict <snapshot-dir> <verdict> <reason> <exit> <signal> <missing-csv> \
#                    <status-array-name> <duration-array-name>
#   snapshot-dir — MANDATORY (gate2_snapshot_worktree output); no exit-time fallback
#   verdict      — PASS | FAIL (derive_gate2)
#   reason       — closed set, derive_gate2's header (none on PASS)
#   exit         — the derived LAYER_ALL_EXIT, or `none` for the run-start verdict
#   signal       — TERM | INT | HUP | none
#   missing-csv  — layers with no terminal record ("" = none)
#   arrays       — per-layer STATUS / DURATION by name (namerefs; unset entries fine)
# Renders one LAYER line per layer 1..LAYER_MAX: the recorded status, or MISSING for a
# layer with no record (display only, like NOT-RUN — never an aggregation input).
# -----------------------------------------------------------------------------
emit_gate2_verdict() {
  gate2_require_bash4 || return 2

  local snap="$1" verdict="$2" reason="$3" layer_all_exit="$4" signal="$5" missing="$6"
  local -n __ls="$7"
  local -n __ld="$8"
  local gate2 tmp n st du sig head

  if [[ -z "$snap" || ! -d "$snap" ]]; then
    printf 'WARN gate2: emit_gate2_verdict called without a snapshot dir — not writing a verdict.\n' >&2
    return 2
  fi
  sig="$(gate2_snapshot_get "$snap" signature)"
  head="$(gate2_snapshot_get "$snap" head)"
  if [[ -z "$sig" || -z "$head" || ! -f "$snap/records" ]]; then
    printf 'WARN gate2: snapshot %s is incomplete — not writing a verdict.\n' "$snap" >&2
    return 2
  fi
  if ! [[ "${LAYER_MAX:-}" =~ ^[1-9][0-9]*$ ]]; then
    printf 'WARN gate2: LAYER_MAX is unset/invalid (_layers.sh not sourced?) — not writing a verdict.\n' >&2
    return 2
  fi

  # DEVLOOP_TMP-inside-repo guard (code-reviewer): the verdict MUST be ephemeral and
  # never committable. If DEVLOOP_TMP is (mis)configured to a path inside the work
  # tree, the artifact would land in the repo. Warn loudly; don't write into the tree.
  local __wt; __wt="$(git rev-parse --show-toplevel 2>/dev/null || printf '')"
  local __vdir; __vdir="$(cd "$(dirname "$GATE2_VERDICT_FILE")" 2>/dev/null && pwd -P || printf '')"
  if [[ -n "$__wt" && -n "$__vdir" && ( "$__vdir" == "$__wt" || "$__vdir" == "$__wt"/* ) ]]; then
    printf 'WARN gate2: DEVLOOP_TMP resolves INSIDE the work tree (%s) — the verdict must stay ephemeral/uncommittable. Set DEVLOOP_TMP to a path outside the repo (default /tmp/devloop). Not writing the verdict.\n' "$__vdir" >&2
    return 2
  fi

  if [[ "$verdict" == "PASS" ]]; then gate2="PASS"; else gate2="FAIL"; fi

  tmp="$(mktemp "${GATE2_VERDICT_FILE}.XXXXXX")" || return 2
  {
    printf 'SCHEMA=%s\n'          "$GATE2_SCHEMA"
    printf 'GATE2=%s\n'           "$gate2"
    printf 'REASON=%s\n'          "${reason:-none}"
    printf 'LAYER_ALL_EXIT=%s\n'  "$layer_all_exit"
    printf 'SIGNAL=%s\n'          "${signal:-none}"
    printf 'MISSING=%s\n'         "$missing"
    printf 'SLUG=%s\n'            "$(gate2_snapshot_get "$snap" slug)"
    printf 'HEAD=%s\n'            "$head"
    printf 'BASE_REF=%s\n'        "$(gate2_snapshot_get "$snap" base_ref)"
    printf 'RUN_AT=%s\n'          "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'EXCLUSIONS=%s\n'      "$GATE2_EXCLUSIONS_SUMMARY"

    # Per-layer recorded detail (display). Layer 4 carries the cargo target counts.
    for (( n = 1; n <= LAYER_MAX; n++ )); do
      st="${__ls[$n]:-}" du="${__ld[$n]:-}"
      if [[ -z "$st" ]]; then
        printf 'LAYER %s MISSING 0\n' "$n"
      elif [[ "$n" -eq 4 ]]; then
        printf 'LAYER %s %s %s %s\n' \
          "$n" "$st" "${du:-0}" "$(gate2_layer4_counts "${DEVLOOP_TMP:-/tmp/devloop}/layer-4.log")"
      else
        printf 'LAYER %s %s %s\n' "$n" "$st" "${du:-0}"
      fi
    done

    # Binding FILE lines — HUMAN-READABLE rendering only (path<TAB>token), derived from
    # the snapshot's record stream (the same bytes SIGNATURE hashes).
    local rec rtoken rpath
    while IFS= read -r -d '' rec; do
      rtoken="${rec%% *}"
      rpath="${rec#* }"
      printf 'FILE %s\t%s\n' "$rpath" "$rtoken"
    done < "$snap/records"

    printf 'SIGNATURE=%s\n' "$sig"
  } > "$tmp" || { rm -f "$tmp"; return 2; }
  mv -fT "$tmp" "$GATE2_VERDICT_FILE" || { rm -f "$tmp"; return 2; }
}

# =============================================================================
# VALIDATOR (hook side). gate2_validate_commit enforces the verdict for devloop
# commits. Factored as a function (not inlined in .githooks/pre-commit) so the
# isolation self-test can drive it against synthetic staged trees without running
# the full hook (which also runs cargo fmt/clippy).
#
# FAIL-CLOSED CONTRACT: every code path that cannot positively establish "this is
# a non-devloop commit" OR "the verdict is present + PASS + signature-matches"
# returns NONZERO (block). No `|| true`. The validator must never become the
# silent-skip it exists to prevent.
#
# Return codes:
#   0 — allow the commit (either: not a devloop commit, or verdict fully valid).
#   1 — block the commit (verdict absent / FAIL / stale / slug-mismatch / error).
# Diagnostics go to stderr.
# =============================================================================

# gate2_verdict_get <KEY> <verdict-file> — print the value of a `KEY=value` scalar
# line (last wins). Empty if absent. Coreutils only (no jq).
gate2_verdict_get() {
  local key="$1" file="$2"
  [[ -f "$file" ]] || return 0
  # grep the anchored KEY=, take the last occurrence, strip the KEY= prefix.
  local line
  line="$(grep "^$key=" "$file" 2>/dev/null | tail -n1 || true)"
  printf '%s' "${line#"$key"=}"
}

# __gate2_verdict_cause <verdict-file> — the one-line "why not PASS" every consumer prints
# (hook, run-story via gate2_verdict_binds_commit): REASON / SIGNAL / MISSING /
# LAYER_ALL_EXIT, control bytes neutralised. Distinguishes a refused start
# (pipeline-incomplete, MISSING=<all>, SIGNAL=none), a kill (SIGNAL=TERM), a run in flight
# or uncatchably killed (in-progress) and a failed layer without opening the file.
__gate2_verdict_cause() {
  local f="$1"
  printf 'REASON=%s SIGNAL=%s MISSING=%s LAYER_ALL_EXIT=%s' \
    "$(__gate2_printable "$(gate2_verdict_get REASON "$f")")" \
    "$(__gate2_printable "$(gate2_verdict_get SIGNAL "$f")")" \
    "$(__gate2_printable "$(gate2_verdict_get MISSING "$f")")" \
    "$(__gate2_printable "$(gate2_verdict_get LAYER_ALL_EXIT "$f")")"
}

# -----------------------------------------------------------------------------
# REPLAY SKIP (devloop 2026-10-03-gate2-replay-skip; runbook §8.5 "Replays").
#
# A conflict-resolved replay (`git cherry-pick --continue`, or a `git commit` at a
# stopped cherry-pick/rebase) re-stages a devloop main.md that was already gated when
# the replayed commit was made, so the trigger re-fires with no new evidence. A staged
# complete main.md is dropped from the trigger set iff ALL of these hold:
#   1. no MERGE_HEAD (merges are not vouched for — rebase instead; §8.5);
#   2. exactly one of CHERRY_PICK_HEAD / REBASE_HEAD resolves to a commit, and it is
#      the pseudoref itself (not a branch of that name);
#   3. the staged blob == that path's blob in the replayed commit (OID equality);
#   4. the replayed commit has exactly ONE parent (read from the raw object, ignoring
#      replace refs/grafts): a root or a merge is never vouched for;
#   5. it wrote exactly one complete devloop main.md, and that is this path (added or
#      modified relative to its parent — `diff-tree`, so an unreadable parent tree is a
#      git error, never "absent").
# The evidence is the commit's TREE, not its message. A commit writing a complete main.md
# went through this hook when it was made, OR through a hook-free path (`--no-verify`,
# `rebase --continue`, a clean pick, `commit-tree`) — a message line could claim no more,
# and CI on PRs into main/develop is the backstop either way. The replayed commit ITSELF
# is the source — no ancestor walk, so a record an ancestor wrote is never vouched for by
# a later commit that did not touch it. The path carries the slug. Exactly-one mirrors the
# creation-time invariant: a commit completing two devloops could only have bypassed it.
#
# WHAT THIS RELAXES (the user's decision): only trigger conjunct 1. When every staged
# complete main.md qualifies, the WHOLE commit needs no local verdict, including the
# bound files the conflict resolution changed — the bytes nobody validated locally. The
# signature, record streams, exclusion set and conjunct 2 are untouched (producer/hook
# symmetry). The skip line counts those paths; CI on PRs into main/develop
# (.github/workflows/ci.yml `on:`) re-runs the full pipeline.
#
# FORGERY: a hand-written CHERRY_PICK_HEAD can only replay a main.md byte-identical,
# at the same path, to one a real commit already wrote — it cannot mint a new
# completion record. Strictly weaker than `--no-verify` or a
# hand-authored PASS verdict, both of which already exist (THREAT MODEL above).
#
# FAIL-CLOSED: these run from `if`/`$(…)` contexts where `set -e` is inert, so every
# git call is rc-checked explicitly and every enumeration goes through
# __gate2_capture_nul (never `< <(…)`). Any git error ⇒ "does not qualify" ⇒ today's
# verdict requirement — never "skip". Diagnostics go to STDERR: the trigger fn runs
# inside `$(…)` and its STDOUT is the slug.
# -----------------------------------------------------------------------------

# Replay state for the commit being evaluated, set by gate2_replay_source and the
# per-path helpers (the trigger fn runs in its own `$(…)` subshell, so these never leak
# into the hook).
__gate2_replay_ref=""      # CHERRY_PICK_HEAD | REBASE_HEAD | MERGE_HEAD (when refusing a merge)
__gate2_replay_sha=""      # full commit OID of the replayed commit ("" if unresolvable)
__gate2_replay_token=""    # refusal token when gate2_replay_source returns 2
__gate2_replay_reason=""   # refusal reason when gate2_replay_source returns 2
__gate2_commit_checked=0   # gate2_replay_commit_facts ran for this commit
__gate2_commit_token=""    # its refusal token ("" = the commit can vouch)
__gate2_commit_reason=""   # its refusal reason
__gate2_commit_written=""  # the one complete devloop main.md path the replayed commit wrote
__gate2_refused_tokens=()  # every refusal token printed this commit
__gate2_modified_paths=()  # paths refused as main-md-modified (the only restorable kind)
__gate2_vouched_paths=()   # paths gate2_replay_qualifies vouched for
__gate2_vouched_blobs=()   # their staged blob OIDs (parallel to __gate2_vouched_paths)

# __gate2_printable <text> — the text with control bytes (newline, ESC, …) replaced by
# `?`, so a hostile path can't inject escapes or forge extra hook lines.
__gate2_printable() {
  printf '%s' "${1//[[:cntrl:]]/?}"
}

# __gate2_replay_refuse <path> <token> <reason> — print the "skip not applied" line and
# record the token (the post-loop action line depends on which kinds were refused).
__gate2_replay_refuse() {
  local sha="${__gate2_replay_sha:-unresolved}"
  printf '⚠️  Gate-2: replay skip not applied to %s (%s %s): [%s] %s\n' \
    "$(__gate2_printable "$1")" "$__gate2_replay_ref" "${sha:0:12}" "$2" "$3" >&2
  __gate2_refused_tokens+=("$2")
  if [[ "$2" == "main-md-modified" ]]; then
    __gate2_modified_paths+=("$1")
  fi
}

# gate2_replay_source — classify the replay state (once per commit).
#   0 — a usable replay: __gate2_replay_ref/__gate2_replay_sha set.
#   1 — no replay in progress (silent; today's behaviour).
#   2 — replay state present but not vouchable: __gate2_replay_token/_reason set.
# EXCLUDED as sources, deliberately: MERGE_HEAD (no single replayed commit — a merge
# carries a whole branch; §8.5 says rebase instead) and REVERT_HEAD (a revert stages
# the PRE-image, not validated content). absorb-devloop.sh's in-progress detector
# lists both as peers for a different question; do not "align" the two lists.
gate2_replay_source() {
  __gate2_replay_ref="" __gate2_replay_sha="" __gate2_replay_token="" __gate2_replay_reason=""
  local mh_path sha rc ref full
  local -a refs=() shas=()

  # (1) MERGE_HEAD first and positively, so a merge stop (incl. `rebase -r` stopped on
  # a merge: REBASE_HEAD + MERGE_HEAD) can never fall through to a pick qualification.
  # MERGE_HEAD is a FILE under every ref backend; its mere presence refuses, whatever
  # it holds. rev-parse is the belt to that brace; any rc other than 0/1 is an error.
  rc=0; mh_path="$(git rev-parse --git-path MERGE_HEAD)" || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    __gate2_replay_ref="MERGE_HEAD"
    __gate2_replay_token="git-error" __gate2_replay_reason="rev-parse --git-path MERGE_HEAD failed (rc=$rc)"
    return 2
  fi
  rc=0; sha="$(git rev-parse --verify --quiet 'MERGE_HEAD^{commit}')" || rc=$?
  if [[ -e "$mh_path" || "$rc" -eq 0 ]]; then
    __gate2_replay_ref="MERGE_HEAD" __gate2_replay_sha="${sha:-}"
    __gate2_replay_token="merge-not-vouched"
    __gate2_replay_reason="merges of devloop records are not vouched for — rebase instead (runbook §8.5)"
    return 2
  elif [[ "$rc" -ne 1 ]]; then
    __gate2_replay_ref="MERGE_HEAD"
    __gate2_replay_token="git-error" __gate2_replay_reason="rev-parse MERGE_HEAD^{commit} failed (rc=$rc)"
    return 2
  fi

  # (2) Exactly one of the replay pseudorefs. rc 1 = absent; any other rc = git error.
  for ref in CHERRY_PICK_HEAD REBASE_HEAD; do
    rc=0; sha="$(git rev-parse --verify --quiet "${ref}^{commit}")" || rc=$?
    if [[ "$rc" -eq 0 ]]; then
      refs+=("$ref"); shas+=("$sha")
    elif [[ "$rc" -ne 1 ]]; then
      __gate2_replay_ref="$ref"
      __gate2_replay_token="git-error" __gate2_replay_reason="rev-parse ${ref}^{commit} failed (rc=$rc)"
      return 2
    fi
  done
  if [[ ${#refs[@]} -eq 0 ]]; then
    return 1
  fi
  __gate2_replay_ref="${refs[0]}" __gate2_replay_sha="${shas[0]}"
  if [[ ${#refs[@]} -gt 1 ]]; then
    __gate2_replay_token="multiple-replay-states"
    __gate2_replay_reason="${refs[0]} and ${refs[1]} are both present"
    return 2
  fi

  # (3) Not a branch spoof: `git rev-parse X` falls back to refs/heads/X when no
  # pseudoref X exists, so a branch named CHERRY_PICK_HEAD would fake replay state.
  rc=0; full="$(git rev-parse --symbolic-full-name "$__gate2_replay_ref")" || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    __gate2_replay_token="git-error"
    __gate2_replay_reason="rev-parse --symbolic-full-name $__gate2_replay_ref failed (rc=$rc)"
    return 2
  fi
  if [[ "$full" != "$__gate2_replay_ref" ]]; then
    __gate2_replay_token="ref-not-pseudoref"
    __gate2_replay_reason="$__gate2_replay_ref resolves via $(__gate2_printable "$full"), not the replay pseudoref"
    return 2
  fi
  return 0
}

# __gate2_phase_cell <content> — print the value cell of the main.md content's FIRST
# Loop State Phase row (`| Phase | <cell> |`), trimmed; return 1 if there is no such row.
# The ONE Phase-row parser: __gate2_is_complete decides on it, and the Phase≠complete
# refusal displays it, so the two can never read different rows.
__gate2_phase_cell() {
  local row
  row="$(printf '%s\n' "$1" | grep -m1 '^| Phase |' || true)"
  [[ -n "$row" ]] || return 1
  row="${row#| Phase |}"
  row="${row%%|*}"
  row="${row#"${row%%[![:space:]]*}"}"   # trim leading whitespace
  row="${row%"${row##*[![:space:]]}"}"   # trim trailing whitespace
  printf '%s' "$row"
}

# __gate2_is_complete <content> — 0 iff the main.md content's Loop State Phase row has
# leading word `complete` (row shape: `| Phase | `<value>` |`, value maybe backticked).
# Exactly lowercase `complete`: no row, an empty cell, `COMPLETE`, `Complete`, `**Phase**`
# or `|Phase|complete|` are all NOT complete (fail-closed — the Phase≠complete refusal
# treats every one of them as an incomplete record).
__gate2_is_complete() {
  local cell w
  cell="$(__gate2_phase_cell "$1")" || return 1
  cell="${cell#\`}"
  [[ "$cell" =~ ^([A-Za-z0-9-]+) ]] || return 1
  w="${BASH_REMATCH[1]}"
  [[ "$w" == "complete" ]]
}

# gate2_replay_commit_facts — rules 4 and 5 for the replayed commit, once per commit:
# sets __gate2_commit_token/_reason (refusal) or __gate2_commit_written (the one record
# it wrote). Objects are read with --no-replace-objects, so a graft or replace ref can't
# make the commit look like a root; a missing object (shallow boundary) is a git error.
gate2_replay_commit_facts() {
  [[ "$__gate2_commit_checked" -eq 1 ]] && return 0
  __gate2_commit_checked=1 __gate2_commit_token="" __gate2_commit_reason="" __gate2_commit_written=""
  local sha="$__gate2_replay_sha" raw line parent="" nparents=0 rc=0 f content
  local -a changed=() complete=()
  raw="$(git --no-replace-objects cat-file commit "$sha")" || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    __gate2_commit_token="git-error" __gate2_commit_reason="cat-file commit <replayed> failed (rc=$rc)"; return 0
  fi
  while IFS= read -r line; do
    [[ -z "$line" ]] && break                       # end of the header
    if [[ "$line" == "parent "* ]]; then parent="${line#parent }"; nparents=$((nparents + 1)); fi
  done <<<"$raw"
  if [[ "$nparents" -eq 0 ]]; then
    __gate2_commit_token="root-commit-not-vouched" __gate2_commit_reason="the replayed commit has no parent"; return 0
  elif [[ "$nparents" -gt 1 ]]; then
    __gate2_commit_token="replayed-merge-not-vouched"
    __gate2_commit_reason="the replayed commit is a merge ($nparents parents) — merges are not vouched for (runbook §8.5)"; return 0
  fi
  __gate2_capture_nul changed git --no-replace-objects diff-tree -r -z --no-renames --no-commit-id --name-only --diff-filter=AM "$parent" "$sha" || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    __gate2_commit_token="git-error" __gate2_commit_reason="diff-tree <parent> <replayed> failed (rc=$rc)"; return 0
  fi
  for f in "${changed[@]+"${changed[@]}"}"; do
    gate2_is_devloop_mainmd "$f" || continue
    rc=0; content="$(git --no-replace-objects cat-file blob "$sha:$f")" || rc=$?
    if [[ "$rc" -ne 0 ]]; then
      __gate2_commit_token="git-error" __gate2_commit_reason="cat-file blob <replayed>:<path> failed (rc=$rc)"; return 0
    fi
    __gate2_is_complete "$content" && complete+=("$f")
  done
  if [[ ${#complete[@]} -gt 1 ]]; then
    __gate2_commit_token="multiple-records-in-replayed-commit"
    __gate2_commit_reason="the replayed commit wrote ${#complete[@]} complete devloop main.md (a commit completes exactly one devloop)"; return 0
  fi
  __gate2_commit_written="${complete[0]:-}"
}

# gate2_replay_qualifies <path> — the per-main.md predicate, after gate2_replay_source
# returned 0. Returns 0 iff <path> qualifies (and records it in __gate2_vouched_*;
# the skip line is printed by the caller once the whole commit is known); otherwise
# prints the refusal line and returns 1.
gate2_replay_qualifies() {
  local path="$1" staged_oid src_oid rc
  local sha="$__gate2_replay_sha"

  rc=0; staged_oid="$(git rev-parse --verify --quiet ":$path")" || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    __gate2_replay_refuse "$path" git-error "rev-parse :<path> failed (rc=$rc)"
    return 1
  fi
  rc=0; src_oid="$(git --no-replace-objects rev-parse --verify --quiet "$sha:$path")" || rc=$?
  if [[ "$rc" -eq 1 ]]; then
    __gate2_replay_refuse "$path" path-absent-in-replayed-commit "the replayed commit's tree has no such path"
    return 1
  elif [[ "$rc" -ne 0 ]]; then
    __gate2_replay_refuse "$path" git-error "rev-parse <replayed>:<path> failed (rc=$rc)"
    return 1
  fi
  if [[ "$staged_oid" != "$src_oid" ]]; then
    __gate2_replay_refuse "$path" main-md-modified "staged blob $staged_oid != replayed blob $src_oid (edited during resolution)"
    return 1
  fi

  # (4)+(5) one parent, and this is the one complete record the commit wrote.
  gate2_replay_commit_facts
  if [[ -n "$__gate2_commit_token" ]]; then
    __gate2_replay_refuse "$path" "$__gate2_commit_token" "$__gate2_commit_reason"
    return 1
  fi
  if [[ "$__gate2_commit_written" != "$path" ]]; then
    __gate2_replay_refuse "$path" not-written-by-replayed-commit "the replayed commit did not write this path (it holds the same blob as its parent)"
    return 1
  fi
  __gate2_vouched_paths+=("$path")
  __gate2_vouched_blobs+=("$staged_oid")
  return 0
}

# __gate2_replay_unvalidated_counts <sha> — for the skip line: prints "N M", where N is
# the number of bound staged paths (validated nowhere locally once the skip fires) and
# M how many of them differ from the replayed commit. On a git error prints the failed
# step + rc and returns 1 (the caller then withdraws the skip: no audit record, no skip).
__gate2_replay_unvalidated_counts() {
  local sha="$1" rc=0
  local -a bound=() differ=()
  __gate2_capture_nul bound gate2_changeset_staged || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    printf 'enumerating the staged changeset failed (rc=%s)' "$rc"; return 1
  fi
  if [[ ${#bound[@]} -gt 0 ]]; then
    __gate2_capture_nul differ git --literal-pathspecs diff --cached -z --no-renames --name-only "$sha" -- "${bound[@]}" || rc=$?
    if [[ "$rc" -ne 0 ]]; then
      printf 'diff --cached against the replayed commit failed (rc=%s)' "$rc"; return 1
    fi
  fi
  printf '%s %s' "${#bound[@]}" "${#differ[@]}"
}

# __gate2_replay_report_vouched <found-array-name> — after the trigger loop, report the
# vouched replays. If nothing else needs a verdict, print the skip line(s) with the
# N/M counts; if a count can't be produced, withdraw the skip (refuse [git-error] and
# append the slugs to <found>: no audit record ⇒ no skip). If another main.md still
# requires a verdict, the "NOT checked" claim would be false, so print the vouched
# variant instead.
__gate2_replay_report_vouched() {
  local -n __gate2_rv_found="$1"
  local f i counts
  [[ ${#__gate2_vouched_paths[@]} -gt 0 ]] || return 0
  if [[ ${#__gate2_rv_found[@]} -gt 0 ]]; then
    for f in "${__gate2_vouched_paths[@]}"; do
      printf '⚠️  Gate-2: replay vouched for %s (%s %s) — but another complete main.md in this commit still requires a verdict.\n' \
        "$(__gate2_printable "$f")" "$__gate2_replay_ref" "$__gate2_replay_sha" >&2
    done
    return 0
  fi
  if ! counts="$(__gate2_replay_unvalidated_counts "$__gate2_replay_sha")"; then
    for f in "${__gate2_vouched_paths[@]}"; do
      __gate2_replay_refuse "$f" git-error "$counts"
      __gate2_rv_found+=("$(gate2_mainmd_slug "$f")")
    done
    return 0
  fi
  for i in "${!__gate2_vouched_paths[@]}"; do
    printf '⚠️  Gate-2: replay skip: %s blob %s == %s %s (slug %s); Gate-2 verdict NOT checked for this commit — %s bound path(s) not validated locally (%s differ from the replayed commit) — run ./scripts/layer-all.sh to validate, or rely on CI on PRs into main/develop (.github/workflows/ci.yml `on:`).\n' \
      "$(__gate2_printable "${__gate2_vouched_paths[$i]}")" "${__gate2_vouched_blobs[$i]}" \
      "$__gate2_replay_ref" "$__gate2_replay_sha" \
      "$(__gate2_printable "$(gate2_mainmd_slug "${__gate2_vouched_paths[$i]}")")" \
      "${counts% *}" "${counts#* }" >&2
  done
}

# __gate2_replay_action_lines — one action line per KIND of refusal printed this
# commit: the fix depends on why the skip didn't apply (§8.5 failure table).
__gate2_replay_action_lines() {
  local tok f merge=0 other=0
  for tok in "${__gate2_refused_tokens[@]+"${__gate2_refused_tokens[@]}"}"; do
    case "$tok" in
      merge-not-vouched) merge=1 ;;
      main-md-modified)  ;;   # restore line below, per path
      *)                 other=1 ;;
    esac
  done
  if [[ "$merge" -eq 1 ]]; then
    printf '   git merge --abort (discards the in-progress resolution), then rebase (or rerun absorb-devloop.sh); or git commit --no-verify if you accept CI as the only check (your decision). See docs/runbooks/devloop-validation.md §8.5.\n' >&2
  fi
  for f in "${__gate2_modified_paths[@]+"${__gate2_modified_paths[@]}"}"; do
    printf '   Restore it from the replayed commit and resume: git checkout %s -- %s (absorb: --theirs for docs/devloop-outputs/**).\n' \
      "$__gate2_replay_ref" "$(__gate2_printable "$f")" >&2
  done
  if [[ "$other" -eq 1 ]]; then
    printf '   Not fixable by restoring main.md: validate then stage (./scripts/layer-all.sh && git add -A — a PASS needs Layer 7, i.e. the devloop cluster helper) and resume; without it, git commit --no-verify is your decision, with CI on PRs into main/develop as the check. See docs/runbooks/devloop-validation.md §8.5.\n' >&2
  fi
}

# __gate2_staged_mainmds — the ONE fail-closed enumeration of staged devloop records,
# shared by the trigger (gate2_staged_trigger_slug) and the Phase≠complete refusal
# (gate2_refuse_incomplete_mainmds) so they cannot disagree about which files are devloop
# records or how a read error fails closed. Sets the parallel arrays __gate2_mm_paths /
# __gate2_mm_contents to every staged, non-deleted (`--diff-filter=d`: a deleted record
# has no Phase, and `git show :<deleted>` is rc 128), non-template devloop main.md and its
# STAGED (index) content. It makes NO empty/complete decision — each caller applies its
# own policy (the trigger skips empty content; the refusal treats empty as incomplete).
# Returns 0, or 3 (with a stderr diagnostic) if the staged changeset cannot be enumerated
# or a present staged main.md cannot be read — never "no records".
__gate2_mm_paths=()
__gate2_mm_contents=()
__gate2_staged_mainmds() {
  local f content rc=0
  local -a candidates=()
  __gate2_mm_paths=() __gate2_mm_contents=()
  __gate2_capture_nul candidates git diff --cached -z --no-renames --name-only --diff-filter=d || rc=$?
  if [[ "$rc" -ne 0 ]]; then
    printf 'gate2: enumerating staged changes failed (rc=%s)\n' "$rc" >&2
    return 3
  fi
  for f in "${candidates[@]+"${candidates[@]}"}"; do
    gate2_is_devloop_mainmd "$f" || continue
    if ! content="$(git show ":$f")"; then
      printf 'gate2: cannot read staged %s\n' "$(__gate2_printable "$f")" >&2
      return 3
    fi
    __gate2_mm_paths+=("$f")
    __gate2_mm_contents+=("$content")
  done
  return 0
}

# gate2_staged_trigger_slug — print the slug of the SINGLE staged devloop main.md that
# is at Phase=complete AND is not a vouched replay (see REPLAY SKIP above).
#   exactly one → prints the slug, returns 0.
#   zero        → prints nothing, returns 0 (not a devloop-completion commit, or every
#                 complete main.md is a vouched replay).
#   more than 1 → prints nothing, returns 2 (AMBIGUOUS) + a stderr diagnostic — when no
#                 replay git error fired.
#   error       → returns 3: the staged changeset could not be enumerated, a PRESENT
#                 staged main.md could not be read (before, both read as "no devloop
#                 main.md" — a silent allow), or more than one main.md is left after a
#                 replay-check [git-error] (the git error, not ambiguity, is the
#                 cause). Staged deletions are excluded from the enumeration
#                 (--diff-filter=d): a deleted file cannot be at Phase=complete, and
#                 `git show :<deleted>` is rc 128.
# The hook is the fail-closed context: >1 non-replay complete main.md is the Rust
# `bail!` analogue (exactly-one is the invariant for a devloop-completion commit), so
# the caller BLOCKS on return 2 or 3. A mixed commit (one vouched replay + one not)
# still requires a verdict for the other, and then no "verdict NOT checked" line is
# printed — the verdict IS checked. (The producer never calls this; it derives the
# slug from the worktree via the PURE, never-aborting gate2_derive_slug instead.)
# Runs inside `$(…)`: STDOUT is the slug; replay skip/refusal lines go to STDERR.
#
# Phase detection: the Loop State table row `| Phase | \`complete...\` |`. The value
# cell is matched on its LEADING word being exactly `complete` (the only "done"
# token; reflection/planning/review/implementation/gate-3/setup are not). Trailing
# annotations like `complete (iteration 2)` still count as complete.
gate2_staged_trigger_slug() {
  local f staged i
  local -a found=()
  local replay_rc=-1
  # Reset the per-commit replay state, so a second call in one shell (or a direct,
  # non-`$(…)` call) never inherits earlier refusals or vouches.
  __gate2_refused_tokens=() __gate2_modified_paths=() __gate2_vouched_paths=() __gate2_vouched_blobs=()
  __gate2_commit_checked=0 __gate2_commit_token="" __gate2_commit_reason="" __gate2_commit_written=""
  # Staged devloop records with their INDEX content (shared, fail-closed enumerator).
  __gate2_staged_mainmds || return 3
  for i in "${!__gate2_mm_paths[@]}"; do
    f="${__gate2_mm_paths[$i]}" staged="${__gate2_mm_contents[$i]}"
    [[ -n "$staged" ]] || continue
    __gate2_is_complete "$staged" || continue

    # Replay skip: classify the replay state once, then vouch per main.md.
    if [[ "$replay_rc" -eq -1 ]]; then
      if gate2_replay_source; then replay_rc=0; else replay_rc=$?; fi
    fi
    if [[ "$replay_rc" -eq 0 ]]; then
      gate2_replay_qualifies "$f" && continue
    elif [[ "$replay_rc" -eq 2 ]]; then
      __gate2_replay_refuse "$f" "$__gate2_replay_token" "$__gate2_replay_reason"
    fi
    found+=("$(gate2_mainmd_slug "$f")")
  done

  __gate2_replay_report_vouched found
  __gate2_replay_action_lines

  # A git error already explains why more than one main.md is left; the ambiguity
  # message would misattribute it. Same block either way, via the trigger-error path.
  if [[ ${#found[@]} -gt 1 ]]; then
    local tok
    for tok in "${__gate2_refused_tokens[@]+"${__gate2_refused_tokens[@]}"}"; do
      if [[ "$tok" == "git-error" ]]; then
        printf 'gate2: replay check hit a git error (above) — blocking on the trigger error\n' >&2
        return 3
      fi
    done
  fi

  if [[ ${#found[@]} -eq 1 ]]; then
    printf '%s' "${found[0]}"
    return 0
  elif [[ ${#found[@]} -gt 1 ]]; then
    printf 'gate2: multiple staged complete devloop main.md (%s) — ambiguous\n' \
      "${found[*]}" >&2
    return 2
  fi
  return 0   # zero found
}

# -----------------------------------------------------------------------------
# PHASE≠COMPLETE REFUSAL (devloop 2026-10-06-pipeline-status-phase-gate-playwright;
# runbook §8.5 "Phase not complete").
#
# The verdict trigger (conjunct 1) fires only for a staged main.md at Phase=complete, so a
# commit that staged its devloop record with Phase still `setup` (the ci-speed commit)
# carried its code past the gate with no verdict checked. This closes that knob: a commit
# that stages ANY devloop main.md that is not at Phase=complete together with ≥1 BOUND
# file is refused. The two sets are a PARTITION (security B1): every staged non-template,
# non-deleted devloop main.md is either complete (__gate2_is_complete → the verdict path)
# or not — an empty file, a missing/garbled row, `COMPLETE` all land here; no third state
# allows.
#
# "Bound" is the binding universe, gate2_changeset_staged — the SAME set conjunct 2 uses
# (Lead ruling; deliberately not the task's literal "outside docs/devloop-outputs/"): the
# refusal fires exactly when setting Phase=complete would make the verdict REQUIRED, so
# Phase can never change WHETHER a verdict is required, and there is no second exclusion
# list to drift from the binding's.
#
# Runs FIRST in gate2_validate_commit, independent of the trigger slug, the replay skip
# and conjunct 2, so mixed commits (a complete record + another incomplete one + code)
# are refused too. NO replay exemption (security B5): a conflict-resolved replay of one of
# the historical commits whose record never reached `complete` is refused; the operator
# decides on `--no-verify` (the message says so, scoped). The replay skip for COMPLETE
# records is unchanged.
#
# Returns 0 (nothing to refuse) or 1 (refused, or any git error — fail-closed). On the
# refusal path only, it calls gate2_replay_source to word the message, which sets the
# __gate2_replay_* globals in the hook's shell; harmless, because that path ends the commit
# (and the trigger resets its own replay state at entry).
# -----------------------------------------------------------------------------
GATE2_REFUSAL_LIST_MAX=5   # bound files named in the refusal before "+N more"

gate2_refuse_incomplete_mainmds() {
  local i f content cell n
  local -a bad=() bad_desc=() bound=()
  # A failed enumeration/read here is the SAME failure the trigger would hit (shared
  # enumerator), so it reports with the trigger-error message the runbook documents.
  local mm_rc=0
  __gate2_staged_mainmds || mm_rc=$?
  if [[ "$mm_rc" -ne 0 ]]; then
    printf '\n❌ Gate-2: error evaluating the commit trigger (rc=%s) — blocking (fail-closed).\n' "$mm_rc" >&2
    return 1
  fi
  for i in "${!__gate2_mm_paths[@]}"; do
    f="${__gate2_mm_paths[$i]}" content="${__gate2_mm_contents[$i]}"
    __gate2_is_complete "$content" && continue
    bad+=("$f")
    if cell="$(__gate2_phase_cell "$content")"; then
      bad_desc+=("Phase reads '$(__gate2_printable "$cell")'")
    elif [[ -z "$content" ]]; then
      bad_desc+=('the staged file is empty (no Loop State "| Phase |" row)')
    else
      bad_desc+=('no Loop State "| Phase |" row')
    fi
  done
  [[ ${#bad[@]} -gt 0 ]] || return 0

  if ! __gate2_capture_nul bound gate2_changeset_staged; then
    printf '\n❌ Gate-2: error enumerating the staged changeset for the Phase check (git output above) — blocking (fail-closed).\n' >&2
    return 1
  fi
  [[ ${#bound[@]} -gt 0 ]] || return 0   # bookkeeping-only commit: no verdict could be required

  printf '\n❌ Gate-2: [phase-not-complete] devloop main.md staged at Phase≠complete with validated files\n' >&2
  for i in "${!bad[@]}"; do
    printf '   %s: %s; the gate needs the Loop State row "| Phase | `complete` |".\n' \
      "$(__gate2_printable "${bad[$i]}")" "${bad_desc[$i]}" >&2
  done
  printf '   Validated files staged with it (%s):' "${#bound[@]}" >&2
  for ((n = 0; n < ${#bound[@]} && n < GATE2_REFUSAL_LIST_MAX; n++)); do
    printf ' %s' "$(__gate2_printable "${bound[$n]}")" >&2
  done
  if [[ ${#bound[@]} -gt GATE2_REFUSAL_LIST_MAX ]]; then
    printf ' +%s more' "$(( ${#bound[@]} - GATE2_REFUSAL_LIST_MAX ))" >&2
  fi
  printf '\n' >&2
  printf '   If this is the devloop you are committing: set its Loop State Phase to `complete` — the hook then requires the Gate-2 verdict (devloop SKILL.md Step 8).\n' >&2
  printf '   If you are editing an older devloop record: commit the docs/devloop-outputs/ change separately from the code.\n' >&2
  if gate2_replay_source; then
    printf '   Replay in progress (%s %s): only if you are replaying a historical commit whose record never reached `complete`, after resolving the conflict, with this main.md byte-identical to the replayed commit, `git commit --no-verify` is your decision — it skips ALL pre-commit checks; CI on PRs into main/develop re-runs the pipeline (it does not check the record).\n' \
      "$__gate2_replay_ref" "${__gate2_replay_sha:0:12}" >&2
  fi
  printf '   See docs/runbooks/devloop-validation.md §8.5 ("Phase not complete").\n' >&2
  return 1
}

# gate2_validate_commit — the hook's entry point. See contract above.
#
# NOTE this function is called DIRECTLY by the hook (not in a subshell), so it must
# NOT mutate the hook's shell options — it deliberately does NOT set strict mode and
# is written to be correct under the hook's ambient `set -e` alone (with `${x:-}`
# guards so it is -u-independent).
gate2_validate_commit() {
  # bash-4 is required to evaluate the trigger (mapfile -d, associative arrays,
  # read -d). If unavailable, the gate cannot run. Rather than wedge EVERY commit
  # on a bash-3 box (the hook gates all commits, not just devloop ones), WARN and
  # skip — consistent with the threat model: the local hook is advisory/anti-drift,
  # and CI's independent from-scratch re-run (which runs in the bash-4 pipeline
  # environment) is the actual non-bypassable enforcement. A bash-3 dev box is
  # exotic here — the pipeline already hard-requires bash-4 via _common.sh.
  if ! gate2_require_bash4; then
    echo "  ⚠️  Gate-2 verdict check SKIPPED (bash < 4.4). CI will still enforce. Upgrade bash to re-enable the local check." >&2
    return 0
  fi
  local verdict="$GATE2_VERDICT_FILE"
  local staged_slug

  # --- Phase≠complete refusal: runs FIRST, independent of everything below. ---------
  gate2_refuse_incomplete_mainmds || return 1

  # --- TRIGGER conjunct 1: a staged devloop main.md at Phase=complete? ---------
  # (Vouched replays are filtered out inside gate2_staged_trigger_slug — REPLAY SKIP.)
  # Capture the rc explicitly: rc 2 = AMBIGUOUS (>1 staged complete main.md) →
  # fail-closed BLOCK (the devloop-commit context where exactly-one is invariant);
  # rc 3 = a staged main.md could not be read → the trigger-error BLOCK below.
  local slug_rc=0
  staged_slug="$(gate2_staged_trigger_slug)" || slug_rc=$?
  if [[ "$slug_rc" -eq 2 ]]; then
    printf '\n❌ Gate-2: more than one staged devloop main.md is at Phase=complete.\n' >&2
    printf '   A commit can complete exactly one devloop. Stage only one completed main.md.\n' >&2
    return 1
  elif [[ "$slug_rc" -ne 0 ]]; then
    # Any other nonzero from the trigger eval → fail closed (never silent-skip).
    printf '\n❌ Gate-2: error evaluating the commit trigger (rc=%s) — blocking (fail-closed).\n' "$slug_rc" >&2
    return 1
  fi
  if [[ -z "$staged_slug" ]]; then
    # No staged complete devloop main.md (or every one is a vouched replay) → this
    # commit needs no verdict.
    # Hook no-ops (matrix f). (Ordinary commits and historical/typo edits to an
    # already-complete main.md that don't re-stage it land here.)
    return 0
  fi

  # --- TRIGGER conjunct 2: staged changeset − exclusions has >=1 file? ---------
  # Reuses the SAME exclusion machinery as the binding, so a main.md-only edit
  # (validated set empty) is a no-op, while a reopened/--continue devloop with
  # validated files staged is still enforced.
  local -a staged_paths=()
  if ! __gate2_capture_nul staged_paths gate2_changeset_staged; then
    # An unreadable changeset must not read as an empty one (that would allow).
    printf '\n❌ Gate-2: error enumerating the staged changeset (git output above) — blocking (fail-closed).\n' >&2
    return 1
  fi
  if [[ ${#staged_paths[@]} -eq 0 ]]; then
    return 0   # nothing validated in this commit → no verdict required (matrix c)
  fi

  # --- From here the verdict is REQUIRED. Any failure below BLOCKS. ------------
  if [[ ! -f "$verdict" ]]; then
    printf '\n❌ Gate-2: no validation verdict found at %s\n' "$verdict" >&2
    printf '   This devloop commit stages validated files but no pipeline run produced a verdict.\n' >&2
    printf '   Run the full pipeline before committing:  ./scripts/layer-all.sh\n' >&2
    printf '   (Authority skip-vector C: a verdict cannot be asserted in lieu of a real run.)\n' >&2
    return 1
  fi

  local gate2 recorded_sig recorded_slug
  gate2="$(gate2_verdict_get GATE2 "$verdict")"
  recorded_sig="$(gate2_verdict_get SIGNATURE "$verdict")"
  recorded_slug="$(gate2_verdict_get SLUG "$verdict")"

  local schema
  schema="$(gate2_verdict_get SCHEMA "$verdict")"
  if [[ "$schema" != "$GATE2_SCHEMA" ]]; then
    printf '\n❌ Gate-2: verdict SCHEMA=%s, expected %s — written by an older layer-all.sh; re-run:  ./scripts/layer-all.sh\n' \
      "$(__gate2_printable "${schema:-MISSING}")" "$GATE2_SCHEMA" >&2
    return 1
  fi

  if [[ "$gate2" != "PASS" ]]; then
    printf '\n❌ Gate-2: verdict is %s (not PASS) — %s\n' "$(__gate2_printable "${gate2:-MISSING}")" "$(__gate2_verdict_cause "$verdict")" >&2
    printf '   Fix the cause and re-run:  ./scripts/layer-all.sh  (runbook §8.5 lists each REASON)\n' >&2
    return 1
  fi

  # Slug-match is a DIAGNOSTIC cross-check (the SIGNATURE is the integrity anchor).
  # Non-empty recorded slug that disagrees with the staged devloop → stale verdict
  # from a different devloop in the same /tmp session → block with a clear message.
  # Empty recorded slug → fall through to signature-only (still blocks on mismatch).
  if [[ -n "$recorded_slug" && "$recorded_slug" != "$staged_slug" ]]; then
    printf '\n❌ Gate-2: verdict is for a different devloop (verdict SLUG=%s, committing %s).\n' \
      "$recorded_slug" "$staged_slug" >&2
    printf '   This looks like a stale verdict from another devloop. Re-run for THIS one:  ./scripts/layer-all.sh\n' >&2
    return 1
  fi

  # --- Recompute the signature over the STAGED index and compare. --------------
  local recomputed_sig
  # pipefail in the subshell: a failed record stream must not hash as a short one.
  if ! recomputed_sig="$(set -o pipefail; gate2_records_staged | gate2_signature)"; then
    printf '\n❌ Gate-2: error recomputing the staged-tree signature (git output above) — blocking (fail-closed).\n' >&2
    return 1
  fi
  if [[ "$recomputed_sig" == "$recorded_sig" ]]; then
    return 0   # binding matches the staged tree → allow (matrix a)
  fi

  # Mismatch → produce the three-bucket drift diagnostic so the block is actionable.
  printf '\n❌ Gate-2: the staged tree does not match the validated tree (signature mismatch).\n' >&2
  printf '   The pipeline verdict was produced for a different set of changes than what is staged.\n' >&2
  gate2_report_drift "$verdict" >&2
  # Order: VALIDATE, then stage what you validated (`./scripts/layer-all.sh && git add -A`), NOT the
  # reverse. `emit_gate2_verdict` signs the WORKTREE; the hook compares the staged INDEX. Staging first
  # then validating would sign a tree the index doesn't hold — and under ADR-0037 §D7 (fmt applies during
  # a local run) the reverse order is actively wrong: layer-all reformats mid-run, so `git add -A &&
  # layer-all` stages pre-format content and signs post-format, reproducing the very mismatch this
  # message resolves and pushing the operator toward `--no-verify`. (Correct regardless of D7 — validate,
  # then stage the validated tree; D7 only makes the wrong order break rather than merely mislead.)
  printf '   Run the pipeline, then stage what it validated:  ./scripts/layer-all.sh && git add -A\n' >&2
  return 1
}

# gate2_report_drift <verdict-file> — print the three-bucket diagnostic (required
# to satisfy matrix (b): NAME the drifted path(s)):
#   recorded ∖ staged  — "validated but not staged"
#   staged   ∖ recorded — "staged but not in verdict"
#   intersection w/ blob mismatch — "modified after validation"
# Builds associative maps path→blob from the recorded FILE lines and the freshly
# recomputed staged records, then compares.
gate2_report_drift() {
  local verdict="$1"
  local -A rec=() stg=()
  local path blob line

  # Recorded: parse the human-readable `FILE <path>\t<blob>` lines. (These ARE the
  # recorded path/blob set; the signature is separate but the FILE lines mirror it.)
  while IFS= read -r line; do
    # strip leading "FILE "
    line="${line#FILE }"
    path="${line%$'\t'*}"
    blob="${line##*$'\t'}"
    [[ -n "$path" ]] && rec["$path"]="$blob"
  done < <(grep '^FILE ' "$verdict" 2>/dev/null || true)

  # Staged: recompute fresh from the SAME records stream the signature uses, so the
  # token (blob OID or GATE2_DELETED_OID) is consistent with the recorded FILE lines
  # and a staged deletion compares correctly (token "DELETED" vs "DELETED").
  local rec_line rtoken rpath
  while IFS= read -r -d '' rec_line; do
    rtoken="${rec_line%% *}"
    rpath="${rec_line#* }"
    stg["$rpath"]="$rtoken"
  done < <(gate2_records_staged)

  local p
  for p in "${!rec[@]}"; do
    if [[ -z "${stg[$p]:-}" ]]; then
      printf '     - validated but not staged: %s\n' "$p"
    elif [[ "${stg[$p]}" != "${rec[$p]}" ]]; then
      printf '     - modified after validation: %s\n' "$p"
    fi
  done
  for p in "${!stg[@]}"; do
    if [[ -z "${rec[$p]:-}" ]]; then
      printf '     - staged but not in verdict: %s\n' "$p"
    fi
  done
}
