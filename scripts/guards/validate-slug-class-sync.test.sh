#!/usr/bin/env bash
# Self-test for validate-slug-class-sync.sh.
#
# A sync guard without a DRIFT CASE cannot fail, and is indistinguishable
# from no guard at all. This file exists so the guard's failure path is
# proven rather than assumed — this story's §Deferred already names "10 of 16
# scripts/**/*.test.sh files have no invocation site" as a live gap, so the
# invocation in layer3.sh is as load-bearing as the assertions here.
#
# Deliberately NOT under guards/simple/: run-guards.sh discovers with
# `find -name '*.sh'`, which matches `*.test.sh` too, so a self-test placed
# there would be auto-run AS A GUARD. Same reasoning as
# validate-subdomain-regex-sync.test.sh.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

GUARD="scripts/guards/simple/validate-slug-class-sync.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
rc_total=0

check() { # check <name> <expected-rc> <actual-rc>
  if [ "$2" -ne "$3" ]; then
    echo "FAIL ${1}: expected rc ${2}, got ${3}" >&2
    rc_total=1
  else
    echo "ok   ${1}"
  fi
}

mk_rust()   { printf 'pub const SLUG_PATTERN: &str = "%s";\n' "$1" > "$2"; }
mk_skill()  { printf 'prose\n<!-- slug-class-sync: %s -->\nmore prose\n' "$1" > "$2"; }
mk_runner() { printf "readonly SLUG_CLASS_CANONICAL='%s'\n" "$1" > "$2"; }

# Third seam is the RUNNER's write-time floor. Defaults to an in-sync file so
# the pre-existing cases keep testing what they were written to test.
run_guard() {
  local runner="${3:-}"
  if [ -z "$runner" ]; then
    runner="${WORK}/default-runner"
    mk_runner '^[a-z0-9]+(-[a-z0-9]+)*$' "$runner"
  fi
  set +e
  SLUG_SYNC_RUST_SRC="$1" SLUG_SYNC_SKILL="$2" SLUG_SYNC_RUNNER="$runner" "$GUARD" >/dev/null 2>&1
  local rc=$?
  set -e
  printf '%s' "$rc"
}

# (a) In sync -> pass.
mk_rust  '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/a.rs"
mk_skill '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/a.md"
check "in-sync-passes" 0 "$(run_guard "$WORK/a.rs" "$WORK/a.md")"

# (b) DRIFT -> fail. The case the guard exists for.
mk_rust  '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/b.rs"
mk_skill '^[a-z0-9-]+$'             "$WORK/b.md"
check "drift-fails" 1 "$(run_guard "$WORK/b.rs" "$WORK/b.md")"

# (c) Consumer marker missing -> fail, not silently pass. A guard that finds
#     nothing and reports success is the failure this pipeline is built on.
mk_rust '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/c.rs"
printf 'prose with no marker\n' > "$WORK/c.md"
check "missing-marker-fails" 1 "$(run_guard "$WORK/c.rs" "$WORK/c.md")"

# (d) Canonical declaration moved/renamed -> fail loudly rather than
#     comparing an empty string against an empty string and passing.
printf 'pub const SOMETHING_ELSE: &str = "x";\n' > "$WORK/d.rs"
mk_skill '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/d.md"
check "canonical-missing-fails" 1 "$(run_guard "$WORK/d.rs" "$WORK/d.md")"

# (e) Two consumer markers -> fail. Two declarations are two homes.
mk_rust '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/e.rs"
{ printf '<!-- slug-class-sync: ^[a-z0-9]+(-[a-z0-9]+)*$ -->\n'
  printf '<!-- slug-class-sync: ^[a-z0-9]+(-[a-z0-9]+)*$ -->\n'; } > "$WORK/e.md"
check "duplicate-marker-fails" 1 "$(run_guard "$WORK/e.rs" "$WORK/e.md")"

# (g) RUNNER DRIFT -> fail. The write-time floor is the site whose drift is
#     worst: if it is wider than canonical the runner writes slugs dt-story
#     then rejects at deserialization; if narrower it silently takes the
#     NO-SLUG lane for slugs the crate would accept. Case (b) covers the skill
#     side only, so without this a regression in the runner extraction passes.
mk_rust   '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/g.rs"
mk_skill  '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/g.md"
mk_runner '^[a-z0-9-]+$'             "$WORK/g.sh"
check "runner-drift-fails" 1 "$(run_guard "$WORK/g.rs" "$WORK/g.md" "$WORK/g.sh")"

# (h) Runner declaration renamed/moved -> fail loudly rather than comparing an
#     empty string and passing. Same vacuity guard as (d), third seam.
mk_rust   '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/h.rs"
mk_skill  '^[a-z0-9]+(-[a-z0-9]+)*$' "$WORK/h.md"
printf "readonly SOMETHING_ELSE='x'\n" > "$WORK/h.sh"
check "runner-missing-fails" 1 "$(run_guard "$WORK/h.rs" "$WORK/h.md" "$WORK/h.sh")"

# (f) The REAL in-tree pair must be in sync.
set +e
"$GUARD" >/dev/null 2>&1
real_rc=$?
set -e
check "real-tree-in-sync" 0 "$real_rc"

exit "$rc_total"
