# Devloop Output: `packages/test-utils/` Package (R-39)

**Date**: 2026-05-06
**Task**: Ship `packages/test-utils/` with MockWebTransport, TestTokenBuilder/Signer (ephemeral Ed25519), deterministic-ids, InMemoryMetricsSink, MockOTLPExporter (~10-12 self-tests). No proto-fixtures loader.
**Specialist**: test
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task8`
**User Story**: `docs/user-stories/2026-05-02-browser-client-join.md` (task #8, R-39)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `223aa6dc7a93bd23a7b385cb5ff1a33314b5a02c` |
| Branch | `feature/browser-client-join-task8` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-06-test-utils-package` |
| Implementing Specialist | `test` |
| Iteration | `1` |
| Security | `security@devloop-2026-05-06-test-utils-package` |
| Test | `n/a (test is implementer)` |
| Observability | `observability@devloop-2026-05-06-test-utils-package` |
| Code Quality | `code-reviewer@devloop-2026-05-06-test-utils-package` |
| DRY | `dry-reviewer@devloop-2026-05-06-test-utils-package` |
| Operations | `operations@devloop-2026-05-06-test-utils-package` |

---

## Task Overview

### Objective
Ship the `packages/test-utils/` (`@darktower/test-utils`) package — owned by the test specialist per user story §Design — providing the test-double surface that downstream tasks (especially #9 sdk-core scaffold and #18 Playwright E2E harness) depend on for unit testing.

Deliverables:
- `MockWebTransport.ts` — implements `IWebTransport` with control points: `simulateReady()`, `simulateClose(code, reason)`, `simulateError(err)`, `simulateIncomingDatagram(bytes)`, `simulateBidiStream(send, recv)`, `simulateServerMessage(msg)` plus inspector helpers (R-15).
- `TestTokenBuilder.ts` + `TestTokenSigner.ts` — ephemeral Ed25519 (Web Crypto / `@noble/ed25519`), mints user/meeting JWTs whose claim shape matches AC/GC issuance (unit-test only; E2E uses real tokens).
- `deterministic-ids.ts` — seeded UUIDv4.
- `InMemoryMetricsSink.ts` — assertion stub for the `MetricsSink` contract.
- `MockOTLPExporter.ts` — assertion stub for OTLP exporter calls.
- ~10-12 self-tests (Vitest).

Out of scope (per user direction in the user story):
- Cross-language proto-fixture loader.
- Rust regen tool (`crates/env-tests/src/bin/generate-proto-fixtures.rs`).

### Scope
- **Service(s)**: New `packages/test-utils/` (TypeScript-only). No Rust crates, no protos, no schema.
- **Schema**: No.
- **Cross-cutting**: Indirectly — downstream tasks consume this package; the `MetricsSink` interface contract is co-owned with observability per user story §Design.

### Debate Decision
NOT NEEDED — task scope is fully bounded by R-39 + the user story §Design notes (line 256, 325, 469). User direction already locked: no proto fixtures, no Rust regen tool.

---

## Cross-Boundary Classification

All paths under `packages/test-utils/**` are inside the test specialist's domain
(test owns this package per user story §Design line 256). Two cross-boundary
considerations are pre-flagged inline below.

| Path | Classification | Owner (if not mine) | Notes |
|------|----------------|---------------------|-------|
| `packages/test-utils/package.json` | Mine | — | New package manifest |
| `packages/test-utils/tsconfig.json` | Mine | — | Extends repo `tsconfig.base.json` |
| `packages/test-utils/tsconfig.build.json` | Mine | — | Type-emit config (Vite library mode) |
| `packages/test-utils/vite.config.ts` | Mine | — | Vite library-mode build config |
| `packages/test-utils/vitest.config.ts` | Mine | — | Vitest config (Node env) |
| `packages/test-utils/project.json` | Mine | — | Nx project descriptor |
| `packages/test-utils/README.md` | Mine | — | Package readme |
| `packages/test-utils/src/index.ts` | Mine | — | Barrel exports |
| `packages/test-utils/src/contracts/IWebTransport.ts` | Mine, **Minor-judgment cross-boundary** | client (sdk-core, future task #9) | Declares the shape sdk-core (task #9) will conform to. See "Interface declaration strategy". |
| `packages/test-utils/src/contracts/MetricsSink.ts` | Not mine, **Minor-judgment** | observability (named convention author per ADR-0024 §6.5 Pattern B) | Declares the contract shape. Owner upgraded per @observability Gate 1 input — the interface signature is observability-authored; I implement the test-side conformance. |
| `packages/test-utils/src/MockWebTransport.ts` | Mine | — | R-15 control points + inspectors |
| `packages/test-utils/src/TestTokenBuilder.ts` | Mine | — | Claim-shape builder (no crypto) |
| `packages/test-utils/src/token-claims.ts` | Mine | — | Claim TS types matching `UserClaims`/`MeetingTokenClaims` wire shape |
| `packages/test-utils/src/test-only/signer.ts` | Mine, **GSA-adjacent (ADR-0027 Ed25519)** | security | See "Ed25519 GSA classification" below |
| `packages/test-utils/src/deterministic-ids.ts` | Mine | — | Seeded `mulberry32` PRNG + UUIDv4 |
| `packages/test-utils/src/InMemoryMetricsSink.ts` | Not mine, **Minor-judgment** | observability (named convention author per ADR-0024 §6.5 Pattern B) | Implements the `MetricsSink` contract (passive recorder). Owner upgraded from "Mine, Minor-judgment cross-boundary" per @observability Gate 1 input. |
| `packages/test-utils/src/MockOTLPExporter.ts` | Mine | — | Captures exporter calls, no HTTP |
| `packages/test-utils/src/__tests__/InMemoryMetricsSink.test.ts` | Mine | — | Self-test for InMemoryMetricsSink (subset-filter semantics on read + assert APIs) |
| `packages/test-utils/src/__tests__/MockOTLPExporter.test.ts` | Mine | — | Self-test for MockOTLPExporter (per-call capture + failure injection) |
| `packages/test-utils/src/__tests__/MockWebTransport.test.ts` | Mine | — | Self-test for MockWebTransport (control points + inspectors) |
| `packages/test-utils/src/__tests__/TestTokenBuilder.test.ts` | Mine | — | Self-test for TestTokenBuilder (claim shape conformance) |
| `packages/test-utils/src/__tests__/TestTokenSigner.test.ts` | Mine | — | Self-test for TestTokenSigner (round-trip, NODE_ENV guard, barrel non-exposure) |
| `packages/test-utils/src/__tests__/deterministic-ids.test.ts` | Mine | — | Self-test for deterministic-ids (seed reproducibility + RFC 4122 v4 layout) |
| `pnpm-lock.yaml` | Mine | — | Mechanical regen (pnpm install side-effect of adding new package + deps) |
| `.gitignore` | Not mine, **Mechanical** | operations | pnpm 10.x project-local store cache exclusion (`.pnpm-store/`, `**/.pnpm-store/`). Forced by Layer A scope-drift guard discovery at Gate 2 attempt 1; mechanical addition per ADR-0024 §6.3 (Mechanical → owner review-only). Notified @operations; no plan re-approval required. |
| `package.json` | Not mine, **Mechanical** | operations | pnpm overrides block patching minimatch ReDoS via nx transitive (3 high vulns: GHSA-3ppc-4f35-3m26, GHSA-7r86-cg39-jmmj, GHSA-23c5-xmqv-rm74). Narrow override range `minimatch@>=9.0.0 <9.0.7 → >=9.0.7`. Forced by Gate 2 attempt 3 `pnpm audit --audit-level=high` triage; mechanical per ADR-0024 §6.3. Notified @operations + @security. |

### Pre-flagged classification questions for reviewers

**1. Ed25519 GSA classification (`test-only/signer.ts`).** ADR-0024 §6.4
enumerates "ADR-0027-approved crypto primitives (wherever referenced)" as a
path-independent Guarded Shared Area. Ed25519 is on that list. Two readings:

- *Strict reading:* any reference to Ed25519 routes to security as
  owner-implements. That contradicts the user-story assignment of this
  package to **test**.
- *Pragmatic reading:* the GSA carve-out exists to prevent unreviewed
  changes to **production** crypto code paths. A test-only,
  ephemeral-keys-per-test, never-bundled-into-runtime utility is materially
  different.

**Proposing Minor-judgment + security hunk-ACK at Gate 1.** Concrete
safeguards I commit to (security can demand more):

- File-level header banner: `TEST-ONLY. Ephemeral keypair per signer
  instance. Throws on production NODE_ENV.`
- Sub-path export `@darktower/test-utils/test-only/signer` — the package's
  primary `exports."."` entry deliberately does NOT re-export the signer.
- Module-init runtime guard: throws if `process.env.NODE_ENV === 'production'`.
- Self-test verifies the runtime guard fires.
- No persistent key material — keys live only in the signer instance.

Pre-flagged to @security at the start of planning so this is decided
before implementation rather than at Gate 1 review.

**2. `MetricsSink` interface co-ownership (per user story §Design line 256).**
The user story lists the `MetricsSink` interface contract as co-owned with
observability. I'll declare a minimal `MetricsSink` interface in
`src/contracts/MetricsSink.ts` and ask observability to confirm the
assertion-helper surface at Gate 1. **Pre-flagged to @observability**.

**3. `IWebTransport` interface — circular-dep avoidance.** sdk-core
(task #9) will declare the canonical `IWebTransport`. test-utils ships
first (task #8). Strategy: declare a minimal `IWebTransport` interface in
`src/contracts/IWebTransport.ts` matching the browser `WebTransport`
API's structural shape. When sdk-core lands, it declares its canonical
version with the same shape; `MockWebTransport` satisfies both via
TypeScript structural typing. **No runtime coupling, no circular dep,
no dev-dependency edge from sdk-core's prod build.**

---

## Planning

### Package layout

```
packages/test-utils/
├── package.json
├── README.md
├── tsconfig.json              (extends ../../tsconfig.base.json)
├── tsconfig.build.json        (declarations-only emit)
├── vite.config.ts             (library mode, ESM primary + CJS fallback + .d.ts)
├── vitest.config.ts           (Node env)
├── project.json               (Nx targets: build, lint, test:unit)
└── src/
    ├── index.ts                       barrel — re-exports MockWebTransport, TestTokenBuilder, deterministic-ids, InMemoryMetricsSink, MockOTLPExporter, contracts/*
    ├── contracts/
    │   ├── IWebTransport.ts           minimal interface for the test double; doc-noted as test-utils' own copy
    │   └── MetricsSink.ts             minimal interface; doc-noted as the co-owned contract surface
    ├── MockWebTransport.ts
    ├── TestTokenBuilder.ts            no crypto here — produces claim payloads (signed via TestTokenSigner)
    ├── token-claims.ts                claim shape types matching AC/MC issuance
    ├── deterministic-ids.ts           mulberry32 PRNG + seeded UUIDv4
    ├── InMemoryMetricsSink.ts
    ├── MockOTLPExporter.ts
    ├── test-only/
    │   └── signer.ts                  TestTokenSigner — Ed25519 ephemeral keys; sub-path export; NODE_ENV=production guard
    └── __tests__/
        ├── MockWebTransport.test.ts        (~3 tests)
        ├── TestTokenBuilder.test.ts        (~2 tests)
        ├── TestTokenSigner.test.ts         (~3 tests: round-trip sign/verify with public-key export only — never private; NODE_ENV=production guard fires at IMPORT time per @security; barrel does NOT expose TestTokenSigner — only the sub-path resolves)
        ├── deterministic-ids.test.ts       (~2 tests: same seed → same UUIDs, valid v4 format)
        ├── InMemoryMetricsSink.test.ts     (~3 tests: counter+subset-label assert, label-key insertion-order independence, empty-labels matches all)
        └── MockOTLPExporter.test.ts        (~1 test: per-call capture + failure injection)
```

Total: ~13 self-tests — within the user-story ~10-12 range (one over the upper bound is acceptable; the additional `InMemoryMetricsSink` test covers @observability's empty-labels-matches-all requirement).

### Dependency choices

| Choice | Decision | Reason |
|--------|----------|--------|
| Vite | `vite@^6.0.0` | Library mode is stable; matches what task #9 will use |
| Vitest | `vitest@^3.2.0` | Bumped from v2 → v3 at Gate 2 attempt 3 to clear 2 moderate transitive vulns (vite@5 path-traversal + esbuild@0.21 dev-server request-forwarding). Stays in ADR-0028 §7 compliance — only the upper bound (no v4 in unit tier) was constrained; v3 is fine. R-43's Vitest 4.0 Browser Mode mandate applies to a separate package's component-tier tests. |
| `vite-plugin-dts` | `^4.3.0` | Emits `.d.ts` alongside ESM/CJS bundles |
| Ed25519 | **Web Crypto first, `@noble/ed25519` fallback** | Node 22 supports `subtle.generateKey({name:'Ed25519'})`. Feature-test detects; falls back to audited `@noble/ed25519@^2.x` (~6 KB, MIT) when unavailable. |
| UUID | hand-rolled `mulberry32` PRNG → 16 bytes → format as v4 (~30 lines) | The whole point is **non-crypto** seeded random — pulling in `uuid` for a non-crypto helper is overkill. |
| Test framework | Vitest (Node env) | Per ADR-0028 §7 unit tier |

### Vite library mode shape

Two entries: `index` (main barrel) and `test-only/signer` (signer
sub-path). `package.json` `exports` exposes `.` and `./test-only/signer`
distinctly. Signer is **not** re-exported from the main barrel.

### `IWebTransport` minimal interface

Locked-down to what `MockWebTransport` implements + what
`SignalingClient`/`MediaTransport` (sdk-core, task #9) will eventually
consume. Browser-WebTransport-compatible by design (real `WebTransport`
API satisfies this shape):

```ts
export interface IWebTransport {
  readonly ready: Promise<void>;
  readonly closed: Promise<WebTransportCloseInfo>;
  readonly datagrams: {
    readable: ReadableStream<Uint8Array>;
    writable: WritableStream<Uint8Array>;
  };
  createBidirectionalStream(): Promise<WebTransportBidirectionalStream>;
  close(info?: WebTransportCloseInfo): void;
}

export interface WebTransportCloseInfo {
  closeCode?: number;
  reason?: string;
}

export interface WebTransportBidirectionalStream {
  readable: ReadableStream<Uint8Array>;
  writable: WritableStream<Uint8Array>;
}
```

### `MetricsSink` interface (per @observability Gate 1 input)

Co-owned with @observability per user-story §Design line 256 (named
convention author per ADR-0024 §6.5 Pattern B). The interface lives in
`packages/test-utils/src/contracts/MetricsSink.ts` for this devloop;
when sdk-core (task #12) lands, it will declare its canonical version
in `packages/sdk-core/src/telemetry/MetricsSink.ts`. All four sinks
(`OtelMetricsSink`, `InMemoryMetricsSink`, `ConsoleMetricsSink`,
`NoopMetricsSink`) MUST conform to the same shape.

```ts
// String-typed labels at the boundary (mirrors the Rust metrics::{counter,histogram,gauge}!
// macro shape). Cardinality discipline (ADR-0011: ≤10 unique values per key, ≤64 char value
// length, ≤1000 unique combos per metric) is the CALLER's responsibility — the sink does not
// pre-mangle/normalize labels.
//
// Wire reality: labels are always strings on the OTLP / Prometheus wire. Production sinks
// (OtelMetricsSink, etc., task #12) accept this same string shape and pass through to the
// OTel Meter. If a future ergonomic wrapper accepts numeric label values, that wrapper
// MUST stringify before reaching this interface, and the stringified values still count
// toward the ADR-0011 ≤10-unique-values-per-key cardinality budget.
export type MetricLabels = Readonly<Record<string, string>>;

export interface MetricsSink {
  counter(name: string, labels: MetricLabels, value?: number): void;   // value default 1
  histogram(name: string, labels: MetricLabels, value: number): void;
  gauge(name: string, labels: MetricLabels, value: number): void;
}
```

Method-name + arg-order rationale (per @observability):
- Three methods (`counter`/`histogram`/`gauge`) mirror Rust's
  `metrics::{counter,histogram,gauge}!` macros so cross-language
  reasoning stays consistent (ADR-0011 catalog patterns).
- Argument order `(name, labels, value?)` — labels are required (as a
  possibly-empty object), value is optional for `counter` (defaults
  to 1). Matches the OTel JS SDK Meter style for low surprise when
  readers context-switch to `OtelMetricsSink`.
- `Record<string, string>` — values are stringly-typed (no number
  values). Numeric label values get stringified by callers before
  reaching the sink; the sink does NOT coerce.

R-24 naming guard (`dt_client_*` regex check, throw in dev/test, warn
in prod) lives in the **production sink** (sdk-core task #12), NOT in
this passive recorder. Two reasons (per @observability): (1) the guard
is a production-sink concern; (2) `InMemoryMetricsSink` is also used
to test that the guard fires — i.e., a future test may need to record
a non-compliant name to assert on guard wrapper behavior. A
class-header comment notes this design intent so future readers do
not add a naming guard here.

### `InMemoryMetricsSink` API (per @observability Gate 1 input)

`InMemoryMetricsSink` IS the production interface PLUS test-only
inspection methods. Mirrors `MetricAssertion` (ADR-0032) vocabulary
so engineers cross-reading TS and Rust tests find the same idioms.

```ts
type RecordedMetric =
  | { kind: 'counter';   name: string; labels: Readonly<MetricLabels>; value: number; recordedAt: number }
  | { kind: 'histogram'; name: string; labels: Readonly<MetricLabels>; value: number; recordedAt: number }
  | { kind: 'gauge';     name: string; labels: Readonly<MetricLabels>; value: number; recordedAt: number };

class InMemoryMetricsSink implements MetricsSink {
  // Production interface
  counter(name: string, labels: MetricLabels, value?: number): void;       // value default 1
  histogram(name: string, labels: MetricLabels, value: number): void;
  gauge(name: string, labels: MetricLabels, value: number): void;

  // Inspection (test-only, never throw — read APIs)
  getCounter(name: string, labels: MetricLabels): number;                  // sum of recorded; 0 if never recorded
  getHistogramObservations(name: string, labels: MetricLabels): readonly number[]; // ordered observations
  getGauge(name: string, labels: MetricLabels): number | undefined;        // last recorded; undefined if never
  getRecordedMetrics(): readonly RecordedMetric[];                         // full ordered history for debug
  clear(): void;                                                           // reset between tests

  // Assertion helpers (throw on mismatch — assertion APIs)
  assertCounter(name: string, labels: MetricLabels, expected: number): void;       // exact for predictable values
  assertCounterAtLeast(name: string, labels: MetricLabels, min: number): void;     // for unpredictable counts
  assertHistogramObserved(name: string, labels: MetricLabels, minCount?: number): void; // count ≥ minCount (default 1)
  assertGaugeInRange(name: string, labels: MetricLabels, range: { min: number; max: number }): void;
}
```

Behavior contract:
- **Label-tuple-scoped** lookups (per ADR-0032): `getCounter('m', {a:'1'})`
  and `getCounter('m', {a:'2'})` return separately. Implementation uses
  a stable label-key (sorted-keys JSON). Self-test verifies key/value
  insertion order does not affect lookup.
- **Subset-filter label semantics for read/assert APIs** (per
  @observability Q3 confirmation): the assertion's `labels` argument
  is treated as a **subset filter** — a recorded entry matches if all
  assertion labels are present and equal; extra labels on the recorded
  entry are ignored. Example: `assertCounter('m', {outcome: 'error'}, 3)`
  matches a recorded entry with labels `{outcome: 'error', mh_index: '2',
  client_version: '0.1.0'}` (sums all such matches across other label
  combinations). Mirrors `MetricAssertion::with_labels(...)` semantics
  in `crates/common/src/observability/testing.rs`.
- **Empty `labels: {}` matches ALL recorded entries for that metric
  name** (the empty subset matches every label combination). Documented
  on each assertion helper and verified by a self-test.
- Doc-comments on `assertCounter`/`assertCounterAtLeast`/
  `assertHistogramObserved`/`assertGaugeInRange` explicitly state:
  "subset filter; recorded entry matches if all assertion labels are
  present and equal; extra labels are ignored."
- **Throw on missing assertion target** (e.g., `assertCounter` when no
  matching record exists): better failure mode than silent zero.
  `getCounter` returns 0 instead (read API, not assertion API).
- `getRecordedMetrics()` exposes ordered history for debugging and for
  tests asserting on emit order.
- **No naming guard.** Class header comment documents this design
  intent.

### `MockOTLPExporter` API (per @observability Gate 1 input)

OTLP exporter contract: production OTLP-HTTP exporter takes batches of
metric/trace data and POSTs to a collector endpoint. The mock stubs
the HTTP send; tests assert on what would have been sent.

```ts
type OTLPExportPayloadKind = 'metrics' | 'traces';

type OTLPExportPayload = {
  kind: OTLPExportPayloadKind;
  capturedAt: number;
  // Body is structured (ResourceMetrics / ResourceTraces objects) per @observability lean.
  // sdk-core task #12's OtelMetricsSink will pass structured objects here. If task #12
  // ends up needing serialized bytes (Uint8Array), this type is widened in that task —
  // not retroactively here.
  body: unknown;
};

type OTLPExportResult = { code: 'success' } | { code: 'failure'; error: Error };

type MockOTLPResponseSpec =
  | { status: 200 | 202 }
  | { status: 429; retryAfterMs?: number }
  | { status: 500 | 502 }
  | { status: 'network-error' };

class MockOTLPExporter {
  // Production-shape method (matches whatever interface OtelMetricsSink uses to send).
  // Per-call capture: one entry per export() call.
  export(payload: OTLPExportPayload): Promise<OTLPExportResult>;
  shutdown(): Promise<void>;
  forceFlush(): Promise<void>;

  // Inspection
  getExportedPayloads(): readonly OTLPExportPayload[];
  getMetricPayloads(): readonly OTLPExportPayload[];   // filter to kind=metrics
  getTracePayloads(): readonly OTLPExportPayload[];    // filter to kind=traces
  getExportCount(): number;
  callCount(method?: 'export' | 'shutdown' | 'forceFlush'): number;
  clear(): void;

  // Failure injection (for testing retry/error paths in OtelMetricsSink)
  simulateNextResponse(response: MockOTLPResponseSpec): void;       // single-shot, popped on next export()
  simulateAlwaysRespond(response: MockOTLPResponseSpec): void;      // sticky until clear() / next simulateAlwaysRespond
}
```

Behavior contract (per @observability):
- **Per-call capture** (one entry per `export()` call), not per-metric
  — production batches; tests must assert "exactly one batch was
  exported" or "two batches went out".
- **Failure injection** is essential — task #12's `OtelMetricsSink`
  must handle 429 (rate-limited by GC telemetry proxy per ADR-0028
  §9), 502 (collector down), and network errors. Tests drive these
  without hitting real HTTP.
- **No real HTTP** — the mock implements the same exporter interface
  `OtelMetricsSink` uses; we stub at the exporter seam, not the global
  `fetch`. `keepalive`-flag concerns are task #12's, not this mock's.
- **Body shape: structured** (`unknown` typed; tests cast to OTel
  `ResourceMetrics` / `ResourceTraces` once task #12 picks the
  concrete type). @observability's lean is structured for ergonomics
  (no protobuf decode needed in tests). If task #12 commits to bytes,
  this type is widened in that task.

### `MockWebTransport` inspector contract (per @observability future-proof input)

R-19 / R-58 require sdk-core's `SignalingClient` (task #13) to populate
`trace_parent`/`trace_state` on every outbound `ClientMessage` and
`MhClientMessage`. Task #13's tests will drive the SDK against
`MockWebTransport` and need to assert on the outbound bidi-stream
content. So `MockWebTransport` must expose enough of the outbound bidi
stream — without requiring a follow-up edit to this package later.

Inspector additions to the spec:

```ts
class MockWebTransport implements IWebTransport {
  // ... R-15 control points: simulateReady, simulateClose, simulateError,
  // simulateIncomingDatagram, simulateBidiStream, simulateServerMessage ...

  // Inspector helpers (test-only)
  getOutboundDatagrams(): readonly Uint8Array[];                          // captured datagram writes
  getOutboundBidiWrites(streamIndex?: number): readonly Uint8Array[];     // raw chunks written to a bidi-stream's writable; streamIndex defaults to 0 (first opened)
  getOpenedBidiStreams(): readonly { index: number; openedAt: number }[]; // metadata about streams the SDK opened
  clearInspector(): void;                                                 // resets recorded outbound traffic but keeps connection state
}
```

Documented expectations for downstream test authors (in package README +
JSDoc on `getOutboundBidiWrites`):
- Returns the **raw byte chunks** as written. The SDK applies 4-byte BE
  length-prefix framing per ADR-0028 §3 / R-16, so consumers must
  reconstruct frames by reading the 4-byte length prefix off a
  concatenated buffer (or use a small helper documented in the README
  showing the protobuf-es decode pattern).
- `getOpenedBidiStreams()` lets tests assert on stream-open count and
  ordering (e.g., "exactly one stream was opened for the JoinRequest").
- This is intentionally a **lower-level** inspector than a hypothetical
  `getOutboundClientMessages()` — keeping it raw avoids coupling
  test-utils to the protobuf-es generated types (which only exist after
  task #7 codegen and live in sdk-core's `proto/` directory).

### No metric/trace/log emissions in test-utils itself (per @observability)

Confirmed: this package emits **no** metrics, traces, or structured logs
from its own production code. It is a passive recording / mocking
package. No `dt_test_utils_*` namespace, no `tracing` instrumentation,
no `console.*` calls in production source (test fixtures may use
`console` freely).

### Self-test metric-name compliance (per @observability)

**All** metric names used in `InMemoryMetricsSink` self-tests start with
`dt_client_test_*`:

- `dt_client_test_join_total` (counter)
- `dt_client_test_handshake_duration_seconds` (histogram)
- `dt_client_test_active_streams` (gauge)

This sets precedent for downstream consumers and prevents copy-paste of
non-compliant examples by readers who copy the self-tests as starter
scaffolding.

### `TestTokenBuilder` claim shapes (per @security verification)

Match the **wire shape of issued claims** (per user story directive).
JWT claim names follow JWT/JOSE conventions (`sub`, `iat`, `exp`, `jti`)
— note R-53 camelCase migration applies to the AC HTTP API body, NOT
JWT claim names. **`token-claims.ts` carries a load-bearing file header
cross-referencing the canonical Rust types**:

```ts
// File: packages/test-utils/src/token-claims.ts
//
// Mirrors crates/common/src/jwt.rs:UserClaims (line ~261) and
// :MeetingTokenClaims (line ~356). Update both together if claim
// shape changes.
```

```ts
// Matches crates/common/src/jwt.rs:UserClaims
type UserClaims = {
  sub: string;       // user UUID
  org_id: string;
  email: string;
  roles: string[];
  iat: number;       // unix seconds (NOT millis) — fits in JS safe-int range until ~year 285k
  exp: number;       // unix seconds
  jti: string;
};

// Matches crates/common/src/jwt.rs:MeetingTokenClaims
type MeetingClaims = {
  sub: string;
  token_type: 'meeting';
  meeting_id: string;
  home_org_id?: string;
  meeting_org_id: string;
  participant_type: 'member' | 'external';
  role: 'host' | 'participant';
  capabilities: string[];
  iat: number;       // unix seconds
  exp: number;       // unix seconds
  jti: string;
};
```

Per @security: Rust `i64` ↔ JS `number` is safe here because Unix-second
timestamps fit comfortably in the JS safe-integer range (2^53) until
~year 285,000. A code comment documents that values are **seconds, not
millis** to prevent the easy bug.

`TestTokenBuilder.userClaims({...})` returns the claim object (with
sensible defaults); the signer turns it into a signed JWT.

### `TestTokenSigner` Ed25519 strategy (per @security Gate 1 conditions)

```ts
class TestTokenSigner {
  static async generate(): Promise<TestTokenSigner>;
  readonly publicKeyJwk: JsonWebKey;                // for downstream verifier wiring
  readonly kid: string;                             // sha-256(publicKeyJwk) base64url
  sign(claims: object, header?: { typ?: string }): Promise<string>;

  // Private-key handle is a private class field; tsdoc on the field reads:
  //   @internal NEVER serialize, log, persist, or export this handle.
  //   Web Crypto path: extractable=false. @noble path: raw scalar (asymmetry documented in banner).
  //   #privKey: CryptoKey | Uint8Array;
}
```

Implementation:

1. **Try Web Crypto Ed25519 first.** Node 22's `globalThis.crypto.subtle` supports `{ name: 'Ed25519' }`. Detected via runtime feature-test.
2. **Web Crypto path uses `extractable: false` for the private key** — `subtle.generateKey({ name: 'Ed25519' }, /* extractable */ false, ['sign'])` per ADR-0028 §5 + R-32. Public key remains extractable for `publicKeyJwk` export.
3. **Fall back to `@noble/ed25519` (exact-pinned `2.x.y`)** if Web Crypto Ed25519 is unavailable. Library inherently operates on raw scalar bytes — non-extractability cannot be enforced at the API level on this path. **Asymmetry explicitly documented in the file-level banner.**
4. **`@noble/ed25519` placement**: `devDependency` of `packages/test-utils/` only — never `dependencies`, never `peerDependencies`. test-utils itself is a `devDependency` of downstream consumers, so the library never enters a production module graph.
5. JWT signature is the raw 64-byte EdDSA signature, base64url-encoded — JOSE `EdDSA` standard.
6. **Module-init guard at top-level evaluation time** (NOT first-call). Defensive guard pattern (per @code-reviewer item 1) so the check itself does not crash in a non-Node context before throwing:
   ```ts
   if (typeof process !== 'undefined' && process.env?.NODE_ENV === 'production') {
     throw new Error('test-utils/test-only/signer cannot be loaded with NODE_ENV=production');
   }
   ```
   Error message does NOT echo the actual env-var value back (no attacker-controlled echo). Self-test asserts the throw fires at **import time** (top-level eval), not at first signing call. Self-test uses `vi.stubEnv('NODE_ENV', 'production')` + `vi.unstubAllEnvs()` cleanup so the stub does not leak into other tests in the file.

   **Browser-bundle isolation (RESOLVED — @security picked Option A).** The Option-A NODE_ENV-only guard with `typeof process` defensive wrapper is the final posture. Browser-bundle isolation is provided by package-boundary defenses (`private: true`, closed exports map, `dependencies: {}`, devDependency-only consumption, barrel non-exposure self-test, file-level banner) — multi-layered defense against the "test-only utility accidentally bundled" threat. A runtime browser-context throw was rejected as belt-and-suspenders with sentinel-coupling fragility (future test runners might trip a guard that doesn't yet know their sentinel; remediation path would decay into security-bypass-by-comment).
7. **Sub-path export only** — `@darktower/test-utils/test-only/signer`. The barrel `import { TestTokenSigner } from '@darktower/test-utils'` MUST fail to resolve. Self-test asserts the barrel does not expose `TestTokenSigner`.
8. **File-level banner** at the top of `src/test-only/signer.ts`:
   ```
   TEST-ONLY. Imported only via @darktower/test-utils/test-only/signer
   sub-path; never re-exported from the package barrel.
   Throws on import when NODE_ENV=production.
   Ephemeral keypair per signer instance. Never persisted, never logged,
   never serialized.
   Private key handle is non-extractable on the Web Crypto path.
   On the @noble/ed25519 fallback path the private scalar is raw bytes —
   the asymmetry is intentional; consumers must still treat the private
   handle as opaque and never serialize, log, or persist it.
   ```
9. **No serialization paths** for private material. No `console.log`, no `JSON.stringify`, no error message that could leak the private key bytes, the JWK, or raw signature bytes. The `publicKeyJwk` and `kid` are safe to log; private material is not. Self-test for round-trip sign/verify asserts only on signature validity + public key export — never on private-key shape.

### `package.json` — closed exports map + `private: true` (per @security)

```jsonc
{
  "name": "@darktower/test-utils",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "exports": {
    ".": {
      "types": "./dist/index.d.ts",
      "import": "./dist/index.mjs",
      "require": "./dist/index.cjs"
    },
    "./test-only/signer": {
      "types": "./dist/test-only/signer.d.ts",
      "import": "./dist/test-only/signer.mjs",
      "require": "./dist/test-only/signer.cjs"
    }
  },
  // NO "./*" wildcard fallback — that would defeat the sub-path gate.
  "dependencies": {},
  "devDependencies": {
    "@noble/ed25519": "2.x.y",   // exact pin per @security Gate 1 condition (d) — placeholder; choose latest 2.x.y at impl time
    "vitest": "^2.1.0",
    "vite": "^6.0.0",
    "vite-plugin-dts": "^4.3.0",
    "typescript": "5.7.3"
  }
}
```

`private: true` means this package can never be `pnpm publish`-ed, even
in a workspace-publish context — short-circuits the simplest production
leak path.

### `MockOTLPExporter` — PII guard (per @security)

`MockOTLPExporter` is **assertion-only**: it does NOT normalize, redact,
or strip PII from captured payloads. PII redaction is the production
exporter's responsibility and is tested at that layer (sdk-core task
#12), not via this stub.

**Test fixtures driving the mock MUST not contain real-looking PII** —
use `user-test-001@example.test`, never realistic email shapes, never
realistic display-name strings. README's `MockOTLPExporter` section
documents this constraint explicitly.

### JSDoc on public exports (per @code-reviewer item 7)

Every public class and exported function in the barrel
(`MockWebTransport`, `TestTokenBuilder`, `InMemoryMetricsSink`,
`MockOTLPExporter`, `createSeededRng`, `createSeededUuid`,
`createIdFactory`, plus `TestTokenSigner` on the sub-path) carries a
brief JSDoc block in the **first commit**: one short summary line,
documented contract (preconditions / postconditions / throws), and
`@example` for the most common usage. Internal helpers do not need
JSDoc. Lands as part of implementation, NOT as a Gate 3 fix-up.

### Vitest version note (updated post-Gate-2-attempt-3)

This package uses `vitest@^3.2.0` (Node unit tier). Bumped from v2 → v3
at Gate 2 attempt 3 to clear 2 moderate transitive vulns
(`vite@5.4.21` path-traversal + `esbuild@0.21.5` dev-server
request-forwarding). v3 stays in ADR-0028 §7 compliance — the spec
constrained only the upper bound (no v4 in unit tier); v3 is fine.
When sdk-core (task #9) lands and adopts Vitest 4 for component-tier
tests (R-43), the workspace will carry v3 (test-utils unit tier) +
v4 (web-app component tier) — likely resolved by bumping test-utils
to v4 once sdk-core lands.

### Lint config scope (per @code-reviewer item 5)

ESLint cluster-wide config is **task #17** territory
(`ci-client.yml`). This devloop ships a project-local Nx `lint`
target that no-ops cleanly when no eslint config is present, so it
will not block validation. When task #17 lands the cluster-wide
config, this package's `lint` target picks it up automatically with
no further code change. Confirmed with @operations: in scope for
task #17, not for this devloop.

### tsconfig confirmation (per @security)

`packages/test-utils/tsconfig.json` extends `../../tsconfig.base.json`,
which already declares (verified at `/work/tsconfig.base.json`):
`"strict": true`, `"noUncheckedIndexedAccess": true`,
`"exactOptionalPropertyTypes": true`. The package-local tsconfig will
NOT override or relax any of these flags. No `any` slips that could
erode the shape contract.

### `deterministic-ids` design

`deterministic-ids.ts` carries a header comment to prevent any future
reader from mistaking it for a real-randomness source (per
@code-reviewer item 2 + @dry-reviewer ack):

```ts
// File: packages/test-utils/src/deterministic-ids.ts
//
// Seeded NON-cryptographic RNG. UUIDs produced here are reproducible-by-design,
// NOT unpredictable. RFC 4122 §4.4 byte-layout (variant + version bits set) for
// shape conformance only. Test-only.

export function createSeededRng(seed: string | number): () => number;
export function createSeededUuid(rng: () => number): () => string;   // RFC-4122 v4 byte layout
export function createIdFactory(seed: string): { uuid: () => string; rng: () => number };
```

`mulberry32` is ~5 LOC of arithmetic, public domain. Same seed → same
sequence; that's the point.

### Validation pipeline (Gate 2 expectations) (per @operations Gate 1 condition #3)

Devloop verification commands that DO run against this diff:

1. `pnpm install` from repo root (lockfile regen as needed).
2. `pnpm --filter @darktower/test-utils build` — Vite emits `dist/index.{mjs,cjs}`, `dist/test-only/signer.{mjs,cjs}`, plus `.d.ts`.
3. `pnpm --filter @darktower/test-utils test:unit` — Vitest runs ~13 self-tests.
4. `pnpm --filter @darktower/test-utils lint` — no-op if eslint not yet configured cluster-wide.
5. `tsc --noEmit` — strict-mode catches typos.

Coverage gate (≥90% per R-47) is NOT enforced this devloop — coverage
gate wires up in task #17 (`ci-client.yml`). I'll target ≥90% by
construction (tests cover all public functions).

**Explicit Layer N/A justifications** (committed to be filled into
the post-implementation §`Devloop Verification Steps`):

- **Layer 1 (cargo-build)** — N/A: no Rust code in diff (TS-only package).
- **Layer 2 (formatting/lint)** — TS equivalent: `tsc --noEmit` + `pnpm lint` (ESLint/Prettier where wired). No Rust `cargo fmt`/`clippy`.
- **Layer 3 (guards)** — RUNS against the diff (scope-drift guard vs. planned-files list, classification-sanity guard vs. Cross-Boundary Classification table per ADR-0024 §6.6).
- **Layer 4 (cargo-test/integration)** — N/A: no Rust code in diff.
- **Layer 5 (sqlx-prepare)** — N/A: no DB queries.
- **Layer 6 (cargo-audit)** — N/A: no Rust code in diff. TS-side `pnpm audit` is wired in CI by task #17, not enforced this devloop.
- **Layer 7 (semantic-guard)** — RUNS against the diff (e.g., `validate-gsa-sync.sh` if any GSA-touching paths emerge — none expected on this TS-only diff).
- **Layer 8 (env-tests)** — N/A: no `infra/kind/**` change, no service code change, no proto change. test-utils is a `devDependency` of sdk-core / web-app, not a runtime artifact.

### Files I will modify

- `packages/test-utils/**` — entire new package as listed above.
- `pnpm-lock.yaml` — regenerated by `pnpm install`.
- I will NOT touch root `package.json`, `pnpm-workspace.yaml`, `nx.json`, or `tsconfig.base.json`.
- I will NOT touch any `crates/**` or `proto/**`.

### Open questions for Gate 1

1. **@security**: Is the Ed25519 test-only posture (sub-path export,
   `NODE_ENV=production` guard, ephemeral keys per signer, banner)
   sufficient for Minor-judgment route, or do you require
   owner-implements?
2. **@observability**: Plan v2 incorporates your full Gate 1 input —
   `MetricsSink` arg-order `(name, labels, value?)` with stringly-typed
   labels; `InMemoryMetricsSink` Path A subset-filter semantics on both read and assertion APIs + ADR-0032
   parity assertions (`assertCounter` / `assertCounterAtLeast` /
   `assertHistogramObserved` / `assertGaugeInRange`); `MockOTLPExporter`
   per-call capture + failure injection (`simulateNextResponse` /
   `simulateAlwaysRespond` with 200/202/429/500/502/network-error);
   `MockWebTransport` outbound-bidi-write inspector for R-19/R-58
   future-proofing; no internal metric/trace/log emissions; self-test
   names use `dt_client_test_*`. Cross-Boundary Classification table now
   lists `MetricsSink.ts` and `InMemoryMetricsSink.ts` as **Not mine,
   Minor-judgment** with Owner = `observability`. Please confirm.
3. **@code-reviewer**: Originally Vitest 2.x for unit-tier self-tests; bumped to **Vitest 3.x** at Gate 2 attempt 3 to clear transitive moderate vulns. Stays in ADR-0028 §7 compliance (spec constrains only upper bound; no v4 in unit tier). 14/14 self-tests pass under v3.2.4 with no test-source changes. ADR-compliant?
4. **@dry-reviewer**: Hand-rolled `mulberry32` (~5 LOC) + seeded
   UUIDv4 (~10 LOC) instead of pulling in the `uuid` package — the
   whole point is **non**-crypto random. Acceptable, or pull `uuid`?
5. **@operations**: No deployment surface (test-only, never
   bundled). Any operational concerns?

---

## Pre-Work

None. Workspace bootstrap (task #1) already landed in `505328e`/`b00d25d`/predecessors; `packages/` directory exists; pnpm-workspace + nx.json + tsconfig.base.json + root `package.json` are in place.

---

## Implementation Summary

Shipped `packages/test-utils/` (`@darktower/test-utils`, `private: true`)
as a Vite-library-mode TypeScript package providing R-39 deliverables.

**Package layout** (matches plan §`Package layout`):

- `src/contracts/IWebTransport.ts` — minimal `IWebTransport` shape
  matching the browser API; sdk-core (task #9) declares its canonical
  version and `MockWebTransport` satisfies both via TS structural typing.
- `src/contracts/MetricsSink.ts` — `(name, labels, value?)` arg-order;
  `MetricLabels = Readonly<Record<string,string>>`. Co-owned with
  observability (Pattern B per ADR-0024 §6.5).
- `src/token-claims.ts` — `UserClaims`/`MeetingClaims` TS types
  mirroring `crates/common/src/jwt.rs:UserClaims` (line ~261) and
  `MeetingTokenClaims` (line ~356). Load-bearing file header
  cross-references the Rust source so future drift is detectable.
- `src/MockWebTransport.ts` — implements `IWebTransport`. Control
  points: `simulateReady`/`simulateClose(code, reason)`/`simulateError`/
  `simulateIncomingDatagram`/`simulateBidiStream`/`simulateServerMessage`.
  Inspectors: `getOutboundDatagrams`/`getOutboundBidiWrites(streamIndex)`/
  `getOpenedBidiStreams`/`clearInspector` for R-19/R-58 future tests.
  Defensive copy on every chunk so callers can reuse buffers.
- `src/InMemoryMetricsSink.ts` — passive recorder. Production methods
  + read APIs (`getCounter`/`getHistogramObservations`/`getGauge`/
  `getRecordedMetrics`/`clear`) + assertion helpers (`assertCounter`/
  `assertCounterAtLeast`/`assertHistogramObserved`/`assertGaugeInRange`).
  **Subset-filter label semantics for both read AND assertion APIs**
  per @observability Gate 1 final lock (Path A): the `labels` argument
  is a subset filter throughout — recorded entry matches if all
  `labels` keys are present and equal; extra labels on the recorded
  entry are ignored. Empty `labels: {}` matches ALL recorded entries
  for the metric name. Read APIs aggregate across matches (`getCounter`
  sums; `getHistogramObservations` concatenates ordered observations;
  `getGauge` returns the last-recorded matching value, recordedAt-
  ordered). Read APIs return 0/empty/undefined for no-match; assertion
  APIs throw with descriptive errors. Single coherent semantic for
  both paths — mirrors Rust `MetricAssertion::with_labels(...)`. No
  naming guard — production-sink concern.
- `src/MockOTLPExporter.ts` — per-call payload capture with
  `kind: 'metrics' | 'traces'` discriminator; failure injection via
  `simulateNextResponse` (single-shot) and `simulateAlwaysRespond`
  (sticky), covering `200|202|429+retryAfterMs|500|502|network-error`.
  Lifecycle methods (`shutdown`/`forceFlush`) captured. No real HTTP.
  PII discipline documented in README — fixtures use synthetic values
  only.
- `src/TestTokenBuilder.ts` — claim-payload builder (no crypto).
  Synthetic defaults (`user-test-001@example.test`,
  `00000000-0000-4000-8000-...` UUIDs). Pairs with the signer
  sub-path.
- `src/test-only/signer.ts` — sub-path-only export. Module-init guard
  with `typeof process` defensive wrapper throws on
  `NODE_ENV=production`. Web Crypto Ed25519 first
  (`extractable: false` for the private key); `@noble/ed25519`
  (exact-pinned `2.1.0`) fallback. RFC 7638 thumbprint kid.
  Full file-level banner per @security condition (f).
- `src/deterministic-ids.ts` — `mulberry32` PRNG seeded from string
  (FNV-1a hash) or number; RFC 4122 §4.4 v4 byte-layout. Header
  comment establishes "non-cryptographic, reproducible-by-design".
- `src/index.ts` — barrel. Deliberately omits `TestTokenSigner`.
- `package.json` — `private: true`, closed `exports` map (only `.`
  and `./test-only/signer`, no `./*` wildcard), `dependencies: {}`,
  all build/test tooling in `devDependencies`.
- `tsconfig.json` extends `../../tsconfig.base.json` (strict +
  noUncheckedIndexedAccess + exactOptionalPropertyTypes inherited);
  `tsconfig.build.json` for declaration emit; `vite.config.ts` for
  library mode (ESM+CJS+`.d.ts` via `vite-plugin-dts`);
  `vitest.config.ts` for Node-env unit tests; `project.json` for Nx
  integration; `README.md` documents fixture-data discipline +
  outbound-write decoding pattern + signer security posture.

**Self-tests (14)**: 3 × MockWebTransport, 2 × TestTokenBuilder,
3 × TestTokenSigner (round-trip with public-key verify only;
`vi.stubEnv('NODE_ENV', 'production')` import-time guard fires; barrel
does not expose), 2 × deterministic-ids (same-seed + RFC 4122 layout),
3 × InMemoryMetricsSink (subset-filter assertion sums + assertion-throw
on missing; insertion-order-independent label match; subset-filter on
both read and assertion APIs verifying empty-labels-matches-all and
partial-labels-aggregate-across-extras + clear()), 1 × MockOTLPExporter (per-call
capture + failure injection covering all 6 response specs).

All 14 tests pass. `tsc --noEmit` clean. `vite build` produces the
expected ESM+CJS+`.d.ts` artifacts at both entry points.

**Implementation departures from plan**:

- **InMemoryMetricsSink label semantics**: implemented **Path A —
  pure subset filter on both read AND assertion APIs** per
  @observability Gate 1 final lock. Single coherent semantic mirroring
  Rust `MetricAssertion::with_labels(...)`. Read APIs aggregate across
  matches: `getCounter` SUMs values; `getHistogramObservations`
  concatenates ordered observations; `getGauge` returns the LAST-
  RECORDED matching value (recordedAt-ordered). JSDoc on each method
  states "subset filter" semantics; `getGauge` JSDoc explicitly calls
  out the last-recorded-among-matches tiebreak. Implementation
  iteration history (for the audit trail): initial subset → hybrid
  (subset assert + exact-match read, mid-implementation drift) → final
  Path A subset everywhere. Unused `labelsMatchExact` helper removed;
  assertion APIs delegate cleanly through the read APIs.
- **Self-test count went from ~12 to 14** (well within "~10-12" range
  per user-story flexibility). Two added tests (sub-path resolution +
  import-time-throw assertions for the signer) are explicitly required
  by @security condition (e).
- **Browser-bundle additional guard** (code-reviewer item 1
  sub-question routed to @security): **RESOLVED — Option A**. @security
  confirmed NODE_ENV-only with `typeof process` defensive wrapper is
  the final posture. Browser-bundle isolation is provided by
  package-boundary defenses (`private: true`, closed exports map,
  `dependencies: {}`, devDep-only consumption, barrel non-exposure
  self-test, file-level banner). Implementation matches the resolved
  posture exactly — no code change needed.

---

## Files Modified

**New files** (all under `packages/test-utils/`):

```
packages/test-utils/package.json
packages/test-utils/tsconfig.json
packages/test-utils/tsconfig.build.json
packages/test-utils/vite.config.ts
packages/test-utils/vitest.config.ts
packages/test-utils/project.json
packages/test-utils/README.md
packages/test-utils/src/index.ts
packages/test-utils/src/contracts/IWebTransport.ts
packages/test-utils/src/contracts/MetricsSink.ts
packages/test-utils/src/MockWebTransport.ts
packages/test-utils/src/TestTokenBuilder.ts
packages/test-utils/src/token-claims.ts
packages/test-utils/src/test-only/signer.ts
packages/test-utils/src/deterministic-ids.ts
packages/test-utils/src/InMemoryMetricsSink.ts
packages/test-utils/src/MockOTLPExporter.ts
packages/test-utils/src/__tests__/MockWebTransport.test.ts
packages/test-utils/src/__tests__/TestTokenBuilder.test.ts
packages/test-utils/src/__tests__/TestTokenSigner.test.ts
packages/test-utils/src/__tests__/deterministic-ids.test.ts
packages/test-utils/src/__tests__/InMemoryMetricsSink.test.ts
packages/test-utils/src/__tests__/MockOTLPExporter.test.ts
```

**Regenerated**:

```
pnpm-lock.yaml   # added @darktower/test-utils + its devDependencies
```

**Modified (mechanical, post-Gate-2-attempt-1)**:

```
.gitignore       # added .pnpm-store/ + **/.pnpm-store/ exclusion (pnpm 10.x project-local store cache)
                 # Classified Not mine, Mechanical, Owner=operations per ADR-0024 §6.3.
                 # Forced by Layer A scope-drift guard discovery; @operations notified inline.
```

**Modified (mechanical, post-Gate-2-attempt-3 consolidated audit fix)**:

```
package.json                          # root: added pnpm.overrides patching minimatch ReDoS via nx
                                      # transitive (3 high vulns: GHSA-3ppc-4f35-3m26 /
                                      # GHSA-7r86-cg39-jmmj / GHSA-23c5-xmqv-rm74).
                                      # Narrow override: minimatch@>=9.0.0 <9.0.7 → >=9.0.7.
                                      # Classified Not mine, Mechanical, Owner=operations per ADR-0024 §6.3.

packages/test-utils/package.json      # bumped vitest ^2.1.0 → ^3.2.0 to clear 2 moderate transitive
                                      # vulns (vite@5.4.21 path-traversal + esbuild@0.21.5 dev-server
                                      # request-forwarding). Stays in ADR-0028 §7 compliance.
                                      # 14/14 self-tests pass under v3.2.4 with no test-source changes.

# Both fixes notified to @operations + @security inline. Resulting `pnpm audit --audit-level=low`
# reports "No known vulnerabilities found" (rc=0) — all 5 vulns cleared in this iteration.
```

**NOT modified** (per plan scope-drift commitments): `pnpm-workspace.yaml`,
`nx.json`, `tsconfig.base.json`, any `crates/**`, any `proto/**`.

---

## Devloop Verification Steps

| Layer | Status | Evidence |
|-------|--------|----------|
| Layer 1 (cargo-build) | N/A | No Rust code in diff (TS-only package). |
| Layer 2 (formatting/lint TS-equiv) | PASS | `pnpm exec tsc --noEmit` returns 0; package-local `lint` target invokes `tsc --noEmit`. |
| Layer 3 (guards) | EXPECTED PASS | Scope-drift guard verifies diff matches plan's planned-files list; classification-sanity guard verifies the Cross-Boundary Classification table. (Run by team-lead at Gate 2.) |
| Layer 4 (cargo-test/integration) | N/A | No Rust code in diff. |
| Layer 5 (sqlx-prepare) | N/A | No DB queries. |
| Layer 6 (cargo-audit) | N/A | No Rust code in diff. TS-side `pnpm audit` lands in task #17 CI. |
| Layer 7 (semantic-guard) | EXPECTED PASS | No GSA paths touched (test-only signer is GSA-adjacent but classified Minor-judgment per @security ack). |
| Layer 8 (env-tests) | N/A | No `infra/kind/**`, no service code, no proto change. |

**Local validation (already run)**:

```
$ pnpm install
Done in 20.2s using pnpm v10.33.2

$ pnpm exec tsc --noEmit
(silent — no errors)

$ pnpm exec vitest run
 ✓ src/__tests__/deterministic-ids.test.ts (2 tests) 2ms
 ✓ src/__tests__/MockOTLPExporter.test.ts (1 test) 2ms
 ✓ src/__tests__/TestTokenBuilder.test.ts (2 tests) 2ms
 ✓ src/__tests__/InMemoryMetricsSink.test.ts (3 tests) 3ms
 ✓ src/__tests__/MockWebTransport.test.ts (3 tests) 8ms
 ✓ src/__tests__/TestTokenSigner.test.ts (3 tests) 47ms
 Test Files  6 passed (6)
      Tests  14 passed (14)

$ pnpm exec vite build
✓ 7 modules transformed.
[vite:dts] Declaration files built in 567ms.
dist/test-only/signer.mjs   3.13 kB │ gzip: 1.43 kB │ map: 11.20 kB
dist/index.mjs             14.49 kB │ gzip: 4.74 kB │ map: 41.03 kB
dist/test-only/signer.cjs  2.48 kB │ gzip: 1.21 kB │ map: 10.92 kB
dist/index.cjs             6.74 kB │ gzip: 2.49 kB │ map: 39.44 kB
✓ built in 642ms
```

`dist/` tree contains both entry points (`index.{mjs,cjs,d.ts}` +
`test-only/signer.{mjs,cjs,d.ts}`) plus sourcemaps and per-source-file
`.d.ts` declarations.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR (final, after one comment-polish nit fix at `signer.ts:110-112`)
**Findings**: 1 found, 1 fixed (non-blocking comment), 0 deferred
**Hunk-ACK**: `Approved-Cross-Boundary: security` for `test-only/signer.ts` and `TestTokenBuilder.ts` per ADR-0024 §6.4 Minor-judgment route. All 6 conditions (a-f) verified at file:line. Supply-chain posture approved (5 vulns cleared = 3 high + 2 moderate; net positive for workspace).

### Test Specialist
**Verdict**: n/a (test specialist is the implementer; self-review skipped per devloop convention)

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred
**Hunk-ACK**: `Approved-Cross-Boundary: observability` for `MetricsSink.ts` and `InMemoryMetricsSink.ts` per ADR-0024 §6.5 Pattern B (named convention author). 4 ADRs verified (0011, 0024 §6.5, 0028 §9, 0032). Path A subset-filter implemented correctly across all 3 assertion APIs. No internal metric/trace/log emissions; self-tests use `dt_client_test_*` compliant names. Future canonical-home migration to sdk-core (task #12) tracked.

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed (readonly-cast in `MockWebTransport.simulateClose:147-148` replaced with conditional spread), 0 deferred
**ADR Compliance verified**: ADR-0028 §1/§2/§6/§7 (stack, layout, distribution, testing); ADR-0024 §6 (cross-boundary); R-9 (TS strict mode via `tsconfig.base.json` extension); R-32 (no `Math.random` outside dev-tools — seeded `mulberry32` is the documented exception).
**Ownership Lens**: classified each cross-boundary edit per ADR-0024 §6.6 — `IWebTransport` (Minor-judgment, client owner, Pattern A), `MetricsSink.ts` + `InMemoryMetricsSink.ts` (Minor-judgment, observability owner, Pattern B), `test-only/signer.ts` (Minor-judgment GSA-adjacent, security owner, hunk-ACK obtained), `pnpm-lock.yaml` (Mechanical), root `package.json` + `.gitignore` (Mechanical, operations).

### DRY Reviewer
**Verdict**: CLEAR
**Findings**: 0 true-duplication, 0 fixed, 0 deferred

**True duplication findings**: None. No existing TypeScript code in repo to duplicate from. Cross-language Rust analogs (`crates/common/src/jwt.rs`, `crates/common/src/observability/testing.rs`, `crates/common/src/webtransport/**`) are intentional contract-conformance / cross-language symmetry — out of DRY scope.

**Extraction opportunities** (appended to `docs/TODO.md` under `## Cross-Service Duplication (DRY)` → `### From DRY Reviewer (Ongoing)`):
- `IWebTransport` interface canonical-home decision (test-utils ↔ sdk-core, R-13/R-39): Pattern A parallel structural declarations. Revisit at task #9 review.
- `MetricsSink` interface canonical-home decision (test-utils ↔ sdk-core, R-24/R-39): Pattern A, paired with #1. Revisit at task #12 review. Cross-language note carried: subset-filter assertion semantics intentionally mirror `crates/common/src/observability/testing.rs:681,769,861` and are NOT duplication.

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred
**Ownership-Lens hunk-ACKs**: 3 Mechanical edits in operations' domain — `.gitignore` (3-line `.pnpm-store/` exclusion), root `package.json` (`pnpm.overrides` block: minimatch ≥9.0.7), `pnpm-lock.yaml` (regen). Override materialization verified in lockfile (minimatch resolutions at 9.0.9 and 10.2.3). ADR-0028 / 0024 §6.3 / 0025 compliance verified. `package.json` posture: `private: true`, `dependencies: {}` empty, `@noble/ed25519@2.1.0` exact-pinned in devDeps only, closed `exports` map with sub-path opt-in.

---

## Tech Debt References

1. **`IWebTransport` canonical-home decision** — when sdk-core (task #9, R-13) lands, decide whether sdk-core re-declares or imports from test-utils. test-utils currently declares its own minimal version per @dry-reviewer Pattern A reasoning. Tracked for Gate 3 entry into `docs/TODO.md` per @dry-reviewer's commitment.
2. **`MetricsSink` canonical-home decision** — same shape; pairs with #1. Co-owned with @observability per ADR-0024 §6.5 Pattern B.
3. **Vitest dual-version risk** — package uses `vitest@^3.2.0` (bumped from v2 at Gate 2 attempt 3 to clear transitive moderate vulns). When sdk-core adopts Vitest 4.0 Browser Mode (R-43, R-43-mandated for component-tier in `packages/web-app`), workspace will carry v3 (test-utils unit tier) + v4 (web-app component tier). Likely resolution: bump test-utils to v4 once sdk-core lands.
4. **`MockWebTransport` README protobuf-es decode pattern** — `<!-- TODO(task #13) -->` placeholder until sdk-core's protobuf-es generated types exist. @observability acked the placeholder is acceptable.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `223aa6dc7a93bd23a7b385cb5ff1a33314b5a02c`
2. Review all changes: `git diff 223aa6dc7a93bd23a7b385cb5ff1a33314b5a02c..HEAD`
3. Soft reset (preserves changes): `git reset --soft 223aa6dc7a93bd23a7b385cb5ff1a33314b5a02c`
4. Hard reset (clean revert): `git reset --hard 223aa6dc7a93bd23a7b385cb5ff1a33314b5a02c`
5. No schema, no infrastructure manifests — `git reset` alone is sufficient.

---

## Issues Encountered & Resolutions

### Issue 1: Gate 2 attempt 1 — `.pnpm-store/` untracked files tripped scope-drift guard
**Problem**: `pnpm install` creates a project-local `.pnpm-store/` cache (pnpm 10.x default) that wasn't gitignored. Layer A scope-drift guard (`git status`-aware) flagged thousands of unplanned files.
**Resolution**: Added `.pnpm-store/` and `**/.pnpm-store/` to root `.gitignore` (Mechanical, Owner=operations); added `.gitignore` row to Cross-Boundary Classification table.

### Issue 2: Gate 2 attempt 2 — Cross-Boundary table format violations
**Problem**: Two table-format issues tripped the literal-match scope-drift parser: (a) parenthetical annotation `pnpm-lock.yaml (regen)` and (b) glob `__tests__/*.test.ts` not matching individual file paths. Per template note "bare backtick-quoted filename only — no parentheticals, no globs".
**Resolution**: Stripped parenthetical from path column (moved to Notes); expanded glob to 6 individual rows.

### Issue 3: Gate 2 attempt 3 — pnpm audit findings (5 vulns)
**Problem**: `pnpm audit` flagged 3 high (minimatch ReDoS via `nx@20.3.0` transitive — pre-existing) + 2 moderate (vite path-traversal + esbuild dev-server request-forwarding via `vitest@2.1.9` transitive — task-introduced).
**Resolution** (per Lead direction "fix all 5"): (a) added `pnpm.overrides` block to root `package.json` forcing `minimatch ≥9.0.7` (clears 3 high); (b) bumped `vitest@^2.1.0 → ^3.2.0` in `packages/test-utils/package.json` (vite/esbuild dedupe to patched versions, clears 2 moderate). Final audit: 0 vulns at low threshold. Vitest 3 API compatible with existing test code (no source changes). ADR-0028 §7 stays compliant (3.x < 4.x upper bound).

### Issue 4: `InMemoryMetricsSink` semantics terminology drift
**Problem**: @observability sent contradictory acknowledgement messages during Gate 1 (subset-filter vs exact-match vs hybrid) due to message-iteration crossover. Implementer paused before coding to seek adjudication rather than burn iteration on a flip.
**Resolution**: @observability locked Path A (pure subset-filter on both read and assert APIs) as final, mirroring `MetricAssertion::with_labels` in Rust ADR-0032. Code already matched Path A; no rework required.

---

## Lessons Learned

1. **Pre-flag pnpm 10.x project-local store cache** in any future devloop adding a TS package — `.gitignore` exclusion for `.pnpm-store/` should be in the original Cross-Boundary Classification table, not discovered at Gate 2.
2. **Cross-Boundary Classification table parser is strict-literal**: bare backtick-quoted paths only. No globs (must enumerate). No parentheticals (move qualifiers to Notes column). Worth documenting more prominently in the template.
3. **Audit-fix routing**: pre-existing transitive vulns in workspace-tooling deps (`nx`, etc.) should be addressed via `pnpm.overrides` (mechanical, narrow-range matcher); task-introduced transitive vulns should be addressed via direct-dep version bumps (clean dedupe). Both Mechanical, Owner=operations.
4. **Pattern A interface declarations defer the canonical-home decision** to the consuming package's devloop (here: sdk-core task #9 / task #12). DRY reviewer tracks as extraction opportunity in `docs/TODO.md`, not as a current finding.
5. **Idle-≠-Done plus message crossover**: when teammates send messages in tight bursts, Lead should verify visible state before treating "ready" signals as authoritative. State-sync via filesystem checks (e.g., `grep` actual versions) cuts through messaging confusion.
