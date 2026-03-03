# Debate: Should we accept ADR-0028 (Client Architecture) as proposed?

**Date**: 2026-02-28
**Status**: Complete — Consensus Reached (Round 3)
**Participants**: Client, Security, Test, Observability, Operations, Protocol, Infrastructure

## Question

Should we accept ADR-0028 (Client Architecture) as proposed?

## Context

Dark Tower's backend architecture is established with defined protocols (HTTP/3, WebTransport, protobuf signaling, 42-byte binary media frames, SFrame E2EE). No client code exists yet. ADR-0028 proposes the complete browser client architecture.

## Final Satisfaction Scores

| Specialist | Score | Would Accept |
|------------|-------|--------------|
| Protocol | 95 | Yes |
| Client | 95 | Yes |
| Security | 93 | Yes |
| Observability | 93 | Yes |
| Test | 92 | Yes |
| Operations | 92 | Yes |
| Infrastructure | 91 | Yes |

## Key Amendments Agreed During Debate

### Security (5 amendments)
1. AES-256-GCM as default cipher; AES-128-GCM only for SFrame interoperability (deviation from ADR-0027 justified)
2. MLS implementation committed to WASM path (Rust crate compiled to WASM, not "maybe")
3. Explicit token cleanup on disconnect/logout (clear all references, overwrite buffers)
4. Non-extractable CryptoKey objects via WebCrypto where possible (XSS protection)
5. npm supply chain protections: `--provenance` flag, SRI hashes for CDN, `pnpm audit` in CI, 2FA for publish

### Test (7 amendments)
1. E2E time ceiling: 12 min per shard maximum, escalation policy is "more shards not longer timeouts"
2. Flaky test policy: Playwright retries=0 by default, quarantine within 24h, fix/delete within one sprint, tracked metric with alert
3. Crypto test quarantine exception: no quarantine for crypto tests — P1 bug, fix immediately or revert
4. Component test "every save" scope: single-file watch mode only, full suite CI-only
5. Chaos/network degradation tests: informational only, not release-blocking, create issue on failure
6. Test data strategy: `packages/test-utils` with TestTokenBuilder, TestTokenSigner (ephemeral keys), deterministic UUIDs, MockWebTransport, Y4M/WAV fixtures
7. MockOTLPExporter: in-memory span/metric collector for asserting observability instrumentation

### Observability (8 amendments)
1. Client metrics catalog deliverable at `docs/observability/metrics/client.md` with `dt_client_` prefix
2. Trace propagation subsection: OTel JS SDK, `traceparent` on HTTP, `trace_parent`/`trace_state` proto fields (20, 21) on ClientMessage/ServerMessage
3. Telemetry routed through GC `/api/v1/telemetry` endpoint (authenticated `fetch` + `keepalive`, not `sendBeacon`)
4. ADR-0011 compliant alert structure in `client-alerts.yaml` with runbook references
5. Three dashboard deliverables: `client-overview.json`, `client-slo.json`, `client-synthetic.json`
6. Metric naming convention: `dt_client_` for app metrics, `dt_synthetic_` for probe metrics
7. MetricsSink testability pattern (InMemoryMetricsSink for test assertions)
8. SDK canary via npm dist-tags (canary → 24h soak → promote to latest)

### Operations (7 amendments)
1. Serving architecture section: static SPA on CDN with environment-specific tiers
2. Local development section: `pnpm dev` + kind, cert fingerprint flow, Skaffold profiles
3. Separate CI workflow: `.github/workflows/ci-client.yml` with path-based triggers
4. Shared Docker services for E2E with TLS cert flow documented
5. Client canary mechanism: CDN-based with `client_version` telemetry label
6. Operational deliverables in Implementation Status: alerts, runbooks, CronJob spec
7. Devloop image update as documented consequence (add pnpm, Playwright deps)

### Protocol (3 ADR text amendments + 5 implementation deliverables)
ADR text:
1. Big-endian (network) byte ordering explicit for all multi-byte frame header fields
2. BigInt/u64 requirement: TypeScript codec must use `BigInt` + `DataView.getBigUint64()`/`setBigUint64()` for User ID, Timestamp, Sequence Number
3. SFrame layering paragraph: SFrame header is part of encrypted payload, not part of 42-byte frame header; frame header sequence number is separate from SFrame counter

Implementation deliverables:
4. Length-prefixed signaling framing reference (4-byte big-endian prefix)
5. MLS handshake message delivery via opaque proto pattern
6. Trace context fields (20, 21) on ClientMessage/ServerMessage
7. Cross-language test vector specification at `proto/test-vectors/`
8. Telemetry beacon documented as fifth transport channel

### Infrastructure (6 amendments)
1. Deployment architecture: static SPA via CDN + nginx, with COOP/COEP/CSP/Permissions-Policy headers
2. Local dev integration: Vite proxy config, cert fingerprint flow, Skaffold profiles
3. Build pipeline: Nx task graph (proto codegen → WASM → sdk-core → sdk-svelte → web-app → Docker)
4. Separate CI workflow with path-based triggers
5. Synthetic probe resource sizing: requests {cpu: 500m, memory: 512Mi}, limits {cpu: 2000m, memory: 1Gi}, concurrencyPolicy: Forbid
6. NetworkPolicy: UDP ingress rules for QUIC on GC/MC/MH

## Cross-Cutting Discoveries

The debate surfaced several issues that no single specialist would have caught:

1. **BigInt/u64 precision** (Protocol + Test): JavaScript `Number` silently loses precision on u64 values > 2^53. Random User IDs exceed this in 99.9% of cases. Would cause frame misrouting in production.
2. **GC telemetry proxy** (Observability + Security + Infrastructure + Operations): Routing client telemetry through GC solves authentication, PII filtering, CORS, and rate limiting simultaneously.
3. **MLS signaling gap** (Protocol + Security): MLS handshake messages (Welcome, Commit, KeyPackage) need a delivery mechanism in signaling.proto — no existing variants cover this.
4. **Trace context in protobuf** (Observability + Protocol): `traceparent` can't be sent as HTTP headers over WebTransport signaling — needs proto envelope fields.
5. **SDK canary mechanism** (Observability + Operations): npm/CDN-distributed SDK can't use Argo Rollouts — needs dist-tag-based canary with synthetic probe soak.
6. **Crypto test quarantine prohibition** (Security + Test): Crypto test failures must never be quarantined — always P1, fix or revert immediately.

## Decision

ADR-0028 accepted with all amendments listed above. The ADR will be revised to incorporate these changes.
