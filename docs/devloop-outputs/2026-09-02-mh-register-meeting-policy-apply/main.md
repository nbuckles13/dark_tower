# Devloop Output: MH RegisterMeeting applies forwarding policy; applied-generation echo

**Date**: 2026-09-02
**Task**: Implement the MC→MH control plane per ADR-0036 §8 in `crates/mh-service` — `RegisterMeeting` applies forwarding policy through a config-apply mailbox, publishes an ArcSwap snapshot for the data plane, echoes the ACTUALLY-APPLIED generation, and the dead-RPC `method` metric-label values are collapsed to `register_meeting`.
**Specialist**: media-handler
**Mode**: Agent Teams (v2)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: two sessions (interrupted after Gate 2 attempt 2; resumed at Gate 3)

Story: `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` task 11 (headless run; `DEVLOOP_HEADLESS=1`).

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ba102ec8d971769fc550ae21fc7339f1dceb56a9` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (media-handler, opus) |
| Implementing Specialist | `media-handler` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Meeting Controller (conditional) | `meeting-controller` |

---

## Task Overview

### Objective
Make `RegisterMeeting` the real MC→MH control plane (ADR-0036 §8): apply the request's `EgressStream` forwarding policy via a config-apply mailbox that is SEPARATE from the bounded connection-lifecycle mailbox, publish a lock-free snapshot for the data plane, and echo `applied_generation` = the generation actually installed (never the received one). Enforce generation monotonicity and re-assert idempotency; echo `transport_mode` and fail loud on mismatch; feed `mh_media_policy_applies_total{status}`. Collapse `mh_grpc_requests_total{method}` to the single value `register_meeting`.

### Scope
- **Service(s)**: mh-service (MH). Response-field semantics coordinated with meeting-controller (task 13 consumer).
- **Schema**: No
- **Cross-cutting**: Yes — observability (new metric + catalog), meeting-controller (contract consumer semantics)

### Debate Decision
NOT NEEDED — ADR-0036 §8 and the landed `internal.proto` reshape (task 4, commit `1a…`/`docs/devloop-outputs/2026-09-01-internal-contract-reshape/`) already fix the contract and its normative rules.

### Task-numbering note (read before following proto comments)
`proto/dark_tower/internal/v1/internal.proto` and `docs/TODO.md` §Media Path Obligations say **"story task 5"** for the MH side and **"story task 6"** for the MC side. Those refer to *this* devloop (story task **11**) and to story task **13** respectively. Every "Enforcement owner: story task 5" in that proto is an obligation of THIS devloop.

---

## Cross-Boundary Classification

| Path | Classification | Owner | Change |
|------|----------------|-------|--------|
| `crates/mh-service/src/routing/mod.rs` | **Mine** | — | NEW — scoped-identifier newtypes, `MeetingPolicy` validation, `RoutingSnapshot`, `RoutingTable` (`ArcSwap`). Holds the two-meeting cross-tenant acceptance pin |
| `crates/mh-service/src/process.rs` | **Mine** | — | NEW — `sample_process_start_epoch_ms`, called once from `main` |
| `crates/mh-service/src/session/mod.rs` | **Mine** | — | Second `mpsc` into the same actor, `select!` over both, `CONFIG_APPLY_CHANNEL_BUFFER`, `ApplyOutcome`/`ApplyFailure`, `apply_policy`, `routing_snapshot`, `new_with_parts`, mailbox-capacity accessors |
| `crates/mh-service/src/grpc/mh_service.rs` | **Mine** | — | `register_meeting` applies policy and echoes the applied generation; `Default` deleted; the §10 Tier-1b gates |
| `crates/mh-service/src/config.rs` | **Mine** | — | `PolicyLimits` + four optional-with-default knobs, each with a hard ceiling and reject-not-clamp parsing |
| `crates/mh-service/src/observability/metrics.rs` | **Mine** | — | `record_media_policy_apply` + `PolicyApplyOutcome`; `record_grpc_request` loses its `method` parameter; label enumerations become catalog pointers |
| `crates/mh-service/src/observability/mod.rs` | **Mine** | — | Seven-metrics-stale table replaced by a catalog pointer; new re-exports |
| `crates/mh-service/src/lib.rs` | **Mine** | — | `pub mod process; pub mod routing;` and the crate-status paragraph |
| `crates/mh-service/src/main.rs` | **Mine** | — | Samples the process epoch once, threads `handler_id`/`policy_limits`, logs the four bounds, sets `max_decoding_message_size` explicitly |
| `crates/mh-service/Cargo.toml` | **Mine** | — | `arc-swap` workspace dependency |
| `crates/mh-service/tests/policy_apply_integration.rs` | **Mine** | — | NEW — metric label set, no-unbounded-label assertion, and the counting-boundary denominator invariant driven through the real handler |
| `crates/mh-service/tests/errors_grpc_metrics_integration.rs` | **Mine** | — | Method matrix collapsed to one value; `status` adjacency preserved; retired values asserted dead |
| `crates/mh-service/tests/common/grpc_rig.rs` | **Mine** | — | `MhMediaService::new` signature |
| `crates/mh-service/tests/gc_integration.rs` | **Mine** | — | One field added to a `Config` literal |
| `docs/specialist-knowledge/media-handler/INDEX.md` | **Mine** | — | Twelve new entries; twelve reclaimed by removing three verbatim-duplicate Integration Seams rows and merging four same-file pairs. Net zero against the 75-line cap |
| `infra/grafana/dashboards/mh-overview.json` | **Mine** (ADR-0031 assigns `<svc>-overview.json` to the service specialist) | — | One panel, purely additive (93 insertions, 0 deletions). Required: `metric_no_dashboard` has no allowlist |
| `Cargo.toml` | **Mine** (Lead ruling 1) | — | `arc-swap = "1.9"` in `[workspace.dependencies]`. Already in `Cargo.lock` transitively via `redis`; @security cleared the supply chain, and the Layer-6 dep-change gate must **fire** |
| `crates/common/src/observability/labels.rs` | Not mine, **Minor-judgment** | observability | NEW — `KEY_CUSTODY_LABEL` / `KEY_CUSTODY_OPERATOR` hoisted at their pre-registered second-consumer trigger, substance verbatim, executed promotion instruction retired per the Lead's ruling |
| `crates/common/src/observability/mod.rs` | Not mine, **Minor-judgment** | observability | `pub mod labels;` + one doc bullet |
| `crates/mc-service/src/observability/metrics.rs` | Not mine, **Minor-judgment** | meeting-controller | Two const definitions → one `pub use`. Zero call-site churn; wire output byte-identical |
| `docs/observability/metrics/mh-service.md` | Not mine, **Domain-judgment** | observability | `mh_media_policy_applies_total` catalog entry incl. the task-11→13 window; `mh_grpc_requests_total` method label corrected to single-valued |
| `docs/observability/label-taxonomy.md` | Not mine, **Minor-judgment** | observability + security | §Shared Label Names gains the missing `outcome` row and the `status`-vs-`outcome` guidance; §R2's first sentence at `:287` restated on the observable, `:288-291` untouched |
| `docs/runbooks/mh-incident-response.md` | Not mine, **Domain-judgment** | operations | One note inside existing Scenario 13. No new numbered scenario, cites by heading, enumerates no `outcome` values |
| `docs/TODO.md` | Not mine, **Minor-judgment** | security + observability (a), operations (b), test (c) | (a) split not closed, (b) and (c) updated; two new entries supplied verbatim by @operations |
| `crates/mh-test-utils/src/media_policy.rs` | **Mine** | — | ADDED AT GATE 3 (@dry-reviewer F3). NEW — the single home for the `EgressStream` / `RegisterMeetingRequest` fixture builders that previously had three |
| `crates/mh-test-utils/src/lib.rs` | **Mine** | — | ADDED AT GATE 3. One `pub mod media_policy;` line |
| `crates/mh-test-utils/Cargo.toml` | **Mine** | — | ADDED AT GATE 3. A `proto-gen` path dependency for the fixture builders. media-handler owns `mh-test-utils`; **no `[workspace] members` edit** — the crate already existed at `ba102ec8` — so Lead Ruling 1's ADR-0034 infrastructure counter-example is not engaged. This path is a direct `audit_dep_changed_rust` trigger, so **Layer 6 must be allowed to fire** — a gate is never skipped on a predicted result. Its expected outcome is a trivial pass: `proto-gen` is an in-tree workspace member already in the build graph, so there is no new registry crate and no new supply-chain surface, and unlike the `arc-swap` promotion there is nothing here for `cargo audit` to clear. **Both halves must travel together** — the first sentence alone implies unassessed risk, the second alone invites skipping the gate. Edge direction, because it is easy to invert (@security and I both did): `mh-test-utils` takes `proto-gen` as a **regular** `[dependencies]` edge, and it is `mh-service` that reaches `mh-test-utils` through `[dev-dependencies]`. The apparent cycle is what induces the misread. The conclusion strengthens rather than survives: the edge sits outside the production binary graph entirely and materialises only when building tests |
| `docs/runbooks/mh-deployment.md` | Not mine, **Domain-judgment** | operations | ADDED AT GATE 3 (OPS-3). The R-36 bake gate's policy-apply arm at the 30-min / 2h / 24h tiers. Query and framing supplied verbatim by @operations, who offered to make the edit themselves; the 24h tier and its ratchet rationale are mine and are called out for their confirmation |
| `docs/observability/dashboards.md` | Not mine, **Domain-judgment** | observability | ADDED AT GATE 3 (@observability F6). §MH Overview's five-panel enumeration replaced by a pointer plus a note recording that all five were fiction; forward-references the §AC Overview sibling filed under `docs/TODO.md` §Observability Debt. @observability to confirm the classification and the landed text |
| `docs/specialist-knowledge/observability/INDEX.md` | Not mine, **Minor-judgment** | observability | ADDED AT GATE 3 (@observability F3 / @dry-reviewer F5). Line 29 repointed from MC's now-re-exporting `metrics.rs` to `common/observability/labels.rs`; the new metric folded into the MH line. Net zero against the 75-line cap |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` | Not mine, **Minor-judgment** | dry-reviewer | ADDED AT GATE 3 (@dry-reviewer F6, wording pre-ACKed by them). Three fold-ins at lines 19 / 32 / 71: the anchor-vs-guard instrument test, the `common` label-vocabulary home, and task 11's four deliberate non-collapses. Net zero against the 75-line cap |

### Lead rulings at Gate 1

**Ruling 1 — root `Cargo.toml` is `Mine`.** This is a **Lead classification ruling at Gate 1, not a reviewer downgrade** — ADR-0024 §6.2 monotonicity binds reviewers, and @operations correctly declined to touch it and routed it here. Grounds: root `Cargo.toml` is in no key of `cross-boundary-ownership.yaml` and a workspace dependency addition matches none of the four §6.4 criteria; no manifest, ADR or INDEX assigns it to infrastructure (CLAUDE.md scopes that specialist to K8s/Terraform/IaC/CI-CD, and a Rust build-graph edge is none of those); and it is settled precedent, not a novel call — `2026-06-21-gc-telemetry-proxy/main.md:96-102` states it in terms, with the same posture at three other devloops. The one infrastructure-owned counter-example is the workspace **`members`** array adding infrastructure's own crate per ADR-0034, a different edit to a different table. **@security's and @operations' conditions stay attached**: supply-chain clearance already given with evidence (already in the build graph via `redis 0.26.1`, checksum-pinned at `Cargo.lock:151`, one trivial transitive dep, zero new crates), and the Layer-6 dep-change gate must **fire** rather than be skipped.

**Ruling 2 — the two Domain-judgment doc rows are discharged in this devloop.** @observability's and @operations' participation *is* the `--paired-with=<owner>` overlay in substance: @observability specified the label key, all five `outcome` values and their remedy mapping, the gen-0 window prose and the SSoT sentence; @operations pre-ACKed the runbook prose with four content constraints. The spin-out alternative is self-defeating — `metric_no_catalog` has no allowlist, so deferring the catalog entry lands this devloop with Layer 3 red or drops the metric, routing around a gate rather than honouring it. **Two mandatory conditions each**: the owner confirms the **exact landed text** at Gate 3 via their Ownership Lens verdict (not a generic approval), and the commit carries `Approved-Cross-Boundary: <owner>` with a reason clause naming the authority (ADR-0011 / ADR-0031 metric taxonomy for the catalog).

**Ruling 3 — the R2 amendment at `label-taxonomy.md:287` is IN SCOPE.** The defect is live and *this devloop's own diff is being reviewed against the rule it defects*. One sentence, zero LoC, zero test surface, no design ambiguity, both required owners seated with trailers pre-committed — the suspicious-deferral check flags all three conditions, so deferral would fail the burden of proof; and the file is already in the changeset for the `outcome` row, so it is not a new surface. **Two conditions**: substitute **only** the first sentence at `:287`, leaving `:288-291` intact so the "described below" pointer still lands on the voice-activity-trace text; and the `docs/TODO.md` task-16 forward-carry **cites** R2 for exemplars and the low-occupancy rule rather than restating them — fixing a drift by creating a second copy would be self-defeating. Both trailers on the commit, each reason-clause scoped to that one sentence.

**Ruling 4 — Q7 was @meeting-controller's call, and they have RULED: ACK, preferred branch, trailer granted.** MC takes the two-line `pub use`; the fallback is dead. Their grounds: "zero mc-service files" was never a blanket veto but "no *unowned* MC edits bleeding in from MH's devloop", and their own confirmation said a named row would be brought to them — so the constraint's escape hatch fired as designed. The trigger is MC's own, recorded in MC's own code, so honouring it is respecting MC's decision rather than MH imposing on MC. And **the fallback would have violated CLAUDE.md's Single-Source-of-Truth principle head-on**: two live definitions of one value until task 22's distant fleet-wide scope, recreating one level up the exact drift the trigger was written to prevent. They verified value-neutrality themselves — qualified use at `actors/meeting.rs:454` resolves through the re-export, unqualified uses at `metrics.rs:552` stay in-module, `media_admission_integration.rs:232` asserts the unchanged string literals, no other tree consumer — and `mc_meeting_kek_generated_total{key_custody="operator"}` is byte-identical on the wire.

**Two conditions on the ACK, both binding on me:**
- **(A) The stale promotion INSTRUCTION must not travel verbatim into the new home.** "Promote to `crates/common/src/observability/` at the SECOND consumer; MC is the first emission site in the tree" is an instruction that is now *executed*; leaving it in `common/labels.rs` would tell a future reader to promote something already promoted and muddy whether `common` is really the home. Reworded to reflect that the promotion happened (or dropped, keeping the substantive docstring about what `key_custody=operator` *means*). The "Fleet-wide rollout is R-26 (observability task 22)" clause is still forward-accurate and stays. **This is the one place where "move the full docstrings verbatim" (@observability/@dry-reviewer) and coherence conflict, and the resolution is: move the substance verbatim, retire the executed instruction.** Final wording is @observability's/@dry-reviewer's to finalise.
- **(B)** I am ACKed only for the **definition-site relocation** of MC's symbol. The label *value* semantics (accepted operator custody, one permitted value, in-place-of an E2E/zero-trust boolean) are @observability's and @security's content and are unchanged.

A third home inside `mh-service` was ruled out by everyone throughout.

---
**Not touched, deliberately**: `proto/**` and `crates/media-protocol/**` (Guarded Shared Areas, ADR-0024 §6.4) — the contract reshape already landed at task 4 and this task consumes it unchanged. All three width constants I derive from (`KEY_ID_SENDER_ID_BITS`, `KEY_ID_STREAM_BITS`, `STREAM_ID_FIELD_BYTES`) are already `pub`, so the `const _` asserts force no GSA edit.

**`crates/common/src/observability/labels.rs` is NOT a Guarded Shared Area** — ADR-0024 §6.4 enumerates only `common/src/{jwt,meeting_token,token_manager,secret}.rs` and `common/src/webtransport/**`, and a label vocabulary matches none of the four criteria (wire-format runtime coupling, auth-routing policy, detection/forensics contract, schema evolution). Minor-judgment with an observability trailer, not owner-implements.

**`crates/mc-service/**` is one 2-line mechanical row, conditional on Q7**, and only because MC's own code authorized it in writing at this exact trigger. If @meeting-controller holds at zero files, the fallback is: promote to `common`, MH imports from `common`, MC keeps its definitions, and a `docs/TODO.md` entry records that **the trigger has already fired** and MC's copy is the one outstanding migration.

`Cargo.lock` changes with the `arc-swap` promotion and has no row (generated). Per @security: the layer-6 audit dep-change gate must be allowed to **fire** on that promotion rather than be skipped — a new direct dependency is exactly what it exists to catch.

---

## Planning

### 0. Mechanism restatement (requested by the workflow), and two contradictions it exposes

**Instance-language** (the task's own nouns): "MH must not build a global `sender_id` → connection index."

**Mechanism-language**: *every identifier on this contract is an ordinal scoped to an enclosing entity, and MH must be structurally incapable of resolving one outside its scope.* The restated mechanism is wider than the task's noun and forbids it:

| Identifier | Enclosing scope | Normative source |
|---|---|---|
| `sender_id` | the meeting | `internal.proto::SubscriberSlot.sender_id` |
| `slot_id` | one subscriber's connection | `signaling.proto::ReceiveSlot.slot_id` ("keyed on (subscriber, slot_id), never `slot_id` alone") |
| `egress_stream_id` | one registration | `internal.proto::EgressStream.egress_stream_id` |
| `stream_number` | one sender | `signaling.proto::SendStream.stream_number` |

All four siblings are same-owner (media-handler) and all four land in *this* changeset. So the plan applies **one** discipline rather than four rules: **the routing snapshot is a nest of maps, and no map anywhere in it is keyed on a scoped ordinal alone.** `HashMap<MeetingKey, MeetingRoutes>` → `MeetingRoutes { by_sender: HashMap<SenderId, …> }` → edges keyed `(subscriber_sender, slot)`. There is no type in the snapshot that can be reached with a `SenderId` and no `MeetingKey`, and the only public lookup is `fn sources_for(&self, meeting: &MeetingKey, sender: SenderId)`. The wrong index is then a *missing function*, not a rule someone has to remember — which is what the TODO entry asks for ("unconstructible without a `MeetingId`").

**Contradiction 1 — the (a) acceptance pin, as worded, is unimplementable without the thing it forbids.** `docs/TODO.md` (a) says "a `sender_id` valid in meeting A must be REJECTED when it appears in meeting B's `RegisterMeetingRequest`." To *reject* on that basis MH must first know that the id is valid in meeting A — i.e. consult a cross-meeting index. **The only way to satisfy the pin as literally worded is to build the global index the same entry forbids.** The proto resolves it and I am following the proto: "*This falls out of the scoping rule rather than needing its own check, which is the point: if you only ever look inside one meeting, you cannot cross meetings.*" So "rejected" must be read as **"does not resolve"**, not "the RPC returns an error". Pinned as: a genuine two-meeting fixture where meeting A's snapshot resolves sender 5 to A's edges and `sources_for(meeting_B, sender_5)` returns **empty** — a test that fails loudly against a flat `HashMap<SenderId, _>` implementation and passes against the nested one. **@security / @test: please confirm this reading before I implement it**; it is the one place I am departing from a TODO entry's literal words, and I would rather have it on the record than quietly reinterpreted.

**Contradiction 2 — the same-generation re-assert can carry different content.** §8 guarantees "unchanged policy carries the same number", so MH may no-op on an equal generation. Nothing makes MC's side of that a *checkable* property. I will no-op (as mandated) **and** emit a loud `WARN` if an equal-generation re-assert's policy content differs from what is installed — a free contract-violation detector that changes no data-plane behaviour. No new metric label value.

---

### 1. Structural spine

**New module `crates/mh-service/src/routing/mod.rs`.** Domain types + the published snapshot. Nothing here knows about proto or gRPC.

- `SenderId(NonZeroU16)` — **fresh MH-local newtype**, never `common::types::UserId` (globally-scoped semantics the reshape deletes) and never `common::types::StreamId` (wrong width, pre-ADR-0036 semantics). `NonZeroU16` so `0` — the reserved-invalid value — is excluded by the type, matching MC's `media_admission::sender_id::SenderId`. Width derived, not restated: `const _: () = assert!(KEY_ID_SENDER_ID_BITS == 16)` against `media_protocol::frame::KEY_ID_SENDER_ID_BITS`, **no `65535` / `u16::MAX` / `1 << 16` literal anywhere in MH**.
  - *Home decision (@dry-reviewer's question)*: **MH-local, not hoisted to `crates/media-protocol`.** `media-protocol` is the wire codec — it owns the key-id *layout*, and it already exports the width both homes derive from, which is the SSoT that matters. A control-plane domain newtype with an allocator invariant on one side (MC: never-recycled, allocator-only constructor) and a validation invariant on the other (MH: parse-and-reject inbound) is not one type wearing two hats; hoisting would force `media-protocol` (a GSA) to grow a construction policy it has no business holding, and would need protocol + MH + MC co-sign for no reduction in the thing that can actually drift (the width). I will land the "why not shared" boundary comment at the definition, matching how task 10 recorded its boundaries.
- `SlotId(u16)` — width derived from `media_protocol::frame::STREAM_ID_FIELD_BYTES` (= 2) by `const _` assert. **Zero is valid** here (the proto's own example is "two subscribers both choosing slot 0"), which is exactly why this is a separate type from `SenderId` and why I will **not** introduce a shared `MAX_16BIT_ID` or `validate_16bit_id()` — `docs/specialist-knowledge/dry-reviewer/INDEX.md:70` records that boundary and the proto restates it. Reject, never truncate.
- `MeetingKey(Arc<str>)` — MH's meeting ids are `String` on the wire and in `SessionState`; `common::types::MeetingId(Uuid)` would force a parse MH has no reason to do.
- `EgressEdge { egress_stream_id, subscriber: SubscriberSlot{sender, slot}, candidates: Vec<CandidateRef>, priority_group, supersede_on_independent_frame }`.
- `MeetingRoutes { generation: u64, transport_mode, edges: Vec<EgressEdge>, by_sender: HashMap<SenderId, Vec<usize>> }` — `by_sender` is the ingress-side lookup the forward path (task 16) will use.
- `RoutingSnapshot { meetings: HashMap<MeetingKey, Arc<MeetingRoutes>>, total_edges: usize }`, published behind `ArcSwap`. **`Arc<MeetingRoutes>` per @security S5** (offered as optional; taking it because it is free): a single global snapshot covering all meetings would otherwise make every per-meeting apply rebuild the whole map — at §8's designed load that is control-plane-rate x O(N) allocation, an amplification factor the cadence supplies without an adversary having to trigger it. With `Arc`s the rebuild clones pointers, not edges, and the `Arc::ptr_eq` idempotency observable survives at **both** levels (snapshot and per-meeting).
- **@security S6 — `by_sender`'s `usize` indices are a latent cross-meeting pointer, and the type system does NOT stop it.** C3's "the wrong index is a missing function" is true of `SenderId` and **false of `usize`**: an index carries no scope in its type, and today it is meeting-local only because `edges` is *contained* in `MeetingRoutes`. The plausible forward hazard is the obvious per-frame cache-locality optimisation — flatten all meetings' edges into one arena and have `by_sender` index it — at which point every `usize` becomes a global handle into a cross-meeting array, rebuilding the exact primitive this task exists to prevent, **reached through a performance change nobody would think to route past a correctness constraint.** Same shape the proto flags for all-edges-or-none ("incremental per-edge application is the obvious optimisation once meetings are large"). Fix is a comment plus discipline, not a redesign: the invariant stated in words at the `by_sender` field definition (*indices index into **this** `MeetingRoutes.edges` and nothing else; a shared or flattened cross-meeting edge arena is forbidden*), and `sources_for` resolving through `self.edges.get(i)` on the **meeting-local** slice — which the workspace's `indexing_slicing = "deny"` already forces.
- **@security C3 — the structural claim holds at the type level, not by convention.** `RoutingSnapshot.meetings`, `MeetingRoutes.by_sender` and `MeetingRoutes.edges` are all **private**; `sources_for(&self, &MeetingKey, SenderId)` is the only public lookup. A `pub by_sender` would degrade "unconstructible without a `MeetingId`" back into the convention the TODO entry set out to eliminate. Module-head comment names the invariant explicitly.

**Snapshot mechanism: `arc-swap`, and why not `tokio::sync::watch` (@dry-reviewer's question).** `watch` is the in-tree idiom for *notification* (`token_manager.rs` — waiters need to learn a token rotated). The forward path wants the opposite: "hand me the current table, never block, never await, never track seen-ness", on every frame. `watch::Receiver::borrow()` returns a guard holding an internal `RwLock` read — held across a forwarding decision it blocks the publisher, and it needs a per-reader `Receiver` clone with `changed()` state nobody reads. `ArcSwap::load_full()` is a plain atomic returning an `Arc` with no guard lifetime. It also gives @test the observable idempotency needs: `Arc::ptr_eq` across a re-assert is a direct "did a swap happen" assertion, which `watch` cannot express. `arc-swap 1.9.2` is already in `Cargo.lock` transitively; this promotes it to a direct workspace dependency.

---

### 2. Config-apply mailbox (ADR-0036 §8; @code-reviewer 2, @operations O-10, @security 4)

**One actor, two mailboxes.** `SessionManagerActor` gains a second `mpsc::Receiver<ConfigApplyMessage>` and its `run()` loop becomes a `tokio::select!` over both. Not a second actor and not a second handle type (@dry-reviewer 5) — the actor stays the single owner of all state, so there is no lock and no ordering hazard between the two mailboxes.

- Lifecycle mailbox: **unchanged**, `SESSION_CHANNEL_BUFFER = 256`.
- Config-apply mailbox: **bounded**, its own `CONFIG_APPLY_CHANNEL_BUFFER` with a calibration comment in the same style — sized from the §8 workload (one full-snapshot message per meeting per ≤10 s cadence), deliberately *not* shared with the lifecycle bound, which was sized for one-shot-per-meeting registration.
- **Never `send().await`**: the handler uses `try_send`. On full → no apply, `outcome=apply_failed`, loud `WARN`, and the echo is read off the live snapshot (below). Never blocks, never silently drops.
- The oneshot reply is awaited under `tokio::time::timeout` (config knob, default 1000 ms) — **no unbounded await** (@operations O-3). Timeout → same treatment as mailbox-full.

---

### 3. The echo is structurally incapable of being the received generation

This is the core anti-pattern defence (@code-reviewer 1, @test's watch-out, @security 5, @meeting-controller 3). Rather than a rule ("do not write `applied_generation: req.policy_generation`"), the value has **one source**:

```
applied_generation = live_snapshot.load().generation_for(&meeting_key).unwrap_or(0)
transport_mode     = live_snapshot.load().transport_mode_for(&meeting_key).unwrap_or(Unspecified)
```

read from the `ArcSwap` **after** the apply attempt resolves — on every path, including mailbox-full, timeout and apply-failure. `req` is not in scope at the point the response is built. Echo-on-receipt is therefore not a one-line mistake; it is a value with no wire from the request to the response.

**This also disposes of @observability's hardest case for free, which is the argument for the shape.** A meeting at applied generation N receiving a `policy_generation: 0` re-assert (what an MC rolled back to a pre-reshape build sends) must answer `outcome=no_generation` *and* `applied_generation: N`. The status answers "what did I do with this request"; the echo answers "what is live"; they must come from **different sources**. Here they structurally do — the status comes from the apply decision, the echo from the snapshot. A coupled implementation is indistinguishable from a correct one in every test written in the pre-task-13 window (nothing has ever been applied, so the echo is legitimately 0) and diverges only during a rollback, manufacturing a false divergence alarm on MC's side at the exact moment its operator is handling something else. Pinned by test 13.

Consequences that fall out for free and that reviewers asked for:
- `applied_generation == 0` stays reachable and is today's truthful value (@meeting-controller 3).
- A stale/failed apply truthfully reports the *prior* generation (@operations O-3, @code-reviewer 3).
- `transport_mode` echoes what was applied, `UNSPECIFIED` when nothing is applied — never fabricated (@meeting-controller 4b).
- **Loud mismatch log** when the request's declared (homogeneous) mode ≠ the mode on the live snapshot for the echoed generation.

---

### 4. Validation: three tiers, and where each one leaves the system

**Tier A — structural rejects. Evaluable from the request alone, no external state, immediate hard reject, whole registration, NO state mutation at all** (no upsert, no promotion, no apply, no snapshot swap) — same shape as the handler's existing field validations, which already return before touching `SessionManagerHandle`. Returns `Status::invalid_argument` with a **generic, bounded** message that never echoes attacker-supplied values (@security 10).

**The validate-before-mutate fork is resolved explicitly, not by where the code was easiest to write** (@operations O-12's closing requirement). I validate **before** the apply and before any state touch, which is what `internal.proto:232-237` mandates for the bounds ("BEFORE building any routing table — the same posture the existing handler already takes on scalars") and what @operations O-11 endorses. @observability is writing the catalog entry against this choice: `rejected_invalid` therefore means *"MC sent a policy MH refused to look at"*, not *"the apply examined it and failed"*.

**O-11 — a rejected registration never regresses live state.** Fail the *new* policy closed; leave everything installed alone. An already-registered meeting keeps its `MeetingRegistration`, keeps its live ArcSwap snapshot at its prior generation, and keeps its active connections. Because validation is complete before any mutation, there is no window where the old snapshot is half-cleared and then the call bails. Pinned by test 10 below.

**O-12 / @observability's third amendment — the counter increments exactly once per `RegisterMeeting` that reaches the POLICY-BEARING part of the handler, on every terminal path from there onward. Not once per apply-mailbox message, and NOT on the pre-boundary scalar checks.** If it were only touched inside the apply, the two loudest failures in the contract would be the two with no counter, leaving only `mh_grpc_requests_total{status="error"}` — which cannot distinguish an MC policy bug from a malformed `mc_grpc_endpoint`, and at 3am those have completely different remedies.

This buys a **denominator**: `sum(mh_media_policy_applies_total)` is the count of policy submissions MH actually processed, so each status is a share of a whole rather than a floating count nothing normalises — the same argument MC used for counting both arms of `mc_join_identity_key_presence_total`, which names `mc_meeting_kek_generated_total` as the counter-example that got it wrong.

**The boundary, per @observability's correction** (which @operations caught and which reverses their own previous wording — I had already adjusted my draft to the over-reaching version, so this is the one to implement):

> The policy counter covers every terminal path **from the first read of a policy-bearing field onward** — `egress_streams`, `selection_rules`, `policy_generation`. Checks reading only the caller-identity and reachability scalars (`meeting_id`, `mc_id`, `mc_grpc_endpoint`) are **pre-boundary** and stay on `mh_grpc_requests_total{status="error"}` alone.

So the **seven existing early returns get no policy-counter increment** and are left exactly as they are. Folding them in would have put "MC's assignment computation produced a policy MH won't apply" in the same series as "this caller's `mc_grpc_endpoint` is malformed" — different remedies, different owners. The rule is phrased against *which fields a check reads* rather than against the current code, so an eighth scalar check added later lands pre-boundary automatically, and the three policy checks resolve without a case list (transport-mode homogeneity and the `candidate_sources` bound read `egress_streams` → `rejected_invalid`; the generation check reads `policy_generation` → `no_generation`).

`sum(mh_media_policy_applies_total)` therefore means **"registrations whose policy MH considered"** — a true sentence and a usable denominator. Test 11 pins it in **both directions**: sum == number of calls that passed field validation, **and** at least one malformed call asserted to increment `mh_grpc_requests_total{status="error"}` and **not** the policy counter. Without the negative pin, an implementer making a "sum == all calls" assertion green would either fold the scalar rejects in or delete the malformed case — deciding the boundary by whatever passed rather than by design.

**Ordered per @security S1 — counts FIRST, then everything that iterates.** My draft had the count bounds at positions 5 and 6, after duplicate detection; but duplicate detection builds `HashSet`s sized by the attacker-controlled repeated field, which is the very allocation the bound exists to prevent, just moved earlier than the routing table. The proto's "BEFORE building any routing table" is the floor, not the ceiling. `max_decoding_message_size` (§8) is the outer defence.

1. `egress_streams.len()` > configured per-meeting bound
2. any `candidate_sources.len()` > configured per-egress bound
3. missing `subscriber` submessage
4. malformed `sender_id` (0, or out of 16-bit range) — subscriber side *and* every candidate source
5. `slot_id` out of 16-bit range — reject, never truncate
6. **`stream_number` out of 8-BIT range — reject, never truncate (@security S2).** My §0 mechanism table listed `stream_number`/sender as one of the four scoped ordinals and then my Tier-A list stopped at `slot_id` — a real gap, and it is the same bug class one scope down: a `uint32` on the wire carrying 8-bit semantics, so a stored `stream_number > 255` truncates into the key id's stream field at task 16 and **aliases two distinct streams of the same sender**. Same-meeting media crossing, same mechanism (wide wire type, narrow semantics). Width derived from `media_protocol::frame::KEY_ID_STREAM_BITS`, no `255`/`u8::MAX` literal.
7. duplicate `egress_stream_id`
8. duplicate `(subscriber.sender_id, subscriber.slot_id)`
9. `transport_mode == TRANSPORT_MODE_UNSPECIFIED` on any egress stream
10. heterogeneous transport modes across the registration's egress streams

All three width constants (`KEY_ID_SENDER_ID_BITS`, `STREAM_ID_FIELD_BYTES`, `KEY_ID_STREAM_BITS`) are already `pub` in `media-protocol`, so the `const _` asserts force **no GSA edit**.

**Tier B — holds, not rejects.** A well-formed `sender_id` that does not resolve to a connection *in this meeting* stays in the applied policy and forwards nothing. **It does not hold back `applied_generation`.** Today MH has no `sender_id → connection` binding at all (`SessionState` keys `meeting → participant → connections`), so *every* source is structurally a Tier-B hold; the meeting-scoped `by_sender` index is what will carry the binding when task 16's forward path lands. This is the case §8 relies on for restart recovery and for loopback's own mid-promotion subscriber.

**Tier C — runtime apply failures.** Not evaluable from the request; the registration **is** accepted, the upsert and pending-connection promotion **do** happen, and only the policy install fails (@operations O-4). **Path 1**, no test-only backdoor: the config-apply mailbox is full (`try_send` fails) or the apply reply times out. This is *literally* the failure §8 names ("the mailbox is full or the apply errors"), and it is deterministically drivable — `MH_POLICY_APPLY_TIMEOUT_MS` set small plus `tokio::time` pause/advance, the tree's deterministic-time idiom and the one the story's ordering notes mandate for deadline logic.

**Q3 VETOED by @operations — I proposed a second path and was wrong.** My draft enforced `Config.max_streams` as a total-egress-edge ceiling, on the grounds that it is advertised to GC (`gc_client.rs:140`, `:301`) and enforced nowhere, which is verbatim ADR-0036 §8's cautionary precedent. @operations checked the deployed manifest rather than the default and the evidence is decisive; recording it because the instinct was reasonable and the next person will have it too:
1. **The deployed value is 100; the code default is 1000.** `infra/services/mh-service/configmap.yaml` sets `MH_MAX_STREAMS: "100"`, no manifest supplies the fallback, and the 10x drift is live today — *harmless precisely because the value is inert*. Making it load-bearing gives a capacity gate a threshold that differs by an order of magnitude depending on which manifest happened to be applied. The ConfigMap already flags the pair and hands the collapse to task 12.
2. **The key is scheduled for retirement** (story 2 derives the ceiling from `MH_EGRESS_BUDGET_BPS`), so story 2 would first have to un-make it load-bearing.
3. **The unit is wrong three ways.** The ConfigMap says it is "a BANDWIDTH budget; a stream count is standing in for it"; GC enforces it at placement as `current_streams < max_streams` (`gc-service/src/repositories/media_handlers.rs:302`), a per-handler concurrent-stream count; "total egress edges across all meetings" is neither. Three sites disagreeing about one number does not close "declared in one place, assumed in another, verified nowhere" — it reproduces it with an extra site.
4. **R-22 defers this chain to story 2 by name**, including "derived stream ceiling" and "admission enforcement". Landing them early, in a third unit, ahead of the settled spec, because a test wanted an injectable seam is a bad reason for a capacity semantic to enter the tree.

The ops failure mode on its own terms also kills it: a cap on the total across *all* meetings makes one meeting's re-assert fail because of other meetings' load, and under §8's jittered cadence **the victim rotates**, so the 3am symptom is `apply_failed` moving between meetings with no relation to anything either meeting did — a per-tenant isolation break. The precedent stays recorded honestly in the ConfigMap's "Enforced by: nothing on MH's data path — GC, at placement" line; nothing needs writing down to keep it from being forgotten.

**@security S7 — a NEW aggregate-memory gap that the veto opens, and the fix is the fallback @operations themselves named.** With `max_streams` off the table nothing in the changeset bounds policy memory *in aggregate*: the two per-meeting bounds (512 x 16) bound one meeting, and `session/mod.rs::handle_register_meeting` inserts into `registered_meetings` with **no count cap**, so the total is per-meeting-bound x unbounded meetings. The unboundedness is inherited, but **the cost per element is mine**: today a `registered_meetings` entry is three short strings and an `Instant` — an uncapped map of cheap things, which is why nobody bounded it — and this task hangs a full routing table off each entry, turning it into an uncapped map of expensive things with an attacker-influenced multiplier inside the per-meeting bound.

**Q9 — @operations has RULED: add it, with six conditions.** A fourth knob, `MH_MAX_TOTAL_EGRESS_EDGES`, optional-with-default, identical treatment to the other three. Their veto was against arming `max_streams` *specifically* — all four grounds were properties of that value — and a fresh MH-local knob has none of them. They also conceded their own coupling objection rather than trading it: *"the alternative is not 'no coupling' — it is the same coupling delivered as an OOM kill that takes the pod and every meeting on it with no signal naming a cause. Bounded-loud-one-apply beats unbounded-silent-all-meetings. That is my own blast-radius principle and I had it pointing the wrong way."* The arithmetic: 512 x 16 = 8192 edges per meeting against an uncapped `registered_meetings` map on a pod with a 1 Gi limit, and a registration needs no connections at all to exist.

The six conditions:

- **C1 — framed as a resource-exhaustion guard, never as capacity, never advertised to GC.** Mirror the language `MH_MAX_CONNECTIONS` already carries in `infra/services/mh-service/configmap.yaml` ("NEVER a capacity figure, never advertised to GC"). That framing is what keeps it from being read as an early piece of the egress-budget chain, and it is why this is not the thing that was vetoed.
- **C2 — default sized FAR above expected peak, and documented as such.** This is what *neutralises* the coupling objection rather than accepting it. Expected peak here is single-digit meetings, so the default is picked such that `apply_failed` from this cause means a bug, a leak, or a hostile MC — never "we got busy". **If the bound can be reached by legitimate growth, the rotating-victim failure becomes routine and the objection comes back.** Sizing reasoning goes in the comment so task 12 inherits it rather than re-deriving it.
- **C3 — subtract the meeting's own current edges before comparing.** `aggregate_total - this_meeting_current + this_meeting_new > bound`, **not** `aggregate_total + this_meeting_new > bound`. This is the sharpest catch in the review: get it wrong and an **idempotent re-assert of unchanged policy** — the §8 steady state, arriving every <=10 s per meeting — double-counts its own edges and trips the cap, converting the cadence itself into a rotating `apply_failed` storm once the tree is even half-full. **And it would pass every single-meeting test.** Pinned by test 14.
- **C4 — Tier C, not Tier A.** It depends on other meetings' state, so it is not evaluable from the request alone. Upsert and pending-connection promotion still happen; only the install fails; prior generation stays live. Preserves O-4.
- **C5 — S4's discrimination, plus a runbook clause.** The WARN distinguishes mailbox-full/timeout from edge-cap refusal via a bounded `&'static str` field, **never a metric label**; the five-value `outcome` set stays as @observability specified. `apply_failed` now has two causes with materially different 3am remedies (a wedged or overloaded actor vs a policy-volume bound), so the O-8 operational note gains one clause pointing at that log field. Still no numbered scenario, still no alert.
- **C6 — it becomes a fourth key in @operations' `docs/TODO.md` entry**, in both the first sentence and the "what task 12 owes" clause.

**Second TODO bullet, supplied verbatim by @operations at @security's request** — a separate top-level §Media Path Obligations item recording that **MH bounds policy per meeting and in aggregate but does NOT bound the meeting count**. The gap is real and deliberately unfixed here: an *empty* `egress_streams` set is legal and meaningful per `internal.proto`, so a caller can register arbitrarily many near-free meetings and the aggregate-edge bound never fires; the caller is an authenticated MC holding `service.write.mh`, making it a capacity-model question rather than an access-control one; and choosing MH's meeting ceiling is the same design decision that settles R-22's deferred egress-budget chain, so bolting a number on here would pre-empt it with a value nobody chose. Filed so it is on the record as *known* rather than as something four reviewers each assumed someone else had checked.

**Explicitly NOT in scope** (@security's own carve-out): a cap on the *number* of registered meetings. That is genuinely pre-existing, an empty policy is legal per `internal.proto`, and the caller is an authenticated MC — it is a capacity-model decision for media-handler + @operations, not something to bolt on here. @operations is getting it named in `docs/TODO.md` §Media Path Obligations as a known gap, so it is on the record rather than four people each assuming someone else checked it.

All-edges-or-none in all three tiers: the new snapshot is built completely, then swapped once. A failure at any edge means **no swap**, prior generation stays live (@code-reviewer 3).

**@security S4 — MOOT, and resolved in the direction S4 preferred.** S4 asked me to name the blast radius of the global `max_streams` ceiling, because a total-edge count across all meetings would let one over-large meeting push an unrelated meeting's legitimate re-assert into `apply_failed` — a cross-meeting *availability* coupling. @operations vetoed that path outright (above), so the coupling never exists and there is nothing to accept. The half of S4 that still applies is kept: the two remaining `apply_failed` sub-causes — mailbox-full vs apply-timeout — are distinguished by a **bounded `&'static str` reason in the WARN log, never as a metric label**; the 5-value `outcome` set stays exactly as @observability specified.

**@security S3 — every log line on this path is counts-and-bounded-reasons only.** The natural implementation of "warn when content differs" is to log *what* differs, and what differs is a set of `sender_id` / `slot_id` / `egress_stream_id` values — precisely the "obvious new back door" the `SubscriberSlot` comment flags now that `StreamTelemetry` is gone. Constrained at write time for **both** new WARNs (equal-generation content-differs, and transport-mode mismatch) and for the Tier-C reason above: **counts and a bounded `&'static str` reason only — never identities, never a `Debug`/`Display` of `RegisterMeetingRequest`, `MeetingRoutes` or the snapshot.** I will grep every path including the error paths for `?req` / `%req` / `{:?}` of those types before I hand over; `#[instrument(skip_all)]` covers the span but not a hand-written `tracing::warn!`. All of these carry `key_custody=operator`.

**Explicitly NOT implemented, by order of five reviewers and the proto: the `policy_generation == 0` rejection** (@security 6, @test 6, @code-reviewer 8, @operations O-1, @meeting-controller 1, `docs/TODO.md` (b), `internal.proto` lines 416-440). Instead, generation 0 is **ignored for apply purposes**: nothing installs, nothing is rejected, `applied_generation` echoes 0, and the site carries a comment naming **story task 13** as the enforcement owner. I will also check that nothing I add reaches the same effect by another route — in particular the monotonicity check must short-circuit *before* it can turn a repeated 0 into `rejected_stale` (@meeting-controller's second coordination question: confirmed, 0 is handled before monotonicity, and `0 < 0` is false regardless).

---

### 5. Apply algorithm (in the actor)

```
if generation == 0        -> outcome=no_generation ; no install ; NO swap
                             loud WARN iff egress_streams is non-empty (contract violation)
                             comment names story task 13 as enforcement owner
if generation <  installed -> rejected_stale ; NO swap
if generation == installed -> no-op ; NO swap ; loud WARN iff content differs ; status=applied
if generation >  installed -> build full snapshot ; swap once
                             -> applied  (or apply_failed if the mailbox was full or the
                                apply timed out; prior generation stays live)
```

**@meeting-controller's first coordination question / @operations O-2 — what fires during the task-11..task-13 window.** My draft answer was "no sample at all". **@observability's amendment overrides it and is better**, and I am adopting it verbatim: generation 0 lands on a dedicated `outcome="no_generation"`. Silence would have made the designed steady state indistinguishable from a dead code path; a named series makes it legible, and it is the *same* value the rejection lands on once task 13 flips enforcement — two eras, one value, no rename, and the series falls to zero exactly when MC starts sending >= 1. The window's expected shape (`applied` flat at zero, `no_generation` counting up, `applied_generation` pinned at 0, all of it **correct**) goes in the catalog entry, the operational note (O-8) and `docs/TODO.md` (b).

---

### 6. Response fields, all sourced from MH's own state (@security 7)

| Field | Source |
|---|---|
| `accepted` | `true` = received and parsed and handed to the apply mailbox. Never conflated with install success (@meeting-controller 2). |
| `applied_generation` | live `ArcSwap` snapshot (§3). On `apply_failed` and `rejected_stale` this is the **prior applied generation, never 0** — the prior policy is still live, and MC feeds its divergence gauge from this field, so echoing 0 there would raise a spurious divergence alarm during an incident that has a different cause. Moot on paths returning a gRPC error with no body. |
| `handler_id` | `Config.handler_id` — **the same value `gc_client.rs:136` sends GC as `MhAssignment.mh_id`**, threaded into `MhMediaService::new` from `main.rs:236`. No pod-name derivation (@operations O-5, CLAUDE.md infra-topology rule). |
| `process_start_epoch_ms` | sampled **once at process start** in `main.rs` and passed into `MhMediaService::new`, so "once per process" is visible at the call site rather than implied (@operations O-6's stated preference). New `crates/mh-service/src/process.rs` holds the sampling fn + type. |
| `transport_mode` | live snapshot (§3) |

`MhMediaService::default()` is **deleted** rather than given a plausible-looking fake `handler_id` (@operations O-5). A service with no handler identity and no process epoch is not a meaningful default; the one unit test using it constructs the real thing.

---

### 7. Metrics

Guard requirements below are not guesswork: I had a subagent read `crates/dt-guard/src/{application_metrics,metric_labels,metric_coverage,dashboard_panels,alert_rules}.rs` and the parsers they share. @observability's two messages and that reading agree on every point; where they differ from my draft, **@observability wins and I have adopted their version.**

**New metric `mh_media_policy_applies_total`.** Labels **`outcome`** + `key_custody`, cardinality 5x1.

**The label key is `outcome`, NOT `status` — @observability's ruling, and they are right.** I proposed `status` and offered to treat the divergent vocabulary as a §Service-local-labels case; flagging it as a decision rather than an omission is what prompted the grep that overturned it. Three reasons, in ascending order of force:
1. `outcome` is the established in-tree name for exactly this shape, already catalogued in two services: `ac_meeting_token_display_name_total{outcome}` (`ac-service/src/observability/metrics.rs:101`) and `mc_join_display_name_resolved_total{outcome}` (`mc-service/src/observability/metrics.rs:493`).
2. **`internal.proto` names the MC-side counterpart of this exact RPC `outcome`**, with its own five-value bounded set ("CANONICAL ENUMERATION of MC's bounded `outcome` label"), which task 13 will emit. Both ends of one handshake then carry the same label key and a responder can put the two series side by side.
3. `status` would be actively *wrong*, not merely suboptimal: `label-taxonomy.md` defines it as a **coarse, fleet-wide shared** classification (`success|error|timeout|rejected|accepted`), whereas mine is a **fine-grained, metric-local remedy taxonomy**. My §Service-local-labels reading doesn't rescue it — that section covers labels whose *name* is local (`actor_type`, `grpc_service`); `status` is the canonical shared name. Borrowing it for a different concept is precisely what the Shared Label Names rule exists to prevent, and it would be the tree's first `status` drift. A const and a few strings now; a labelled-series migration across catalog, panel, runbook and tests later.

Metric **name** stays `mh_media_policy_applies_total`. `{outcome="rejected_stale"}` on an `_applies_total` counter is ordinary counter idiom (`mh_gc_registration_total{status="error"}` counts registrations that failed), and the story froze the name in two places.

`mh_grpc_requests_total` keeps `status` — that one *is* the coarse shared vocabulary, used correctly.

`key_custody="operator"` is on the **metric**, not only the logs (`label-taxonomy.md` §Key custody: "logs **and** metrics ... unqualified and deliberately so").

**Q6 answered, and it was not a fresh judgment call — the tree pre-registered the trigger and this diff is it firing.** `crates/mc-service/src/observability/metrics.rs:504-506`, directly above `KEY_CUSTODY_LABEL`: *"Promote to `crates/common/src/observability/` at the SECOND consumer; MC is the first emission site in the tree. Fleet-wide rollout to every service is R-26 (observability task 22), not this task."* MH is the second consumer. My weak preference had been MH-local consts; @dry-reviewer is right that taking it would silently step over a pre-registered trigger, and **a trigger nobody honours is worse than one never written, because the next reader trusts it.** Note the same docstring pre-empts the obvious objection: this is not R-26 / observability task 22, which is the fleet-wide rollout of the *label onto every service's metrics*.

So: **hoist to `crates/common/src/observability/labels.rs` (NEW)**, moving the full docstrings with the consts — that prose (the "constraint, not a snapshot" reasoning, the no-E2E-boolean rationale) is the valuable part and must not be left behind or duplicated. `mh-service` already depends on `common`, so no new dependency edge. **A third home inside `mh-service` is ruled out.**

MC has exactly three consumer lines (`metrics.rs:507`, `:524`, `:552`, plus `actors/meeting.rs:454`), so MC's side is its two definitions collapsing to one `pub use common::observability::labels::{KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR};` — preserving both the qualified path and the unqualified uses, zero call-site churn. `mc-service/tests/media_admission_integration.rs:232` asserts the string literals, so it is unaffected either way. **This collides with my stated "zero `crates/mc-service/**` files", so it is Q7 to @meeting-controller** (see §10).

**`outcome` value set — @observability's authorized 5, replacing the story's 3.** Additive, no renames. Every value maps to a *different remedy*, which is the test for whether a bounded outcome label is doing its job:

| `outcome` value | condition | remedy |
|---|---|---|
| `applied` | generation >= 1 and **>** installed (swap happened) **or == installed** (idempotent re-assert, no swap — the live path already reflects it) | none; this is health |
| `rejected_stale` | generation >= 1 and **strictly lower** than installed | reorder/retry on MC→MH; abnormal, alertable |
| `no_generation` | `policy_generation == 0` | expected steady state for the whole task-11→task-13 window; becomes the value the *rejection* lands on at task 13 |
| `rejected_invalid` | policy fails structural validation (§4 Tier A) | MC sent bad policy; fix upstream |
| `apply_failed` | MH internal fault; prior generation stays live | MH-side; what the Tier-1b gate asserts against |

I am taking **@observability's design (b)** for the heterogeneous-transport-mode reject (and the rest of Tier A): a dedicated `rejected_invalid` value, so a bad-policy-from-MC reject is never indistinguishable from an MH internal fault. The gRPC call still returns `InvalidArgument` and still increments `mh_grpc_requests_total{status="error"}` — (b) *adds* the bounded landing spot in the policy counter, it does not move the error.

**Correction I am accepting**: the equal-generation idempotent re-assert counts as **`applied`**, not `rejected_stale`. If it landed on `rejected_stale`, the handler-restart story's periodic re-assert cadence would drive that counter monotonically upward in perfect health and any alert on it would be dead on arrival.

Emission uses a **private enum → `&'static str`** mapping, never a free `&str` (@security 10). Emitted as a multi-line `counter!("mh_media_policy_applies_total", "status" => ..., "key_custody" => ...)` with the name as a **string literal first argument** — a `const NAME: &str` is invisible to every guard's `MACRO_INVOCATION_WITH_FIRST_ARG_RE` and would silently switch off the catalog/dashboard/coverage checks in my favour.

**Not label material, on the record** (ADR-0036 §11 + `label-taxonomy.md` §Media-path identity R1/R2, @observability 4): `policy_generation`, `applied_generation`, `process_start_epoch_ms` (unbounded — one series per policy change / per restart); `egress_stream_id`, `sender_id`, `slot_id`, `stream_number`, `participant_id`, and any meeting id **raw or hashed** (the aggregation floor is `pod`; note `meeting_id_hash` would slip past the PII vocabulary guard via `HASHED_SUFFIXES`, so this is discipline, not machinery). Generation numbers go in the **log line** and in the metric's **value**. `handler_id` is *permitted* as a label but I am **not** adding it — Prometheus already carries `instance`/`pod` from the scrape. No end-to-end / zero-trust boolean anywhere: not a label, not a log field, not a panel, not a doc sentence. **And no Prometheus exemplars** (@observability): an exemplar is none of those three surfaces — it hangs off a histogram bucket with its own label set — so a rule phrased against labels/spans/logs does not reach it, and ADR-0036 §11 rejects them by name. Moot in this task (no histogram), recorded because task 16's forward-latency histogram is exactly where someone will reach for one. `#[instrument(skip_all)]` stays on the handler and goes on the apply path, with fields allow-listed explicitly (the request carries the whole policy).

**Three guards fire the moment the counter exists; all three fixes are in this devloop, none is deferrable:**

1. `metric_no_catalog` → an H3 heading exactly `### \`mh_media_policy_applies_total\`` in `docs/observability/metrics/mh-service.md` (parser: `CATALOG_HEAD_RE = ^###\s+\`([a-z_][a-z0-9_]*)\``; three hashes, column 0, backticks mandatory). The entry must **state the task-11→task-13 window explicitly** so a reader opening the panel in that window can tell the designed steady state from an outage — citing `internal.proto`'s ordering constraint and naming task 13 as the point the shape inverts, the same way `mh_register_meeting_timeouts_total` spells out which arm fires it.
2. `metric_no_dashboard` → a panel in `infra/grafana/dashboards/mh-overview.json` (`mh-media.json` does not exist until task 21; precedent is one commit back at `ba102ec8`, which added two MC metrics to catalog **and** `mc-overview.json` in the same devloop). Shape, per ADR-0029 + `dashboard-conventions.md`: `sum by(outcome) (increase(mh_media_policy_applies_total[$__rate_interval]))`, `legendFormat "{{outcome}}"`, `editorMode: "code"`, `range: true`, `datasource.uid: "$datasource"`, `fieldConfig.defaults.unit: "short"`. Never a bare `sum(metric)` (`counter_misuse`), never a literal `[5m]` (`rate_window`), never a hardcoded uid (`hardcoded_datasource`). Description notes that a counter materialises no series until first incremented.
3. `uncovered_metric` (ADR-0032) → the name must appear under `crates/mh-service/tests/**/*.rs`. **My Tier-1b unit gates live in `src/` and do not satisfy this** — a new `crates/mh-service/tests/policy_apply_integration.rs` drives all five `status` values through `MetricAssertion` with an adjacency `.assert_delta(0)`. No registration needed (MH's `tests/` has no `*_tests.rs` entry point, so a flat file is its own cargo target).

**No alert rule.** There is no metric→alert coverage rule; `alert_rules.rs` only validates alerts that already exist. `mh-alerts.yaml:25-31`'s prose block is where a future policy-apply alert's reasoning belongs, and story task 21 owns numbered scenarios (@operations O-8).

**One `docs/observability/label-taxonomy.md` edit IS required after all**, and @observability has given the trailer. §Shared Label Names has **no `outcome` row** despite AC and MC both already emitting it — a pre-existing gap, and the section's own rule is "Shared labels are added here BEFORE they're used in a second service." This metric is the third emitter and task 13 makes four. One row (`outcome` | fine-grained result of a single operation where each value names a **distinct remedy** | closed enum **per metric**, defined in that metric's catalog entry, not enumerated here), plus one sentence under the table distinguishing it from `status` — use `status` for the shared coarse values a responder compares across services, `outcome` when the set is metric-local and each value points at a different fix — citing the three instances. Not a PII-denylist change, so no guard edit and no security co-sign. Commit trailer: `Approved-Cross-Boundary: observability outcome-label row records an existing two-service convention per label-taxonomy.md's own add-before-second-service rule`.

**Collapse `mh_grpc_requests_total{method}`.** Taking @security 11 + @dry-reviewer 3's structural point, **bounded by @observability's constraint**: drop the **parameter**, keep the **label**. `record_grpc_request(method: &str, status: &str)` → `record_grpc_request(status: &str)`, emitting `"method" => "register_meeting"` from a `&'static str` const inside the one function. A second value then cannot be introduced without editing the emitter; and the label stays on the series so `sum by(status) (rate(mh_grpc_requests_total{method="register_meeting"}[5m]))` in `mh-incident-response.md:756,786,790` and the `mh-overview.json` panel keep working. **Dropping the label would have broken all four** — @observability caught this and they are right.

**No guard fires on this collapse.** It is a documentation-drift trap, not a CI trap: the catalog parser reads H3 headings only, so `mh-service.md:214-221` would advertise three impossible values forever. Full re-encoding site list (union of @dry-reviewer 3 and @observability 5, both verified):
- `metrics.rs:12` module docstring — **already wrong today**: lists 3 values and omits `register_meeting`
- `metrics.rs:133-135` docstring + the "Cardinality: 8 (4 methods x 2 statuses)" line → `Low (2 = 1 method x 2 statuses)`
- `metrics.rs:317-326` (`test_record_grpc_request`), `:398-410` (`test_cardinality_bounds`), `:463-465` (`test_prometheus_metrics_endpoint_integration`)
- `observability/mod.rs:10` ("`method`: bounded by gRPC methods (~3 values)")
- `tests/errors_grpc_metrics_integration.rs:16-17` (doc), `:41`, `:56-78` (the 4-element array written twice in one fn)
- `docs/observability/metrics/mh-service.md:208,216,218-221` — the R-26 note and description also lean on multi-method framing. Task 21 claims this edit "in the same sweep"; doing it here means task 21 finds it done, and the alternative is a knowingly-false catalog in the interim.

Per @test: `status` stays 2-valued and the **success/error adjacency assertion is preserved**, not deleted with the method matrix.

**One deliberate collapse, not four copies nudged into temporary agreement.** @dry-reviewer's follow-up found the decisive evidence: `metrics.rs` contradicts **itself** 122 lines apart — the module-level block at `:12` says 3 values and **omits `register_meeting`, the only value the code emits**, while `record_grpc_request`'s own docstring at `:134` says 4 and includes it. Four hand-maintained homes, three different answers, none correct; reality is 1. That is the CLAUDE.md convention demonstrated rather than asserted, so the fix is structural:

| Site | Fate |
|---|---|
| `docs/observability/metrics/mh-service.md` | **survives as the catalog of record**; gains the sentence that `internal.proto`'s `MediaHandlerService` block is the SSoT for the value set |
| `observability/mod.rs:5-12` label-bounds block + `:14-26` metric table (**already 7 metrics stale** — missing `mh_active_connections`, `mh_webtransport_connections_total`, `mh_webtransport_handshake_duration_seconds`, `mh_jwt_validations_total`, `mh_caller_type_rejected_total`, `mh_mc_notifications_total`, `mh_register_meeting_timeouts_total`) | collapse to a pointer at the catalog; **no 8th row**, and `mh_media_policy_applies_total` is deliberately NOT added to it — that would make the new metric the eighth thing to drift on day one |
| `metrics.rs:8-14` module-level `# Cardinality` block | collapse to a pointer; stops re-listing label values |
| `metrics.rs:133-135` `record_grpc_request` docstring | **KEPT**, rewritten to *reference* rather than *enumerate*: `Labels: \`method\` (single value, bound by GRPC_METHOD_REGISTER_MEETING), \`status\` (success \| error)` / `Cardinality: 2`. 12 of 13 `record_*` fns in this file carry the `Metric:`/`Labels:`/`Cardinality:` triple, so stripping it would make this the lone outlier and invite the next reader to "restore" the deleted list |

The rule is **enumeration-vs-reference, not location** (@observability's framing, sharper than the one I started with): a docstring that *points at* the binding cannot drift; one that *restates* the enumeration is a copy, wherever it lives. The four homes existed because four places restated. Three-layer split the diff holds to: **the const** is SSoT for the value, **`internal.proto::MediaHandlerService`** for the value set, **the catalog** for what the series means.

In-changeset rather than a follow-up: `mod.rs:10` and `:25` are sites the `{method}` collapse must touch anyway, it is a deletion plus a pointer inside files already being edited, and there is no design ambiguity.

**Already correct — deliberately NOT re-edited**: `infra/grafana/dashboards/mh-overview.json:1676` panel description and `infra/docker/prometheus/rules/mh-alerts.yaml:25-29` both already assert the single-valued state (docs ran ahead of code); this diff closes the gap toward them. `docs/runbooks/mh-incident-response.md:756,786,790` only ever filter `method="register_meeting"` — preserved.

---

### 8. Config knobs (CLAUDE.md config-over-hardcoding; R-32; @operations O-7)

Three new **optional-with-default** knobs in `crates/mh-service/src/config.rs` (NOT `MissingEnvVar` — a new required var with no manifest is a deploy-time CrashLoop and would strand a `kubectl rollout undo`):

| Env var | Default | Purpose |
|---|---|---|
| `MH_MAX_EGRESS_STREAMS_PER_MEETING` | 512 (ceiling 8192) | per-meeting `egress_streams` bound |
| `MH_MAX_CANDIDATE_SOURCES_PER_EGRESS` | 16 (ceiling 256) | per-egress `candidate_sources` bound |
| `MH_POLICY_APPLY_TIMEOUT_MS` | 1000 (ceiling 10000, = §8's re-assert cadence) | bound on the apply oneshot await |
| `MH_MAX_TOTAL_EGRESS_EDGES` | 65536 (ceiling 1048576) — far above expected peak per @operations C2 | aggregate policy-memory guard (@security S7, @operations-approved); a resource-exhaustion bound, **never a capacity figure, never advertised to GC** |

**Startup validation needs an upper ceiling, not just `> 0` (@security D1).** A `>0`-only check lets `MH_MAX_EGRESS_STREAMS_PER_MEETING=1000000` — a fat finger, or a capacity figure copied from elsewhere — silently re-open the allocation surface the bound exists to close, and it would read as configured-on-purpose forever. So: hard code-level ceiling constants, rejected loudly at startup. The operator can raise the bound; they cannot raise it to an unbounded one. Effective values logged at startup like the rest of MH config. **@operations has supplied the verbatim `docs/TODO.md` entry** for this handoff (a new top-level item in §Media Path Obligations, matching the section's house style and the task-12-precondition precedent set by the `terminationGracePeriodSeconds` entry). I am pasting it as given. The two parts to keep intact under any trimming: the sentence naming **what stays silent if task 12 never lands** — `dt-guard env-config` rule 1 tracks only `MissingEnvVar` reads so a defaulted key is invisible to it, and rule 3's orphan check fires only on keys that *are* in a ConfigMap, so **both halves of the guard are silent by construction** — and the **both-ConfigMaps-and-both-deployments** instruction (per-workload, not union). It also records this as the `MH_MAX_CONNECTIONS` 10,000-default failure R-22 documents, reproduced with three keys instead of one, noting the defect there was never *defaultness* but the conjunction of no manifest, no log and no choice; task 11 fixes two of those three.

**No manifest edits this task** — per @operations O-7 the ConfigMap keys ride with **story task 12**, which is already doing MH manifest work for task 9's landed keys; named explicitly here and in `docs/TODO.md` rather than left as "later". Per @security D2 the TODO entry states the consequence out loud rather than just naming the handoff: **if task 12 does not land the ConfigMap keys, the defaults become permanent and nobody finds out, because nothing fails.** That sentence is the difference between a tracked handoff and a silent one. This keeps @operations O-9's rollback posture true: code-only, no migration, no required env var, no proto change, no manifest a rollout-undo could strand.

Also in scope, one line, "fix don't defer": **`max_decoding_message_size` set explicitly on `MediaHandlerServiceServer`** in `main.rs:254` (@security 3) — the repeated fields just grew unbounded and tonic's 4 MiB default is currently implicit.

---

### 9. Tests

| # | Test | Kills |
|---|---|---|
| 1 | **Tier-1b apply-failure gate (PRIMARY)** — `MH_MAX_TOTAL_EGRESS_EDGES` set low, apply refused → echoed `applied_generation` does **not** advance (stays prior), `outcome=apply_failed`, snapshot `Arc::ptr_eq` unchanged. **A static threshold with a live actor over the full gRPC path: zero timing dependence, no clock manipulation, no race** | echo-on-receipt; partial install |
| 2 | **Idempotency = true data-plane no-op** — identical re-assert → `Arc::ptr_eq(before, after)` holds (zero swap) and active-connection count unchanged (zero churn) | re-apply landing on the same value masquerading as a no-op |
| 3 | **Monotonicity** — lower generation ignored, live snapshot unchanged, `outcome=rejected_stale` | rollback on reordered delivery |
| 4 | **MULTI-MEETING negative** (two-meeting fixture; shape is the requirement). **@security C1**: both meetings registered into the **same live `SessionManagerActor` / same published snapshot** — two separately-constructed routers pass vacuously and pin nothing. **@security C2**: assert **both arms** in one test — `sources_for(A, sender_5)` non-empty AND `sources_for(B, sender_5)` empty — with a comment naming the corpse (a flat `HashMap<SenderId, _>`) and why no other layer can catch it; without the positive arm, an implementation that resolves nothing anywhere also passes | a flat `HashMap<SenderId, _>` cross-tenant index |
| 5 | **`process_start_epoch_ms` within-process stability** — two `register_meeting` calls in one process return the same non-zero epoch. **No changes-on-restart test** (not constructible until the handler-restart story) | per-call `now()` |
| 6 | Every Tier-A structural reject, one test each, asserting **no state mutation** | last-write-wins / partial application |
| 7 | Transport-mode: `UNSPECIFIED` rejected; heterogeneous rejected; applied mode echoed | fail-open echo |
| 8 | `applied_generation` advances to N on a successful apply, and equals the **snapshot's** value not the request's | — |
| 9 | Metric label-set tests for `mh_media_policy_applies_total` (**all 5** `outcome` values + `key_custody`, with adjacency `.assert_delta(0)`) and the collapsed `mh_grpc_requests_total` (status adjacency preserved, **plus an assertion that the `method` label value equals `register_meeting`** — after the parameter is dropped, a const typo is the one method-label bug still possible, and that assertion is what catches it) | label swaps; const typo |
| 10 | **@operations O-11 no-regression** — apply generation N, then send an invalid re-assert; snapshot generation, `MeetingRegistration`, active-connection count all unchanged, response still reports prior `applied_generation`, `outcome=rejected_invalid` | one bad policy push turning "new joiners stuck" into "everyone dark" |
| 11 | **Denominator invariant** — sum across all five `outcome` values == number of `RegisterMeeting` calls made, over a fixture exercising every terminal path including early returns | reject paths with no counter; unnormalisable ratios |
| 12 | `apply_failed` / `rejected_stale` echo the **prior** generation, not 0, when a prior policy is live | spurious MC divergence alarms |
| 13a | **@test R1 — the Contradiction-2 detector.** Same generation, **differing** `egress_streams` content: `Arc::ptr_eq` still holds (no swap), `outcome=applied`, and the WARN fires | an untested contract-violation detector, which is an unreliable one |
| 13b | **@test R2 — the transport-mode mismatch loud log** is executable: a same-generation re-assert declaring a different `transport_mode` (rides on 13a's fixture) | a "log loud on mismatch" requirement with no observable |
| 13c | **@test R3 — Tier-B invariant named, not implicit.** A policy whose sender resolves to NO connection still advances `applied_generation` to N (`internal.proto`: "A PENDING SOURCE DOES NOT HOLD BACK `applied_generation`, AND MUST NOT") | a task-16 refactor that adds sender→connection binding silently starting to hold the echo back |
| 1b | **Determinism construction for the two mailbox sub-paths** (@test): both driven at the `SessionManagerHandle` level against an actor built via `new_with_parts()` and **deliberately never spawned**, so the consumer is *structurally absent* rather than merely slow. Mailbox-full: `try_send` capacity+1, the last fails with no consumer in existence. Timeout: the oneshot has no task that could ever resolve it. No clock manipulation, no race | a `pause()`/`advance()` race where the reply "usually" loses |
| 14 | **@operations C3 — the re-assert double-count.** Register a meeting at the cap, re-assert the **identical** policy: `outcome=applied`, no snapshot swap, **no cap trip**. Kills `aggregate_total + this_meeting_new > bound` | the §8 cadence turning itself into a rotating `apply_failed` storm — and it passes every single-meeting test |
| 13 | **Rollback case** — a meeting at applied generation N receives a `policy_generation: 0` re-assert (what an MC rolled back to a pre-reshape build sends): `outcome=no_generation` **and** `applied_generation: N`, not 0 | status/echo computed from one coupled source — indistinguishable from correct in every pre-task-13 test, diverges only during a rollback |

**No test asserting `policy_generation == 0` is rejected** (@test 6) — that behaviour must not exist yet.

Every echo test is constructed so that an `applied_generation: req.policy_generation` implementation **fails** it (tests 1, 3 and the gen-0 case all have `req.policy_generation != installed`).

---

### 10. Open questions for reviewers (I will not proceed past these silently)

- **Q1 — CONFIRMED independently by @security and @test.** "Rejected" reads as "does not resolve". @security overruled the wording of their own TODO entry; @test verified against `internal.proto:64-95` that the proto was explicitly *corrected away* from the reject-on-non-resolution wording, so the literal pin would recreate the global index (a) forbids.
  **@security C4 — item (a) is SPLIT, not ticked off**, exactly like item (c), for two reasons. (i) The entry records the corrected reading and why the literal wording is self-contradictory, so the next reader does not re-derive the contradiction or "restore" the literal pin. (ii) The load-bearing half: MH has **no `sender_id → connection` binding today**, so what lands here is *policy-table* scoping, and the primitive that makes cross-tenant leakage actually reachable — sender→live-connection resolution — arrives at **task 16**. Closing (a) here would retire the forcing function one story before the threat becomes reachable, and the task-16 implementer would find a green checkbox where the requirement used to be. The entry stays OPEN with the halves named: *discipline + scoped snapshot + two-meeting resolution test landed at task 11; the ingress binding at task 16 MUST consume `sources_for(meeting, sender)` and MUST NOT introduce a second index.* Item (a) also carries @observability's task-16 observability forward-carry, **third and final version** — the earlier two are superseded. The correction matters and is the same failure the R2 amendment is about: @observability had told me exemplars "have no home in R2", and @security caught that R2 *already* covers them twice (`label-taxonomy.md:278` enumerates "metric label, log field, span attribute, **or exemplar**", and `:293` is a dedicated paragraph naming the MH forward-latency histogram specifically). Restating it would create the second copy whose drift the whole amendment exists to prevent — **and it would be the copy a task-16 reader hits first**, since a TODO entry is what a story task hands you while a canonical doc is what you go looking for. So the entry **cites R2 and is deliberately silent on both exemplars and the low-occupancy rule**, and states in full only the two things R2 genuinely does not cover: gRPC `Status` message strings (not a metric, log, span or exemplar, and they cross a service boundary into MC's logs where MH's field discipline no longer applies), and the `skip_all` mechanism gap (a `tracing` event *is* a log record so R2 covers it, but `#[instrument(skip_all)]` bounds the span's own fields and does not reach a hand-written `event!`/`warn!` in the body — so a `skip_all`-shaped pattern guard reports clean over a leak).
- **Q2 — RESOLVED before I asked it.** I was going to ask whether Tier-A rejects should skip the upsert and promotion. @operations O-11/O-12 answered it: validate-before-mutate is the right posture, the O-4 blast-radius concern is bounded at Tier C where runtime apply failures live, and the residual risk (an MC policy bug costs that meeting's joiners a 15s provisional kick) is accepted and bounded by O-11. Recorded here rather than deleted so the reasoning survives.
- **Q3 — VETOED by @operations, and correctly.** `Config.max_streams` stays inert on MH's data path. Reasoning recorded in §4 Tier C rather than deleted, because the instinct that led me there was reasonable and the next person will have it too. Tier C is now the single mailbox-full/timeout path, which is both the failure §8 names and the more deterministic gate.
- **Q4 (@observability)**: answered in advance by your two messages — §7 adopts all of it, including the `no_generation` amendment, design (b), equal-generation-is-`applied`, drop-the-parameter-keep-the-label, and the `mod.rs` table → pointer. One thing left: the `status` row in `label-taxonomy.md`'s Shared Label Names table lists `success|error|timeout|rejected|accepted`, and mine is a different vocabulary under that canonical name. You said you expect **no** taxonomy edit, so I am treating this as a §Service-local-labels case and **not** touching the file. Confirm, or tell me to extend the cell.
- **Q5 (@infrastructure — not seated on this team, raising to @team-lead)**: promoting `arc-swap` to `[workspace.dependencies]` in the root `Cargo.toml`.
- **Q6 — ANSWERED by @dry-reviewer**: hoist to `crates/common/src/observability/labels.rs`, because MC's own docstring pre-registered "promote at the SECOND consumer" and MH is it. A third MH-local home is ruled out. Recorded in §7.
- **Q7 (@meeting-controller, cc @team-lead)**: the hoist implies a **2-line mechanical** edit in `crates/mc-service/src/observability/metrics.rs` (two const definitions → one `pub use`), which collides with your "zero mc-service files". I read your constraint as "don't do task 13's MC work inside MH's devloop", and this is instead the mechanical completion of a promotion MC's own code authorized in writing at this trigger — but it is your constraint, not mine to reinterpret. Preferred: you ACK the 2-line re-export with an `Approved-Cross-Boundary: meeting-controller` trailer. Fallback: MC keeps its definitions, MH imports from `common`, and I file a TODO entry naming that the trigger has already fired and MC's copy is the outstanding migration.

---

## Gate 1 — Plan Confirmation Tracking

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Meeting Controller (conditional) | confirmed |

### Lead rulings at Gate 1

Four items were escalated to the Lead. All four are recorded here as the
authoritative statement; the implementer folded them into the plan body.

1. **Root `Cargo.toml` (`arc-swap` → `[workspace.dependencies]`) — reclassified `Mine`.**
   Not a Guarded Shared Area (absent from every key of
   `scripts/guards/simple/cross-boundary-ownership.yaml`; matches none of the four
   ADR-0024 §6.4 criteria). No manifest, ADR or INDEX assigns root `Cargo.toml` to
   infrastructure, whose CLAUDE.md scope is Kubernetes/Terraform/IaC/CI-CD — a Rust
   build-graph edge is none of those. Settled precedent, not a novel call:
   `docs/devloop-outputs/2026-06-21-gc-telemetry-proxy/main.md:96-102` states it in
   terms, with the same posture at `2026-05-23-common-otel-helper-grpc-interceptor-task24/main.md:76`,
   `2026-05-11-wtransport-0-7-bump/main.md:100` and `2026-07-03-gc-otel-wiring-task26/main.md:91`.
   The one infrastructure-owned counter-example
   (`2026-05-19-wave1-python-guards-dt-guard/main.md:108`) is the workspace `members`
   array adding infrastructure's own crate per ADR-0034 — a different edit to a
   different table. The real controls are supply-chain review and the Layer-6
   dep-change gate; both owners are seated and both engaged (security cleared it with
   evidence — already in the build graph via `redis 0.26.1`, checksum-pinned at
   `Cargo.lock:151`, one trivial transitive dep, zero new crates; operations holds the
   gate at Gate 2).
   **This is a Lead classification ruling at Gate 1, not a reviewer downgrade** —
   ADR-0024 §6.2 monotonicity binds reviewers, and @operations correctly declined to
   touch it and routed it here.

2. **`docs/observability/metrics/mh-service.md` (Domain-judgment, @observability) — discharged in this devloop.**
   ADR-0024 §6.3 allows owner-implements *or* `--paired-with=<owner>`. Observability's
   participation **is** that overlay in substance: they specified the label key, all five
   `outcome` values and their remedy mapping, the gen-0 window prose and the SSoT
   sentence, and the implementer transcribed to their direction. The spin-out
   alternative is self-defeating — `dt-guard application-metrics` → `metric_no_catalog`
   has no allowlist, so deferring lands Layer 3 red or drops the metric, routing around
   a gate rather than honouring it. Conditions: owner confirms the **exact landed text**
   at Gate 3 via the Ownership Lens, and the commit carries
   `Approved-Cross-Boundary: observability`.
   Same ruling and conditions for `docs/runbooks/mh-incident-response.md`
   (Domain-judgment, @operations), pre-ACKed with four content constraints.

3. **`docs/observability/label-taxonomy.md:287` R2 amendment — IN SCOPE.**
   "Aggregate distributions are safe." is false at pod occupancy one, which is the
   loopback shape this story ships: an unlabelled pod-level latency histogram on a
   one-stream pod *is* a per-stream histogram and reconstructs the voice-activity trace
   R2 exists to forbid, while carrying no prohibited label. The defect is live and this
   devloop's own diff is reviewed against the rule it defects. One sentence, zero LoC,
   zero test surface, no design ambiguity, both required owners seated with trailers
   pre-committed — the protocol's suspicious-deferral check flags all three conditions,
   so deferral would fail the burden of proof. The file is already in the changeset for
   the §Shared Label Names `outcome` row. Conditions: replace **only** the first
   sentence at `:287` so `:288-291` runs intact and the "described below" pointer still
   lands on the voice-activity text; and the `docs/TODO.md` task-16 forward-carry
   **cites** R2 for exemplars and the low-occupancy rule rather than restating them —
   fixing a drift by creating a second copy would be self-defeating.

4. **Q7 (`crates/mc-service/src/observability/metrics.rs` two-line `pub use`) — not a Lead ruling.**
   Classification is Minor-judgment @meeting-controller (@code-reviewer's upgrade from
   Mechanical was correct: relocating a `pub const`'s definition site is not
   structure-preserving under the sed-test). The owner is seated, so §6.3's mechanism is
   an owner confirmation, not a Lead override. Either branch is acceptable; if MC holds
   at zero files, the fallback's `docs/TODO.md` entry must name an owner **and** a
   trigger. A third home inside `mh-service` is ruled out either way.

---

## Pre-Work

None — working tree clean at `ba102ec8`.

Note on Navigation injection: teammates are instructed to read their own
`docs/specialist-knowledge/{name}/INDEX.md` as their first action (rather than
the Lead pasting it inline). Same navigation map, Lead context stays lean for
the gate work.

---

## Implementation Summary

`RegisterMeeting` is now the real ADR-0036 §8 control plane. **255 mh-service tests pass** (878 across the three touched crates), clippy is clean workspace-wide, and 19 Layer-3 guards report OK.

### The four structural decisions

**1. The applied-generation echo has one source, so echo-on-receipt is unreachable.** `applied_generation` and `transport_mode` are read from the live `ArcSwap` snapshot *after* the apply resolves — on every path, including mailbox-full, timeout and apply failure. `req` is not in scope where the response is built. `internal.proto` says an implementer who writes `applied_generation: req.policy_generation` "has deleted the feature while leaving the field"; here there is no wire from request to response for that value to travel along. This also disposes of the rollback case for free: `outcome` comes from the apply decision and the echo from the snapshot, so a meeting at generation N receiving a gen-0 re-assert answers `no_generation` **and** `applied_generation: N`.

**2. One mechanism instead of four rules for scoped identifiers.** `crates/mh-service/src/routing/mod.rs` is a nest of private maps — `RoutingSnapshot.meetings` → `MeetingRoutes.by_sender` → meeting-local `edges` — and `sources_for(&MeetingKey, SenderId)` is the only sender lookup in the crate. There is no function taking a `SenderId` alone, so a global cross-meeting index is a *missing function*, not a remembered rule. `SenderId(NonZeroU16)`, `SlotId(u16)` (zero valid) and `StreamNumber(u8)` are fresh newtypes with widths derived from `media-protocol` by `const` assertion; there is no `65535`, `255` or `u16::MAX` literal in the crate.

**3. Two mailboxes, one actor.** A second `mpsc` into the same `SessionManagerActor`, `select!` over both, its own calibrated `CONFIG_APPLY_CHANNEL_BUFFER`. One actor keeps sole ownership of all state, so there is no lock and no ordering hazard. `try_send` never blocks; the reply await is bounded by `MH_POLICY_APPLY_TIMEOUT_MS`.

**4. Validate before mutate, counts before iteration.** Structural rejects run before any state is touched, and the two count bounds run before *anything that iterates* — duplicate detection builds `HashSet`s sized by the attacker-controlled repeated field, which is the allocation the bound exists to prevent, merely moved earlier than the routing table.

### Deliberately not implemented, with the owner named at the site

- **`policy_generation == 0` rejection** — story task 13. Enforcing it before MC emits >= 1 rejects every registration for the whole window, which is §8's blackhole reached through the field added to prevent it. Gen 0 installs nothing, advances nothing, rejects nothing; it short-circuits **before** the monotonicity check so a repeated 0 cannot become `rejected_stale`. There is deliberately no test asserting 0 is rejected.
- **`sender_id` → live-connection binding** — story task 16. `docs/TODO.md` (a) is **split, not ticked**, because the cross-tenant threat is not reachable until that binding exists.

### What changed from the approved plan, and why

| Change | Driver |
|---|---|
| `status` label → **`outcome`** | @observability: `internal.proto` already names MC's counterpart `outcome`; `status` is the coarse fleet-wide vocabulary and this would have been its first drift |
| 3 outcome values → **5** (`no_generation`, `rejected_invalid` added) | @observability: silence would make the designed gen-0 steady state indistinguishable from a dead code path; and an MC policy bug must not be indistinguishable from an MH internal fault |
| `Config.max_streams` as the apply-failure seam → **dropped** | @operations vetoed with manifest evidence (deployed 100 vs code default 1000, harmless only because inert; retiring at story 2; three disagreeing units) |
| **New** `MH_MAX_TOTAL_EGRESS_EDGES` | @security S7: with `max_streams` off the table nothing bounded policy memory in aggregate. @operations approved it on a fresh knob with six conditions |
| `stream_number` added to Tier A | @security S2 caught it missing from my own reject list after I had named it in my own mechanism table |
| Count bounds moved to run **first** | @security S1: "before building any routing table" is the floor, not the ceiling |
| `key_custody` consts **hoisted to `common`** | @dry-reviewer: MC's own docstring pre-registered "promote at the SECOND consumer", and MH is it |
| Counting boundary = **field-read**, not all terminal paths | @operations caught @observability's first rule over-reaching; the seven scalar early returns stay pre-boundary |

### Also fixed in-tree (fix, don't defer)

- `mh_grpc_requests_total{method}` collapsed to one value **structurally** — the parameter is gone, the label value is a const. Four hand-maintained homes restated the value set and three were wrong; `metrics.rs` contradicted itself 122 lines apart, and the copy at the top omitted `register_meeting`, the only value ever emitted. The three enumerating homes are now pointers to the catalog.
- `observability/mod.rs`'s metric table was seven metrics stale; replaced with a pointer rather than an eighth row.
- `max_decoding_message_size` set explicitly on the generated server — the decoder is the outermost allocation bound now that the request carries two unbounded repeated fields.
- `label-taxonomy.md` §Shared Label Names had **no `outcome` row** despite two existing production emitters; added.
- `label-taxonomy.md` R2's "Aggregate distributions are safe." was false at pod occupancy one — the loopback shape this story ships. Restated on the observable.

### Review-driven changes (Gate 3)

Every entry here is a **code or config change**, so **Gate 2 must be re-run in full** — not a
targeted layer. Seven paths entered the changeset after the recorded `TOTAL_DURATION=731` run,
including new Rust (`crates/mh-test-utils/src/media_policy.rs`) and a new
`crates/mh-test-utils/Cargo.toml` dependency edge, which is a direct
`audit_dep_changed_rust` trigger the recorded Layer-6 run never evaluated. The validated artifact
is not the committed artifact until it re-runs (@operations OPS-5).

**Self-caught on the Gate-3 re-read, before any reviewer finding landed:**

- `routing/mod.rs` — `PolicyRejection::UnspecifiedTransportMode`'s reason was a multi-line literal with **no `\` continuation**, so the "bounded `&'static str`" actually contained an embedded newline plus 17 spaces. That value is both the outbound `Status::invalid_argument` message and the `reason` field of a `tracing::warn!`, so it emitted a multi-line gRPC status and a log field that breaks line-oriented parsing. Fixed; then every string literal in the changeset was scanned for unescaped embedded newlines (only hit) and for mid-sentence whitespace runs (two, both in assertion text, both collapsed).
- `grpc/mh_service.rs` — the §8 transport-mode mismatch WARN is now gated on `outcome == Applied`. On every other outcome MH deliberately installed nothing, so `applied_mode` is the *prior* generation's mode and the line was comparing two different generations and reporting the difference as a contract fault — firing on exactly the paths an operator is already reading logs on (mailbox-full, apply-timeout, stale re-assert, the whole gen-0 window) and naming a transport-mode disagreement as the cause of an incident that has a different one. The surviving case is the one nothing else can see: an equal-generation re-assert whose content changed mode.

**@security** (S-1 code + docs, S-2, S-3, S-4 fixed; reclamation and ownership-enforcement deferred with entries):

- `session/mod.rs` — `EdgeCapExceeded` WARN gains `installed_total_edges` and `installed_meeting_count`, so "one fat policy or accumulated dead meetings?" is answerable from the one line an operator has. Both bounded aggregates, no identity.
- `session/mod.rs` — `handle_register_meeting` now WARNs with `previous_mc_id` / `new_mc_id` when the owning MC changes, instead of one unconditional INFO for both an ownership takeover and an ordinary re-assert. The **detection** half only: MH cannot distinguish attack from failover, because `MhAuthLayer` binds no caller to a `meeting_id`. Enforcement is filed.
- `routing/mod.rs` — `projected_total_edges` uses `saturating_sub` behind a `debug_assert`: loud where it can be fixed, safe where it cannot. A release-mode wrap would read as "over the aggregate bound" and refuse every apply on the pod, permanently and silently.
- `config.rs` + Scenario 13 — the falsified C2 property ("reaching it means a bug, a leak or a hostile MC, never growth") replaced with the **floor** model. Not monotone: a live meeting shrinking 5→4 edges computes `B - 5 + 4` and releases headroom. What ratchets is the floor held by meetings that have **ended**, which nothing reclaims. Remedy is reclamation, not a larger ceiling — raising it buys time proportional to the meeting-completion rate.
- S-5 (gitignored `Cargo.lock`) was **withdrawn by @security as a new entry** — the fact is already tracked twice — and folded as one sentence into the existing entry, recording only what is genuinely new: a supply-chain clearance cannot cite a lockfile line as evidence.

**@operations** (OPS-1 docs, OPS-2, OPS-3, OPS-4, OPS-5 fixed; reclamation deferred):

- `mh-overview.json` — the new panel's description cites `mh-incident-response.md` Scenario 13, closing the runbook↔panel loop in both directions.
- `mh-deployment.md` — the R-36 bake gate was blind to policy-apply failure from the moment this lands: `RegisterMeeting` answers `accepted: true` and records `status="success"` on `apply_failed`, `rejected_stale` and `rejected_invalid` alike. A cumulative-zero check on those three (never `no_generation`, which is the expected value until task 13) at the 30-min, 2h and 24h tiers. The 24h tier matters most — a ratchet does not surface in a 30-minute bake.
- `routing/mod.rs` — the `debug_assert` above, so saturating does not also silence the accounting bug it protects against.

**@observability** (F1-F11 fixed; F8 filed):

- `label-taxonomy.md` — the two new paragraphs had been inserted *inside* §Shared Label Names, orphaning the `reason` row out of the rendered table. Moved below it; the table is contiguous again.
- `mh-overview.json` — panel 32's array position and `gridPos` disagreed about which row owns it (F2); duplicate panel id 29 renumbered to 33 (F5); and panel 33's own array/`gridPos` disagreement, pre-existing and one panel over, fixed the same way (F11). Array order and layout now agree for every panel. Semantically the file is exactly *one new panel + one id renumber*, verified by object-level comparison against `ba102ec8`.
- `mh-service.md` — the `mh_grpc_requests_total` ratio example divided one series by two and evaluated to a constant 1 (F7); `sum()` on both sides, with the full-label-set matching explained inline. **Its pre-existing sibling at `mh_gc_registration_total` is fixed too** — same owner, same mechanism, same file, and it is where the broken idiom was copied from.
- `mh-service.md` — new `## Media Policy Metrics` H2 (F9), placed to keep the entry physically adjacent to `mh_grpc_requests_total` so the receipt-vs-application comparison needs no section jump.
- `observability/INDEX.md` (F3) repointed at `common/observability/labels.rs`; `dashboards.md` §MH Overview (F6) replaced with a pointer, §AC Overview filed as a tracked sibling.
- `session/mod.rs` — `#[tracing::instrument(skip_all)]` on `SessionManagerHandle::apply_policy` (F10), the plan commitment that had evaporated. The actor-side handler is deliberately **not** instrumented and the reason is recorded at the site: it runs on another task, so a span there is a disconnected root, not a child.
- `docs/TODO.md` (b) — the task-13 window prose is now an **obligation** on task 13 to retire it at all three named sites, not a description of where it lives (F4).

**@dry-reviewer** (F1, F2, F3, F4, F7 fixed):

- `routing/mod.rs` — `with_policy` now *delegates* to `projected_total_edges` instead of re-deriving the same subtract-then-add, so the number the bound is checked against and the number that is installed are one computation.
- `config.rs` — `parse_bounded_usize` / `parse_bounded_u64` collapsed to one generic. The drift risk was the two operator-facing message strings, not the bodies.
- `mh-test-utils/src/media_policy.rs` — **one** fixture home for the `EgressStream` / `RegisterMeetingRequest` builders that had three, reachable from `src/` unit tests and `tests/` binaries alike. `transport_mode: Datagram` is the fixture default all three asserted against and story 3 changes it.
- `routing/mod.rs` — the module doc claimed "no `65535` / `255` literal in this crate", which a grep falsifies against the file's own boundary tests. Scoped to what is actually true and pointed at the `const _` asserts, which are the enforcement.
- `PolicyApplyOutcome::ALL` (`[Self; 5]`, length tied to the catalogued cardinality) and `PolicyRejection::ALL` / `IdError::ALL`. The `PolicyRejection` half was a live gap, not tidiness: the exhaustiveness test listed **8** of `reason()`'s **11** arms — three `IdError` variants were unpinned in a test asserting they were pinned, and the `Id(_)` nesting made it short *on arrival*. The test now also asserts the reasons are single-line and mutually distinct. **The exhaustive walk paid for itself immediately**: adding it is what surfaced the embedded-newline reason string above, which every previous version of that test had passed because it only asserted the string was non-empty. That is the argument for exhaustive walks over hand-listed samples making itself one review earlier than expected, and it is more persuasive to the next author than the abstract case.

**@test** (both behavioural gaps fixed; WARN-fires assertion deferred with an entry):

- `grpc/mh_service.rs` — two new tests for the equal-generation branch that had no coverage at all: differing **content** must not swap (a version that swapped on difference would churn the data plane every cadence tick and passed every other test), and differing **transport mode** must echo the INSTALLED mode, never the requested one.
- The "assert the WARN fired" half is deferred: `SpanCapture` captures spans, not `tracing` events, so it needs an `EventCapture` sibling in `crates/common` with a test-isolation story — cross-service infrastructure, filed under §Test Debt against the mechanism rather than against MH.

**@code-reviewer** (both fixed): the `label-taxonomy.md` table placement, and hoisting the double-computed `projected_total_edges` in the edge-cap arm.

**Deferrals taken, all with named owners, triggers and `docs/TODO.md` entries** — three new entries plus two fold-ins:

| Deferral | Owner / trigger | Entry |
|---|---|---|
| Reclamation of an ended meeting's routing entry | protocol + meeting-controller + media-handler (needs a teardown RPC — `proto/**` GSA); story 2 R-22 | §Media Path Obligations |
| Meeting-ownership enforcement / generation-ratchet recovery | media-handler + meeting-controller + security; story task 13 | §Media Path Obligations |
| `tracing` event capture for "log loud" assertions | test + observability; next log-line acceptance criterion | §Test Debt |
| Aggregate-edge saturation gauge | observability + operations; story task 21 | §Observability Debt |
| `dashboards.md` §AC Overview panel-list drift | observability; next AC dashboard change | §Observability Debt |
| Supply-chain clearance cannot cite a lockfile line | security + infrastructure | folded into the existing §Infrastructure Validation entry, **not** a new one |

**Re-verified after every change**, with the pipeline's own invocations: `bash scripts/lang/rust/lint.sh` → `STATUS=OK REASON=cargo-clippy-passed`; `cargo test -p mh-service -p common -p mc-service` → 38 test binaries, 0 failed; `cargo fmt --all -- --check` → clean. `mh-overview.json` re-validated as parseable JSON with unique panel ids and object-level equivalence to `ba102ec8` modulo the one added panel and the one renumber.

---

## Files Modified

**New**: `crates/mh-service/src/routing/mod.rs`, `crates/mh-service/src/process.rs`, `crates/mh-service/tests/policy_apply_integration.rs`, `crates/common/src/observability/labels.rs`, **`crates/mh-test-utils/src/media_policy.rs`** (Gate 3)

**Modified**: `Cargo.toml`, `crates/mh-service/{Cargo.toml, src/config.rs, src/grpc/mh_service.rs, src/session/mod.rs, src/observability/metrics.rs, src/observability/mod.rs, src/lib.rs, src/main.rs}`, `crates/mh-service/tests/{errors_grpc_metrics_integration.rs, gc_integration.rs, common/grpc_rig.rs}`, `crates/common/src/observability/mod.rs`, `crates/mc-service/src/observability/metrics.rs`, `docs/observability/{metrics/mh-service.md, label-taxonomy.md}`, `docs/runbooks/mh-incident-response.md`, `docs/TODO.md`, `docs/specialist-knowledge/media-handler/INDEX.md`, `infra/grafana/dashboards/mh-overview.json`, and **added at Gate 3**: `crates/mh-test-utils/{Cargo.toml, src/lib.rs}`, `docs/runbooks/mh-deployment.md`, `docs/observability/dashboards.md`, `docs/specialist-knowledge/{observability, dry-reviewer}/INDEX.md`

**Dashboard diff, corrected at Gate 3**: no longer purely additive — **103 insertions, 10 deletions**. The stated 93/0 property was given up deliberately and for cause, in two steps. (i) Panel id 29 was **duplicated** across a row and a timeseries at `ba102ec8`, making `?viewPanel=29` ambiguous and inviting Grafana to silently reassign one on a UI round-trip; the trailing timeseries is renumbered 33. Nothing in the tree links to it — `grep -rn viewPanel` is empty — so no bookmark breaks. (ii) Two panels had their array position and their `gridPos` disagreeing about which collapsible row owns them: the panel this devloop added, and the pre-existing receipts panel beside it. Both moved so array order and layout agree everywhere; the deletions are the block-move, not content. Verified by object-level comparison against `ba102ec8`: the file differs by **exactly one added panel and one changed `id`**, nothing else.

`Cargo.lock` is gitignored and therefore does not appear in the diff at all; the Layer-6 dep-change gate keys on `Cargo.toml` paths, of which there are now three (`Cargo.toml`, `crates/mh-service/Cargo.toml`, `crates/mh-test-utils/Cargo.toml`).

---

## Devloop Verification Steps

| Check | Result |
|---|---|
| `cargo test -p mh-service` | 255 passed, 0 failed (16 targets) |
| `cargo test -p mh-service -p common -p mc-service` | 878 passed, 0 failed |
| `bash scripts/lang/rust/lint.sh` (the pipeline's own invocation, `-D warnings`) | `STATUS=OK REASON=cargo-clippy-passed` |
| `cargo fmt --all -- --check` | clean |
| 21 Layer-3 guards | all `STATUS=OK` |
| `scripts/layer1.sh` / `layer2.sh` / `layer3.sh` / `layer5.sh` | all `RESULT=OK` |

Guards specifically exercised by this change: `application-metrics`, `metric-labels`, `metric-coverage`, `dashboard-panels`, `alert-rules`, `env-config`, `todo-tracking`, `knowledge-index`, both `doc-citations`, `cross-boundary-classification`, `gsa-sync`, `no-pii-in-logs`, `no-secrets-in-logs`, `instrument-skip-all`, `test-rigidity`, `test-registration`.

**Not run by me** (Lead owns Gate 2): `scripts/layer-all.sh`.

---

### Gate 2 — Lead's authoritative pipeline record

Run by the Lead via `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (unattended
mode per ADR-0033 §4 — all seven layers evaluated, nothing rendered `NOT-RUN`).

**Attempt 1/3 — `TOTAL_RESULT=FAIL`** (`L1 OK, L2 OK, L3 OK, L4 N/A, L5 FAIL, L6 N/A, L7 OK`, 916s).
Layer 5 `STATUS=FAIL REASON=cargo-clippy-failed` — one `clippy::doc-markdown`
error on `PromQL` in `crates/common/src/observability/labels.rs:6`. A `FAIL`
(exit 1) consumes an attempt.

**Attempt 2/3 — `TOTAL_RESULT=N/A`, wrapper exit 0, no `FAIL`, no `NOT-RUN`. PASS.**

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 1s |
| 2 Format | OK | 1s |
| 3 Guards | OK | 48s |
| 4 Test | N/A (aggregate) | 199s |
| 5 Lint | OK | 2s |
| 6 Audit | N/A (aggregate) | 2s |
| 7 Env-tests | OK | 478s |

`TOTAL_DURATION=731`.

**The two `N/A` aggregates are the documented self-justifying case, not skips**
(SKILL.md §Layer N/A justification template): proto registers intentional-gap
placeholder wrappers for the test and audit phases, each emitting
`STATUS=N/A REASON=not-applicable-to-this-lang`, and `layer-all.sh` aggregates
the worst child status into the layer line. Every real child underneath is
green: `cargo-test-passed`, `nx-test-passed`, `cargo-clippy-passed`,
`buf-lint-passed`, `buf-breaking-passed`.

**@operations' and @security's standing condition — the Layer-6 dep-change gate
must FIRE on the `arc-swap` promotion — is satisfied and was verified, not
inferred**: `STATUS=OK REASON=cargo-audit-passed` on the run containing the
manifest edit; not skipped, not suppressed, no audit-suppression entry added.
The `STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes` line in Layer 6 is the
TypeScript half, correct because no `package.json` changed.

Layer 7 ran the full cluster bring-up plus both Phase-2 suites (Rust env-tests
and browser E2E) green in 478s; no Layer-7 attempt was consumed.

**Lesson carried into §Lessons Learned**: the implementer's own pre-Gate-2
clippy check reported clean against a failing build, for two independent
reasons — it omitted the pipeline's `-D warnings`, and it grepped for
`^(error|warning:)` while cargo emits ANSI escape sequences even through a
pipe, so the anchor never matched. It would have reported clean for *any*
failure. Compounding it, clippy stops at the first failing crate, so fixing the
one reported error surfaced eight more behind it. Self-checks now invoke
`scripts/lang/rust/lint.sh` and read its `STATUS=` line.


### Gate 2 — RE-RUN after Gate-3 drift (attempt 3, authoritative for the commit)

**Why a third run at all.** @operations (OPS-5) and @security independently established that the
changeset had drifted **seven paths** past the file set the `TOTAL_DURATION=731` run validated, so
the validated artifact was not the artifact about to be committed. The material half was never the
doc drift: `crates/mh-test-utils/src/media_policy.rs` is ~99 lines of **new Rust** that Layers 1, 2,
4 and 5 had never compiled, formatted, tested or linted in this changeset — and Gate-2 attempt 1
died on clippy over new Rust in this same changeset, which is the concrete reason not to assume new
Rust is green. Layer 3's two cross-boundary guards also read the diff, so their green described a
file set that no longer existed. Both reviewers left the call to the Lead and neither re-ran
anything; the re-run is a Gate obligation, not a nicety.

**Attempt 3/3 — `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh`, wrapper exit 0, `TOTAL_RESULT=N/A`,
no `FAIL`, no `NOT-RUN`. PASS.**

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 6s |
| 2 Format | OK | 1s |
| 3 Guards | OK | 50s |
| 4 Test | N/A (aggregate) | 182s |
| 5 Lint | OK | 2s |
| 6 Audit | N/A (aggregate) | 1s |
| 7 Env-tests | OK | 605s |

`TOTAL_DURATION=847`. Layer 7 ran the full cluster bring-up plus **both** Phase-2 suites green — Rust
env-tests (`env-tests-passed`) and the 8-test Playwright browser E2E (`browser-e2e-passed`, 1.2m). No
Layer-7 attempt consumed.

**The two `N/A` layer lines are the documented self-justifying case, not skips** (SKILL.md §Layer N/A
justification template): proto registers intentional-gap placeholder wrappers for the test and audit
phases emitting `STATUS=N/A REASON=not-applicable-to-this-lang`, and `layer-all.sh` aggregates the
worst child status into the layer line. **Every real child underneath was verified green
individually rather than inferred from the aggregate** — the first `layer-all.sh` invocation was
piped through `tail`, which discarded the child `STATUS=` lines, so Layers 4 and 6 were re-run
un-truncated against the same unchanged tree to capture them:

- Layer 4 — `STATUS=OK REASON=cargo-test-passed`, `STATUS=OK REASON=nx-test-passed`
- Layer 6 — `STATUS=OK REASON=cargo-audit-passed`, `STATUS=OK REASON=buf-breaking-passed`

**@security's and @operations' standing supply-chain condition is satisfied on the NEW trigger, and
verified rather than predicted.** `crates/mh-test-utils/Cargo.toml` gained a `proto-gen` path edge —
a live `audit_dep_changed_rust` trigger (`scripts/lang/_audit_gate.sh:105-107`) that the recorded
Gate-2 run never evaluated. It **fired**: `STATUS=OK REASON=cargo-audit-passed`, not skipped, no
suppression entry added. Both halves of the reviewers' framing hold together — *a gate is never
skipped on a predicted result*, **and** the predicted result was correct, because `proto-gen` is an
in-tree workspace member already in the build graph and adds zero external graph nodes, unlike the
`arc-swap` promotion where the gate had real work to do. The `SKIPPED-NO-DIFF no-dep-changes` line
in Layer 6 is the TypeScript half, correct because no `package.json` changed.

**No `[workspace] members` edit** — `mh-test-utils` pre-existed at `ba102ec8` (@operations caught and
corrected their own near-miss on this), so Ruling 1's ADR-0034 infrastructure carve-out is not
engaged and no infrastructure involvement is owed.

---

---

## Code Review Results

### Resume note (session 2)

The devloop session was interrupted after Gate 2 passed and before Gate 3
verdicts were collected. Per SKILL.md §Recovery (headless-infra-interruption
exception), main.md's Loop State is authoritative: Gate 1 confirmations and the
Gate 2 pipeline record above stand, the working tree is unchanged at base
`ba102ec8`, and the roster was respawned directly into the review phase. No
re-planning, no re-validation — Gate 3 only.

### Gate 3 — Verdict Tracking

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 5 | 4 | 3 accepted | S-5 withdrawn by the reviewer, not deferred. Ruling-3 Ownership Lens discharged on exact landed text (`label-taxonomy.md:303-308`), scope proved by byte-diffing base `:288-291` against landed `:310-313` |
| Test | RESOLVED-DEFERRED | 1 | 1 (behavioural half) | 1 accepted (WARN-fired half) | Planned tests 13a/13b landed as `reassert_at_an_equal_generation_with_different_content_does_not_swap` and `..._echoes_the_installed_transport_mode`. Asserting the `tracing::warn!` FIRED needs a new `EventCapture` layer in `common` — filed under §Test Debt |
| Observability | RESOLVED-DEFERRED | 11 | 11 | 2 accepted | Zero refused. Ruling-2 Ownership Lens discharged on exact landed text for the `mh-service.md` catalog entry; Ruling-3 R2 amendment verified byte-exactly against `ba102ec8`; Ruling-4(A) wording finalised by them as owner. F7 (catalog ratio PromQL evaluating to a constant 1) and F1 (orphaned `reason` row) were correctness bugs this diff introduced |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | Mandatory ADR Compliance + Ownership Lens delivered. Concurred with every classification incl. all seven `ADDED AT GATE 3` rows — **no upgrades, so nothing auto-routed to ESCALATE**. Flagged the trailer-coverage action discharged by Ruling 5 below |
| DRY | RESOLVED-DEFERRED | 7 (+1 cross-domain) | 7 | 2 accepted | Three fixed better than asked. F7's exhaustive walk over `PolicyRejection::ALL` immediately caught a live defect — an embedded-newline reason string reaching both an outbound `Status` and a `warn!` field, invisible to a test that only asserted non-emptiness |
| Operations | RESOLVED-DEFERRED | 5 | 4 | 3 accepted | Ruling-2 Ownership Lens discharged on exact landed text for BOTH `mh-incident-response.md` Scenario 13 and `mh-deployment.md` (the latter arrived unclassified and operations ruled on it as Domain-judgment/operations). OPS-5's in-tree half closed |
| Semantic Guard | CLEAR | 0 | — | — | Credential Leak, Client Credential Lifetime, Actor Blocking, Error Context Preservation, Metrics Path Completeness — all clean |
| Meeting Controller (conditional) | CLEAR | 0 | — | — | Ruling-4 conditions (A) and (B) confirmed on exact landed text; contract-consumer review of `RegisterMeetingResponse` for task 13 passed on all four properties |

---

## Accepted Deferrals

Nothing was deferred that could have been done here. Each item below is a scope boundary agreed at Gate 1, with a named owner and trigger; the bodies live in `docs/TODO.md` §Media Path Obligations.

- `policy_generation == 0` rejection — owner story task 13; see `docs/TODO.md` §Media Path Obligations item (b). Site marked in `crates/mh-service/src/grpc/mh_service.rs`; verified nothing reaches the same effect by another route.
- Meeting-scoped `sender_id` obligation — **split, not closed**; owner story task 16; see `docs/TODO.md` §Media Path Obligations item (a). The policy-table half landed here; the live-connection binding that makes the threat reachable does not exist yet.
- `process_start_epoch_ms` changes-on-restart — owner handler-restart story; see `docs/TODO.md` §Media Path Obligations item (c). The within-process half is closed here.
- The four `MH_MAX_*` / `MH_POLICY_*` ConfigMap keys — owner story task 12; see the dedicated `docs/TODO.md` §Media Path Obligations entry, which states out loud that `dt-guard env-config` is silent on defaulted keys in both directions.
- MH does not bound the number of registered meetings — owner media-handler + operations, trigger R-22 at story 2; see the dedicated `docs/TODO.md` §Media Path Obligations entry.
- Task-16 observability forward-carry (gRPC `Status` strings; the `#[instrument(skip_all)]` mechanism gap) — owner observability; carried inside `docs/TODO.md` §Media Path Obligations item (a).

---

## Commit Trailers Required

Per the Lead's Gate-1 rulings — each scoped to the hunks its owner ACKed:

```
Approved-Cross-Boundary: meeting-controller — pub use definition-site relocation of
  KEY_CUSTODY_LABEL/KEY_CUSTODY_OPERATOR only, at the promotion trigger MC's own
  docstring pre-registered; not the label's meaning
Approved-Cross-Boundary: observability — mh-service.md catalog entry for
  mh_media_policy_applies_total and the mh_grpc_requests_total single-valued
  method correction; label-taxonomy.md outcome row, status-vs-outcome guidance
  and the R2 first sentence at :287; dashboards.md §MH Overview panel-list
  pointer; observability/INDEX.md key_custody definition-site repoint; all per
  ADR-0011 metric taxonomy and ADR-0031 dashboard ownership
Approved-Cross-Boundary: security — label-taxonomy.md R2 first sentence at :287, scoped
  to that sentence and nothing else in the file
Approved-Cross-Boundary: operations — mh-incident-response.md Scenario 13 note
  (incl. the Gate-3 edge-cap ratchet clause), and mh-deployment.md's R-36
  bake-gate policy-apply arm at the 30-min / 2h / 24h tiers
Approved-Cross-Boundary: dry-reviewer — dry-reviewer/INDEX.md fold-ins at lines
  19 / 32 / 71 (anchor-vs-guard instrument test, common label-vocabulary home,
  task-11 false-positive boundaries), net zero against the 75-line cap
```

**@observability's trailer above supersedes the shorter Gate-1 version**, which predated the
`dashboards.md` and `observability/INDEX.md` rows and named neither. The authority clause now
carries ADR-0031 as well as ADR-0011, because the `dashboards.md` row rests on dashboard
ownership rather than on metric taxonomy.

### Ruling 5 (Gate 3) — Ruling 2 EXTENDED to the two Gate-3-added Domain-judgment doc rows

**Granted, on @observability's own narrower formulation rather than a general carve-out.** Ruling 2
enumerated `mh-service.md` and `mh-incident-response.md`; Gate 3 added `dashboards.md`
(observability) and `mh-deployment.md` (operations). Ruling 2 is extended to reach **Domain-judgment
doc rows raised, specified and reviewed by the owner-specialist themselves**, on Ruling 2's two
unchanged conditions: the owner confirms the *exact landed text* via their Ownership Lens verdict,
and the commit carries an owner `Approved-Cross-Boundary:` trailer whose reason clause names the
authority.

**Grounds.** (a) Both rows meet both conditions already — @observability specified pointer-not-
corrected-copy before it was written and re-derived both panel counts (23 MH, 31 AC) against the
committed state; @operations specified the R-36 bake-gate PromQL, ruled on their own file, and
confirmed the landed text. (b) The spin-out alternative is circular in the same way Ruling 2 found
it self-defeating for the catalog: routing an owner's own reviewer-raised finding to a separate
owner-implemented devloop means routing it to the person who just wrote it. (c) The formulation is
narrower than "Gate-3 rows are exempt" — it reaches only rows the owner raised AND specified AND
reviewed, which is precisely where the `--paired-with=<owner>` substance is strongest. A
Domain-judgment row added at Gate 3 that the owner did *not* raise still routes to owner-implements.

**This ruling does not touch Guarded Shared Areas** (ADR-0024 §6.4) — no §6.4 path is in this
changeset, and §6.4's stricter posture is unaffected by anything above.

**Trailer count confirmed at five**, one per owning specialist: meeting-controller, observability,
security, operations, dry-reviewer. Each reason clause is scoped to the hunks its owner ACKed;
@observability's superseding version and @operations' widened version are the ones to use.

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `ba102ec8d971769fc550ae21fc7339f1dceb56a9`
2. Review all changes: `git diff ba102ec8..HEAD`
3. Soft reset (preserves changes): `git reset --soft ba102ec8`
4. Hard reset (clean revert): `git reset --hard ba102ec8`
5. No schema changes; no infrastructure manifests — code-only revert is sufficient.

---

## Issues Encountered & Resolutions

**1. `docs/TODO.md` (a)'s acceptance pin is self-contradictory.** "A `sender_id` valid in meeting A must be REJECTED in meeting B's request" cannot be evaluated without a cross-meeting index — the exact primitive the entry forbids. Surfaced at Gate 1 rather than resolved silently; @security overruled their own wording and @test verified `internal.proto` had been *explicitly corrected away* from that phrasing. Read as "does not resolve", pinned by a two-meeting fixture asserting **both** arms against one live routing table. The correction is recorded in the TODO entry so nobody "restores" the literal pin.

**2. Two reviewer instructions collided on the hoisted docstrings.** @observability and @dry-reviewer required the `key_custody` docstrings to move verbatim; @meeting-controller required the now-*executed* promotion instruction not to travel into the new home. Raised rather than decided; the Lead ruled: move the substance, retire the executed instruction, keep the still-forward-accurate R-26/task-22 clause.

**3. My proposed apply-failure seam was wrong, and the replacement is better.** I proposed enforcing `Config.max_streams` — advertised to GC, enforced nowhere, which is verbatim §8's cautionary precedent. @operations checked the deployed manifest: `MH_MAX_STREAMS` is 100 in the ConfigMap and 1000 in code, harmless *only because the value is inert*. Vetoed. @security then found the gap the veto opened (no aggregate policy-memory bound at all) and @operations approved a fresh `MH_MAX_TOTAL_EGRESS_EDGES` with six conditions.

**4. The mailbox-full test hung.** My first version asserted `is_meeting_registered()` in a fixture whose actor is deliberately never spawned — the lifecycle oneshot could never be answered. Replaced with `config_apply_capacity()` / `lifecycle_capacity()` accessors, which assert §8's separation requirement *directly*: the config-apply mailbox at zero and the lifecycle mailbox at full headroom. A shared mailbox would show both at zero. Better than what it replaced.

**5. The knowledge index was already at its 75-line cap.** Adding twelve entries meant reclaiming twelve. Three Integration Seams rows duplicated Code Locations rows verbatim (gc_client, mc_client, mh_service) and were removed; four pairs of same-file rows were merged. Net zero, and the file is more accurate than before.

**6. `max_decoding_message_size` lives on the generated server, not on `InterceptedService`.** `with_interceptor` returns a wrapper that does not forward it, so the server is now built explicitly and wrapped with `InterceptedService::new`.

**7. Gate 2 attempt 1 failed on Layer 5 (`cargo-clippy-failed`), and my self-check was the reason it got that far.** I ran `cargo clippy --workspace --all-targets` and grepped for `^(error|warning:)`. That is wrong twice over: the pipeline runs it with **`-D warnings`** (`scripts/lang/rust/lint.sh:7`), without which a pedantic lint does not fail the run; and cargo emits ANSI colour codes even through a pipe, so `^error` never matches — the line actually begins with an escape sequence. My check reported `0` against a build that was failing. **Self-checks now go through `bash scripts/lang/rust/lint.sh` and read its `STATUS=` line**, not a hand-rolled cargo invocation.

The first failure was one `clippy::doc-markdown` hit (`PromQL`). Rather than fix it and burn another attempt, I wrote a scanner implementing clippy's actual `is_camel_case` heuristic plus its default `doc-valid-idents` allowlist, ran it over **only the lines this changeset added** in the two crates that enable `clippy::pedantic` (`crates/common/src/**` and `crates/mh-service/src/**` — `mc-service` and every `tests/**` target do not, which is why comparable bare identifiers there are not flagged), and fixed all four hits at once: `PromQL`, `SSoT`, and `SFrame` twice. `SFrame` now matches the tree's existing convention in `crates/media-protocol/src/codec.rs`.

Fixing `common` then unblocked compilation of `mh-service`, surfacing **eight more** pedantic errors clippy had never reached — one unused import (`MeetingKey`, left behind when the mailbox-full test moved to capacity accessors), four `needless_borrow` on `&vars` where `vars` was already a reference, and three `missing_errors_doc` on the `from_wire` parsers. All fixed. **Lesson: a clippy run that stops at the first crate is not a clean bill of health for the crates behind it.**

---

## Lessons Learned

**Restating the task in mechanism-language paid off twice, and then caught me out.** "Every identifier on this contract is an ordinal scoped to an enclosing entity" produced a wider class than the task's own noun — four same-owner siblings, all in this changeset — and turned four rules into one nesting discipline. It also exposed the contradiction in the acceptance pin. But @security then found `stream_number` **missing from my own Tier-A list after I had named it in my own mechanism table**: deriving the right mechanism does not mean the enumeration follows it, and the gap between the two is exactly where a reviewer earns their keep.

**Structural beats disciplined, and the test is whether the wrong version compiles.** `applied_generation` having one source means echo-on-receipt is not a line anyone can write. `sources_for` requiring a `MeetingKey` means the global index is a missing function. @security's S6 is the honest counterweight: the same claim is **false** for the `usize` indices inside `by_sender`, which are meeting-local only because `edges` is contained in the same struct. Knowing where the structural guarantee stops is worth as much as having it.

**A reviewer's veto opened a hole neither of us had seen.** Dropping `max_streams` removed the only aggregate bound; @security spotted it, and @operations — who had written the veto — reversed their own coupling objection rather than defending it ("bounded-loud-one-apply beats unbounded-silent-all-meetings; that is my own blast-radius principle and I had it pointing the wrong way"). The three-way exchange produced a better answer than any single position in it.

**The best catch in the review was arithmetic.** @operations' C3: the aggregate check must subtract the meeting's own current edges. Get it wrong and §8's own ≤10s re-assert cadence becomes a rotating `apply_failed` storm once the handler is half full — **and it passes every single-meeting test**. Same shape as the echo-on-receipt bug this whole task exists to prevent, arriving inside the guard added to prevent something else.

**Documentation drift is measurable, and it was worse than anyone predicted.** Four homes restated `mh_grpc_requests_total`'s label set; three were wrong, and `metrics.rs` disagreed with *itself* 122 lines apart with the copy at the top omitting the only value ever emitted. That is a stronger argument for enumeration-vs-reference than any principle, and it is why three homes became pointers rather than being corrected in lockstep.
