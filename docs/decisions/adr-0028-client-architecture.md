# ADR-0028: Client Architecture

**Status**: Accepted

**Date**: 2026-02-28

**Deciders**: Client, Protocol, Security, Test, Observability, Operations, Infrastructure

**Debate**: `docs/debates/2026-02-28-client-architecture/debate.md`

---

## Context

Dark Tower needs a browser-based client for video conferencing. The backend architecture is established (ADR-0003, ADR-0010, ADR-0020, ADR-0023) with defined protocols: HTTP/3 REST to GC, WebTransport signaling to MC, WebTransport media to MH, SFrame E2EE, and a 42-byte binary media frame format. No client code exists yet.

Requirements:
- Modern UI with real-time audio/video rendering and user-controlled layouts
- SDK model: core functionality available to external developers, our app built on top
- Leverages WebTransport, WebCodecs, and WebCrypto browser APIs
- Chrome-only MVP (Firefox/Safari deferred)
- Highly testable across all tiers (unit, component, integration, E2E)
- End-to-end encrypted media via SFrame + MLS
- Communicates with AC/GC via HTTP and MC/MH via WebTransport

## Decision

### 1. Technology Stack

| Choice | Technology | Rationale |
|--------|-----------|-----------|
| Language | TypeScript | SDK consumer DX, direct browser API access, npm distribution |
| UI Framework | Svelte 5 (runes) | Fine-grained reactivity for video grids, ~3x faster than React for continuous updates, 1.6KB vs 42KB bundle |
| Build | Vite | Industry standard, native ESM, strong WASM plugin support |
| Monorepo | Nx | Polyglot support (Rust WASM crates + TypeScript packages as first-class targets) |
| Package Manager | pnpm | Workspace support, strict dependency resolution |
| Protobuf | @bufbuild/protobuf-es | Conformance-tested, tree-shakeable, ESM-native. Wire format compatible with Rust prost. |

**Supply chain protections**: All npm packages published with `--provenance` flag. CDN assets include SRI hashes. `pnpm audit` runs in CI. 2FA required for npm publish.

### 2. Package Architecture

```
packages/
├── sdk-core/          # @darktower/sdk-core — pure TypeScript, no framework
│   ├── src/
│   │   ├── transport/    # WebTransport abstraction + IWebTransport interface
│   │   ├── signaling/    # Protobuf signaling client (MC communication)
│   │   ├── media/        # WebCodecs encode/decode pipeline
│   │   ├── crypto/       # SFrame E2EE, WebCrypto AES-GCM, MLS key management
│   │   ├── framing/      # 42-byte binary media frame codec
│   │   ├── room/         # Room, Participant, Track state machines
│   │   ├── api/          # HTTP client for GC/AC REST endpoints
│   │   ├── telemetry/    # OTel JS SDK instrumentation + MetricsSink
│   │   └── proto/        # Generated protobuf-es types (from proto/*.proto)
│   └── package.json
├── sdk-svelte/        # @darktower/sdk-svelte — Svelte 5 stores + components
│   ├── src/
│   │   ├── stores/       # Svelte rune stores wrapping SDK events
│   │   ├── components/   # VideoTile, AudioRenderer, VideoGrid, Controls
│   │   └── layouts/      # Grid layout engine (auto/manual, drag-and-drop)
│   └── package.json
├── test-utils/        # @darktower/test-utils — shared test infrastructure
│   ├── src/
│   │   ├── TestTokenBuilder.ts    # Ephemeral token generation
│   │   ├── TestTokenSigner.ts     # Ephemeral key signing
│   │   ├── MockWebTransport.ts    # Transport test double
│   │   ├── MockOTLPExporter.ts    # In-memory span/metric collector
│   │   ├── InMemoryMetricsSink.ts # MetricsSink for test assertions
│   │   ├── deterministic-ids.ts   # Deterministic UUID generation
│   │   └── fixtures/              # Y4M/WAV test media files
│   └── package.json
└── web-app/           # @darktower/web-app — our video conferencing app
    ├── src/
    │   ├── routes/       # SvelteKit or Svelte SPA routes
    │   ├── features/     # Login, Meeting, WaitingRoom, Settings
    │   └── lib/          # App-specific utilities
    └── package.json
```

The SDK follows the pattern established by LiveKit, Daily, and Vonage: a framework-agnostic core with optional framework adapters. External developers use `@darktower/sdk-core` directly or with an adapter.

**Code location convention**: Rust code lives in `crates/`, TypeScript code lives in `packages/`. WASM crates (e.g., `crates/browser-wasm/`) are Rust source that compiles to WASM and is consumed by TypeScript packages.

**Build pipeline** (Nx task graph): proto codegen → WASM → sdk-core → sdk-svelte → web-app → Docker

### 3. Transport Strategy

| Connection | Protocol | Purpose |
|-----------|----------|---------|
| Client → AC | HTTPS (fetch) | User token acquisition |
| Client → GC | HTTPS (fetch) | Meeting CRUD, meeting token exchange |
| Client → MC | WebTransport (bidirectional streams) | Signaling: join, subscribe, mute, layout |
| Client → MH | WebTransport (bidirectional datagrams) | Media: send and receive encrypted audio/video frames |
| Client → GC `/api/v1/telemetry` | HTTPS (fetch + keepalive) | Client-side telemetry (OTel traces/metrics) |

The client uses standard `fetch()` for AC and GC — Chrome automatically negotiates the best available HTTP version (1.1, 2, or 3) based on server capabilities. The client code is protocol-agnostic; no special handling is needed regardless of which HTTP version the server supports. WebTransport connections use `serverCertificateHashes` for development (self-signed certs) and standard TLS in production.

**Telemetry routing**: Client telemetry is sent via authenticated `fetch` with `keepalive` flag to GC's `/api/v1/telemetry` endpoint — not `navigator.sendBeacon` to a public OTLP endpoint. GC proxies to the collector after validating the auth token, filtering PII, and applying rate limits. This solves authentication, PII filtering, CORS, and rate limiting simultaneously. A dedicated telemetry ingestion service could replace this if telemetry volume warrants it; the SDK's `MetricsSink` abstraction makes the endpoint swappable without client changes.

**Signaling framing**: Protobuf messages on WebTransport bidirectional streams use 4-byte big-endian length-prefix framing.

**No fallback transport in MVP.** Chrome supports all required APIs. WebSocket/WebRTC fallback deferred to a future ADR when Firefox/Safari support is added.

### 4. Media Pipeline

```
Capture → Encode → Encrypt → Frame → Transport → Deframe → Decrypt → Decode → Render

getUserMedia()         SFrame E2EE        42-byte header      WebTransport
     │                 (WebCrypto          + payload           datagrams
     ▼                  AES-GCM)              │                    │
 VideoFrame ──► EncodedVideoChunk ──► encrypted bytes ──► QUIC datagram
                  (WebCodecs)                                      │
                                                                   ▼
                                          QUIC datagram ──► 42-byte header
                                               │              + payload
                                               ▼                   │
                                          VideoFrame ◄── EncodedVideoChunk ◄── decrypted bytes
                                           (render)      (WebCodecs)        (SFrame + WebCrypto)
```

Key design decisions:
- WebCodecs for encode/decode (not MediaRecorder) — gives frame-level control
- SFrame encryption happens after encoding, before framing — MH never sees plaintext
- SFrame header is part of the encrypted payload, not part of the 42-byte frame header; the frame header sequence number is separate from the SFrame counter
- Layout subscription model: client requests a grid (e.g., 3x3), MC returns `StreamAssignments` mapping grid slots to participants + MH URLs
- Client may connect to multiple MH instances simultaneously, sending its own media to and receiving other participants' media from different MH instances as directed by MC's `StreamAssignments`

**Byte ordering**: All multi-byte fields in the 42-byte frame header use big-endian (network) byte ordering.

**BigInt requirement**: The TypeScript frame codec must use `BigInt` + `DataView.getBigUint64()`/`setBigUint64()` for u64 fields: User ID, Timestamp, Sequence Number. JavaScript `Number` silently loses precision on u64 values > 2^53 — random User IDs exceed this in 99.9% of cases.

### 5. E2EE Architecture

| Component | Technology | Notes |
|-----------|-----------|-------|
| Frame encryption | SFrame (AES-256-GCM) | Per-frame encryption via WebCrypto; AES-128-GCM only for SFrame interop |
| Key agreement | MLS (RFC 9420) via WASM | Rust MLS crate compiled to WASM, shares implementation with backend |
| Key derivation | HKDF-SHA256 | Per-sender key derivation |
| Replay protection | Counter-based | Per-sender monotonic counter in SFrame header |

**Default cipher**: AES-256-GCM, aligning with ADR-0027. AES-128-GCM is permitted only when required for SFrame interoperability with external clients.

**MLS implementation**: Rust MLS crate compiled to WASM via `wasm-bindgen`. This shares the implementation with the backend and avoids maintaining a separate TypeScript MLS library. The WASM module lives in the Cargo workspace at `crates/browser-wasm/`.

**MLS signaling**: MLS handshake messages (Welcome, Commit, KeyPackage) are delivered via an opaque bytes pattern in `signaling.proto`. Existing message variants do not cover MLS handshake delivery.

**Key management**:
- Key rotation triggers: participant join/leave, manual request, hourly time-based
- Keys stored in memory only — never persisted to localStorage or sent to server
- All `CryptoKey` objects created as non-extractable via WebCrypto where possible (XSS mitigation)
- Explicit token cleanup on disconnect/logout: clear all references and overwrite key buffers

### 6. SDK Design Patterns

**Event-driven core**: The SDK emits typed events (`RoomEvent`, `ParticipantEvent`, `TrackEvent`). Framework adapters convert these to native reactivity (Svelte stores, React hooks).

**Transport abstraction**: All WebTransport usage goes through an `IWebTransport` interface. Production code uses the real `WebTransport` API. Tests inject `MockWebTransport` with control points (`simulateReady()`, `simulateClose()`, `simulateIncomingDatagram()`).

**Token-source pattern**: The SDK receives a token or a token-fetching function, decoupling credential management from connection logic.

**Error hierarchy**: Typed SDK errors — `AuthError`, `NetworkError`, `MediaError`, `CryptoError` — with actionable error codes for SDK consumers.

**Distribution**: ESM primary + CJS fallback, published to npm under `@darktower/` scope. CDN distribution via jsDelivr (auto-mirrors npm). SDK canary releases use npm dist-tags: publish to `canary` tag → 24h synthetic probe soak → promote to `latest` tag.

### 7. Testing Strategy

#### Tier Model

| Tier | Environment | Scope | Speed | When |
|------|------------|-------|-------|------|
| Unit | Vitest (Node) | SDK logic, state machines, protobuf, frame codec | ~10-30s | Every save |
| Component | Vitest Browser Mode (Chromium) | Svelte components, WebCrypto, WebCodecs | ~60-120s | Single-file watch mode; full suite CI-only |
| Integration | Vitest Browser Mode + Docker services | Real WebTransport to MC/MH, real E2EE | ~2-5min | CI |
| E2E | Playwright (multi-context) | Full user flows, multi-participant meetings | Max 12min/shard | CI (4 shards) |

**E2E time ceiling**: 12 minutes per shard maximum. If tests exceed this, the escalation policy is "more shards, not longer timeouts."

#### Unit Tests (Node)

Pure logic with mock transport. Tests SDK state machines, reconnection backoff, protobuf round-trips, 42-byte frame codec, event emission.

Property-based testing (`fast-check`) for the frame codec — generate random payloads, verify serialize/deserialize round-trips.

#### Component Tests (Browser)

Vitest 4.0 Browser Mode with Playwright provider. Tests run in real Chromium — all browser APIs available (WebTransport, WebCodecs, WebCrypto, OffscreenCanvas).

- Chromium launch flags: `--use-fake-ui-for-media-stream`, `--use-fake-device-for-media-stream`
- MSW (Mock Service Worker) for HTTP API mocking (GC REST endpoints)
- MSW cannot intercept WebTransport — mock at the `IWebTransport` interface layer
- Synthetic `VideoFrame` from `OffscreenCanvas` for codec pipeline tests

#### Integration Tests (Browser + Docker)

Real WebTransport connections to Dockerized `mc-service`/`mh-service`. Browser trusts test cert via `serverCertificateHashes` (ephemeral cert generated at CI start, fingerprint passed as env var).

Full pipeline tests: encode → encrypt → serialize → deserialize → decrypt → decode.

#### E2E Tests (Playwright)

Multi-participant scenarios via separate `BrowserContext` instances. Fake media via Y4M/WAV fixtures and Chromium flags.

Video validation: verify `currentTime` advances (proves playback, not just DOM presence). Canvas pixel sampling for stronger assertions on test patterns.

**Network degradation**: Docker `tc-netem` for QUIC-level chaos (CDP network throttling doesn't apply to WebTransport). Chaos/network degradation tests are **informational only, not release-blocking** — create an issue on failure. Run weekly, not per-commit.

**Flaky test policy**:
- Playwright `retries=0` by default (no masking failures with retries)
- Quarantine flaky tests within 24 hours of identification
- Fix or delete quarantined tests within one sprint
- Track flaky test count as a metric with alerting
- **Crypto test exception**: crypto test failures are never quarantined — always P1, fix immediately or revert
- Weekly `--repeat-each=20` runs to proactively identify flaky tests

#### Test Data Strategy

Shared test utilities in `packages/test-utils/`:
- `TestTokenBuilder` / `TestTokenSigner` — ephemeral key generation for test tokens
- Deterministic UUIDs for reproducible test scenarios
- `MockWebTransport` — transport test double with simulation control points
- `MockOTLPExporter` — in-memory span/metric collector for asserting observability instrumentation
- `InMemoryMetricsSink` — MetricsSink implementation for metric assertion in tests
- Y4M/WAV fixtures for fake media injection

#### Protocol Compatibility

`buf breaking` in CI against `main` branch — catches wire-format breaking changes in `.proto` files before merge. Shared JSON test vectors for the 42-byte media frame header (both TypeScript and Rust tests consume the same fixture file at `proto/test-vectors/`).

#### Coverage

`@vitest/coverage-v8` with thresholds: 90% overall, 100% crypto modules. Mirrors backend coverage policy (ADR-0005).

#### Client Guards

Adapt existing guard patterns (ADR-0015) to TypeScript:
- `no-test-removal` — warn on removed `.test.ts` files
- `test-registration` — verify all test files are discoverable by Vitest
- `no-secrets-in-code` — scan for hardcoded keys/tokens

### 8. CI Pipeline

Separate workflow at `.github/workflows/ci-client.yml` with path-based triggers (`packages/**`, `proto/**`):

```
lint (30s)              unit+component (2-3min)         e2e (4 shards, max 12min each)
├── pnpm lint           ├── vitest --run (Node)         ├── Docker services (MC, MH, GC, AC)
├── prettier            ├── vitest --run (Browser)      ├── Generate TLS cert
├── svelte-check        ├── MSW for HTTP mocking        ├── playwright test --shard=N/4
├── buf lint            └── coverage thresholds         └── Upload traces on failure
├── buf breaking
└── client guards
```

**Shared Docker services** for E2E: TLS cert generation and fingerprint flow documented for consistent setup across local dev and CI.

#### Nightly/Weekly Jobs

- **Weekly flaky detection**: Playwright `--repeat-each=20` on full E2E suite
- **Weekly cross-version**: SDK version N against `mc-service` version N±1
- **Nightly synthetic probe**: Headless Chrome joins canary meeting, collects quality metrics

### 9. Observability

#### Metrics

**Metric naming convention**:
- `dt_client_` prefix for application metrics (e.g., `dt_client_frames_decoded`, `dt_client_ttfv_ms`)
- `dt_synthetic_` prefix for synthetic probe metrics (e.g., `dt_synthetic_fps`, `dt_synthetic_ttfv_ms`)

Client metrics catalog deliverable at `docs/observability/metrics/client.md`.

#### Trace Propagation

OTel JS SDK for client-side instrumentation. Trace context propagation:
- HTTP requests: standard `traceparent` header
- WebTransport signaling: `trace_parent` (field 20) and `trace_state` (field 21) proto fields on `ClientMessage`/`ServerMessage` envelope

#### Telemetry Collection

`TransportMetricsCollector` in SDK reports via authenticated `fetch` with `keepalive` to GC `/api/v1/telemetry` endpoint. WebTransport has no `getStats()` equivalent — all metrics are application-layer instrumented.

**MetricsSink pattern**: Abstraction layer for metric emission. Production uses OTel exporter; tests use `InMemoryMetricsSink` for assertions.

#### Synthetic Probe

Kubernetes CronJob, every 5 minutes:
- Headless Chrome creates meeting, joins as 2 synthetic participants with fake media
- Collects SDK metrics: `framesDecoded`, `timeToFirstVideoMs`, FPS, bytes received
- Exports to Prometheus
- Resource sizing: requests `{cpu: 500m, memory: 512Mi}`, limits `{cpu: 2000m, memory: 1Gi}`, `concurrencyPolicy: Forbid`

#### Alert Rules

ADR-0011 compliant alert structure in `client-alerts.yaml` with runbook references:
- `dt_synthetic_fps < 25` for 3 probes → page on-call
- `dt_synthetic_ttfv_ms > 3000` → warning
- `dt_synthetic_probe_failure` → critical

#### Dashboards

Three dashboard deliverables:
- `client-overview.json` — SDK connection health, error rates, participant counts
- `client-slo.json` — SLO burn rates for client-facing reliability targets
- `client-synthetic.json` — Synthetic probe results, quality trends

#### End-to-End Journey Monitoring

Design for (post-MVP) full-flow observability that correlates client and server telemetry to answer operational questions like:
- "Was a user able to join a meeting and receive media?" — trace spans from client join request → GC meeting token → MC signaling → MH media flow → client first frame rendered
- "Was a user dropped from a meeting unexpectedly?" — correlate client disconnect events with MC session state, MH datagram loss, and network-layer errors

This requires:
- **Shared trace context**: `traceparent` propagated from client through GC, MC, and MH (via HTTP headers and proto fields 20/21) so a single trace ID follows the entire user journey
- **Client lifecycle events**: SDK emits structured events for join, first-media-received, quality-degraded, reconnect-attempt, disconnect (with reason codes) — all tagged with the trace context
- **Server-side correlation**: MC and MH annotate their spans with the client's trace ID, enabling cross-service trace assembly
- **Journey dashboards**: Aggregate traces into journey success/failure rates (e.g., "95% of join attempts result in media received within 5s")

The trace propagation infrastructure (Section 9.2) and MetricsSink abstraction are designed to support this. Implementation is post-MVP but the instrumentation hooks must be present from day one.

#### SDK Canary

SDK canary releases via npm dist-tags (Argo Rollouts does not apply to npm/CDN-distributed packages):
1. Publish to `canary` dist-tag
2. Synthetic probe soak for 24 hours against canary version
3. Promote to `latest` dist-tag if quality gates pass
4. `client_version` telemetry label enables filtering metrics by SDK version

### 10. Deployment Architecture

**Serving**: Static SPA served via CDN with nginx origin. Environment-specific tiers (dev, staging, production).

**Security headers**: COOP, COEP, CSP, and Permissions-Policy headers configured at the CDN/nginx layer.

**NetworkPolicy**: UDP ingress rules for QUIC on GC, MC, and MH services.

**Local development**: `pnpm dev` + kind cluster. Vite proxy config for backend services. Cert fingerprint flow for WebTransport `serverCertificateHashes`. Skaffold profiles for client dev workflow.

### 11. WASM Strategy

Add targeted WASM modules only when justified:

| Candidate | Decision | Rationale |
|-----------|----------|-----------|
| MLS key agreement | Yes (WASM) | Shares Rust implementation with backend; committed path |
| Noise suppression | Likely WASM | CPU-bound, SIMD-amenable, proven pattern (Google Meet) |
| Background segmentation | Maybe WASM | CPU-bound, could also use WebGPU |
| Protobuf encoding | No | protobuf-es is conformance-tested, wire compatible |
| AES-GCM encryption | No | WebCrypto is hardware-accelerated |
| Connection management | No | I/O-bound, browser API is sufficient |

WASM crates live in the Cargo workspace (e.g., `crates/browser-wasm/`). Build via `cargo build --target wasm32-unknown-unknown` + `wasm-bindgen` + `wasm-opt` (wasm-pack is archived as of Sept 2025). Integrated into Nx task graph.

## Consequences

### Positive
- SDK-first architecture enables external developer adoption and keeps our own app disciplined
- Svelte 5 provides best-in-class rendering performance for high-participant-count grids
- Browser Mode testing gives access to real WebTransport/WebCodecs/WebCrypto in unit tests
- Chrome-only MVP eliminates fallback transport complexity from initial scope
- Monorepo with shared proto files ensures client-server protocol consistency
- Testing strategy mirrors established backend patterns (four tiers, guards, coverage gates)
- GC telemetry proxy solves auth, PII, CORS, and rate limiting for client telemetry
- Non-extractable CryptoKey objects reduce XSS key exfiltration risk

### Negative
- Svelte has smaller ecosystem than React — fewer off-the-shelf component libraries
- Chrome-only MVP limits initial market reach
- No WebSocket fallback means corporate networks blocking QUIC will be unsupported initially
- Vitest Browser Mode adds ~2-4s cold start per worker vs Node-only tests
- Client-side observability requires manual instrumentation (no WebTransport `getStats()`)
- Devloop image needs update to include pnpm and Playwright dependencies

### Neutral
- TypeScript protobuf-es vs Rust prost — different codegen, same wire format
- Nx adds monorepo complexity but is necessary for polyglot workspace management

## Implementation Status

| # | Component | Status | Notes |
|---|-----------|--------|-------|
| 1 | Nx workspace + Vite setup | Pending | |
| 2 | sdk-core package scaffold | Pending | |
| 3 | sdk-svelte package scaffold | Pending | |
| 4 | web-app package scaffold | Pending | |
| 5 | test-utils package scaffold | Pending | TestTokenBuilder, MockWebTransport, MockOTLPExporter |
| 6 | Protobuf-es codegen pipeline | Pending | Including trace context fields (20, 21) |
| 7 | MLS opaque proto pattern | Pending | signaling.proto addition for MLS handshake messages |
| 8 | WebTransport abstraction + mock | Pending | |
| 9 | 42-byte frame codec (TypeScript) | Pending | BigInt for u64 fields, big-endian byte ordering |
| 10 | Cross-language test vectors | Pending | `proto/test-vectors/` shared with Rust |
| 11 | Vitest + Playwright test setup | Pending | |
| 12 | CI workflow (`ci-client.yml`) | Pending | Path-based triggers, separate from backend CI |
| 13 | buf breaking CI check | Pending | |
| 14 | Client metrics catalog | Pending | `docs/observability/metrics/client.md` |
| 15 | Alert rules (`client-alerts.yaml`) | Pending | ADR-0011 compliant with runbook references |
| 16 | Dashboards (3 JSON files) | Pending | client-overview, client-slo, client-synthetic |
| 17 | Synthetic probe CronJob spec | Pending | Resource sizing defined |
| 18 | Runbooks | Pending | Client-specific operational runbooks |
| 19 | Devloop image update | Pending | Add pnpm, Playwright dependencies |

## Alternatives Considered

- **React 19**: Largest ecosystem, most prior art for video conferencing SDKs. Not chosen because we're building our own SDK (ecosystem advantage matters less) and Svelte 5's performance advantage is meaningful for large video grids.
- **SolidJS**: Best raw performance via fine-grained signals. Not chosen due to smaller ecosystem than even Svelte, and Svelte 5 runes close the reactivity gap.
- **Rust-to-WASM for entire SDK**: Guarantees code sharing with backend. Not chosen because it creates poor DX for external SDK consumers (opaque binary, no debuggability, complex initialization).
- **Turborepo**: Simpler monorepo tool. Not chosen because it lacks first-class polyglot support for Rust crate builds.
- **WebSocket fallback from day one**: Broadens browser support immediately. Not chosen to reduce scope — Chrome-only MVP with all required APIs is sufficient for initial validation.
- **Pure TypeScript MLS**: Avoids WASM complexity. Not chosen because sharing the Rust MLS implementation with the backend ensures consistency and reduces maintenance burden.
- **navigator.sendBeacon for telemetry**: Simpler client-side implementation. Not chosen because it bypasses authentication, can't filter PII server-side, and creates CORS challenges.

## References

- ADR-0003: Authentication Architecture
- ADR-0005: Test Strategy
- ADR-0010: Global Controller Architecture
- ADR-0011: Observability Standards
- ADR-0015: Guards Methodology
- ADR-0020: User Auth and Meeting Access Flows
- ADR-0023: Meeting Controller Architecture
- ADR-0027: Approved Cryptographic Algorithms
- [Vitest 4.0 Browser Mode](https://vitest.dev/guide/browser/)
- [Svelte 5 Runes](https://svelte.dev/docs/svelte/$state)
- [protobuf-es](https://buf.build/blog/protobuf-es-the-protocol-buffers-typescript-javascript-runtime-we-all-deserve)
- [SFrame Draft](https://datatracker.ietf.org/doc/draft-ietf-sframe-enc/)
- [MLS RFC 9420](https://datatracker.ietf.org/doc/rfc9420/)
- [WebTransport API](https://developer.mozilla.org/en-US/docs/Web/API/WebTransport)
- [WebCodecs API](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API)
