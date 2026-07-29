# @darktower/web-app

Minimal browser demo for the Dark Tower join flow — a four-view SPA wired to
`@darktower/sdk-core` + `@darktower/sdk-svelte` so a developer can drive the full
happy path by hand: **sign-up → create-meeting → copy code → join** (open a
second browser context to see live `ParticipantJoined`/`ParticipantLeft`).

No video, layouts, settings, or controls (deferred). This is the dev-loop demo,
not a production app.

> **Non-goal banner.** Production deployment, CSP/COEP/COOP headers, CDN serving,
> and synthetic probes are **out of scope** here — tracked separately under
> ADR-0028 §10. This README covers the local-Kind dev loop only.

## The four views

| View | Uses | Notes |
|------|------|-------|
| Sign-up | `AuthApiClient.register` | email / password / displayName / orgSubdomain |
| Sign-in | `AuthApiClient.login` | email / password / orgSubdomain |
| Create-meeting | `MeetingApiClient.createMeeting` | title (+ optional scheduled start) → shows the meeting code |
| Meeting-join | `MeetingSession.join` | meeting code → live roster from the reactive store |

## Transport & scheme (important)

- **AC / GC are plain HTTP in dev.** Both services bind a plain TCP listener in
  dev-Kind (no TLS) — see `crates/env-tests/src/cluster.rs`. The Vite dev-server
  proxy forwards to them over `http://`. `https://` is reserved for the prod
  bundle only.
- **MC / MH are `https`/QUIC (WebTransport).** These are **direct** browser
  connections (not proxied) that pin the dev self-signed certs via
  `serverCertificateHashes`, loaded from `infra/docker/certs/fingerprints.json`.

## Run the demo locally (end-to-end)

**Fastest path — `scripts/dev-web.sh`.** It preflights the whole setup against
the repo's own pins (Node vs `.nvmrc`, pnpm vs `packageManager`, `nvm` present,
AC/GC reachable, cert fingerprints, MC/MH WebTransport listeners, and
`demo.localhost`), then runs `pnpm install`, generates the protobuf-es client
code, and launches the dev server — failing loudly with the exact fix command
for anything missing. The MC/MH check reads the advertise ports from the service
configmaps and verifies a listener on the right address family, so it flags the
WSL2 mirrored-mode IPv4/IPv6 loopback trap (browser dials IPv6 `::1`, podman
publishes IPv4 only) before you hit it at join time:

```bash
scripts/dev-web.sh            # preflight + install + codegen + launch
scripts/dev-web.sh --check    # preflight only (no install/launch)
```

Then open Chrome at `http://demo.localhost:5173`. You still need the cluster up
(step 1) and the `/etc/hosts` entry (step 4) — the script detects and tells you,
but can't do the privileged/cluster parts for you. The numbered steps below are
what the script automates, plus the manual fallback.

Prereqs: Node 22 (`.nvmrc`), pnpm (`corepack enable`), a running host-side Kind
cluster with AC + GC (+ MC + MH for the join step).

1. **Bring up the Kind cluster** (AC, GC, MC, MH) with the host-side helper
   (per ADR-0030):

   ```bash
   infra/devloop/dev-cluster setup
   ```

   The fuller operational runbook lands in this story as
   `docs/runbooks/client-dev-local.md` (R-49, task #20 — Pending; not yet on this
   branch). The demo assumes AC on host port **8443** and GC on host port
   **8444** (loopback `127.0.0.1`, per R-37).

2. **Seed a `demo` org** so sign-up has an org to register against (subdomain
   `demo`) — handled by the Kind setup (R-38).

3. **Generate dev certs + fingerprints** so the browser will accept the MC/MH
   WebTransport certs:

   ```bash
   scripts/generate-dev-certs.sh      # writes infra/docker/certs/fingerprints.json (R-36)
   ```

   The fingerprints file is read at **Vite config time** — after regenerating
   certs you must **restart `pnpm dev`** to pick up new hashes. Certs rotate on a
   ≤14-day window, so a stale dev server is a common trap (see "Things that go
   wrong").

4. **Add the `/etc/hosts` entry** so the subdomain-qualified AC origin resolves
   (AC extracts the org from the `Host` header):

   ```
   127.0.0.1  demo.localhost
   ```

5. **Install + run** (through Nx, so proto codegen runs first):

   ```bash
   pnpm install
   pnpm nx run web-app:dev     # runs proto-gen:codegen (dependsOn) then vite
   ```

   > The protobuf-es client (`packages/sdk-core/src/proto/**/*_pb.ts`) is
   > gitignored generated code produced by `proto-gen:codegen`, which the `dev`
   > target `dependsOn` — so the Nx target generates it before starting vite.
   > (`scripts/dev-web.sh` launches this way for you.)

6. **Open Chrome at `http://demo.localhost:5173`** (the `demo.` prefix is
   required so the AC auth proxy forwards the subdomain in the `Host` header).
   Then: **Sign up** → **Create meeting** (copy the code) → **Join** (paste the
   code). Open a second tab/profile at the same URL and join the same code to see
   `ParticipantJoined` in the first tab.

## Configuration

Defaults work with the topology above; override via env (Vite `VITE_*` for
runtime, plain env for the proxy upstreams):

| Var | Default | Purpose |
|-----|---------|---------|
| `VITE_AC_ORIGIN_TEMPLATE` | `http://{subdomain}.localhost:5173` | SDK AC origin (subdomain-qualified, same-origin via proxy) |
| `VITE_GC_BASE_URL` | `` (same-origin) | SDK GC base URL |
| `VITE_TELEMETRY_ENDPOINT` | _(unset)_ | When set, enables SDK telemetry to GC `/api/v1/telemetry`; otherwise telemetry is **OFF** |
| `VITE_AC_PROXY_TARGET` | `http://127.0.0.1:8443` | Dev-proxy upstream for `/api/v1/auth/*` (Host preserved) |
| `VITE_GC_PROXY_TARGET` | `http://127.0.0.1:8444` | Dev-proxy upstream for `/api/v1/meetings` + `/api/v1/telemetry` |

## Things that go wrong (first-run)

Deep diagnosis lives in `docs/runbooks/client-dev-local.md` (R-49, lands as task
#20). Quick pointers:

1. **`fingerprints.json` missing/stale → WebTransport to MC/MH refused.** The
   browser rejects the self-signed cert when the pin is absent or out of date.
   Run `scripts/generate-dev-certs.sh` and **restart `pnpm dev`** (fingerprints
   are read at Vite config time; certs rotate ≤14 days). HTTP views
   (sign-up/create) still work without it — only join's media step fails.
2. **`demo.localhost` doesn't resolve.** Add `127.0.0.1 demo.localhost` to
   `/etc/hosts` and open the app at `http://demo.localhost:5173`.
3. **Proxy `ECONNREFUSED` on AC/GC.** The Kind cluster / NodePorts (host
   8443/8444) aren't up — bring up the cluster first.

## E2E contract (for tasks #18/#19)

The demo exposes a stable surface the test-owned Playwright specs consume:

- **`data-testid`** hooks — the canonical set: `email`, `password`,
  `display-name`, `org-subdomain`, `meeting-title`, `scheduled-start`,
  `create-button`, `created-meeting-code`, `meeting-code`, `join-button`,
  `participant-list`, `participant-{id}`, `last-error`. Plus the auth-submit
  buttons the UI-driven signup → login flow clicks: `create-account-button`
  (sign-up), `signin-button` (sign-in); and `meeting-state` (join view's current
  `MeetingSessionState`, for DOM assertions where the event bus isn't used).
- **`window.__darktower_test__`** — a replay-buffered event bus, gated behind the
  `__E2E_HOOKS__` build define (absent from prod bundles). It carries only
  bounded, non-PII events (`stateChange`, `joined`, `participantJoined`,
  `participantLeft`, `mediaConnected`, `error`). The `joined` event is a
  whitelist projection — it **omits** `bindingToken` and `correlationId`; errors
  cross via `SdkError.toJSON()` (no tokens). Read it as:

  ```js
  window.__darktower_test__.events            // replay buffer of all events so far
  window.__darktower_test__.on('joined', cb)  // subscribe to future events
  ```

## Scripts

| Script | What |
|--------|------|
| `pnpm dev` | Vite dev server |
| `pnpm build` | Production SPA build |
| `pnpm test:component` | Vitest 4 Browser Mode (Chromium) component tests |
| `pnpm test:unit` | Node-tier prod bundle-content assertion (R-14) |
| `pnpm lint` | svelte-check + eslint + prettier |
