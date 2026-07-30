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

Then open Chrome at `http://demo.localhost:5173` and drive: **Sign up** →
**Create meeting** (copy the code) → **Join** (paste the code). Open a second
tab/profile at the same URL and join the same code to see `ParticipantJoined` in
the first tab.

You still need the Kind cluster up and (for WSL2-side tooling) the `/etc/hosts`
entry — the script detects both and tells you, but can't do the
privileged/cluster parts for you:

```bash
./infra/kind/scripts/setup.sh                              # cluster: AC, GC, MC, MH
echo '127.0.0.1  demo.localhost' | sudo tee -a /etc/hosts  # see the runbook — this is the WSL2-side file
```

Use `./infra/kind/scripts/setup.sh`, **not** `infra/devloop/dev-cluster setup`.
The latter is the ADR-0030 in-container helper: it needs the devloop helper
socket (which only exists inside a running devloop container) and it allocates
dynamic ports bound to the podman host-gateway IP, so the fixed loopback ports
this demo assumes would not be there.

> **Full operational runbook: `docs/runbooks/client-dev-local.md`** — the
> two-machine Windows/WSL2 topology (§0), which of the two Kind clusters you're
> on (§1), step-by-step bring-up (§3), how to tell a real join from a false one
> (§4), and ten first-run failure modes (§5). It owns the prose and the
> diagnosis; this README is the quick start.

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

> **Unguarded coupling.** The two proxy-target ports above are `vite.config.ts`'s own hardcoded
> defaults (its SSoT), but they must equal the `hostPort` values in `infra/kind/kind-config.yaml`.
> The two constants are independent and **nothing enforces the match** — see `docs/TODO.md`
> §Port Constant Scattering. A mismatch surfaces as `scripts/dev-web.sh` reporting AC/GC
> unreachable, whose message says "is the Kind cluster up?" — the wrong diagnosis when the real
> cause is that a `hostPort` moved and these defaults no longer point at it.

## Things that go wrong (first-run)

**Owned by `docs/runbooks/client-dev-local.md` §5** — ten failure modes, each with
a runnable discriminator, because several share a symptom. Not summarised here:
a short list would have to pick which traps to omit, and the omitted ones are the
expensive ones.

The two highest-frequency starting points: run `scripts/dev-web.sh --check`
first, and if sign-up and create-meeting work while **only** join fails, you are
in runbook §5 F1/F7/F8 — the WebTransport path — not a proxy problem.

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
