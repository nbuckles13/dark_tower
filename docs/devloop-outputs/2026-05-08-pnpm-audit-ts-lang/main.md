# Devloop Output: pnpm audit always-run + TS language directory

**Date**: 2026-05-08
**Task**: R-62 ADR-0033 Wave 1 #2 — `scripts/lang/ts/{changed.sh, changed.test.sh, audit.sh}` with `pnpm audit --audit-level=high` always-run; `.github/workflows/ci.yml` calls `scripts/layer-all.sh` end-to-end. Closes minimatch-class incident permanently.
**Specialist**: infrastructure
**Mode**: Agent Teams (full, `--paired-with=security`)
**Branch**: `feature/browser-client-join-task33`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a570ee7ad439653e130b34f60c46ad3d30bd0e04` |
| Branch | `feature/browser-client-join-task33` |
| User Story | `docs/user-stories/2026-05-02-browser-client-join.md` (task #33) |
| Depends On | task #32 (commit `a570ee7`, ADR-0033 Wave 1 #1 — pipeline scaffolding) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-08-pnpm-audit-ts-lang` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security (paired) | `security@devloop-2026-05-08-pnpm-audit-ts-lang` |
| Test | `test@devloop-2026-05-08-pnpm-audit-ts-lang` |
| Observability | `observability@devloop-2026-05-08-pnpm-audit-ts-lang` |
| Code Quality | `code-reviewer@devloop-2026-05-08-pnpm-audit-ts-lang` |
| DRY | `dry-reviewer@devloop-2026-05-08-pnpm-audit-ts-lang` |
| Operations | `operations@devloop-2026-05-08-pnpm-audit-ts-lang` |

### Plan Confirmation Tracking

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed (Gate 1 ACK: threshold + no-pass-through; trailer requested) |
| Test | confirmed (with FYI on _test_changed_predicates Wave 2 deferral; scope flag on cargo build→check) |
| Observability | confirmed |
| Code Quality | confirmed (after root-files §2 alignment fix accepted) |
| DRY | confirmed |
| Operations | confirmed (two operational divergences noted; UX hedge suggested for ci.yml step name) |

### Gate 3 Verdict Tracking

| Reviewer | Verdict | Findings (count) | Notes |
|----------|---------|------------------|-------|
| Security (paired) | **CLEAR** | 0 (1 optional micro-nit not adopted; trailer recommended) | All 4 security-policy items re-verified; Ownership Lens for `lang/ts/audit.sh` Minor-judgment confirmed; `Approved-Cross-Boundary: security` trailer recommended on commit |
| Test | **CLEAR** | 0 | All 5 test-coverage concerns satisfied; `changed.test.sh` 10/10 PASS local re-run |
| Observability | **CLEAR** | 0 | All 4 checkpoints verified against on-disk code; deferral framing accepted within obs lens |
| Code Quality | **CLEAR** | 0 | All ADR compliance items PASS; mirror-fidelity vs `lang/rust/*.sh` excellent; Ownership Lens for Minor-judgment edit confirmed |
| DRY | **CLEAR** | 0 | 0 true-duplication findings; 1 extraction observation appended to `docs/TODO.md` § Cross-Service Duplication (locality-self-test harness, defer until Wave 2 #5 or 3rd lang) |
| Operations | **CLEAR** | 0 | All 7 checkpoints PASS; UX hedge landed at ci.yml:69-71; runbook stub explicitly deferred to task #39 |

---

## Task Overview

### Objective

Wire the always-run `pnpm audit --audit-level=high` gate into the polyglot
validation pipeline introduced by task #32, by:

1. Creating the `scripts/lang/ts/` language directory with:
   - `changed.sh` — exit 0 if TS touched (packages/**, package.json,
     pnpm-lock.yaml, pnpm-workspace.yaml, tsconfig*.json, .npmrc), exit 1 if
     untouched. Mirrors the `lang/rust/changed.sh` predicate shape.
   - `changed.test.sh` — locality self-test matrix (mirrors
     `lang/rust/changed.test.sh`).
   - `audit.sh` — `pnpm audit --audit-level=high` always-run, deliberately
     no `"$@"` pass-through (mirrors `lang/rust/audit.sh` per the security
     finding from task #32).
2. Updating `.github/workflows/ci.yml` to call `scripts/layer-all.sh`
   end-to-end so CI uses the same router as local `verify-completion.sh`.
   This closes the router-drift between local devloop and CI that allowed
   the minimatch incident to persist undetected.

### Scope

- **Service(s)**: tooling only (`scripts/lang/ts/`, `.github/workflows/`); no
  Rust services touched.
- **Schema**: No.
- **Cross-cutting**: Yes — CI workflow change affects all PRs.

### Debate Decision

NOT NEEDED — implementation strategy fully specified by ADR-0033 §3
(always-run principle), §6 (per-language wrapper shape), §11
(audit-level threshold owned by security), and the parallel anchor in
`scripts/lang/rust/audit.sh` (committed in task #32). The
`--paired-with=security` overlay is the codified collaboration channel for
the §11 threshold ownership.

### Why minimatch closes here

The minimatch-class incident (ADR-0033 §1: "3 high-severity transitive
ReDoS vulns in nx@20.3.0's minimatch chain were latent until the first TS
task ran the audit ad-hoc"). Two structural fixes land here:

1. `pnpm audit --audit-level=high` runs always-run via Layer 6 (audit) —
   advisory DB updates surface independently of diff state.
2. CI calls the same `scripts/layer-all.sh` orchestrator that local devloops
   call — eliminates router-drift where local pipeline could detect a
   regression CI would silently skip.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/ts/changed.sh` | Mine | — |
| `scripts/lang/ts/changed.test.sh` | Mine | — |
| `scripts/lang/ts/audit.sh` | Not mine, Minor-judgment | security |
| `.github/workflows/ci.yml` | Mine | — |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine | — |

**Classification rationale**:

- `lang/ts/audit.sh` — the wrapper plumbing is infrastructure's domain
  (ADR-0033 §11), but the **`--audit-level=high` threshold value is
  security policy** (ADR-0033 §11: "security owns the policy thresholds").
  Classified Minor-judgment with security as paired collaborator + Gate 1
  + Gate 3 confirmer. Not Mechanical because the threshold encodes a
  security-policy decision (what severity blocks CI), not a structure-
  preserving substitution.
- `.github/workflows/ci.yml` — CI pipeline is enumerated as
  infrastructure-owned in `docs/specialist-knowledge/infrastructure/INDEX.md`.
  This change is structural (workflow now invokes `scripts/layer-all.sh`
  end-to-end), not a content change to job semantics — Mine.
- No paths fall inside ADR-0024 §6.4 Guarded Shared Areas (no `proto/**`,
  no `crates/common/**`, no `db/migrations/**`, no auth/crypto primitives).

---

## Planning

### Approach

Mirror the shape of `scripts/lang/rust/{changed.sh, changed.test.sh, audit.sh}` for
the new `scripts/lang/ts/` directory; replace the `test`-job inline-cargo block in
`.github/workflows/ci.yml` with a single `scripts/layer-all.sh` call (so CI uses
the same router as local devloop's `verify-completion.sh`). Plumb the existing
`DATABASE_URL`/`AC_MASTER_KEY`/`AC_BIND_ADDRESS` env vars to the layer-all step,
and add Node 22 + pnpm@10.33.2 install to the test job's setup so Layer 6
(`pnpm audit`) runs in CI.

The four scripts plus the ci.yml change are sized to fit the dispatcher's
existing contracts (lint-at-startup `changed.sh` requirement,
`DEVLOOP_DISPATCH_ALWAYS_RUN=1` from `scripts/audit.sh`, `_dispatch.sh`'s
`langs+=("$name")` autodiscovery for any non-underscore subdirectory).

### Near-final script bodies

#### `scripts/lang/ts/changed.sh`

```bash
#!/usr/bin/env bash
# TS changed-classifier: exit 0 if TS touched, 1 if untouched.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_changed_helpers.sh"
diff_touches_path "packages/" \
  || diff_touches_root_files \
       "package.json" "pnpm-lock.yaml" "pnpm-workspace.yaml" \
       "nx.json" "tsconfig.base.json" ".nvmrc"
```

Notes:
- Root-files list mirrors **ADR-0033 §2 verbatim** (line 128). All six exist in
  the workspace today: `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`,
  `nx.json` (Nx task graph + target defaults), `tsconfig.base.json` (workspace
  tsconfig anchor), `.nvmrc` (Node version pin). `nx.json` and `.nvmrc` are
  load-bearing for ADR-0033 §3 "when in doubt, always-run": an Nx config change
  or Node version bump must re-trigger TS verbs (Layer 1/2/4/5) — silent skip
  here is exactly the gap §3 exists to close. (Initial draft erroneously listed
  `tsconfig.json`/`.npmrc` instead — neither exists in the workspace; corrected
  per code-reviewer Gate 1 finding.)
- `diff_touches_path "packages/"` covers everything under `packages/**` (current
  workspace globs `packages/*` per `pnpm-workspace.yaml`; the prefix arm is
  intentionally one level broader so future nested workspace re-orgs don't slip
  through).
- `diff_touches_root_files` is fixed-string per `_changed_helpers.sh` security
  finding 2, so a `tsconfig*.json` glob isn't possible at this seam — the
  workspace ships only `tsconfig.base.json` today. If a root `tsconfig.json` is
  added later, the predicate update lands as a one-line edit here.
- Cache-driven (sources `_changed_helpers.sh`); no diff-of-its-own. Same shape as
  rust's two-arm OR.

#### `scripts/lang/ts/changed.test.sh`

```bash
#!/usr/bin/env bash
# changed.test.sh — locality self-test for lang/ts/changed.sh.
#
# Mirrors the rust self-test layout — small representative path set fired
# against the predicate via injected DEVLOOP_TMP cache. Catches local-only
# regressions; full cross-language drift detection lives in
# _test_changed_predicates.sh (Wave 1 only asserts rust there; ts column TBD).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PASS=0
FAIL=0
FAILURES=()

assert_rc() {
  local label="$1" expected="$2" actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected_rc=${expected} actual_rc=${actual}")
  fi
}

run_with_cache() {
  local content="$1"
  local tmp; tmp=$(mktemp -d)
  printf '%s\n' "$content" > "${tmp}/changed-files.layer-locality"
  local rc=0
  env -i \
      PATH="$PATH" \
      HOME="$HOME" \
      DEVLOOP_TMP="$tmp" \
      DEVLOOP_LAYER=locality \
      "${__here}/changed.sh" >/dev/null 2>&1 || rc=$?
  rm -rf "$tmp"
  printf '%s\n' "$rc"
}

# Touched cases (exit 0). Each touched assertion exercises a distinct
# arm of the predicate so the locality test catches drift on any one of them.
assert_rc "packages/foo/src/index.ts"  0 "$(run_with_cache "packages/foo/src/index.ts")"  # packages/ prefix arm
assert_rc "package.json"               0 "$(run_with_cache "package.json")"               # root manifest
assert_rc "pnpm-lock.yaml"             0 "$(run_with_cache "pnpm-lock.yaml")"             # lockfile
assert_rc "pnpm-workspace.yaml"        0 "$(run_with_cache "pnpm-workspace.yaml")"        # workspace globs
assert_rc "nx.json"                    0 "$(run_with_cache "nx.json")"                    # Nx task graph / generators
assert_rc "tsconfig.base.json"         0 "$(run_with_cache "tsconfig.base.json")"         # workspace tsconfig anchor
assert_rc ".nvmrc"                     0 "$(run_with_cache ".nvmrc")"                     # Node toolchain pin

# Untouched cases (exit 1).
assert_rc "docs/x.md"                  1 "$(run_with_cache "docs/x.md")"
assert_rc "crates/foo/src/lib.rs"      1 "$(run_with_cache "crates/foo/src/lib.rs")"
assert_rc "scripts/test.sh"            1 "$(run_with_cache "scripts/test.sh")"

printf '\nlang/ts/changed.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
  exit 1
fi
exit 0
```

#### `scripts/lang/ts/audit.sh` ← **paired with security at Gate 1 + Gate 3**

```bash
#!/usr/bin/env bash
# TS audit: pnpm audit --audit-level=high (always-run per ADR-0033 §3 + §6).
# Wave 1 ships threshold=high — security owns the policy threshold (ADR-0033 §11).
#
# IMPORTANT (security finding mirrored from lang/rust/audit.sh): we deliberately
# do NOT pass "$@" through to pnpm audit. Threshold and ignore-list edits are
# security's domain (ADR-0033 §11) and MUST land via a tracked audit-config file
# in a follow-up, not via ad-hoc CLI flags. Allowing CLI pass-through would let a
# caller silence advisories at runtime (e.g. `scripts/audit.sh --audit-level=critical`
# or `--ignore=GHSA-...`) without leaving a trace in the layer log — bypassing the
# always-run gate. If a future caller needs to pass legitimate args (e.g.
# `--registry=...` for a private mirror), add an explicit allowlist of safe flags
# rather than blanket pass-through.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
run_and_emit "pnpm-audit" pnpm audit --audit-level=high
```

The threshold value `high` is the security-policy decision; everything else is
mechanical mirroring of `lang/rust/audit.sh`. Soliciting @security ack on the
literal string `--audit-level=high` plus the no-pass-through stance.

#### `.github/workflows/ci.yml` — `test` job change (structural)

Replace these five inline steps in the `test` job:

```yaml
- name: Check formatting
  run: cargo fmt --all -- --check

- name: Run simple guards
  run: ./scripts/guards/run-guards.sh --verbose
  env:
    GUARD_DIFF_BASE: ${{ github.event.pull_request.base.sha }}

- name: Run Clippy
  run: cargo clippy --workspace -- -D warnings

- name: Run tests
  env:
    DATABASE_URL: postgresql://postgres:postgres@localhost:5432/dark_tower_test
    AC_MASTER_KEY: AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=
    AC_BIND_ADDRESS: 127.0.0.1:8082
  run: cargo test --workspace --verbose

- name: Build binaries
  run: cargo build --workspace
```

with two new setup steps + one orchestrator step:

```yaml
- name: Install Node.js
  uses: actions/setup-node@v4
  with:
    node-version: '22'

- name: Install pnpm
  uses: pnpm/action-setup@v4
  with:
    version: 10.33.2
    run_install: false

- name: Run polyglot validation pipeline
  env:
    DATABASE_URL: postgresql://postgres:postgres@localhost:5432/dark_tower_test
    AC_MASTER_KEY: AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=
    AC_BIND_ADDRESS: 127.0.0.1:8082
    GUARD_DIFF_BASE: ${{ github.event.pull_request.base.sha }}
  run: ./scripts/layer-all.sh
```

Coverage job is left intact (orthogonal to ADR-0033 always-run set per §4).
`pnpm install` is **not** invoked in CI — `pnpm audit` reads `pnpm-lock.yaml`
directly via the registry, so we don't need `node_modules` materialized to run
the always-run advisory gate.

### Key planning anchors (for implementer + reviewers)

- **Mirror, don't invent**: `lang/rust/audit.sh` is the parallel anchor for
  `lang/ts/audit.sh`. Same shape, same security guard against `"$@"`
  pass-through.
- **`changed.sh` semantics**: TS-touched ≡ any of `packages/**`,
  `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `tsconfig*.json`,
  or `.npmrc` (when present) modified. Source the same
  `_changed_helpers.sh` primitives (`diff_touches_path`,
  `diff_touches_root_files`).
- **Always-run vs skip-if-untouched**: `audit.sh` is dispatched via
  `scripts/audit.sh` with `DEVLOOP_DISPATCH_ALWAYS_RUN=1`, so it runs
  regardless of `changed.sh` outcome (ADR-0033 §3). `changed.sh` still
  needs to exist + be executable for the dispatcher's lint-at-startup
  check.
- **CI workflow change**: replace the inline `cargo fmt --check`,
  `run-guards.sh`, `cargo clippy`, `cargo test`, `cargo build` steps with a
  single `scripts/layer-all.sh` invocation. Coverage job stays separate
  (ADR-0033 §4 only governs the always-run pipeline; coverage is
  orthogonal). Need to plumb `DATABASE_URL`, `AC_MASTER_KEY`,
  `AC_BIND_ADDRESS` env vars to layer-all.sh's child processes.

---

## Pre-Work

None. Task #32 (commit `a570ee7`) landed all dispatcher + layer skeleton
prerequisites.

---

## Implementation Summary

Landed exactly the four artifacts the plan called for, plus the user-story
tracking-row update:

1. **`scripts/lang/ts/changed.sh`** — TS-touched predicate, two-arm OR
   (`packages/` prefix || ADR-0033 §2 root-files). Sources
   `_changed_helpers.sh`. 9 lines, executable.
2. **`scripts/lang/ts/changed.test.sh`** — locality self-test, 7 touched + 3
   untouched cases (one assertion per predicate arm, per code-reviewer Gate 1
   feedback). Hermetic via injected `DEVLOOP_TMP` cache. Executable.
3. **`scripts/lang/ts/audit.sh`** — `run_and_emit "pnpm-audit" pnpm audit
   --audit-level=high`. No `"$@"` pass-through; full security comment block
   mirrored from `lang/rust/audit.sh` adapted for pnpm (registry override
   example, not RustSec ignore-id). 17 lines, executable.
4. **`.github/workflows/ci.yml`** — `test` job: replaced 5 inline cargo steps
   (`fmt --check`, `run-guards.sh`, `clippy`, `test`, `build`) with three new
   steps: `actions/setup-node@v4` (Node 22) + `pnpm/action-setup@v4`
   (10.33.2 to match the `packageManager` pin) + a single
   `./scripts/layer-all.sh` step plumbing `DATABASE_URL`, `AC_MASTER_KEY`,
   `AC_BIND_ADDRESS`, `GUARD_DIFF_BASE`. Coverage job intact (orthogonal to
   ADR-0033 always-run set per §4). Operations UX hedge from team-lead's
   Gate-1 advisory included as a comment on the layer-all step pointing PR
   authors at `./scripts/layer6.sh` for local repro of pnpm audit failures.
5. **`docs/user-stories/2026-05-02-browser-client-join.md`** Devloop Tracking
   row #33: Status `Pending` → `Completed`, Devloop Output cell filled with
   the path to this file.

### Plan-revision history

- **Gate 1 (code-reviewer finding, accepted in full)**: original draft
  `changed.sh` root-files list diverged from ADR-0033 §2 line 128 — included
  non-existent `tsconfig.json` and `.npmrc`, omitted existing `nx.json` and
  `.nvmrc`. Corrected to match §2 verbatim before implementation; locality
  self-test cases re-shaped from 5 touched to 7 touched (one per arm) so the
  test catches future drift on each individual root file.
- **Security trailer (Gate 1 advisory from team-lead)**: commit landing
  `lang/ts/audit.sh` will carry `Approved-Cross-Boundary: security ADR-0033 §11
  threshold=high paired-collab` per ADR-0024 §6.7.

---

## Files Modified

```
 .github/workflows/ci.yml                                              |  29 +-
 docs/devloop-outputs/2026-05-08-pnpm-audit-ts-lang/main.md            | NEW
 docs/user-stories/2026-05-02-browser-client-join.md                   |   2 +-
 scripts/lang/ts/audit.sh                                              |  17 + (NEW)
 scripts/lang/ts/changed.sh                                            |   9 + (NEW)
 scripts/lang/ts/changed.test.sh                                       |  62 + (NEW)
```

### Key Changes by File

| File | Changes |
|------|---------|
| `scripts/lang/ts/changed.sh` | NEW — TS-touched predicate |
| `scripts/lang/ts/changed.test.sh` | NEW — locality self-test matrix |
| `scripts/lang/ts/audit.sh` | NEW — `pnpm audit --audit-level=high` always-run |
| `.github/workflows/ci.yml` | REPLACE inline cargo steps with `scripts/layer-all.sh` invocation |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Update Devloop Tracking row #33: Status=Completed, Devloop Output filled |

---

## Devloop Verification Steps

Implementer (Gate 2) ran the targeted self-tests + audit-dispatcher path
that exercise the new lang/ts/ wiring. Layer-by-layer narration below; the
canonical pipeline (`./scripts/verify-completion.sh --layer full`) is in
the Appendix and will be re-run by the harness at the validation gate.

### Locality self-test: `lang/ts/changed.test.sh`
**Status**: PASS (10/10 — 7 touched + 3 untouched).
**Command**: `./scripts/lang/ts/changed.test.sh`
**Output**: `lang/ts/changed.test.sh: 10 passed, 0 failed`. Each touched
arm of the predicate fired green; each untouched negative case correctly
returned exit 1.

### Cross-language meta-test: `_test_changed_predicates.sh`
**Status**: PASS (13/13). `STATUS=OK REASON=predicate-meta-test-passed`.
This confirms the lang/ts/ addition did not regress the rust column of
the predicate fixture matrix.

### Regression: `lang/rust/changed.test.sh`
**Status**: PASS (5/5). Confirms task #32's behavior preserved.

### Dispatcher self-tests
- `_dispatch.test.sh`: PASS (7/7). Confirms `for_each_lang_with_verb` still
  enumerates all non-underscore lang directories (now `rust` + `ts`),
  lint-at-startup passes for every lang's `changed.sh` (both are executable),
  and aggregation precedence holds.
- `_common.test.sh`: PASS (20/20). `STATUS=OK REASON=common-tests-passed`.

### Layer 6 (audit dispatcher path) — exercises new `lang/ts/audit.sh`

**TS arm**: PASS — `STATUS=OK REASON=pnpm-audit-passed`. The dispatcher
discovered `lang/ts/audit.sh` via `for_each_lang_with_verb` (which iterates
`lang/*/`), invoked it with `DEVLOOP_DISPATCH_ALWAYS_RUN=1` (set by
`scripts/audit.sh`), and the command returned 0. The `minimatch` override
in `package.json` does its job: the lockfile carries no high-or-above
advisory. **This is the part this task is responsible for, and it's green.**

**Rust arm**: FAIL — `STATUS=FAIL REASON=cargo-audit-failed`. Six
pre-existing advisories on the `wtransport 0.1.14` chain plus one on
`sqlx-mysql`'s rsa transitive. These are **not introduced by this task**
— `Cargo.lock` is unchanged in this devloop (last touched in `d918343`).
They became visible because Layer 6 audit is now always-run per ADR-0033
§3 + §10; that's the property the polyglot pipeline exists to enforce.
The full Gate 2 harness run from team-lead surfaced the precise advisory
list:

| Advisory | Crate (version) | Severity | Fix |
|----------|-----------------|----------|-----|
| RUSTSEC-2026-0037 | `quinn-proto 0.10.6` | high (DoS) | `>=0.11.14` (req. wtransport 0.2.x) |
| RUSTSEC-2025-0009 | `ring 0.16.20` | (AES panic) | `>=0.17.12` (req. wtransport 0.2.x) |
| RUSTSEC-2026-0098 | `rustls-webpki 0.101.7` | (req. wtransport 0.2.x) |
| RUSTSEC-2026-0099 | `rustls-webpki 0.101.7` | (req. wtransport 0.2.x) |
| RUSTSEC-2026-0104 | `rustls 0.21.12` (chain) | (req. wtransport 0.2.x) |
| RUSTSEC-2023-0071 | `rsa 0.9.x` via `sqlx-mysql` | medium (Marvin attack) | no fix; sqlx feature trim or accept |

**Aggregated dispatcher status**: `STATUS=FAIL REASON=audit-some-lang-failed`,
exit code 1 (correct §6 worst-child precedence: rust FAIL > ts OK).

#### Deferral (review-protocol.md §Fix-or-Defer)

This task **defers** the cargo-audit FAIL with the following justification:

- **Valid deferral criteria met**:
  - "Requires changing files outside the PR's changeset" — bumping
    `wtransport` or landing a tracked `cargo-audit` allowlist touches
    `Cargo.toml` / `Cargo.lock` and (for the wtransport path)
    `crates/mc-service/`, `crates/mh-service/`, `crates/env-tests/`,
    plus the `crates/common/src/webtransport/**` Guarded Shared Area —
    none of which are in this PR's stated scope (per the
    Cross-Boundary Classification table above).
  - "Needs cross-service coordination" — the `crates/common/src/webtransport/**`
    GSA path requires security + protocol + MC + MH co-signed work per
    ADR-0024 §6.4. That's a separate devloop, not iteration on this one.
  - "Pre-existing condition unchanged by this diff" — confirmed by
    `git diff` showing no `Cargo.lock` modification.

- **Spin-out plan — 2 devloops, A/B/C cluster split** (per Lead 2026-05-08):
  the 9 lockfile rows surfaced (6 errors + 3 unmaintained warnings) cluster
  along three axes; the spin-out is structured as two devloops, not one,
  to keep blast-radius and review burden proportionate to each cluster.

  - **Cluster A** = wtransport 0.1 → 0.7 major version bump. Touches
    `mc-service` / `mh-service` / `env-tests` TLS setup, accept loops, and
    Identity handling (`Identity::self_signed`, `Identity::load_pemfiles`,
    accept-loop signatures, dangerous-configuration cert pinning). Closes
    **5 of 9 rows**: RUSTSEC-2026-0037 (quinn-proto DoS), RUSTSEC-2025-0009
    (ring AES panic), RUSTSEC-2025-0010 (ring unmaintained), and the two
    RUSTSEC-2025-0134 entries (rustls-pemfile 1.0.4 + 2.2.0 unmaintained).
    Real engineering — 5 major versions of API change.

  - **Cluster B** = `cargo update -p rustls-webpki@0.103.10`. Closes
    **3 of 9 rows**: RUSTSEC-2026-0099, RUSTSEC-2026-0104, RUSTSEC-2026-0098
    (all rustls-webpki). Surgical lockfile bump; TLS-stack so still earns
    real review.

  - **Cluster C** = `rsa@0.9.10` ignore in audit config with rationale.
    Closes **1 of 9 rows**: RUSTSEC-2023-0071 (rsa via sqlx-mysql;
    build-time-only via `sqlx-macros-core`, runtime tree empty, no fix
    available upstream). Pure config decision, security-owned per
    ADR-0033 §11. **Policy text of record** lives in `docs/TODO.md`
    § Supply-Chain Audit Debt (Devloop I entry); it specifies
    `cargo tree -p rsa --invert` as the verification command, a
    **90-day sunset (re-evaluate by 2026-08-08)** for active
    re-justification, and a **fail-closed condition**: if any runtime
    driver starts pulling `rsa::*`, the ignore expires immediately —
    no extension. The ADR-0033 §12 14-day MTTR tripwire continues to
    apply to this entry as light periodic re-verification of the
    "still build-time-only, still no fix" invariant — distinct from
    the 90-day sunset (heavy active re-justification); both fire,
    both add value, they don't conflict. Cluster C is expected to be
    the longest-lived entry in `audit-config.toml`; the TODO.md text
    anchors that expectation in rationale so the 2026-08-08
    re-justification doesn't read as a process failure.

  - **Devloop I (small, fast — ~3-4 hours)**: Cluster B + Cluster C.
    `cargo update -p rustls-webpki@0.103.10` (closes 3 rows) plus the
    rsa@0.9.10 ignore entry (closes 1 row). Specialist:
    `infrastructure --paired-with=security`. Closes 4 of 9 rows.
    Intermediate state: CI Layer 6 stays red on the remaining 5 (Cluster A)
    until Devloop II lands. **Security pre-condition for Gate 3 sign-off
    on Devloop I**: resolved `rustls` / `rustls-webpki` / `quinn` version
    pins in `Cargo.lock` must be reviewed before merge — "surgical"
    depends on the bump not unintentionally pulling the rustls 0.22+
    chain widening.

  - **Devloop II (separate, multi-day)**: Cluster A. Workspace
    `wtransport = "0.1"` → `"0.7"` bump; updates across mc-service +
    mh-service + env-tests as listed above. Specialist:
    `infrastructure --paired-with=meeting-controller --paired-with=media-handler --paired-with=security`.
    Add `protocol` if the framed-envelope handshake is touched. Closes the
    remaining 5 of 9 rows. **Security has approved starting with
    `/debate` first** — they will be active in the debate (cert-verification
    + WT handshake = §11 / ADR-0023 territory). The dry-run survey ("here's
    what changes; how do we want to sequence?") prevents a stuck implementer.

  Both devloops recorded in `docs/TODO.md` § Dependency Vulnerabilities
  (cargo audit) under the Wave 1 #2 re-surface block referencing this
  devloop output.

- **CI consequence acknowledged**: until **both** spin-out devloops land,
  every PR's CI run will Layer 6 RED. ADR-0033 §10 explicitly accepts this
  as the alternative to silent bypass; confirmed acceptable. After
  Devloop I lands, the residual is 5 rows (Cluster A only); after Devloop II
  lands, Layer 6 returns to green.

- **Security ACK** (relayed via team-lead 2026-05-08): deferral
  approved. Cluster C policy text finalized and lives in
  `docs/TODO.md` § Supply-Chain Audit Debt (Devloop I entry) as the
  policy-of-record per ADR-0033 §11 (security owns the threshold +
  ignore-list). ADR-0033 §12 14-day MTTR tripwire continues to apply
  per security's explicit decision (no special-casing for the rsa
  entry). This devloop's commit framing unchanged from prior
  Gate 1 ACK: `Approved-Cross-Boundary: security ADR-0033 §11
  threshold=high paired-collab` trailer per ADR-0024 §6.7. Cluster C
  is its own spin-out per ADR-0024 §6.3 owner-implements (security
  implements the audit-config landing in Devloop I), separate from
  this devloop's commit.

#### Full Gate 2 harness pipeline (team-lead-run)

| Layer | Result | Duration | Notes |
|-------|--------|----------|-------|
| 1 (compile) | SKIPPED-NO-DIFF | 0s | No Rust diff (correct) |
| 2 (fmt) | SKIPPED-NO-DIFF | 0s | No Rust diff (correct) |
| 3 (guards + meta-test) | OK | 7s | 22/22 simple guards PASS; predicate meta-test PASS |
| 4 (test) | SKIPPED-NO-DIFF | 0s | No Rust diff (correct) |
| 5 (lint) | SKIPPED-NO-DIFF | 0s | No Rust diff (correct) |
| 6 (audit) | FAIL | 1s | TS arm OK; Rust arm FAIL (pre-existing — deferred above) |
| 7 (semantic) | N/A | 0s | wave2-pending |

### Layer 8: Env-tests
**Status**: SKIPPED-NO-DIFF (confirmed by Gate 2 harness — no Rust service or
`infra/kubernetes/` change in this task).

---

## Code Review Results

TBD — populated at Gate 3.

### Security Specialist (paired)
**Verdict**: TBD

### Test Specialist
**Verdict**: TBD

### Observability Specialist
**Verdict**: TBD

### Code Quality Reviewer
**Verdict**: TBD

### DRY Reviewer
**Verdict**: TBD

### Operations Reviewer
**Verdict**: TBD

---

## Tech Debt References

TBD — appended at completion.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `a570ee7ad439653e130b34f60c46ad3d30bd0e04`
2. Review all changes: `git diff a570ee7..HEAD`
3. Soft reset (preserves changes): `git reset --soft a570ee7`
4. Hard reset (clean revert): `git reset --hard a570ee7`
5. No schema/migration changes — `git reset` is sufficient.
6. CI workflow rollback: GitHub Actions picks up the new YAML on next push;
   reverting the YAML restores the previous inline-step pipeline.

---

## Issues Encountered & Resolutions

TBD.

---

## Lessons Learned

TBD.

---

## Appendix: Verification Commands

```bash
# Full verification (canonical pipeline; calls scripts/layer-all.sh).
./scripts/verify-completion.sh --layer full

# Layer 6 in isolation (audit always-run; should run pnpm audit + cargo audit).
./scripts/layer6.sh

# TS lang directory self-test.
./scripts/lang/ts/changed.test.sh

# pnpm audit ad-hoc invocation (sanity check the value of --audit-level).
pnpm audit --audit-level=high

# CI workflow lint (after edit).
shellcheck scripts/lang/ts/*.sh
yamllint .github/workflows/ci.yml  # if installed
```
