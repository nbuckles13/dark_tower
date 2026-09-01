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
#   3. pins an EXACT total-occurrence count over a tracked-file sweep, so ADDING AN EIGHTH
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
# The sweep runs over GIT-TRACKED files only (see step (3) below), so build artifacts,
# node_modules, target/ and dist/ need NO denylist — they are gitignored, never tracked, and
# excluded by construction. The only exclusions that remain are TRACKED content that quotes
# the pattern without being an enforced encoding:
#   - `docs/devloop-outputs/**` and `docs/user-stories/**` are dated records that quote the
#     pattern as it stood on a given day; rewriting them when the rule changes would destroy
#     the audit trail the repo keeps on purpose, so they are out. They ARE tracked and DO carry
#     the literal (verified: excluding them is exactly what keeps the pinned total at 10).
#   - this guard's own file, whose `readonly CANONICAL=` line would otherwise self-count.
# Everything else tracked IS scanned — notably `docs/DATABASE_SCHEMA.md`, which opens "This
# document defines the data models for PostgreSQL" and is therefore a live spec mirror, not a
# historical record; it is enumerated above. Excluding all of docs/ was the earlier and lazier
# cut, and it left the one doc where drift actually matters unenforced.
#
# These are git PATHSPECS (`:(exclude)…`), passed after `--` to `git grep`; they are NOT grep
# `--exclude`/`--exclude-dir` flags. Directory forms end in `/` so they match everything under.
readonly SWEEP_EXCLUDES=(
  ':(exclude)docs/devloop-outputs/'
  ':(exclude)docs/user-stories/'
  ':(exclude)scripts/guards/simple/validate-subdomain-regex-sync.sh'
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

# --- (3) Tracked-file sweep: no unenumerated encoding, and no drifted one -----------------
# The table alone cannot see a SEVENTH copy someone adds elsewhere. This sweep can, and it
# reds until the count and the table are updated together.
#
# THE SWEEP IS GIT-TRACKED-ONLY, by construction. `git grep` searches the working-tree content
# of the files git tracks and NOTHING else — so a gitignored build artifact can never be
# counted: `packages/sdk-core/coverage/**/limits.ts.html`, `.nx/cache/**` copies of limits.ts,
# and every future generated reflection of a source site are all untracked and simply absent
# from the scan. This closes BOTH failure modes of the `grep -rn` filesystem walk with a
# `--exclude-dir` denylist that this guard shipped with:
#   - the false POSITIVE — a run after `pnpm/nx test` generated coverage/.nx copies of the
#     sdk-core SUBDOMAIN_REGEX reds the guard on phantom hits (observed 10 -> 13 under
#     /usr/bin/grep, which walks the filesystem regardless of .gitignore); and, because the
#     total is pinned by EXACT EQUALITY,
#   - the false NEGATIVE a denylist structurally CANNOT close — an untracked artifact adding
#     +1 masks a deleted tracked pin (-1), nets back to 10, and passes SILENTLY, losing a real
#     encoding. A denylist is whack-a-mole on the first and blind to the second; tracked-only
#     is immune to both, and to build-state nondeterminism besides.
# It NARROWS coverage on purpose, and the trade is right for a merge gate: a new copy is seen
# once it is STAGED/committed; an UNSTAGED working-tree addition is out of scope by construction
# (nothing unstaged reaches main, and CI scans committed content). MODIFICATIONS to an
# already-tracked file are still caught from the working tree with no re-staging — `git grep`
# reads working-tree content for tracked paths — so a loosened literal in a committed site reds
# immediately, which is the case that actually matters.
#
# Flags: `-e "$CANONICAL"` keeps the leading `^` from being read as an option; `-F` is
# fixed-string; `-I` skips binary files (the old `--binary-files=without-match`). NO `:/`
# anchor pathspec: `git grep` then scans from REPO_ROOT downward, and the `:(exclude)…` audit-
# trail pathspecs are resolved relative to that same REPO_ROOT — anchoring the scan and the
# carve-out at ONE place. (`:/` would anchor the scan at the enclosing repo's top level while
# the excludes stayed REPO_ROOT-relative, so a REPO_ROOT below the top level would scan wider
# than it excludes and the carve-out would evaporate — @security S1.) This matches the old
# `grep -rn … "$REPO_ROOT"` scoping exactly. `git grep` prints REPO_ROOT-relative paths, so no
# prefix stripping is needed. It counts matching LINES, one per hit — exactly as the old
# `grep -rn`/mapfile did, and no enumerated site carries two occurrences on one line, so
# lines == occurrences here.
#
# PRECONDITION (@security S2): REPO_ROOT must be the TOP LEVEL of a git work tree. `git -C`
# resolves UPWARD, so a REPO_ROOT that is a subdirectory of some repo (or not a repo at all)
# would otherwise scan the wrong tree, or nothing, and surface as a bare "found zero". Enforce
# it as code, not prose: require the resolved toplevel to BE REPO_ROOT. This splits "git is
# broken / wrong root" from "git works and the tree genuinely has zero copies" (the vacuity
# branch), so a failure names its real cause, and it holds S1's single-anchor property by
# construction rather than by the fixture comments merely talking around it.
sweep_hits=()
total=0
# The `-ef` is load-bearing — do NOT simplify it to `[[ -n "$__sweep_top" ]]` or a string
# compare. `rev-parse` SUCCEEDING is not the property we need: for a REPO_ROOT that is a
# SUBDIRECTORY of a repo it succeeds and returns the ENCLOSING repo's toplevel. Requiring the
# resolved toplevel to BE REPO_ROOT (same device+inode, symlink-safe) is what rejects that
# subdirectory case — the S1 hazard — and what keeps self-test case 9 sound even if the test's
# $WORK ever lands inside a repo (rev-parse would then succeed and `-ef` would still red).
if ! __sweep_top="$(git -C "$REPO_ROOT" rev-parse --show-toplevel 2>/dev/null)" \
     || [[ ! "$__sweep_top" -ef "$REPO_ROOT" ]]; then
  violation "REPO_ROOT='${REPO_ROOT}' is not the top level of a git work tree (git absent, not a repo, or a subdirectory of one) — the tracked-file sweep cannot run, so this guard is VACUOUS until the root is corrected; it is not evidence that the sites are in sync"
else
  mapfile -t sweep_hits < <(
    git -C "$REPO_ROOT" grep -I -n -F -e "$CANONICAL" -- "${SWEEP_EXCLUDES[@]}" \
      2>/dev/null || true
  )
  total="${#sweep_hits[@]}"

  if [[ "$total" -eq 0 ]]; then
    # The extractor finding NOTHING is the guard's own vacuous-pass mode: with zero hits every
    # comparison below is trivially satisfied. REPO_ROOT is a valid git tree (the precondition
    # passed), so the pattern is genuinely absent from tracked source — a broken extraction, not
    # evidence of sync. Treat it as a hard failure of the guard itself.
    violation "the tracked-file sweep found ZERO occurrences of the canonical pattern — the guard's own extraction has broken (REPO_ROOT='${REPO_ROOT}' is a git work tree, but the pattern is absent from every tracked file). This guard is VACUOUS until repaired; it is not evidence that the sites are in sync"
  elif [[ "$total" -ne "$EXPECTED_TOTAL_OCCURRENCES" ]]; then
    violation "found ${total} occurrences of the org-subdomain pattern in scanned source, expected exactly ${EXPECTED_TOTAL_OCCURRENCES}"
    printf '  every occurrence found:\n'
    printf '    %s\n' "${sweep_hits[@]}"
    printf '  If you ADDED an encoding: add it to SITES (with its delimiters) and bump BOTH\n'
    printf '  counts. If you REMOVED one: drop the row and lower the counts. Either way the\n'
    printf '  edit is deliberate — that is the point of pinning a literal number.\n'
  fi
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
