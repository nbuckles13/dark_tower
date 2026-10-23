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
| s2 | Slot 0 polls to exactly **`fewer_sources_than_slots`** | **The positive control.** A specific, non-default wire token proves MC's assignment arrived and says "nobody to hear" — not merely "not awaiting-assignment", which is vacuous under R-3. `active` here would mean MC routed the client to itself. |
| s3 | `framesSent` and `framesAccepted` are both **flat** over a window after s2 | Egress is flat because MC directs no audio stream to a publisher nobody holds (a `SendDirective` listing audio with empty targets, §5), so no send instruction is ever applied — not because of mute. Ingress is flat because MC pushed no edge into this client. |
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
| `over-subscription.spec.ts` | **S2** under-fill and over-subscription | A receiver declaring N=2 (per-context `audioReceiveSlots` lever): with one sender, slot 1 reads `fewer_sources_than_slots` as an empty cell in a 2-cell grid; with three senders joined one at a time, its slots hold the two earliest, and the third has zero layer-1/2 at it WHILE the third's send counter and a full-N receiver's accepted-from-third advance. (The task text's "fewer-sources on the third" has no third slot at N=2; both R-2 clauses are covered instead.) |
| `server-mute.spec.ts` | **S3** server mute | The host clicks the real affordance; every receiver's slot for B reads `avatar_server_muted` and its roster row `data-server-muted=true` (`data-client-muted=unknown`); accepted-from-B is flat at every receiver AND B's own send counter is flat (story 3 MC-B: a server mute withdraws B's audio directive; MH's ingress drop is the backstop, proved by the Rust patched-publisher env-test 35) WHILE the other pairs advance; B's tone is absent; B's own client-mute indicator is unchanged. B's unmute request reaches the host and lifts nothing. The host's unmute restores B at all three layers. |
| `server-mute.spec.ts` | **Client STRUCTURAL mute** (restores the proof task 6 removed) | C mutes itself: `expectEgressFlatWhileMuted(C)` holds WHILE the other pairs advance; receivers see `avatar_client_muted` with `data-client-muted=true`. Unmute resumes. Compose: B client-mutes, is server-muted, client-unmutes — B's egress STAYS flat (the server mute still withholds the directive) and receivers stay flat, WHILE the other pairs advance. |
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
(refuses dials to exact-match offered handlers — it can only narrow),
`audioReceiveSlots` / `videoReceiveSlots` (a per-context N / M through the SDK's
strict parser) and `forceHostControls`.

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
create their meeting Node-side. The multi-party specs sign each context in from
the pre-registered cohort; sign-ins spend no AC registration budget (§Budgets).

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
single-attempt Layer-7 budget (its own wall clock,
`DEVLOOP_BROWSER_E2E_TIMEOUT`, default in `scripts/layer7.sh`). It **always runs** whenever Layer 7 runs (the
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
  accounts at the suite's one audio N (`SUITE_AUDIO_RECEIVE_SLOTS`), registered
  once per run by `global-setup.ts` — see §Budgets. `playwright.config.ts`
  derives the dev server's `VITE_DT_AUDIO_RECEIVE_SLOTS` from it and
  `VITE_DT_VIDEO_RECEIVE_SLOTS` from `SUITE_VIDEO_RECEIVE_SLOTS` (0: the story-2
  audio specs run with no video slots, story 3 R-33), and `readOwnTone` asserts
  the build's declared N and M at runtime (`expectDeclaredReceiveSlots`) because
  a reused server keeps whatever values it was started with. The retired
  `VITE_DT_RECEIVE_SLOTS` fails the web app's config load if set. The same
  fixture asserts MC advertised BOTH per-kind caps (`JoinResponse`
  `max_audio_receive_slots` / `max_video_receive_slots`) and that each admits
  its count (audio cap >= N, video cap >= M); an `unknown` cap of either kind is
  a failure, and a video cap of 0 reads `known`.
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

- **AC auth-rate budget — the unit is REGISTRATION ATTEMPTS per source IP,
  successful AND failed; sign-ins do not count.** AC's registration limit
  (`AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS` per
  `AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES`) is enforced by
  `count_registration_attempts_by_ip` in `crates/ac-service/src/repositories/auth_events.rs`,
  which counts `auth_events` rows whose `event_type` is `user_registered` or
  `user_registration_failed` (the `AuthEventType` SSoT in
  `crates/ac-service/src/models/mod.rs`) for the caller's IP, any `success`.
  `/user/token` sign-ins write `user_login` and **do not spend this budget**.
  A request the limiter itself refuses (429) writes no row. The SSoT for the
  Kind cluster is `infra/services/ac-service/config.env` (100 per 1 minute; the
  Kind overlay does not patch it; a prod-configured AC falls back to
  `crates/ac-service/src/config.rs` `DEFAULT_REGISTRATION_RATE_LIMIT_*`, far
  tighter). The bucket is per source IP, and all host traffic reaches AC from
  one address, so it is **shared with the Rust env-tests `scripts/layer7.sh`
  runs from the same host just before this suite** — their registrations in the
  preceding window count, **failed ones included** (any negative `/register`
  case: bad format, email already exists), and no static check here can see
  them.

  What the suite spends, by rule (the counts follow the specs; do not freeze a
  total here):
  - **Registrations.** V, the shared valid user (`fixtures.ts` `SHARED_USER`),
    once per worker process — so once on a green run, **plus one per failing
    test** (Playwright restarts the worker after a failure, re-creating the
    memo). `userB`, the distinct second party of the two-party test, and the
    throwaway account auth-rejection signs up to obtain a real retained token:
    one each. The **N+1 cohort**: `cohortSize(SUITE_AUDIO_RECEIVE_SLOTS)` = **4 at the
    suite's locked N=3**, registered **once per run in `global-setup.ts`**,
    whatever fails later.
  - **Failed registrations.** Any `/register` that reaches AC and is refused for
    its input (400/409) spends one. auth-rejection's `/register` is
    route-fulfilled in the browser (a 200 with a garbage token) and never reaches
    AC, so it spends nothing; no spec sends a failing `/register` to AC.
  - **Sign-ins spend nothing** here (AC's login limiter is a separate per-user
    failed-attempt count).

  **Setup-time fit check.** `global-setup.ts` reads the two keys from
  `config.env` (a missing or malformed key fails loudly) and refuses to start if
  the cohort's own worst-case window — its N+1 registrations
  (`cohortWindowSpend` in `cohort.ts`) — exceeds the limit. That is necessary,
  not sufficient: it catches N raised past the limit, or an AC limit below N+1,
  before any account exists, not the env-tests' share of the bucket. A **429
  fails loudly** with the budget named (`captureAccessToken` in `fixtures.ts`)
  and is **never retried** — a retry only spends more of the bucket.

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
  and scales with `SUITE_AUDIO_RECEIVE_SLOTS`, so raising N is a wall-clock decision as
  well as an auth-budget one.
- **Peak concurrency**: at most `cohortSize(SUITE_AUDIO_RECEIVE_SLOTS)` = 4 browsing
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

## Video test infrastructure (story 3, R-32 / R-3 / R-30)

Helpers only. The video specs that use them land with C-C1 (task 17). Every helper below is pure and has a node-tier self-test under `tests/`.

### Video test pattern (THE spec)

This prose is the pattern's single home. The client's generator restates it in its file header. The detector (`patternDetector.ts`) is written from this text and shares **no code** with the generator: it never imports it, and its self-test builds its inputs from its own transcription of this section. This mirrors `testTone.ts` / `toneDetector.ts`.

- **Input:** only the meeting-scoped `sender_id`, an integer in 1..65535. It is a visual watermark, never a durable identity: no user id, email, display name or token. The generator throws on 0 or out of range.
- **Geometry:** the whole encoded picture (directed width W, height H) is a 6-column × 4-row grid of 24 equal blocks. Block (c, r) spans x ∈ [⌊c·W/6⌋, ⌊(c+1)·W/6⌋) and y ∈ [⌊r·H/4⌋, ⌊(r+1)·H/4⌋). Its row-major index is i = r·6 + c.
- **Colours:** every block is one flat colour, pure black rgb(0,0,0) or pure white rgb(255,255,255). There are no gradients, text or borders, so nothing is smearable by VP8 at the Kind test profile. White = 1.
- **Assignment** (row-major):

  | row | c0  | c1  | c2  | c3  | c4  | c5  |
  | --- | --- | --- | --- | --- | --- | --- |
  | 0   | G0  | I15 | I14 | I13 | I12 | G1  |
  | 1   | I11 | I10 | I9  | I8  | I7  | I6  |
  | 2   | I5  | I4  | I3  | I2  | I1  | I0  |
  | 3   | G2  | C3  | C2  | C1  | C0  | G3  |
  - **I15..I0:** the 16 bits of `sender_id`, MSB first (index 1 = bit 15 … index 17 = bit 0). That is the full 16-bit space, so ids never collide.
  - **Guards:** the corners in a fixed checkerboard, G0 (top-left) = white, G1 (top-right) = black, G2 (bottom-left) = black, G3 (bottom-right) = white. They catch blank (all equal), inverted, mirrored and flipped frames.
  - **C3..C0:** the Gray code g = n ^ (n >> 1) of n = capturedFrameIndex mod 16, MSB first. It advances on every CAPTURED frame, so frames the encoder drops or a hop abandons simply skip values.
- **Gating (security, binding on the client tasks):** the test-pattern generator, and any browser patched-client harness (C-C1's server-mute evasion case), is reachable ONLY behind an opt-in build define resolved by `vite/testDefines.ts:resolveOptInTestDefine` — never default-on, never runtime, and a production build THROWS. Their synthesis and lever tokens join the FORBIDDEN list of `tests/bundle-content.test.ts`, each with a positive control, so the bundle test proves their absence.

### Render-surface luma projection (what the detector reads)

- **Where sampled:** the slot's **render** canvas (behind the SDK's `VideoSink` seam), over the rectangle the picture was drawn into (letterbox excluded). Never at the decoder output, so a frame painted into the wrong slot reads as the wrong id.
- **Per block:** the mean of Y = 0.299R + 0.587G + 0.114B over the block's central half in each dimension (x ∈ [bx + bw/4, bx + 3bw/4), and likewise for y). That keeps edge ringing and the floor() seams out. The value is rounded to an integer 0..255. There are exactly 24 values, row-major.
- **When present:** only while the slot is rendering video and at least one frame has been painted. **Absent, never zero-filled,** on avatar or empty slots. An all-zero array would read as "blank, guard fail", which is the wrong diagnosis.

### Detector rules (`patternDetector.ts`)

- **Thresholds:** a block is black at luma ≤ `BLACK_MAX` (80), white at ≥ `WHITE_MIN` (175), and **undecided** in between. The generator paints 0 / 255, and a canvas readback of VP8 flat blocks typically lands within ~0–20 / ~235–255. Each threshold therefore keeps ~60 of margin beyond observed codec drift. A sample with any undecided block is "not a pattern frame". It is never coerced to a bit.
- **One sample:** `decodePattern(lumas)` returns `{ decided, guardPass, id, counter }`. The id and counter are read only when the guard passes.
- **A window:** `judgeSlotPattern(samples, expectedId)` returns `{ idMatch, guardPass, liveness, decodedIds }`.
  - `guardPass` holds only if every sample passed.
  - Liveness counts only decided, guard-passing samples. It is `live` when there are at least 2 such samples and the counter differs across any pair. It never requires "+1" (R-12 abandonment skips frames). With fewer than 2 valid samples it is `insufficient`, reported apart from `frozen`.
  - At a 250 ms bus tick and 15 fps (~3.75 frames per tick; the counter cycles every 16 frames), a live stream reads equal on consecutive samples only on an exact 16k-frame alias.
- **Malformed input** (not 24 entries, NaN, null, a non-integer, outside 0..255) THROWS `PatternInputError`. A harness error is never a verdict.
- **Known non-catches:**
  - A 180° rotation preserves the corner checkerboard. It still misreads the id, so layer 3's expected-id check catches it.
  - A shifted or cropped frame is caught deterministically only where a corner lands on opposite content. Within a live window the bottom-right corner eventually lands on the moving counter column and fails, and the expected-id check backs it up.

### Slot oracle (`slotOracle.ts`): the rule source

MC never names the source of a live video slot (video ACTIVE carries no `sender_id`, R-5), so layer 1 needs the test's OWN expectation.

- **Rule source:** R-3's text and the MC-A SLOT TABLE bullets in `docs/user-stories/2026-10-07-see-each-other.md`, including the 2026-10-21 **KIND ORDER** amendment (lead ruling at T-A planning). It is never MC's code: the oracle imports nothing from `crates/mc-service`, and `tests/slotOracle.test.ts` enforces that.
- **Fill:** per subscriber S and kind K, (1) the earliest joiner already holding a slot of the other kind with S but none of K, else (2) the earliest eligible joiner holding no slot of K.
- **Eligibility:**
  - never S itself;
  - shares a connected handler with S (otherwise the pair is unreachable and consumes no slot);
  - audio slots take audio publishers only;
  - video slots take any participant, so receive-only and audio-only participants show as `avatar_no_video`.
- **Stability:** no join, leave, mute or other-kind redeclaration moves an occupied slot. Over-cap slots are `unavailable`. The server mute avatar wins over the client one.
- **Kind order:** `KIND_FILL_ORDER` = audio, then video, one slot at a time, to a fixed point. MC-A implements the same order.

### Video bus contract (`videoBusShape.ts`)

The client projects these under `__E2E_HOOKS__`, sampled on the existing tick and never per frame.

- **The events:**
  - `slotAssignments` entries gain `kind: 'audio' | 'video'`.
  - **`videoSlots`** is `{ slots: [{ slotId, mcState, rendered: 'video'|'avatar'|'empty', renderedSenderId?, framesDecoded, framesRendered, lumas? }] }`, with **no** timestamp; the bus replay order is the order.
  - Layer 2 reuses the existing `receiveLayers` `{ slot, senderId, keyed, verified }` rows.
  - `sendTracks` is `{ cameraTrackLive, micTrackOpen }`.
  - `videoCaptureSource` is `{ mode: 'camera'|'test_pattern', patternId? }`.
- **Exact-key parsers:** every parser rejects an unknown field, so a GoP value, a frame size, a key id or a timestamp cannot slip onto the bus unnoticed.
- **Identity carve-out (R-30 vs R-32):** `renderedSenderId`, `receiveLayers.senderId` and `patternId` carry the meeting-scoped `sender_id`, stringified, on the test-build bus only. This follows the story-2 `receiveLayers` precedent. There is never a stream number, relay id, GoP value, size, key id, timing or durable identity. The decoded pattern id is computed test-side and never written back to the bus.
- **Lumas exception** (security's conditions, binding on C-B / C-C1):
  - emitted only inside `__E2E_HOOKS__`;
  - **the `videoSlots` event type goes into the FORBIDDEN list of `tests/bundle-content.test.ts`**, with its positive control;
  - never sent off-page or logged;
  - no per-sample timestamp, frame index, GoP, key id or size;
  - absent on avatar and unavailable slots.
  - why it is acceptable: with a real camera this is a 6x4 greyscale thumbnail; no added exposure because same-page scripts can already read the canvas.
- **Test-pattern camera:** the synthetic capture track must stop and restart on exactly the withdrawal, mute and failure edges a real camera does. Otherwise every camera-light assertion is vacuous.

### Video evidence (`videoEvidence.ts` judges; Playwright side in `receiveEvidence.ts`)

The checks run per receiver, per ACTIVE video slot, against the oracle's expected occupant and that occupant's sender id **from the harness's own join binding**, never from the occupant's own announcement:

- **Layer 1:** the key-id sender of the rendered frames (`renderedSenderId`) is the expected sender. A frame from anyone else fails layer 1, and its lumas are never pooled into layer 3.
- **Layer 2:** the verified-and-decrypted count for (slot, expected sender) advances across the window.
- **Layer 3:** the detector reads the expected id, the guard passes, and the slot is live.
  - Only samples with `rendered === 'video'`, lumas present and the expected sender count. Leading avatar samples (avatar → active) are outside the window.
  - An `insufficient` window is a HARNESS error.

Separately, for every slot, the window's last sample must show the MC state token and the matching rendered kind (`active` → video, `avatar_*` → avatar, `fewer_sources_than_slots` / `unavailable` → empty). Unreachable senders come from `unreachableSenderIds`.

The send side:

- `cameraTrackLive` is exactly (directed AND NOT client-muted AND camera healthy), so an unwatched publisher's camera is off and a server-muted one's track has ended.
- The mic stays open through client and server audio mute.
- `videoCaptureSource.patternId` must equal the member's join-bound sender id, so a generator that drifts from `sender_id` fails loudly.

Windows reuse `windowSampling.ts`'s stop rule and count samples by order.

### Cohort plans (`cohort.ts`)

- **Plans:** a scenario gives each member a `PublishSet` (`audio` / `video` / `both` / `none` = receive-only) plus its own audio N and video M.
- **Registration:** the cohort is still registered once per run (global setup, AC's auth-rate limit read from AC config). `assertPlanFitsCohort` refuses a plan larger than the registered cohort rather than registering more.
- **Levers:** `leversForPlan` maps a member to the per-context levers `publish`, `audioReceiveSlots` (audio N, the `VITE_DT_AUDIO_RECEIVE_SLOTS` form) and `videoReceiveSlots` (parsed by the app since C-A1). The app's lever parser learns the new keys in the client's publish-path task.

## VP8 substrate probe (story 3 R-28)

The browser test substrate must be shown to ENCODE and DECODE VP8 at the profile the cluster directs. If it cannot, the browser layer fails loudly. The probe **never skips** and **never degrades the suite to audio-only**.

One module, `vp8Probe.ts`. It deliberately does not import the SDK or its VP8 support check: it is an independent check of the substrate, and the SDK's own fail-loud is tested in the client's task. Its in-page body (`runVp8ProbeInPage`) does the following:
- checks for a secure context and the WebCodecs constructors;
- calls `VideoEncoder.isConfigSupported` and `VideoDecoder.isConfigSupported` at the profile;
- then performs a REAL round trip, because isConfigSupported alone can lie. One synthetic `OffscreenCanvas` frame is encoded as a keyframe and decoded with the encoder's emitted decoder config, and the decoded display size is compared with the profile.

There is no getUserMedia, no camera and no browser flag. Each WebCodecs step is raced against `PROBE_STEP_BOUND_MS`, a liveness bound and never a latency gate. The observation carries booleans, numbers and enums only.

**Verdicts.** The result is `pass` or exactly one sub-check:

| Sub-check | Meaning |
|-----------|---------|
| `webcodecs_unavailable` | Not a secure context, or no WebCodecs constructors |
| `encoder_config_unsupported` | The encoder config query said no |
| `decoder_config_unsupported` | The decoder config query said no |
| `encode_failed` | Encoding errored, or produced nothing |
| `encode_timeout` | An encoder-side step HUNG |
| `decode_failed` | Decoding errored, or produced nothing |
| `decode_timeout` | A decoder-side step HUNG |
| `dimension_mismatch` | The decoded size differs from the profile |

`tests/vp8Probe.test.ts` runs the real body against fake WebCodecs on `globalThis`, under fake timers.

**Two callers, two profile sources, one reader** (`configEnv.ts:readVideoProfile`, which holds the five `MEDIA_VIDEO_*` keys once):

1. **Layer 7.** `global-setup.ts` runs the probe FIRST, before the cert check and before cohort registration.
   - **Profile source:** the DEPLOYED `media-codec-profile` ConfigMap. `scripts/layer7.sh` (Phase 1g) resolves its one live generation from the `mc-service` pods and dumps every key to `${DEVLOOP_TMP}/layer-7-media-codec-profile.env`, exported as `E2E_MEDIA_CODEC_PROFILE_ENV`. INF-C's Kind test-profile patch is therefore probed by construction.
   - **Missing source:** if the variable is unset, global-setup throws. There is deliberately no fallback to the repo file, which would probe the base profile instead of the Kind patch.
   - **Manual runs:** dump the live ConfigMap yourself and point the variable at the dump:
     `kubectl -n dark-tower get cm <media-codec-profile-HASH> -o json | jq -r '.data|to_entries[]|"\(.key)=\(.value)"' > /tmp/profile.env`
2. **CI.** `src/__tests__/vp8Probe.test.ts` runs in vitest browser mode under the existing `pnpm test:component` step of `ci-client.yml`'s Chromium job.
   - **Profile source:** the repo's BASE `infra/services/media-codec-profile/profile.env`, injected by `vitest.config.ts` as the `__E2E_VP8_PROBE_PROFILE__` define through `configEnv.ts:vp8ProbeProfileDefine`. The values are never restated, and the define is test-runner-only (absent from `vite.config.ts`).
   - **Strictness:** the test asserts `pass` exactly, and a missing define throws.

**Layer-7 routing (REASON tokens).**

| Token | When | Lane |
|-------|------|------|
| `media-codec-profile-unresolved` | Phase 1g cannot resolve exactly one deployed generation, or cannot read it. Decided pre-suite, so neither suite runs. | PRECONDITION_FAILURE, exit 2 |
| `browser-vp8-unsupported` | An UNSUPPORTED verdict, and only that. | PRECONDITION_FAILURE, exit 2 |

How an unsupported verdict is routed:
- Global-setup writes the verdict to `E2E_VP8_VERDICT_FILE` (`${DEVLOOP_TMP}/layer-7-vp8-verdict`, removed in Phase 1g before the suite can run). Line 1 is the token. Line 2 is the sub-check, validated by `layer7.sh` against its mirror of the list above. Then global-setup throws with remediation.
- Every harness error writes nothing and stays `FAIL` / `browser-e2e-failed`. That covers an unset variable, a malformed deployed profile, and a page crash.
- Routing is fail-closed. No file, any other line 1, or a line 2 outside the list stays `FAIL`. A verdict file next to a green suite is a loud `FAIL`.
- The token and the sub-check list are declared once in `vp8Probe.ts`. `tests/vp8Probe.test.ts` fails if `layer7.sh`'s mirror drifts.
- Remediation and the accepted residual (the probe runs after the Rust env-tests) are in `docs/runbooks/devloop-validation.md` §6.7.
