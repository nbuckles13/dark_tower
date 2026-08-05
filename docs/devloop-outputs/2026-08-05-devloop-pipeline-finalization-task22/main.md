# Devloop Output: Devloop Validation Pipeline Finalization (R-48 infra, T-INFRA-5, task #22)

**Date**: 2026-08-05
**Task**: Alignment/verification of the client E2E lane — devloop image pins (task #3), Kind reachability + cert fingerprints (task #4), CI green + local-gate parity (task #17), and end-to-end execution of the Layer 7 hookup (task #19). Fix any drift found; do not defer.
**Specialist**: infrastructure
**Mode**: Agent Teams (full) — headless (run-story task #22)
**Branch**: `feature/user-story-run-test`
**Duration**: ~2h (full team, headless; incl. a ~17m live Gate-2 `layer-all.sh` run — L7 cluster teardown+setup 460s + rebuild 197s + env-tests + browser E2E)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `157af6cb8c850677b674dce335c5bf3e9b43d654` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (ad293f1dfd533968b) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security` (a762449cd7d79f10b) |
| Test | `test` (ac650d7e00ee13c2b) |
| Observability | `observability` (a22f1fbbd6e159282) |
| Code Quality | `code-reviewer` (ad2ffbe28bd38918d) |
| DRY | `dry-reviewer` (a8f1c661586224365) |
| Operations | `operations` (a1829761ccda8b754) |
| Semantic Guard | `semantic-guard` (aeca6fd1c6080831c) |

### Gate 1 — Plan Confirmations
| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

### Gate 3 — Final Verdicts
| Reviewer | Verdict |
|----------|---------|
| Security | CLEAR |
| Test | CLEAR |
| Observability | RESOLVED-FIXED |
| Code Quality | RESOLVED-FIXED |
| DRY | CLEAR |
| Operations | RESOLVED-FIXED |
| Semantic Guard | CLEAR |

Gate 2: `scripts/layer-all.sh` → `LAYER_ALL_EXIT=0` (Layer 7 end-to-end: env-tests-passed + browser-e2e-passed on one live cluster).

---

## Task Overview

### Objective
Confirm the three legs the client E2E lane stands on all agree, and that the Layer 7 hookup (task #19) actually executes end-to-end (`layer-all.sh` → `layer7.sh` → env-tests then browser E2E against the same live Kind cluster, with correct `STATUS=` reporting). Fix any drift found now.

### Scope
- **Service(s)**: none (infra/tooling/docs verification)
- **Schema**: No
- **Cross-cutting**: Yes — devloop image, Kind helper, CI, validation pipeline

### Debate Decision
NOT NEEDED — verification/alignment task within existing ADR-0030 / ADR-0033 boundaries.

---

## Cross-Boundary Classification

<!-- Populated by implementer at planning; every touched file gets a row. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/devloop-outputs/2026-08-05-devloop-pipeline-finalization-task22/main.md` | Mine | — |
| `.github/workflows/ci-client.yml` (stale audit-comment refresh — drift fix) | Mine | — |
| `docs/runbooks/devloop-validation.md` (§3 rank ladder L101 + L104 prose clause + L423 + L449 ranks — sync-to-code-SSoT) | **Not mine — Minor-judgment** | **operations** (owner **APPROVED**; upgraded Mechanical→Minor-judgment per ADR-0024 §6.2/§6.6 — no guard covers ladder-vs-code drift + L104 adds authored prose; resolved via `Approved-Cross-Boundary: operations` commit trailer, added by Lead at Step 8) |
| `docs/user-stories/2026-05-02-browser-client-join.md` (§Devloop Tracking row #22 only) | Mine | — |
| `docs/TODO.md` (§Env-Test Resilience & Runbook Validation — one forward-looking follow-up bullet, operations-authored per review protocol) | Mine | — (shared bookkeeping doc; content authored by @operations) |

---

## Planning

Verification-first task (mechanism: confirm the three legs the browser-E2E lane stands on
all *agree*, and that the Layer 7 hookup *executes*; fix any drift found in-tree, do not
defer). Approach per leg:

- **Leg 1 (image pins):** grep-compare the four version contracts across their five
  encodings — Dockerfile ARGs, root `package.json`, `packages/web-app` + `packages/sdk-svelte`
  devDeps, `pnpm-lock.yaml`, and `ci-client.yml` — then confirm the *running container* matches
  (`pnpm --version`, `node --version`, `ls /opt/ms-playwright`).
- **Leg 2 (Kind reachability + certs):** confirm BOTH intentional topologies are internally
  consistent — the host manual-debug path (8443/8444 three-way sync) and the container devloop
  path (`ports.json` overrides) — and that `fingerprints.{json,env}` agree, are unexpired, and
  have a single writer.
- **Leg 3 (CI parity):** map each CI job's commands to the local `verify-completion.sh` layer
  that gates the same thing; attempt `gh run list` for runtime-green (document if unavailable).
- **Layer 7 hookup:** read `layer-all.sh` → `layer7.sh`, run the `layer7.test.sh` self-test for
  the STATUS-lane contract, and fold the Lead's live Gate-2 `layer-all.sh` STATUS output in as
  the end-to-end proof.

Predicted outcome (confirmed): **all aligned**, one small drift fixed. Reviewer sign-off (Gate 1)
obtained from all seven before implementing.

---

## Lead Pre-Scoping Findings (for implementer to verify/extend)

The Lead gathered the following during setup. Treat as a starting map, not ground truth — re-verify each before relying on it.

### Leg 1 — Devloop image pins (task #3, `infra/devloop/Dockerfile`)
- `PNPM_VERSION=10.33.2` (Dockerfile L112) == root `package.json` `packageManager: pnpm@10.33.2` == CI `ci-client.yml` pnpm `10.33.2` == this container `pnpm --version` → **10.33.2**. Aligned.
- `PLAYWRIGHT_VERSION=1.62.1` (Dockerfile L165) == `packages/web-app` devDep `playwright: 1.62.1` == `pnpm-lock.yaml` `playwright@1.62.1`. Aligned.
- Node 22 (Dockerfile L79) == CI `node-version: '22'` == container `node --version` v22.23.1. Aligned.
- Chromium baked at `/opt/ms-playwright` (`PLAYWRIGHT_BROWSERS_PATH`), `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`; `chromium*` dirs present. Aligned.
- Corepack pin contract (Dockerfile L107-117): `PNPM_VERSION` MUST match root `packageManager`. Holds.

### Leg 2 — Kind reachability + cert fingerprints (task #4)
- **Two intentional topologies** (`kind-config.yaml` header: "do NOT align the two"):
  - Host manual-debug: `127.0.0.1:8443` (AC) / `127.0.0.1:8444` (GC) — static `infra/kind/kind-config.yaml` `extraPortMappings`; matched as the SSoT default by `packages/web-app/e2e/env.ts` (`E2E_AC_URL`/`E2E_GC_URL` defaults) and `packages/web-app/vite.config.ts:21-22` (`VITE_*_PROXY_TARGET`). **CORRECTION (implementer verification):** `crates/env-tests/src/cluster.rs` does NOT default to 8443/8444 — its defaults are `http://localhost:8082` (AC) / `http://localhost:8080` (GC), the *raw pod ports reached over `kubectl port-forward`*, a DIFFERENT host-access mechanism than the Kind `extraPortMappings`. This is intentional and `env.ts`'s own header (lines 9-15) documents it ("the Rust env-test counterpart ... over port-forward defaults"). So the "three-way 8443/8444 sync" is precisely **kind-config.yaml ⇄ env.ts ⇄ vite.config.ts** (browser host path); `cluster.rs` is a fourth, port-forward-based leg — not drift, and NOT part of the 8443/8444 triad. (Prometheus 9090 happens to coincide across both mechanisms; env.ts default `http://127.0.0.1:9090` == cluster.rs default `http://localhost:9090` == kind-config `9090`.)
  - Container pipeline: `host.containers.internal:29400/29401` from `/tmp/devloop/ports.json` (dynamic `kind-config.yaml.tmpl` + `dev-cluster`). `layer7.sh` exports these as `E2E_AC_URL`/`E2E_GC_URL` + `VITE_*_PROXY_TARGET` **overrides**, so from-container E2E uses 29400/29401, not 8443/8444.
  - ⇒ The task text's "8443/8444 work from where the E2E runs" is precise only for the **host** path; the **container** path (where the devloop pipeline's E2E runs) uses the ports.json overrides. Confirm both and document the distinction — this is the subtle alignment the task exists to check.
- Cert fingerprints: `fingerprints.env` and `fingerprints.json` both present, both carry `MC_CERT_SHA256` + `MH_CERT_SHA256`, values match, certs expire `2026-08-17` (today 2026-08-05 → valid). `layer7.sh` reads `fingerprints.json` (`BROWSER_E2E_FINGERPRINTS`) for its Phase-1g precondition; the Vite loader path is `packages/web-app/vite/fingerprints.ts`. Confirm the env↔json SSoT relationship (`scripts/generate-dev-certs.sh` writes both).
- Cluster is UP + healthy (`dev-cluster status` → exists=true, pods healthy=true).

### Leg 3 — CI green + local-gate parity (task #17, `.github/workflows/ci-client.yml`)
- CI jobs: `lint` (ESLint+Prettier+svelte-check, buf lint/breaking/format, TS guards, pnpm audit), `unit` (`pnpm test:unit` + coverage), `component` (`pnpm test:component`, Playwright Chromium provisioned).
- `verify-completion.sh` gating: `standard` → client unit+component via L4 `lang/ts/test.sh`; `full` → browser E2E via L7; L3 guards + L5 lint + L6 audit cover the CI `lint` job's checks. Confirm command-level parity (`pnpm test:unit`/`test:component`/`lint`/audit) and CI green status (`gh run list` if available; otherwise document structural verification + the runtime-green limitation).

### Layer 7 hookup end-to-end (task #19)
- `layer-all.sh` loops `n=1..7`, invoking `layerN.sh`; `layer7.sh` runs `cargo test -p env-tests --features all` then (diff-triggered) `pnpm --filter @darktower/web-app test:e2e` against the same cluster.
- STATUS lanes: env-tests OK/`env-tests-passed`, FAIL/`env-tests-failed`; browser OK/`browser-e2e-passed`, FAIL/`browser-e2e-failed`, SKIPPED-NO-DIFF/`browser-e2e-no-diff`; precondition failures (`dev-certs-missing`, `playwright-browser-missing`, `cluster-unhealthy`, `ports-json-missing`, etc.) → `precondition_fail` (exit 2, operator lane) — **never** a silent skip.
- Gate-2 diff base = `origin/main` (461 files; includes `packages/**` + Rust) ⇒ browser E2E **triggered**, env-tests always attempted ⇒ **Gate 2 = the required end-to-end run**. Capture `STATUS=`/`LAYER_SUMMARY` output into the Devloop Verification Steps section below.

---

## Pre-Work
None.

---

## Implementation Summary

**Result: all three legs aligned; one small drift fixed (a stale CI comment). No functional
code / config / manifest changes.** This was a verification/alignment task and the expected
"all aligned — here is the evidence" outcome held, with the single exception below.

### Leg 1 — Devloop image pins (task #3): ALIGNED

Every version contract agrees across all its encodings, including the *running container*:

| Contract | Dockerfile ARG/ENV | Workspace pin(s) | `pnpm-lock.yaml` | CI (`ci-client.yml`) | Running container |
|----------|--------------------|------------------|------------------|----------------------|-------------------|
| pnpm | `PNPM_VERSION=10.33.2` (L112) | root `packageManager: pnpm@10.33.2` | — | `10.33.2` (all 3 jobs) | `pnpm --version` → **10.33.2** |
| Playwright | `PLAYWRIGHT_VERSION=1.62.1` (L165) | `web-app` + `sdk-svelte` devDep `1.62.1` | `playwright@1.62.1` | keyed on `hashFiles('pnpm-lock.yaml')` | baked at `/opt/ms-playwright` (`chromium-1234`, `chromium_headless_shell-1234`, `ffmpeg-1011`) |
| Node | `setup_22.x` (L79) | root `engines: >=22 <23` | — | `node-version: '22'` (all 3 jobs) | `node --version` → **v22.23.1** |

The Dockerfile's own contract comments (corepack pin L107-117, Playwright L144-167) already spell
out the "bump the ARG in the SAME commit" discipline; no drift against them. `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`
+ `PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright` are set so a container `pnpm install` uses the baked browser.

### Leg 2 — Kind reachability + cert fingerprints (task #4): ALIGNED

- **Two intentional topologies confirmed** (`kind-config.yaml` header explicitly says "do NOT align
  the two"). See the corrected Pre-Scoping bullet above for the key nuance: the **host** path is a
  three-way 8443/8444 sync (kind-config.yaml ⇄ env.ts ⇄ vite.config.ts); `cluster.rs` is a *fourth,
  port-forward-based* leg on 8082/8080 (raw pod ports), documented and intentional — NOT part of the triad.
- **Container devloop path** (where the pipeline's E2E actually runs): live `dev-cluster status` +
  `/tmp/devloop/ports.json` resolve AC/GC to `host.containers.internal:29400`/`29401`. `layer7.sh`
  Phase-1d reads `.container_urls.*` once and exports them as `ENV_TEST_*_URL` (Rust suite) then, for
  the browser suite, re-exports the SAME values as `E2E_AC_URL`/`E2E_GC_URL` + `VITE_AC_PROXY_TARGET`/`VITE_GC_PROXY_TARGET`
  (layer7.sh L581-582) — one read, zero gate-vs-suite drift. So the container E2E uses 29400/29401, never the 8443/8444 host defaults.
- **Cert fingerprints:** `fingerprints.json` and `fingerprints.env` BOTH carry `MC_CERT_SHA256` +
  `MH_CERT_SHA256`; the MC and MH base64 values **byte-match across the two files** (verified programmatically;
  values not reproduced here — dev-only public DER-SHA256 hashes, kept out of this doc). Certs expire
  `2026-08-17T00:33:22Z`, generated `2026-08-05T21:45:19Z` → ~12 days remaining, inside the 14-day Chrome
  `serverCertificateHashes` cap → **unexpired**.
- **SSoT + consumers:** `scripts/generate-dev-certs.sh` is the single writer of both files (§5 rewrites
  them every run from the on-disk leaves). `layer7.sh` Phase-1g reads `fingerprints.json` (`BROWSER_E2E_FINGERPRINTS`);
  the Vite loader `packages/web-app/vite/fingerprints.ts` (via `vite.config.ts`) reads the same `.json`.
  `fingerprints.env` is currently *unconsumed* on this branch (documented R-36 future reader) — not dead output.

### Leg 3 — CI green + local-gate parity (task #17): STRUCTURALLY ALIGNED (runtime-green needs GitHub access)

Command-level parity confirmed job-by-job:

| CI job (`ci-client.yml`) | Command | Local gate (`verify-completion.sh`) |
|--------------------------|---------|-------------------------------------|
| `unit` | `pnpm test:unit` (+coverage) | `standard` → L4 `lang/ts/test.sh` (client unit) |
| `component` | `pnpm test:component` (Playwright provisioned) | `standard` → L4 `lang/ts/test.sh` (client component); L7 for browser E2E |
| `lint` | `pnpm lint` + buf lint/breaking/format + TS guards (`no-secrets`, `no-test-removal`) + `pnpm audit` | L5 lint + L3 TS guards + L6 audit (`scripts/lang/ts/audit.sh`) |

- **Documented intentional scope difference (test asked to make this explicit):** CI runs `nx run-many`
  (**ALL** client packages) while local `verify-completion --layer standard` runs L4's `nx affected`
  (only diff-affected packages, ADR-0033 §3). Same *commands/targets*, different *run scope* — "matches CI"
  here means **command-level parity, not identical run-scope**. Do not read it as exact equivalence.
- **Runtime-green:** `gh run list --workflow=ci-client.yml` is **UNAVAILABLE in this sandbox** (gh not
  authenticated — prompts for `gh auth login` / `GH_TOKEN`). Green is therefore verified **structurally**
  (jobs coherent + commands match the local gates + the workflow's own in-file "verified-on" notes) and
  by the local audit re-run (below). The runtime-green check (a live CI run being green on GitHub) needs
  GitHub access and is the one item not closable from inside the sandbox.

### Layer 7 hookup end-to-end (task #19): VERIFIED

- `scripts/layer-all.sh` loops `n=1..7` invoking `layer${n}.sh` (L103-137). `scripts/layer7.sh` Phase-2
  runs `cargo test -p env-tests --features all`, then — AFTER it passes AND when diff-triggered — runs
  `pnpm --filter @darktower/web-app test:e2e` against the SAME live cluster.
- **STATUS-lane contract self-test:** `bash scripts/layer7.test.sh` → **70 passed, 0 failed**. It pins all
  four terminal lanes incl. every operator-lane `PRECONDITION_FAILURE` (exit 2) case (local-no-helper,
  dead-socket, setup-fail, rebuild-fail, ports-missing, prometheus-not-ready, dev-certs-missing,
  playwright-browser-missing), the `SKIPPED-NO-DIFF` browser child (layer still OK), and the env-red →
  `browser-e2e-not-run` skip — so a cluster precondition failure surfaces as PRECONDITION_FAILURE, NOT a
  test FAIL and never a silent skip, exactly as the task requires.
- **Token detail (observability's flag, resolved):** layer 7 reports its per-suite lane tokens
  (`env-tests-passed`, `browser-e2e-passed`) directly via `tee_collect_statuses`; the additional
  `STATUS=OK REASON=layer7-summary` line is emitted by the **shared lifecycle wrapper** in
  `scripts/lang/_common.sh:458` (the `layer<n>-summary` aggregate convention), not by `layer7.sh` itself.
  Both are expected.
- **Live end-to-end proof:** the Lead's Gate-2 `scripts/layer-all.sh` run IS this verification (Gate-2 diff
  vs `origin/main` = 461 files incl. `packages/**` + Rust → browser E2E triggered; env-tests always attempted).
  See §Devloop Verification Steps for the captured STATUS output.

### The one drift fixed

`.github/workflows/ci-client.yml` L147-149 (audit step) carried a dated `Current state (2026-06-23)` comment
claiming a `minimatch@>=9.0.0 <9.0.7` `pnpm.overrides` entry "resolves the 3 HIGH minimatch ReDoS advisories;
audit exits 0 with only 1 moderate remaining". Both claims were **false today**: git commit `511d201`
("clear all pnpm audit findings") replaced the overrides — current root `pnpm.overrides` are
`brace-expansion`/`fast-uri`/`postcss` (no minimatch) — and `.pnpm-audit-ignore.json` now suppresses
`GHSA-h67p-54hq-rp68` (js-yaml, task #54). I re-ran the gate to get ground truth:
`DEVLOOP_AUDIT_FORCE_RUN=1 bash scripts/lang/ts/audit.sh` → `STATUS=OK REASON=pnpm-audit-passed` (exit 0,
0 unsuppressed findings) — used only to *confirm* the state before rewriting, NOT frozen into the comment.
Fix: kept the accurate mechanism lines (142-146), replaced only 147-149 with a **date-free, count-free
SSoT-pointer** comment (points at `.pnpm-audit-ignore.json` + root `package.json` overrides and deliberately
does NOT restate the override list OR any dated/live advisory count — a dated snapshot is exactly what
drifted, so the fix must not reintroduce one). Comment-only; no gate step / run / env / needs / if /
audit-level changed. (Code-review note: an interim revision of this comment DID carry a `Current state
(2026-08-05) … 0 unsuppressed findings` sentence; @code-reviewer correctly flagged it as self-contradictory
with the very next line and a reopened drift vector — cut per that finding.)

---

## Files Modified

```
docs/devloop-outputs/2026-08-05-devloop-pipeline-finalization-task22/main.md   # this output doc
.github/workflows/ci-client.yml                                                # audit-step comment L147-149: stale snapshot → SSoT-pointer (comment-only)
docs/runbooks/devloop-validation.md                                            # §3 STATUS rank ladder L101 + L104 prose + L423 + L449 ranks: synced to _common.sh __status_rank SSoT (doc-only; operations-owned, APPROVED cross-boundary — commit trailer required, see below)
```

**REQUIRED COMMIT TRAILER (Lead, at commit time)** — the operations-owned runbook edit is a Minor-judgment
cross-boundary change (ADR-0024 §6.6, owner-cosign; no guard covers runbook-ladder-vs-code drift). Owner
(@operations) has APPROVED the content. The commit that lands `docs/runbooks/devloop-validation.md` MUST carry:

```
Approved-Cross-Boundary: operations rank-ladder sync-to-code matches _common.sh::__status_rank (0-8) SSoT
```

---

## Devloop Verification Steps

Commands run during verification (read-only unless noted):

1. `pnpm --version` → `10.33.2`; `node --version` → `v22.23.1`; `ls /opt/ms-playwright` → `chromium-1234 chromium_headless_shell-1234 ffmpeg-1011`. (Leg 1 container match.)
2. Version greps across Dockerfile / `package.json` (root + web-app + sdk-svelte) / `pnpm-lock.yaml` / `ci-client.yml`. (Leg 1 contracts.)
3. `dev-cluster status` → `Cluster exists: true`, `Pods healthy: true`, `Setup in progress: false`; AC/GC container URLs `host.containers.internal:29400`/`29401`. Cross-checked `/tmp/devloop/ports.json`. (Leg 2 container path.)
4. Fingerprint cross-file byte-match (json ⇄ env, MC + MH) → both match; expiry `2026-08-17` vs today `2026-08-05` → unexpired. (Leg 2 certs.)
5. `DEVLOOP_AUDIT_FORCE_RUN=1 bash scripts/lang/ts/audit.sh` → `STATUS=OK REASON=pnpm-audit-passed` (exit 0). (Leg 3 audit ground-truth for the comment fix.)
6. `gh run list --workflow=ci-client.yml` → **unavailable** (gh unauthenticated in sandbox). (Leg 3 runtime-green limitation.)
7. `bash scripts/layer7.test.sh` → **70 passed, 0 failed**. (Layer 7 STATUS-lane contract.)
8. Source reads: `scripts/layer-all.sh`, `scripts/layer7.sh`, `scripts/verify-completion.sh`, `scripts/lang/_common.sh` (lifecycle token). (Layer 7 hookup.)

### Gate-2 live `scripts/layer-all.sh` STATUS output (task22) — END-TO-END PROOF

**Gate 2 PASSED — `LAYER_ALL_EXIT=0`.** This is the literal end-to-end demonstration the task asked for:
`layer-all.sh` reached `layer7.sh`, which ran `cargo test -p env-tests --features all` then the
diff-triggered `pnpm --filter @darktower/web-app test:e2e` against the **same one live Kind cluster**,
with correct `STATUS=` reporting throughout.

```
LAYER=1 RESULT=OK  DURATION=1     (cargo-build)
LAYER=2 RESULT=OK  DURATION=1     (cargo-fmt)
LAYER=3 RESULT=OK  DURATION=6     (guards)
LAYER=4 RESULT=OK  DURATION=166   (cargo-test)
LAYER=5 RESULT=OK  DURATION=1     (clippy)
LAYER=6 RESULT=N/A DURATION=2     (audit: cargo-audit OK + pnpm-audit OK + buf-breaking OK;
                                   layer6 summary line N/A REASON=audit-aggregate-na — cosmetic, exit 0)
LAYER=7 RESULT=OK  DURATION=849   (env-tests + browser E2E)
TOTAL_DURATION=1026 TOTAL_RESULT=N/A   → LAYER_ALL_EXIT=0 (clean pass; see §Issues #4 on TOTAL_RESULT=N/A)
```

**Layer 7 detail (one live cluster, both suites sequential):**
- STEP timings (`LAYER=7 STEP=… DURATION=…`): cluster-ready 1s, infra-change teardown+setup 460s,
  rebuild 197s, ports-json 0s, health-confirm 12s, observability-ready 16s, browser-e2e-preconditions 0s,
  browser-e2e 35s.
- `cargo test -p env-tests --features all` → **`STATUS=OK REASON=env-tests-passed`**.
- `pnpm --filter @darktower/web-app test:e2e` → **`STATUS=OK REASON=browser-e2e-passed`**; 6/6 Playwright
  specs passed (auth-rejection ×2, join-happy-path ×2, mc-token-rejection, meeting-not-found; 32.8s).
- Suite URLs resolved to `E2E_AC_URL=http://host.containers.internal:29400`,
  `E2E_GC_URL=http://host.containers.internal:29401` (ports.json-derived, exported by `layer7.sh`) —
  **live confirmation the container path uses the override, NOT the 8443/8444 host defaults** (the exact
  Leg-2 nuance this task exists to verify). Cert fingerprints OK (MC+MH, 2 hashes) — the Phase-1g precondition
  passed against the same `fingerprints.json` this doc verified.

This supersedes the prior-task21 interim evidence: the hookup is now proven end-to-end on THIS branch's diff.

---

## Code Review Results

All seven reviewers returned final verdicts at Gate 3. **No ESCALATED, no RESOLVED-DEFERRED** — every finding was fixed in-tree.

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | CLEAR | 0 | 0 | 0 |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 |
| DRY | CLEAR | 0 | 0 | 0 |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 |

### Security — CLEAR
Confirmed the changeset touches no security-control source (cert-fingerprint pins, `DEVLOOP_TEST` sentinel trust boundary, `precondition_fail` exit-2 loudness, supply-chain pins all structurally out of the diff). The `ci-client.yml` audit-comment rewrite is security-positive (drift-prone snapshot → SSoT pointer; verified `audit-suppressions-check.sh` fails on drift and `_audit_gate.sh` fails-secure to zero suppressions). New doc scanned clean for secrets/keys/tokens.

### Test — CLEAR
Independently re-ran `scripts/layer7.test.sh` → 70/70. Verified Layer 7 runs both suites on one live cluster (env-tests then diff-triggered browser E2E, URLs exported once from ports.json), STATUS lanes pin every PRECONDITION_FAILURE operator case so a cluster precondition can never be misreported as a test FAIL or silent skip, and `verify-completion.sh` parity (standard→L4 client unit/component; full→L7 E2E) matches CI at command level. Corroborated by the live Gate-2 run.

### Observability — RESOLVED-FIXED
Finding: stale STATUS rank ladder in `docs/runbooks/devloop-validation.md` §3 (L101 + L449) drifted from the `_common.sh::__status_rank` SSoT — omitted the two task-#56 enums and was internally inconsistent with the runbook's own §6.7. Fixed in-tree (synced to the full 9-enum ladder). Re-verified line-for-line. Gate-2 token contract (STEP durations, lane tokens, layer7-summary) matches `layer7.sh`; TOTAL_RESULT=N/A confirmed non-masking.

### Code Quality — RESOLVED-FIXED
ADR Compliance: ADR-0030 / ADR-0033 / ADR-0025 all compliant; SSoT convention upheld. Ownership Lens: both original diff files Mine/infra, non-GSA. Finding: an interim revision of the `ci-client.yml` comment reintroduced a dated live-advisory snapshot (`2026-08-05 … 0 unsuppressed findings`) that self-contradicted its own SSoT-pointer clause and reopened the exact drift vector. Fixed (cut the dated sentence → date-free/count-free pointer). Re-reviewed post-fix: comment-only, drift-proof.

### DRY — CLEAR
The `ci-client.yml` change is a genuine de-duplication (removed a snapshot restating two drift-prone values → pointer to the SSoT files, both confirmed to exist). Version pins, 8443/8444 host defaults, and fingerprints.env/json untouched — nothing re-duplicated.

### Operations — RESOLVED-FIXED
Verified operations-owned mechanism files (`SKILL.md`, `scripts/layer7.sh`, `scripts/layer-all.sh`) UNEDITED and runbook §6.7 still accurate vs `layer7.sh`. Finding: the observability rank-ladder fix was PARTIAL — L423 still carried the pre-#56 ranks (5/2), contradicting the corrected L101 ladder. Fixed (7/3); full-file re-grep clean. Ownership Lens: upgraded the runbook edit Mechanical → **Minor-judgment (operations)** per ADR-0024 §6.2 (no guard covers ladder-vs-code drift; L104 adds authored prose). Owner APPROVED the content; resolved via the `Approved-Cross-Boundary: operations` commit trailer (applied by Lead at Step 8).

### Semantic Guard — CLEAR (native SAFE)
Checks applied: Credential Leak, Client Credential Lifetime, Actor Blocking, Error Context, Metrics Path. No Rust/TS production surface in the diff; the two destination-based leak surfaces (CI comment, doc) are clean — no raw fingerprint/token values, no weakening of the precondition-loud contract.

---

## Accepted Deferrals

**No reviewer verdict is RESOLVED-DEFERRED** — every finding raised (ci-client.yml audit comment, runbook rank-ladder incl. the L423 sibling) was fixed in-tree. The single entry below is a *forward-looking preventive control* (a single-source-of-truth drift guard, in the DRY-extraction-opportunity category), not a defect left in the diff — surfaced here per SKILL Step 9 so the cost shift is visible at the devloop level.

- `docs/TODO.md` §Env-Test Resilience & Runbook Validation — new `dt-guard` subcommand to fail validation on `devloop-validation.md` rank-ladder ↔ `_common.sh::__status_rank` drift (root cause of both the drift and the initially-missed L423 sibling; owner: operations)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `157af6cb8c850677b674dce335c5bf3e9b43d654`
2. Review: `git diff 157af6c..HEAD`
3. Soft reset: `git reset --soft 157af6c`

---

## Issues Encountered & Resolutions

1. **Stale CI audit comment (FIXED, not deferred).** `ci-client.yml` L147-149 asserted a live mechanism
   (a `minimatch` `pnpm.override`) that no longer exists and an advisory count that is no longer true.
   Triage confirmed the *gate behavior* (mechanism lines 142-146) was still accurate — a 3am oncall would
   not be misled about what runs — but the dated snapshot of override content + advisory count was a false
   factual provenance claim. Per "fix, don't defer" and reviewer consensus (code-reviewer, operations,
   dry-reviewer), rewritten to an SSoT-pointer comment after re-running the audit for ground truth
   (`STATUS=OK`, 0 unsuppressed findings). Recurrence-proof: the new comment references the SSoT files
   instead of restating a re-datable snapshot.

2. **Pre-scoping imprecision on `cluster.rs` (CORRECTED in-doc).** The Lead's Pre-Scoping listed
   `crates/env-tests/src/cluster.rs` as matching the 8443/8444 host defaults. It actually defaults to
   `8082`/`8080` (raw pod ports over `kubectl port-forward`) — a different, intentional host-access
   mechanism documented in `env.ts`. Corrected the Pre-Scoping bullet; no code change (not drift).

3. **Runtime CI-green not checkable in sandbox.** `gh` is unauthenticated, so `gh run list` cannot confirm
   a live green CI run. Green is verified structurally + via the local audit re-run; the runtime-green
   check is the single item that needs GitHub access. Not a code problem — an environment limitation, called
   out loudly rather than silently skipped.

4. **`TOTAL_RESULT=N/A` on the Gate-2 PASS is NOT a failure (read-carefully note).** The Gate-2
   `layer-all.sh` run exited clean (`LAYER_ALL_EXIT=0`) with every layer OK, yet the aggregate line reads
   `TOTAL_RESULT=N/A`. This is the documented aggregation carry-up from Layer 6, whose summary line is
   `N/A REASON=audit-aggregate-na` (cargo-audit OK + pnpm-audit OK + buf-breaking OK, but the layer's
   *aggregate* enum renders N/A) — cosmetic, exit 0. `N/A` aggregates upward into `TOTAL_RESULT` without
   changing the exit code (`status_to_exit_code` maps both OK and N/A to 0). A future reader scanning for a
   red must key on `LAYER_ALL_EXIT` / per-layer `RESULT=`, NOT on `TOTAL_RESULT=N/A`, which here means
   "clean pass with an N/A-typed layer", not "failure".

5. **Stale STATUS rank ladder in the runbook (FIXED — @observability finding).** `docs/runbooks/devloop-validation.md`
   §3 (line 101) documented the rank ladder as `… OK (2) < N/A (3) < FAIL (4) < FAIL-MISSING-VERB (5) < UNKNOWN (6)`
   — the pre-task-#56 snapshot, OMITTING two enums that exist in the `_common.sh::__status_rank` SSoT
   (`SKIPPED-NO-CLUSTER`, `PRECONDITION_FAILURE`). A 3am operator reading §3 to understand why a Layer-7
   `PRECONDITION_FAILURE` dominated the aggregate found a ladder that didn't even list that enum — and it
   contradicted the runbook's OWN §6.7 (line 501, "ranks between FAIL and FAIL-MISSING-VERB"). Directly on the
   N/A-carry-up contract this task's Gate-2 run exercised, so in-scope for "fix any drift; do not defer".
   Fix: synced line 101 to the full 9-enum ladder (`SKIPPED-NO-VERB 0 < SKIPPED-NO-DIFF 1 < SKIPPED-NO-CLUSTER 2
   < OK 3 < N/A 4 < FAIL 5 < PRECONDITION_FAILURE 6 < FAIL-MISSING-VERB 7 < UNKNOWN 8`), corrected the stale
   `(N/A rank 3 > OK rank 2)` worked-example parenthetical (line 449) to `(N/A rank 4 > OK rank 3)`, and added a
   PRECONDITION_FAILURE/SKIPPED-NO-CLUSTER clause to the §3 intuition prose mirroring the `_common.sh:229-245`
   comment. **@operations re-review caught a missed sibling: line 423** still carried the pre-task-#56 ranks
   `FAIL-MISSING-VERB (rank 5)` / `OK, rank 2` — corrected to `(rank 7)` / `(rank 3)`, which restored internal
   consistency with the line-101 ladder (the first pass had *relocated* the contradiction rather than removed it).
   Re-grepped the whole runbook: lines 101/423/449 are now the ONLY numeric `rank N` references and all three
   match the SSoT (0-8). **Classification upgraded Mechanical → Minor-judgment** per ADR-0024 §6.6 (owner @operations's
   call): no guard covers runbook-ladder-vs-code drift (which is why it drifted undetected AND why 423 was missed
   on the first pass), and line 104 adds authored prose (not a structure-preserving substitution). Owner APPROVED
   the content; landing commit carries the `Approved-Cross-Boundary: operations …` trailer (see §Files Modified).

## Lessons Learned

- **Dated "current state" snapshots in comments are latent drift.** A comment that restates a value living
  in another file (an override list, an advisory count) *will* diverge the moment that file changes — the
  exact SSoT failure mode CLAUDE.md warns about. The durable fix is a pointer to the SSoT, not a fresher
  snapshot (which just resets the drift clock). Applied here to the `ci-client.yml` audit comment.
- **"Works from where the E2E runs" has two answers, and the sandbox can hide the wrong one.** The 8443/8444
  host defaults are correct for manual host debugging but are NOT what the container devloop pipeline uses —
  it overrides to the `ports.json` 29400/29401 URLs. Verifying "the E2E lane is reachable" means checking
  the *override wiring* (`layer7.sh` export), not just the static defaults. Two intentionally-divergent
  topologies stay correct only because one file (`kind-config.yaml`) explicitly documents "do NOT align the two".
- **A self-test can stand in for a fresh live run of a contract.** `layer7.test.sh` (70/70) pins the full
  four-lane STATUS/exit-code contract hermetically, so the operator-vs-implementer lane behavior is provable
  without waiting on a live cluster failure — the live Gate-2 run then only needs to prove the happy path executes.
- **Verification is a valid deliverable.** Four legs cross-checked, one 3-line comment fixed. Restraint
  (not "improving" aligned-and-intentional divergences like `cluster.rs`'s port-forward defaults) is as much
  the job as fixing real drift.
