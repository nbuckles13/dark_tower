# Devloop Output: Playwright Browser E2E Harness + Join Happy-Path Spec

**Date**: 2026-08-03
**Task**: Build the Playwright browser E2E harness and happy-path spec for the Dark Tower browser client (R-40, R-44, R-46) — story task #18
**Specialist**: test
**Mode**: Agent Teams (v2)
**Branch**: `feature/user-story-run-test`
**Duration**: 2 sessions — 2026-08-03 (plan + implementation + Gate 2 attempt 1; interrupted before review) and 2026-08-04 (resumed headless: full review, Gate 2 re-validation ×2, commit)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3f4fc92aa521085200bdb6f677d0132e41495534` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `test` |
| Iteration | `1` (review iteration 1; Gate 2 attempts: 2 of 3) |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-FIXED` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `CLEAR` |
| Semantic Guard | `RESOLVED-FIXED` |

---

## Task Overview

### Objective
Build the Playwright browser E2E harness and happy-path join spec for the browser client under `packages/web-app/`:

- `playwright.config.ts` — Chromium-only project, retries=0 (ADR-0028 flaky policy), single browser context by default, trace artifacts on failure
- `e2e/global-setup.ts` — reads `MC_CERT_SHA256_B64` / `MH_CERT_SHA256_B64` from `infra/docker/certs/fingerprints.env` (written by `scripts/generate-dev-certs.sh`); asserts host-side Kind cluster reachable
- Fixtures: `bootstrapMeeting(adminToken)` (direct GC POST /api/v1/meetings), `joinAsUser(page, meetingCode)`, `waitForJoined(page)`
- `e2e/join-happy-path.spec.ts` — fake media flags, Vite-served demo, sign-up → create → join. Flow contract (post task #58): `MeetingSession.join` is token-based, no login step, forced-login band-aid (7b69288) must NOT be encoded.
- Assertions: (a) MC JoinResponse with participant_id via `window.__darktower_test__` hook (gated by `__E2E_HOOKS__` Vite define); (b) MH WebTransport handshake succeeds for EVERY media_servers URL (active/active); (c) second context join fires ParticipantJoined in first context within 5s; (d) SDK sends MediaConnectionUpdate ClientMessage state=CONNECTED for ≥1 MH; (e) token-only invariant (task #58 c-iii): no email/password on any request at join time.
- `e2e/README.md` — division of responsibilities vs `crates/env-tests/tests/24_join_flow.rs` (unchanged)

Requires live host-side Kind cluster (AC 127.0.0.1:8443, GC 127.0.0.1:8444 per task #4); reuse NodePort pattern from `crates/env-tests/src/cluster.rs` (ADR-0030).

### Scope
- **Service(s)**: packages/web-app (client E2E layer); no Rust service changes expected
- **Schema**: No
- **Cross-cutting**: Touches test infrastructure; possible Vite config change for `__E2E_HOOKS__` define

### Debate Decision
NOT NEEDED — implementation of an already-decomposed story task with explicit requirements; design decisions covered by ADR-0028 (client testing tiers, flaky policy) and ADR-0030 (cluster helper).

---

## Cross-Boundary Classification

E2E harness/spec files are test-owned (ADR-0028 amendment: browser driver of the Env-Test tier; `e2eBus.ts` explicitly names the Playwright specs "test-owned"). ALL cross-boundary rows are Mechanical (review-only) per @team-lead's Gate 1 decision — see Scope Decision below.

**Scope decision (@team-lead, Gate 1 — devloop-local context, NOT an Accepted Deferral):** the originally planned lint-scope widening (package.json `lint` covering `e2e/`, and the follow-on project.json lint-command dedup per @code-reviewer point 1) is DROPPED from this task. Rationale: those edits are Minor-judgment on client-owned files, ADR-0024 §6.3 requires the owner-specialist (client) as reviewer, and no client teammate is on this roster; the lead will not self-grant an owner ACK in headless mode. `e2e/` stays typechecked via the tsconfig include (svelte-check runs the whole program); eslint coverage of `e2e/` moves to task #19 (pipeline integration — its natural home). Related informational note for client/#19: the package.json↔project.json command-string duplication class (build/dev/test:unit/test:component/lint all duplicated verbatim) surfaced by @code-reviewer remains un-addressed by design — client-owned cleanup.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/web-app/playwright.config.ts` (new) | Mine | — |
| `packages/web-app/e2e/env.ts` (new) | Mine | — |
| `packages/web-app/e2e/global-setup.ts` (new) | Mine | — |
| `packages/web-app/e2e/mcMetrics.ts` (new — split from fixtures.ts during implementation to satisfy the one-PromQL-home condition from @dry-reviewer) | Mine | — |
| `packages/web-app/e2e/fixtures.ts` (new) | Mine | — |
| `packages/web-app/e2e/join-happy-path.spec.ts` (new) | Mine | — |
| `packages/web-app/e2e/README.md` (new) | Mine | — |
| `packages/web-app/package.json` (edit: add `test:e2e` script) | Mechanical (additive script entry) | client |
| `packages/web-app/project.json` (edit: add `test:e2e` Nx target, NOT in default pipeline) | Mechanical (additive target, mirrors existing target shape) | client |
| `packages/web-app/tsconfig.json` (edit: add `e2e/**/*` + `playwright.config.ts` to `include`) | Mechanical (additive include entries) | client |
| `.gitignore` (edit: add web-app `test-results/`, `playwright-report/`, `blob-report/`) | Mechanical | repo-wide (no single owner) |
| `package.json` (root — edit during Gate 2 attempt 2: bump existing `pnpm.overrides` floor `brace-expansion` `>=5.0.8` → `>=5.0.9`, GHSA-rgw5-rvv9-x895 HIGH DoS advisory landed in the audit DB mid-review; upgrade not suppression, @security confirming per ADR-0033 §11) | Mechanical (one-line floor bump extending an existing security override; directed by @team-lead) | repo-wide (no single owner) |
| `pnpm-lock.yaml` (re-resolved by `pnpm install` for the override bump; all `brace-expansion@` paths now 5.0.9) | Mechanical (derived artifact of the above) | repo-wide (no single owner) |
| `docs/devloop-outputs/2026-08-03-playwright-e2e-harness/main.md` | Mine (devloop record) | — |

**Explicitly NOT touched**: `packages/web-app/vite.config.ts` (the `__E2E_HOOKS__` define already exists, R-29), `packages/web-app/src/lib/e2eBus.ts` (bus already implemented), `packages/sdk-core/**`, `crates/env-tests/**` (24_join_flow.rs stays unchanged per task), `scripts/layer*.sh` / `scripts/lang/ts/*` (pipeline wiring is task #19), `scripts/generate-dev-certs.sh`, `infra/**`.

---

## Planning

### Mechanism restatement

Instance-language: "add a Playwright config, a global-setup, three fixtures, one spec." Mechanism-language: **give the existing Env-Test tier its browser driver** — ADR-0028 (amended) says browser-driven and Rust-driven tests are *different drivers of the same tier*; the Rust driver (`crates/env-tests/`) exists, the browser driver does not. This task adds the driver harness plus its first flow. The wider class contains exactly one same-owner sibling: pipeline integration/sharding — which is story task #19 and explicitly out of scope (SKILL.md:421, @operations point 1). No hidden wider class; the task framing matches the mechanism.

### Survey findings that shape the plan

1. **`window.__darktower_test__` and `__E2E_HOOKS__` already exist** (task R-29): bus in `packages/web-app/src/lib/e2eBus.ts` (replay-buffered, whitelist projection, drops bindingToken/correlationId), define in `vite.config.ts:35` (`mode !== 'production'`, DCE'd + bundle-content-guarded), types in `src/globals.d.ts` (`DarktowerTestBus`). The spec CONSUMES this contract; no production-code changes needed.
2. **Task-text env-name discrepancy (SSoT)**: the task says read `MC_CERT_SHA256_B64`/`MH_CERT_SHA256_B64`; the writer (`scripts/generate-dev-certs.sh:296-323`) emits `MC_CERT_SHA256`/`MH_CERT_SHA256` (values already base64) into BOTH `fingerprints.env` and `fingerprints.json`. I code to the writer's names. Further, web-app already has a Node-side parser for `fingerprints.json` (`vite/fingerprints.ts:loadCertFingerprints`) — global-setup reuses that loader on the `.json` (one parser, one writer) instead of hand-rolling an `.env` parser, and wraps it with an assert-non-empty that FAILS LOUDLY (the Vite loader intentionally degrades gracefully; global-setup must not).
3. **Runner needs no new dependency**: `playwright@1.62.1` is already a web-app devDependency (Vitest browser provider) and its `./test` export IS the `@playwright/test` runner (verified in node_modules). Specs import from `'playwright/test'`; no lockfile churn, no version skew with the Vitest provider.
4. **Ports/URLs**: host contract is AC `http://127.0.0.1:8443`, GC `http://127.0.0.1:8444` (SSoT `infra/kind/kind-config.yaml:26-35`; matches the Vite proxy defaults in `vite.config.ts:21-22`), Prometheus `http://127.0.0.1:9090` (kind-config.yaml:46). All become documented defaults behind env overrides (`E2E_AC_URL`, `E2E_GC_URL`, `E2E_PROMETHEUS_URL`, `E2E_BASE_URL`) in ONE module `e2e/env.ts`, mirroring `ClusterPorts::from_env()` (`crates/env-tests/src/cluster.rs`), logging env-vs-default provenance. MC/MH endpoints come ONLY from the join response `media_servers` (ADR-0030 invariant).
5. **`demo` org is seeded** by `infra/kind/scripts/setup.sh:seed_demo_org` (R-38) — the UI sign-up default subdomain works against the dev cluster.
6. **AC registration rate limit is 5/hour** (per `crates/env-tests/tests/24_join_flow.rs` shared-user discipline). Plan budgets **2 registrations per run** (see spec layout) and documents the budget in the README.
7. **MC-side observability for assertion (d)**: MC records client `MediaConnectionUpdate` as `mc_participant_mh_status_total{state="connected"}` (`crates/mc-service/src/observability/metrics.rs:384`); the Rust env-test `26_mh_quic.rs` Scenario 7 already asserts this end-to-end with a *synthetic Rust client* via Prometheus (60s poll budget, 15s scrape SLA). The browser spec adds the *real-browser-SDK* driver of the same series (monotonic `> baseline` assertion — concurrent suites can only push the counter further up, so no cross-suite flake).

### File-by-file design

**1. `packages/web-app/playwright.config.ts`**
- `testDir: './e2e'`, single project `chromium` only (Chromium is the only WebTransport-capable engine).
- `retries: 0` EXPLICIT with a comment citing ADR-0028 flaky policy; `fullyParallel: false`, `workers: 1` (one shared cluster + one shared Vite server; deterministic ordering; AC rate-limit budget).
- Artifacts: `trace: 'retain-on-failure'` (NOT `on-first-retry` — never fires with retries=0, @observability point 1), `screenshot: 'only-on-failure'`, `outputDir: 'test-results'`. No video (trace covers network+console).
- Timeouts: global `timeout: 120_000` per test (MH handshake + 60s Prometheus poll headroom, bounded so a wedged WebTransport handshake cannot hang the suite); default expect timeout 5s; per-wait explicit timeouts in fixtures.
- `use.baseURL` from `e2e/env.ts` (`E2E_BASE_URL`, default `http://demo.localhost:5173` — Host header must carry the `demo` subdomain for the AC proxy, vite.config.ts:52-55; Chromium resolves `*.localhost` natively).
- `launchOptions.args: ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream']` (ADR-0028 fake-media flags). **No** `ignoreHTTPSErrors`, **no** `--ignore-certificate-errors` — cert trust flows exclusively through `serverCertificateHashes` pinning (`__DEV_CERT_SHA256_HASHES__`); a bypass would mask the invariant assertion (b) proves (@security point 4).
- `webServer`: `command: 'pnpm dev'` (Vite dev mode ⇒ `__E2E_HOOKS__` true via the EXISTING gate; never forces the define), readiness `url: 'http://127.0.0.1:5173'` (readiness doesn't need the subdomain Host), `reuseExistingServer: true`, bounded timeout.
- `globalSetup: './e2e/global-setup.ts'`.

**2. `e2e/env.ts`** — the ONE config module (defaults + env overrides + provenance logging), cross-referencing `infra/kind/kind-config.yaml` and `crates/env-tests/src/cluster.rs` in comments so Rust↔TS drift is traceable (@dry-reviewer point 5). Also exports the org subdomain constant (`demo`, citing `setup.sh:seed_demo_org`).

**3. `e2e/global-setup.ts`** — ordered, fail-loud preconditions (no skip, no warn-and-continue):
  1. Cert fingerprints: `loadCertFingerprints(<repo>/infra/docker/certs/fingerprints.json)` (reused loader; comment citing `scripts/generate-dev-certs.sh` as the writer and noting `fingerprints.env` carries the same pair `MC_CERT_SHA256`/`MH_CERT_SHA256`); throw if `< 2` hashes, naming the file AND the fix (`scripts/generate-dev-certs.sh`, then restart `pnpm dev`). Reads ONLY the fingerprints file — never `.key` material.
  2. Cluster reachability: `GET {AC}/health` and `{GC}/health` with `AbortSignal.timeout(5000)` (mirrors `cluster.rs` per-service 5s checks); on failure name the exact URL, the timeout, and remediation (`infra/kind/scripts/setup.sh`). Prometheus probed the same way (needed by assertion d).

**4. `e2e/fixtures.ts`**
- `randomCredentials()` — per-run throwaway: `e2e-<runId>-<n>@darktower.test` + `crypto.randomUUID()`-derived password. Never hardcoded, never logged (traces will contain them by nature; they are disposable — README notes this).
- `signUpViaUi(page, creds)` — drives the existing SignUp testids; **captures the `accessToken` from the register response body via `page.on('response')`**. Rationale: the demo (correctly, R-23) never exposes the token on the bus or DOM, and Node-side direct AC calls hit the fetch forbidden-`Host`-header wall (AC org extraction needs `demo.<host>`; undici strips `Host`). The register exchange is already recorded in traces, so the test reading it adds no new exposure surface. Token is passed by value, never logged.
- `signInViaUi(page, creds)` — same shape against the SignIn view (used by context 2; a fresh auth exchange is legal — the forbidden thing is re-auth *at join time*).
- `bootstrapMeeting(adminToken)` — Node-side direct `POST {E2E_GC_URL}/api/v1/meetings`, reusing `MeetingApiClient` from `@darktower/sdk-core` (resolves via the existing tsconfig `paths` alias to sdk-core *source*; Playwright's TS loader honors tsconfig paths) so the R-53 camelCase wire mapping stays single-sourced (@dry-reviewer point 4). NOT through the demo UI, per task. Fallback if the sdk-core transitive graph misbehaves under Playwright's Node loader: one raw `fetch` with README justification — will flag to @dry-reviewer if needed.
- `joinAsUser(page, meetingCode)` — clicks `nav-join`, fills `meeting-code`, clicks `join-button` (token-based join; NO login step — post-task-#58 contract).
- `waitForJoined(page)` — `page.waitForFunction` on the bus replay buffer for a `joined` event, explicit timeout; on failure the error message includes the events actually observed so far (@observability point 5). Returns the projected joined payload (participantId, mediaServers, …). Typed against `DarktowerTestBus` (`src/globals.d.ts`) — no `any` casts.
- `busEvents(page, type)` — typed replay-buffer read.
- `recordRequests(context)` — `context.on('request')` recorder (url, method, headers, postData) powering assertion (e).

**5. `e2e/join-happy-path.spec.ts`** — two tests, each self-contained:

*Test 1 — UI happy path (assertions a–e), 1 registration:*
1. Context 1: `signUpViaUi(creds1)` (UI-driven, R-40) → UI create-meeting (`meeting-title`, `create-button`, read `created-meeting-code`) → start request recorder → `joinAsUser` → `waitForJoined`.
2. **(a)** joined event `participantId` non-empty.
3. **(b)** `mediaServers` from the joined event is non-empty; wait until `mediaConnected` bus events cover EVERY `media_servers` URL (bounded wait; failure message names which URLs are missing, "m of n connected"). No hardcoded MH count — topology stays out of test code.
4. **(d)** Prometheus `sum(mc_participant_mh_status_total{state="connected"})` baseline captured BEFORE join; after join, poll until `> baseline` (60s budget / 2s interval — same shape as `26_mh_quic.rs` Scenario 7, browser-driver counterpart; README documents the division). Client-side corroboration: per the R-21 contract `join()` resolves only after the one `MediaConnectionUpdate` send settles, so `joined` + non-empty `mediaConnected` already implies a CONNECTED report was sent — the Prometheus check proves MC *received and recorded* it (drives task #6's handler end-to-end).
5. **(c)** Context 2 (same browser, second context): `signInViaUi(creds1)` (no second registration; participant identity is per-join, not per-user) → `joinAsUser(same code)` → assert context 1's bus receives `participantJoined` with context 2's participantId **within 5s** (explicit 5000ms wait; failure message dumps observed events).
6. **(e)** Token-only invariant, value-based (@security point 3): from join-start onward in BOTH contexts, no recorded request URL/header/body contains `creds1.email` or `creds1.password` (actual string values, not field names); additionally zero requests to `/api/v1/auth/*` after join start (structural no-re-login guard against re-encoding 7b69288), and the GC join request carries `Authorization: Bearer`.

*Test 2 — bootstrapMeeting consumer (harness building block for #19), 1 registration:*
`signUpViaUi(creds2)` (captures token) → `bootstrapMeeting(token2)` Node-side (direct GC POST — proves the fixture against the live cluster so #19's multi-user specs can lean on it) → `joinAsUser(bootstrapped code)` → `waitForJoined` → assert (a)-shape join succeeded, plus the (e) recorder scoped to this context.

Total: 2 registrations/run against AC's 5/hour budget (documented; the Rust suite applies the same discipline).

**6. `e2e/README.md`** — division of responsibility vs `crates/env-tests/tests/24_join_flow.rs` (UNCHANGED: Rust drives AC→GC→MC wire-level with a synthetic client; browser E2E adds real Chromium, the real WebTransport API incl. `serverCertificateHashes`, real demo DOM — the layer Rust cannot reach; also vs `26_mh_quic.rs` Scenario 7 for the (d) metric); prerequisites (cluster via `infra/kind/scripts/setup.sh`, certs via `scripts/generate-dev-certs.sh`, `/etc/hosts` `demo.localhost` note for WSL2, Playwright Chromium installed); how to run (`pnpm test:e2e` — MANUAL ONLY in this task; Layer-7 wiring is #19); env-override knobs; failure triage (`test-results/`, `pnpm exec playwright show-trace`); artifact hygiene note (traces contain synthetic-credential traffic + bearer tokens against a local dev cluster — do not promote artifacts to shared storage); AC registration budget note.

**7–9. Registration edits (all Mechanical)** — `package.json` script `"test:e2e": "playwright test"`; `project.json` Nx target `test:e2e` mirroring existing target shape (NOT added to any default pipeline); `tsconfig.json` include gains `e2e/**/*` + `playwright.config.ts` so the harness is fully typechecked (svelte-check covers the program — no untyped island). Lint scripts are NOT touched (see Scope Decision: eslint coverage of `e2e/` lands with #19). `src/globals.d.ts` is already in the same TS program, so `DarktowerTestBus` types flow into the specs.

**10. `.gitignore`** — add `packages/web-app/test-results/`, `packages/web-app/playwright-report/`, `packages/web-app/blob-report/` (traces contain request bodies — never committable, @security point 2).

### Validation plan (this environment has no live cluster)
- `pnpm --filter web-app lint` / `svelte-check` (typecheck incl. e2e), `prettier`.
- `playwright test --list` to prove config + spec discovery parse correctly.
- Global-setup failure path exercised locally (it should fail loudly, naming the cluster remediation, when no cluster is up — that IS the designed behavior).
- Full live run is a host-side action (needs Kind cluster + certs); documented in README and called out as the remaining host-side step.

### Reviewer planning confirmations (conditions to honor at implementation)

- **@security** — accepted; token capture from the auth response approved on three conditions: (1) response listener narrowly scoped to the auth endpoint and removed after the read, (2) token held by-value in a local, never logged/persisted, (3) recorder-based value-scan for creds stands as specified.
- **@observability** — accepted; two notes: Prometheus empty-vector result must read as baseline 0 (already the plan: `unwrap_or(0.0)` shape); use Prometheus `/-/healthy` for the global-setup probe.
- **@semantic-guard** — accepted; notes: (1) the Minor-judgment lint-scope row needs an `Approved-Cross-Boundary: client <reason>` commit trailer (ADR-0024 §6.3) — relayed to @team-lead; (2) fixtures/spec treated as in-scope test code at review; ping if anything outside the NOT-touched list changes.
- **@operations** — accepted; conditions: (1) README must state the HOURLY ceiling explicitly ("more than 2 consecutive runs within an hour will exhaust AC's 5/hour registration limit — expect 429 on sign-up; wait or reseed"); (2) global-setup Prometheus failure message must say the observability stack is REQUIRED (do not use `setup.sh --skip-observability`), and README prerequisites must state it.
- **@dry-reviewer** — Q1 RESOLVED: cross-driver Prometheus parallel accepted (same series, different driver/purpose, cross-language). Conditions: (1) bidirectional cross-references — TS assertion helper cites `26_mh_quic.rs` Scenario 7; README names the shared series `mc_participant_mh_status_total` explicitly; (2) PromQL literal lives in ONE TS helper, not per-spec; (3) ping before falling back from sdk-core `MeetingApiClient` to raw fetch.
- **@test** — Q1 CONFIRMED (Prometheus is the assertion; R-21 inference stays as comment) with refinements: capture the (d) baseline immediately before the join action (narrowest false-pass window); complete the (d) poll before context 2 joins (attribution — plan's step order 4-before-5 kept); README notes the single-workstation assumption next to the AC budget note. Q3 CONFIRMED (participant_id is minted per-connection, `mc-service/src/webtransport/connection.rs:322`); strengthen (c): assert context 2's participantId != context 1's (pins the per-join identity contract). Requirement A: assertion (e) recorder windows are PER-CONTEXT — each context's window opens at ITS OWN join action (context 2's sign-in happens after context 1's join-start and must not trip the structural guard). Requirement B: waitForJoined's failure message (and/or global-setup) must mention the squatting-server failure mode — server at E2E_BASE_URL must be a dev-mode Vite server with `__E2E_HOOKS__` active (reuseExistingServer can pick up `pnpm preview`/prod builds). Point C (cross-boundary trailer) superseded by @team-lead's scope decision — lint rows dropped.
- **@code-reviewer** — confirmed after two amendments: point 1 (lint-command drift) superseded by the same @team-lead scope decision (no lint edits at all; wider duplicated-command class recorded as informational for client/#19); point 2: `e2e/env.ts` default-URL comment must cite all three sites — `infra/kind/kind-config.yaml:26-35` (SSoT), `vite.config.ts:21-22`, `crates/env-tests/src/cluster.rs`.

### Open questions surfaced to reviewers
1. **(d) via Prometheus poll** duplicates the *series* asserted by Rust `26_mh_quic.rs` Scenario 7 with a different driver (real browser SDK). I judge this the honest end-to-end observation (the client-side bus has no "update sent" event, by R-23 design); alternative is ordering-inference only. @test/@dry-reviewer — confirm.
2. **adminToken via response capture** on the register/login exchange (traces already record it; per-run throwaway) — @security confirm this is the right way to obtain `bootstrapMeeting`'s token given the Host-header constraint on Node-side AC calls.
3. **Context 2 signs IN as the same user** (participant identity is per-join) to hold the registration budget at 2/run — @test confirm same-user two-context join is acceptable for assertion (c), else test 1 takes a third registration.

---

## Pre-Work

None

---

## Implementation Summary

Implemented exactly per the approved plan; no deviations requiring re-review, one ESM fix found at runtime (below). All reviewer Gate 1 conditions honored:

- **playwright.config.ts** — Chromium-only project; explicit `retries: 0` (ADR-0028 comment); `workers: 1` + `fullyParallel: false` (shared cluster + AC rate budget); `trace: 'retain-on-failure'` + `screenshot: 'only-on-failure'`; 120s per-test ceiling, 5s expect default; fake-media launch args, NO cert-bypass flags; `webServer: pnpm dev` (dev mode load-bearing for `__E2E_HOOKS__`), readiness probed via 127.0.0.1; `globalSetup` wired. Runner is the already-installed `playwright@1.62.1` via its `./test` export — zero new dependencies.
- **e2e/env.ts** — single topology module; `E2E_BASE_URL`/`E2E_AC_URL`/`E2E_GC_URL`/`E2E_PROMETHEUS_URL` env-over-default with fail-loud URL validation and provenance (`describeEnv()`); three-site cross-reference comment (kind-config.yaml SSoT, vite.config.ts, cluster.rs); org subdomain `demo` citing `seed_demo_org`.
- **e2e/global-setup.ts** — fail-loud order: fingerprints via the REUSED `vite/fingerprints.ts` loader on `fingerprints.json` (writer-canonical keys `MC_CERT_SHA256`/`MH_CERT_SHA256`; task-text `_B64` names corrected to the writer per SSoT) asserting >= 2 hashes with actionable remediation; then AC/GC `/health` + Prometheus `/-/healthy` (@observability) probes, 5s bounded, each naming URL + fix; Prometheus failure text states the observability stack is REQUIRED (no `--skip-observability`) per @operations. Informational squatting-server note when a server already occupies the Vite port.
- **e2e/mcMetrics.ts** — the ONE PromQL home (`sum(mc_participant_mh_status_total{state="connected"})`); empty instant vector reads as 0 (@observability); poll helper 60s/2s mirroring `26_mh_quic.rs` Scenario 7 with bidirectional cross-reference comments (@dry-reviewer).
- **e2e/fixtures.ts** — `randomCredentials` (per-run throwaways), `signUpViaUi`/`signInViaUi` (token captured via narrowly-scoped `waitForResponse` on the exact auth path, auto-removed — @security conditions), `bootstrapMeeting` via sdk-core `MeetingApiClient` resolved through tsconfig paths (VERIFIED working under Playwright's loader at `--list` — no raw-fetch fallback needed, @dry-reviewer not pinged), `joinAsUser`, `waitForJoined`/`waitForAllMediaConnected`/`waitForParticipantJoined` (observed-events failure context; explicit squatting-server message when the bus is absent — @test Requirement B), per-context `recordRequests` + `assertTokenOnlyJoinTraffic` (value-based cred scan + structural zero-`/api/v1/auth/*` guard + Bearer-only on GC requests).
- **e2e/join-happy-path.spec.ts** — Test 1: UI sign-up → UI create → token-based join; (a) participantId non-empty; (b) every `media_servers` URL handshaken (count not hardcoded); (d) baseline captured immediately before the join action, poll completed BEFORE context 2 joins (@test refinements); context 2 signs IN as the same user (1 registration), `joined2.participantId !== joined.participantId` (per-join identity contract), (c) ParticipantJoined in context 1 within 5s; (e) asserted on BOTH per-context windows (context 2's window opens at its own join action — Requirement A). Test 2: `bootstrapMeeting` consumer (sign-up #2, API-created meeting, join, (a) + (e)).
- **e2e/README.md** — division-of-responsibilities table vs unchanged `24_join_flow.rs` (+ `26_mh_quic.rs` Scenario 7 cross-driver note naming the shared series); prerequisites (Prometheus REQUIRED — no `--skip-observability`); AC hourly ceiling wording per @operations ("more than 2 consecutive runs within an hour ... expect 429; wait or reseed") + single-workstation assumption (@test); manual-run-only (Layer 7 = #19); artifact hygiene; triage via `show-trace`.

Scope decision honored: NO lint-script changes (all cross-boundary edits Mechanical).

---

## Files Modified

New (test-owned):
- `packages/web-app/playwright.config.ts`
- `packages/web-app/e2e/env.ts`
- `packages/web-app/e2e/global-setup.ts`
- `packages/web-app/e2e/mcMetrics.ts` (split from fixtures per the one-PromQL-home condition)
- `packages/web-app/e2e/fixtures.ts`
- `packages/web-app/e2e/join-happy-path.spec.ts`
- `packages/web-app/e2e/README.md`

Edited (all Mechanical, additive):
- `packages/web-app/package.json` — `test:e2e` script only
- `packages/web-app/project.json` — `test:e2e` Nx target only (not in default pipelines)
- `packages/web-app/tsconfig.json` — `include` gains `e2e/**/*` + `playwright.config.ts`
- `.gitignore` — web-app `test-results/`, `playwright-report/`, `blob-report/`
- `docs/devloop-outputs/2026-08-03-playwright-e2e-harness/main.md` — this record

Untouched, as planned: `vite.config.ts`, `src/lib/e2eBus.ts`, `packages/sdk-core/**`, `crates/env-tests/**`, `scripts/layer*.sh`, `scripts/generate-dev-certs.sh`, `infra/**`.

---

## Devloop Verification Steps

Run in this container (no live cluster available here):
1. `pnpm typecheck` (svelte-check over the whole program incl. e2e): **684 files, 0 errors, 0 warnings**.
2. `pnpm lint` (unchanged scope) + `prettier --check` on the new files: **clean**.
3. `pnpm test:unit`: **5/5 pass** (no regression from the tsconfig/package edits).
4. `pnpm exec playwright test --list`: **2 tests collected** — proves config parses AND the `@darktower/sdk-core` tsconfig-paths resolution works under Playwright's loader (module graph executes at collection).
5. `pnpm exec playwright test` (no cluster): global-setup **fails loudly by design** — provenance log, fingerprints OK (2 hashes present in this checkout), then the three cluster probes each fail with the exact URL + remediation text. This exercises the designed failure path end-to-end.

Remaining HOST-SIDE step (cannot run in this container): a full green run against a live Kind cluster (`infra/kind/scripts/setup.sh` with observability + `scripts/generate-dev-certs.sh`, then `pnpm test:e2e`).

---

## Code Review Results

Review ran in the resumed headless session (2026-08-04) with the full 7-reviewer panel. All verdicts in; none ESCALATED.

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | Assertion-(e) cred scan dodgeable via percent-encoding + `Authorization: Basic` base64 → 4-needle raw+encoded scan, window-wide Bearer-only header check (`fixtures.ts:assertTokenOnlyJoinTraffic`). Re-verified post-parallel-edits; also co-signed the Layer 6 dependency remediation (ADR-0033 §11). |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | Diagnostic-path `last-error` read auto-waited unbounded (actionTimeout default 0), swallowing crafted failure context on pure hangs → 1s-bounded read + `use.actionTimeout: 15_000` backstop. |
| Observability | CLEAR | 0 | 0 | 0 | All Gate 1 conditions verified; PromQL matches MC catalog + recording site. Informational only: pre-existing `Err(_) => 0.0` masking in `26_mh_quic.rs` poll helpers → tracked in `docs/TODO.md` §Test Debt (test-owned, outside this diff). |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | Dead `let current = NaN` binding in mcMetrics poll loop; two `as string` casts in the spec → both fixed. ADR-0028/0030/0024 §6 compliant; Ownership Lens confirms all Mechanical rows. |
| DRY | RESOLVED-DEFERRED | 2 | 2 | 0* | Both true-duplication findings fixed (inline auth-response type → sdk-core `AuthTokenResponse`; port-5173 double-encoding → single `loopbackBaseUrl` export in env.ts). *Verdict DEFERRED solely for its own ADR-0019-lane extraction opportunity (see §Accepted Deferrals). |
| Operations | CLEAR | 0 | 0 | 0 | Both Gate 1 conditions honored verbatim; `test:e2e` verified unreachable from every default pipeline path; all waits bounded; fail-loud preconditions intact. |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | [credential-leak] failure messages echoed raw request URLs (would copy a URL-borne credential regression into assertion errors/CI logs) → shared `redact` helper at all four echo sites (two fix rounds; round 1 covered only one site and was sent back). |

**Gate 2 re-validation after review fixes**: `./scripts/layer-all.sh` re-run twice.
- Attempt 2: layers 1-5, 7 OK (full env-tests green against the live cluster); Layer 6 FAIL — new HIGH advisory GHSA-rgw5-rvv9-x895 (`brace-expansion` DoS via `nx`, patched ≥5.0.9) surfaced by advisory-DB drift, not this diff. Fixed by upgrade, not suppression: root `package.json` `pnpm.overrides` floor `>=5.0.8` → `>=5.0.9` + lockfile re-resolve (all paths 5.0.9). Security co-signed per ADR-0033 §11.
- Attempt 2 (re-run after fix): all layers green — LAYER=6 aggregate `N/A` is the worst-child ordering over documented self-justifying children (`pnpm audit STATUS=OK`, `buf breaking STATUS=OK`, rust `SKIPPED-NO-DIFF` no-dep-changes, proto intentional-gap `N/A`); exit 0.

**Files changed during review phase** (all re-validated): `e2e/fixtures.ts`, `e2e/env.ts`, `e2e/global-setup.ts`, `e2e/mcMetrics.ts`, `e2e/join-happy-path.spec.ts`, `playwright.config.ts`, `e2e/README.md` (prettier only), root `package.json` + `pnpm-lock.yaml` (audit fix), `docs/TODO.md` (reviewer-tracked entries).

---

## Accepted Deferrals

One accepted deferral exists (DRY reviewer's own ADR-0019-lane entry; zero implementer deferrals):

- **sdk-core wire-path constants re-encoded in E2E fixtures** → `docs/TODO.md` §Cross-Service Duplication (DRY), entry dated 2026-08-04. Extraction opportunity, not true duplication: `REGISTER_PATH`/`LOGIN_PATH`/`MEETINGS_PATH` are module-private in sdk-core (closed barrel — nothing importable exists), so `e2e/fixtures.ts` re-encodes the path literals. Fix is a client-owned sdk-core export, explicitly outside this devloop's scope per the Gate 1 scope decision; natural landing spot is task #19 or the next sdk-core http-surface change. Drift fails loudly today (waitForResponse timeout).

Not deferrals of this diff (informational, tracked for other owners): pre-existing `Err(_) => 0.0` Prometheus-error masking in `crates/env-tests/tests/26_mh_quic.rs` → `docs/TODO.md` §Test Debt (surfaced by @observability, whose verdict stays CLEAR; owner test, next env-tests devloop).

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `3f4fc92aa521085200bdb6f677d0132e41495534`
2. Review all changes: `git diff 3f4fc92..HEAD`
3. Soft reset (preserves changes): `git reset --soft 3f4fc92`
4. Hard reset (clean revert): `git reset --hard 3f4fc92`

---

## Issues Encountered & Resolutions

1. **`__dirname` under Playwright's ESM loader** — global-setup initially used `resolve(__dirname, ...)`; the package is `"type": "module"` so Playwright loads it as ESM and `__dirname` is undefined. Fixed with `fileURLToPath(new URL(..., import.meta.url))`. Caught by exercising the real failure path, not just typecheck.
2. **Task-text fingerprint env names don't exist** (`MC_CERT_SHA256_B64`/`MH_CERT_SHA256_B64`) — coded to the writer's canonical `MC_CERT_SHA256`/`MH_CERT_SHA256` via the reused JSON loader (SSoT; surfaced at Gate 1, approved).
3. **`team-lead` not directly reachable via SendMessage** — phase transitions relayed through the main conversation; coordinator relayed approval back.
4. **webServer starts before globalSetup** in this Playwright version (observed in the failure-path run) — harmless here (globalSetup checks cluster/fingerprints, not the Vite server), noted for #19.
5. **Gate 2 attempt 1: two Layer 3 guard reds, both fixed properly (no ignores).** (a) `no-secrets-in-ts` flagged `password: \`pw-${...}\`` in fixtures.ts (regex: secret identifier + any quoted/template RHS) — fixed by moving generation into a `randomPassword()` helper so the RHS is a call expression, not a literal (better shape regardless of the guard). (b) `validate-cross-boundary-scope` flagged `e2e/mcMetrics.ts` as in-diff-but-not-in-plan — the file was split out of fixtures.ts during implementation to satisfy the one-PromQL-home condition; classification row added (Mine, new). Both guards re-run locally: `STATUS=OK`.

---

## Lessons Learned

1. **Resume-from-main.md works as designed** — the interrupted session's Loop State (`Phase = review`, verdicts pending) was sufficient to respawn the roster and finish review + gates without redoing planning or implementation.
2. **Advisory-DB drift can red Gate 2 between sessions** with zero dependency changes in the diff (`brace-expansion` GHSA-rgw5-rvv9-x895 published between attempt 1 and the resumed re-validation). The upgrade-over-suppression path (bump the existing `pnpm.overrides` floor, security co-sign per ADR-0033 §11) resolved it inside the attempt budget; classification rows for root `package.json`/`pnpm-lock.yaml` were required to keep the Layer A scope-drift guard green.
3. **Diagnostic code paths need the same timeout discipline as assertions** — the review's highest-value finding class was failure-path quality (unbounded diagnostic read, un-redacted URL echoes), not happy-path correctness. Failure context that only renders when things hang must itself be bounded and redacted.
4. **Layer 6 aggregate `N/A` with green children is expected** worst-child-ordering behavior when the only ran verbs pass and the rest are documented skips — worth knowing at Gate 2 so a `TOTAL_RESULT=N/A`/exit-0 isn't mistaken for a wiring bug.
