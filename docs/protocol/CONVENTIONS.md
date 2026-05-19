# Protocol Conventions

Canonical rules for `.proto` files in this repository. This doc is the **spec**
for tasks #30 (file-layout cleanup) and #31 (STANDARD-lint rename sweep) of the
browser-client-join user story (R-61). New `.proto` work must conform to these
rules from the start; pre-existing deviations are draining via the R-61 chain.

## Enforcement

`proto/buf.yaml` is the enforcement mechanism. It pins:

- `lint.use: [STANDARD]` — the full buf STANDARD ruleset, no carve-outs.
- `breaking.use: [WIRE_JSON]` — wire- and JSON-compatible breakage detection.

There is no `lint.ignore` block. If a finding surfaces, the fix lives in the
`.proto` source, not in a buf config carve-out (consistent with ADR-0033 §13
and ADR-0034's "fix the parser, don't relax the check" principle).

## Rules

### 1. File layout mirrors package path

A `.proto` file lives at `proto/<package_path>/<file>.proto`, where
`<package_path>` is the protobuf `package` declaration with dots replaced by
directory separators.

Concrete: package `dark_tower.internal.v1` lives at
`proto/dark_tower/internal/v1/internal.proto`. Task #30 lands this layout.

### 2. Package version-suffix

Every package ends in `vN`, where `N` is a positive integer. `v1` is the
current major version for all packages.

New major versions live in sibling `vN+1` directories. Once a package has
external on-the-wire clients, its `vN` files are frozen — additive,
wire-compatible changes only. Wire-breaking changes get a new `vN+1` package.

### 3. Bare RPC request/response names

For each RPC `Foo`, the request type is `FooRequest` and the response type is
`FooResponse` — **bare**, not service-prefixed.

Concrete: the `RegisterParticipant` RPC on `MediaHandlerService` uses
`RegisterParticipantRequest` / `RegisterParticipantResponse`, not
`MediaHandlerServiceRegisterParticipantRequest`.

Each service's RPCs live under their own service block; cross-service name
collisions don't happen in practice and bare names read cleaner at call sites.

Rationale: Clarification Q14 in
`docs/user-stories/2026-05-02-browser-client-join.md`.

### 4. Distinct response type per RPC

STANDARD's `RPC_REQUEST_RESPONSE_UNIQUE` requires every RPC to have its own
response message type. No sharing across RPCs, even when the shapes are
currently identical.

Concrete: the legacy `HeartbeatResponse` (shared by Fast + Comprehensive
heartbeats on `GlobalControllerService`) splits into `FastHeartbeatResponse`
and `ComprehensiveHeartbeatResponse`. Task #31 lands this rename.

Rationale: Clarification Q15 in
`docs/user-stories/2026-05-02-browser-client-join.md`. The split is cheap now
and keeps future divergence (extra fields on one side) wire-clean.

## Why STANDARD, not a custom ruleset

We adopted the full buf STANDARD ruleset rather than carving exceptions because
it is the lingua franca for protobuf hygiene — future tooling, `buf breaking`
semantics, and ecosystem interop all assume STANDARD shapes. The one-time
wire-break cost to bring the repo into compliance is acceptable: there are no
on-the-wire clients outside this codebase yet (same precedent as R-60's
`MediaConnectionUpdate` redesign earlier in this story).

Carve-outs would be a permanent tax. Each `// buf:lint:ignore` annotation or
`lint.ignore` entry is a maintenance hazard ("remember to remove this") that
silently rots into the codebase. We pay the rename cost once.

## Sequencing note (R-61, task #29 → #30 → #31)

This doc lands first as the spec. Once it lands, `buf lint` fails repo-wide on
the 21 pre-existing STANDARD findings on `proto/internal.proto` and
`proto/signaling.proto` until task #31 closes. Track 2 therefore runs as a
contiguous, exclusive 29 → 30 → 31 sequence — no other devloops in flight
during the window, because every devloop's Layer 5 will fail until the rename
sweep completes.

This trade-off is accepted to eliminate the `buf.yaml` `lint.ignore` carve-out
anti-pattern, per Revision 8 of the user story.
