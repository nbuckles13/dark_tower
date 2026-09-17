#!/usr/bin/env bash
# coverage-exempt: checks org-subdomain regex drift across source files, does not drive the dt-guard binary via $DT_GUARD
# validate-subdomain-regex-sync.test.sh — self-test for the org-subdomain drift guard
# (scripts/guards/simple/validate-subdomain-regex-sync.sh; story R-7, task #3).
#
# WHY THIS FILE EXISTS. The guard's whole value is in its FAILURE branches, and a passing
# guard exercises none of them. Every devloop and CI run proves only that the happy path
# returns 0 — which is precisely the shape of the bug the guard was written to prevent: a
# check that greps, finds nothing, compares nothing, and reports success. A guard whose
# negative branches never run is one typo away from being the vacuous pass it exists to stop,
# and it is the only thing standing between a loosened database CHECK and a browser-suite
# failure billed to the implementer lane (see the guard's own header).
#
# Every case drives the REAL guard against a SYNTHETIC tree through its DEVLOOP_TEST-gated
# `SUBDOMAIN_GUARD_ROOT` seam. Nothing here mutates the repo: the alternative — breaking a
# real encoding site to watch the guard notice — would red the pipeline to test the test.
#
# PLACEMENT IS DELIBERATE: this file lives in `scripts/guards/`, NOT `scripts/guards/simple/`.
# run-guards.sh discovers `simple/**/*.sh` by `find -name '*.sh'`, which matches `*.test.sh`
# too — so a self-test placed there would be auto-executed AS A GUARD on every run, with the
# repo path as $1, in addition to its wiring here. One test, two invocations, one of them
# semantically wrong. Kept out of the discovery glob instead, and wired explicitly at
# scripts/layer3.sh (there is no *.test.sh auto-runner).
#
# Consumes scripts/lang/_test_helpers.sh (assert_rc / assert_status / report_results).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"    # scripts/guards
REPO_ROOT="$(cd "${__here}/.." && pwd)"                   # scripts/
REPO_ROOT="$(cd "${REPO_ROOT}/.." && pwd)"                # repo root
GUARD="${__here}/simple/validate-subdomain-regex-sync.sh"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

# The guard exits non-zero on every case below; set -e must not abort the harness.
set +e

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

CANON='^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'

# Build a synthetic tree carrying exactly the ground truth the guard pins: the seven enumerated
# sites in their real delimiter forms, plus the three test occurrences, for a total of ten. The
# tree is `git init`ed and staged at the end (the guard's sweep is `git grep`, tracked-only).
# Each case then mutates ONE thing, so a failure names one cause; a case that adds a NEW file
# re-stages it.
mk_tree() {  # $1 = root
  local r="$1"
  mkdir -p "$r/migrations" "$r/infra/docker/postgres" "$r/infra/kind/scripts" \
           "$r/packages/sdk-core/src/validation" "$r/packages/web-app/e2e" \
           "$r/crates/env-tests/src/fixtures" "$r/scripts"
  printf "    CONSTRAINT subdomain_format CHECK (subdomain ~ '%s'),\n" "$CANON" \
    > "$r/migrations/20250118000001_initial_schema.sql"
  printf "    CONSTRAINT subdomain_format CHECK (subdomain ~ '%s')\n" "$CANON" \
    > "$r/infra/docker/postgres/init.sql"
  printf 'const SUBDOMAIN_REGEX = /%s/;\n' "$CANON" \
    > "$r/packages/sdk-core/src/validation/limits.ts"
  printf 'const SUBDOMAIN_REGEX = /%s/;\n' "$CANON" \
    > "$r/packages/web-app/e2e/env.ts"
  printf "    local subdomain_re='%s'\n" "$CANON" \
    > "$r/infra/kind/scripts/setup.sh"
  {
    printf 'const SUBDOMAIN_PATTERN: &str = r"%s";\n' "$CANON"
    printf '        assert_eq!(SUBDOMAIN_PATTERN, "%s");\n' "$CANON"
  } > "$r/crates/env-tests/src/fixtures/auth_client.rs"
  printf 'if [[ "$sub1" =~ %s ]]; then\n' "$CANON" \
    > "$r/scripts/layer7.test.sh"
  # This file's OWN copy of the literal. The guard counts it (it is an ordinary mirroring
  # consumer, like the two assertions above), so the fixture must carry it or the pristine
  # tree would be one occurrence short of the pinned total and every case would red for a
  # reason unrelated to what it tests.
  mkdir -p "$r/scripts/guards"
  printf "CANON='%s'\n" "$CANON" \
    > "$r/scripts/guards/validate-subdomain-regex-sync.test.sh"
  # docs/DATABASE_SCHEMA.md — enumerated, even though it lives under docs/. Only the dated
  # audit trail (devloop-outputs/, user-stories/) is excluded from the sweep.
  mkdir -p "$r/docs"
  printf "    CONSTRAINT subdomain_format CHECK (subdomain ~ '%s')\n" "$CANON" \
    > "$r/docs/DATABASE_SCHEMA.md"
  # The guard's step-(3) sweep is `git grep` — tracked files only, by construction (this is
  # the whole point of the tracked-only rewrite: a filesystem walk counted gitignored build
  # artifacts). So the synthetic fixture must be a git work tree with its content STAGED, or
  # `git grep` sees nothing and every case reds on the vacuity branch. No commit is needed —
  # `git grep` (no --cached) searches the working-tree content of tracked (indexed) files — and
  # no identity is needed, since `git add` requires none. Cases that add a NEW file after this
  # point must re-stage it (see case 4); modifications to already-tracked files are picked up
  # from the working tree with no re-add.
  #
  # `-Af` — FORCE past any global core.excludesFile (@security S4). Without it, a developer
  # whose global gitignore matches a fixture path (e.g. `*.md`, so DATABASE_SCHEMA.md) would
  # leave that file unstaged, dropping the pinned total below 10 and reding every case for a
  # reason unrelated to what it tests. Force-add makes the fixture hermetic against the host.
  git init -q "$r"
  git -C "$r" add -Af
}

# Run the guard against a tree. Sets global G_OUT / G_RC.
run_guard() {  # $1 = root
  G_OUT="$(DEVLOOP_TEST=1 SUBDOMAIN_GUARD_ROOT="$1" bash "$GUARD" 2>&1)"
  G_RC=$?
}

new_tree() {  # echoes a fresh populated root
  local r; r="$(mktemp -d "${WORK}/tree.XXXXXX")"
  mk_tree "$r"
  printf '%s' "$r"
}

# === (1) PRISTINE tree → PASS ================================================================
# The control. Without it, a guard that failed unconditionally would satisfy every negative
# case below and this file would certify it as working.
T="$(new_tree)"
run_guard "$T"
assert_rc     "pristine-exit0"  0 "$G_RC"
assert_status "pristine-status" "STATUS=OK REASON=subdomain-regex-in-sync" "$G_OUT"
assert_status "pristine-counts" "7 enumerated site(s) in sync; 10 total occurrence(s)" "$G_OUT"

# === (2) A SITE'S LITERAL DRIFTS → FAIL, naming that site ====================================
# The headline case: someone loosens the bound in one place. `{0,61}` -> `{0,62}` is a
# realistic edit that still looks like the same rule.
T="$(new_tree)"
printf 'const SUBDOMAIN_REGEX = /^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$/;\n' \
  > "$T/packages/web-app/e2e/env.ts"
run_guard "$T"
assert_rc     "drift-exit1"   1 "$G_RC"
assert_status "drift-status"  "STATUS=FAIL REASON=subdomain-regex-drift" "$G_OUT"
assert_status "drift-names-file" "packages/web-app/e2e/env.ts no longer contains the canonical pattern" "$G_OUT"

# === (3) A SITE IS RENAMED/DELETED → FAIL, NOT silence ======================================
# THE anti-vacuity case, and the reason this file exists. A guard that greps N files and
# compares what it finds would compare five identical strings instead of six here and go
# GREEN — the `*) exit 0` bug reappearing inside the guard built to prevent it. Zero matches
# must be indistinguishable from drift, because from the guard's position it is.
T="$(new_tree)"
rm -f "$T/packages/sdk-core/src/validation/limits.ts"
run_guard "$T"
assert_rc     "missing-exit1"  1 "$G_RC"
assert_status "missing-names-file" "enumerated site is MISSING: packages/sdk-core/src/validation/limits.ts" "$G_OUT"
# The partial-comparison guard fires too: five of six matching is not evidence of sync.
assert_status "missing-partial-comparison" "of 7 enumerated sites matched" "$G_OUT"

# === (4) A SEVENTH ENCODING APPEARS → FAIL until it is enumerated deliberately ================
# The site table alone cannot see a copy added somewhere new; the repo-wide total can. Growth
# must be a decision, not an accident.
T="$(new_tree)"
mkdir -p "$T/crates/gc-service/src"
printf 'const SUB: &str = r"%s";\n' "$CANON" > "$T/crates/gc-service/src/validate.rs"
# The sweep is `git grep`, which sees TRACKED files only — an unstaged new encoding would be
# invisible and this case would pass for the wrong reason. Force-stage it (past any host global
# gitignore, per mk_tree's -Af note), exactly as a real eighth copy committed into the tree.
git -C "$T" add -Af
run_guard "$T"
assert_rc     "seventh-exit1" 1 "$G_RC"
assert_status "seventh-count" "found 11 occurrences of the org-subdomain pattern in scanned source, expected exactly 10" "$G_OUT"
assert_status "seventh-names-new-file" "crates/gc-service/src/validate.rs" "$G_OUT"

# === (5) AN OCCURRENCE DISAPPEARS → FAIL (the count is pinned in BOTH directions) ============
# A test assertion that pinned the literal being quietly dropped is drift too: the encoding
# stops being checked and nothing else notices.
T="$(new_tree)"
printf 'if [[ "$sub1" =~ ^[a-z0-9-]+$ ]]; then\n' > "$T/scripts/layer7.test.sh"
run_guard "$T"
assert_rc     "removed-exit1" 1 "$G_RC"
assert_status "removed-count" "found 9 occurrences of the org-subdomain pattern in scanned source, expected exactly 10" "$G_OUT"

# === (6) THE GUARD'S OWN EXTRACTION BREAKS → FAIL LOUDLY, never a silent pass ================
# An empty tree yields zero hits, at which point every comparison in the guard is trivially
# satisfied. That is the guard's own vacuous-pass mode and it must be a hard failure with a
# message saying so — "no violations found" and "I could not look" are different answers.
# It is a git work tree with NOTHING staged: `git grep` over an empty index returns zero hits
# cleanly, exercising the "found zero, extraction is broken" branch rather than the "not a git
# repo" one — and a fresh init also shadows any repo the temp dir might sit inside, so the case
# does not depend on $WORK being outside version control.
T="$(mktemp -d "${WORK}/empty.XXXXXX")"
git init -q "$T"
run_guard "$T"
assert_rc     "vacuous-exit1"    1 "$G_RC"
assert_status "vacuous-named"    "the guard's own extraction has broken" "$G_OUT"
assert_status "vacuous-not-evidence" "it is not evidence that the sites are in sync" "$G_OUT"

# === (7) THE PINNED COUNT AND THE TABLE MUST AGREE ==========================================
# Driven by copying the guard and bumping only EXPECTED_SITE_COUNT, which is the half-edit
# this branch exists to catch: a number changed without the row that justifies it. Comparing
# the table to its own length would be circular and could never catch this.
T="$(new_tree)"
SKEWED="${WORK}/guard-skewed.sh"
sed 's/^readonly EXPECTED_SITE_COUNT=7$/readonly EXPECTED_SITE_COUNT=8/' "$GUARD" > "$SKEWED"
# Prove the injection actually took, or this case tests the unmodified guard and passes for
# the wrong reason.
if grep -q 'EXPECTED_SITE_COUNT=8' "$SKEWED"; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[count-skew-fixture] could not inject a skewed EXPECTED_SITE_COUNT into a copy of the guard — the constant was renamed or reformatted, so this case is VACUOUS until the sed is repaired")
fi
skew_out="$(DEVLOOP_TEST=1 SUBDOMAIN_GUARD_ROOT="$T" bash "$SKEWED" 2>&1)"; skew_rc=$?
assert_rc     "count-skew-exit1"  1 "$skew_rc"
assert_status "count-skew-named"  "site table holds 7 entries but EXPECTED_SITE_COUNT is 8" "$skew_out"

# === (8) SEAM INERTNESS: SUBDOMAIN_GUARD_ROOT is IGNORED without DEVLOOP_TEST ================
# The seam redirects what the guard examines, so honoring it in production would be a
# silent-validation-disable lever: point it at an empty tree and the guard reports OK having
# examined nothing. Asserted behaviourally — with the sentinel UNSET and the override pointing
# at an empty directory, the guard must still scan the REAL repo and pass. (It passing is
# meaningful here precisely because case 6 proved an empty tree FAILS.)
empty="$(mktemp -d "${WORK}/inert.XXXXXX")"
# `env -u DEVLOOP_TEST` — the sentinel's ABSENCE is CONSTRUCTED, never inherited
# (@code-reviewer F1). Asserting inertness by merely *not setting* the variable made this case
# depend on the caller's environment: under an ambient DEVLOOP_TEST=1 the guard honored the
# override, scanned an empty tree, and red with "the guard's own extraction has broken" — an
# unattributable red naming the wrong cause, in the exact class this story exists to remove.
# The precedent is layer7.test.sh's resolve_seam_var, which builds the sentinel state with
# `env -i` + `${2:+DEVLOOP_TEST=...}` for precisely this reason. Measured before the fix:
# `DEVLOOP_TEST=1 bash <this file>` → 20 passed / 2 failed; with `env -u` → 22 / 0 either way.
inert_out="$(env -u DEVLOOP_TEST SUBDOMAIN_GUARD_ROOT="$empty" bash "$GUARD" 2>&1)"; inert_rc=$?
assert_rc     "seam-inert-exit0"  0 "$inert_rc"
assert_status "seam-inert-scanned-real-repo" "7 enumerated site(s) in sync" "$inert_out"

# === (9) REPO_ROOT IS NOT A GIT TOP LEVEL → FAIL, naming THAT (not "found zero") =============
# The sweep is `git grep`, and `git -C` resolves UPWARD, so a REPO_ROOT that is not itself a
# git top level would scan the wrong tree (a subdirectory of an enclosing repo) or nothing at
# all — surfacing as a bare "found zero" that names the wrong cause. The guard's precondition
# (@security S2) must catch this. Proven FAIL-CLOSED: the fixture carries all ten real
# occurrences ON DISK, then has its `.git` stripped — so a filesystem walk (the old grep -rn)
# WOULD have found and counted them, but the tracked-only guard must red because git cannot see
# them, and must say so ("not the top level of a git work tree"), NOT falsely report zero. This
# is the case that distinguishes "the content is missing" from "I could not look".
T="$(new_tree)"                 # populated AND git-init'd by mk_tree...
rm -rf "$T/.git"                # ...then de-repo'd: 10 occurrences on disk, no git to see them
run_guard "$T"
assert_rc     "nogit-exit1"        1 "$G_RC"
assert_status "nogit-named"        "is not the top level of a git work tree" "$G_OUT"
assert_status "nogit-not-evidence" "it is not evidence that the sites are in sync" "$G_OUT"
# It must NOT pass, and must NOT masquerade as a genuine in-sync result despite the files being
# physically present — the whole point of naming the precondition instead of "found zero".
assert_absent "nogit-not-ok"       "STATUS=OK" "$G_OUT"

report_results "scripts/guards/validate-subdomain-regex-sync.test.sh"
