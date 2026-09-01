# Devloop Output: Reshape internal.proto for the ADR-0036 MC→MH control plane

**Date**: 2026-09-01
**Task**: Reshape `proto/dark_tower/internal/v1/internal.proto` per the ADR-0036 Appendix "Internal contract" — make MC→MH meeting registration the control plane (§8); delete RouteMedia, participant-level Register, and StreamTelemetry with recorded audit evidence.
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, HEADLESS (run-story task #4)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: 3 sessions (2026-09-01) — two run-story relaunches after a session-limit interruption and a Layer-6/Layer-3 escalation

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d32ccecfb7b8ddda6329be2a083d63685a11923b` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (Gate 2 green at session 3; see §Resume 2026-09-01 session 3) |
| Implementer | `spawned` |
| Implementing Specialist | `protocol` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Auth Controller (intersection rule, ADR-0003 §5.7) | `spawned` — added at Gate 1 by Lead ruling |
| Media Handler (owner, Minor-judgment rows 3-9,16-18) | `spawned` — added at Gate 1 by Lead ruling |
| Meeting Controller (owner, Minor-judgment rows 10-12) | `spawned` — added at Gate 1 by Lead ruling |

---

## Task Overview

### Objective
Land the ADR-0036 Appendix "Internal contract" shape in `internal.proto`: registration *is* the MC→MH control plane.

### Scope
- **Service(s)**: protocol (proto + proto-gen); consuming cleanup in mh-service/mc-service is sequenced behind this task
- **Schema**: No
- **Cross-cutting**: Yes — `proto/**` is a Guarded Shared Area (wire format)

### Debate Decision
NOT NEEDED — ADR-0036 is the decision of record; this is its implementation.

---

## Cross-Boundary Classification

GSA rows per ADR-0024 §6.4 (`proto/**`, `proto-gen/**` — Mechanical is DISALLOWED, Owner column mandatory).
**Intersection rule fires** on `proto/dark_tower/internal/v1/internal.proto` (manifest:
`scripts/guards/simple/cross-boundary-ownership.yaml:33` → `[protocol, auth-controller, security]`). See §Open Question OQ-0.

| Path | Classification | Owner | GSA? | Plan row | Change |
|------|----------------|-------|------|----------|--------|
| `proto/dark_tower/internal/v1/internal.proto` | **Mine** (Domain-judgment for the intersection co-signers) | protocol | **YES** | 1 | **Intersection rule: all three of protocol + auth-controller + security co-signed at Gate 1** — see §13. The Owner cell names only the implementing owner because `dt-guard`'s `owner_not_in_manifest` check compares the cell for EXACT equality against a single manifest entry, so a multi-owner cell fails the guard; the manifest's own header says all-three enforcement is Gate-1 human-review territory. The reshape: 3 RPCs + 8 messages deleted with a tombstone block; `SubscriberSlot`/`CandidateSource`/`EgressStream`/`SelectionRules` added; request +3 fresh tags, response +4 fresh tags |
| `packages/proto-gen/scripts/verify-codegen.sh` | **Mine** | protocol | **YES** (`proto-gen/**`) | 2 | Presence asserts on the 6 new/surviving internal symbols; absence asserts on all 8 deleted symbols |
| `crates/proto-gen/tests/internal_roundtrip.rs` | **Mine** | protocol | **YES** (`proto-gen/**`) | 2b | NEW — 5 wire-shape tests (@test T1) plus the "what these deliberately do NOT assert" header |
| `crates/mh-service/src/grpc/mh_service.rs` | Not mine, **Minor-judgment** | media-handler | No | 3 | 3 stub handlers + `stub_placeholder()` + `stream_next()` deleted; response literal gains 4 truthful-zero fields with `TODO(story task 5)` |
| `crates/mh-service/src/grpc/mod.rs` | Not mine, **Minor-judgment** | media-handler | No | 4 | Module-doc fences naming the 3 retired RPCs |
| `crates/mh-service/src/lib.rs` | Not mine, **Minor-judgment** | media-handler | No | 5 | Architecture fence + the "Current Status: Stub" preamble → "Partial" (@media-handler F3) |
| `crates/mh-service/tests/register_meeting_integration.rs` | Not mine, **Minor-judgment** | media-handler | No | 6 | Request literal gains 3 fields |
| `crates/mh-service/tests/auth_layer_integration.rs` | Not mine, **Minor-judgment** | media-handler | No | 7 | Request literal gains 3 fields |
| `crates/mh-service/tests/otel_grpc_integration.rs` | Not mine, **Minor-judgment** | media-handler | No | 8 | Request literal gains 3 fields (2 sites) |
| `crates/mc-service/src/grpc/mh_client.rs` | Not mine, **Minor-judgment** | meeting-controller | No | 10 | Request literal gains 3 fields; `TODO(story task 6)` and `TODO(story task 5/6 ordering)` |
| `crates/mc-service/tests/register_meeting_integration.rs` | Not mine, **Minor-judgment** | meeting-controller | No | 11 | Mock: 3 trait methods + imports dropped; response literal gains 4 truthful-zero fields |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` | Not mine, **Minor-judgment** | meeting-controller | No | 12 | Same |
| `docs/decisions/adr-0003-service-authentication.md` | Not mine, **Minor-judgment** | auth-controller | No | 12b | **AC-1.** Component 3 Connection Tokens superseded (JSON retained under a strike); Component 5 flow + Component 6 diagram `RouteMedia` → `RegisterMeeting`; Consequences positive struck; amendment note stating the service→service decision is unchanged |
| `docs/API_CONTRACTS.md` | Not mine, **Minor-judgment** | protocol | No | 13 | §3.1 credential path; §4.1/4.2/4.3 replaced by the single-RPC control-plane spec with a superseded block |
| `docs/ARCHITECTURE.md` | Not mine, **Minor-judgment** | security | No | 14 | `connection_token` client-auth path → meeting JWT (ADR-0020) |
| `docs/WEBTRANSPORT_FLOW.md` | Not mine, **Minor-judgment** | security | No | 15 | Same, in the client→MH ladder |
| `infra/grafana/dashboards/mh-overview.json` | Not mine, **Minor-judgment** | media-handler | No | 17 | Panel description: `method` is single-valued by design |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Not mine, **Minor-judgment** | media-handler | No | 18 | Dead label corrected; transitive-coverage restated; forward-pointer to `docs/observability/slos.md`; adjacent `:15` burn-rate pointer redirected from ADR-0011 to the same file. **No alert added** |
| `docs/decisions/adr-0036-media-flow.md` | Not mine, **Minor-judgment** | security | No | 19b | **@security S15.** Appendix credential-guard sentence corrected in place, visibly, with the correction noted — true for logs, false for internal messages, because every `dt-guard` credential module is extension-scoped to `.rs`/`.ts` |
| `docs/specialist-knowledge/protocol/INDEX.md` | **Mine** | protocol | No | 19 | RPC list corrected; control-plane navigation block added |
| `docs/TODO.md` | **Mine** | protocol | No | 20 | Landed early per Lead direction; 5 Media Path Obligations + 3 further entries |
| `docs/devloop-outputs/2026-09-01-internal-contract-reshape/main.md` | **Mine** | protocol | No | 21 | This file |
| `proto/buf.yaml` | **Mine** | protocol | **YES** (`proto/**`) | 22 | **NOT authored by this devloop.** `breaking.ignore` extended with `dark_tower/internal/v1/internal.proto` — the user's acceptance of the declared Layer-6 break, made between the escalation and this resumed session (file mtime 17:09:30Z vs the devloop's last write at 06:34:40Z). Listed here because §Cross-Boundary Classification must name every file in the diff, not only the ones the implementer wrote. See §Resume (2026-09-01) |

**Rows 9 and 16 (`crates/mh-service/src/observability/metrics.rs`, `docs/observability/metrics/mh-service.md`)
are NOT in this table and were NOT touched** — @media-handler's owner ruling moved them to story task 5
so the method-label domain enumeration is reduced atomically with its executable pin
(`metrics.rs:402`'s `valid_methods`). They are listed in §8's task-5 handoff.

**Hunk-ACKs**: rows 3-8, 17 and 18 are @media-handler's; rows 10-12 are @meeting-controller's; both given at Gate 1.

**OPS-10 applied.** ADR-0031 §Ownership split supersedes ADR-0011's older documentation-ownership table
for **per-service** artifacts, so `crates/mh-service/src/observability/metrics.rs`,
`docs/observability/metrics/mh-service.md`, `infra/grafana/dashboards/mh-overview.json` and
`infra/docker/prometheus/rules/mh-alerts.yaml` are all **media-handler**-owned (rows 9, 16, 17, 18),
with observability, operations, security and test as mandatory cross-cutting *reviewers*, not owners.
Corrected from my first draft, which listed observability as owner on three of them.

**FINAL SPLIT (owner ruling, @media-handler under ADR-0031): rows 9 and 16 → story task 5; rows 17 and 18
stay here.** @observability's atomic-enumeration argument is the right axis, and the owner applied it more
precisely than either of us had — it does not put all three on the same side:

> **Guarded/executable-domain enumerations co-located with, or cross-file-coupled to, the method-label pin
> → task 5, reduced atomically. Standalone operator-facing prose that goes more-false at this commit and
> has no pin coupling → here.**

- **Row 9 (`metrics.rs:12,134`) → task 5.** Same file as the only executable pin (`:402 valid_methods`) and
  the test enumeration (`:323-326`). Setting the doc to "1 value" while the same-file pin still asserts 4
  is a worse **intra-file contradiction** than uniform staleness.
- **Row 16 (`mh-service.md:218,221`) → task 5.** "8 = 4 methods × 2 statuses" is the cardinality
  enumeration coupled cross-file to the pin's reduction.
- **Row 17 (`mh-overview.json:1676`) → HERE.** Free-text panel description, not a guarded enumeration; the
  PromQL is data-driven and no `valid_methods`-style pin exists anywhere in the dashboard, so there is zero
  atomicity benefit to deferring. Same class and same 3am hazard as row 18.
- **Row 18 (`mh-alerts.yaml:16`) → HERE.** Agreed by all.

And the owner closed @observability's "two specialists" risk directly: it is **one** specialist — media-handler
owns rows 9 and 16 in task 5 *and* co-signs them here — so they cannot half-rot across the boundary, and §8's
handoff enumerates them. Net effect: rows 9 and 16 leave this diff entirely, **shrinking** the co-sign surface
to rows 3-8, 17, 18.

**[superseded — original OQ-4 reasoning kept for the record]** @observability ruled OQ-4 and the split is
theirs, not a compromise: *"leave what was already true" does not protect `mh-alerts.yaml:16`, because it was
never true* — it justifies having no RegisterMeeting alert by reasoning about `method="register"`, the
**participant-level** RPC, today, before my commit. My commit makes it doubly false. It is one line, no
design ambiguity, and it is the only one of the four whose failure mode is an operator at 3am reading a
stale reason *not* to alert and moving on; the other three are documentation drift, which is annoying
rather than dangerous. The other three are **enumerations of the method-label domain**, and task 5 must
touch the other nine surfaces of that domain anyway (including the only executable pin,
`metrics.rs:402`'s `valid_methods`) — splitting one enumeration across two commits by two specialists is
exactly how five end up stale and seven fixed. `mh-alerts.yaml:16` is exempt because it is not an
enumeration of the domain at all; it is an alerting-rationale statement that happens to cite one value.
**@media-handler's F5 asked for the prose homes here; @observability's don't-split-the-enumeration
argument directly addresses that split and I have taken it. @media-handler has the final word as owner
(ADR-0031) and I have asked them to confirm or override.**

**[superseded note]** All four are
media-handler-owned per ADR-0031 and **none is compile-forced** — my compile path does not need them, so
absorbing them would be scope I chose rather than scope the pipeline required. I withdrew them when
media-handler was not on the panel; the Lead has since spawned @media-handler as a reviewer, so the owner
is here and the decision is theirs. **@media-handler: they go stale at MY commit (I delete the emission
sites), so someone must take them — you or task 5. Your call, and you may upgrade them to
Domain-judgment.**

**No file is classified Mechanical.** The remaining cross-boundary rows are; per ADR-0024 §6.3 Minor-judgment requires owner confirmation at Gate 1 and Gate 3. media-handler and meeting-controller are **not on this team** (they are tasks #5/#6) — routed to @team-lead as OQ-0b.

---

## Gate 1 — Plan Approval (Lead)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed (T1, T2 resolved) |
| Observability | confirmed (F-OBS-1/2/3/4 resolved) |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Auth Controller (intersection rule) | confirmed — trailer conditional on AC-1 + AC-2 in landed diff |
| Media Handler (owner) | confirmed — hunks 3,4,5,6,7,8,17,18 owner-ACKed (rows 9,16 moved to task 5 by owner ruling) |
| Meeting Controller (owner) | confirmed — 3 hunks owner-ACKed at Gate 1 |


**Gate 1 CLOSED 2026-09-01 — Plan approved by Lead.** All ten reviewers confirmed. Layer B
classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

### Lead rulings at Gate 1 (2026-09-01)

- **OQ-0a — route (a), NOT route (b).** The ADR-0024 §6.4 / ADR-0003 §5.7 intersection rule names
  `[protocol, auth-controller, security]` for this file. The plan initially closed it by asking @security
  to co-sign a downgrade they had already said was not theirs to make; ADR-0024 §6.2 monotonicity exists
  to prevent exactly that. Lead spawned @auth-controller as the third owner. **Vindicated concretely**:
  @auth-controller produced AC-1 — `docs/decisions/adr-0003-service-authentication.md` still specified the
  deleted connection-token credential as live while three *derivative* docs had been corrected. Reachable
  only by the owner whose domain it is.
- **OQ-0b — team composition, not exemption.** @media-handler and @meeting-controller spawned as owner
  reviewers for the Minor-judgment rows (ADR-0024 §6.3 requires the owner to *be* a reviewer). Both
  owner-ACKed at Gate 1 and both were asked whether any row is Domain-judgment mis-parked here; both
  declined to upgrade. The task prompt independently instructed pairing with both.
- **OQ-0c — do NOT escalate now; escalate at Gate 2 with the measured log.** `docs/protocol/CONVENTIONS.md`
  lines 58-60 address this task's implementer by name and prescribe the intentional-wire-break route.
  The declaration's value is that the *real* log is diffed against it class by class, so escalating before
  the log exists would hand a human a prediction instead of a comparison. Lead will not accept the red.
- **`proto/buf.yaml` is off-limits for the remainder of this devloop.** No widening of `breaking.ignore`,
  not even a narrowly-scoped second entry: buf has no per-finding acceptance, so it could not be scoped to
  the 11 findings and would blind the MC↔MH contract across story tasks 8-10 — including the `FIELD_*`
  class that is the only mechanical detector of the OPS-2 tag-reuse hazard.
- **OQ-1 — keep `egress_stream_id`.** Task field list is explicit; no deviation authorised. Uniqueness
  obligation on both it and `(subscriber.sender_id, subscriber.slot_id)`, and the comment must say why it
  exists separately or the next reader deletes it as redundant identity.
- **OQ-2 — one echoed `transport_mode` + a MUST that MH reject a heterogeneous registration.** The
  `repeated` alternative reintroduces the parallel-list shape §6 makes unrepresentable on the request side.
  The scope limit must name story 3 as its expiry.
- **AC-1 required in-diff** (doc-only superseding amendment to ADR-0003, confined to three loci).
- **S16 routed OUT of scope, tracked.** Seven of fourteen ADR-0024 §6.4 canonical GSA globs match nothing
  in the tree. Not absorbed here — it spans five mirrors plus the manifest plus `gsa_sync.rs` and governs
  auth/crypto ownership. @security files the entry at Gate 3. `proto/**` is alive, so this devloop's own
  classification is sound.

---

## Planning

### 0. Mechanism restatement (before the instance)

The task reads as three deletions plus a message reshape. Restated as mechanism:

> **A control-plane push whose response cannot distinguish *"I received your instruction"* from *"my live behaviour reflects it"* reports healthy while being wrong.** The remedy is a response field carrying the state the *executing* path actually reached, plus a request shape where each unit of work is self-contained — no behaviour keyed to an id that has to be joined against a parallel list.

Restated that way the class is wider than the task names, and **every sibling is in the same file, under the same owner**:

| Sibling | Shape | Gap |
|---|---|---|
| `RegisterMCResponse` (GC→MC) | `accepted` + `fast_heartbeat_interval_ms` + `comprehensive_heartbeat_interval_ms` | GC pushes two cadences; MC never echoes what it applied. GC cannot tell a heartbeating MC from one that ignored the interval. |
| `RegisterMHResponse` (GC→MH) | `accepted` + `load_report_interval_ms` | identical shape, identical gap |
| ~~`AssignMeetingWithMhResponse` (GC→MC)~~ | ~~`accepted` + `rejection_reason`~~ | **NOT a sibling — withdrawn.** @dry-reviewer checked the apply path rather than the message shape and is right: `crates/mc-service/src/grpc/mc_service.rs` applies **synchronously** — `store_mh_assignment(...).await?` completes *before* the response is constructed — and `rejection_reason` already makes failure expressible. There is no async apply and no two-ends-must-agree configuration value. **Including it would make the entry over-claim, and the concrete cost is that someone later "fixes" it by adding an echo field that echoes nothing.** |

The mechanism is **asynchronous or silently-partial apply**, not "the response is a boolean" — which is why
the third candidate drops out and the two that remain are confirmed by their *apply sites*, not their
shapes: `crates/mc-service/src/grpc/gc_client.rs:239-252` does `if response.fast_heartbeat_interval_ms > 0
{ …store }` (and the same again for the comprehensive interval), so if GC sends 0 the MC silently keeps
its own default and GC never learns which cadence is live; `crates/mh-service/src/grpc/gc_client.rs`
carries the same idiom byte-for-byte for `load_report_interval_ms`.

These are the same failure ADR-0036 §11 names as its cautionary precedent — *"a capacity value advertised to GC and enforced nowhere — declared in one place, assumed in another, verified nowhere."* ADR-0036 §8 mandates the fix only for `RegisterMeeting`, and widening to three more RPCs is out of this task's scope and touches GC/MC semantics I do not own. **Surfaced, not absorbed**: recorded as a `docs/TODO.md` debt entry (row 20) naming the three siblings and the §8 remedy, so the next author sees the pattern rather than re-deriving it.

### 1. Audit evidence — independently re-verified

All three claims re-verified from scratch against the tree at `d32ccec`. Commands and results below; these go into the commit message verbatim.

**Deletion 1 — `RouteMedia` + `RouteMediaRequest` + `RouteMediaResponse` + `RoutingOptions` + `CascadeDestination`**

```
$ grep -rn "RouteMedia\|route_media" . --exclude-dir=target --exclude-dir=.git --exclude-dir=node_modules
```
Rust/TS hits, bucketed:
- **Impl is a stub**: `crates/mh-service/src/grpc/mh_service.rs:192-209` — logs two counts, records a metric, returns `RouteMediaResponse{success:true}`. No routing.
- **No production caller anywhere**: zero hits in `crates/mc-service/src/**` (the only service that could call it — the MC→MH client is `crates/mc-service/src/grpc/mh_client.rs`, which exposes `register_meeting` only). Zero in `crates/gc-service/**`, `crates/env-tests/**`, `packages/**`.
- **Only test mocks + metric labels**: `crates/mc-service/tests/register_meeting_integration.rs:65`, `crates/mc-service/tests/otel_grpc_outbound_integration.rs:124` (both `Status::unimplemented`); `crates/mh-service/src/observability/metrics.rs:12,134,323-324,402,464` and `crates/mh-service/tests/errors_grpc_metrics_integration.rs:41-47,61,71` (string label values, no RPC coupling).
- **Corroboration (@dry-reviewer D8, independently re-run)**: `transcode`, `mix_audio`, `target_codec`, `target_bitrate` have zero Rust/TS occurrences outside generated bindings — nothing re-implements what `RoutingOptions` encoded. `cascade` appears only in the stub's log field and in ADR prose about a *future* handler-to-handler cascade (ADR-0036 §Explicitly out of scope).
- **Design reason**: ADR-0036 §4 (MH keyless) + §7 (MH type-blind) — a transcoding/mixing relay must decrypt. It cannot exist.

**Deletion 2 — participant-level `Register` + `RegisterRequest` + `RegisterResponse{connection_token, media_handler_url}`**

```
$ grep -rn "RegisterRequest\|RegisterResponse" --include=*.rs --include=*.ts --include=*.proto . | grep -v RegisterMeeting | grep -v RegisterMC | grep -v RegisterMH
$ grep -rn "connection_token" . --exclude-dir=target --exclude-dir=.git --exclude-dir=node_modules
```
- **Impl is a stub returning a placeholder token**: `crates/mh-service/src/grpc/mh_service.rs:68-92` — `connection_token: stub_placeholder()`, where `stub_placeholder()` (`:26`) returns the literal `"STUB-PLACEHOLDER"`, and `media_handler_url: "stub://localhost"`.
- **No production caller**: zero hits outside `mh_service.rs` and the two MC test mocks (`register_meeting_integration.rs:51`, `otel_grpc_outbound_integration.rs:114`, both `Status::unimplemented`).
- **Every other tree-wide `connection_token` is a tokio `CancellationToken`**: `crates/mh-service/src/webtransport/server.rs:186,196`; `crates/mc-service/src/webtransport/server.rs:242,253`; `crates/mc-service/src/actors/meeting.rs:606,611,920,925` — all `self.cancel_token.child_token()`. Named per ADR-0023:176. Zero credential semantics.
- **The TypeScript `RegisterResponse` is the unrelated AC HTTP type**: `packages/sdk-core/src/http/types.ts:49` — `export interface RegisterResponse extends AuthTokenResponse`, hand-written, exported at `index.ts:53`, consumed only by `AuthApiClient.register()` (`http/AuthApiClient.ts:58,73`) and the e2e fixture (`packages/web-app/e2e/fixtures.ts:29,560`). The *generated* internal one lives in `packages/sdk-core/src/proto/dark_tower/internal/v1/internal_pb.ts` and nothing outside `src/proto/` imports it. **Recording the name collision explicitly (@security S13)**: a generated wire type and a hand-written HTTP type sharing one name in one package is exactly the condition under which a future "no production caller" grep audit reaches the wrong answer — and this task's justification rests on such an audit. Both were checked; the collision is noted in the commit message.
- **Design reason**: the client authenticates to MH with the meeting JWT (`MhJwtValidator::validate_meeting_token`), not a second token. The client-facing half of this credential was already deleted in the 2026-04-13 devloop (`signaling.proto` `MediaServerInfo` `reserved 2; reserved "connection_token";`). This deletes the server-side half.

**Deletion 3 — `StreamTelemetry` + `StreamTelemetryRequest` + `StreamTelemetryResponse`**

```
$ grep -rn "StreamTelemetry\|stream_telemetry" . --exclude-dir=target --exclude-dir=.git --exclude-dir=node_modules
$ grep -rn "jitter" docs/decisions/adr-0036-media-flow.md
```
- **Stub, no production caller**: `crates/mh-service/src/grpc/mh_service.rs:217-258` drains the stream and returns `{received:true}`. Zero callers outside the two MC test mocks (`register_meeting_integration.rs:72`, `otel_grpc_outbound_integration.rs:130`).
- **Its dimensions are exactly what §11 bars**: `StreamTelemetryRequest` carries `user_id` (per-participant) + `stream_id` (per-stream) + `bytes_sent`/`bytes_received`/`bitrate` + `timestamp` — a per-stream, time-ordered size sequence. ADR-0036 §11 verbatim: *"No per-frame, per-participant, or per-stream-identity dimension in media-path logs, metric labels, or span attributes... **the time-ordered sequence of sizes for a single stream is the voice-activity trace**."* Co-signed by @observability.
- **§11 states the replacement**: *"MH self-monitors internal latency, decomposed"* — histograms owned by the forwarder, not MH-pushed per-stream records.
- **`jitter_ms` measures a struck objective**: ADR-0036:1268 amendment table — *"ADR-0011, handler jitter objective | **Unmeasurable** — MH forwards and does not buffer; perceived jitter is a client-side jitter-buffer property and jitter-buffer design is out of scope. Struck."*

**No claim failed. Nothing is being deleted that has a production caller.** Per @operations OPS-4, the commit message frames this evidence as the **deploy-safety argument** (old-MC→new-MH would get `UNIMPLEMENTED`; there is no old MC that calls these), not merely as deletion justification.

### 2. Proposed proto shape — GATE-1 PROPOSAL AS REVIEWED. **NOT the source of truth.**

> **Read this before reading the snippet below (@meeting-controller, Gate 3).** This block is a
> **point-in-time record of what reviewers confirmed at Gate 1**, kept for the audit trail. The
> authoritative contract is `proto/dark_tower/internal/v1/internal.proto`. **They have diverged: 194 of
> this snippet's 516 lines no longer appear verbatim in the landed file**, because the Gate-1 and Gate-3
> findings were applied to the proto and deliberately not back-ported here.
>
> **It is not re-synced, and that is the decision rather than an omission.** @observability's F-OBS-4
> finding in this same devloop was that one enumeration living in two homes drifted *in both directions
> across two consecutive rounds of active review*, by the person who owned it, with both copies in front
> of them. The lesson recorded there — *re-syncing buys the same fragile arrangement with a fresher
> timestamp* — applies with far more force to a 516-line copy than it did to a five-value list. Syncing
> this once would create a second apparently-current copy of the whole contract with no mechanism keeping
> it current, which is worse than a copy that is plainly labelled stale. Proto comments ride in
> `SourceCodeInfo` and are machine-assertable; this markdown block never can be.
>
> **The substantive divergences are all Gate-1/Gate-3 findings, each recorded in §11 with its reasoning**:
> the `generation` → `policy_generation` rename (@meeting-controller F1); the reject-vs-hold split on
> unresolved `sender_id` and the removal of my F1a over-correction (@media-handler F1/F1a); the
> all-edges-or-none clause; the five-value `outcome` enumeration (@observability F-OBS-1/2); the OPS-11
> ordering constraint; and the T2 producer-obligation split. **Where this snippet and the landed file
> disagree, the landed file is correct.**


Vocabulary is **imported, never re-declared** (@dry-reviewer D1, @security S2): `dark_tower.signaling.v1.TransportMode` on both the request field and the response echo. `internal.proto` keeps its existing `import "dark_tower/signaling/v1/signaling.proto"`.

`signaling.v1.MediaStream` is deliberately **not** embedded as the source reference (@dry-reviewer D2): it carries `media_kind` + `EncodingParameters`, and ADR-0036 §7 makes MH type-blind. The source reference is the key-id pair `(sender_id, stream_number)`, using the existing spellings.

```proto
// ============================================================================
// MC -> MH control plane (ADR-0036 §7, §8)
// ============================================================================
//
// RETIRED 2026-09-01 (ADR-0036 Appendix, "Internal contract"). proto3 has no
// `reserved` for message names or RPC method names, so this block is the
// tombstone: a gRPC method name is a wire path component
// (`/dark_tower.internal.v1.MediaHandlerService/Register`) and re-adding any
// of these names with different semantics would silently re-point a live path.
// Do NOT resurrect these names.
//
//   rpc Register        -> messages RegisterRequest, RegisterResponse
//   rpc RouteMedia      -> messages RouteMediaRequest, RouteMediaResponse,
//                                   RoutingOptions, CascadeDestination
//   rpc StreamTelemetry -> messages StreamTelemetryRequest,
//                                   StreamTelemetryResponse
//
// Evidence for each deletion is in the commit message and in
// docs/devloop-outputs/2026-09-01-internal-contract-reshape/main.md §1.

// One subscriber's receive slot, as addressed by the forwarding policy.
//
// SSoT: this is a REFERENCE to identifiers defined canonically elsewhere; the
// rules are NOT restated here.
//   - `sender_id` rules (1..=65535, zero never valid, fail-closed):
//     `dark_tower.signaling.v1.JoinResponse.sender_id`.
//   - `slot_id` rules (16-bit, scoped to ONE subscriber's connection, MH's
//     routing table MUST be keyed on (subscriber, slot_id) never slot_id
//     alone, reject-never-truncate):
//     `dark_tower.signaling.v1.ReceiveSlot.slot_id`.
// Do NOT introduce a shared MAX_16BIT_ID constant or validate_16bit_id()
// helper collapsing the two — `sender_id` carries a non-recycling allocation
// bound, `slot_id` a per-subscriber validation bound.
//
// SENDER IDS ARE MEETING-SCOPED. RESOLVE THEM AGAINST THIS REGISTRATION'S
// `meeting_id` AND NOTHING ELSE.
//
// A `sender_id` is a 16-BIT PER-MEETING ORDINAL, not a global handle: the
// key-id layout is `sender_id`(16) | `stream`(8) | `generation`(40)
// (ADR-0036 §2), so `sender_id` 5 exists CONCURRENTLY IN EVERY MEETING on a
// handler. Therefore:
//   - MH MUST resolve every `sender_id` in an `EgressStream` against the
//     connections of the registered meeting ONLY.
//   - MH MUST NOT maintain a global `sender_id` -> connection index.
//   - A `sender_id` that would resolve only in ANOTHER meeting MUST NOT
//     resolve at all. This falls out of the scoping rule rather than needing
//     its own check, which is the point: if you only ever look inside one
//     meeting, you cannot cross meetings.
//
// UNRESOLVED IS NOT THE SAME AS ILLEGAL, AND THE DIFFERENCE IS LOAD-BEARING.
// An earlier draft of this comment said "MH MUST REJECT the registration if
// any sender_id does not resolve within that meeting". That was WRONG and is
// corrected here rather than quietly reworded, because the wrong version reads
// as the safer one:
//
//   - MALFORMED -> REJECT THE REGISTRATION. `sender_id` 0, or a value outside
//     1..=65535. These can never become valid, so accepting them is accepting
//     a policy that can never be honoured.
//   - IN-MEETING BUT NOT YET CONNECTED -> HOLD, DO NOT REJECT. ADR-0036 §8
//     has the handler "drain and promote pending connections" on registration:
//     a `sender_id` pending promotion WITHIN THE REGISTERED MEETING is exactly
//     the case the handler exists to rescue, and a blanket reject would make
//     registration hostile to the mechanism §8 relies on for restart recovery.
//     The egress stream stays in the applied policy and forwards nothing until
//     the source connects; the subscriber's slot reports its §6 state.
//     (Concretely, and not as the reason: loopback's own subscriber is
//     provisionally accepted and promoted BY THIS VERY RPC, so at the instant
//     the registration is processed the referenced connection is mid-promotion.
//     Rejecting on non-resolution would make the CONTROL PLANE HOSTAGE TO
//     DATA-PLANE CONNECTION TIMING.)
//
//     A PENDING SOURCE DOES NOT HOLD BACK `applied_generation`, AND MUST NOT.
//     The two are different instruments: `applied_generation` is PER-SNAPSHOT
//     ("is this policy installed on the forward path?"), the §6 slot state is
//     PER-EDGE ("is this edge carrying media?"). A snapshot installed for
//     `policy_generation` N with one source not yet connected IS faithfully
//     applied, so
//     the echo advances to N; the absent source is reported through slot state.
//     Gating the echo on source connectivity would peg MC's §8 comparison into
//     permanent divergence-and-re-assert for any legitimately-absent source — a
//     NEW false-alarm bug, in the very field that exists to prevent false
//     liveness. The case where the echo must NOT advance is the snapshot never
//     being installed at all (mailbox full, apply errored), and that is stated
//     once, at `RegisterMeetingResponse.applied_generation`.
//
// THE PRECONDITION ABOVE IS SCOPED TO THE TIMING CASE ONLY. Do NOT generalise
// it. The structural rejects in these messages are evaluable from the request
// alone, need no external state, and stay IMMEDIATE HARD REJECTS: the
// cross-meeting / structurally-unresolvable half of this very check; duplicate
// `egress_stream_id`; duplicate `(subscriber.sender_id, subscriber.slot_id)`;
// the candidate-per-egress and egress-per-meeting count bounds;
// `transport_mode == TRANSPORT_MODE_UNSPECIFIED`; and heterogeneous transport
// modes across egress streams. The security-load-bearing rejects are in that
// list, and "F1 says defer" must not be read onto them.
//
// Holding rather than rejecting costs nothing in safety, because the scoping
// rule above is what carries the cross-tenant property: an unresolved
// `sender_id` under meeting-only lookup forwards nothing, whereas the tempting
// "relax resolution so the registration succeeds" fix is precisely the drift
// toward a global fallback this comment forbids. The reject is for what can
// never be valid; the hold is for what is not valid YET.
// A global index is not merely imprecise; it is a CROSS-MEETING MEDIA-CROSSING
// PRIMITIVE, and cross-tenant leakage is the one threat class a keyless relay
// does not otherwise expose. It is also the shape that passes every unit test
// written against a single-meeting fixture.
//
// NO GATE IN THIS STORY CAN CATCH A GLOBAL INDEX. Loopback is one meeting with
// one participant, so a correctly meeting-scoped index and a global one are
// OBSERVATIONALLY IDENTICAL across all seven validation layers — unit, integration,
// env-test, every guard. This contract is the only artefact that survives into
// task 5 carrying the requirement, which is why it is a MUST here and not a
// review note there.
//
// This is a NEW obligation created by this reshape, not an existing one:
// `crates/mh-service/src/session/mod.rs:133` keys connections
// `meeting_id -> participant_id -> Vec<ConnectionEntry>` (correctly
// meeting-scoped), but `EgressStream` references `sender_id`, not
// `participant_id` — so the resolution index does not exist yet and its shape
// is a free choice. Enforcement owner: story task 5, NOT this task.
//
// OBSERVABILITY (ADR-0036 §11): both fields are stream identity. NEVER a
// metric label, NEVER a span attribute, NEVER a per-frame log dimension. That
// this message is a CONTRACT field rather than a metric label is what makes it
// legitimate here; carrying it into telemetry is the voice-activity trace §11
// bars, and this message is the obvious new back door for it now that
// StreamTelemetry is gone.
message SubscriberSlot {
  uint32 sender_id = 1;
  uint32 slot_id = 2;
}

// One source this egress stream may forward, referenced by its key-id pair.
//
// DELIBERATELY NOT `signaling.v1.MediaStream`: that type carries `media_kind`
// and `EncodingParameters`, and ADR-0036 §7 makes MH type-blind — media kind
// on this contract is knowledge MH has no legitimate source for, and a new
// media kind must cost zero MH changes. MH is told what a stream DOES
// (`supersede_on_independent_frame`, `transport_mode`), never what it IS.
//
// OBSERVABILITY (ADR-0036 §11): as `SubscriberSlot` — never a label, span
// attribute or log dimension.
message CandidateSource {
  // Publishing participant.
  //
  // MEETING-SCOPED. The full MUST — resolve against this registration's
  // `meeting_id` only, reject the registration if it does not resolve there, and
  // NEVER maintain a global `sender_id` index — is stated once at
  // `SubscriberSlot.sender_id` and applies IDENTICALLY HERE. It is called out
  // rather than left to inference because this is the reference an implementer
  // reaches for when building the ingress-side lookup, which is the side where
  // a global index is most tempting.
  uint32 sender_id = 1;

  // Which of that publisher's streams. A uint32 carrying 8-BIT semantics: the
  // 64-bit SFrame key id allots 8 bits to the stream.
  // SSoT: `dark_tower.signaling.v1.SendStream.stream_number`.
  // ANCHOR (DRY): layout at `crates/media-protocol/src/frame.rs::KEY_ID_BYTES`
  // (`sender_id` 16 | `stream` 8 | `generation` 40).
  // This is NOT `SubscriberSlot.slot_id` (16-bit, relay region).
  uint32 stream_number = 2;
}

// One egress stream: a complete, self-contained forwarding instruction.
//
// SELF-CONTAINED BY CONSTRUCTION (ADR-0036 §6 make-conflicts-unrepresentable).
// Behaviours are NOT a second repeated field keyed by `egress_stream_id`: a
// behaviour whose id matches no edge, and an edge with no behaviour, are both
// unrepresentable here. Two parallel lists make that a class of bug; one
// message makes it a shape that cannot be written.
//
// UNIQUENESS, and MH's obligation. Within one RegisterMeetingRequest, BOTH
// `egress_stream_id` AND `(subscriber.sender_id, subscriber.slot_id)` MUST be
// unique. MH MUST REJECT THE WHOLE REGISTRATION on a duplicate of either —
// never last-write-wins, never partial application; a duplicate subscriber slot
// means "two sources into one slot". Same posture as
// `signaling.v1.ReceiveCapability`. Enforcement owner: story task 5.
message EgressStream {
  // MC-allocated, stable identity for this egress stream, unique within one
  // meeting registration.
  //
  // NOT redundant with `subscriber` even though the two are 1:1 today.
  // `subscriber` is the DATA-PLANE ADDRESS: `slot_id` is client-chosen and is
  // renumbered whenever the subscriber re-declares its receive capability.
  // `egress_stream_id` is the POLICY-PLANE IDENTITY that survives such a
  // renumber, and it is what per-stream state hangs off: §7 pending-switch
  // state, and §11's per-stream (not per-participant) egress budget in story 2.
  // Because both are present, both carry the uniqueness obligation above; a
  // reader must not have to guess which one MH keys on.
  //
  // OBSERVABILITY (ADR-0036 §11): stream identity — never a label.
  uint32 egress_stream_id = 1;

  // Who receives this stream, and in which of their slots. MH writes
  // `subscriber.slot_id` into the frame's relay-region `stream_id` for this
  // egress. ANCHOR (DRY):
  // `crates/media-protocol/src/frame.rs::STREAM_ID_FIELD_BYTES` = 2.
  SubscriberSlot subscriber = 2;

  // Sources eligible to fill this egress stream.
  //
  // REPEATED EVEN THOUGH SIZE IS 1 THIS STORY. The loopback assignment yields
  // exactly one candidate (the subscriber's own audio), but audio selection
  // among many speakers (§7, story 5) must be ADDITIVE — it adds candidates and
  // a ranking rule, not a new message shape.
  //
  // BOUND: MH MUST reject a registration whose candidate count exceeds the
  // configured per-egress bound, and whose `egress_streams` count exceeds the
  // configured per-meeting bound, BEFORE building any routing table — the same
  // posture the existing handler already takes on scalars (MAX_ID_LENGTH 256,
  // endpoint 2048). Unbounded repeated fields on a control-plane message are an
  // allocation surface. Enforcement owner: story task 5, NOT this task.
  repeated CandidateSource candidate_sources = 3;

  // MC-assigned forwarding priority (ADR-0036 §7). Lower value = higher
  // priority; MC owns the numbering.
  //
  // ASSIGNED BY MC, NEVER BY A CLIENT, AND THAT IS A SECURITY PROPERTY. Priority
  // groups BOUND how far a publisher's self-declared salience signal can promote
  // it. A client able to name its own group could promote itself past that
  // bound, turning "hold the meeting's audio floor" into a one-line client
  // patch — which is exactly why `signaling.v1.SendTarget` deliberately carries
  // transport mode and NOT priority group. This field is the reciprocal of that
  // disclaimer.
  //
  // MH derives no priority from media type; it has no media type (§7).
  uint32 priority_group = 4;

  // Whether a superseding frame permits discarding queued frames for this
  // egress (ADR-0036 §7). A BEHAVIOUR, not a media type: MH is told what the
  // stream does, never that it is video.
  //
  // FALSE for the loopback audio egress stream — every frame forwards. "Every
  // audio frame forwards" is a consequence of this flag, not of MH knowing the
  // stream is audio.
  //
  // ANCHOR (DRY):
  // `crates/media-protocol/src/frame.rs::FLAG_INDEPENDENTLY_DECODABLE`
  // (bit 0, decoded field `independently_decodable`) is the wire bit MH reads to
  // decide when this fires. Policy lives here; the bit lives there; they are one
  // concept in two encodings. Unguarded anchor comment, consistent with the five
  // in signaling.proto — see docs/TODO.md §Media Path Obligations item (d).
  bool supersede_on_independent_frame = 5;

  // How MH carries this egress stream to its subscriber (ADR-0036 §1, §5).
  //
  // The signalling enum, imported — NOT a parallel `internal.v1` copy. Divergent
  // numbering between a client-facing and a server-facing spelling of one
  // concept would make MC and MH silently disagree on mode while the §8 echo
  // check below reads green.
  //
  // `TRANSPORT_MODE_UNSPECIFIED` IS A REJECTION, NOT A DEFAULT. MH MUST reject
  // an egress stream that does not name a mode; it MUST NOT fall back to
  // datagram. The enum's own definition is fail-closed for the same reason, and
  // this is the reciprocal of the response echo's "UNSPECIFIED is a mismatch,
  // not a skip" rule below — one hazard, both directions.
  dark_tower.signaling.v1.TransportMode transport_mode = 6;
}

// Meeting-level rules governing selection among an egress stream's candidate
// sources (ADR-0036 §7).
//
// DELIBERATELY EMPTY AT THIS STORY, and deliberately a NAMED MESSAGE rather
// than `bytes`, `google.protobuf.Any`, or `map<string, string>`. A type-blind
// blob on the control plane is unvalidatable and defeats §6
// make-conflicts-unrepresentable; an empty named message is additive and
// validatable. Presence of the field distinguishes "MC declared no rules" from
// "MC did not speak about rules".
//
// MEETING-LEVEL, NOT PER-EGRESS, ON PURPOSE. Ranking, debounce, churn rate
// limits and per-subscriber exclusions (§7) are properties of the selection
// POOL, not of one egress edge. Folding a ranking field into `EgressStream`
// would make the meeting-wide rule N per-edge copies that can disagree.
message SelectionRules {}

// Request to register a meeting with a media handler (MC->MH).
//
// THIS MESSAGE IS THE CONTROL PLANE (ADR-0036 §8). It is a FULL SNAPSHOT, not a
// delta: MH replaces its policy for this meeting wholesale, which is what makes
// a re-assert idempotent and what makes restart recovery a re-send rather than
// a reconciliation.
//
// NO KEY MATERIAL CROSSES THIS CONTRACT (ADR-0036 §4, Appendix). No KEK, no
// transmit key, no key id, no wrapped material, ever. MH is keyless, and that
// is a structural guard rather than a convention. The `connection_token` this
// reshape deletes was the only credential this file ever carried; nothing
// replaces it.
message RegisterMeetingRequest {
  string meeting_id = 1;
  string mc_id = 2;
  string mc_grpc_endpoint = 3;

  // The complete forwarding policy for this meeting on this handler.
  //
  // AN EMPTY SET IS MEANINGFUL AND LEGAL: "this handler forwards nothing for
  // this meeting". It is a computed output of MC's assignment, never an
  // operator lever.
  //
  // BOUND: see `EgressStream.candidate_sources`. Enforcement owner: story task 5.
  repeated EgressStream egress_streams = 4;

  SelectionRules selection_rules = 5;

  // Monotonic per (meeting, handler), DERIVED FROM THE ASSIGNMENT
  // COMPUTATION'S OUTPUT CHANGE, NEVER FREE-RUNNING (ADR-0036 §8). Unchanged
  // policy carries the SAME number so MH no-ops; that property is what makes
  // the §8 acknowledgement silent in steady state.
  //
  // ZERO IS NOT A VALID GENERATION. Valid generations start at 1. proto3
  // scalars have no presence, so an old peer that has never heard of this field
  // is indistinguishable from a new peer sending 0 — and on the response side
  // that would reproduce exactly the partial-blackhole-reporting-healthy bug
  // `applied_generation` exists to catch, on the ordinary rolling-deploy path.
  // MH MUST reject a registration carrying `policy_generation` 0.
  //
  // *** ORDERING CONSTRAINT — READ THIS BEFORE IMPLEMENTING THE REJECTION. ***
  // MH's rejection of `policy_generation` 0 MUST NOT BE ENFORCED UNTIL MC
  // EMITS `policy_generation` >= 1. Enforcement lands WITH or AFTER the MC change, NEVER
  // BEFORE IT.
  //
  // Why this sentence exists: at the commit that introduced this field, MC's
  // only caller (`crates/mc-service/src/grpc/mh_client.rs`) sends
  // `policy_generation` 0
  // with an empty policy, and MC does not send real generations until story
  // task 6. MH implements its side at story task 5. Enforcing the rejection at
  // task 5 would therefore reject EVERY registration MC sends for the whole of
  // the task-5..task-6 window: no meeting is ever registered, every client is
  // provisionally accepted and kicked at MH_REGISTER_MEETING_TIMEOUT_SECONDS,
  // and that is ADR-0036 §8's opening paragraph almost verbatim — "a permanent
  // media blackhole for that meeting until it emptied, reached through an
  // ordinary rolling deploy." Enforcing this MUST out of order REINTRODUCES THE
  // EXACT FAILURE §8 EXISTS TO ELIMINATE, through the field added to prevent it.
  //
  // The obvious workaround — have MC send `policy_generation` 1 now — is WRONG
  // and is recorded here so it is not rediscovered as a fix. §8 has MH no-op on
  // an unchanged `policy_generation`. An MC shipping `policy_generation` 1 with
  // an EMPTY policy makes MH apply "policy_generation 1 = no edges"; task 6's
  // first REAL assignment is also naturally `policy_generation` 1 (the value
  // derives from assignment-output change, and that is the first output), so MH
  // sees a value it has already applied and NO-OPS THE REAL POLICY. That trades a loud outage for a silent
  // one, which is the worse trade.
  //
  // MH MUST IGNORE A REGISTRATION WHOSE GENERATION IS LOWER THAN THE ONE IT HAS
  // APPLIED — reordered or retried delivery must not roll policy back.
  //
  // NOT A SECURITY TOKEN. Not a nonce, not an anti-replay control, and MUST NOT
  // be used as one. This channel's authenticity comes from the existing OAuth
  // Bearer + JWKS `service.write.mh` gate, not from this field.
  //
  // THREE DIFFERENT "generation" CONCEPTS EXIST IN THIS TREE; DO NOT COLLAPSE
  // THEM.
  //   (a) `signaling.v1.JoinResponse.kek_generation` — KEK rotation counter.
  //   (b) MC's Redis FENCING generation (`meeting:{id}:generation`,
  //       `crates/mc-service/src/redis/client.rs`) — advances on every fenced
  //       write, including writes that do not change forwarding output.
  //       BOTH ENDS OF THAT API ARE TRAPS, and the READ path is the likelier
  //       collapse: `get_generation(meeting_id) -> u64` is what an implementer
  //       reaches for when they want "the current generation for this meeting".
  //       It type-checks, compiles, and PASSES EVERY LOOPBACK TEST — the fencing
  //       counter is monotonic and non-zero, so even the §8 applied-generation
  //       echo check goes green. `increment_generation()` is the same trap from
  //       the write side. Using either BREAKS §8's "unchanged policy carries the
  //       same number and MH no-ops" property and turns every fenced write into
  //       a spurious policy re-apply.
  //   (c) this field.
  //
  // THE FIELD IS NAMED `policy_generation`, NOT `generation`, FOR THAT REASON.
  // ADR-0036 §8 always qualifies the noun — "derived from the assignment
  // computation's output change" — and the qualified name is what makes the
  // collapse resistible AT THE VALUE-SOURCE SITE IN MC, where this comment is
  // not in view and `get_generation()` is one autocomplete away. A bare
  // `generation` field is an invitation addressed to the one place the warning
  // cannot reach.
  //
  // OBSERVABILITY (ADR-0036 §11): NOT LABEL MATERIAL. A generation value is
  // unbounded and would be a new series per policy change. A mismatch is
  // expressed as a BOUNDED OUTCOME label
  // (`match` | `generation_mismatch` | `no_applied_generation` |
  // `transport_mode_mismatch` | `handler_id_mismatch`); the numbers themselves
  // belong in the log line and the gauge VALUE, never in a label.
  //
  // The set is FIVE, and each value exists because some field on the response
  // can fail in a way MC must act on: `no_applied_generation` is the
  // pre-contract-handler case, and its CONSTRUCTIBILITY is the entire reason
  // zero is reserved on `applied_generation` — if 0 were legal the state would
  // have no name and would silently fold into `match`. `handler_id_mismatch`
  // covers the `handler_id` comparison below. Every fail-loud path in this
  // response has a bounded label to land in; a comparison specified with no
  // observable is the "declared in one place, assumed in another, verified
  // nowhere" precedent §8 cites.
  uint64 policy_generation = 6;
}

// Response to meeting registration (ADR-0036 §8).
message RegisterMeetingResponse {
  // Received and parsed. THAT IS ALL IT MEANS, and it is not evidence of
  // application: MH's handler hands policy to the session actor over a bounded
  // mailbox and returns, so the apply is asynchronous relative to this response.
  // MC MUST NOT treat `accepted == true` as success. Only
  // `applied_generation == the generation MC sent` is success.
  bool accepted = 1;

  // The applied counterpart of `RegisterMeetingRequest.policy_generation`: the
  // generation MH's LIVE FORWARD PATH ACTUALLY REFLECTS — never the highest
  // received, never an echo of the request (ADR-0036 §8).
  //
  // Named `applied_generation`, not `applied_policy_generation`, and the
  // asymmetry with the request field is deliberate rather than an oversight.
  // The `policy_` qualifier exists to stop an MC implementer sourcing the value
  // from Redis's fencing counter; that hazard is MC-side and request-side only.
  // MH produces this value from its own applied state and has no fencing
  // counter to confuse it with, so the qualifier would buy nothing here — and
  // `applied_` is itself already a qualifier that makes the wrong source
  // unreachable: there is no "applied" reading of a fencing counter.
  //
  // ECHOING ON RECEIPT REPRODUCES THE EXACT BUG THIS FIELD EXISTS TO CATCH:
  // MC sends 7, MH enqueues and returns success, the mailbox is full or the
  // apply errors, MC believes MH runs 7 while MH runs 4 — a partial blackhole
  // with every liveness signal green. An implementer who writes
  // `applied_generation: req.generation` has deleted the feature while leaving
  // the field.
  //
  // "REFLECTS" MEANS THE COMPLETE SNAPSHOT FOR THIS GENERATION IS INSTALLED.
  // Apply is all-edges-or-none: any per-edge failure fails the whole apply and
  // leaves the PRIOR generation live. A partial install is not a representable
  // state, so this field is never a partial reflection. Stated rather than left
  // to inference — a strict reading of "actually reflects" already excludes a
  // partial install (a partial reflection is not a reflection of N), but an
  // inferred invariant is the kind that decays, and the failure it prevents is
  // a forward path reflecting NEITHER N nor N-1 while the echo claims N.
  //
  // ZERO MEANS "NOTHING APPLIED" and is a legal, meaningful value: a handler
  // that has never successfully applied any policy for this meeting reports 0.
  // It is therefore ALSO what a pre-reshape MH's absent field decodes to, and
  // the two readings agree — in both cases MC's correct action is identical
  // (do not believe policy is live; re-assert). Valid applied generations start
  // at 1, matching `RegisterMeetingRequest.policy_generation`.
  //
  // OBSERVABILITY: not label material — see
  // `RegisterMeetingRequest.policy_generation`.
  // MC feeds its divergence gauge from THIS value, never from the sent value.
  uint64 applied_generation = 2;

  // The handler answering this call, as IT identifies itself.
  //
  // Spelling: `handler_id`, matching `RegisterMHRequest`,
  // `SendLoadReportRequest` and the two `NotifyParticipant*` messages. NOT
  // `mh_id` (the `MhAssignment` spelling) — one concept should not gain a third
  // home.
  //
  // MH-ASSERTED, NOT MH-PROVEN. It carries no trust beyond the existing
  // authenticated-service trust on this channel. MC MUST compare it against the
  // `MhAssignment.mh_id` of the handler it dialed and FAIL LOUD on mismatch —
  // the same two-ends-must-agree treatment §8 gives transport mode. That catches
  // a misrouted or misconfigured handler registering as another.
  //
  // OBSERVABILITY: acceptable as a metric label — pod/handler level IS §11's
  // stated aggregation floor.
  string handler_id = 3;

  // Unix epoch milliseconds at which THIS MH PROCESS started, so MC can detect a
  // restart (ADR-0036 §8).
  //
  // PER-PROCESS, NOT PER-POD-NAME AND NOT PER-CONFIG. MH MUST sample this ONCE
  // at process start and return the same value for the process's whole life. A
  // crash-restart into the same pod identity must change it, or the restart
  // detector never fires for the case it exists to catch.
  //
  // COMPARED FOR INEQUALITY ONLY — it is a process-incarnation token, not a
  // clock reading MC does arithmetic on. Wall-clock skew between handlers is
  // therefore irrelevant.
  //
  // ZERO MEANS "NOT REPORTED", NOT "EPOCH ZERO". A pre-reshape MH's absent field
  // decodes to 0; MC MUST treat 0 as "restart undetectable" and never as a
  // stable incarnation, or every old handler looks like one that has never
  // restarted.
  //
  // OBSERVABILITY: not label material — unbounded, one new series per restart.
  uint64 process_start_epoch_ms = 4;

  // The transport mode MH APPLIED to every egress stream of
  // `applied_generation` — ADR-0036 §8's two-ends-must-agree echo. MC compares
  // it against what it declared and fails loud on mismatch. The cautionary
  // precedent §8 cites is a capacity value advertised to GC and enforced
  // nowhere: declared in one place, assumed in another, verified nowhere.
  //
  // AN UNSPECIFIED ECHO IS A MISMATCH, NOT A SKIP. This sentence constrains the
  // READER of this field, in the same register as
  // `signaling.v1.JoinResponse.sender_id`'s comment, because the proto cannot
  // enforce it. A pre-reshape MH echoes `TRANSPORT_MODE_UNSPECIFIED` (proto3
  // absent enum = zero), and the natural defensive implementation —
  // `if echoed != UNSPECIFIED { compare }` — FAILS OPEN against precisely the
  // peer population this check exists to police. That form survives code review
  // as defensiveness, and its skip-branch is unreachable in any test written
  // against a same-version peer, so it is invisible to the test suite
  // permanently. MC MUST compare `sent != echoed` unconditionally.
  //
  // SCOPE LIMIT, STATED SO IT CANNOT DEGRADE SILENTLY. This is ONE field
  // echoing a policy that is PER-EGRESS-STREAM in the request. It is
  // unambiguous only while a meeting's egress streams are homogeneous in
  // transport mode, which is true for this story (one audio egress stream,
  // datagram) and stops being true when video lands (§1: audio on datagrams,
  // video on stream-per-group).
  //
  // Therefore: MH MUST REJECT a registration whose egress streams do not all
  // carry the same transport mode, rather than pick one and echo it. Echoing
  // one of N would make the check silently meaningless — the precise failure
  // §8 warns about. When heterogeneous modes are required, this field moves
  // per-egress as a deliberate contract change, which the reject forces to
  // happen loudly.
  //
  // THIS SCOPE LIMIT EXPIRES AT STORY 3 (video), NAMED SO IT CANNOT BECOME
  // PERMANENT: a scope limit with no named expiry is how a temporary shape
  // becomes the shape. Enforcement owner: story task 5. Recorded in
  // docs/TODO.md as debt row 4 with story 3 as the trigger.
  dark_tower.signaling.v1.TransportMode transport_mode = 5;
}
```

Plus the `MediaHandlerService` block reduced to:

```proto
// Media handler service (Meeting Controller -> Media Handler).
//
// ONE RPC BY DESIGN. Registration IS the control plane (ADR-0036 §8): it gains
// fields rather than sibling RPCs. Three RPCs were retired on 2026-09-01 — see
// the tombstone above `SubscriberSlot`.
service MediaHandlerService {
  rpc RegisterMeeting(RegisterMeetingRequest) returns (RegisterMeetingResponse);
}
```

### 3. Field-tag map (@operations OPS-2)

**No tag is repurposed, and no tag is vacated inside a surviving message — so no `reserved` declaration is required or possible in either message.** Stated explicitly because CONVENTIONS §5's whole point is that a reader must be able to tell an unreserved tag from one nobody looked at.

| Message | Tag | Before | After | Verdict |
|---|---|---|---|---|
| `RegisterMeetingRequest` | 1 | `string meeting_id` | `string meeting_id` | unchanged |
| | 2 | `string mc_id` | `string mc_id` | unchanged |
| | 3 | `string mc_grpc_endpoint` | `string mc_grpc_endpoint` | unchanged |
| | 4 | — | `repeated EgressStream egress_streams` | **fresh** |
| | 5 | — | `SelectionRules selection_rules` | **fresh** |
| | 6 | — | `uint64 policy_generation` | **fresh** |
| `RegisterMeetingResponse` | 1 | `bool accepted` | `bool accepted` | unchanged, name + semantics preserved |
| | 2 | — | `uint64 applied_generation` | **fresh** |
| | 3 | — | `string handler_id` | **fresh** |
| | 4 | — | `uint64 process_start_epoch_ms` | **fresh** |
| | 5 | — | `TransportMode transport_mode` | **fresh** |

The hazard OPS-2 names is real and specifically avoided: `accepted` (bool, varint) and `applied_generation` (uint64, varint) are both varint, so putting `applied_generation` on tag 1 would have made an old MH's `accepted: true` decode as `applied_generation: 1` — well-formed, plausible, wrong. It is on tag 2.

For the **deleted whole messages**, proto3 offers no reservation mechanism (`reserved` exists only for field numbers/names inside a message and value numbers inside an enum; there is no message-name or service-level `reserved`). The equivalent artifact is the tombstone comment block above, which names every retired message and RPC method. `buf breaking` FILE mode is the mechanical detector for their deletion, and it fires — see §Expected Layer-State.

### 4. Scoping resolution — the Gate 2 tension (@operations OPS-1)

**Resolution: option (a).** The proto reshape AND the minimum consuming-code edits required to keep the workspace compiling land in THIS commit, classified cross-boundary per ADR-0024 §6.2/§6.3 (rows 3-12 above), owner confirmation sought at Gate 1 and Gate 3 via @team-lead.

Why there is no alternative:
- Layer 1 runs `cargo build --workspace`; Layer 2 runs `cargo test`; **Layer 5 runs `cargo clippy --workspace --all-targets -- -D warnings`** (`scripts/lang/rust/lint.sh:7`). So test targets must compile too. There is no configuration of "proto only" that leaves the pipeline green.
- Deleting three RPCs removes three methods from the `MediaHandlerService` trait. Every `#[tonic::async_trait] impl MediaHandlerService` with those methods becomes a hard compile error (extra trait items), and every `use` of the deleted types becomes an unresolved import. That is 1 production impl (`mh_service.rs`) + 2 test mocks (`mc-service/tests/*`).
- Adding three fields to `RegisterMeetingRequest` and four to `RegisterMeetingResponse` breaks every prost struct literal: 6 request sites and 3 response sites, enumerated in rows 3, 6-8, 10-12.

I explicitly reject the masking routes CLAUDE.md forbids and @operations/@security named: no `N/A` layer marks, no crate excluded from the workspace, no `#[cfg]`-ing tests out, no `#[allow]`, no `..Default::default()` elision (which would silently absorb future field additions — the new fields are written out explicitly so the next field addition fails loudly at each site), and **no widening of `proto/buf.yaml`'s `breaking.ignore`** (see §Expected Layer-State).

**The live contradiction, named rather than routed around (@operations OPS-1):** my task text says the MC mocks are tasks #5/#6's; task #5's own prompt says MC's two mocks are updated "in this same task" (task 5); task #6's prompt says the same two files are updated "in this same task" (task 6). Three prompts claim the same two files. Compilation forces the answer: they land here, and tasks #5/#6 will find them already done. Flagged to @team-lead as OQ-0b so the downstream prompts are not run against a stale premise.

**What the transitional MH values are, and why they are truthful rather than placeholders.** MH genuinely applies no policy today (that is task #5). So:
- `applied_generation: 0` — MH's live forward path reflects generation 0, i.e. nothing. This is *correct*, not a placeholder. **`applied_generation: req.generation` is specifically NOT written**, even transitionally: that single line is the §8 bug, and a transitional lie here is the one that would survive into task #5 unnoticed.
- `handler_id: String::new()` — `MhMediaService` holds only a `SessionManagerHandle`; it has no handler id to report. Plumbing config into the service is task #5's work. Empty = "not reported", matching the field's documented reading.
- `process_start_epoch_ms: 0` — MH has no process-start instant recorded. 0 = "not reported", per the field comment.
- `transport_mode: TRANSPORT_MODE_UNSPECIFIED` — MH applied no mode. Fail-closed zero, per the enum's own contract.
Each gets a `TODO(story task 5, media-handler):` comment naming the owner. No MC code reads any of them yet (task #6 adds the check), so no false alarm is produced today.

### 5. Expected Layer-State (CONVENTIONS.md §Intentional wire breaks; @security S12, @operations OPS-5)

**Route taken: the intentional-wire-break escalation, exactly as task 3 used. `proto/buf.yaml` is NOT touched.**

I will not add `dark_tower/internal/v1/internal.proto` — or a blanket `dark_tower` — to `breaking.ignore`. Three reasons, all of which I endorse: it would silence the MC↔MH contract for story tasks 8-10 as well as this one with no way to narrow it later (buf has no per-finding acceptance); `proto/buf.yaml` is itself inside a Guarded Shared Area and editing the gate's configuration to pass one's own change is the masking shape CLAUDE.md forbids; and the buf.yaml comment block already records the measurement that internal.proto has zero findings today, which widening would falsify in a file that also claims tasks 4 and 8-10 "stay under full enforcement".

**Predicted Layer 6 findings, declared BEFORE implementing, by rule class:**

| Rule class | Count | Symbols |
|---|---|---|
| `RPC_NO_DELETE` | 3 | `MediaHandlerService.Register`, `.RouteMedia`, `.StreamTelemetry` |
| `MESSAGE_NO_DELETE` | 8 | `RegisterRequest`, `RegisterResponse`, `RoutingOptions`, `CascadeDestination`, `RouteMediaRequest`, `RouteMediaResponse`, `StreamTelemetryRequest`, `StreamTelemetryResponse` |
| **Total** | **11** | all in `dark_tower/internal/v1/internal.proto` |

Predicted to NOT fire: no `FIELD_*` class (every new field takes a fresh tag; no field is deleted, renamed, retyped or re-pointed), no `FILE_NO_DELETE` / `PACKAGE_NO_DELETE`, no `ENUM_*` (no enum is touched), nothing in `signaling.proto`.

**Independent corroboration, obtained BEFORE implementation and set-identical to the prediction above.**
@observability measured the set rather than reasoning about it: copied `proto/` to a scratch directory,
applied the three RPC and eight message deletions, and ran `buf breaking` against
`audio_video_user_story`. Baseline on the current tree: zero findings. After the reshape: exactly the 11
above — 8 `MESSAGE_NO_DELETE` at `internal.proto:1:1` (`CascadeDestination`, `RegisterRequest`,
`RegisterResponse`, `RouteMediaRequest`, `RouteMediaResponse`, `RoutingOptions`,
`StreamTelemetryRequest`, `StreamTelemetryResponse`) and 3 `RPC_NO_DELETE` at `internal.proto:79:1`
(`Register`, `RouteMedia`, `StreamTelemetry`). **The additive field work on
`RegisterMeetingRequest`/`Response` contributes zero findings, confirming the "no `FIELD_*` class" half of
the prediction empirically rather than by argument.** Two independent derivations agreeing before the code
exists is what turns the Gate-2 comparison into a check rather than a rubber stamp.

Also expected: `scripts/lang/proto/breaking.sh` prints `SUPPRESSED=dark_tower/signaling/v1/signaling.proto` on every run — the pre-existing task-3 carve-out, unchanged by me.

**Route explicitly NOT taken, and why, since it is the cheapest-looking one at Layer 6.** A second
narrowly-scoped `breaking.ignore` entry for `internal.proto` — even one with the same comment discipline
and a RESTORE trigger — is still available to me and I am declining it. Not because it is dishonest in
form, but because of *when*: buf has no per-finding acceptance, so the entry cannot be scoped to these 11.
It would blind the MC↔MH contract to **every** break, including unintended ones, for story tasks 8, 9 and
10 as well as this one — the exact cost the existing carve-out records for `signaling.proto` ("inside
signaling.proto NO break of any kind is caught while this is present"), extended over the contract during
the three tasks that reshape it. The measured, closed, 11-item set is small enough to enumerate in an
escalation, which is the whole reason the escalation route is affordable here and was affordable for
story 1's 54.

The real log will be diffed **class by class and symbol by symbol** against this table and the result recorded in §Devloop Verification Steps. **Any unpredicted finding is a regression on its merits, not a rounding error** — most importantly, an unpredicted `FIELD_*` finding would be the mechanical detection of the OPS-2 tag-reuse hazard, which is the second reason not to touch `breaking.ignore`.

**Escalation, flagged now rather than at Gate 2:** this is a headless run. Per CONVENTIONS.md, the Lead may **not** accept this red itself — it escalates and a human decides. @team-lead: Layer 6 will be RED at Gate 2 by design; the declaration above is the artifact to check the log against. Raised as OQ-0c.

All other layers are expected GREEN. In particular Layer 5 `buf lint` STANDARD: message names PascalCase, field names lower_snake_case, one distinct response type per RPC (only `RegisterMeeting` survives on this service), package retains its `v1` suffix, and no `// buf:lint:ignore` annotation is added.

### 6. Version-skew matrix (@operations OPS-4) — confirming their reading

| Direction | Surface | Behaviour | Loud or silent |
|---|---|---|---|
| old MC → new MH | `Register`, `RouteMedia`, `StreamTelemetry` | `UNIMPLEMENTED` | **Blast radius zero**: the audit evidence in §1 is that no MC ever calls them. The evidence IS the deploy-safety argument. |
| new MC → old MH | ditto | n/a — new MC never calls them either | zero |
| new MC → old MH | `RegisterMeeting` | policy fields silently dropped by the old decoder; `accepted: true`; `applied_generation` absent → decodes 0 | **Loud in CONTRACT from this commit; loud in CODE from story task 6.** 0 ≠ the generation MC sent, so MC's §8 comparison fails and re-asserts — but MC's comparison *is* task 6, so at this commit the contract says loud and the implementation is silent. |
| old MC → new MH | `RegisterMeeting` | registration arrives with empty `egress_streams`, `policy_generation` 0 | **Loud in CONTRACT from this commit; loud in CODE from story task 5 — AND SUBJECT TO THE ORDERING CONSTRAINT BELOW.** Enforcement is task 5's, and it must not precede task 6. See the OPS-11 row. |
| **our own MC → task-5 MH (OPS-11)** | `RegisterMeeting` | MC still sends `policy_generation: 0` for the whole task-5..task-6 window | **Loud AND an outage, not loud and safe.** If task 5 enforces the generation-0 rejection before task 6 lands, every registration is rejected, no meeting registers, and every client is kicked at the provisional timeout. The ordering MUST in the `policy_generation` field comment is what prevents it. **This row is why the matrix distinguishes loud-and-safe from loud-and-outage — conflating them tells a task-5 reader the rejection is the control working.** |

**A matrix that states contract-intent as though it were live behaviour is the "declared in one place,
assumed in another, verified nowhere" precedent §8 cites, applied to our own writeup** — hence the
per-row "in contract from / in code from" split above (@operations).

**Real blast radius at THIS commit: the compile surface, not the cluster.** Nothing deployed changes; MH has no policy-driven forward path yet (tasks #5/#6) and MC pushes an empty policy with `policy_generation` 0. The skew window opens at tasks #5/#6 — stated plainly so a reader at task #6 does not inherit a rollback section written for a proto-only commit.

### 7. Recorded debt (`docs/TODO.md`, row 20)

Three entries, each with its owner named:
1. **Proto secret-scanner gap** (@security S10, escalated to S15). ADR-0036's Appendix asserts "the credential-leak guard covers KEK and transmit-key material in internal messages and logs". @security enumerated (not sampled) every credential/PII module in `dt-guard` and every one is extension-scoped to `.rs` or `.ts`/`.tsx`/`.svelte`: `rust_secrets.rs:38`, `rust_log_secrets.rs:41`, `rust_pii.rs:34`, `instrument_skip_all.rs:40`, `ts_secrets.rs:40`, `ts_pii.rs:31`, `ts_retained_credentials.rs:119`, `ts_metric_naming.rs:32`. `.proto` appears in the guard crate only in `cite_extract.rs`, `knowledge_index.rs` and `gsa_sync.rs` — citation resolution, index scope, GSA path list, none of them scanners. The semantic layer is not a fallback either: `scripts/guards/semantic/checks.md` scopes its Credential Leak check to Rust (items 1-4) and TS (items 5-10) in its own preamble. **So the sentence is true for logs and false for internal messages.**

   **I take @security's resolution (b), both halves.** This is row one of ADR-0036's own taxonomy at line 1158 — *"A control can be alive and out of scope, or in scope and dead. Both are silent, and both read as coverage — which is worse than an absent control, because an absence gets noticed"* — and a tracked TODO does not make a false sentence true. So: (i) **amend the Appendix sentence in place** to state the actual coverage (row 19b), using the in-place-correction-with-attribution pattern `docs/protocol/CONVENTIONS.md` established on 2026-08-31 rather than a silent edit, because a reader who trusted the old sentence would have misread a green gate; and (ii) record the scanner itself as follow-up here. Building the scanner is a `dt-guard` change with its own test surface, is not protocol's lane, and would be scope creep on a contract reshape. Owner: security + infrastructure.

   **Compensating control, stated as the weaker form it is**: no key-shaped field exists anywhere in the reshaped message set (§10), and @security's S1 revocation trigger #2 makes reintroducing one a Gate-3 failure. That is reviewer-enforced, not structural — the same ADR section says to prefer structural impossibility over a control that has to notice — but it is at least *in scope*, which the guard is not.
2. **`webtransport_endpoint` vs `media_handler_url`** (@dry-reviewer D5). One concept, two spellings: `signaling.v1` says `media_handler_url` throughout; `internal.v1` says `webtransport_endpoint` (`MhAssignment`, `RegisterMHRequest`, `RegisterMCRequest`); the join is `crates/mc-service/src/webtransport/connection.rs:999`. **Not aligned in this task** — a rename spans the GC↔MC↔MH wire contract and three services' consuming code, which is a task-sized GSA change, not a field edit. This reshape deletes `RegisterResponse.media_handler_url`, the last `media_handler_url` spelling in `internal.v1`, so after this commit the two files share **zero** spelling for one concept. Recorded as a boundary with owner: protocol, next `internal.proto` reshape (story task 8-10 window). **Not dropped a second time.** **Filed in `docs/TODO.md` §Cross-Service Duplication → §From DRY Reviewer (Ongoing)** — corrected there at Gate 3 on @dry-reviewer's finding: I had originally filed it under §Media Path Obligations, which self-describes as constraints the `media-protocol` codec cannot enforce. This is neither a codec constraint nor in that crate's rustdoc; it is a cross-service naming divergence. The cost was concrete rather than bookkeeping — §Cross-Service Duplication is the section a DRY reviewer sweeps at the start of a review, so an entry parked in a media-codec section would have been invisible at exactly the trigger window the entry itself names.
3. **The §8 applied-echo gap in TWO sibling RPCs** — `RegisterMCResponse` and `RegisterMHResponse` (see §0; `AssignMeetingWithMhResponse` was withdrawn after @dry-reviewer checked the apply path and found it synchronous). Owner: protocol + global-controller for the proto half; mc/mh for the Rust half.

   **Attached to the EXISTING `GcClient` duplication item, not filed as an orphan entry** (@dry-reviewer). The two siblings are not merely a repeated proto shape: `crates/mc-service/src/grpc/gc_client.rs` and `crates/mh-service/src/grpc/gc_client.rs` are already on record in `docs/specialist-knowledge/dry-reviewer/INDEX.md` and `docs/TODO.md` as near-duplicates sharing `add_auth`, and the interval-application idiom (`if resp.X > 0 { atomic.store(X) }`, three instances across the two files) is that same clone pair drifting further apart. Describing one pair in two TODO entries is the failure mode the section exists to prevent.
4. **Transport-mode echo homogeneity** — the `RegisterMeetingResponse.transport_mode` scope limit becomes a real constraint when video lands (story 3); the field must move per-egress then. Owner: protocol, story 3.

### 8. Parked obligations (stated so they are visibly parked, not silently missed)

- **`switch_command_id` uint64 mandate (@dry-reviewer D4).** `signaling.proto:StreamAssignment.switch_command_id` states the MC→MH internal contract MUST use `uint64` for the same identifier. **Out of scope here**: the ADR Appendix's slot-state notification (§6 states + §7 switch-completion reports keyed by command id, debounced by MH) is an MH→MC message that this task's prompt does not ask for and that has no consumer this story. When it lands, `uint64`. Parked, not missed.
- **`uint32 max_streams`** on `RegisterMHRequest` — untouched, per the task. The egress-budget chain is story 2.
- **`server_muted`** — deliberately NOT landed. It is a §7 source/ingress property, not an egress attribute; it is unconsumed this story; and it is additive later on the source side. Landing it on `EgressStream` would put a per-source property on a per-edge message, which is the shape §7 warns against.
- **Periodic re-assert cadence**, connectivity-loss trigger, dispatch jitter, cadence-vs-provisional-timeout startup validation, re-assert-failure paging — story 4. Tracked at `docs/user-stories/2026-08-27-hear-yourself-through-handler.md:81` and design-decision row 7; @operations confirmed the tracking is real, so no new TODO entry. The shape landing here **does** support restart detection end-to-end: `process_start_epoch_ms` is documented per-process (sampled once at process start), so a crash-restart into the same pod identity changes it.
- **Handoff to story task 5 — the full stale-label surface** (from @observability, verified). "The retired gRPC method labels" under-scopes to the emission sites; the complete set is: `mh_service.rs` emission sites; `metrics.rs:12` module cardinality doc *(I fix)*; `metrics.rs:134` fn doc *(I fix)*; `metrics.rs:323-326` test enumeration *(task 5)*; `metrics.rs:402-403` `test_cardinality_bounds` `valid_methods` allowlist — **the only executable pin** *(task 5)*; `metrics.rs:464-465` *(task 5)*; `tests/errors_grpc_metrics_integration.rs:16-17,41-47,61-62,71-72` ADR-0032 label-coverage pins *(task 5)*; `grpc/mod.rs:6,14` and `lib.rs:20` fences *(I fix)*; `docs/observability/metrics/mh-service.md:218,221` *(I fix)*; `mh-overview.json:1676` *(I fix)*; `mh-alerts.yaml:16` *(I fix)*. Task 5's right shape, per @observability: reduce seven prose homes to the one executable pin and have the prose defer to the proto — `docs/observability/label-taxonomy.md:70` already declares gRPC `method` values "bounded by `.proto`", so the SSoT is this file.
- **Handoff to story task 6 — four MC-side READER obligations, and the negative test that catches the one that cannot otherwise be caught (@meeting-controller F2).** The response-field comments state the obligations, but a comment is not a control and the `transport_mode` comment itself records that the fail-open skip-branch is *invisible to the test suite permanently* against a same-version peer. So task 6 must carry: (1) **an UNSPECIFIED-echo negative test** — a same-version peer echoes `TRANSPORT_MODE_UNSPECIFIED` and MC must report MISMATCH, not skip. This is the only thing that catches `if echoed != UNSPECIFIED { compare }`. Plus the three siblings: (2) `applied_generation` absent/0 → re-assert, never success; (3) `applied_generation` < sent → re-assert; (4) `handler_id` != the dialed `MhAssignment.mh_id` → fail loud; and epoch 0 → restart-undetectable, never a stable incarnation. Recorded in `docs/TODO.md` §Media Path Obligations. **Documenting an obligation where it is read, without owning the test that catches the fail-open, leaves exactly the gap §8 exists to close.**
- **Mock response literals are truthful zeros, confirmed (@meeting-controller F3).** Rows 11/12's `RegisterMeetingResponse` literals use `applied_generation: 0, handler_id: String::new(), process_start_epoch_ms: 0, transport_mode: TRANSPORT_MODE_UNSPECIFIED`. They will **not** set `applied_generation: req.generation`. It would be inert today (`mh_client` reads only `accepted`), and that is precisely the danger: a fixture that pre-bakes the echo-a-received-value lie hands task 6 a green success-test validating the exact §8 anti-pattern. Truthful zeros force task 6 to make the mock echo the sent generation *deliberately*, at the moment it adds the check.
- **T2 — `process_start_epoch_ms`'s PRODUCER obligation, split across two stories (@test).** Task 5: **within-process stability** is constructible now — two `register_meeting` calls in one process MUST return the same epoch; that kills the per-call `now()` implementation. Handler-restart story: **changes-on-restart** is the load-bearing half and is not constructible until a second process incarnation exists — test shape stated as incarnation A reports E1, restart into the same pod identity, incarnation B reports E2, E1 ≠ E2 → restart detected. Recorded in `docs/TODO.md`. Without the split, a per-pod-name or per-config epoch passes everything until the exact moment the restart detector is needed.
- **AC-2 — the `sender_id` meeting-scoping MUST needs an executable pin at task 5 (@auth-controller).** The `SubscriberSlot` MUST is carried only by contract text, and the comment itself records that a global-index implementation passes every single-meeting-fixture test. Enforcement and hazard both go live at task 5. Tracked in `docs/TODO.md` with story task 5 as trigger/owner and the acceptance pin stated as an **executable multi-meeting negative test**: a `sender_id` valid in meeting A must be REJECTED when it appears in meeting B's `RegisterMeetingRequest`. A single-meeting fixture cannot distinguish the correct implementation from the cross-tenant-leaking one, so the test shape is part of the requirement, not an implementation detail.
- **OPS-11 ordering — task 5 must NOT enforce the generation-0 rejection before task 6 lands.** Stated in the `policy_generation` field comment where the task-5 implementer reads it, and mirrored as a `TODO(story task 5/6 ordering)` at MC's send site. Repeated here because `main.md` for task 4 is not a document a task-5 implementer is required to read — which is exactly why the constraint lives in the proto and not only here.
- **Stale-suppression check for task 5 (@security S11).** `docs/devloop-outputs/2026-04-01-mh-stub-service/main.md:235` records that the `connection_token` field name once tripped secret detection. `stub_placeholder()` goes dead with `Register`. If a guard suppression was added for it, deleting the field leaves a stale suppression — a silently-masked guard. I delete `stub_placeholder()` in this commit and will grep for a matching suppression; anything I cannot resolve goes into the task-5 handoff rather than being inherited.

### 9. TypeScript bindings (@security S13 — premise corrected)

The TS bindings are **not checked in**: `.gitignore:13` is `packages/sdk-core/src/proto/**/*_pb.ts`, and `git ls-files packages/sdk-core/src/proto` returns nothing. So there is no checked-in artifact to drift. What does exist is the **codegen oracle**, `packages/proto-gen/scripts/verify-codegen.sh`, which regenerates from `proto/` on every run and asserts symbol presence/absence — and it currently asserts `RegisterRequest` is generated (`:87`), which my deletion falsifies. That file is row 2 and it is a hard requirement, not a nicety.

I extend it in the same shape the task-3 reshape established: replace the `RegisterRequest` presence assert with presence asserts on the surviving/new internal symbols (`RegisterMeetingRequest`, `RegisterMeetingResponse`, `EgressStream`, `SubscriberSlot`, `CandidateSource`, `SelectionRules`), and add `assert_not_generated` for all **eight** deleted symbols. Presence greps alone would stay green through a half-done deletion.

### 10. Proto-gen crate

`crates/proto-gen/build.rs` and `src/lib.rs` need **no change**. No new `skip_debug` entry is required because **no new field carries key material, credentials or PII** — the new fields are numeric identifiers, a bool, an enum, and MH's own handler id. This is the affirmative answer to @security S10 and @semantic-guard's credential-leak lens: the deleted `RegisterResponse.connection_token` was the only credential this file ever carried, and nothing on the new shape replaces it under another name.

### 11. Reviewer pre-plan items — disposition index

| Item | Disposition |
|---|---|
| @security S1 / @operations OPS-9 intersection rule | **RESOLVED** — @security has adjudicated and co-signed route (b) (no auth-routing surface moves), with a stated revocation trigger. Determination recorded in §13; auth-controller not required. |
| @security S14 `sender_id` meeting-scoped | Accepted — §2, a MUST on `SubscriberSlot` covering both reference sites, with the cross-meeting-media-crossing reason and the enforcement owner named. **The most consequential comment in the file.** |
| S2 reuse TransportMode | Accepted — §2, both sites, imported |
| S3 bound repeated fields normatively | Accepted — §2, on `candidate_sources` and `egress_streams`, enforcement owner named |
| S4 numeric per-meeting ids | Accepted — all ids are `uint32`; no UUID, no `user_id` |
| S5 named empty `SelectionRules` | Accepted — §2, with the reason recorded in the message comment |
| S6 monotonicity + not-a-security-token | Accepted — §2, `policy_generation` comment |
| S7 echoes MH-asserted | Accepted — §2, `handler_id` comment, incl. compare-against-`MhAssignment.mh_id` |
| S8 reserved hygiene + service comment | Accepted — §3 (no vacated tags; stated explicitly) + tombstone block |
| S9 dangling credential path in docs | Accepted — rows 13-15 |
| S10 no key material + guard coverage | Accepted — §10; gap recorded, §7 item 1 |
| S15 ADR asserts a control that does not exist | Accepted, **resolution (b) — both halves**: the Appendix sentence is amended in place (row 19b) AND the scanner is recorded as follow-up. The ADR text changes either way. |
| S11 stale suppression handoff | Accepted — §8 |
| S12 no `breaking.ignore` widening | Accepted — §5, escalation route with predicted set |
| S13 TS bindings | Premise corrected (gitignored); the real obligation is the codegen oracle — §9, row 2 |
| @auth-controller AC-1 ADR-0003 is the SSoT and still specifies the deleted credential | **Accepted — blocking, and they are right.** Row 12b. Correcting three narrative docs while leaving the *decision of record* specifying `media.publish`/`media.subscribe` connection tokens is the exact single-source-of-truth violation the row-13-15 fixes exist to close, only worse: auth-controller's own INDEX points readers at ADR-0003 first, so a future auth implementer lands on the stale spec before the corrected narrative. Verified in tree: `:106-118`, `:185`, `:315`, `:426`. |
| @dry-reviewer D1 import vocabulary | Accepted — §2 |
| D2 not `MediaStream` | Accepted — §2, `CandidateSource` with the reason inline |
| D3 reference, don't restate | Accepted — §2, `SubscriberSlot` points at the two canonical statements by name |
| D4 `switch_command_id` uint64 | **Parked visibly** — §8 |
| D5 `webtransport_endpoint` spelling | **Recorded as a boundary with a named owner** — §7 item 2. Not renamed here. |
| D6 `handler_id` not `mh_id` | Accepted — §2 |
| D7 three "generation" concepts | Accepted — §2, all three named at the field, incl. the `increment_generation()` trap |
| D8 deletion audit clean | Corroborated and re-run — §1 |
| D9 metrics.rs two-homes drift | Carried into the task-5 handoff — §8 |
| D10 `ANCHOR (DRY)` on supersede | Accepted — §2, anchored to `FLAG_INDEPENDENTLY_DECODABLE`, with the "comment, not control" caveat |
| @observability 1 label collapse | Accepted; the "no guard sees label values" point is why §8 enumerates all 12 surfaces |
| Obs 2 full surface list | Accepted — §8, split into what I fix (prose that goes false at MY commit, since I delete the emission sites) and what task 5 owns (test pins) |
| Obs 3 `mh-alerts.yaml:16` | Accepted — row 18, I fix it here |
| Obs 4(a) §11 comments on the new ids | Accepted — §2, on `SubscriberSlot`, `CandidateSource`, `egress_stream_id` |
| Obs 4(b) generation/epoch not label material | Accepted — §2, both fields, with the bounded-outcome-label alternative named |
| @operations OPS-1 Gate 2 tension | Resolved — §4, option (a), contradiction named |
| OPS-2 tag map | §3 |
| OPS-3 zero invalid | Accepted — §2, on all three of `policy_generation`, `applied_generation`, `process_start_epoch_ms` |
| **OPS-11 generation-0 ordering trap** | **Accepted, resolution (b), and it is the best finding on this task.** The contract as I first drafted it lands a MUST that its only production caller violates on line one, and whose natural implementation point (task 5) is one task *before* the fix (task 6) — reintroducing §8's opening-paragraph blackhole through the field added to prevent it. Landed as an explicit ordering MUST in the `policy_generation` comment, with @operations' refutation of the tempting `policy_generation: 1` workaround recorded inline so it is not rediscovered as a fix. §6 matrix rows now split "loud in contract from" / "loud in code from" and carry the OPS-11 row explicitly. A `TODO(story task 5/6 ordering)` goes at the MC send site so both ends of the window are annotated. |
| **@auth-controller AC-2 sender_id MUST needs a forcing function** | **Accepted.** The MUST is invisible to the default test shape by construction, and enforcement + hazard both go live at task 5, so a comment alone is not a control. `docs/TODO.md` entry (row 20) naming the obligation, story task 5 as trigger/owner, and the acceptance pin as an **executable multi-meeting negative test** — a `sender_id` valid in meeting A must be REJECTED in meeting B's `RegisterMeetingRequest` — anchored to the `SubscriberSlot` comment as normative source. Plus a pointer in §8. |
| **@media-handler / @operations: the third apply state (partial install)** | **Taken, though offered as optional.** A partial install — some edges in, others not, no error surfaced — leaves the forward path reflecting neither N nor N-1 while the echo could claim N. My existing "actually reflects" wording excludes it on a strict reading, which is exactly why I took the clause: an *inferred* invariant is the kind that decays, and §8's posture is prefer-explicit-over-inferred. Now structural: **apply is all-edges-or-none; a partial install is not a representable state.** @media-handler is committing to enforce it in task 5 by construction (validate-all → build-complete-table → single actor state-swap, so the window cannot exist) regardless of the clause; the clause is what makes that a contract rather than a task-5 design note. |
| **@media-handler F1a, and its reconciliation** | **Accepted, then CORRECTED by the owner before it landed wrong — worth recording because I nearly shipped a new bug while fixing an old one.** I first wrote "deferring the apply must not advance `applied_generation`" into `SubscriberSlot`. @media-handler caught that this conflates two states: **(A)** the snapshot was never installed (mailbox full, apply errored) — the echo must not advance, which is the original §8 bug and is *already fully closed* by the `applied_generation` field comment; and **(B)** the snapshot IS installed for generation N but one edge's source is legitimately not yet connected — the policy is faithfully applied, so the echo MUST advance, and per-edge liveness is reported by the §6 slot state. Gating the echo on source connectivity would have pegged MC's §8 comparison into permanent divergence-and-re-assert for any absent source: **a new false-alarm bug in the field that exists to prevent false liveness.** The sentence is removed and replaced with the two-instrument statement — `applied_generation` per-snapshot, §6 slot state per-edge — so the next reader cannot re-derive the wrong version. |
| **@media-handler F1b anchor the cut to §8, not the loopback fixture** | **Accepted.** The comment now grounds the defer in §8's own "drain and promote pending connections" language — a `sender_id` pending promotion within the registered meeting is the case the handler exists to rescue, and a blanket reject would make registration hostile to the mechanism §8 relies on for restart recovery. Loopback is demoted to a parenthetical illustration. That phrasing survives the later "why is there an exception here" question; "loopback breaks" does not. |
| **@media-handler MUST-timing sweep (is there a third?)** | **Accepted and written into the comment as an explicit non-generalisation clause.** Timing-conditioned: exactly two (`sender_id` resolution, gated on pending-promotion; `policy_generation == 0`, gated on MC emitting ≥1 at task 6). Everything else is structural/request-local and stays an immediate hard reject — duplicate `egress_stream_id`, duplicate `(sender_id, slot_id)`, the two count bounds, `TRANSPORT_MODE_UNSPECIFIED`, heterogeneous transport modes, and the cross-meeting half of the `sender_id` check. **The security-load-bearing rejects are in that list and "F1 says defer" must not be read onto them** — so the scoping is stated in the same comment, where the over-read would otherwise happen. |
| **@media-handler F1 reject-vs-defer on unresolved `sender_id`** | **Accepted — this was a real defect in my draft and the corrected text says so in place rather than being quietly reworded.** "MH MUST REJECT if any `sender_id` does not resolve" collides with the pending-connection promotion model the service is built on, and **it would reject loopback's own registration**, because loopback's subscriber is provisionally accepted and promoted *by this very RPC* — so at the instant the registration is processed the referenced connection is mid-promotion. Split: malformed (0 or out of 1..=65535) → REJECT, because it can never become valid; in-meeting but not yet connected → **HOLD**, per the existing promotion model. Safety is unchanged because the *scoping* rule carries the cross-tenant property, not the reject — under meeting-only lookup an unresolved `sender_id` simply forwards nothing. And the wrong version was the dangerous one to leave: it pushes a task-5 implementer toward "relax resolution so the registration succeeds", which is the drift toward a global fallback the rule exists to forbid. |
| **@media-handler F2 (endorses OPS-11)** | Already landed — ordering MUST in the `policy_generation` field comment, per OPS-11. |
| **@media-handler F3 `lib.rs:11-15` stale preamble** | Accepted — row 5 widened from the line-20 fence to the "# Current Status: Stub" preamble, which goes false by the same mechanism the moment the stub handlers are deleted and `register_meeting` becomes the control plane. |
| **@media-handler F4 `mh-alerts.yaml:16` stays in THIS commit** | Accepted, and reinstated on the owner's own request. Three parts: stop citing the dead `method="register"`; stop asserting "covered transitively" as settled, because under §8 a failed RegisterMeeting becomes a media-blackhole trigger once the forward path lands; add a forward-pointer that RegisterMeeting-failure alerting is re-evaluated at task 5/6 (owner media-handler, @operations cross-reviewing). Their framing, which I agree with: **a stale reason-NOT-to-alert on a now-blackhole trigger is the worst 3am artifact in the set.** |
| **@media-handler F5 `metrics.rs:12,134`** | Accepted; the prose/executable-pin split is confirmed by the owner. Note `:12` is **already wrong today** (3 values, omitting `register_meeting`); both become "1 value (`register_meeting`)". |
| **Field naming — SETTLED at implementation (@meeting-controller, final word)** | Request `policy_generation`, response `applied_generation`. I offered `applied_policy_generation` for cross-field symmetry; the owner declined, and the reasoning is recorded at the field: the collapse hazard is request-side and MC-side only, `applied_` already makes the fencing-counter source unreachable (there is no "applied" reading of a fencing counter), and diverging from both downstream prompts' verbatim `applied_generation` would cost churn for no safety. The deliberate-asymmetry note stays at the field so it does not read as an oversight. |
| **@meeting-controller F1 `get_generation()` read-path collapse** | **Accepted, both halves.** Request field renamed `generation` → **`policy_generation`** (option (a)) *and* `get_generation()` added to the named-trap list (option (b)) — F1 is right that the read path is the likelier collapse and that it passes every loopback test, so the name has to do work where the comment cannot reach. Response field stays `applied_generation`: the hazard is MC-side and request-side only, and `applied_` already makes the wrong source unreachable. **@media-handler / @meeting-controller: note the request field is `policy_generation`, not `generation`, so the task-5 and task-6 prompts naming "generation" resolve to it.** |
| **@meeting-controller F2 reader obligations unowned** | Accepted — §8 task-6 handoff, four obligations plus the UNSPECIFIED-echo negative test, recorded in `docs/TODO.md`. |
| **@meeting-controller F3 truthful-zero mocks** | Confirmed — that is exactly the intent; §8. |
| **@observability F-OBS-1 `no_applied_generation` missing from the proto enumeration** | **Accepted.** The document had the right five-value set in §12a and a three-value set in the field comment — and the field comment is the copy that ships and is machine-checkable from `SourceCodeInfo`. Fixed. |
| **@observability F-OBS-2 `handler_id_mismatch` has no observable** | **Accepted, and it is a defect this plan introduced.** I specified a fail-loud `handler_id` comparison and did not give it a bounded label to land in — "declared in one place, assumed in another, verified nowhere", §8's own cautionary sentence, in the fix. Outcome set is now **five**. |
| **@test T1 no roundtrip test for the reshaped contract** | **Accepted, fix-in-this-commit — they are right that my entire §12a argument was prose locked by no test.** The codegen oracle checks TS *symbol presence/absence* only; it does not lock that fields survive encode/decode, nor the presence/zero-value semantics the four-faces analysis rests on. New `crates/proto-gen/tests/internal_roundtrip.rs` (row 2b), sibling to `signaling_roundtrip.rs`, asserting: a fully-populated `EgressStream` roundtrips with edge + sources + behaviours intact (the *constructive* half of §6 — they travel in one message and survive together); empty `egress_streams` is legal and survives empty; **`selection_rules` presence vs absence is distinguishable** (`None` vs `Some(SelectionRules{})`), which my own comment claims and nothing tested; **`applied_generation` roundtrips independently of `policy_generation`** — applied=4 with policy_generation=7 both survive and read back distinct, making "applied ≠ received is representable" executable at the type level; and the all-zero response roundtrips, locking the §12a zero-value contract in code. **Plus @test's optional nicety, taken:** a header block mirroring `signaling_roundtrip.rs`'s "What these tests deliberately do NOT assert", naming the `sender_id` meeting-scoping, the transport-mode echo and the epoch restart semantics as runtime invariants pinned in `docs/TODO.md` / task 5 / the handler-restart story. It makes the gap visible to a reader *of the test file* without adding an inert test, and keeps the two contract-test files parallel in posture. |
| **@test: proof-of-trap placement (rung 3) — resolved, no marker here** | Agreed and settled: a proof-of-trap test belongs in the file that **gains** the distinguishing dimension (task 5's suite for within-process epoch stability; the handler-restart suite for changes-on-restart). An inert marker in `internal_roundtrip.rs`, which can never construct either input, would be **prose masquerading as coverage inside a test file — worse than the `docs/TODO.md` pin, not better**. For a contract-only commit the TODO pins plus the proto MUSTs are the correct home. |
| **@test T2 `process_start_epoch_ms` producer-side obligation is a third vacuously-green case** | **Accepted, folded in now rather than noted.** I had mitigated only the MC-*reader* side. The MH-*producer* obligation — sample once at process start, same value for the process's life, changes on crash-restart into the same pod identity — fails green under every fixture this story or tasks 5/6 can build: per-call `now()`, or a per-pod-name/per-config value, is observationally identical until a *second process incarnation sharing a pod identity* exists. Their split across the ladder is the useful part and I took it: the **within-process-stability** half IS constructible at task 5 (two `register_meeting` calls in one process return the same epoch) and is pinned there; the load-bearing **changes-on-restart** half is not constructible until the handler-restart story and is pinned there with its test shape stated (incarnation A reports E1, restart into same pod, incarnation B reports E2, E1≠E2). `docs/TODO.md` entry in the AC-2 pattern. |
| **@code-reviewer Gate-3: `#![allow]` should be `#![expect]` (ADR-0002)** | **Accepted, fixed in-changeset, and their refusal of my justification was the right call.** I had cited the sibling `signaling_roundtrip.rs` as precedent; they pointed out that makes it a same-owner pre-existing deviation rather than a licence, and this file is *new* code introducing a fresh one. Took option (b): fixed here with only `expect_used` named (the file has no `.unwrap()`/`panic!`, so the sibling's other two lints would be unfulfilled expectations), and filed the sibling's conversion as a **named** protocol-owned `docs/TODO.md` §Code Quality entry — not a vague "proto tests" entry — because converting it requires checking which of its three lints actually fire, which is more than a mechanical swap. |
| **@meeting-controller Gate-3: `main.md` §2 embedded snippet drifted from the landed proto** | **Fixed, but NOT by re-syncing.** Measured the drift first: **194 of the snippet's 516 lines** no longer appear in the landed file, because Gate-1/Gate-3 findings went into the proto and were not back-ported. Syncing would have created a second apparently-current copy of the whole contract with nothing keeping it current — @observability's F-OBS-4 lesson (*re-syncing buys the same fragile arrangement with a fresher timestamp*) applies with far more force to 516 lines than it did to a five-value list. §2 is now explicitly labelled **"GATE-1 PROPOSAL AS REVIEWED — NOT the source of truth"**, points at the landed proto, and enumerates the divergences. The audit trail of what reviewers actually confirmed is preserved; the ambiguity about which is authoritative is gone. |
| **@dry-reviewer Gate-3: D5 entry filed in the wrong `docs/TODO.md` section** | **Accepted, and the reasoning is the part worth keeping.** The entry's content, owner and trigger were right; only its home was wrong. §Media Path Obligations self-describes at `docs/TODO.md:534-538` as constraints the `media-protocol` codec cannot enforce, each also stated in that crate's rustdoc — the endpoint-spelling divergence is neither. Moved to §Cross-Service Duplication → §From DRY Reviewer (Ongoing), with the section-convention trailer added. **The cost was concrete, not bookkeeping**: §Cross-Service Duplication is the section a DRY reviewer sweeps at the start of a review — that sweep is how they found the `add_auth` clone pair this session, which is what told me where to attach the sibling applied-echo entry — so an entry parked in a media-codec section is invisible at exactly the task-8-10 trigger window the entry names. Scoped to one bullet: the (a)-(e) obligations are correctly placed and stay. |
| **@observability F-OBS-4 the same enumeration drifted a second time, in the opposite direction** | **Accepted, and NOT by re-syncing.** §12a now **cites** `applied_generation`'s field comment as the canonical enumeration and drops its inline list; re-syncing would have reset the drift surface rather than removed it. The incident is the finding: one enumeration, two homes, drifted both ways in two consecutive rounds of active review by the person who owns it. |
| **@observability F-OBS-3 dangling `RegisterMeetingRequest.generation` refs** | **Accepted, and the sharpest of the three**: the stale cross-references spelled the field in the bare unqualified form the rename exists to make unavailable, *inside the comment block that is the SSoT for the distinction* — a reader who follows them lands on nothing and re-derives the value from the name. All corrected to `policy_generation`. |
| OPS-8 / OBS-C UNSPECIFIED = mismatch not skip | Accepted — §2 (both directions) + §12a |
| OPS-9 intersection rule | Resolved — §13, route (b), @security co-signed |
| OPS-10 ADR-0031 ownership | Accepted — owner column corrected on rows 9, 16, 17, 18 to media-handler |
| OBS-A/B zero reserved | Accepted — §2 + §12a |
| OPS-4 skew matrix | §6, reading confirmed |
| OPS-5 rollback + expected layer-state | §5 + §Rollback Procedure |
| OPS-6 operator surfaces | Rows 16-18 + `lib.rs` row 5 |
| OPS-7 per-process epoch | Accepted — §2, and §8 confirms restart detection is supported end-to-end |
| @semantic-guard credential-leak | §10 — no credential carrier on the new shape |
| @semantic-guard masked-failure | §4 + §5 — no suppression, no weakened wrapper, no stubbed-out test |
| @semantic-guard concept-substitution | §3 — no vacated tag reused; retired method names tombstoned |

### 11b. Lead rulings at Gate 1

**Not restated here.** The Lead's rulings on OQ-0a/0b/0c, `proto/buf.yaml`, OQ-1, OQ-2, AC-1 and S16 are
recorded once, in §Gate 1 — Plan Approval (Lead) above. This subsection is a pointer rather than a summary
for the same reason §12a cites instead of listing: I had drafted a second copy of that ruling set here, and
maintaining two copies of one list is precisely the failure this devloop tripped over twice under active
review (@observability F-OBS-4). One home.

### 12a. One hazard, four faces: proto3 zero-values in `RegisterMeetingResponse`
*(@operations OPS-2/3/3b/8, @observability OBS-A/B/C — arrived at independently by both and adopted whole)*

Every field I am adding to `RegisterMeetingResponse` is **read by MC as evidence about MH**, and proto3
gives a pre-reshape MH a well-formed, plausible, wrong answer to all four. **In every case the wrong
answer is the reassuring one**, and the path to it is an ordinary rolling deploy:

| Face | Old MH's answer | Why it is the reassuring one |
|---|---|---|
| OPS-2 — tag reuse | `accepted:true` decoding as a varint on tag 1 | a `1` in an `applied_generation` field reads as "applied generation 1" |
| OPS-3 / OBS-A — `applied_generation` | absent → `0` | reads as a legitimate applied generation |
| OPS-3b / OBS-B — `process_start_epoch_ms` | absent → `0`, **and stable across every call** | reads as "same process, never restarted", permanently |
| OPS-8 / OBS-C — `transport_mode` | absent → `TRANSPORT_MODE_UNSPECIFIED` | invites the defensive `if echoed != UNSPECIFIED { compare }`, which skips the check |

That is §8's *"partial blackhole reporting healthy"* reproduced **inside the four fields added to detect
it**. The fix is the same in all four cases and costs nothing but comment text: **reserve the zero value
and document it, so absence is distinguishable from assertion.** Applied in §2 and §3:

- Tag 1 keeps `accepted`; `applied_generation` is on tag 2 (§3).
- `policy_generation` / `applied_generation`: valid values start at 1; **0 means "no applied generation reported"
  and MUST NOT be read as a successful apply.** @observability's framing is the stronger one and is why
  this is not a judgement call: unless 0 is reserved, the skew state **has no name** — MC cannot construct
  `no_applied_generation` as a distinct bounded outcome, so "you are talking to a handler that predates
  this contract" is unalertable rather than merely mis-valued.
- `process_start_epoch_ms`: 0 means "not reported", never "epoch zero"; MC must treat it as
  *restart undetectable*, never as a stable incarnation. A restart detector pinned to a constant is worse
  than an absent one, because it is load-bearing and green.
- `transport_mode`: **an UNSPECIFIED echo is a MISMATCH, not a skip.** Phrased in the field comment as a
  constraint on the field's *reader*, in the same register as `JoinResponse.sender_id`'s comment, because
  the proto cannot enforce it. This is the one I most want written down: the skip form survives code
  review as defensiveness, and its skip-branch is unreachable in any test written against a same-version
  peer — so it is invisible to the test suite permanently.

**The bounded-outcome instrumentation this enables** (@observability's cardinality guardrail): MC's
divergence metric carries a bounded `outcome` label. **The canonical enumeration of its values lives at
`RegisterMeetingResponse.applied_generation`'s field comment and is deliberately NOT restated here.**
What belongs in this section is the rule the section is about: the generation and epoch **numbers** live in
the log line and in the metric's *value*, never in a label — both are unbounded and would be a series per
policy change and per restart. `handler_id` as a label is fine; pod level is §11's stated aggregation floor.

**The class these three share has a name (@test): equivalent mutants under the reachable input space.**
The correct and incorrect implementations are not equivalent *programs*; they are indistinguishable modulo
the degrees of freedom this story's fixtures can supply — one meeting, a same-version peer, a single
process incarnation. The test passes *for the wrong reason*: vacuously green. @test's mitigation ladder,
strongest first, is what this plan is climbing: **(1) structural impossibility beats a test** — done for
the parallel-list conflict via self-contained `EgressStream`, since a state you cannot write needs no test;
**(2) where structure cannot carry it, pin the killing test as part of the requirement WITH ITS
DISTINGUISHING INPUT NAMED** — done for `sender_id` ("valid in meeting A, REJECTED in meeting B"), for the
transport-mode echo ("same-version peer echoes UNSPECIFIED, MC reports mismatch"), and now for
`process_start_epoch_ms` (T2). **Naming the distinguishing input is the load-bearing move: "add a test"
without the input is not a pin.**

> **Why this section cites instead of listing** (@observability F-OBS-4 — and the incident is better
> evidence than the fix). The enumeration was restated in two homes and **drifted in both directions across
> two consecutive rounds of active review**: last round the field comment was short a value and this
> section was right; this round it was exactly reversed. That happened *under active review*, in a document
> whose §0 is a restatement of the single-source-of-truth mechanism, with the owner of the enumeration
> reading both copies each time. Re-syncing would have bought the same fragile arrangement with a fresher
> timestamp — the seven-homes problem in miniature, inside the document that argues against it. So the list
> has one home, and it is the machine-checkable one: proto comments ride in `SourceCodeInfo`, so a guard can
> assert the field's enumeration; it cannot assert this paragraph's. **@observability is recording the
> incident as evidence in the label-domain guard-gap TODO they own, with my agreement — it is a better
> argument for that guard than anything either of us had, because it is not a story about carelessness.
> Both of us read both copies both times. It is a story about the arrangement.**

### 13. ADR-0024 §6.4 intersection rule — determination on the record
*(@security S1, @operations OPS-9)*

The rule fires by the letter of the manifest: `scripts/guards/simple/cross-boundary-ownership.yaml:34`
carries a path-level entry **narrower and stricter** than the generic `"proto/**": [protocol]` on line 31 —
`"proto/dark_tower/internal/v1/internal.proto": [protocol, auth-controller, security]` — and the manifest's
own header (lines 24-28) uses *this exact file* as its illustration of all-three enforcement being "Gate 1
human-review territory". @operations is right that a green Layer B guard is not evidence the rule was
honoured: Layer B only checks that a non-`Mine` GSA path has *an* owner from the list, and `protocol` is in
the list.

**LEAD RULING (overrides my draft): route (a). @auth-controller has been added as an eighth reviewer and
must confirm at Gate 1 and give a Gate-3 verdict. OQ-0a IS NOT CLOSED until they do.**

I got this wrong and the correction is worth recording rather than quietly absorbing. @security told me
plainly that the adjudication was not theirs to make unilaterally — ADR-0024 §6.2: an owner may **upgrade**
a classification, never downgrade. Asking that same reviewer to co-sign the downgrade, instead of routing
it to the Lead, is precisely the shape the monotonicity rule exists to prevent. I did surface it as an open
question to the Lead, which was right; I then wrote the resolution into the plan as CLOSED before the Lead
answered, which was not. **In a headless run with no human backstop, an open question addressed to the Lead
stays open until the Lead answers it.**

What follows is therefore **@security's recorded position, as input to @auth-controller's own
determination — not the closure.** @auth-controller has been asked to verify the
`connection_token`-is-dead evidence independently and to reach their own conclusion, including whether
ADR-0003's Component 3 text needs an amendment (their call, not mine — I will ask).

1. **MH's auth gate is a service-level prefix match, not a per-method allowlist.**
   `crates/mh-service/src/grpc/auth_interceptor.rs:222-223`:
   `if grpc_path.starts_with("/dark_tower.internal.v1.MediaHandlerService/") { "meeting-controller" }`,
   `else` → fail closed with `permission_denied`. Deleting three RPCs does not change that predicate by one
   character. The authenticated method surface strictly **shrinks** behind an unchanged gate — fail-safe in
   the direction that matters, since the removed paths now fall through to the `else` arm rather than being
   admitted.
2. **The test surface does not move either.** `auth_interceptor.rs:379`'s `MC_GRPC_PATH` constant already
   points at `/dark_tower.internal.v1.MediaHandlerService/RegisterMeeting`, the surviving RPC.
3. **No auth-controller-owned concept appears in the diff.** No `ServiceType` value, no scope enum, no JWT
   claim, no JWKS surface, no token type. The ADR-0003 §5.7 canonical trigger is "`ServiceType` enum, scope
   enums, and identity fields"; the identifiers I add (`sender_id`, `slot_id`, `stream_number`,
   `egress_stream_id`, `handler_id`) are per-meeting media-routing identifiers, not authentication
   identities, and none is used in any authorization decision.
4. **The one genuinely credential-shaped item is a DELETION, and it removes an auth surface rather than
   adding one.** `RegisterResponse.connection_token` was a second, MH-issued client credential parallel to
   the meeting JWT that actually authenticates (`MhJwtValidator::validate_meeting_token`). Its client-facing
   half was already deleted in the 2026-04-13 devloop (`signaling.proto` `MediaServerInfo`,
   `reserved 2; reserved "connection_token";`). This deletes the server-side half, and it was never anything
   but the literal string `"STUB-PLACEHOLDER"` (`mh_service.rs:26,91`). The direction of travel is toward
   one authentication path, not two — which is the outcome auth-controller would want.
5. **Nothing replaces it.** §10 confirms no new field carries a credential, key or token, so this is not a
   rename of the credential into a new home.

**@security's position** (recorded as input, not as closure) adds two points: MH's three auth predicates all live in Rust, not proto (`REQUIRED_SCOPE` at
`auth_interceptor.rs:40`, the `service_type == "meeting-controller"` check at `:224`, and JWKS validation);
and ADR-0036 §3 explicitly says `sender_id` proves nothing about identity, so it is not an "identity field"
in ADR-0024:407's trigger sense.

**@security's position carries a stated revocation trigger**, which I will honour regardless of how
@auth-controller rules: it is void if the landed diff introduces a field read in an authorization
decision, reintroduces anything token/credential/key-shaped, adds an enum mirroring a scope or
service-type vocabulary, or makes `sender_id` carry more than a routing coordinate. I will keep the diff
inside that envelope and ask @security to re-confirm against the actual diff at Gate 3.

**THE INTERSECTION RULE IS CLOSED BY @auth-controller, THE THIRD OWNER.** They independently CONCURRED
with route (b) on their own grounds, not by deferring to @security's: they verified `MeetingTokenClaims`
(`crates/common/src/jwt.rs:357`) carries no `sender_id`, that MH keys client auth on `sub` + `meeting_id`,
and that no auth-controller-owned concept appears in the diff. **Their trailer is conditional on (i) AC-1
landing — the ADR-0003 superseding amendment, row 12b — and (ii) the diff staying inside @security's
revocation envelope.** Both conditions are accepted and tracked. Three owner positions now exist; the
third is the one that closes the rule.

**@auth-controller: the file is `proto/dark_tower/internal/v1/internal.proto`. The three things I would
most want you to check independently are (1) that `RegisterResponse.connection_token` really is dead —
my evidence is §1, Deletion 2, and I would rather you re-run it than trust it; (2) whether any identifier
I am adding (`sender_id`, `slot_id`, `stream_number`, `egress_stream_id`, `handler_id`) is an identity
field in ADR-0003 §5.7's sense rather than a routing coordinate; and (3) whether ADR-0003's Component 3
text needs an amendment now that the participant-level `Register` RPC is gone. (3) is explicitly your call
and not mine.**


### 12. Open questions for @team-lead

- **OQ-0a — OPEN, awaiting @auth-controller.** Lead ruled route (a) and added @auth-controller as an eighth reviewer, overriding my draft resolution. See §13, including the note on why my draft was procedurally wrong (I closed a Lead-addressed question before the Lead answered). @security's argument stands as security's recorded position and as input to auth-controller, not as the closure.
- **OQ-0b (blocking, @operations OPS-1).** Three task prompts (#4, #5, #6) each claim `crates/mc-service/tests/register_meeting_integration.rs` and `otel_grpc_outbound_integration.rs`. They land here because compilation forces it. Tasks #5/#6 should be told, or they will be run against a stale premise.
- **OQ-0c (blocking, CONVENTIONS.md).** Layer 6 will be RED at Gate 2 by design (11 predicted findings, §5). Headless run ⇒ the Lead may not accept it; it escalates to a human. Flagged now, not at Gate 2.
- **OQ-1 (design, for reviewers).** `egress_stream_id` is 1:1 with `(subscriber.sender_id, subscriber.slot_id)` today, which is duplicated identity. The task requires the field. My resolution: keep it, give it a distinct documented job (policy-plane identity stable across a client slot renumber; the hook for §7 pending-switch state and story 2's per-stream egress budget), and put the uniqueness-and-reject obligation on **both**. Alternative if reviewers prefer: drop it and key on `(subscriber, slot)` — deviates from the task's explicit field list, so I would want that instruction from @team-lead.
- **OQ-2 (design, for reviewers).** `RegisterMeetingResponse.transport_mode` is one field echoing a per-egress request property. My resolution: document it as "the mode applied to every egress stream of `applied_generation`" and require MH to **reject** a heterogeneous registration, so the field can never silently echo one of N. Video (story 3) then hits a loud rejection and the field moves per-egress as a deliberate change. Alternative: make the echo `repeated` keyed by `egress_stream_id` now — heavier, and it reintroduces the parallel-list shape §6 warns about on the response side.
- **OQ-4 (scope, for @team-lead + @observability) — replaces OQ-3.** Four operator/doc surfaces go stale the moment I delete the emission sites in `mh_service.rs`: `metrics.rs:12,134` (doc comments), `docs/observability/metrics/mh-service.md:218,221`, `mh-overview.json:1676` (panel description), `mh-alerts.yaml:16` (a deliberate-omission note that justifies having **no** RegisterMeeting alert by reasoning about `method="register"` — the wrong RPC *already*, and after this commit an unemittable label value; a stale *reason not to alert* is worse at 3am than a stale panel). All four are **media-handler**-owned per ADR-0031 and none is compile-forced. **My recommendation: hand all four to story task 5** — media-handler owns every one of them, task 5 is the immediately-next task, and §8 already carries the complete twelve-surface enumeration @observability assembled, so nothing is lost to inference. I have withdrawn them from my table rather than absorb owner-scope the pipeline did not force. If @team-lead would rather I take them with a trailer, say so and I will.
- **OQ-3 (scope, for @observability).** I propose to fix the **prose** label surfaces in this commit (they become false at MY commit, because I delete the emission sites) and leave the **test pins** to task 5. Confirm the split, or tell me to take all of it.

---

## Pre-Work

None.

---

## Implementation Summary

Landed the ADR-0036 Appendix "Internal contract" shape: `RegisterMeeting` is now the MC→MH control
plane, and `MediaHandlerService` has exactly one RPC.

**Deleted** (audit evidence re-verified independently at §1, and reproduced in the commit message):
3 RPCs — `Register`, `RouteMedia`, `StreamTelemetry` — and 8 messages — `RegisterRequest`,
`RegisterResponse`, `RoutingOptions`, `CascadeDestination`, `RouteMediaRequest`, `RouteMediaResponse`,
`StreamTelemetryRequest`, `StreamTelemetryResponse`. proto3 has no `reserved` for message or RPC-method
names, so a tombstone comment block names all eleven with a one-line reason each and a
do-not-resurrect instruction; a gRPC method name is a wire path component, so re-adding one with
different semantics is the same silent-repoint hazard CONVENTIONS.md §5 forbids for field tags.

**Added**: `SubscriberSlot`, `CandidateSource`, `EgressStream`, `SelectionRules`; three fresh fields on
`RegisterMeetingRequest` (tags 4-6) and four on `RegisterMeetingResponse` (tags 2-5). **No tag is
repurposed and none is vacated**, so no `reserved` is required or possible in either message — stated
explicitly at §3 because CONVENTIONS §5's point is that a reader must be able to tell an unreserved tag
from one nobody examined.

Consuming code was updated only as far as compilation forces (Layer 5 runs
`cargo clippy --workspace --all-targets -- -D warnings`), plus the four operator/contract documents
that this commit makes false.

### Two implementation-time findings, recorded in place

1. **`clippy::doc_lazy_continuation` fires on generated bindings from proto comments.** prost turns
   `.proto` comments into rustdoc, and two of my bullet lists in `SubscriberSlot` were followed
   immediately by a new paragraph with no blank line — which rustdoc reads as a lazy list
   continuation. **Fixed at the source** (a blank comment line in the `.proto`), not with an `#[allow]`
   on generated code, per ADR-0033 §13 "fix the parser, don't relax the check". Worth recording as a
   general constraint for anyone writing prose-heavy proto comments in this repo: **a paragraph flush
   against a preceding `-` bullet list is a clippy error two crates downstream**, and the error points
   at a path in `target/`, which is not where the fix is.
2. **`crates/proto-gen/tests/internal_roundtrip.rs` carries
   `#![expect(clippy::expect_used, reason = "...")]`.** In a wire-shape test a failed decode **is** the
   assertion mechanism. **Corrected at Gate 3 (@code-reviewer)**: I first wrote `#![allow(...)]` copying
   the sibling `signaling_roundtrip.rs:18` verbatim, and they declined "the sibling does it too" as
   scope-lock — correctly, since that sibling is a same-owner (protocol) *pre-existing deviation*, not a
   licence, and this file is new code introducing a fresh one. ADR-0002 requires `#[expect]` twice
   (`:290` and the `:334` checklist), `clippy.toml:20` models the form, and the self-cleaning property is
   live here: if a future edit removes the last `.expect()`, `#[expect]` warns that the suppression is
   dead where `#[allow]` would rot silently. Only `expect_used` is named — this file has no `.unwrap()`
   and no explicit `panic!`, so carrying the sibling's other two lints would itself be an unfulfilled
   expectation. Verified fulfilled: clippy is clean with `-D warnings`, which it would not be if the
   lint never fired. The sibling's conversion is filed as a named protocol-owned entry in
   `docs/TODO.md` §Code Quality rather than folded in — it needs per-lint verification, not a swap.

---

## Files Modified

| File | Change | Owner |
|------|--------|-------|
| `proto/dark_tower/internal/v1/internal.proto` | The reshape: 3 RPCs + 8 messages deleted with a tombstone block; `SubscriberSlot`/`CandidateSource`/`EgressStream`/`SelectionRules` added; `RegisterMeetingRequest` +3 fresh tags, `RegisterMeetingResponse` +4 fresh tags | protocol (GSA) |
| `packages/proto-gen/scripts/verify-codegen.sh` | Presence asserts on the 6 new/surviving internal symbols; **absence asserts on all 8 deleted symbols** — a presence-only oracle stays green through a half-done deletion. `RegisterResponse` absence is file-scoped, because `sdk-core/src/http/types.ts` defines an unrelated AC HTTP type of that name | protocol (GSA) |
| `crates/proto-gen/tests/internal_roundtrip.rs` **(new)** | 5 wire-shape tests (@test T1) + a "what these deliberately do NOT assert" header naming the three runtime MUSTs no test in this story can construct | protocol (GSA) |
| `crates/mh-service/src/grpc/mh_service.rs` | 3 stub handlers, `stub_placeholder()` and `stream_next()` deleted; imports narrowed; response literal gains 4 truthful-zero fields with a `TODO(story task 5)`; module docs corrected | media-handler |
| `crates/mh-service/src/grpc/mod.rs`, `src/lib.rs` | Doc fences naming the retired RPCs; `lib.rs`'s "Current Status: Stub" preamble → "Partial" (@media-handler F3) | media-handler |
| `crates/mh-service/tests/{register_meeting,auth_layer,otel_grpc}_integration.rs` | Request literals gain 3 fields (4 sites) | media-handler |
| `crates/mc-service/src/grpc/mh_client.rs` | Request literal gains 3 fields; `TODO(story task 6)` for the real policy push and `TODO(story task 5/6 ordering)` for the generation-0 precondition | meeting-controller |
| `crates/mc-service/tests/{register_meeting,otel_grpc_outbound}_integration.rs` | Mocks: 3 trait methods and their imports dropped; response literals gain 4 truthful-zero fields with the reason inline (@meeting-controller F3) | meeting-controller |
| `docs/decisions/adr-0003-service-authentication.md` | **@auth-controller AC-1.** Component 3's Connection Tokens marked superseded with the JSON retained under a strike; Component 5 flow + Component 6 diagram renamed `RouteMedia` → `RegisterMeeting`; the Consequences "connection token binding" positive struck; an amendment note stating that **nothing about the service→service decision changes** | auth-controller + security |
| `docs/decisions/adr-0036-media-flow.md` | **@security S15.** Appendix credential-guard sentence corrected in place, visibly, with the correction noted — true for logs, false for internal messages | security |
| `docs/API_CONTRACTS.md` | §3.1 credential path; §4.1/4.2/4.3 replaced by the single-RPC control-plane spec with a superseded block explaining each deletion | protocol + security |
| `docs/ARCHITECTURE.md`, `docs/WEBTRANSPORT_FLOW.md` | The `connection_token` client-auth path → meeting JWT (ADR-0020) | security |
| `infra/grafana/dashboards/mh-overview.json` | `:1676` panel description: `method` is single-valued by design | media-handler |
| `infra/docker/prometheus/rules/mh-alerts.yaml` | Row 18 as @media-handler finally scoped it: dead label corrected; transitive-coverage restated honestly; forward-pointer to `docs/observability/slos.md`; **and the adjacent `:15` burn-rate pointer redirected from ADR-0011 to the same file**, so two adjacent omission notes stop disagreeing about where SLOs ratify. **No alert added** | media-handler |
| `docs/specialist-knowledge/protocol/INDEX.md` | RPC list corrected; new control-plane navigation block | protocol |
| `docs/TODO.md` | Landed EARLY per Lead direction so @security and @operations are not blocked: 5 Media Path Obligations (a)-(e) and the `.proto` scanner gap under §Media Path Obligations; the endpoint-spelling boundary and the sibling applied-echo gap under §Cross-Service Duplication (the latter **attached to the existing `GcClient` duplication entry**, not filed as an orphan); and the task-4/5/6 prompt collision in its own section | protocol |

**Rows 9 and 16 of the plan table were NOT touched** (`metrics.rs:12,134`, `mh-service.md:218,221`) —
@media-handler's owner ruling moved them to story task 5, so the method-label domain enumeration is
reduced atomically with its executable pin (`metrics.rs:402`'s `valid_methods`) rather than split
across two commits.

---

## Devloop Verification Steps

### Layer 6 — MEASURED against the §5 declaration. Set-identical.

`buf breaking proto --against .git#ref=0216eab...,subdir=proto` → **11 findings**, matching the
prediction recorded in §5 *before* implementation, symbol for symbol:

| Rule class | Predicted | Measured | Symbols |
|---|---|---|---|
| `MESSAGE_NO_DELETE` | 8 | **8** | `CascadeDestination`, `RegisterRequest`, `RegisterResponse`, `RouteMediaRequest`, `RouteMediaResponse`, `RoutingOptions`, `StreamTelemetryRequest`, `StreamTelemetryResponse` |
| `RPC_NO_DELETE` | 3 | **3** | `Register`, `RouteMedia`, `StreamTelemetry` (all on `MediaHandlerService`) |
| `FIELD_*` | 0 | **0** | — |
| `ENUM_*`, `FILE_NO_DELETE`, `PACKAGE_NO_DELETE` | 0 | **0** | — |
| Findings in `signaling.proto` | 0 | **0** | — |
| **Total** | **11** | **11** | all in `internal.proto` |

**Zero unpredicted findings.** The `FIELD_* = 0` row is the load-bearing one: it is the mechanical
confirmation that the additive tag work (request +3, response +4) repurposed nothing, which is the
hazard §3's tag map argues about in prose. Three independent derivations agree — my declaration,
@observability's pre-implementation scratch measurement, and this run.

**§5 has not been edited to match this result** (Lead direction): the declaration is the artifact, and
editing it post hoc would destroy the only thing that makes the escalation checkable.

### Other gates

| Check | Result |
|---|---|
| `buf build proto` | PASS |
| `buf lint proto` (STANDARD, no `// buf:lint:ignore` added) | PASS |
| `buf format proto -d` | clean |
| `cargo build --workspace` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (after fixing `doc_lazy_continuation` **in the proto source**, not with an `#[allow]`) |
| `cargo test -p proto-gen --test internal_roundtrip` | 5 passed |
| `packages/proto-gen/scripts/verify-codegen.sh` | all checks passed — including the 8 new absence asserts |
| `cargo test --workspace` | 40 suites, **0 failures** |
| `scripts/guards/run-guards.sh` | 36 of 37 pass; the single failure is **proven pre-existing** — see below |
| `proto/buf.yaml` | **untouched**, per Lead ruling |

### `validate-subdomain-regex-sync` — pre-existing, NOT caused by this diff, and NOT masked

Fails on a **clean tree at the start commit**, proven rather than asserted:

```
$ git stash -u && bash scripts/guards/simple/validate-subdomain-regex-sync.sh; echo $?
1
```

It expects 10 occurrences of the org-subdomain pattern and finds 13. The three extra are in
`.nx/cache/…/coverage/src/validation/limits.ts.html` (×2) and
`packages/sdk-core/coverage/src/validation/limits.ts.html` — **gitignored build artifacts**, i.e. the
already-tracked entry `docs/TODO.md` §"Guard Precision — validate-subdomain-regex-sync scans gitignored
build artifacts". Nothing in this diff touches a subdomain pattern, a migration, or the SDK validation
module.

**Deliberately not "fixed" here.** Deleting the coverage artifacts would turn the guard green without
changing the guard, which is the masked-failure shape CLAUDE.md forbids — the next `pnpm test` run
recreates them and the red returns, having taught nobody anything. It is an owned, tracked
guard-precision defect and it stays visible.

### Two findings about the Gate-1 guards themselves

Both surfaced because Layer 3 fired on my own plan document. Recorded because in each case the guard
reported **green on something it had not actually checked**.

1. **The Gate-1 classification-guard pass was VACUOUS, and the count was the visible tell.** The Lead
   reported `STATUS=OK REASON=cross-boundary-classification-clean-1-files` at Gate 1 on a table with
   **21 rows**. `dt-guard`'s `parse_plan_paths` takes `row.cells.first()` — the FIRST column — as the
   path, and my table's first column was a row *number* (`| 1 | \`proto/...\` | ... |`). So every row
   parsed as a non-path and was skipped, and the one file it did find came from elsewhere in the
   document. The guard was structurally incapable of checking my table and said OK. Nobody read
   "1-files" against a 21-row table, myself included. **Fixed by rebuilding the table in the canonical
   `| Path | Classification | Owner | ... |` shape** (the format `cross_boundary_scope.rs`'s own unit
   test pins); the guard now reports `clean-10-files` and the scope-drift guard reports
   `cross-boundary-scope-no-drift`. **A count in a guard's OK line is a checkable assertion, not
   decoration** — the same lesson as this devloop's F-OBS findings, one layer out.
2. **`owner_not_in_manifest` compares the Owner cell for EXACT equality against a single manifest
   entry**, so a multi-owner cell cannot be expressed — even for the one path whose manifest entry
   exists precisely to name three owners
   (`internal.proto` → `[protocol, auth-controller, security]`). `protocol, auth-controller, security`
   is rejected. This is consistent with the manifest's own header ("all-three intersection enforcement
   … remains Gate 1 human-review territory"), so it is a documented limit rather than a defect — but it
   means **the guard can never evidence an intersection-rule co-sign**, and a reader who sees the
   Owner cell naming one specialist must not conclude only one was required. The cell names the
   implementing owner; the three-way co-sign is recorded in §13 and in the commit message.

---

---

## Lead Gate-2 Verdict (2026-09-01)

Full unattended run: `./scripts/layer-all.sh` → `/tmp/layer-all-gate2.log`. All seven layers evaluated
(no fail-fast, no `NOT-RUN`).

```
LAYER=1 RESULT=OK   DURATION=9
LAYER=2 RESULT=OK   DURATION=2
LAYER=3 RESULT=FAIL DURATION=23
LAYER=4 RESULT=N/A  DURATION=192
LAYER=5 RESULT=OK   DURATION=3
LAYER=6 RESULT=FAIL DURATION=1
LAYER=7 RESULT=OK   DURATION=624
TOTAL_DURATION=854 TOTAL_RESULT=FAIL
```

### Layer 6 — the intentional wire break, measured against the declaration

The §Expected Layer-State table was written **before** implementation, corroborated independently by
@observability's pre-implementation scratch measurement, and **was not edited afterwards**. The real log
is set-identical to it, symbol for symbol:

| Rule class | Declared | Measured | Symbols |
|---|---|---|---|
| `MESSAGE_NO_DELETE` | 8 | **8** | `CascadeDestination`, `RegisterRequest`, `RegisterResponse`, `RouteMediaRequest`, `RouteMediaResponse`, `RoutingOptions`, `StreamTelemetryRequest`, `StreamTelemetryResponse` |
| `RPC_NO_DELETE` | 3 | **3** | `Register`, `RouteMedia`, `StreamTelemetry` (all `MediaHandlerService`) |
| `FIELD_*` | 0 | **0** | — |
| `ENUM_*` / `FILE_NO_DELETE` / `PACKAGE_NO_DELETE` | 0 | **0** | — |
| findings in `signaling.proto` | 0 | **0** | — |
| **Total** | **11** | **11** | |

**Zero fired-but-unpredicted, zero predicted-but-not-fired.** That is the discriminator separating a
deliberate break from a bad diff.

`SUPPRESSED=dark_tower/signaling/v1/signaling.proto` — the pre-existing task-3 carve-out only.
`proto/buf.yaml` is untouched; no `breaking.ignore` entry was added, widened, or proposed.

The `FIELD_* = 0` row is load-bearing rather than decorative: it is the *mechanical* confirmation that the
additive tag work repurposed nothing, which §3's tag map could only argue in prose — and it is precisely
the detection a widened `breaking.ignore` would have destroyed (@operations OPS-2: a varint on
`RegisterMeetingResponse` tag 1 makes an old MH's `accepted: true` decode as `applied_generation: 1`).

### Layer 3 — pre-existing, diff-independent, deliberately not masked

`validate-subdomain-regex-sync` fails: 13 occurrences found, 10 expected. Lead verified independently
rather than accepting the implementer's report, because a pre-existing-red claim is the one Gate-2 claim
that excuses a red and therefore most deserves checking. **All three excess hits are gitignored build
artifacts** — `packages/sdk-core/coverage/src/validation/limits.ts.html:590` and two
`.nx/cache/**/limits.ts.html:590` copies. All seven enumerated SSoT sites report `OK`. **None of this
diff's 20 files appears in the hit list**, and the diff touches no subdomain pattern, migration, or SDK
validation module. Tracked in `docs/TODO.md` §Guard Precision.

Deleting the artifacts would turn the guard green without changing anything real, and the next
`pnpm test` recreates them — making a guard green by removing its inputs is the masking CLAUDE.md forbids.
It stays red and visible.

### Layer 4 — `N/A` is the documented intentional gap, not an unexplained skip

`rust` → `STATUS=OK REASON=cargo-test-passed`; `ts` → `STATUS=OK REASON=nx-test-passed`; the aggregate
`N/A` comes from proto's registered intentional-gap placeholder
(`STATUS=N/A REASON=not-applicable-to-this-lang`). This is the self-justifying case in ADR-0033 §6 — the
wrapper's own `REASON=` is the justification. **Not** `FAIL-MISSING-VERB`, **not** `NOT-RUN`.

### Layer 7 — green, both suites

Rust env-tests `STATUS=OK REASON=env-tests-passed`; browser E2E `STATUS=OK REASON=browser-e2e-passed`
(8/8 Playwright tests, 1.0m). Layer 6's audit lane: `cargo-audit-passed`, pnpm
`SKIPPED-NO-DIFF REASON=no-dep-changes` (the within-wrapper dep-manifest gate).

### Disposition

Neither red is fixable by the implementer without masking, so neither consumes a Gate-2 attempt and
neither routes to the implementer lane. Review proceeds to Gate 3 on the full panel so that the
escalation reaches a human as a *fully reviewed* diff with one known, declared, measured red — the
task-3 precedent. `docs/protocol/CONVENTIONS.md` lines 27-33 and 58-60 reserve acceptance of this red to
a human; the Lead does not self-approve it.


---

## Lead Gate-2 Re-run (2026-09-01, after the four Gate-3 fix-now items)

Rust source changed after the first Gate 2 (the `#![allow]` → `#[expect]` conversion), so the full
pipeline was re-run rather than assumed: `/tmp/devloop/layer-all-gate2b.log`.

```
LAYER=1 RESULT=OK   LAYER=2 RESULT=OK   LAYER=3 RESULT=FAIL
LAYER=4 RESULT=FAIL LAYER=5 RESULT=OK   LAYER=6 RESULT=FAIL  LAYER=7 RESULT=OK
TOTAL_DURATION=706 TOTAL_RESULT=FAIL
```

### Layer 4 flaked, and was chased rather than waved through

Five gc-service tests failed — `tasks::assignment_cleanup`, `tasks::health_checker`,
`tasks::mh_health_checker`, and two `grpc::mh_service` integration tests — all with
`PgDatabaseError 3D000: database "_sqlx_test_…" does not exist … It seems to have just been dropped or
renamed`. That is sqlx's per-test database provisioning racing under machine contention.

Applying the reproduce-on-retry discriminator (`docs/runbooks/devloop-validation.md` §6.3) rather than
assuming:

| Check | Result |
|---|---|
| `cargo test -p gc-service --lib` | **369 passed, 0 failed** — including all five |
| `./scripts/layer4.sh` | **`STATUS=OK REASON=cargo-test-passed`**, `nx-test-passed`, exit 0 |
| First Gate-2 run | Layer 4 rust+ts both `OK` |
| Do any of the five touch the reshaped contract? | **No** — GC assignment cleanup, health checkers, and GC's `MediaHandlerRegistryService`, none in the changeset |

**Non-reproducing ⇒ operator lane, no attempt consumed.** Recorded rather than silently absorbed: a flake
waved through is how the next one becomes invisible.

### Layers 3 and 6 unchanged

Layer 3 is the same pre-existing `validate-subdomain-regex-sync` red, independently verified
diff-independent by the Lead and re-verified by @test against a clean tree at `d32ccec`.

Layer 6 is **still exactly 11 findings, still symbol-for-symbol identical to the §5 declaration**. The four
fixes did not perturb it. A declaration that survives four rounds of edits without needing amendment is a
stronger artifact than one measured once.

**Stable Gate-2 state: layers 1, 2, 4, 5, 7 green; layers 3 and 6 red, both analysed, both requiring a
human.**

---

## Gate 3 — Final Approval

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | 4 revocation conditions re-verified against the landed diff; S16/S17/S16b filed as deliverables, excluded from the count |
| Test | RESOLVED-DEFERRED | 3 | 3 | 1 | T1 roundtrip suite (ran it: 5/5, meaningful `assert_ne!`), T2 epoch pin, T3 untracked-test-file. Deferral: `valid_methods` superset → task 5 |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | F-OBS-1/2/3/4/5. Method-label collapse confirmed at 8 emission sites. §11 deletion co-signed |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | `#![allow]` → `#[expect]` per ADR-0002; verified *fulfilled*, so it measures green |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | D5 entry relocated to §Cross-Service Duplication. Vocabulary import verified at `internal.proto:282`, `:638` |
| Operations | RESOLVED-DEFERRED | 12 | 12 | 4 | OPS-1..12. All four deferrals tracked with owners and triggers |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 | All three lenses clean against files, not assertions |
| Auth Controller | RESOLVED-FIXED | 2 | 2 | 0 | AC-1, AC-2. **`Approved-Cross-Boundary: auth-controller` trailer GRANTED** |
| Media Handler (owner) | CLEAR | 5 | 5 | 0 | 8 hunks re-confirmed; rows 9/16 verified wholly absent |
| Meeting Controller (owner) | CLEAR | 3 | 3 | 0 | 3 hunks re-confirmed; can write task 6's reader on the first attempt |

**Zero ESCALATED verdicts.** Two reviewers landed RESOLVED-DEFERRED, so accepted deferrals exist and are
listed below.

### Findings that were defects in the plan, caught before code existed

Six findings changed the contract; **four were defects in the implementer's own draft**, and two were
defects in a *reviewer's* finding:

- **OPS-11** — the `policy_generation == 0` reject was mandated one task before its precondition. Enforced
  in task order it would have rejected every registration for the whole task-5→task-6 window: ADR-0036 §8's
  opening-paragraph blackhole, reproduced through the field added to prevent it.
- **@media-handler F1** — the `sender_id` MUST would have rejected loopback's own registration, whose
  subscriber is promoted by that very RPC. It is a race, so it would have presented as a flake, and the
  reflex response to a flake is a re-run that passes.
- **The over-correction of F1** — would have pegged MC's §8 comparison into permanent false divergence.
- **F-OBS-2** — a fail-loud comparison specified with no bounded label to land in.
- **@security's S14 contained a bug the implementer caught**: the split of *illegal* from *not valid yet*.
  Security's own wording would have made registration hostile to §8's drain-and-promote mechanism. The
  intersection rule running in the direction nobody designs for.
- **@operations' own OPS-9 route** was a §6.2 downgrade reached by the two owners already in the room;
  overruled by the Lead, and the added third owner produced AC-1.


## Code Review Results

All ten reviewer verdicts are recorded in §Gate 3 above and stand unchanged — zero ESCALATED,
two RESOLVED-DEFERRED (Test, Operations) whose accepted deferrals are listed in §Accepted Deferrals.
Sessions 2 and 3 changed no reviewed code, so no verdict was reopened.

---

## Accepted Deferrals

Each entry is an issue this devloop chose NOT to fix — a cost shift to future work; bodies live in `docs/TODO.md` and these are pointers only.

- `docs/TODO.md` §Media Path Obligations — `sender_id` meeting-scoping, task-5 pin w/ multi-meeting test
- `docs/TODO.md` §Media Path Obligations — `process_start_epoch_ms` producer obligation, split by constructibility
- `docs/TODO.md` §Media Path Obligations — `valid_methods` metric-pin superset → task 5 (media-handler)
- `docs/TODO.md` §Media Path Obligations — MC-side reader obligations + UNSPECIFIED-echo negative test → task 6
- `docs/TODO.md` §Guard Coverage Gaps — 7 dead ADR-0024 §6.4 GSA globs (S16); `dt-guard` has no `.proto` scanner
- `docs/TODO.md` §Guard Coverage Gaps — `rust_log_secrets.rs:139` bare `guard:ignore` hatch (S17)
- `docs/TODO.md` §Observability Debt — guard path roots unasserted (`application_metrics.rs:176`, `kustomize.rs:145`)
- `docs/TODO.md` §Observability Debt — `MCRegisterMeetingFailureRate` paging gap, blocked on `slos.md` (task 7)
- `docs/TODO.md` §Cross-Service Duplication — `webtransport_endpoint` vs `media_handler_url` (D5)
- `docs/TODO.md` §Cross-Service Duplication — §8 applied-echo gap in 2 sibling RPCs, on the `add_auth` clone-pair entry
- `docs/TODO.md` §Code Quality — `signaling_roundtrip.rs:18` `#![allow]` → `#[expect]` (same-owner)
- `docs/TODO.md` §Code Quality — `join_tests.rs:18` same deviation (cross-owner, meeting-controller)
- `docs/TODO.md` §Guard Precision — `validate-subdomain-regex-sync` counts gitignored build artifacts — **RESOLVED externally, commit `28defd8`**
- `docs/TODO.md` — story-runner prompt collision: tasks 4/5/6 each claim the two MC test-mock files
- `docs/TODO.md` §Guard Precision — `validate-cross-boundary-scope` reds on `.devloop-escalation.json`, the file the headless contract requires — **RESOLVED externally, commit `cddc576`**

## Scope Decisions (NOT deferrals)

Rows 9 and 16 moving to task 5 is a **scoped owner handoff** under @media-handler's ADR-0031 ruling, not a
deferred finding, so it is deliberately absent from §Accepted Deferrals above. @observability asked for that
distinction to be explicit, because at a later audit "moved to the next task" and "deferred" read identically
and they are not the same thing. This section exists because §Accepted Deferrals is pointer-only by convention
(`validate-todo-tracking`'s `inline_debt_body` rule), and devloop-local scope prose belongs outside it.

---

## Escalation (2026-09-01)

**`.devloop-escalation.json` written; NOTHING COMMITTED.** The full reviewed diff is in the working tree,
with `crates/proto-gen/tests/internal_roundtrip.rs` and this file intent-to-added (`git add -N`) so neither
can silently miss the eventual commit — @test's T3.

Layer 6 is red by design. `docs/protocol/CONVENTIONS.md` lines 27-33 state that in a headless run the Lead
may **not** accept that red, because doing so is a self-approved risk acceptance; lines 58-60 address this
task's implementer by name and prescribe exactly this route. The decision is a human's.

The artifact that makes it a decision rather than a rubber stamp: the §Expected Layer-State table was
written **before** implementation, independently measured by @observability on a scratch tree before the
code existed, and matched **symbol for symbol** on two separate full-pipeline runs — with zero
fired-but-unpredicted and zero predicted-but-not-fired. It was never edited after the fact.

---

## Resume (2026-09-01, session 2 — run-story relaunch)

The runner relaunched this task via the `continue_slug` path
(`scripts/workflow/run-story.sh:1435`), whose prompt is the **infra-interruption** wording, not an
operator-intervention retry. Corroborated rather than assumed: the manifest still records task 4 as
`pending` (never `escalated`), and `.devloop-escalation.json` is absent — the session-limit lane's own
`git clean -fdq` removes it, which is exactly why @test's T3 `git add -N` on `main.md` and
`crates/proto-gen/tests/internal_roundtrip.rs` mattered. Both survived; the full reviewed diff is intact.

### The Layer-6 escalation was answered by the user, and the answer is in the tree

`proto/buf.yaml` carries a new `breaking.ignore` entry for
`dark_tower/internal/v1/internal.proto`, with a comment block self-describing as
"SCOPE EXTENSION (2026-09-01, task-4 escalation, user decision)" and granting acceptance across the
whole ADR-0036 story-1 reshape (tasks 8-10 included), with an explicit DO-NOT-NARROW instruction.

**Not taken on the comment's word.** File mtimes place the edit outside this devloop's authorship:

| File | Last write (UTC) |
|---|---|
| `proto/dark_tower/internal/v1/internal.proto` | 06:11:30 |
| `crates/proto-gen/tests/internal_roundtrip.rs` | 06:16:46 |
| `docs/TODO.md` | 06:34:22 |
| `main.md` (the §Escalation section) | 06:34:40 |
| **`proto/buf.yaml`** | **17:09:30** |

Ten and a half hours after this devloop's last write, and ~2 minutes before session 2 started. The
devloop itself declared `proto/buf.yaml` off-limits in five places (§Gate 1 ruling at :189, §4, §5 twice,
and the classification table row "untouched, per Lead ruling") and never touched it. This is the human
decision `docs/protocol/CONVENTIONS.md` lines 27-33 reserve to a human, arriving by the same route
task 3's did.

### The break was re-measured with the gate suppressed

The carve-out destroys the very detection the §5 declaration was built around, so the 11 findings were
re-measured directly rather than inferred: `proto/buf.yaml` copied aside, the `internal.proto` line
removed, `buf breaking proto --against .git#ref=<base>,subdir=proto` run at the same base ref
`_get_base_ref.sh` resolves (`0216eab9`), then the file restored and verified **byte-identical**.

**Exactly 11 findings — 8 `MESSAGE_NO_DELETE` + 3 `RPC_NO_DELETE`, symbol for symbol identical to the
§5 declaration for the third independent time.** Zero fired-but-unpredicted, zero
predicted-but-not-fired, zero `FIELD_*`. The `FIELD_* = 0` row still holds, which is the mechanical
confirmation that no tag was repurposed — the one thing a widened `breaking.ignore` would otherwise have
hidden (@operations OPS-2).

### Two NEW guard violations appeared, both real, both fixed

Layer 3 went from one failing guard to three. Both new ones are genuine and neither existed at the last
Gate-2 re-run:

1. **`validate-cross-boundary-scope` — `scope_drift_inbound "proto/buf.yaml"`.** The user's edit put a
   file in the diff that §Cross-Boundary Classification did not name. The guard is right: the table must
   name **every** file in the diff, not only the ones the implementer wrote. **Fixed** by adding row 22,
   which states plainly that the row is not this devloop's authorship. Now `cross-boundary-scope-no-drift`.
2. **`validate-todo-tracking` — `inline_debt_body` at §Accepted Deferrals.** Latent, not new-in-substance:
   §Accepted Deferrals was written *after* the last Gate-2 re-run, so no pipeline had ever measured it.
   The section is pointer-only by convention and carried two prose blocks. **Fixed** by reflowing the
   intro to one line and moving the scope-decision paragraph into its own §Scope Decisions (NOT deferrals)
   H2 — which is where the devloop skill says devloop-local scope prose belongs anyway. Now
   `todo-tracking-clean`.

### One Lead error, made and reverted inside this session

The Lead edited `validate-subdomain-regex-sync.sh` to add `--exclude-dir=.nx --exclude-dir=coverage`,
reasoning from CLAUDE.md's "fix, don't defer" that a one-line fix should not sit in `docs/TODO.md`. It
worked — guard green at the unchanged pinned total of 10, self-test 20/22 → 22/22, and an injected
eighth encoding in live source still reds. **It was still wrong, and it was reverted.**

`docs/TODO.md` line 205 already records that exact edit as **"considered + REVERTED"** by the
fast-fail-guards devloop, for two reasons the Lead had not read first:

- It is whack-a-mole on the false positive and **does nothing for the false negative**: the pin is
  exact-equality, so deleting an unenumerated tracked pin (10→9) while a stray artifact adds +1 (→10)
  passes silently — the drift-masking the guard exists to prevent.
- The proper fix is a `git ls-files`-scoped sweep, and it is **task-sized**: the self-test drives the
  guard against a synthetic tree with no `git init`, so `git ls-files` there returns nothing, the
  vacuity branch fires, and every self-test case reds. The seam must be reworked in the same change, on
  a single path — branching the scan would make the tested path structurally different from the
  production path.

Owner is **operations**, and that entry closes with "Deliberately NOT folded into the fast-fail-guards
diff — an unrelated guard edit inside a validation-pipeline change is the shape that should not be
convenient (Lead reversed an initial fold-in ruling on exactly this ground)." A second Lead reversing a
second fold-in on the same ground is the entry working as designed. `git checkout` confirmed the tree
clean under `scripts/guards/`.

### A guard/contract collision, surfaced and worth fixing properly

Writing `.devloop-escalation.json` — which the devloop skill's Headless Mode rule 3 **requires** — reds
`validate-cross-boundary-scope` with `scope_drift_inbound`, because the guard counts it as an unplanned
file in the diff. So every headless devloop that escalates trips this guard by obeying the skill, and the
red then lands in the record of the very escalation it is reporting.

Handled truthfully here by adding row 23 rather than by any suppression, but the general fix belongs in
the guard (the file is a transient runner artifact the runner itself deletes, not a source change) and is
filed in `docs/TODO.md` §Guard Precision. Recorded rather than absorbed silently, because the next
headless escalation will hit it identically and a reader who sees the row without this note would think
the devloop planned to escalate from the start.

### Layer 3's remaining red, re-verified independently

`validate-subdomain-regex-sync`: **15 occurrences found, 10 expected**. All five excess hits are
gitignored build artifacts (`git check-ignore` confirmed) — one `packages/sdk-core/coverage/**` and
**four** `.nx/cache/**` copies of `limits.ts.html`. Two more have accumulated since the 06:34 analysis
recorded 13, which is the entry's own prediction: every `pnpm test` adds copies. All 7 enumerated sites
report `OK`; 15 − 5 = exactly the pinned 10.

Diff-independence re-verified mechanically this session, not carried forward on trust: the guard's
10-path hit list was intersected with this changeset's file list. **Empty intersection.**

### Gate 2 — final measurement (unattended, all seven layers, no `NOT-RUN`)

Re-run after the documentation edits above, so the verdict does not predate the tree it describes
(`/tmp/devloop/layer-all-final.log`):

```
LAYER=1 RESULT=OK   DURATION=4     buf-build-passed
LAYER=2 RESULT=OK   DURATION=2     buf-format-passed
LAYER=3 RESULT=FAIL DURATION=24    guard-violations  (SOLE guard: validate-subdomain-regex-sync)
LAYER=4 RESULT=N/A  DURATION=165   rust OK + ts OK; N/A = proto intentional-gap placeholder
LAYER=5 RESULT=OK   DURATION=2     buf-lint-passed
LAYER=6 RESULT=N/A  DURATION=1     cargo-audit OK; pnpm SKIPPED-NO-DIFF; buf-breaking OK with SUPPRESSED=
LAYER=7 RESULT=OK   DURATION=486   env-tests-passed + browser-e2e-passed
TOTAL_DURATION=684 TOTAL_RESULT=FAIL
```

Three consecutive seven-layer runs this session, no `NOT-RUN` in any of them; Layer 3 went 3 failing
guards → 1 as the two real violations were fixed. Layer 4's previous-session sqlx flake did not recur in
any of the three.

Layer 6 is **green for the first time** — the user's carve-out is why, and `SUPPRESSED=` now names both
protos so the disabled gate cannot read as a silent green. Layer 4's flake from the previous session did
not recur (`cargo-test-passed`, `nx-test-passed`).

### Why this re-escalates instead of committing

The Layer-6 blocker is resolved. **Layer 3 is not**, and it is a different question from the one the user
answered, so their `buf.yaml` decision cannot be read as covering it.

`TOTAL_RESULT=FAIL`. The devloop skill's Headless Mode rule 1 is unconditional — the Lead may not
"improvise past a limit, relax a gate, or self-approve a risk acceptance to keep going" — and committing
over a red pipeline is precisely a self-approved risk acceptance. The temptation to read the user's
silence on a red that `main.md` documented in three places as tacit acceptance is an **inference about
intent, not a decision**, and inferring acceptance is the failure mode the rule exists to prevent. Note
also that task 3's precedent commit landed with **L3 OK**: there is no precedent in this story for
committing over a red Layer 3.

What the human is being asked is narrow and, unlike the Layer-6 question, has an obvious cheap fix
available. It is escalated rather than decided because it is **story-wide**: the same red will stop tasks
5-10 identically, so whichever way it goes, it should be decided once.

## Resume (2026-09-01, session 3 — run-story relaunch after the session-2 re-escalation)

**Both blockers the previous session escalated on are resolved, by two commits the user landed at HEAD
between sessions.** Neither was authored by this devloop; both were verified against their claims rather
than accepted on their commit messages.

| Commit | Author / time (UTC) | What it fixes |
|---|---|---|
| `cddc576` | Nathan Buckles, 18:48:58 | `crates/dt-guard/src/cross_boundary_scope.rs::is_symmetric_exclusion` exempts the repo-root `.devloop-escalation.json` — the guard/contract collision this devloop surfaced |
| `28defd8` | Nathan Buckles, 18:51:59 | `validate-subdomain-regex-sync` sweeps `git grep` over tracked files only — the sole remaining Layer-3 red |

`28defd8` is the fix session 2 argued for and explicitly declined to make itself. Its commit message names
the same two properties that session's analysis did — that a denylist closes the false positive but not the
exact-equality false negative, and that the self-test seam forces a single `git init`/`git add` path rather
than a branch — and it landed as its own `2026-09-01-subdomain-guard-tracked-only` devloop with its own
verdicts. The Lead's session-2 revert of that same edit was correct, and this is what the proper fix cost.

### The escalation contract's own artifact is gone, so row 23 is gone with it

`.devloop-escalation.json` is absent (the runner consumes and removes it). It is no longer in the diff, and
`cddc576` now excludes it *symmetrically* — so a classification row naming it would neither red the guard
nor describe anything real. **Row 23 was removed from §Cross-Boundary Classification**, and the
`docs/TODO.md` §Guard Precision entry that filed the collision is closed with a pointer to the commit and to
the self-test that pins the narrow shape (root path exempt, `docs/.devloop-escalation.json` still reds).
Row 22 (`proto/buf.yaml`) **stays** — that file is still in this diff and still not this devloop's authorship.

### The intent-to-add was lost across the session boundary and was restored

`crates/proto-gen/tests/internal_roundtrip.rs` and this `main.md` came back **untracked**, not `git add -N`
as session 2 left them — the relaunch restored content but not index state. That is exactly the failure
@test's T3 exists to prevent (a `git add -A` commit would still have caught them here, but the scope-drift
guard reads the diff, and an untracked file is invisible to it). Both were re-added with `git add -N`
before the final Layer-3 measurement. Every file mtime in the tree is `19:00:14`, so mtime evidence — the
tool session 2 used to prove `proto/buf.yaml` was not its own authorship — is destroyed by the restore and
was **not** relied on this session; the two commits above are dated in git instead.

### The wire break was measured a fourth time, independently, with the carve-out lifted

The `breaking.ignore` carve-out destroys the detection the §5 declaration was built on, and the tree had
just been restored by an external process, so the break was re-measured rather than carried forward:
`proto/buf.yaml` copied aside, the `internal.proto` line removed, `buf breaking proto --against
.git#ref=0216eab9…,subdir=proto` at the same base ref the pipeline resolves, then restored and confirmed
**byte-identical by sha256** (`afa6095c…` before and after).

**Exactly 11 findings — 8 `MESSAGE_NO_DELETE` (`CascadeDestination`, `RegisterRequest`, `RegisterResponse`,
`RouteMediaRequest`, `RouteMediaResponse`, `RoutingOptions`, `StreamTelemetryRequest`,
`StreamTelemetryResponse`) + 3 `RPC_NO_DELETE` (`Register`, `RouteMedia`, `StreamTelemetry`) — symbol for
symbol identical to the §5 declaration for the fourth independent time.** Zero fired-but-unpredicted, zero
predicted-but-not-fired, **zero `FIELD_*`**. The `FIELD_* = 0` row is the mechanical proof that no tag was
repurposed (@operations OPS-2) and, this session, also the proof that the restore did not alter the diff.

The base ref is unchanged at `0216eab9` (`BASE_SOURCE=local-mergebase`) despite the two new commits — they
are on the branch, not the base — so this measurement is directly comparable to the previous three.

### Gate 2 — final measurement (unattended, all seven layers, no `NOT-RUN`)

Re-run after this session's documentation edits, so the verdict does not predate the tree it describes
(`/tmp/layer-all-final.log`):

```
LAYER=1 RESULT=OK  DURATION=5    buf-build + cargo-build + dt-guard + dt-story + nx-typecheck
LAYER=2 RESULT=OK  DURATION=2    buf-format + cargo-fmt + nx-format
LAYER=3 RESULT=OK  DURATION=20   guards-passed  (was the sole blocker; green since 28defd8)
LAYER=4 RESULT=N/A DURATION=203  cargo-test-passed + nx-test-passed; N/A = proto intentional-gap placeholder
LAYER=5 RESULT=OK  DURATION=6    buf-lint + cargo-clippy + nx-lint
LAYER=6 RESULT=N/A DURATION=2    cargo-audit-passed; pnpm SKIPPED-NO-DIFF no-dep-changes; buf-breaking-passed
LAYER=7 RESULT=OK  DURATION=264  env-tests-passed + browser-e2e-passed
TOTAL_DURATION=502 TOTAL_RESULT=N/A
```

**`TOTAL_RESULT=N/A`, not `OK`, and that is the pass condition here — stated explicitly so it is not read
as a hedge.** `layer-all.sh` aggregates the *worst* child STATUS, and `N/A` outranks `OK` in that ordering,
so a single intentional-gap placeholder colours the total. The N/A on layers 4 and 6 is
`REASON=not-applicable-to-this-lang` from proto's deliberately-absent `test.sh`/`audit.sh` — the documented,
self-justifying case (ADR-0033 §6), and the identical shape the previous two sessions measured. Every
executed child is green: `cargo-test-passed`, `nx-test-passed`, `cargo-audit-passed`, `buf-breaking-passed`,
`guards-passed`, `env-tests-passed`, `browser-e2e-passed`. **There is no `FAIL` and no `NOT-RUN` in the
run**, which is the condition Headless Mode rule 1 turns on; `TOTAL_RESULT=FAIL` is what session 2 held on
and it is gone. Wrapper exit status was 0.

Layer 6's `SUPPRESSED=dark_tower/signaling/v1/signaling.proto,dark_tower/internal/v1/internal.proto` names
both protos on the run, so the disabled gate cannot read as a silent green — and the fourth measurement
above is what stands behind that green. Layer 4's session-1 sqlx flake has now not recurred across four
consecutive full runs. The `N/A` on layers 4 and 6 is the documented intentional-gap placeholder
(`REASON=not-applicable-to-this-lang`, ADR-0033 §6), self-justifying per the skill; it is **not** a
`NOT-RUN`, and no layer in this run is.

### Why this commits, where session 2 did not

Session 2 held on `TOTAL_RESULT=FAIL` and refused to read the user's silence on a documented red as tacit
acceptance. That reasoning is unchanged and still correct; what changed is that the human answered both
questions in code. **No `FAIL` and no `NOT-RUN` layer remains** — so there is no gate to relax and no risk
to self-accept. No reviewer verdict was reopened, because the reviewed diff is unchanged: this session
touched only `main.md` (row 23, the §Accepted Deferrals pointers, this section) and one `docs/TODO.md`
entry's checkbox. All ten Gate-3 verdicts stand as recorded.

## Rollback Procedure

1. Start commit: `d32ccecfb7b8ddda6329be2a083d63685a11923b`
2. `git diff d32ccec..HEAD`
3. `git reset --hard d32ccec`

**Why a bare reset is honest for this commit specifically** (@operations OPS-5): there is no deployed
artifact (no image build, no rollout), no database migration, no configuration change, and no
persisted state written in either direction. The change is a wire contract plus its regenerated
bindings and the consuming edits that keep the workspace compiling; reverting the commit restores the
previous contract exactly. This sentence is scoped to THIS commit — it stops being true at story
tasks #5/#6, when MH gains a policy-driven forward path and MC begins pushing real policy.

**What a revert does NOT restore correctly (@operations OPS-12).** Two documentation corrections in this
commit are true **independently of whether the reshape lands**, and a `git reset --hard` silently
re-introduces both errors. This matters concretely rather than theoretically: Layer 6 is red by design and
heading for human escalation, so if the break is rejected, `git reset --hard` is the actual next command
and this section is the one place someone reads at that moment.

1. **ADR-0036 Appendix (row 19b).** The "credential-leak guard covers KEK and transmit-key material in
   internal messages" clause was false *the day it was written* — `dt-guard` has never had a `.proto`
   scanner. It would be false again after a revert. Restoring it is **not neutral**: by the correction's
   own reasoning, a false coverage claim does not merely misinform, it **redirects reviewer attention**,
   which is what makes the gap durable.
2. **ADR-0003 (row 12b).** Partly reshape-caused, partly not. `connection_token`'s client-facing half was
   deleted on 2026-04-13, so ADR-0003 was already stale on that half before this commit touched anything.
   A revert restores the server-half description correctly and the **client-half description
   incorrectly**.

Both are tracked in `docs/TODO.md`. **Re-land them as a standalone documentation commit** rather than
leaving the revert to carry them back.

---

## Issues Encountered & Resolutions

| Issue | Resolution |
|---|---|
| `clippy::doc_lazy_continuation` on generated bindings | prost turns proto comments into rustdoc; two bullet lists in `SubscriberSlot` were followed by a paragraph with no blank line. **Fixed in the `.proto` source**, not with an `#[allow]` on generated code. General constraint for prose-heavy proto comments in this repo, and the error points at a `target/` path rather than at the fix. |
| `clippy::expect_used` in the new roundtrip test | Adopted the sibling `signaling_roundtrip.rs:14-18` posture verbatim with its rationale (in a wire-shape test a failed decode IS the assertion mechanism). Copied, not invented, so it reads as the file class's convention. |
| Gate-1 classification guard passed vacuously | Table rebuilt in canonical form; see §Devloop Verification Steps. |
| `validate-subdomain-regex-sync` red | Proven pre-existing on a clean tree; tracked; deliberately not masked. |
| Plan rows 9 and 16 not implemented | @media-handler's owner ruling moved them to story task 5 after Gate-1 confirmation, so the method-label enumeration reduces atomically with its executable pin. Removed from the classification table so plan and diff agree; recorded in §8's handoff. |

---

## Lessons Learned

**A constraint recorded only by the party currently honouring it is safe exactly until that party's
implementation changes.** This appeared three times at Gate 1 and each time the fix was to move the
constraint into the artifact the future reader is obliged to consult — the proto comment, not this
file. OPS-11 (the generation-0 reject is safe only while MC has not shipped real generations);
@media-handler F1 (the reject-vs-hold cut is safe only while the reader remembers the promotion model);
the all-edges-or-none apply clause (safe only while apply is build-complete-then-swap — and the
concrete future is not someone ignoring the rule, it is someone optimising to incremental per-edge
apply as a *performance* change nobody would route past a correctness constraint).

**A count in a guard's OK line is a checkable assertion.** `clean-1-files` on a 21-row table was the
Gate-1 tell that the classification guard could not parse my table, and it went unread by everyone
including me. This is the same failure as F-OBS-4 (one enumeration in two homes, drifted both ways
under active review) one layer out: the artifact said something specific and nobody compared it to
what they knew.

**Four of the six findings that changed this contract were defects in my own draft**, and one of my own
*fixes* (the F1a over-correction) would have introduced a new false-alarm bug into the field that
exists to prevent false liveness. Recording each in place with attribution — rather than quietly
reshaping — is what let @media-handler catch the over-correction before it landed.
