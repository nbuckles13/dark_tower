# @darktower/test-utils

Test doubles, mocks, and assertion stubs for Dark Tower client packages.

**Test-only.** This package is `private: true` and cannot be published. It is
consumed via pnpm workspace links as a `devDependency` of `@darktower/sdk-core`
and `@darktower/web-app`. It emits no metrics, traces, or structured logs of
its own — it is a passive recording / mocking package.

## Contents

| Export | Path | Purpose |
|--------|------|---------|
| `MockWebTransport` | barrel | `IWebTransport` test double with R-15 control points and outbound-write inspectors |
| `TestTokenBuilder` | barrel | Unsigned claim builder matching AC `UserClaims` / MC `MeetingTokenClaims` wire shape |
| `InMemoryMetricsSink` | barrel | Passive `MetricsSink` recorder with subset-label-filter assertion helpers |
| `MockOTLPExporter` | barrel | OTLP exporter stub with per-call capture and failure injection |
| `createSeededRng` / `createSeededUuid` / `createIdFactory` | barrel | Deterministic, seeded, **non-cryptographic** test IDs |
| `TestTokenSigner` | `@darktower/test-utils/test-only/signer` (sub-path) | Ephemeral Ed25519 JWT signer for unit tests |

## `MockOTLPExporter` — fixture-data discipline

The exporter mock is **assertion-only**. It does NOT redact, normalize, or
strip PII from captured payloads — that is the production exporter's
responsibility (sdk-core's `OtelMetricsSink` plus the GC `/api/v1/telemetry`
proxy's PII allowlist filter).

**Test fixtures driving this mock MUST not contain real-looking PII.** Use
synthetic values like `user-test-001@example.test` and
`00000000-0000-4000-8000-000000000001`. Never realistic email shapes, never
realistic display-name strings, never realistic IP addresses.

## `MockWebTransport` — outbound-write decoding

`getOutboundBidiWrites(streamIndex)` returns raw `Uint8Array` chunks AS WRITTEN
by the SDK. The signaling stream uses 4-byte big-endian length-prefix framing
per ADR-0028 §3 / R-16. To assert on a specific outbound `ClientMessage`,
concatenate the chunks and strip the framing.

`<!-- TODO(task #13): document the protobuf-es decode pattern once the
generated types exist in sdk-core. -->`

## `TestTokenSigner` — security posture

Imported only via the `@darktower/test-utils/test-only/signer` sub-path. The
package's main barrel deliberately does NOT expose it. Module-init guard
throws if `NODE_ENV=production`. Ephemeral keypair per signer instance — never
persisted, never logged, never serialized.

Web Crypto path uses `extractable: false` for the private key. The
`@noble/ed25519` fallback path operates on raw scalar bytes; the asymmetry
is intentional but consumers must still treat the private handle as opaque.
