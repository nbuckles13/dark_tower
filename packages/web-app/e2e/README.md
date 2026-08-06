# Browser E2E (Playwright) — tasks #18/#19

Playwright specs that drive the **real product surface**: real Chromium, the real
WebTransport API (including `serverCertificateHashes` cert pinning), and the real
demo-page DOM, against the **live host-side Kind cluster** (ADR-0030). This is
the browser driver of the project's Env-Test tier (ADR-0028, 2026-06-25
amendment) — not a separate tier.

## Division of responsibilities vs the Rust env-tests

`crates/env-tests/tests/24_join_flow.rs` is **unchanged** by this suite and stays
authoritative for what it covers.

| Layer                                                                                                 | Rust env-tests (`crates/env-tests/`)            | Browser E2E (this directory)                   |
| ----------------------------------------------------------------------------------------------------- | ----------------------------------------------- | ---------------------------------------------- |
| Driver                                                                                                | Synthetic Rust client (`wtransport`, `reqwest`) | Real Chromium + the real `@darktower/sdk-core` |
| AC→GC→MC wire protocol, framing, token validation, negative cases                                     | ✅ `24_join_flow.rs`                            | Not re-tested here                             |
| MH QUIC data plane, MC↔MH coordination                                                                | ✅ `26_mh_quic.rs`                              | Not re-tested here                             |
| Real browser WebTransport stack + `serverCertificateHashes` pinning                                   | ❌ unreachable                                  | ✅                                             |
| Real demo DOM (sign-up → create → join views, testids)                                                | ❌ unreachable                                  | ✅                                             |
| Browser-SDK behavior end-to-end (token-based join, active/active MH connect, `MediaConnectionUpdate`) | ❌ unreachable                                  | ✅                                             |
| Multi-context roster propagation seen by a real client (`ParticipantJoined`)                          | partial (wire-level)                            | ✅ within 5s, via the E2E bus                  |

**Deliberate cross-driver parallel**: assertion (d) reads the same Prometheus
series — `mc_participant_mh_status_total{state="connected"}` — as
`26_mh_quic.rs` Scenario 7. Same series, different driver: the Rust test proves
MC's R-60 handler against a synthetic client; this suite proves the **real
browser SDK** feeds it. The PromQL lives in exactly one helper per language
(`e2e/mcMetrics.ts` here).

## What the happy-path spec asserts

`join-happy-path.spec.ts`, observed via the `window.__darktower_test__` replay
bus (`src/lib/e2eBus.ts`, compile-time gated by `__E2E_HOOKS__` — dev builds
only). Four focused tests (each its own fresh 120s budget):

**Test 1 — token-based join with active/active media (a/b/d/e):**

- **(a)** MC `JoinResponse` received with a `participant_id`
- **(b)** ≥1 successful MH WebTransport handshake for **every** URL in
  `media_servers` (active/active; MH count is cluster topology, not hardcoded)
- **(d)** the SDK's post-join `MediaConnectionUpdate` (state=CONNECTED) reached
  MC's handler — `mc_participant_mh_status_total{state="connected"}` rises above
  a baseline captured immediately before the join
- **(e)** token-only join (task #58 c-iii): in the join window, no request
  carries the raw email/password **values**, no `/api/v1/auth/*` endpoint is hit
  (the removed forced-login band-aid, commit 7b69288, must never come back), and
  GC requests authenticate via `Authorization: Bearer` only

**Test 2 — distinct-user two-party join + rendered roster (gaps 1 + 4):** a
second context joins as a GENUINELY DIFFERENT account (`userB`, freshly
registered — not a second sign-in of the same user). Asserts two distinct
`participant_id`s AND two distinct `user_id`s; **(c)** the first context sees
`ParticipantJoined` within **5s**; and the rendered roster DOM
(`participant-list` / `participant-${id}`) shows BOTH peers with correct display
names — each context renders its peer (the roster excludes self). The host (V)
is signed in, so its name in the peer's roster proves the meeting token carries
the registered name, not a client-side value.

**Test 3 — participant leave, MC counter, and clean rejoin (gap 2):** a second
participant joins, then departs (the demo's own teardown drives a clean
`session.disconnect()`, then the context is closed). The first context observes
the departure three ways — the `participantLeft` bus event, the roster DOM
removal, and MC's server-side `mc_participant_leaves_total` (all-reasons
monotonic sum) rising above a pre-departure baseline. Then a FRESH context
rejoins (via sign-in, no client displayName): a NEW `participant_id`, a fresh
`joined`, and the roster DOM shows the rejoined peer's TOKEN-CARRIED registered
name (the sign-up path alone would stay green if the name were band-aided
client-side — task 63 second variant).

**Test 4 — bootstrap-join:** a meeting created via direct GC `POST` (not the
demo UI) is joinable from the browser, with the token-only invariant held.

## What the negative-path specs assert (task #19, R-45)

Each spec pins the **typed** error discriminant (the `CODE:` prefix `errorText`
renders — never free message text) plus at least one status-exact or
server-side observation, so a dead cluster / rate-limit 429 / wrong-hop
rejection cannot false-pass it:

- **`auth-rejection.spec.ts`** (0 registrations) — both credential-rejection
  hops. Test A: the demo retains a runtime-generated garbage token (the
  register exchange is route-fulfilled; AC is never hit) and joins → GC responds
  exactly **401** and the demo drops the session (the session-drop policy fires
  ONLY on `MeetingUnauthorizedError` — that behavior IS the typed-error
  assertion). Test B: sign-in with never-registered credentials → AC responds
  exactly **401** and `last-error` renders the typed `AUTH:` prefix (the literal
  R-45 "typed AuthError in the demo UI" surface).
- **`meeting-not-found.spec.ts`** (0 registrations — reuses V) — a real user
  joins a random well-formed code → GC responds exactly **404**, `last-error`
  renders the typed `MEETING:` prefix and **stays** rendered (nav-signin must NOT
  reappear: a 404 is not a credential rejection, discriminating this path from
  the 401 session drop).
- **`mc-token-rejection.spec.ts`** (0 registrations — reuses V) — a real token +
  real `mcAssignment`, but the GC join response's `meetingId` is route-rewritten
  to garbage: MC's step-6 binding check rejects with the bounded generic
  `Unauthorized` → `last-error` renders the typed `SIGNALING:` prefix **and**
  MC's own `mc_session_join_failures_total{error_type="jwt_validation"}` rises
  above a pre-join baseline (the server-side proof — a dead/unreachable MC also
  yields `SIGNALING:` in the DOM but structurally cannot increment its own
  rejection counter).

All three additionally assert that no `joined` bus event exists (a
point-in-time scan after each spec's terminal condition — never a
wait-for-absence).

**Join-after-error recovery (gap 3):** each negative spec ends with a recovery
tail — after the asserted failure, a valid join in the SAME page session (no
reload) succeeds (`joined` observed). Because `MeetingSession` is single-use, the
recovery remounts the join view (`recoverByJoining`). meeting-not-found and
mc-token-rejection reuse their retained session (the latter clears the meetingId
rewrite first); auth-rejection Test A re-authenticates as V after the session
drop. This proves a failed join is not a dead end.

## Prerequisites (host-side)

1. **Kind cluster with AC+GC+MC+MH _and_ the observability stack**:
   `infra/kind/scripts/setup.sh`. Prometheus is **required** (assertion (d));
   do **not** use `--skip-observability`.
2. **Dev WebTransport certs**: `scripts/generate-dev-certs.sh` (writes
   `infra/docker/certs/fingerprints.json` + `fingerprints.env`; the canonical
   keys are `MC_CERT_SHA256` / `MH_CERT_SHA256`). The harness reads
   `fingerprints.json` through the same loader as the Vite config
   (`vite/fingerprints.ts`) — one writer, one parser. Regenerated certs require
   a Vite dev-server restart.
3. **Playwright Chromium**: `pnpm exec playwright install chromium` (one-time).

No `/etc/hosts` entry is needed for the tests themselves — Chromium resolves
`*.localhost` natively. (The `demo.localhost` hosts entry from the web-app
README is for manually browsing with other tools.)

## Running

```bash
cd packages/web-app
pnpm test:e2e            # or: pnpm exec playwright test
```

Playwright starts `pnpm dev` itself (or reuses a running one — it **must** be
dev mode: a `pnpm preview`/prod server has the `__E2E_HOOKS__` bus
dead-code-eliminated and every spec fails in `waitForJoined` with a message
naming this failure mode).

**Pipeline wiring (task #19, R-48).** `scripts/layer7.sh` runs this suite as
its second Phase-2 step: sequentially **after** `cargo test -p env-tests
--features all`, against the same live Kind cluster, under the same
single-attempt Layer-7 budget (its own 600s wall clock,
`DEVLOOP_BROWSER_E2E_TIMEOUT`). It is **diff-triggered** — it runs only when
the diff touches `packages/**`/TS root manifests (via `lang/ts/changed.sh`),
`proto/`, or a browser-observable backend crate (the
`__BROWSER_E2E_TRIGGER_PATHS` array in `layer7.sh` is the one encoding); an
untriggered diff emits an explicit `STATUS=SKIPPED-NO-DIFF
REASON=browser-e2e-no-diff` child line. If the Rust env-tests fail first, the
browser suite is not run that attempt (loud `browser-e2e-not-run:` stderr note;
it runs on the retry). Missing dev certs or a missing Playwright Chromium
surface as `PRECONDITION_FAILURE` (operator lane) **before** either suite runs
— never as a cryptic spec timeout. Triage: `docs/runbooks/devloop-validation.md`
§6.7. The layer exports `E2E_*`/`VITE_*_PROXY_TARGET` from the helper's
ports.json, so a pipeline run needs none of the manual env knobs below.

### Environment knobs (defaults = static Kind config)

| Variable             | Default                      | Purpose                                           |
| -------------------- | ---------------------------- | ------------------------------------------------- |
| `E2E_BASE_URL`       | `http://demo.localhost:5173` | Vite-served demo (Host carries the org subdomain) |
| `E2E_AC_URL`         | `http://127.0.0.1:8443`      | AC NodePort (health probe)                        |
| `E2E_GC_URL`         | `http://127.0.0.1:8444`      | GC NodePort (health probe + `bootstrapMeeting`)   |
| `E2E_PROMETHEUS_URL` | `http://127.0.0.1:9090`      | Assertion (d) counter reads                       |

Defaults trace to `infra/kind/kind-config.yaml` (SSoT), mirrored by
`vite.config.ts` (dev proxy) and `crates/env-tests/src/cluster.rs` (Rust
counterpart). MC/MH endpoints come exclusively from the join response's
`media_servers`.

## Budgets and policies

- **Registration budget: 2 throwaway users per run.** The suite registers
  exactly two AC accounts:
  - **V** — the shared valid user (`fixtures.ts` `SHARED_USER`), registered ONCE
    per run via `authAsSharedUser` (in an isolated, route-mock-safe throwaway
    context) and reused by every "needs a valid session" role through
    `signInViaUi` (0 further registrations): the happy-path host, the
    bootstrap-join, the leave/rejoin participants, and the negative specs
    (meeting-not-found, mc-token-rejection, and auth-rejection's recovery tail).
  - **userB** — the ONE genuinely-distinct second party, registered by the
    distinct-user two-party test so it can assert two different accounts (distinct
    `user_id`s), not two sessions of the same user.

  **Why not more:** every recovery tail (gap 3) reuses a live/valid session — the
  meeting-not-found and mc-token-rejection recoveries reuse their spec's retained
  token; auth-rejection's recovery reuses V. The leave/rejoin second participant
  is a second V *session* (participant identity is minted per-join), so it costs 0.

  **workers=1 ↔ register-once coupling (do not decouple silently):** "V once per
  run" holds only because `workers: 1` gives the whole run a single worker
  process, so `fixtures.ts`'s `sharedRegistration` memo is a per-run singleton.
  Raising `workers` would give each worker its own module instance and re-register
  V per worker — move the memo to a global-setup / setup-project first (see the
  comment at `SHARED_USER` and in `playwright.config.ts`).

  The rate-limit **SSoT is the AC config the target cluster actually runs**: the
  Kind cluster this suite targets ships
  `infra/services/ac-service/configmap.yaml`
  (`AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS: "100"` per 1-minute window,
  relaxed for dev/test), so consecutive runs and the Layer-7 retry are safe. A
  **prod-configured AC** target instead gets the production default of 5 per
  60-minute window (`crates/ac-service/src/config.rs`
  `DEFAULT_REGISTRATION_RATE_LIMIT_*`) — at 2/run even the Layer-7 retry
  (a second in-window run → 4 registrations) stays under that prod default,
  a margin the previous 4/run budget did not have. A 429 there still cannot
  false-pass a negative spec (their 401/404-exact assertions reject it). This
  suite assumes a **single workstation against its own cluster**; it is not
  designed for concurrent runs sharing one cluster's rate-limit budget.
- **`retries: 0`** (ADR-0028): a failure is real. Fix it or delete the test —
  never mask with retries.
- **Timeouts**: 120s/test ceiling; assertion-meaningful waits are tighter (5s
  roster propagation; 60s Prometheus budget for the join-side counter matching
  the Rust Scenario 7 helper; the departure-side leave counter uses a wider ~90s
  ceiling — a leave's worst case includes MC's disconnect grace path, so the
  budget is derived from `MC_QUIC_MAX_IDLE_TIMEOUT + MC_DISCONNECT_GRACE_PERIOD +
  grace-check + scrape SLA`; see `mcMetrics.ts` and
  `docs/observability/metrics/mc-service.md`. The leave/rejoin spec drives a
  clean close, so in practice it settles in ~1 scrape interval).

## Artifacts & triage

Failures leave traces/screenshots in `test-results/` (gitignored):

```bash
pnpm exec playwright show-trace test-results/<test-dir>/trace.zip
```

**Artifact hygiene**: retain-on-failure traces capture full network
request/response **bodies** and therefore CAN contain live credentials — real
user and meeting tokens for the cluster the run targeted — **local-only,
gitignored, treat as sensitive**. They persist across pipeline runs by design
(they live outside `DEVLOOP_TMP`'s per-run `layer-*.log` cleanup — that is what
makes them useful for triage). The tokens are disposable per-run values for a
local dev cluster, but do not promote artifacts to shared storage or attach
them to issues.
