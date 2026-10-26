#!/usr/bin/env bash
# Guard: NO `google.protobuf.Any` IN ANY PROTO — the precondition of the `buf breaking`
# `MESSAGE_NO_DELETE` exception (docs/protocol/CONVENTIONS.md §Enforcement, human decision
# 2026-10-23).
#
# Rule content: @protocol. Machinery: @infrastructure (CLAUDE.md guard-ownership split).
# Runbook: docs/runbooks/devloop-validation.md §6.3
#
# WHY. `proto/buf.yaml` excepts MESSAGE_NO_DELETE because a message name is on neither the
# binary nor the proto3 JSON wire. `Any` breaks that premise: it carries the message's
# fully-qualified name as a type URL, so deleting or renaming a message packed into an `Any`
# is a real wire break that buf no longer reports. Before this guard, the only defence was a
# "revisit if `Any` is introduced" sentence in CONVENTIONS.
#
# WHAT IT ENFORCES, over every `proto/**/*.proto`, on comment-stripped, whitespace-normalised
# text (one statement per line, `a . b` joined), so formatting cannot slip past it:
#   proto-any-import — an `import` (plain, `public` or `weak`) of `google/protobuf/any.proto`.
#   proto-any-type   — a reference to `google.protobuf.Any` (with or without a leading dot).
# The import check alone is sufficient (Layer 1's `buf build` rejects an `Any` reference
# without it); the type check makes the finding name the field. BOUND, stated honestly: a
# hand-rolled type-URL carrier (e.g. a `string type_url` + `bytes value` pair) is not
# detectable by grep; CONVENTIONS records that as part of the same revisit condition.
#
# VACUITY (proto-any-scan-vacuous): the proto file set must be non-empty, every file
# non-empty, no `/*` block comment outside a `//` comment (the line strip cannot see inside
# one), and every grep must exit 0 or 1. The vacuity text differs from the content text so a
# vacuity red is never "fixed" by loosening a pattern.
#
# NO SUPPRESSION PATH. A legitimate need for `Any` is a policy change with @protocol: remove
# MESSAGE_NO_DELETE from `breaking.except` and this guard together, in one reviewed commit.
#
# Emits NO `STATUS=` line: run-guards.sh owns STATUS emission. Exit 0 = clean, 1 = violation
# or vacuity. Messages go to stderr. Self-test: scripts/guards/validate-proto-no-any.test.sh
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # scripts/guards/simple
__real_root="$(cd "${__here}/../../.." && pwd)"

# shellcheck source=../common.sh
source "${__here}/../common.sh"
# Root seam (PROTO_NO_ANY_GUARD_ROOT), DEVLOOP_TEST-gated — rationale on guard_seam_root.
ROOT="$(guard_seam_root PROTO_NO_ANY_GUARD_ROOT "$__real_root")"

RULE='proto/buf.yaml excepts MESSAGE_NO_DELETE on the premise that no proto uses google.protobuf.Any (docs/protocol/CONVENTIONS.md §Enforcement)'
ROUTE='Using Any is a policy change with @protocol: remove MESSAGE_NO_DELETE from breaking.except and this guard together. Do not edit the guard to go green (it has no suppression path).'

fail=0
vacuous() {
  echo "VIOLATION [proto-any-scan-vacuous] $* — the guard could not evaluate its input, so it refuses to report clean. This is NOT a content violation: do not loosen a pattern; restore the input." >&2
  fail=1
}
violation() {   # $1 token, $2 file (root-relative), $3 offending statements
  echo "VIOLATION [$1] in ${2}:" >&2
  printf '%s\n' "$3" | sed 's/^/    /' >&2
  echo "  ${RULE}. ${ROUTE}" >&2
  fail=1
}

mapfile -t protos < <(find "${ROOT}/proto" -name '*.proto' -type f 2>/dev/null | sort)
if [[ "${#protos[@]}" -eq 0 ]]; then
  vacuous "no .proto files found under ${ROOT#"${__real_root}"}proto/"
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Same normalisation as validate-internal-proto-no-key-material.sh: join lines, collapse
# whitespace (and around `.`), break at `;` `{` `}`. Runs AFTER the `//` strip.
# Duplicated by crates/dt-guard/src/common/proto_lex.rs; port tracked in docs/TODO.md §Polyglot Pipeline Follow-ups.
normalise() {
  tr '\n' ' ' < "$1" \
    | sed -E 's/[[:space:]]*\.[[:space:]]*/./g; s/[[:space:]]+/ /g' \
    | tr ';{}' '\n\n\n'
}

# grep_or_vacuous <token> <pattern> <file> <rel>: violation on a match, vacuity on rc>1.
grep_or_vacuous() {
  local out rc
  out="$(grep -E "$2" "$3")" && rc=0 || rc=$?
  case "$rc" in
    0) violation "$1" "$4" "$out" ;;
    1) ;;
    *) vacuous "grep for ${1} failed on ${4} (rc=${rc})" ;;
  esac
}

for f in "${protos[@]}"; do
  rel="${f#"${ROOT}"/}"
  if [[ ! -s "$f" ]]; then
    vacuous "file is empty: ${rel}"
    continue
  fi
  sed 's://.*::' "$f" > "${work}/stripped"
  if grep -qF '/*' "${work}/stripped"; then
    vacuous "a '/*' block comment is present in ${rel}; the '//' line strip cannot see inside it. Use '//' comments"
    continue
  fi
  normalise "${work}/stripped" > "${work}/stmts"
  grep_or_vacuous proto-any-import \
    '^[[:space:]]*import\b[[:space:]]*(public|weak)?[[:space:]]*"google/protobuf/any\.proto"' \
    "${work}/stmts" "$rel"
  grep_or_vacuous proto-any-type \
    '(^|[^A-Za-z0-9_])google\.protobuf\.Any([^A-Za-z0-9_]|$)' \
    "${work}/stmts" "$rel"
done

exit "$fail"
