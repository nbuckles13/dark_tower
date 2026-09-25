# Devloop Output: MH EndMeeting teardown, reclamation and capacity guards (story 2 task 11)

**Date**: 2026-09-25
**Task**: MH `EndMeeting` teardown with ownership reject, reclamation, capacity gauges, meeting cap, restored method label (R-20, R-21)
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/hear-each-other`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8d0869659bd4dd7ca758397cbbcb4648da7f2f7a` |
| Branch | `feature/hear-each-other` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer (spawned)` |
| Implementing Specialist | `media-handler` |
| Tier | `full` |
| Iteration | `1` |
| Security | `spawned` |
| Test | `spawned` |
| Observability | `spawned` |
| Code Quality | `spawned` |
| DRY | `spawned` |
| Operations | `spawned` |
| Semantic Guard | `spawned` |
| Paired Meeting Controller | `spawned` |
| Protocol (conditional, GSA proto comment) | `spawned` |
| Infrastructure (Gate-3, Minor-judgment rows) | `spawned` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
Implement MH's `EndMeeting` (story 2 R-20) with the `mc_id` ownership reject, reclaim every meeting-keyed resource on release, publish the capacity gauges and enforce `MH_MAX_REGISTERED_MEETINGS` (R-21), restore the two-value `method` label, and prove teardown on a long-running Kind pod.

### Scope
- **Service(s)**: mh-service (code); env-tests + Layer 7 (proof); proto comment, catalog, dashboards, alerts note, ConfigMap comments, runbooks, `docs/TODO.md`
- **Schema**: No
- **Cross-cutting**: Yes — proto GSA comment (protocol), Layer-7 machinery (operations/infrastructure), observability surfaces

### Debate Decision
NOT NEEDED — the contract, semantics and names were fixed by the story plan and the protocol task; open mechanics were settled by Lead rulings at Gate 1.

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mh-service/src/grpc/mh_service.rs` | Mine | — |
| `crates/mh-service/src/session/mod.rs` | Mine | — |
| `crates/mh-service/src/routing/mod.rs` | Mine | — |
| `crates/mh-service/src/observability/metrics.rs` | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/src/grpc/gc_client.rs` | Mine | — |
| `crates/mh-service/tests/end_meeting_integration.rs` (new) | Mine | — |
| `crates/mh-service/tests/errors_grpc_metrics_integration.rs` | Mine | — |
| `crates/mh-service/tests/policy_apply_integration.rs` | Mine | — |
| `crates/mh-service/tests/auth_layer_integration.rs` | Mine | — |
| `crates/mh-service/tests/stream_admission_integration.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_integration.rs` | Mine | — |
| `crates/mh-service/tests/webtransport_accept_loop_integration.rs` | Mine | — |
| `crates/mh-service/tests/otel_webtransport_integration.rs` | Mine | — |
| `crates/mh-service/tests/media_session_binding_integration.rs` (register_meeting cap argument) | Mine | — |
| `crates/mh-service/tests/common/**` | Mine | — |
| `crates/mh-test-utils/src/admission.rs` | Mine | — |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` (new) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (header claim + ratchet doc/message retarget only) | Not mine, Minor-judgment | test |
| `crates/env-tests/Cargo.toml` (tonic dev-dependency) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/kube.rs` (new — `configmap_key`/`configmap_u64` hoisted from test 28 at their second consumer) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mod.rs` (register `kube`) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` (`MH_MAX_REGISTERED_MEETINGS` in the logged-bounds table) | Not mine, Minor-judgment | test |
| `crates/env-tests/README.md` (MH gRPC forward + MC credential documented) | Not mine, Minor-judgment | test |
| `Cargo.lock` (regen) | Not mine, Mechanical | test |
| `scripts/layer7.sh` (start per-workload kubectl port-forwards for mh-0/mh-1; export ENV_TEST_MH_{0,1}_GRPC_URL / _METRICS_URL; trap-kill; TCP probe) | Not mine, Minor-judgment | operations |
| `scripts/lang/_common.sh` (add `layer_register_cleanup`, invoked from `__layer_lifecycle_end`) | Not mine, Minor-judgment | infrastructure |
| `scripts/lang/_common.test.sh` (lifecycle cleanup hook self-test) | Not mine, Minor-judgment | infrastructure |
| `scripts/layer7.test.sh` (Phase-1i fixtures, fakes and lanes) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` (the four Phase-1i reason tokens) | Not mine, Minor-judgment | operations |
| `crates/env-tests/src/fixtures/metrics.rs` (bounded poll-until-below-baseline sibling) | Not mine, Minor-judgment | test |
| `infra/kind/kind-config.yaml.tmpl` (correct the misleading "future NodePort" mh gRPC comment → env-test route is the layer7 port-forward; comment only) | Not mine, Minor-judgment | infrastructure |
| `proto/dark_tower/internal/v1/internal.proto` (comment only) | Not mine, Domain-judgment | protocol |
| `docs/observability/metrics/mh-service.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` | Not mine, Minor-judgment | observability |
| `docs/observability/dashboards.md` (mh-media panel rows) | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mh-overview.json` (panel 28 + policy-apply outcome panel descriptions) | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mh-alerts.yaml` (comment only) | Not mine, Minor-judgment | operations |
| `infra/services/mh-service/configmap.yaml` (comment tense only) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/mh-incident-response.md` (Scenario 13 remedy + ownership-reject / meeting-cap arms) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-deployment.md` (required key + R-24 rollout/rollback order) | Not mine, Minor-judgment | operations |
| `docs/TODO.md` | Not mine, Minor-judgment | security |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-09-25-mh-end-meeting-teardown/main.md` | Mine | — |

---

## Planning

### Mechanism restatement
"Every per-meeting entry MH holds must be reclaimable by one teardown, and every bound MH enforces must be observable from the value it enforces." The first widens the class beyond the two maps the task names: `SessionState` has FOUR meeting-keyed maps (`registered_meetings`, `active_connections`, `pending_connections`, `meeting_notifiers`), and today `meeting_notifiers` is insert-only and `active_connections` never drops empty participant/meeting entries. Both are reclaimed here (release + `remove_connection`/`remove_pending` empty-entry cleanup). `LocalSubscribers` / `SenderBindings` are reclaimed by the closed connections' own compare-and-remove teardown, not by release directly.

### Teardown (R-20)
- `RoutingSnapshot::without_meeting` + `RoutingTable::remove` mirroring `with_policy`/`install` (one successor build, one swap). Edge accounting is NOT duplicated (DRY D1): extract `RoutingSnapshot::edges_after_displacing(&MeetingKey) -> usize` (the displaced lookup + `saturating_sub` + the one `debug_assert`); then `projected_total_edges = self.edges_after_displacing(&policy.meeting) + policy.edges.len()` and `without_meeting`'s total = `self.edges_after_displacing(meeting)`. `with_policy`'s SSoT comment stays literally true and names the helper (three callers, one accounting path). Removing the routes entry IS forgetting the applied generation (`generation_for` -> 0), so a same-id re-create at gen 1 is a fresh install.
- New lifecycle-mailbox message `SessionMessage::EndMeeting` (lifecycle, not config-apply: it is teardown, and FIFO with `RegisterMeeting` on the same mailbox). Handle awaits the reply bounded by `MH_POLICY_APPLY_TIMEOUT_MS` (no second literal); timeout / actor gone -> gRPC `UNAVAILABLE`, `mh_grpc_requests_total{method="end_meeting",status="error"}`, no teardown outcome.
- Actor decision (pure fn `ownership(registered: &str, caller: &str) -> OwnershipCheck`, byte-exact `==`, no normalisation, not constant-time; the SAME helper drives the existing register-path takeover WARN so detection and enforcement cannot drift):
  - no registration -> `UnknownMeeting` (debug_assert no routes entry) -> ack true.
  - mismatch -> `RejectedOwnership`, nothing mutated, WARN with `registered_mc_id`/`caller_mc_id` server-side only -> `FAILED_PRECONDITION` with a generic message that does NOT echo the registered id. (O-1) The existing register-path takeover WARN at session/mod.rs:685 is renamed to the SAME `registered_mc_id`/`caller_mc_id` vocabulary (its `previous`/`new` reads wrong on the reject path), so one grep covers both sites. RENAME COMPLETENESS (CR-5): the field names are updated at EVERY reference — session/mod.rs log site, docs/TODO.md:1398 (which the re-derived ownership entry rewrites anyway), and any log-assertion test that names them. The story-file mention (476) is task-prompt text, not a field reference, and is left. No stale old field name survives in code, tests, or runbook.
  - match -> remove registration, remove routes, cancel every ACTIVE connection's per-connection token, drop `active_connections[m]`, drop notifier only if no pending remain (pending are left to the registration timeout per contract), republish `mh_media_egress_edges` + `mh_media_registered_meetings` BEFORE `respond_to.send` (O-4: a synchronous ack the env-test reads immediately after must observe the moved gauge; a comment at the site pins the ordering against a later refactor) -> ack true.
- Connection close: `handle_connection` creates `conn_cancel = cancel_token.child_token()`, carried on `PendingConnection`/`ConnectionEntry`; media loops use `conn_cancel.child_token()`; hold loop selects on `conn_cancel` and distinguishes shutdown vs meeting-released by `cancel_token.is_cancelled()`. Registration is gone on release, so no MC disconnect notify fires (MC ended the meeting). Race closed: `AddConnection` for a no-longer-registered meeting cancels the token instead of inserting.
- In-code notes at the release site: story-2 assumption that MC quiesces all pushes first (pointer to `EndMeetingRequest` in internal.proto as SSoT, not restated), the accepted deadline-expired residual, and the story-4 re-assert fence (pointer to `docs/TODO.md`).
- Pre-boundary: empty / >256-byte `meeting_id` or `mc_id` -> `INVALID_ARGUMENT`, `end_meeting`/`error`, no teardown outcome.
- Metrics: `mh_media_meeting_teardowns_total{outcome, key_custody}`, `MeetingTeardownOutcome::ALL: [Self; 3]` = released | unknown_meeting | rejected_ownership, zero-initialised; counter handles cached on the actor's handle struct and recorded by the actor where the decision is made. `mh_media_registered_meetings` is always `set(registered_meetings.len())` (never inc/dec) at EVERY mutation site: construction (0), register-insert, release. (Re-register of an existing id does not change len; cap-refusal does not mutate.)
- Method label: `GrpcMethod { RegisterMeeting, EndMeeting }`, `ALL: [Self; 2]`, `as_label`; `record_grpc_request(method, status)`; every call site passes it; zero-init is the full 2x2; docstrings state the true two-member set with internal.proto's service block as SSoT.

### Capacity gauges and cap (R-21)
- `MH_MAX_REGISTERED_MEETINGS` REQUIRED on `PolicyLimits.max_registered_meetings` (resource guard family), `MissingEnvVar` literal on one line, `parse_bounded` against new `MAX_REGISTERED_MEETINGS_CEILING` (proposed 65_536 = 8x the deployed 8192; security to rule).
- The cap refusal is a TYPED variant (CR-2), mapped to `RESOURCE_EXHAUSTED` at the gRPC boundary, never a String/anyhow out of the actor. The `registered_meetings` publish carries `#[expect(clippy::cast_precision_loss, reason=...)]` like `publish_egress_edges` (CR-1). Enforced inside the actor's `handle_register_meeting` (atomic with the insert): only a NEW meeting id at `len >= cap` is refused; a re-register of an existing id is never refused. ORDER (O-6): the cap is a REGISTER-path check (lifecycle mailbox), so for a new meeting it fires BEFORE the config-apply stream-admission check ever runs — a cap refusal short-circuits the handler with no apply. This is stated in code and in the catalog's identity paragraph. Handler: no upsert, no promotion, no apply; `PolicyApplyOutcome::RejectedMeetingCap` ("rejected_meeting_cap", `ALL` 6->7 — post-boundary, so the denominator invariant requires exactly one policy outcome), grpc `register_meeting`/`error`, `RESOURCE_EXHAUSTED`, WARN with count + cap.
- Gauges, `key_custody=operator` only: `mh_media_registered_meetings` (cached handle, `set(len())`, published at construction and on every insert/release); `mh_media_egress_edges_limit` AND `mh_media_registered_meetings_limit` (D2 + O-3, raised independently by DRY and observability: the meeting cap must be observable from the value it enforces, or task 18's PromQL hardcodes 8192 — the second-literal the task forbids, relocated to a rule file) published once at startup from `config.policy_limits.max_total_egress_edges` / `max_registered_meetings` — the same struct fields enforcement reads — by extending `publish_egress_admission` to take `&PolicyLimits`.
- Dashboards (mh-media.json): `mh_media_egress_edges_limit` is added as a FOURTH TARGET on the existing edges-vs-ceiling-vs-recommended_min panel (O-2: a separate edges-vs-limit panel frames `_limit` as a saturation denominator, the exact misread that panel's description forbids); its description is updated to name which bound binds. New panels for teardowns-by-outcome, registered-meetings (with its `_limit` target). Catalog entries for all four new series and the new outcome value (application-metrics guard requires dashboard + catalog).

### Tests
- Unit: ownership fn both arms; cap (new id at cap refused, existing id at cap accepted); `RoutingSnapshot::without_meeting` total/other-meeting untouched; release forgets generation.
- Integration `tests/end_meeting_integration.rs` over the real service: released (routes gone, edges returned, `mh_media_registered_meetings` FALLS, `outcome="released"`, ack true); unknown -> ack + `unknown_meeting`; mismatch -> `FailedPrecondition` + `rejected_ownership` + state untouched; stale-generation re-register from a new mc_id rebinds -> EndMeeting from it releases (MC-restart / failover case); release -> re-register at gen 1 -> `Applied`; active connection token cancelled, pending untouched; meeting cap counted `rejected_meeting_cap`; two-value method label (both methods emit, zero-init has 2x2). Auth-layer test for the EndMeeting path.
- Env-test: see sizing call below.

### Env-test (Lead ruling #2: path (A'), container-run, NO host/infra action)
Ruling #1's premise (ports.json already forwards MH gRPC) was false; ruling #2 supersedes it. The devloop container has working kubectl access to the Kind cluster (layer7.sh already `kubectl exec`s into postgres; `kubectl get deploy -n dark-tower` lists mh-0/mh-1).
- `scripts/layer7.sh` (operations-owned) STARTS its own per-workload forward for gRPC 50053 ONLY: `kubectl port-forward -n dark-tower deployment/mh-0` and `deployment/mh-1` (NOT svc/, NOT the mh-service-{0,1} UDP Services; per-deployment at replicas:1 is deterministic).
  - LOCAL PORT = the ALREADY-ALLOCATED per-slug `.ports.mh_0_grpc` / `.mh_1_grpc` from ports.json, bound explicitly (`<allocated>:50053`). This REPLACES ruling #2's "parse kubectl's 'Forwarding from 127.0.0.1:<port>' line", and is strictly better on three counts every reviewer raised: it removes the foreign-format fail-open parse entirely (@operations vacuity-mechanism-4, @test's fail-closed concern), it keeps ONE source for bound-port-vs-exported-URL, and it is run-scoped so concurrent devloop runs cannot collide (@security condition 1). It also turns the dead `.ports.mh_N_grpc` allocation LIVE instead of leaving it a landmine. Flagged to @team-lead as a deviation from ruling #2's stated mechanism.
  - WHY THE BIND IS SAFE — a NON-LOCAL reason, so it gets a comment at the site (@operations verified): layer7 runs inside the devloop container, which has its OWN network namespace (`infra/devloop/devloop.sh:681` uses `--network "$NETWORK_NAME"`, the named podman network from `:611-615`, NOT `--network host`; the "sharing a network namespace" wording at `devloop.sh:5` is loose — it means dev+postgres on one named network, not host netns). On the HOST those same numbers ARE already bound: Kind publishes them as `extraPortMappings` hostPorts (`kind-config.yaml.tmpl:86,103`), so docker-proxy holds them even with nothing behind them in-cluster. Under the `STORY_RUNNER_ALLOW_HOST` hatch the bind would fail EADDRINUSE — LOUD, so no guard is needed, but the comment is, or the next reader thinks the port choice is arbitrary or "fixes" it back to a kubectl-parsed port and reintroduces the parse.
  - Exports `ENV_TEST_MH_{0,1}_GRPC_URL`. REAPS stale forwards first (the setup.sh/teardown.sh `pkill -f "kubectl --context kind-<cluster> port-forward"` pattern), then WAITS-until-ready with a bounded TCP dial poll (never a fixed sleep). Missing kubectl / context / ports.json / `.ports.mh_0_grpc` / forward-did-not-come-up => `precondition_fail` with DISTINCT reason tokens each (the :956-966 precedent, NOT the fail-open `&& export` at :692-696).
  - NO `trap` in layer7.sh (@operations, verified): `scripts/lang/_layer_skeleton.test.sh:75` asserts `^\s*trap\s` absent in every `scripts/layer[0-9]*.sh`, AND `layer_lifecycle_begin` already owns `trap '__layer_lifecycle_end' EXIT` (`_common.sh:591`) which bash would REPLACE, not chain — clobbering the handler that recomputes the layer's exit code, so layer 7 would report green over real failures. Fix per the linter's own prescription: add `layer_register_cleanup` to `scripts/lang/_common.sh`, invoked from `__layer_lifecycle_end`. That gives EXIT-time cleanup on the `precondition_fail`/`set -e` paths (@security condition 2, @test point 4) with no second trap. In-tree script edit, NOT host-side, so the Lead's no-host-change constraint holds. Pulls @infrastructure (already on the panel for kind-config).
  - NO 8083 forward (@operations, verified): the suite ALREADY reads MH per-pod gauges through Prometheus with an `instance` label (`28_mh_egress_admission.rs:81,156,177,180` — `sum by (instance)`), so per-pod targeting is a solved problem. A direct `:8083` scrape would be a SECOND, divergent metrics path that can disagree with Prometheus across its 15s scrape interval — an unattributable flake. This also retires @security condition 3 (no per-pod 8083 forward, so nothing can dedupe against setup.sh's Service-level one) and corrects my own earlier "the ack is synchronous so an instant read is deterministic" claim: that holds for MH's internal state, NOT for what Prometheus has scraped.
  - CONSEQUENCES of the Prometheus-only read, to build: (a) the gauge-FALLS assertion needs a bounded poll-until-BELOW-baseline sibling of `poll_until_any_instance_above` (which only polls upward) — added to `fixtures/metrics.rs` in the same shape, `#[must_use]`-consistent, and its failure message NAMES THE BASELINE AND THE OBSERVED VALUE (not just "did not fall"): a responder reading a CI log needs both numbers (@operations); (b) CONFIRMED-RISE-THEN-FALL, two-phase (@security found it, @test sharpened it): through Prometheus the gauge is an eventually-consistent SAMPLED read at a 15s interval, so a bare poll-until-below-baseline can pass VACUOUSLY — if the registered state was never scraped, "below" is trivially true (§Assertion Vacuity case 1, input absent), and this is exactly the proof the task calls load-bearing. So the fall is measured against an OBSERVED PEAK, never an assumed baseline: (i) after RegisterMeeting, poll-until-ABOVE to confirm Prometheus actually observed the rise FOR THIS INSTANCE — that IS the fall's positive control, and gating here before releasing is also what makes the intermediate peak observable despite the interval; (ii) only THEN release; (iii) poll-until-BELOW the confirmed peak, ideally to the expected post-release value rather than "any decrease". Each phase fails loud with its OWN distinct reason token, so "never observed the before" triages as environment/timing and never as a reclamation bug — if they looked alike, someone would later "fix" the vacuity case by relaxing the fall assertion, which is the one that matters. Bound >= one scrape interval (28 uses 120s) so a slow scrape fails loud rather than flaking. NO "read immediately after the ack" assertion survives against Prometheus — that was the direct-scrape argument and is now flaky; every Prometheus read is poll-until-condition. (c) the target pod is PINNED EXACTLY: the queried `instance` label (pod IP:port) is resolved from the SAME `-l instance=mh-N` lookup that feeds the forward, so the forward target and the queried instance provably resolve to ONE pod. If the forward hit mh-0 but the query filtered another instance the test reads the wrong pod's gauge — a silent test-correctness bug, and "any instance" would let the assertion pass on the handler that was never torn down.
  - Comments at the forward site: kubectl port-forward bypasses the MH NetworkPolicy (dev-only) AND (sec Ask-3) the PLAINTEXT h2c is production-wide, tracked at TODO:1251 — the two facts kept separate so no reader infers "prod is TLS". A forward dies with its pod; a mid-suite MH restart is not re-established (residual at the probe site). Forwards leak if cleanup is SIGKILLed, bounded by teardown.sh:73's existing pkill. Do NOT write the inaccurate "layer7 already kubectl-execs into postgres" precedent into a comment (@security: that exec lives in setup.sh, which layer7 invokes; the conclusion survives, the premise does not).
- `infra/kind/kind-config.yaml.tmpl`: the misleading "future NodePort" mh gRPC comment is corrected to say the env-test route is the layer7 port-forward (comment-only). The ports.json `mh_N_grpc` allocation is LEFT ALONE (changing it is a helper Rust change + host rebuild, out of scope).
- NO setup.sh / helper / NodePort / NetworkPolicy change, so @infrastructure is not pulled in for the mechanism (the kind-config comment is the only infra-owned edit, comment-only).
- Test 29 (`flows`): MH client generated from `proto-gen` ONLY (sec: ADR-0028 crate layering unchanged, only the credential convention changes). AC-issued `meeting-controller` client-credentials token (via `auth_client.rs::client_credentials`/`issue_token`), used only against MH, never logged. Per pod, on a test-unique uuid `meeting_id` (the containing key), always `policy_generation` 1 (never high — no downward path):
  - register(mc A, gen 1, edges): auth PRECONDITION control; registered-meetings gauge + edges rise
  - EndMeeting(mc B) → `FAILED_PRECONDITION`, `rejected_ownership` +1, gauge/edges unchanged
  - EndMeeting(A) → ack, `released` +1, registered-meetings gauge FALLS, edges return
  - EndMeeting again → `unknown_meeting` +1
  - re-register at gen 1 → `applied_generation==1` — the task's REQUIRED live artifact: a long-running pod admits after teardown exactly as a fresh pod does
  THE PROMETHEUS-READ CONTRACT for 29 (every metric read obeys all four; @test): (1) INSTANCE-PINNED, never any-instance — 29 acts on the ONE pod the gRPC forward hit, so both the rise confirm and the fall are read on THAT `instance` label (a `poll_until_INSTANCE_*` pinned helper or a one-instance filter, NOT 28's `poll_until_any_instance_above` which is right only for 28's "some handler breaches"); any-instance would let the rise be seen on mh-0 and the fall read on mh-1, or another suite's activity satisfy it. (2) DELTA FROM A PER-INSTANCE BASELINE, never an absolute "expected registered count" — 28's docstring says other suites SHARE these pods, so `mh_media_registered_meetings` on mh-N is a moving target 29 does not own: capture this instance's baseline first, rise = baseline+1 (29's own contribution), fall = back to <= baseline (or strictly below the observed peak); the below-baseline helper names the per-instance baseline AND observed value (@operations). (3) confirmed-rise-then-fall as in (b) below. (4) each read fails loud with its OWN distinct reason token and has a POSITIVE CONTROL (a sibling arm rising, the 28 admission-counter pattern) so a flat arm cannot pass vacuously. The TEARDOWN COUNTER arms (released / rejected_ownership / unknown_meeting) are Prometheus-lagged too — each a before/after per-instance delta with a positive control. If 29 asserts edges-return-to-baseline, that is a SECOND gauge FALL and gets the identical confirmed-rise-then-fall + instance-pin treatment; `_limit`/`registered_meetings_limit` are static present-at-startup so presence + instance-pin (or assert on both instances) suffices.
  SYNCHRONOUS proofs stay on the gRPC CALL RETURN, never re-expressed as lagged metric reads (@test): Code==FailedPrecondition AND !=PermissionDenied on the reject call, the ack booleans, and applied_generation==1 on the re-register (the "admits like a fresh pod" proof). Only the counter/gauge arms are the lagged part. Cleanup is STRUCTURAL (Drop guard) so a panic cannot leak. No wall-clock gates.
- Integration-tier coverage (below) stays IN ADDITION, per the ruling.

Integration tier (runs IN ADDITION to env-test 29; @test requirements):
- Reject arm OVER REAL gRPC (MhAuthLayer -> tonic -> handler, the register_meeting_integration.rs pattern): assert Code==FailedPrecondition AND != PermissionDenied (a future collapse fails), rejected_ownership +1, ack not-false and not-idempotent-accept, and state fully untouched (routes present, registered-meetings gauge unchanged, edges unchanged). Plus one happy release arm over real gRPC (covers Status serialization + auth on the success path).
- Gauge-FALLS with a POSITIVE CONTROL: register TWO meetings (gauge rises to 2), release one, assert the gauge FALLS to 1 via MetricAssertion delta AND the routes-map entry is gone AND edges returned — the gauge+map+edges triple, not the counter alone.
- unknown_meeting distinguishable: ack true, unknown_meeting +1, no state change.
- Cap: RESOURCE_EXHAUSTED + rejected_meeting_cap; the cap path short-circuits (no apply_policy, so NO second PolicyApplyOutcome, and record_grpc_request(RegisterMeeting,"error") not the bottom "success"). Assert the policy counter moved by EXACTLY 1 and grpc recorded error (obs Note 1: protects sum() as a denominator). Plus missing-key and bounds unit tests.
- Method label: both members emit, zero-init 2x2, the ALL-iteration membership test.
The live-cluster teardown proof is delivered by THIS task's env-test 29 under (A') (Lead ruling on plan approval): gauge falls on the pinned pod, the teardown counter increments, the reject arm fires, and the gen-1 re-register is `applied`. It is NOT relocated to task 12. Path (B) was not taken.

### Honest residual on reclamation
After this task AND task 12, a meeting whose owning MC dies without calling `EndMeeting` is still never reclaimed. The two "trigger: story 2" TODO entries close; that residual is re-filed as one narrow entry (age-out shape, its own planning), framed HONESTLY (sec-6): with the cap now enforced, repeated MC crashes progressively DENY new registrations on the pod against a hard cap, unrecoverable without a restart — worse than the pre-task memory-growth. The re-derived ownership-gap entry also records (sec-5) that the cap turns unbounded-memory-until-OOM into a cheaper, more targeted denial primitive: a scope holder fills 8192 near-free empty meetings that a legitimate MC cannot evict (eviction needs the attacker's mc_id) — net-positive but a new primitive, not purely a mitigation. Operational reclamation starts when task 12 lands, so ratchet prose that says "until task 11" is retargeted to "until MC calls EndMeeting (task 12)", not deleted.

### Docs
Stale prose retightened in the same commit: the catalog's "Identity with mh_media_policy_applies_total" paragraph (rejected_meeting_cap is a new member of the differ-set, counted as a policy outcome but never an admission decision) and its "four static gauges" sentence (now six with the two `_limit`s); internal.proto L442-445 + the L795 parenthetical (ROLLOUT note untouched), metrics.rs docs, catalog (method label, cardinality, ratchet paragraph, `_limit` promise), mh-overview panel 28, mh-alerts.yaml notes (alert itself stays task 18), configmap.yaml tense sites, config.rs `MAX_TOTAL_EGRESS_EDGES_CEILING` docstring, gc_client/session comments, 28 env-test doc, errors_grpc_metrics_integration module doc, runbooks (Scenario 13 + ownership-reject recovery order + meeting-cap arm; deployment checklist R-24 order). TODO.md: close the two entries, re-derive the ownership-gap entry per security H, and (sec Ask-2) fire the existing defer trigger on TODO:1251 (the MC<->MH-plaintext entry) rather than filing a new one — task 11 adds a second RPC on that plaintext channel plus a per-pod MH gRPC port-forward, which fires its "next devloop touching MC<->MH transport" trigger; add the missing MC->MH/50053 + service.write.mh-leak direction (it currently narrates only MH->MC/50052) and re-defer with a dated in-place note per that entry's own convention. The TLS fix itself is DEFERRED (task-sized: cert/SAN for pod-IP peers, advertise->https, MC trust config, rollout order) with @security's explicit acceptance. Port-forward comment (sec Ask-3): keep the two facts separate — the forward is dev-only and bypasses the CNI; the PLAINTEXT is everywhere and tracked at TODO:1251 — so no reader infers "prod is TLS, dev is the exception". Meeting-cap runbook arm states (per @paired-meeting-controller) that MH's `rejected_meeting_cap` outcome is the discriminator: MC does not classify RegisterMeeting statuses and shows only a generic push failure plus bounded retries.

### Rollout / rollback
Key already in ConfigMap + both deployments (verified); MH before MC (R-24); rolling MH back is safe (key survives, MC gets UNIMPLEMENTED, counted, not retried).

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Protocol (GSA owner) | confirmed |
| Paired Meeting Controller | confirmed |

### Lead Rulings
1. Env-test ruling #1 (MH gRPC via "existing" port-forward) — premise FALSE (no MH gRPC forward; ports.json mh_N_grpc are dead node-port mappings). Caught by operations/security/test/implementer.
2. Env-test ruling #2 (A'): layer7.sh starts per-workload `kubectl port-forward deployment/mh-{0,1}` (50053, 8083) from inside the container (container kubectl access verified), exports ENV_TEST_MH_* URLs, TCP positive control, distinct precondition_fail tokens, trap teardown. No setup.sh/helper/NodePort/NetworkPolicy change, no host action. Keeps the task's live-cluster teardown artifact.
3. Mechanics amendments accepted at Gate 1: bind allocated `.ports.mh_N_grpc` (not a parsed kubectl port); cleanup via new `layer_register_cleanup` in `scripts/lang/_common.sh` (no trap in layer scripts); 8083 forward dropped — metrics via instance-pinned Prometheus reads. `infrastructure` added as a Gate-3 reviewer for its Minor-judgment rows.
4. Classification guard: `STATUS=OK`. Plan approved.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Teardown (R-20)
| Item | Before | After |
|------|--------|-------|
| `EndMeeting` RPC | `UNIMPLEMENTED` stub | released / unknown (ack) / `FAILED_PRECONDITION` on mismatch; malformed ids `INVALID_ARGUMENT`; wedged actor `UNAVAILABLE` within `MH_POLICY_APPLY_TIMEOUT_MS` |
| Ownership comparison | one inline `!=` driving a WARN | `session::Ownership::of` (byte-exact, no normalisation, not constant-time) drives BOTH the takeover WARN and the reject; one log vocabulary `registered_mc_id`/`caller_mc_id` |
| Routing release | none (insert-only) | `RoutingTable::remove` → `without_meeting`; ONE edge-accounting kernel `edges_after_displacing` shared with install and projection; forgets the applied generation |
| Session reclamation | registrations, notifiers, emptied connection entries never removed | release removes the registration and active entry and closes ACTIVE connections via a per-connection `close` token (pending left to the timeout); notifiers dropped with their last pending connection or on promotion; emptied connection entries dropped |
| Race: connection added after release | inserted into a map nothing would release | closed instead |
| Pre-existing lost wakeup (connection registering in the gap before `notified()` existed) | waited out the timeout, then kicked | `Notified` created and enabled first, then registration re-checked |
| `mh_media_meeting_teardowns_total{outcome}` | — | released / unknown_meeting / rejected_ownership, cached actor handles, zero-initialised |
| `mh_grpc_requests_total{method}` | `record_grpc_request(status)` hard-coded `register_meeting` | `record_grpc_request(GrpcMethod, status)`, `GrpcMethod::ALL: [Self; 2]`, 2×2 zero-init, the `&str` const retired |

### Capacity (R-21)
| Item | Before | After |
|------|--------|-------|
| `MH_MAX_REGISTERED_MEETINGS` | in the ConfigMap, unread | REQUIRED on `PolicyLimits`; `MAX_REGISTERED_MEETINGS_CEILING` 65 536; 0 refused; advisory WARN below 100 |
| Cap enforcement | none | actor-atomic, NEW ids only; `RESOURCE_EXHAUSTED`; exactly one policy outcome `rejected_meeting_cap` (`PolicyApplyOutcome::ALL` 6→7) + grpc error |
| Gauges | `mh_media_egress_edges` only | + `mh_media_registered_meetings` (`set(len())` at every mutation, 0 at construction, published BEFORE the actor's reply), `mh_media_egress_edges_limit`, `mh_media_registered_meetings_limit` (from the fields enforcement reads) |

### Env-test proof (Lead ruling #2, path A')
`scripts/layer7.sh` step (i) forwards each MH POD's gRPC (label-resolved; one lookup feeds the forward and the exported pod IP), bound to the slug's allocated `.ports.mh_N_grpc`, after checking the port is FREE; bounded readiness (liveness then acceptance); four distinct precondition tokens; forwards stopped on every exit path by the new `_common.sh::layer_register_cleanup` (no `trap` in a layer script). `29_mh_meeting_teardown.rs`, per pod: register → confirmed rise → foreign release `FAILED_PRECONDITION` + `rejected_ownership` (still held) → release `released` + gauge FALLS below the observed peak + edges return → repeat `unknown_meeting` → re-register at generation 1 is `applied` (the fresh-pod proof); both `_limit` gauges equal the deployed ConfigMap. Every metric read is instance-pinned, relative to that pod's baseline, and a bounded poll; synchronous proofs stay on the gRPC response; structural `Drop` cleanup.

### Additional Changes
- Stale "until task 11" / `UNIMPLEMENTED` prose retightened everywhere it asserted an absence that no longer holds: `internal.proto` service note and SEMANTICS attribution, `metrics.rs`, `config.rs`, `gc_client.rs`, `session/mod.rs`, catalog, both dashboards, `dashboards.md`, `mh-alerts.yaml`, `configmap.yaml`, runbooks, tests 01/28. Operational reclamation is retargeted to story 2 task 12 (when MC calls `EndMeeting`), not deleted.
- `mh-overview.json`'s policy-apply panel had omitted `rejected_stream_ceiling` (task 8); both it and `rejected_meeting_cap` are now described.
- `docs/TODO.md`: two story-2 entries CLOSED; the ownership-gap entry RE-DERIVED (open; three defects, cap as a cheaper denial primitive, principal set + on-path bound, recovery procedure pointed at the runbook); the plaintext-channel entry's defer trigger FIRED and re-deferred with the MC→MH direction added; the `end_meeting` present-at-zero entry CLOSED; a narrow residual entry filed (a meeting whose MC never calls `EndMeeting` now consumes a HARD cap); the saturation-gauge entry annotated (instrument landed, alert open).
- `configmap_key`/`configmap_u64` hoisted to `env_tests::fixtures::kube` at their second consumer.
- Two real defects found by the new tests before landing: the layer7 pod lookup's trailing space would have exported an EMPTY pod IP (a pin matching nothing); a readiness probe without the free-port check would pass against the wrong listener on the host.

### Mutation checks run (each red, then restored green)
registration not removed on release → gauge assertion red (`expected 1, got 2`); cleanup hook unguarded → `_common.test.sh` red (no STATUS line, exit 7); layer7 cleanup unregistered → forwards-still-running red; free-port check removed → occupied lane red.

### Host action
None. Layer 7 rebuilds and starts its own forwards inside the container. Test 29 cannot pass against the currently running (pre-change) MH image, so Gate 2's rebuild is required; verified live instead: the allocated port is free in the container netns, the forward accepts, and the Prometheus `instance` for MH is `podIP:8083`.

---

## Files Modified

```
 Cargo.lock                                         |   1 +
 crates/env-tests/Cargo.toml                        |   8 +
 crates/env-tests/README.md                         |   5 +-
 crates/env-tests/src/fixtures/metrics.rs           | 116 ++++
 crates/env-tests/src/fixtures/mod.rs               |   1 +
 crates/env-tests/tests/01_mh_deployment_config.rs  |   4 +-
 crates/env-tests/tests/28_mh_egress_admission.rs   |  61 +-
 crates/mh-service/src/config.rs                    | 140 +++--
 crates/mh-service/src/grpc/gc_client.rs            |  11 +-
 crates/mh-service/src/grpc/mh_service.rs           | 580 ++++++++++++++++--
 crates/mh-service/src/main.rs                      |  25 +-
 crates/mh-service/src/observability/metrics.rs     | 355 ++++++++---
 crates/mh-service/src/routing/mod.rs               | 136 ++++-
 crates/mh-service/src/session/mod.rs               | 655 ++++++++++++++++++---
 crates/mh-service/src/webtransport/connection.rs   | 151 ++++-
 crates/mh-service/tests/auth_layer_integration.rs  |  62 +-
 crates/mh-service/tests/common/grpc_rig.rs         |  21 +-
 .../tests/errors_grpc_metrics_integration.rs       |  59 +-
 .../tests/media_session_binding_integration.rs     |  12 +-
 .../tests/otel_webtransport_integration.rs         |   8 +-
 .../mh-service/tests/policy_apply_integration.rs   |  14 +-
 .../tests/stream_admission_integration.rs          |  19 +-
 .../tests/webtransport_accept_loop_integration.rs  |   8 +-
 .../mh-service/tests/webtransport_integration.rs   |  16 +-
 crates/mh-test-utils/src/admission.rs              |   3 +-
 docs/TODO.md                                       |  49 +-
 docs/observability/dashboards.md                   |   4 +-
 docs/observability/metrics/mh-service.md           |  79 ++-
 docs/runbooks/devloop-validation.md                |   6 +-
 docs/runbooks/mh-deployment.md                     |  12 +-
 docs/runbooks/mh-incident-response.md              |  13 +-
 docs/specialist-knowledge/media-handler/INDEX.md   |  18 +-
 infra/docker/prometheus/rules/mh-alerts.yaml       |  18 +-
 infra/grafana/dashboards/mh-media.json             | 210 ++++++-
 infra/grafana/dashboards/mh-overview.json          |   4 +-
 infra/kind/kind-config.yaml.tmpl                   |  14 +-
 infra/services/mh-service/configmap.yaml           |  50 +-
 proto/dark_tower/internal/v1/internal.proto        |  13 +-
 scripts/lang/_common.sh                            |  72 +++
 scripts/lang/_common.test.sh                       |  85 +++
 scripts/layer7.sh                                  | 218 +++++++
 scripts/layer7.test.sh                             | 213 ++++++-
 42 files changed, 3073 insertions(+), 476 deletions(-)
crates/env-tests/src/fixtures/kube.rs
crates/env-tests/tests/29_mh_meeting_teardown.rs
crates/mh-service/tests/end_meeting_integration.rs
```

---

## Devloop Verification Steps

### Gate 2 — Validation (Lead), attempt 1
`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=N/A`, exit 0. L1 OK, L2 OK, L3 OK (102s), L4 N/A (cargo-test + nx-test passed; proto intentional-gap placeholder), L5 OK, L6 N/A (audit dep-manifest gate + proto placeholder; cargo/pnpm audit + buf breaking passed), L7 OK (env-tests-passed incl. `29_mh_meeting_teardown` 1 passed in 108s; browser-e2e-passed; 1270s). No FMT_APPLIED.


### Gate 2 — re-validation after Gate-3 fixes, attempt 2
`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=N/A`, exit 0. L1 OK, L2 OK, L3 OK, L4 N/A (cargo/nx tests passed; proto placeholder), L5 OK, L6 N/A, L7 OK (env-tests incl. `29_mh_meeting_teardown` + browser E2E passed; 1106s). No FMT_APPLIED.

---

## Code Review Results

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | Hardcoded MC client secret in test 29 → read from live Secret/Deployment via `kube.rs::secret_key`; GSA proto edit confirmed |
| Test | CLEAR | 0 | 0 | 0 | |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | OBS-F1: unqualified `method=` selectors in catalog/runbook/comments |
| Code Quality | CLEAR | 0 | 0 | 0 | |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | F1: duplicated id validation → `validate_caller_ids`; 2 extraction opportunities filed (ADR-0019 exception) |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | pod-lookup stderr discarded; stale pkill-precedent comment |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | |
| Protocol (GSA owner) | CLEAR | 0 | 0 | 0 | proto comment-only; contract semantics verified |
| Paired Meeting Controller | RESOLVED-FIXED | 1 | 1 | 0 | F1: stale NotifyParticipantDisconnected after release → structural `ConnectionEnd` |
| Infrastructure | RESOLVED-FIXED | 6 | 6 | 0 | pkill absent in container (silent no-op) → argv0-restricted /proc reap; `_common.sh` empty-array under `set -u` on bash<4.4; ss remediation; bounded port-free poll; terminating-pod filter test; runbook remedy clause |

**DRY extraction opportunities** (appended to `docs/TODO.md` §Cross-Service Duplication (DRY)): live-ConfigMap reads have two homes in `env-tests`; `pub const ALL: [Self; N]` mirror-gap entry amended (8 arrays in MH).

**Follow-up recorded by Lead** (surfaced by infrastructure, not a finding in this diff): `docs/TODO.md` §Devloop Container Resource Hygiene & Build Isolation — host-side `pkill -f` port-forward reaps in setup.sh/teardown.sh are over-broad.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied

---

## Issues Encountered & Resolutions

### Issue 1: Ruling #1's env-test premise was false
**Problem**: ports.json's `.ports.mh_N_grpc` pointed at NodePorts that never existed. MH gRPC is ClusterIP-only, and its NetworkPolicy admits only MC.
**Resolution**: Found independently by @operations, @security and @test. Lead ruling #2 (A'): Layer 7 starts its own per-pod forwards inside the container.

### Issue 2: Gate-3 findings, all fixed (no deferrals)
- **@paired-meeting-controller F1**: the no-MC-notification-after-release guarantee rested on timing. It is now structural (`ConnectionEnd::MeetingReleased` → no notice), with a unit test.
- **@observability OBS-F1**: three unqualified `mh_grpc_requests_total` selectors, plus two more in code comments, are now qualified with `method="register_meeting"`.
- **@security**: test 29 hard-coded a duplicated MC secret. It now reads the live Secret and Deployment (`kube::secret_key`, `kube::deployment_env_value`), and a refusal names the credential.
- **@dry-reviewer F1**: the id validation was copied across both RPCs. It is now one `validate_caller_ids`.
- **@operations**: the pod-lookup stderr was discarded. Its tail is now in the failure text. The step-(i) comment also no longer credits `pkill`.
- **@infrastructure F1–F6**:
  - The stale-forward `pkill` reap was a permanent silent no-op, because the container has no procps. It is now a `/proc` scan restricted to argv[0]=kubectl, and reports "reaped: N".
  - The `ss` remediation is replaced.
  - The free-port check is now a bounded poll.
  - The cleanup loop's empty-array expansion broke every layer on bash 4.0–4.3. It is now guarded, with a zero-cleanups test.
  - The terminating-pod filter now has a test.
  - The runbook remedy covers the `/proc` case.

### Issue 3: The first reaper killed the shell that wrote its own test
**Problem**: A bare cmdline-substring match killed any process whose argv merely contained the pattern. That included the tool shell whose heredoc held the test text.
**Resolution**: Only processes whose argv[0] basename is `kubectl` are candidates. The bystander test failed under a mutation that removed the check.

### Issue 4: A vacuous self-check grep hid a clippy error
**Problem**: I reported "clippy clean" after the DRY fix. The grep `^(error|warning)` never matches cargo's colourised output, so `clippy::result_large_err` on `validate_caller_ids` went unseen until `layer-fast.sh` went red.
**Resolution**: The validator returns the message and callers build the `Status`. The check now uses clippy's exit code, not a grep.

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
