// File: packages/web-app/vite/devServer.ts
//
// NODE-ONLY (Vite config + Playwright config time, never shipped to the browser).
// The ONE home of the Vite dev server's listen address. Consumers:
//   - vite.config.ts `server.host` / `server.port` — every `pnpm dev`, interactive or not;
//   - e2e/env.ts — the default page-origin port and the loopback host every Node-side
//     probe uses (Playwright `webServer.url` readiness, global-setup, `toLoopbackUrl`),
//     from which playwright.config.ts also derives the `--host`/`--port` it starts Vite
//     with. So the address Vite binds and the address Playwright polls are one value.
//
// WHY A LITERAL, NOT `localhost`. Vite's default host is the NAME `localhost`, and Node
// binds only the FIRST address the system resolver returns for it. On a host whose
// resolver answers `::1` first (a stock GitHub ubuntu runner) Vite therefore listened on
// `[::1]:5173` only, while the Node-side probes dialled `127.0.0.1:5173` and the E2E
// suite timed out waiting for a server that was up. Any name-based bind reintroduces
// that resolver-order dependency; a literal address removes it.
//
// WHY 127.0.0.1 AND NOT ::1. The project is IPv6-first for DEPLOYED addressing (MC/MH
// per-instance endpoints); this is a dev-only, loopback-only listener, and the rest of
// the dev topology it talks to is IPv4 loopback (infra/kind/kind-config.yaml binds every
// host port on `listenAddress: "127.0.0.1"`; e2e/env.ts's AC/GC/Prometheus defaults are
// 127.0.0.1). `127.0.0.1` exists on every host this runs on, whereas `::1` is absent
// wherever IPv6 is disabled in the network namespace (a common container default), where
// a `::1` bind fails outright. The browser side is family-agnostic: Chromium resolves
// `*.localhost` to BOTH loopbacks itself and falls back across them, so the page origin
// `http://<org>.localhost:5173` reaches this listener whichever family it tries first.
// (TCP falls back across families; it is UDP/QUIC that does not — the MC/MH advertise-
// address case in docs/runbooks/client-dev-local.md F1, which chose the IPv4 literal too.)
// Not a wildcard (`::` / `0.0.0.0`) on purpose: the dev server must not be reachable
// from the network. Moving to `::1` is a one-line change here (e2e/env.ts brackets it).
export const DEV_SERVER_HOST = '127.0.0.1';

/** Vite's own default port, pinned so the page origin and the bind cannot drift. */
export const DEV_SERVER_PORT = 5173;
