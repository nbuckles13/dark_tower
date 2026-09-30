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

## What the solo-participant spec asserts (ADR-0036 story 2 R-3: loopback removed)

`solo-participant.spec.ts` (story 1's `media-loopback.spec.ts`, renamed in story 2
task 15: story 1's hear-yourself expectation is superseded by R-3). Its subject
is the **solo case** of the multi-party model: a participant never hears its own
audio, so alone in a meeting it hears nothing — explicitly — and is directed to
send nothing.

Capture is synthesized and the microphone permission auto-granted by the
`--use-fake-device-for-media-stream` / `--use-fake-ui-for-media-stream` launch flags
already in `playwright.config.ts`. **No setting that weakens certificate validation,
web security or origin trust appears anywhere in this suite** — MC/MH trust flows
exclusively through `serverCertificateHashes` pinning.

| # | Assertion | Why it is shaped this way |
|---|---|---|
| s1 | `joined.mediaServers` is non-empty and every entry connects | R-33: every participant is offered the meeting's whole handler set and dials all of it. No handler COUNT is asserted — `env.ts` keeps topology out of this tier. |
| s2 | Slot 0 polls to exactly **`fewer_sources`** | **The positive control.** A specific, non-default wire token proves MC's assignment arrived and says "nobody to hear" — not merely "not awaiting-assignment", which is vacuous under R-3. `active` here would mean MC routed the client to itself. |
| s3 | `framesSent` and `framesAccepted` are both **flat** over a window after s2 | Egress is flat because MC directs no audio stream to a publisher nobody holds (a `SendDirective` with no streams, §5), so no send instruction is ever applied — not because of mute. Ingress is flat because MC pushed no edge into this client. |
| s4 | The window contains **≥ 4 samples**, asserted separately (shared `expectCountersFlatOverWindow`) | Flatness over zero samples is vacuously true, so a stalled sampler must be reported as a harness failure. |

Browser-tier proof of client STRUCTURAL mute (egress flat while muted, then
resuming) needs someone to hold the muted client in a slot, so it lives in
`server-mute.spec.ts` ("client STRUCTURAL mute") — see below.

## What the multi-party specs assert (story 2 task 15: S1, S2, S3, S6, S10a)

One browsing context per cohort member (`e2e/cohortContexts.ts`), each built by
the same Playwright-started dev server with three build knobs asserted at runtime
before any media assertion (`expectBuildKnobs`, `readOwnTone`): the test tone
(`DT_TEST_TONE=1`), the per-context test levers (`DT_TEST_LEVERS=1`) and SDK
telemetry (`VITE_TELEMETRY_ENDPOINT`). A reused dev server started without them
fails naming that cause.

**The evidence rule** (`crates/env-tests/README.md`): every claim about what ONE
participant receives from ONE sender comes from that receiver's own E2E bus —
`receiveLayers` (layer 1 `keyed`, layer 2 `verified`, per observed slot and
sender), `receiveAnalysis` (layer 3, the decoded tone, through the pure
`toneDetector.ts`) and `mediaFrameCounts` (the send counter). Prometheus client
series are fleet aggregates (the collector is their `instance`, and they carry no
participant label), so they prove only the pipe, the exported NAME and the
alert's applicability. Every "flat"/"zero" claim is sampled together with a
probe that must ADVANCE in the same window ON THE SAME PAGE (a stalled page
sampler repeats a stale snapshot; its own mover is what catches that), over a
≥ 4-sample floor (`receiveEvidence.ts:observeWindow`). Every multi-party test takes the MH
admission baseline BEFORE its joins, so a missing sender is diagnosed (budget
rejection / misrouting / unobservable) automatically.

| Spec | Scenario | What is asserted |
|---|---|---|
| `multi-party-hear.spec.ts` | **S1** hear by content, N=3, 4 participants | All 12 ordered pairs at all three layers at the slot MC assigned, and no frame keyed to a sender on any other slot. First-media time per receiver is recorded as an annotation — observed, never gated. **R-27 part 1**: the send counter rises in Prometheus under its exact `_total` name (a solo client sends nothing under R-3, so this runs here). **R-27 part 2**: the LOADED `MCMediaMissingKeyMaterial` rule selects exactly the names in `clientMetricNames.ts`, and the received counter rises under that exact name. |
| `over-subscription.spec.ts` | **S2** under-fill and over-subscription | A receiver declaring N=2 (per-context `receiveSlots` lever): with one sender, slot 1 reads `fewer_sources` as an empty cell in a 2-cell grid; with three senders joined one at a time, its slots hold the two earliest, and the third has zero layer-1/2 at it WHILE the third's send counter and a full-N receiver's accepted-from-third advance. (The task text's "fewer-sources on the third" has no third slot at N=2; both R-2 clauses are covered instead.) |
| `server-mute.spec.ts` | **S3** server mute | The host clicks the real affordance; every receiver's slot for B reads `source_muted` and its roster row `data-server-muted=true` (`data-client-muted=unknown`); accepted-from-B is flat at every receiver WHILE B's send counter and the other pairs advance; B's tone is absent; B's own client-mute indicator is unchanged. B's unmute request reaches the host and lifts nothing. The host's unmute restores B at all three layers. |
| `server-mute.spec.ts` | **Client STRUCTURAL mute** (restores the proof task 6 removed) | C mutes itself: `expectEgressFlatWhileMuted(C)` holds WHILE the other pairs advance; receivers see `source_muted` with `data-client-muted=true`. Unmute resumes. Compose: B client-mutes, is server-muted, client-unmutes — B's egress resumes, receivers stay flat. |
| `server-mute.spec.ts` | **Non-host refusal** (@security M2) | The `forceHostControls` lever renders the affordance for a non-host; MC answers `FORBIDDEN`; the refused client stays joined (post-join `FORBIDDEN` no longer closes the connection) and B keeps being heard. |
| `partial-connectivity.spec.ts` | **S10a** partial connectivity | A on every handler; B and C each block one (the `blockHandlers` lever, by exact match against A's offered list). Connected sets proven first; A hears B and C, B and C each hear A (all layers); B and C see each other with `data-reachability=source_unreachable` from `unreachable_sender_ids`, in no slot, not muted; C's accepted count at B stays zero WHILE A's advances (and vice versa). |
| `kek-rotation.spec.ts` | **S6** joiner half | A leave; both remaining clients' bus `kekGeneration` advance to the same generation; `dt_client_media_kek_updates_total{source="kek_update"}` and `dt_client_media_kek_generations_retained_total` each rise by ≥ the remaining-client count (a baseline→delta, not `increase()`, which cannot see a series born in the window); a joiner after the rotation holds that generation and opens every remaining sender's frames at all three layers. Budget from MC's config: grace + W. |

**The token-only scan now covers telemetry** (`credentialScan.ts`): every
recorded join window forces a metric export (`flushTelemetry` on the E2E bus)
before it closes, and the scan fails with `telemetry-not-recorded` when no
`/api/v1/telemetry` request was recorded, and `telemetry-not-bearer` when one
carried no `Authorization: Bearer` header — the needle scan and the Bearer check
are counted per surface, so "ran over it" is observable.

**The test levers** (`src/lib/testLevers.ts`): one opt-in build define,
`__DT_TEST_LEVERS__` (`DT_TEST_LEVERS=1`; THROWS in production; absent from the
production bundle — `tests/bundle-content.test.ts`), gating one init-script
global `window.__dt_test_levers__` read once per session: `blockHandlers`
(refuses dials to exact-match offered handlers — it can only narrow), `receiveSlots`
(a per-context N through the SDK's strict parser) and `forceHostControls`.

**Traces and HARs** from these multi-context runs contain several live meeting
and user tokens at once: they stay gitignored, local-only and are never promoted
to CI artifacts or attached to issues.

**Timing constants** live at the top of `fixtures.ts`'s media section: a 750 ms
post-mute settle (frames already queued at the instant of mute may still drain —
§5 stops *capture* within one frame, which is not the same as un-queueing), a
2 500 ms observation window, and a 4-sample floor. Both windows —
`observeWindow` and `expectCountersFlatOverWindow` — stop only when the window has
elapsed AND the floor is met; slow reads or a slipping in-page sampler under load
extend them up to `FLAT_WINDOW_MAX_OBSERVE_FACTOR` × the span, where they fail as
a `HARNESS:` error — the floor is never lowered (stop rule:
`e2e/windowSampling.ts:windowStep`). `observeWindow` also races each read against
that bound, so a hung read fails there too.

**Registration cost (solo spec): 0.** Both tests sign in as the shared user V and
create their meeting Node-side. The multi-party specs spend one sign-in per
context from the pre-registered cohort (§Budgets).

## Prerequisites (host-side)

1. **Kind cluster with AC+GC+MC+MH _and_ the observability stack**:
   `infra/kind/scripts/setup.sh`. Prometheus is **required** (assertion (d));
   the environment root always deploys it, so if it is missing the stack is
   not up or not Ready yet.
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

**Corollary — the one rule for Node-side hops.** That native resolution is a
**browser** behavior; **Node does not resolve `*.localhost`**. The per-run org
subdomain (`e2e-<hex>.localhost`) therefore resolves in Chromium and NOWHERE
else, and no hosts entry can help — the label is random per run. So any code in
this harness that fetches a browser URL from the **Node** side — `route.fetch()`,
`APIRequestContext`, a bare `fetch()` built from `e2eEnv.baseUrl` — must route it
through `toLoopbackUrl()` in `env.ts`, which owns the one hostname swap.
Skipping it yields `getaddrinfo ENOTFOUND e2e-<hex>.localhost` at that call, with
every browser-side spec around it still green. Node-side calls that already
target a NodePort (`E2E_AC_URL` / `E2E_GC_URL` / `E2E_PROMETHEUS_URL`, all
loopback by default) are unaffected — the rule applies only to the page origin.

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
`DEVLOOP_BROWSER_E2E_TIMEOUT`). It **always runs** whenever Layer 7 runs (the
diff-trigger was retired 2026-08-20 — gate coverage is independent of
change-detection, so there is no longer a "no diff" skip for the browser suite).
If the Rust env-tests fail first, the browser suite is not run that attempt
(loud `browser-e2e-not-run:` stderr note; it runs on the retry). Missing dev certs or a missing Playwright Chromium
surface as `PRECONDITION_FAILURE` (operator lane) **before** either suite runs
— never as a cryptic spec timeout. Triage: `docs/runbooks/devloop-validation.md`
§6.7. The layer exports `E2E_*`/`VITE_*_PROXY_TARGET` from the helper's
ports.json, so a pipeline run needs none of the manual env knobs below —
**except `E2E_ORG_SUBDOMAIN`, which has no default and no fallback** (R-7).

### Environment knobs (defaults = static Kind config)

| Variable              | Default                                        | Purpose                                                     |
| --------------------- | ---------------------------------------------- | ----------------------------------------------------------- |
| `E2E_ORG_SUBDOMAIN`   | **required — no default**                      | The per-run organization. `env.ts` throws if unset or blank. |
| `E2E_BASE_URL`        | `http://${E2E_ORG_SUBDOMAIN}.localhost:5173`   | **Derived.** Vite-served app; the Host carries the org.      |
| `E2E_AC_URL`          | `http://127.0.0.1:8443`                        | AC NodePort (health probe)                                   |
| `E2E_GC_URL`          | `http://127.0.0.1:8444`                        | GC NodePort (health probe + `bootstrapMeeting`)              |
| `E2E_PROMETHEUS_URL`  | `http://127.0.0.1:9090`                        | Assertion (d) counter reads                                  |

**`E2E_ORG_SUBDOMAIN` is required on purpose.** Layer 7 provisions a fresh
organization per run (`scripts/layer7.sh` Phase 1h) because no production code
marks a meeting ROW ended (MC's meeting-end notify, story 2 task 12, ends only
GC's MC-assignment row, never `meetings.status`, which is what the cap counts),
so an org's live-meeting count only climbs toward
`max_concurrent_meetings` — this suite creates ~7 meetings per run, and a second
run against a cap of 10 used to fail partway with a 403 the pipeline blamed on
the diff. A default here would silently restore that: green on the first run of
the day, 403 later. Running the suite by hand therefore needs an explicit value,
e.g. `E2E_ORG_SUBDOMAIN=demo pnpm --filter @darktower/web-app test:e2e` against
a hand-seeded org.

`E2E_BASE_URL` **derives** from `E2E_ORG_SUBDOMAIN` rather than being a second
knob. Setting both is allowed but cross-checked: `env.ts` throws if the base
URL's first host label disagrees with the subdomain. That is a correctness
constraint, not tidiness — `vite.config.ts:52-57` proxies `/api/v1/auth` with
`changeOrigin: false` so the Host reaches AC for ADR-0020 org extraction, so two
values that merely *happen* to agree would let sign-up fill one org into the form
while AC resolved a different one from the Host.

The throw fires at Playwright **config-load** (`playwright.config.ts` imports
`env.ts`), i.e. before any browser launches — a named error rather than a spec
timeout.

Defaults trace to `infra/kind/kind-config.yaml` (SSoT), mirrored by
`vite.config.ts` (dev proxy) and `crates/env-tests/src/cluster.rs` (Rust
counterpart). MC/MH endpoints come exclusively from the join response's
`media_servers`.

## Multi-party harness (story 2, R-30)

Building blocks for the N+1 specs; each module header carries the detail.

- **Cohort** (`cohort.ts`, `fixtures.ts` `authAsCohortMember`): N+1 distinct
  accounts at the suite's one N (`SUITE_RECEIVE_SLOTS`), registered once per run
  by `global-setup.ts` — see §Budgets. `playwright.config.ts` derives the dev
  server's `VITE_DT_RECEIVE_SLOTS` from it, and `readOwnTone` asserts the build's
  declared N at runtime (`expectDeclaredReceiveSlots`) because a reused server
  keeps whatever N it was started with.
- **Layer 3 tone detector** (`toneDetector.ts`, `fixtures.ts` `readOwnTone` /
  `expectHearsSender`): reads the bus's per-lane dB spectrum; two-sided (the
  sender's announced tone is dominant in its lane AND no other cohort tone — the
  receiver's own included — is present). Expected tones come from each
  participant's own `captureSource.toneHz`; the module shares no code with the
  SDK's tone derivation or synthesis (enforced by `tests/toneDetector.test.ts`).
  A cohort whose tones cannot be told apart fails as a precondition, never as
  misrouting.
- **S1 diagnostic** (`s1Diagnostic.ts`, `mcMetrics.ts`
  `mhAdmissionRejectionsByInstance` / `diagnoseMissingSender`): for a missing
  sender, reads MH's stream-admission rejection counter and reports
  config-caused budget rejection, misrouting, or unobservable (MH not, or only
  partly, scraped). Take `mhAdmissionRejectionsByInstance()` BEFORE the
  scenario's joins and pass it to `expectHearsSender` as `admissionBaseline`;
  a missing-sender failure then carries the report automatically.

## Budgets and policies

- **AC auth-rate budget — the unit is SUCCESSFUL TOKEN ISSUES per source IP,
  not registrations.** AC's "registration" limit
  (`AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS` per
  `AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES`) is enforced by
  `count_registrations_from_ip` in `crates/ac-service/src/services/user_service.rs`,
  which counts `auth_events` rows with `event_type='user_login' AND success=true`
  for the caller's IP — and BOTH `/register` and `/user/token` write one. So
  **every sign-in spends budget exactly like a registration.** The SSoT for the
  Kind cluster is `infra/services/ac-service/config.env` (the Kind overlay does
  not patch it; a prod-configured AC falls back to
  `crates/ac-service/src/config.rs` `DEFAULT_REGISTRATION_RATE_LIMIT_*`, far
  tighter). The bucket is per source IP, and all host traffic reaches AC from
  one address, so it is **shared with the Rust env-tests `scripts/layer7.sh`
  runs from the same host just before this suite** — their logins in the
  preceding window count, and no static check here can see them.

  What the suite spends, by rule (the counts follow the specs; do not freeze a
  total here):
  - **Registrations.** V, the shared valid user (`fixtures.ts` `SHARED_USER`),
    once per worker process — so once on a green run, **plus one per failing
    test** (Playwright restarts the worker after a failure, re-creating the
    memo). `userB`, the distinct second party of the two-party test, and the
    throwaway account auth-rejection signs up to obtain a real retained token:
    one each. The **N+1 cohort**: `cohortSize(SUITE_RECEIVE_SLOTS)` = **4 at the
    suite's locked N=3**, registered **once per run in `global-setup.ts`**,
    whatever fails later.
  - **Sign-ins.** One per `authAsSharedUser` / `authAsCohortMember` call — i.e.
    one per browsing context a spec signs in. A multi-party test signs in all
    N+1 members (the auth session is per-context in-memory state; there is no
    sign-in-free way in). Rejected sign-ins (auth-rejection's never-registered
    user) do not count: the row is `success=false`.

  **Setup-time fit check.** `global-setup.ts` reads the two keys from
  `config.env` (a missing or malformed key fails loudly) and refuses to start if
  the cohort's own worst-case window — N+1 registrations plus the first
  multi-party test's N+1 sign-ins (`cohortWindowSpend` in `cohort.ts`) — exceeds
  the limit. That is necessary, not sufficient: it catches N raised past the
  limit or a prod-default AC before any account exists, not the env-tests'
  share of the bucket. A **429 fails loudly** with the budget named
  (`captureAccessToken` in `fixtures.ts`) and is **never retried** — a retry only
  spends more of the bucket.

  **Why the cohort is in global setup, not a worker memo:** the worker restart
  after a failing test would re-register all N+1 accounts per failure. Global
  setup runs once per run (after Playwright's `webServer` is up) and hands the
  credentials to the workers through `E2E_COHORT_CREDENTIALS` (`COHORT_ENV_VAR`),
  Playwright's documented globalSetup→worker channel. The credentials are
  per-run throwaways in the per-run org, never logged; the decoder refuses a
  short, malformed or duplicated cohort rather than letting two participants
  share an account (which would read as a routing bug).

  **workers=1; escalate by shards — with care.** `workers: 1` stays
  (`playwright.config.ts`): a second worker would re-register V. More capacity
  comes from **shards**, but shards against the **same cluster from the same
  host share the one per-IP bucket**, so they add auth headroom only when each
  shard targets its own cluster. This suite assumes a single workstation against
  its own cluster.

- **Wall-clock budget** — measured on the Layer-7 Kind cluster (story 2 task 15,
  2026-09-30), per test: S1 `multi-party-hear` ≈ 13 s; S2 `over-subscription` ≈ 8 s;
  S3 `server-mute` ≈ 16 s + client structural mute ≈ 11 s + non-host refusal ≈ 6 s;
  S10a `partial-connectivity` ≈ 7 s; **S6 `kek-rotation` ≈ 100 s** (it waits out
  MC's disconnect grace plus the rotation debounce W, both read from MC's config —
  see the spec header); `solo-participant` ≈ 5 s; the story-1 specs ≈ 60 s
  (dominated by the leave/rejoin test's grace path). **Whole suite ≈ 3.5–4 min**,
  including the dev-server start and global setup, against the suite-wide
  `BROWSER_E2E_TIMEOUT` (`scripts/layer7.sh`, `DEVLOOP_BROWSER_E2E_TIMEOUT`) —
  under 40 % of it, so the default is deliberately NOT raised. **Why the numbers
  are written down**: on budget exhaustion `layer7.sh` prints "exited 124" and
  emits `FAIL browser-e2e-failed` — the *same* terminal status as a genuine
  assertion failure, in the implementer lane — so a timeout presents as a diff
  bug and sends triage hunting a defect that does not exist. S6 scales with
  `MC_KEK_ROTATION_DEBOUNCE_SECONDS`: raising W (or the grace) is a wall-clock
  decision for this suite. If headroom gets tight the fix is a deliberate
  `DEVLOOP_BROWSER_E2E_TIMEOUT` change, not a discovery at 3am.
- **Cohort registration wall clock**: `global-setup.ts` runs N+1 full sign-up
  UI flows (a fresh context and page load each) before any spec — measured at
  **≈ 4 s at N=3** (2026-09-30). It comes out of the same `BROWSER_E2E_TIMEOUT`
  and scales with `SUITE_RECEIVE_SLOTS`, so raising N is a wall-clock decision as
  well as an auth-budget one.
- **Peak concurrency**: at most `cohortSize(SUITE_RECEIVE_SLOTS)` = 4 browsing
  contexts at once (S1, S2, S3), each left gracefully and closed in a `finally`
  before the next test opens its own (`e2e/cohortContexts.ts`) — so peaks never
  stack, and a leaver's MH connections (so its edges) drop at once. RESIDUAL:
  MC today classifies the SDK's clean close as `server_initiated` and holds the
  participant through its 30 s disconnect grace before the roster removal (a
  meeting-controller follow-up in `docs/TODO.md`), so a previous test's
  participant can outlive its test on MC; the S1 diagnostic's budget verdict is
  the backstop if that ever reaches MH admission.
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
them to issues. Since R-27 the telemetry path is also a token carrier: every
metric export to `/api/v1/telemetry/v1/metrics` sends the user bearer in its
`Authorization` **header** (`createMetricExporter` in
`packages/sdk-core/src/telemetry/telemetryConfig.ts`), roughly every 10 s for the
life of each session — so a retained trace holds many copies of the token in
headers, not one per join in bodies, which is why the policy above applies with
more force, not less.
