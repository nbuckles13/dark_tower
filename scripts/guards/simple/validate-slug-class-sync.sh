#!/usr/bin/env bash
# Guard: the THREE MANIFEST-SLUG CLASSES agree, character for character.
#
# The canonical class is `manifest::SLUG_PATTERN` in
# crates/dt-story/src/manifest.rs, enforced in the Slug newtype's
# deserializer. `/close-story` re-validates every slug it reads out of a
# manifest before constructing a filesystem path from it — defence in depth
# at an input boundary, since the manifest is a hand-editable markdown block
# regardless of who wrote it. That redundancy is deliberate and stays.
#
# What is NOT deliberate is the two classes DISAGREEING, which is what this
# guard exists to prevent: a producer that accepts a value the consumer
# rejects turns a validated write into a hard failure at close time, days
# later, in a different tool.
#
# SCOPE — read this before "unifying" anything. This guard covers exactly
# THREE literals of the NARROW manifest-slug class:
#   1. crates/dt-story/src/manifest.rs   SLUG_PATTERN          (canonical)
#   2. .claude/skills/close-story/SKILL.md   read-time regex   (consumer)
#   3. scripts/workflow/run-story.sh     SLUG_CLASS_CANONICAL  (write-time floor)
#
# Site 3 is the WRITE-TIME FLOOR — the gate deciding whether a slug enters the
# durable manifest — so drift there breaks in both directions and each
# direction reintroduces a failure the floor was added to prevent: if Rust
# widens and bash does not, the floor refuses a slug the manifest would accept
# and the completion is recorded with NO SLUG; if Rust narrows and bash does
# not, bash passes a slug the deserializer rejects and `complete --slug` exits
# 2 under `set -e` with the task's work already committed and its gates green.
#
# WHAT THIS GUARD DOES NOT PROVE: it pins the three literals as BYTE-IDENTICAL,
# which is not the same as the three MATCHERS being EQUIVALENT. Rust uses
# `is_ascii_lowercase()`/`is_ascii_digit()` (locale-independent); bash `[[ =~ ]]`
# uses POSIX ERE, where bracket ranges such as `[a-z]` are historically
# locale-sensitive and can match uppercase under some collations. Equivalence
# was verified separately on 2026-08-17 (@paired-protocol) across every locale
# present in the devloop container — `C`, `C.utf8`, `POSIX` — over `Abc`, `aBc`,
# `abc`, `a_b`, `a.b`, `É`: all agree, no exposure. Recorded so a future author
# does not read literal identity as proof of semantic identity; if either side's
# matcher implementation changes, that check has to be redone by hand.
#
# `scripts/workflow/run-story.sh`'s SLUG_CLASS_RESUME (`[0-9A-Za-z._-]+`) is a
# DELIBERATELY SEPARATE, WIDER class with a different job: guarding shell
# command-line interpolation of an ephemeral `--continue` value read off the
# filesystem, not manifest membership. Unifying it upward would silently WIDEN
# the manifest class, which is the opposite of the point.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../.."

RUST_SRC="${SLUG_SYNC_RUST_SRC:-crates/dt-story/src/manifest.rs}"
SKILL="${SLUG_SYNC_SKILL:-.claude/skills/close-story/SKILL.md}"
RUNNER="${SLUG_SYNC_RUNNER:-scripts/workflow/run-story.sh}"

fail() { echo "slug-class-sync: $*" >&2; exit 1; }

[ -f "$RUST_SRC" ] || fail "canonical source ${RUST_SRC} not found"
[ -f "$SKILL" ]    || fail "consumer ${SKILL} not found"
[ -f "$RUNNER" ]   || fail "producer ${RUNNER} not found"

# Canonical: `pub const SLUG_PATTERN: &str = "<class>";`
canonical="$(sed -n 's/^pub const SLUG_PATTERN: &str = "\(.*\)";$/\1/p' "$RUST_SRC")"
[ -n "$canonical" ] \
  || fail "could not extract SLUG_PATTERN from ${RUST_SRC} — the declaration moved or changed shape, so this guard is no longer reading the canonical class. Fix the extraction rather than deleting the guard."

# Consumer: the marked line in close-story's SKILL.md. Marked explicitly
# rather than grepped loosely, so unrelated regexes in that file cannot
# satisfy this check (a guard that greps, finds something else and reports
# success is the bug this pipeline exists to prevent).
consumer="$(sed -n 's/^<!-- slug-class-sync: \(.*\) -->$/\1/p' "$SKILL")"
[ -n "$consumer" ] \
  || fail "could not find the '<!-- slug-class-sync: ... -->' marker in ${SKILL}. /close-story must declare the slug class it validates against, so this guard can pin it to ${RUST_SRC}."

count="$(printf '%s\n' "$consumer" | grep -c . || true)"
[ "$count" -eq 1 ] \
  || fail "expected exactly 1 slug-class-sync marker in ${SKILL}, found ${count} — two declarations are two homes, which is what this guard prevents."

# Site 3: the runner's write-time floor.
runner="$(sed -n "s/^readonly SLUG_CLASS_CANONICAL='\(.*\)'$/\1/p" "$RUNNER")"
[ -n "$runner" ] \
  || fail "could not extract SLUG_CLASS_CANONICAL from ${RUNNER} — the declaration moved or changed shape, so this guard is no longer reading the runner's write-time floor. Fix the extraction rather than deleting the check."

rcount="$(printf '%s\n' "$runner" | grep -c . || true)"
[ "$rcount" -eq 1 ] \
  || fail "expected exactly 1 SLUG_CLASS_CANONICAL declaration in ${RUNNER}, found ${rcount}."

if [ "$canonical" != "$consumer" ]; then
  fail "slug classes DISAGREE.
  canonical (${RUST_SRC}): ${canonical}
  consumer  (${SKILL}): ${consumer}
A value accepted by the producer and rejected by the consumer fails at
/close-story time, days after it was written. Update the consumer to match
the canonical class; do not widen the canonical one to match the consumer."
fi

if [ "$canonical" != "$runner" ]; then
  fail "slug classes DISAGREE.
  canonical (${RUST_SRC}): ${canonical}
  runner    (${RUNNER}): ${runner}
This is the WRITE-TIME FLOOR. If it is wider than the canonical class the
runner writes slugs dt-story then rejects at deserialization; if it is
narrower the runner silently takes the NO-SLUG lane for slugs the crate would
have accepted. Update the runner to match the canonical class."
fi

exit 0
