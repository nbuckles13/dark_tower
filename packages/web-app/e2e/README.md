# Browser E2E (Playwright) — task #18

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
only):

- **(a)** MC `JoinResponse` received with a `participant_id`
- **(b)** ≥1 successful MH WebTransport handshake for **every** URL in
  `media_servers` (active/active; MH count is cluster topology, not hardcoded)
- **(c)** a second browser context joining the same meeting fires
  `ParticipantJoined` in the first context within **5s**
- **(d)** the SDK's post-join `MediaConnectionUpdate` (state=CONNECTED) reached
  MC's handler — `mc_participant_mh_status_total{state="connected"}` rises above
  a baseline captured immediately before the join
- **(e)** token-only join (task #58 c-iii): in each context's join window, no
  request carries the raw email/password **values**, no `/api/v1/auth/*`
  endpoint is hit (the removed forced-login band-aid, commit 7b69288, must never
  come back), and GC requests authenticate via `Authorization: Bearer` only

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

**Manual invocation only for now.** This suite is _not_ wired into
`scripts/layer-all.sh` Layer 7 — pipeline integration, sharding, and the
`@smoke` tag are story task #19.

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

- **AC registration limit: 5/hour.** The suite registers **2 throwaway users per
  run** — more than 2 consecutive runs within an hour will exhaust the limit and
  sign-up fails with 429; wait for the window to pass or reseed the cluster.
  This suite assumes a **single workstation against its own cluster**; it is not
  designed for concurrent runs sharing one cluster's rate-limit budget.
- **`retries: 0`** (ADR-0028): a failure is real. Fix it or delete the test —
  never mask with retries.
- **Timeouts**: 120s/test ceiling; assertion-meaningful waits are tighter (5s
  roster propagation; 60s Prometheus budget matching the Rust Scenario 7 helper,
  which includes the 15s scrape SLA).

## Artifacts & triage

Failures leave traces/screenshots in `test-results/` (gitignored):

```bash
pnpm exec playwright show-trace test-results/<test-dir>/trace.zip
```

**Artifact hygiene**: traces record full request/response traffic — including
the run's synthetic credentials and bearer tokens for the local dev cluster.
They are disposable per-run values, but do not promote artifacts to shared
storage or attach them to issues.
