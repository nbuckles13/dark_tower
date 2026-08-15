#!/usr/bin/env bash
# validate-subdomain-regex-sync.sh — org-subdomain pattern drift guard (story R-7, task #3).
#
# THE PROBLEM. The DNS-label rule for `organizations.subdomain` is encoded in SEVEN places
# across four languages plus the schema doc. Every one of the copies has been independently
# ruled *required*: they sit at distinct trust boundaries (database CHECK, shell validator
# before a SQL boundary, two client-side config-time validators, a Rust fixture, the schema
# specification doc), and collapsing them
# would remove defense-in-depth rather than duplication. CLAUDE.md's rule for exactly this
# situation is "derive one from the other, **or add a guard that fails validation on drift**".
# This is that guard: it converts SEVEN copies into one ENFORCED encoding.
#
# WHY IT MATTERS, CONCRETELY. Since R-7, a malformed org subdomain throws at Playwright
# CONFIG-LOAD (packages/web-app/e2e/env.ts is imported by playwright.config.ts), so the
# browser suite exits non-zero and Layer 7 records `FAIL browser-e2e-failed`, **exit 1,
# IMPLEMENTER lane**. Loosen the database CHECK without the `env.ts` mirror and you get an
# organization the database accepts and the browser suite rejects — a provisioning-input
# defect billed to the diff. That is R-4's exact misattribution class, inside the story about
# that confusion. Drift here is not hypothetical; it has a demonstrated path to a wrong lane.
#
# ANTI-VACUITY — the binding design condition (@test). A guard that greps N files and
# compares whatever it happens to find PASSES TRIVIALLY when a file is renamed and its
# extraction returns nothing: it would compare six identical strings instead of seven and go
# green. That is the `*) exit 0` vacuous-pass bug reappearing inside the guard built to
# prevent it. So this guard:
#   1. pins an EXACT literal site count, independent of the table it checks;
#   2. FAILS LOUDLY if any single enumerated site yields ZERO matches (missing file, renamed
#      file, edited literal — all indistinguishable from "nothing to check", so all red);
#   3. pins an EXACT total-occurrence count over a repo-wide sweep, so ADDING AN EIGHTH
#      encoding reds until the table and the counts are deliberately updated. Growth must be
#      a decision, not an accident.
#
# SSoT: `migrations/20250118000001_initial_schema.sql`'s `subdomain_format` CHECK. The
# ANCHOR-comment chain is UNEVEN and that is why this guard, not the comments, is the
# enforcement: only `crates/env-tests/src/fixtures/auth_client.rs` names the migration
# directly; `packages/sdk-core/src/validation/limits.ts` anchors to R-11 in
# `docs/user-stories/2026-05-02-browser-client-join.md`; `packages/web-app/e2e/env.ts`
# anchors to limits.ts; and the two SQL files plus `infra/kind/scripts/setup.sh` carry no
# ANCHOR comment at all. Comments are navigation, and they drift; the SITES table below is
# the inventory of record. AC's own
# `crates/ac-service/src/middleware/org_extraction.rs::extract_subdomain` is an EIGHTH,
# hand-rolled, weaker variant (character predicates, no length bound). It is deliberately NOT
# enumerated here — it encodes no pattern literal, so there is nothing to compare — and it is
# recorded rather than silently ignored so a future reader does not conclude it was missed.
# Do not add a NINTH style: an invented-but-equivalent rule is uncheckable against anything.
#
# Wired into Layer 3 two ways: auto-discovered by scripts/guards/run-guards.sh (which
# scripts/layer3.sh invokes), and its own self-test is wired explicitly at scripts/layer3.sh
# — the negative branches below cannot be exercised by the guard merely passing.
#
# Exit: 0 on all-sync; 1 on any drift, missing site, or count mismatch.

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # scripts/guards/simple
__real_root="$(cd "${__here}/../../.." && pwd)"

# Scan-root seam, DEVLOOP_TEST-gated — same trust boundary as scripts/layer7.sh's seams and
# for the same reason. Redirecting the root at a tree with no violations is a
# SILENT-VALIDATION-DISABLE lever: the guard would report OK having examined nothing this
# repo cares about. So it is honored ONLY under the test sentinel, which
# `assert_no_ci_sentinel_leak` (run at the top of layer3.sh and layer-all.sh) independently
# reds if it ever leaks into CI. The self-test needs it because the guard's FAILURE branches
# can only be driven against a synthetic tree — driving them against the real one would mean
# breaking the repo to test the guard.
if [[ "${DEVLOOP_TEST:-}" == "1" ]]; then
  REPO_ROOT="${SUBDOMAIN_GUARD_ROOT:-$__real_root}"
else
  REPO_ROOT="$__real_root"
fi

# The canonical literal, byte for byte. Sourced from the migration's CHECK.
readonly CANONICAL='^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'

# Enumerated sites. Each entry is `<path>|<expected fixed-string fragment>`; the fragment
# includes the site's own DELIMITERS, so a pattern that merely CONTAINS the canonical text
# (e.g. `/^…$|.*/`, which would accept everything) does not satisfy it.
readonly SITES=(
  "migrations/20250118000001_initial_schema.sql|CHECK (subdomain ~ '${CANONICAL}')"
  "infra/docker/postgres/init.sql|CHECK (subdomain ~ '${CANONICAL}')"
  "packages/sdk-core/src/validation/limits.ts|const SUBDOMAIN_REGEX = /${CANONICAL}/;"
  "packages/web-app/e2e/env.ts|const SUBDOMAIN_REGEX = /${CANONICAL}/;"
  "infra/kind/scripts/setup.sh|local subdomain_re='${CANONICAL}'"
  "crates/env-tests/src/fixtures/auth_client.rs|const SUBDOMAIN_PATTERN: &str = r\"${CANONICAL}\";"
  # docs/DATABASE_SCHEMA.md is ENUMERATED despite living under docs/ (@dry-reviewer). The
  # docs/** carve-out below is about dated AUDIT RECORDS; this file opens "This document
  # defines the data models for PostgreSQL" — a live spec mirror, and it carries the pattern
  # in the same delimiter form as the migration. It is also already demonstrating the failure
  # mode: last touched 2025-11-22 while the migrations moved on. An unenforced doc mirror
  # drifts, which is this guard's entire thesis.
  "docs/DATABASE_SCHEMA.md|CHECK (subdomain ~ '${CANONICAL}')"
)

# PINNED INDEPENDENTLY of the array's own length — comparing the array to itself would be
# circular and would pass no matter how many rows were added or dropped.
readonly EXPECTED_SITE_COUNT=7

# Total occurrences of the canonical literal in scanned source. Seven enforced sites plus three
# TEST occurrences that pin the literal in place: crates/env-tests/.../auth_client.rs's
# in-crate byte-identity assertion, scripts/layer7.test.sh's DNS-label case, and this guard's
# own self-test fixture (scripts/guards/validate-subdomain-regex-sync.test.sh). Those three
# are counted but not enumerated — they assert the pattern rather than gate on it, so a change
# there is caught by the total while the delimiter-exact table stays about enforcement.
#
# The self-test's copy is COUNTED rather than excluded, deliberately. Only THIS file is
# excluded, and only because counting its own `CANONICAL` definition would make the pinned
# number self-referential. The self-test is an ordinary consumer that mirrors the literal,
# exactly like the two assertions above, and it is self-checking besides: if its copy drifted,
# its synthetic fixture would stop matching the delimiter-exact SITES table and its pristine
# case would red.
readonly EXPECTED_TOTAL_OCCURRENCES=10

# EXCLUSIONS ARE NARROWED TO THE AUDIT TRAIL, not to docs/ wholesale (@dry-reviewer).
# `docs/devloop-outputs/**` and `docs/user-stories/**` are dated records that quote the pattern
# as it stood on a given day; rewriting them when the rule changes would destroy the audit trail
# the repo keeps on purpose, so they are out. Everything else under docs/ IS scanned — notably
# `docs/DATABASE_SCHEMA.md`, which opens "This document defines the data models for PostgreSQL"
# and is therefore a live spec mirror, not a historical record. Excluding all of docs/ was the
# earlier and lazier cut, and it left the one doc where drift actually matters unenforced.
readonly SWEEP_EXCLUDES=(
  --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target
  --exclude-dir=dist --exclude-dir=devloop-outputs --exclude-dir=user-stories
  --exclude=validate-subdomain-regex-sync.sh
)

violations=0
violation() { printf 'VIOLATION: %s\n' "$*"; violations=$((violations + 1)); }

printf 'Org-subdomain pattern drift guard\n'
printf '  canonical: %s\n' "$CANONICAL"
printf '  SSoT:      migrations/20250118000001_initial_schema.sql (subdomain_format CHECK)\n\n'

# --- (1) The pinned count must match the table ------------------------------------------
# Catches the half-edit in BOTH directions: a row added without bumping the count, and a
# count bumped without adding the row.
if [[ "${#SITES[@]}" -ne "$EXPECTED_SITE_COUNT" ]]; then
  violation "site table holds ${#SITES[@]} entries but EXPECTED_SITE_COUNT is ${EXPECTED_SITE_COUNT} — update both together, deliberately"
fi

# --- (2) Every enumerated site must yield AT LEAST ONE exact match -----------------------
# Zero matches is a FAILURE, never "nothing to check". A renamed or deleted file, and a file
# whose literal was edited, are indistinguishable from here — and all three are drift.
found_sites=0
for entry in "${SITES[@]}"; do
  path="${entry%%|*}"
  fragment="${entry#*|}"
  abs="${REPO_ROOT}/${path}"
  if [[ ! -f "$abs" ]]; then
    violation "enumerated site is MISSING: ${path} (renamed or deleted?) — the pattern it carried is now unchecked; update this guard's table in the same commit that moves the file"
    continue
  fi
  hits="$(grep -c -F -- "$fragment" "$abs" || true)"
  if [[ "$hits" -eq 0 ]]; then
    violation "${path} no longer contains the canonical pattern in its expected form. Expected to find: ${fragment}"
    printf '  what is there now:\n'
    grep -n -F -- 'a-z0-9' "$abs" | head -3 | sed 's/^/    /' || true
  else
    found_sites=$((found_sites + 1))
    printf '  OK  %s\n' "$path"
  fi
done

if [[ "$found_sites" -ne "$EXPECTED_SITE_COUNT" ]]; then
  violation "only ${found_sites} of ${EXPECTED_SITE_COUNT} enumerated sites matched — a partial comparison is NOT evidence of sync"
fi

# --- (3) Repo-wide sweep: no unenumerated encoding, and no drifted one -------------------
# The table alone cannot see a SEVENTH copy someone adds elsewhere. This sweep can, and it
# reds until the count and the table are updated together.
# OPTION ORDER IS LOAD-BEARING, and the reason is the `--` END-OF-OPTIONS TERMINATOR, not any
# grep dialect. After `--`, EVERY remaining argument is an operand by definition — so
# `grep -rn -F -- "$PAT" "$ROOT" --exclude-dir=docs` parses the exclusion as a PATH TO SEARCH.
# GNU grep 3.8 (what actually runs here) and ugrep both behave this way; both also permute
# options fine when there is no `--`. Measured on a synthetic tree:
#     grep -rn -F     NEEDLE /tmp/pt --exclude-dir=docs   -> exclusion honored,  rc 0
#     grep -rn -F --  NEEDLE /tmp/pt --exclude-dir=docs   -> exclusion IGNORED,  rc 2
#     grep -rn --exclude-dir=docs -F -- NEEDLE /tmp/pt    -> exclusion honored,  rc 0
# The rule: put every flag BEFORE the `--`, and reserve `--` for when the pattern may begin
# with `-` (it can here — the canonical pattern starts with `^`, but a future one might not).
# An earlier revision of this comment blamed ugrep for "not permuting"; that was measured in a
# Claude Code agent shell where `grep` is a shell FUNCTION shimming ugrep, and is false for the
# `/usr/bin/grep` every script, layer and CI job actually gets. Corrected in place because this
# is the site a future scanner author copies from.
mapfile -t sweep_hits < <(
  grep -rn --binary-files=without-match "${SWEEP_EXCLUDES[@]}" -F -- "$CANONICAL" "$REPO_ROOT" \
    2>/dev/null || true
)
total="${#sweep_hits[@]}"

if [[ "$total" -eq 0 ]]; then
  # The extractor finding NOTHING is the guard's own vacuous-pass mode: with zero hits every
  # comparison below is trivially satisfied. Treat it as a hard failure of the guard itself.
  violation "the repo-wide sweep found ZERO occurrences of the canonical pattern — the guard's own extraction has broken (wrong REPO_ROOT='${REPO_ROOT}', or grep semantics changed). This guard is VACUOUS until repaired; it is not evidence that the sites are in sync"
elif [[ "$total" -ne "$EXPECTED_TOTAL_OCCURRENCES" ]]; then
  violation "found ${total} occurrences of the org-subdomain pattern in scanned source, expected exactly ${EXPECTED_TOTAL_OCCURRENCES}"
  printf '  every occurrence found:\n'
  printf '    %s\n' "${sweep_hits[@]#"${REPO_ROOT}/"}"
  printf '  If you ADDED an encoding: add it to SITES (with its delimiters) and bump BOTH\n'
  printf '  counts. If you REMOVED one: drop the row and lower the counts. Either way the\n'
  printf '  edit is deliberate — that is the point of pinning a literal number.\n'
fi

printf '\n'
if [[ "$violations" -eq 0 ]]; then
  printf '%d enumerated site(s) in sync; %d total occurrence(s) in scanned source.\n' \
    "$found_sites" "$total"
  printf 'STATUS=OK REASON=subdomain-regex-in-sync\n'
  exit 0
fi

printf '%d violation(s).\n' "$violations"
printf 'The org-subdomain rule is defense-in-depth at four separate trust boundaries plus the\n'
printf 'schema doc that specifies it; the\n'
printf 'copies are deliberate and this guard is what keeps them honest. Change the SSoT\n'
printf '(migrations/20250118000001_initial_schema.sql) and every mirror in ONE commit.\n'
printf 'STATUS=FAIL REASON=subdomain-regex-drift\n'
exit 1
