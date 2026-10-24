#!/usr/bin/env bash
# coverage-exempt: drives validate-proto-no-any.sh (a shell guard) against fixture trees; it never runs the dt-guard binary via $DT_GUARD
# Self-test for validate-proto-no-any.sh.
#
# In-tree the guard always passes, so its failure branches are the only part with any value:
# each is exercised here against a COPY of the real proto/ tree with one edit, via the
# PROTO_NO_ANY_GUARD_ROOT seam (honoured only under DEVLOOP_TEST=1). Every red case asserts
# both the exit code and the token, so a vacuity red cannot stand in for a content red.
#
# Deliberately NOT under guards/simple/: run-guards.sh discovers with `find -name '*.sh'`,
# so a self-test there would be auto-run AS A GUARD. Invoked from scripts/layer3.sh.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

GUARD="scripts/guards/simple/validate-proto-no-any.sh"
TARGET="dark_tower/internal/v1/internal.proto"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
rc_total=0
n=0

# expect <name> <expected-rc> <expected-token|-> <root>
expect() {
  local name="$1" want_rc="$2" token="$3" root="$4" err rc
  n=$((n + 1))
  set +e
  err="$(DEVLOOP_TEST=1 PROTO_NO_ANY_GUARD_ROOT="$root" "$GUARD" 2>&1 >/dev/null)"
  rc=$?
  set -e
  if [[ "$rc" -ne "$want_rc" ]]; then
    echo "FAIL ${name}: expected rc ${want_rc}, got ${rc}: ${err}" >&2
    rc_total=1
  elif [[ "$token" != "-" && "$err" != *"VIOLATION [${token}]"* ]]; then
    echo "FAIL ${name}: rc ${rc} but token '${token}' absent: ${err}" >&2
    rc_total=1
  else
    echo "ok   ${name}"
  fi
}

# fixture <name> <text-appended-to-internal.proto> -> root. Fails the self-test if the edit
# did not land (no vacuous fixture).
fixture() {
  local root="${WORK}/$1"
  mkdir -p "$root"
  cp -r proto "$root/"
  printf '%s\n' "$2" >> "${root}/proto/${TARGET}"
  grep -qF -- "$(printf '%s' "$2" | head -n1)" "${root}/proto/${TARGET}" \
    || { echo "FAIL fixture $1: edit did not apply" >&2; rc_total=1; }
  printf '%s' "$root"
}

# POSITIVE CONTROL: the real tree is clean AND the guard demonstrably had files to read.
count="$(find proto -name '*.proto' -type f | wc -l)"
if [[ "$count" -ge 2 ]]; then echo "ok   real-tree-has-protos (${count})"; else
  echo "FAIL real-tree-has-protos: found ${count}" >&2; rc_total=1; fi
expect real-tree-clean 0 - "$PWD"

# CONTENT reds.
expect import-plain 1 proto-any-import "$(fixture import-plain 'import "google/protobuf/any.proto";')"
expect import-public 1 proto-any-import "$(fixture import-public 'import public "google/protobuf/any.proto";')"
expect import-weak 1 proto-any-import "$(fixture import-weak 'import weak "google/protobuf/any.proto";')"
expect import-no-space 1 proto-any-import "$(fixture import-no-space 'import"google/protobuf/any.proto";')"
expect type-field 1 proto-any-type "$(fixture type-field 'message ProbeAny { google.protobuf.Any payload = 1; }')"
expect type-leading-dot 1 proto-any-type "$(fixture type-leading-dot 'message ProbeAny { .google.protobuf.Any payload = 1; }')"
expect type-repeated 1 proto-any-type "$(fixture type-repeated 'message ProbeAny { repeated google.protobuf.Any payload = 1; }')"
expect type-map-value 1 proto-any-type "$(fixture type-map-value 'message ProbeAny { map<string, google.protobuf.Any> payload = 1; }')"
expect type-split-lines 1 proto-any-type "$(fixture type-split-lines 'message ProbeAny {
  google . protobuf
    .Any payload = 1;
}')"

# GREEN look-alikes: prose in a `//` comment, and a longer identifier.
expect comment-mention-only 0 - "$(fixture comment-mention-only '// google.protobuf.Any and google/protobuf/any.proto are banned here')"
expect longer-identifier 0 - "$(fixture longer-identifier 'message ProbeAny { google.protobuf.AnyThing payload = 1; }')"

# VACUITY reds.
expect block-comment 1 proto-any-scan-vacuous "$(fixture block-comment '/* import "google/protobuf/any.proto"; */')"
empty_root="${WORK}/no-protos"; mkdir -p "${empty_root}/proto"
expect no-proto-files 1 proto-any-scan-vacuous "$empty_root"
empty_file_root="$(fixture empty-file '// placeholder')"
: > "${empty_file_root}/proto/${TARGET}"
expect empty-proto-file 1 proto-any-scan-vacuous "$empty_file_root"

# The seam is inert without DEVLOOP_TEST=1: pointed at a red fixture, the guard reads the real tree.
n=$((n + 1))
if PROTO_NO_ANY_GUARD_ROOT="${WORK}/import-plain" "$GUARD" 2>/dev/null; then
  echo "ok   seam-inert-without-devloop-test"
else
  echo "FAIL seam-inert-without-devloop-test: the seam was honoured outside DEVLOOP_TEST=1" >&2
  rc_total=1
fi

echo "validate-proto-no-any.test.sh: ${n} cases, $([[ "$rc_total" -eq 0 ]] && echo all passed || echo FAILURES above)"
exit "$rc_total"
