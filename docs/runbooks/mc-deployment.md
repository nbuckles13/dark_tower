# MC Service Deployment Runbook

**Service**: Meeting Controller (mc-service)
**Version**: Phase 4a (ADR-0010 Implementation)
**History**: `git log --follow -- docs/runbooks/mc-deployment.md`
**Owner**: Operations Team

---

## MC Topology — Read This Before Any `kubectl` Command

MC ships as **two singleton Deployments**, `mc-0` and `mc-1` (each `replicas: 1`).
There is **no `deployment/mc-service`** — `mc-service` is a Service, a PDB and the
container name, so `kubectl ... deployment/mc-service` fails with
`deployments.apps "mc-service" not found`.

- **Every command below names `mc-0`. Repeat it for `mc-1`.** A symptom on one
  instance says nothing about the other; they are independent processes with
  independent state.
- **Health/metrics is port `8081`**, not 8080. Port-forwards here map local 8080
  to remote 8081, so `curl localhost:8080/metrics` is correct once forwarded.
  **Do not "simplify" these to `service/mc-service`.** The `mc-service` ClusterIP
  in `infra/services/mc-service/service.yaml` selects on `app` + `component` and
  deliberately omits `instance:`, so it fans across mc-0 and mc-1 both and would
  scrape a nondeterministic instance — a silent wrong answer for a metrics check,
  rather than an error. `deployment/mc-0` is the only form that names what you
  are measuring.
- **You cannot `curl` MC's health port from another pod, and the failure is a
  TIMEOUT rather than a refusal.** MC's NetworkPolicy
  (`infra/services/mc-service/network-policy.yaml`) admits TCP 8081 from
  **Prometheus only**; GC and MH are admitted on gRPC 50052 and nothing else. So
  `curl http://mc-service.dark-tower.svc.cluster.local:8081/health` from a GC,
  MH or debug pod is silently dropped — and a hang, during an incident, reads as
  "MC is wedged", which is precisely the conclusion a dependency-health check
  exists to rule out. **The sibling services are not symmetric and that is what
  makes this a trap**: AC admits 8082 from gc/mc/mh and GC admits 8080 from
  everywhere, so in a block of three dependency curls the AC and GC lines
  genuinely work and only the MC line cannot. Use the port-forward form below
  from the operator's own machine, per instance. To answer "can GC reach MC at
  all", read `kubectl get endpoints mc-service -n dark-tower` plus GC's own
  registration/heartbeat metrics — MC's health endpoint is not the instrument.
- **Do not add replicas.** `MC_WEBTRANSPORT_ADVERTISE_ADDRESS` comes from a
  per-instance ConfigMap, so extra replicas all advertise the same address to GC
  and clients get routed to a pod that does not hold their session (ADR-0023
  session binding). Capacity is added by adding an *instance*, not a replica.
  The manifests state this at the `replicas: 1` field itself — see the comment
  above `replicas:` in `infra/services/mc-service/mc-0-deployment.yaml`, which
  carries the same arithmetic.
- **`kubectl exec` depends on the image variant.** `infra/docker/mc-service/Dockerfile`
  defines both a shell-free `runtime` stage (distroless `cc-debian12`) and a
  `runtime-with-healthcheck` stage (`:debug` + busybox). The build passes no
  `--target`, so the last stage wins and busybox is present *by stage ordering,
  not by decision*. If a `--target runtime` ever lands, every `exec`-based step
  in this runbook stops working at once — prefer the shell-free alternatives in
  §Diagnostic Commands where they exist.

---

## Overview

This runbook covers deployment, rollback, and troubleshooting procedures for the Meeting Controller service. The MC service is responsible for WebTransport signaling, session management, and participant coordination within meetings.

**Critical Service**: MC downtime affects all active meetings. Users will be disconnected and unable to communicate. Follow pre-deployment checklist carefully.

---

## Manifest Structure

MC service Kubernetes manifests are managed via Kustomize with a base/overlay pattern:

```
infra/
├── services/mc-service/                    # Base manifests
│   ├── kustomization.yaml                  # Explicit resource list
│   ├── config.env                          # configMapGenerator source (name hash-suffixed)
│   ├── deployment.yaml
│   ├── service.yaml
│   ├── secret.yaml
│   ├── pdb.yaml
│   ├── network-policy.yaml
│   └── service-monitor.yaml               # Present in dir but not in kustomization.yaml (needs Prometheus Operator CRD)
└── kubernetes/overlays/kind/
    └── services/mc-service/
        └── kustomization.yaml             # Kind overlay — refs base, adds Kind-specific labels
```

- **Base** (`infra/services/mc-service/`): Contains all production manifests. The `kustomization.yaml` explicitly lists each resource. Files like `service-monitor.yaml` are present in the directory but omitted from `kustomization.yaml` when they require CRDs not available in all environments.
- **Kind overlay** (`infra/kubernetes/overlays/kind/services/mc-service/`): References the base and adds Kind-specific labels.
- Deploy with: `kubectl apply -k infra/kubernetes/overlays/kind/services/mc-service/`

> **Note:** The MC WebTransport TLS secret (`mc-service-tls`) is created imperatively by `infra/kind/scripts/provision.sh` (via `create_tls_secret()`) when the platform is provisioned, not managed by Kustomize. Ensure TLS secrets are provisioned before deploying MC. See [Common Deployment Issues](#common-deployment-issues) for TLS troubleshooting.

---

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Deployment Steps](#deployment-steps)
3. [Rollback Procedure](#rollback-procedure)
4. [Configuration Reference](#configuration-reference)
5. [Common Deployment Issues](#common-deployment-issues)
6. [Smoke Tests](#smoke-tests)
7. [Monitoring and Verification](#monitoring-and-verification)

---

## Pre-Deployment Checklist

Complete ALL items before deploying to production:

### Code Quality

- [ ] **Code review approved** by at least one reviewer
- [ ] **CI tests passing** in GitHub Actions
  ```bash
  # Verify CI status
  gh pr checks <PR-NUMBER>
  ```
- [ ] **Test coverage meets minimum** (targeting 90%+ for critical paths)
  ```bash
  # Check coverage report
  cargo llvm-cov --workspace --lcov --output-path lcov.info
  ```
- [ ] **Linting passes** with zero warnings
  ```bash
  cargo clippy --workspace --lib --bins -- -D warnings
  ```
- [ ] **Security scan passes** (Trivy, no CRITICAL vulnerabilities)
  ```bash
  trivy image mc-service:latest --severity CRITICAL
  ```

### Infrastructure

- [ ] **Config changes documented** in this runbook or ADR
  - New environment variables added to ConfigMap/Secret
  - Default values appropriate for production
  - Breaking changes communicated to dependent services (GC, MH)

- [ ] **Rollback plan confirmed**
  - Previous container image available in registry
  - Rollback criteria defined (see [Rollback Procedure](#rollback-procedure))

- [ ] **Capacity planning verified**
  - Current resource utilization <70% (CPU/memory)
  - Active meeting count within capacity limits
  - GC has awareness of MC capacity

- [ ] **GC coordination confirmed**
  - GC informed of maintenance window (if applicable)
  - GC can route new meetings to other MC instances
  - Draining period sufficient for active meetings to complete

### Coordination

- [ ] **Story 4 P3 retired the superseded signalling surfaces** (`JoinResponse.media_servers`,
  `SendTarget`/`StreamAssignment.media_handler_url`, `MhConnectionStatus.mh_url`,
  `MhConnectRequest.join_token`). Its deploy order and rollback floors are stated once in
  `mh-deployment.md` §Rollback ("P3 coupling"). MC-local facts: the
  `MCMediaConnectionAllFailed` input (`mc_participant_mh_status_total{state}`) keeps its name,
  labels and expression across the roll (only its emit site moved), and
  `mc_participant_mh_status_dropped_total` loses its `reason="cap"` series (2 reasons now: no
  post-deploy check expects `cap`).
- [ ] **Story 3 ("see each other") is ONE coordinated cutover: config → MH → MC → browser SDK,
  and rolling back across it is all components together** (R-25, confirmed 2026-10-08). The
  authority, with the skew-detection table and the three-layer rollback, is
  [`mh-deployment.md` §Story 3 cutover](mh-deployment.md#story-3-cutover-see-each-other-order-skew-detection-rollback).
  The MC view is [§Story 3 cutover](#story-3-cutover-see-each-other-the-mc-view) below. **For that
  cutover it supersedes every ordering bullet that follows in this list.** Those bullets were written
  for story-2 releases. Within the v3 era they impose no order (R-25: one component, one version
  back, holds), and across the cutover nothing rolls alone. The bullets most likely to mislead are
  "the browser SDK must roll BEFORE this MC image" and the `sender_id` rule's "forward MC first",
  because story 3 puts MH before MC and the SDK last. *(Added 2026-10-21, story 3 OPS-A.)*

- [ ] **MC and the browser SDK ship together** (wire-lockstep)
  - The ADR-0036 signalling reshape is a breaking wire change: `ClientMessage`
    tag 8 changed message type, and stream assignments moved from tag 5 to 12.
    A split deploy fails in the decodes-successfully-wrong-semantics direction,
    not with a loud error — so MC and the SDK build (`@darktower/sdk-core`
    consumers) must be deployed as one coordinated release, not independently.
  - **MH is NOT in the coupled set for the `signaling.proto` reshape**:
    `MhClientMessage` is unchanged and no `mh-service` source is in it, so MH
    deploys independently. **This changes when the `internal.proto` reshape
    (story task 4) lands** — that is the MC↔MH contract, and MH joins the
    coupled set then; revisit this line at that point.
  - Note: `## Rollback Procedure` below covers rolling back the *repo*, not a
    *deployment* — a deployment rollback must revert MC and the SDK together.
  - **Story 2 task 13 (`JoinResponse.max_receive_slots`) needs NO roll order.**
    The field is additive: an older SDK ignores it; a newer SDK against an older
    MC sees it absent, treats the cap as unknown and declares, and MC's
    whole-declaration rejection (`slot_count_over_cap`) stays the enforcer.
    *(Pre-story-3 history. Since story 3 MC-A each kind is truncated to its own
    cap and the excess slots show `unavailable`; an over-cap declaration is
    never rejected, `slot_count_over_cap` is retired, and MC no longer writes
    `max_receive_slots` (tag 12) — only the per-kind tags 13/14; MC-B deleted
    tag 12 from the wire and reserved it. Since 2026-10-25 tags 13/14 are
    reserved too and the caps ride `receive_slot_caps` (tag 15), one entry per
    known kind. The first bullet of this list governs the story-3 roll.)*
  - **Story 2 task 20 (shared-handler edge model) adds a VERSION-SKEW WINDOW,
    not just an ordering rule** — see Deployment Steps §0. MC now offers every
    handler and may direct a sender at SEVERAL handlers and a subscriber's slots
    at different handlers. An SDK predating task 20 **silently** sends only to
    its first target (no fault, no metric — "some peers hear me, some don't"
    with every client signal green) and **loudly** faults on slot assignments
    naming two handlers. Browsers hold cached bundles, so the exposure is "new MC
    + any old SDK still loaded", whatever order the deploys land in. The
    reassuring half: an **all-connected** meeting co-locates on one handler (one
    target per sender, one url per subscriber), which an old SDK handles
    correctly — the skew bites only under genuinely partial connectivity.
  - **`connection_id` is REQUIRED from story 4 MC-B1 — a HARD ordering.** This
    MC refuses an empty `connection_id` (`sender_id` 0, nothing recorded,
    counted `mc_mh_notifications_unapplied_total{reason="connection_id_missing"}`),
    so an MH older than the story 2 task 20 (`connection_id`) image gets every
    Connected refused: **a total media outage for that handler**. Every MH must
    be at or above the task-20 image BEFORE this MC rolls, and while this MC is
    live no MH may be rolled back below it. The pre-MC-B1 detector
    (`mc_mh_notifications_without_connection_id_total`) is retired by the same
    release: read it on the OLD MC before rolling (§Pre-Deployment Verification).
  - **MC-B1 is the floor MH-2 requires (story 4).** Rolling this MC forward
    before MH-2 is safe: an older MH ignores `RegisterMeetingRequest.heartbeat_interval_ms`
    (an unknown proto3 field) and never calls `MeetingHeartbeat`. Once MH-2 is
    live, rolling MC back below MC-B1 is UNSAFE: MH-2 refuses every
    `RegisterMeeting` carrying the old MC's 0 interval (a total media outage for
    new and re-pushed meetings) and every cause-less `EndMeeting` (released
    meetings keep their streams); its heartbeats get UNIMPLEMENTED, counted with
    no action. (MH-3 later adds the owner-silence release, which would then also
    end those meetings.) Roll MH back first (the `internal.proto` ORDERING rule;
    `mh-deployment.md` §Rollback, MH-2 coupling).
  - **MC-B2 is the first MC that ACTS on missing heartbeats (story 4) — a
    HARD ordering.** A handler is declared Dead when no accepted heartbeat
    arrived for longer than N after its first confirmed `RegisterMeeting`, and
    is then REMOVED from the meeting (its clients are told, its meeting is
    ended on it). An MH older than MH-2 never heartbeats, so every handler it
    runs is declared Dead at N and **every meeting on it loses media**. Every
    MH must be at or above MH-2 BEFORE this MC rolls, and while this MC is
    live no MH may be rolled back below MH-2. Pre-deploy check:
    `mh_mc_heartbeats_total{outcome="success"}` is rising on EVERY MH pod
    (and `mc_media_handler_heartbeats_total{outcome="owned"}` is rising on the
    current MCs). Nothing in this release replaces a removed handler (that is
    MC-C): a meeting left with no handler stays without media until it ends,
    and shows on `mc_media_meetings_without_handlers`.
  - **A new REQUIRED key rides this release** (`MC_MEDIA_CONNECT_SETTLE_MS`;
    from story 2 task 9, `MC_KEK_ROTATION_DEBOUNCE_SECONDS`; from story 4
    MC-B1, `MC_MH_HEARTBEAT_TIMEOUT_MS`; and from story 4 MC-D,
    `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS`, whose revert is coupled to MH-1 — see
    its §Configuration Reference row; from story 4 MC-F,
    `MC_PLANNED_TRANSFER_MARGIN_MS`; and from story 4 MC-E,
    `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` and
    `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS`).
    Apply the ConfigMap and both Deployments with or before the image: a new
    image against a stale in-cluster ConfigMap crash-loops on
    `MissingEnvVar("MC_MEDIA_CONNECT_SETTLE_MS")` (or
    `MissingEnvVar("MC_KEK_ROTATION_DEBOUNCE_SECONDS")`,
    `MissingEnvVar("MC_MH_HEARTBEAT_TIMEOUT_MS")`, or
    `MissingEnvVar("MC_MEDIA_DIAL_TOKEN_TTL_SECONDS")`) — and the last time this
    happened (`MC_MAX_RECEIVE_SLOTS`, story 2, recorded in `docs/TODO.md`) the presenting
    symptom was a *port-allocation* error, not the missing key. Env-test
    `02_mc_deployment_config.rs` (Layer 7) catches a key missing from mc-0's or
    mc-1's wiring or from the live ConfigMap, per workload; it does NOT catch an
    image applied against a stale in-cluster ConfigMap, which still crash-loops
    on `MissingEnvVar` — apply the ConfigMap first.
  - **The browser SDK must roll BEFORE this MC image (story 2 task 9, R-16) —
    and nothing on the wire can tell you it hasn't.** This MC reissues a
    `sender_id` after a meeting exhausts its namespace (a KEK-epoch reset). A
    pre-story-2 SDK scopes its replay state by `sender_id` alone, so it drops
    the reissued sender's frames as replays of the id's previous holder:
    **that sender becomes inaudible to that participant, silently and
    permanently**, until the user reloads. The reverse (old MC, new SDK) is
    benign — the finer scope is simply never exercised. **This fix needed no
    wire change, which is exactly why there is no detector**: no field, no
    negotiation, no counter; MC cannot tell an old client from a new one.
    Do not read "no protocol change" as "nothing to coordinate". This one is
    one-directional and has no detector. And it is **not** the task-20 tail's kind:
    that degradation is transient and self-correcting, this one is permanent
    for the affected sender until the page is reloaded and invisible to both
    parties — the distinction that matters when deciding whether to force a
    bundle refresh. It is an **availability** failure, not a security one:
    a pre-story-2 receiver drops rather than mis-attributes, verification
    always uses the current roster key, and the one-`sender_id`-per-identity-
    per-KEK-generation invariant holds server-side whatever the client version.
    It only bites in a meeting that has exhausted its `sender_id` namespace (size set by
    the allocator, `crates/mc-service/src/media_admission/sender_id.rs`).
  - **The Prometheus rules are a FOURTH artifact in this release's coupled
    set, and the coupling runs both ways.** `infra/docker/prometheus/rules/`
    is content-hashed through `configMapGenerator`
    (`infra/kubernetes/observability/kustomization.yaml`), so it self-rolls;
    the MC ConfigMap does not. This release retires `MCSenderIdSpaceExhausted`
    (exhaustion now self-repairs). **Retire it with or after the MC image, and
    restore it with or before any MC image rollback**: a pre-story-2 image
    under the new rules refuses admissions at exhaustion permanently with
    NOTHING firing — the terminal reject is back, its detector is gone, and the
    replacement info rule keys on a trigger that image never emits. Rules that
    land ahead of the image are benign but SILENT (they compare series the old
    image does not emit), so an operator who sees no KEK alerts after shipping
    the rules has not learned that rotation is healthy.
  - **Identity-key handling adds NO deploy-ordering constraint** beyond the
    tag-reshape lockstep above. MC admits a joiner that sends no
    `identity_public_key` (length 0 is the contract's NO KEY PUBLISHED state), so
    an un-updated SDK still joins; it is counted on
    `mc_join_identity_key_presence_total{presence="absent"}`, not as a failure.
    What the admission work adds is a new *observable*:
    `error_type=identity_key_invalid` on `mc_session_join_failures_total` means a
    client sent a **wrongly-encoded** key (PEM/JWK/base64 rather than the raw 32
    bytes), never that a client has not implemented the field.
  - **Story 4 MC-C (crash replacement via GC) requires GC ≥ story 4 GC-3:
    forward GC → MC; rollback MC → GC.** MC-C makes an MC REFUSE to register
    (and re-register) when `RegisterMCResponse.max_exclude_handler_ids` is 0 or
    absent, because it derives each meeting's replacement attempt bound from
    it. A GC predating GC-3 sends 0. **Forward:** GC (≥ GC-3) is fully rolled
    before any MC pod runs this image. **Rollback:** roll MC back past MC-C
    before GC goes below GC-3; otherwise every MC registration AND the heartbeat
    path's re-registration fail (both go through the same accept path), and
    replacement stops working. **Symptom an on-call sees — two cases:** (a) an MC pod
    that STARTS against a pre-GC-3 GC never turns Ready; it logs an ERROR on
    `mc.grpc.gc_client`, `field=max_exclude_handler_ids`, "GC sent
    max_exclude_handler_ids=0 (absent): GC predates story 4 GC-3. Roll GC
    forward (or this MC back)", every 5 s from the GC task's retry loop.
    (b) An MC that is ALREADY RUNNING when GC goes below GC-3 STAYS Ready
    (readiness is set once, at the first registration, and GC's registry row
    survives the GC restart, so heartbeats keep succeeding): replacement is
    then silently dead. After any MH loss
    `mc_media_handler_migrations_total{outcome="gc_unavailable"}` rises and
    the replacement attempt logs carry `code=Unimplemented`; the
    `max_exclude_handler_ids` ERROR appears only if GC answers a heartbeat
    NOT_FOUND and MC re-registers. Procedure: roll GC forward to ≥ GC-3, or roll
    MC back past MC-C. GC side: `gc-deployment.md` §"How to Rollback"
    (**Rollback floor: `max_exclude_handler_ids`**). The two new MC keys
    (`MC_MH_REPLACEMENT_MIN_INTERVAL_MS`, `MC_MH_REPLACEMENT_BACKOFF_MAX_MS`)
    must be in the ConfigMap in the SAME apply as the image (see their rows
    below); an image-only rollback is safe (an older MC ignores them).
  - **Story 2 task 12 (server mute + meeting teardown) orders the rollout
    across THREE services as ONE total order: forward GC → MC → MH; rollback
    MH → MC → GC.** It composes with the standing MC/MH rule in
    `mh-deployment.md` §"Cross-service ordering: MC and MH are coupled by the
    sender_id binding contract" (forward MC first, back MH first), and **that
    rule wins wherever the two appear to conflict** — its skew is a total media
    blackout, this one's is a counted degradation. The two reasons:
    - **GC before MC** — a meeting now ENDS when its last participant leaves,
      and MC tells GC (`NotifyMeetingEnded`). A GC without the task-12
      assignment fix cannot re-assign an ended meeting id: every later join to
      that id fails until the retention cleanup removes the row
      (`GC_RETENTION_DAYS`, 7 days by default). A cross-service rolling deploy
      IS that skew window, so GC must be fully rolled before any MC pod runs
      this image, and MC must be rolled back before GC is.
    - **MH after MC is safe, not merely tolerated** — every MH since story 2
      task 11 serves `EndMeeting`; this task's only MH-side change is the
      apply-time refusal for a released meeting. While MH lags, an MC release
      that races a late apply is not refused MH-side (Race A in
      `EndMeetingRequest`, `internal.proto`), which leaves at most that one
      registration until the handler restarts — and rolling MH restarts it.
      An MH that predated the RPC entirely would answer `UNIMPLEMENTED`,
      counted on `mc_media_end_meeting_total{outcome="unimplemented"}` and
      never retried: a degradation, not an outage.
  - **Mid-roll, the two MC pods legitimately disagree about meeting counts.**
    A new mc-0 ends empty meetings, releases their handlers and ends their GC
    assignments; an old mc-1 does not. Per-instance active-meeting counts, MH
    registered-meeting counts and GC assignment counts diverge for the duration
    of the roll — expected, not a defect. §1 Pre-Deployment Verification gates
    on the active-meeting count per instance; read it with this in mind.
  - **What operators see differently after this release**: a meeting whose last
    participant leaves is gone (a rejoin creates a new meeting — same meeting
    id, fresh sender ids and KEK); `mc_media_end_meeting_total{outcome="released"}`
    and `mc_gc_notify_meeting_ended_total{status="success"}` move on ordinary
    turnover. Teardown triage: `mc-incident-response.md` Scenario 20.


- [ ] **Maintenance window scheduled** (if downtime expected)
  - Dependent services notified (GC, MH)
  - Users notified if user-facing impact
  - On-call engineer available

- [ ] **Runbook reviewed** by deployment engineer
  - All steps understood
  - Required access verified (kubectl, monitoring)

---

## Deployment Steps

### Story 3 cutover (see each other): the MC view

The order, the skew table and the rollback have **one home**:
[`mh-deployment.md` §Story 3 cutover](mh-deployment.md#story-3-cutover-see-each-other-order-skew-detection-rollback).
What MC operators need from it:

- **Order: config → MH → MC → browser SDK.** MC goes third. Do not roll MC until both MH pods show
  `mh_media_policy_applies_total{outcome="rejected_transport_inconsistent"}` **present** (MH-A1's
  per-stream transport check is live). A story-3 MC in front of a story-2 MH has every
  registration that carries a video egress rejected **whole**, audio included
  ([Mixed-Transport Rollback Hazard](mh-deployment.md#mixed-transport-rollback-hazard)). The
  config step (a) already rolled both MC pods once, because the shared `media-codec-profile`
  changed their pod templates. That roll changes no behaviour on the old image.
- **On Kind it is one converge.** `infra/kind/scripts/deploy.sh` applies MC, MH and the config in
  one `kubectl apply -k` and cannot sequence MH before MC. Run it with no live meetings. The
  browser bundle is a separate step that `deploy.sh` does not perform.
- **What MC counts during the window.** An old SDK joining a story-3 MC is refused at join
  (`supported_header_versions` lacks 3) on
  `mc_session_join_failures_total{error_type="header_version_unsupported"}`. That is the
  stale-bundle tail. Users clear it by reloading.
- **Rollback.** Within the v3 era, `kubectl rollout undo` of mc-0 AND mc-1 holds (one version
  back, R-25) -- except that from 2026-10-25 (ADR-0036 §5 amendment of that date) **MC and client
  deploy and revert together.** **Across the story-3 cutover, never roll MC back alone.** A pre-story MC directs
  `header_version` 2, which a v3 SDK refuses and a v3 MH drops. Roll back everything together:
  on Kind, `deploy.sh` from the last pre-story-3 commit, then the bundle.
- **For a video-only incident, use the [video-off lever](#video-off-lever) BEFORE any rollback.**

### Kind-label `content_kind` (2026-10-26): receiving side first, series identity changes

`docs/devloop-outputs/2026-10-26-mediakind-metric-labels/` adds `content_kind` beside
`media_kind` on every per-kind MC and client series. It is ONE revert unit — GC
(`MEDIA_DATAPOINT_EXTRA` admits the key), the OTel collector (`keep_keys` + the exact
`^(main|screen)$` rule), MC, the browser SDK, and the alert rules + dashboards — and it has
one safe direction:

- **Roll out the receiving side (GC + collector) BEFORE MC and the SDK; revert it AFTER them.**
  A client emitting `content_kind` into a GC that does not admit the key gets the label
  stripped on every honest client and counted on `gc_telemetry_pii_attributes_dropped_total`
  as a datapoint drop, and its `main`/`screen` points collapse into one series. On Kind the
  single `deploy.sh` converge applies all of them together; the bundle follows.
- **Expected at rollout, not an incident.** Every per-kind series is a NEW series (series
  identity, not traffic). `rate()` / `increase()` panels ramp on the new series, and the
  `for:`-gated ratio alerts (`MCMediaMissingKeyMaterial`, the client video alerts) restart
  their timers. Every series reads `content_kind="main"` until screen share ships.
- **Open tabs do not deploy atomically** (see [§0](#0-roll-the-browser-sdk-first-story-2-task-20-and-later)
  and the stale-bundle tail above). FORWARD: a tab still on the previous bundle sends
  `media_kind` without `content_kind`; the collector's all-or-none rule stores it as
  `media_kind="invalid", content_kind="invalid"`, so it is invisible to the
  `content_kind="main"` panels and alerts until it reloads. A burst of `invalid` right after
  rollout is that tail, not a patched client. REVERT: tabs on the new bundle keep sending
  `content_kind` into the reverted GC, so `gc_telemetry_pii_attributes_dropped_total{kind="datapoint"}`
  ticks until they reload — expected, not a leak.

### 0. Roll the browser SDK first (story 2 task 20 and later)

> **Scope (corrected 2026-10-21, story 3 OPS-A, R-25): this step does NOT apply to the story-3
> cutover, and within the v3 era it imposes no order.** Story 3's cutover puts the SDK **last**
> (config → MH → MC → SDK; [§Story 3 cutover](#story-3-cutover-see-each-other-the-mc-view)).
> A reader who trusted "SDK first" for story 3 would ship a v3 SDK in front of a pre-story MC,
> and once C-A1 lands that SDK refuses the old MC's `header_version` 2 directive and sends no
> audio at all. The rule below still describes story 2 task 20's skew between pre-story-3
> builds. Every story-3 build has multi-target send and multi-transport receive, so the skew it
> guards against cannot occur within the v3 era.

The SDK tolerates an old MC (one handler offered, one target), but an old SDK
does NOT tolerate the new MC under partial connectivity (see §Coordination: it
silently sends to only its first target). So:

1. Deploy the web app / SDK bundle carrying multi-target send and multi-transport
   receive.
2. Only then roll MC.
3. Expect a stale-bundle tail regardless: users with a cached old bundle are in
   the old-SDK case until they reload. It is safe for all-connected meetings; for
   a report of "some peers hear me, some don't" during that window, ask the user
   to reload before triaging (`mc-incident-response.md` Scenario 18 arm (c2)).

**The SDK-first rule above does NOT extend to `kek_rotation_debounce_seconds`.**
That field (the rotation debounce W that MC sends with the KEK, from which the
client derives how long it keeps the previous KEK generation) imposes **no**
SDK<->MC ordering constraint. The client substitutes a retention floor for a
zero or absent W, so it tolerates an MC without the field. An old SDK ignores
the field, so an MC that sends it is also safe. Either may roll first. Do not
generalise task 20's rule to this field.

### 1. Pre-Deployment Verification

**Verify current state:**

```bash
# Check current deployment status
kubectl get deployment mc-0 mc-1 -n dark-tower

# Check current pod status
kubectl get pods -n dark-tower -l app=mc-service

# Check current resource utilization
kubectl top pods -n dark-tower -l app=mc-service

# Check active meetings (critical - do not deploy if high)
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl -s http://localhost:8080/metrics | grep mc_meetings_active
kill %1

# Verify readiness of current pods
kubectl get pods -n dark-tower -l app=mc-service -o json | jq '.items[].status.conditions[] | select(.type=="Ready")'
```

**Story 4 MC-B1 (connection_id REQUIRED): read the retiring detector on the OLD MC — the last
moment it exists.** Before rolling an MC image that contains MC-B1, confirm no MH still sends
an empty `connection_id`:

```promql
sum(increase(mc_mh_notifications_without_connection_id_total[24h])) == 0
```

Non-zero means an MH below the story 2 task 20 image is in the fleet; roll that MH forward
first. The new MC refuses its Connecteds outright (§Coordination).

**Expected output:**
- Deployment shows desired replicas (2+)
- All pods in Running state
- All pods Ready=True
- CPU <70%, Memory <70%
- Active meetings at manageable level (ideally <50 per pod)

### 2. Initiate Graceful Drain (Optional, for zero-impact deployments)

**For critical deployments with active meetings:**

```bash
# Mark MC as draining in GC (stops new meeting assignments)
# This is done via GC admin API or database update
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'draining' WHERE id = '<MC_ID>';"

# Wait for active meetings to decrease
watch -n 10 'kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 2>/dev/null & sleep 1; curl -s http://localhost:8080/metrics | grep mc_meetings_active; kill %1 2>/dev/null'

# Proceed when active meetings reach acceptable level (e.g., <10)
```

### 3. Update Container Image

> **Apply the manifests FIRST if the release changes `infra/services/mc-service/**`.**
> MC reads every **Required** row in [§Configuration Reference](#configuration-reference)
> via `ConfigError::MissingEnvVar` with no Rust default (that table is the one place
> the count lives — do not restate it here), and each is injected by a per-key `configMapKeyRef` that lives in the
> **Deployment**, not the ConfigMap. A release that adds a required key and is
> shipped with `kubectl set image` alone puts **both MC pods into
> `CrashLoopBackOff`** — new image, old pod template, no reference to the new key.
> This is not hypothetical: it is what happened on 2026-09-03 when the five
> ADR-0036 `MC_AUDIO_*` / `MC_MAX_RECEIVE_*` keys were introduced. Both pods fail
> identically, so the PDB offers no protection and it is a full signalling-plane
> outage. See §Config-failure triage for the two signals and their opposite first
> steps.
>
> ```bash
> # Does this release touch the manifests? If yes, Option B is the ONLY safe route.
> git diff --name-only <PREVIOUS_TAG>..<NEW_TAG> -- infra/services/mc-service/
> ```

**Option A: Using kubectl (direct deployment) — image-only releases**

Safe **only** when the diff above is empty. `set image` changes the image and
nothing else; it cannot add a `configMapKeyRef`.

```bash
# Set new image version
export NEW_VERSION="v1.2.3"  # Replace with actual version tag

# Update BOTH instances -- mc-0 alone is half of MC
kubectl set image deployment/mc-0 \
  mc-service=mc-service:${NEW_VERSION} \
  -n dark-tower
kubectl set image deployment/mc-1 \
  mc-service=mc-service:${NEW_VERSION} \
  -n dark-tower

# Verify image updated on both
kubectl describe deployment mc-0 -n dark-tower | grep Image:
kubectl describe deployment mc-1 -n dark-tower | grep Image:
```

**Option B: Using kubectl apply -k (declarative, Kustomize) — always safe**

Applies the ConfigMaps, both Deployments and the image together, which is the
coupled set §Config-failure triage requires. Use this whenever the release
touches `infra/services/mc-service/**`, and prefer it otherwise.

```bash
# Update the image tag in BOTH per-instance Deployments:
#   infra/services/mc-service/mc-0-deployment.yaml
#   infra/services/mc-service/mc-1-deployment.yaml
# Change image tag: mc-service:latest → mc-service:v1.2.3
# (There is no `deployment.yaml` -- MC's workloads are per-instance files.)

# Apply the overlay, NOT the base. The Kind overlay strategic-merge-patches
# OTEL_ENABLED="true" into mc-service-config; applying the base directly would
# silently revert it (see §Config-failure triage, Recovery).
kubectl apply -k infra/kubernetes/overlays/kind/services/mc-service/

# Verify change on both instances
kubectl describe deployment mc-0 -n dark-tower | grep Image:
kubectl describe deployment mc-1 -n dark-tower | grep Image:
```

### 4. Rolling Update Monitoring

Deployments update pods via rolling strategy (maxSurge=1, maxUnavailable=0 for zero-downtime).

**Monitor rollout:**

```bash
# Watch pod status (Ctrl+C to exit)
kubectl get pods -n dark-tower -l app=mc-service -w

# Check rollout status
kubectl rollout status deployment/mc-0 -n dark-tower

# Monitor logs from new pod
kubectl logs -f deployment/mc-0 -n dark-tower
```

**Expected sequence:**
1. New pod created (beyond current replica count)
2. New pod starts and becomes Ready (health check + readiness check pass)
3. Old pod marked for termination
4. Old pod drains connections gracefully (SIGTERM, 60s drain period)
5. Old pod terminates
6. Repeat for all pods

**Typical timeline:**
- Pod startup: 10-20 seconds (GC registration + actor system init)
- Pod termination: 60-90 seconds (graceful connection drain)
- Total per pod: ~90 seconds
- **Total rollout: ~5-6 minutes for 3 replicas**

### MC rollout and MH meetings (ADR-0039 §6)

**Incidental, not an MC-failover mechanism.** MC failure is out of scope (ADR-0039): the meetings an
MC was serving are **already lost when that MC restarts**. What follows is how the MHs give the
capacity back, so you can read the signals after every MC rollout without opening an incident.

- **A graceful rollout (SIGTERM) releases its meetings itself.** On SIGTERM MC tears down every
  meeting and sends `EndMeeting` to each MH within its shutdown release budget
  (`SHUTDOWN_RELEASE_BUDGET`, 25 s; `crates/mc-service/src/media_routing/teardown.rs`); MH ends them
  at once (`mc_media_end_meeting_total{outcome="released"}`, or `superseded_by_successor` when a
  successor MC already re-registered the meeting). **Expected after a normal rolling update:** a
  brief MH heartbeat transport-error blip (MC's gRPC server stops ~2 s before teardown), MH meeting
  counts drop within the shutdown window, and **owner-silence releases ≈ 0**.
- **Only meetings whose release did not complete wait for the owner-silence release**: a crash,
  OOM or SIGKILL exit (including an in-place crash restart), teardowns cut off at the shutdown
  deadline (the MC pod's last ERROR "Meeting teardowns still running at the shutdown deadline were
  cut off", `teardowns_cut_off=<n>`), and `mc_media_end_meeting_total{outcome="unavailable_exhausted"}`.
  For those:
  - **MC ids are per process.** `MC_ID` is unset in our deploy, so every MC process generates a new
    id (`mc-<hostname>-<suffix>`). The MHs keep heartbeating those meetings to the registered MC
    endpoint (`MC_GRPC_ADVERTISE_ADDRESS`, the pod IP). An MC that restarted **in place** (same pod
    IP), or another MC pod that reused that IP, answers `FAILED_PRECONDITION` (owner mismatch); MH
    counts it (`mh_mc_heartbeat_verdicts_total{outcome="owner_mismatch"}`) and takes no action, and
    the answering MC counts it too (`mc_media_handler_heartbeats_total{outcome="owner_mismatch"}`).
    A replacement pod with a **new** IP makes the heartbeats fail as transport errors instead.
    `mh_mc_heartbeats_total{outcome="error"}` rises in **both** cases (`FAILED_PRECONDITION` is an
    `error` too); **the discriminator is `mh_mc_heartbeat_verdicts_total{outcome="owner_mismatch"}`**:
    non-zero for an in-place restart or IP reuse, flat for a new pod IP.
  - **MHs keep those meetings until the owner-silence release**: after
    `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (default 900 s) with no communication from the owning
    MC (errors and owner-mismatch answers do not count), MH ends each meeting, closing its sessions
    `HANDLER_RELEASED`. **About 15 minutes later**
    `mh_media_handler_releases_total{trigger="owner_silence", clients="none"}` rises by roughly the
    unreleased count, and the MH meeting counts and `mh_media_meetings_without_clients` drop.

```promql
sum by (outcome) (increase(mc_media_end_meeting_total[15m]))                     # graceful release
sum by (outcome) (increase(mh_mc_heartbeat_verdicts_total{outcome="owner_mismatch"}[15m]))
sum by (outcome) (increase(mh_mc_heartbeats_total{outcome=~"error|timeout"}[15m]))
sum by (clients) (increase(mh_media_handler_releases_total{trigger="owner_silence"}[30m]))
max(mh_media_meetings_without_clients)
```

Not expected: `clients="present"` releases ([`mh-incident-response.md` Scenario 26](mh-incident-response.md#scenario-26-owner-silence-release-with-clients-present)),
or meetings without clients persisting past the owner-silence timeout
([`mh-incident-response.md` Scenario 25](mh-incident-response.md#scenario-25-meetings-held-without-clients)).
The MC-side owner-mismatch reading is in
[`mc-incident-response.md` Scenario 29](mc-incident-response.md#scenario-29-heartbeat-misroute-or-owner-mismatch).
A known adjacent hazard: mc-0/mc-1's RollingUpdate surge can briefly put two live MCs behind one
NodePort during the rollout (`docs/TODO.md`, "Two live MCs can sit behind one client-facing NodePort during an mc-0/mc-1 rollout"; out of scope here).

### 5. Verify Deployment Success

**Pod health:**

```bash
# All pods Running and Ready
kubectl get pods -n dark-tower -l app=mc-service

# Check pod events for errors
kubectl get events -n dark-tower --field-selector involvedObject.kind=Pod,involvedObject.name=mc-service-<pod-suffix>
```

**Logs review:**

```bash
# Check for startup errors
kubectl logs deployment/mc-0 -n dark-tower --tail=50

# Look for error patterns
kubectl logs -n dark-tower -l app=mc-service --tail=100 | grep -i "error\|panic\|fatal"
```

**Expected log messages:**
```
Starting Meeting Controller
Configuration loaded successfully
Registering with Global Controller...
GC registration successful
Actor system initialized
WebTransport listener started on 0.0.0.0:4433
Prometheus metrics recorder initialized
Meeting Controller ready
```

**Verify the EFFECTIVE config, not the ConfigMap.** A *missing* required key is
loud (CrashLoop, `logs --previous` names it — §Config-failure triage). A
**wrong-but-valid** one is silent: nothing fails, no counter moves, and the two
MC pods can quietly disagree. The `Configuration loaded successfully` line is
the only record of what the process actually received, and the ConfigMap cannot
be its own evidence — the ConfigMap being out of step with what the Deployment
injects *is* the failure mode these keys create.

```bash
# What each pod actually loaded (structured fields on the startup line):
kubectl logs deployment/mc-0 -n dark-tower | grep "Configuration loaded successfully"
kubectl logs deployment/mc-1 -n dark-tower | grep "Configuration loaded successfully"
```

- [ ] `max_audio_receive_slots`, `max_video_receive_slots` and
      `max_receive_capability_declarations` are present on the line and match
      `MC_MAX_AUDIO_RECEIVE_SLOTS` / `MC_MAX_VIDEO_RECEIVE_SLOTS` /
      `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` in `infra/services/mc-service/config.env`
      (**not** the retired `MC_MAX_RECEIVE_SLOTS`, which MC no longer reads; the story-2
      field `max_receive_slots` is gone from the line). A video cap of 0 here means the
      [video-off lever](#video-off-lever) is pulled.
- [ ] `audio_codec`, `audio_max_bitrate_bps`, `audio_frame_rate_hz`, `video_codec`,
      `video_width`, `video_height`, `video_frame_rate_hz` and `video_max_bitrate_bps`
      match `MEDIA_AUDIO_*` / `MEDIA_VIDEO_*` in
      `infra/services/media-codec-profile/profile.env` (**not** the retired `MC_AUDIO_*`).
- [ ] `video_min_keyframe_interval_ms` matches `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` in
      `infra/services/mc-service/config.env` (MC's own ConfigMap since amendment 4 — not
      `profile.env`).
      The same values are readable at runtime on `mc_media_receive_slot_cap{media_kind, content_kind}`
      and `mc_media_video_keyframe_min_interval_seconds`.
- [ ] **mc-0 and mc-1 report identical values.** They read one shared ConfigMap,
      so a difference means one Deployment's `configMapKeyRef` block is stale —
      the guard-green split-brain the ConfigMap banner warns about.
- [ ] **`mc_id` is the ONE field that must DIFFER between the two pods — and must
      differ from what the same pod reported before this roll.** Every other
      field on this line is shared by design; `mc_id` is the opposite, and it is
      the field MH's ownership check compares when a departing MC releases its
      meetings on graceful shutdown (story 2 task 12). Two pods reporting the
      same `mc_id`, or a pod reporting the same value across a restart, means
      `MC_ID` has been pinned — see the DO-NOT-PIN warning in §Configuration
      Reference for why that turns a rolling deploy into a meeting outage. This
      check is the only thing that detects a pinned `MC_ID` before it costs a
      meeting: MC cannot detect it itself, because no pod can see another pod's
      id.

      ```bash
      # Must print two DIFFERENT ids. One id printed twice is the footgun.
      for p in mc-0 mc-1; do
        kubectl logs -n dark-tower "deployment/$p" \
          | grep -o 'mc_id="[^"]*"' | tail -1
      done
      ```
- [ ] No `audio frame rate other than 20 ms frames` WARN, unless you set one
      deliberately
- [ ] `push_quiesce_bound_seconds`, `teardown_fence_hold_max_seconds` and
      `shutdown_release_budget_seconds` are present (story 2 task 12). They are
      derived from compiled constants rather than from config, so they are identical
      on every pod of one image — with one exception worth knowing: the inputs to
      `shutdown_release_budget_seconds` include this pod's
      `terminationGracePeriodSeconds`, restated in Rust as
      `SHUTDOWN_TERMINATION_GRACE_SECONDS`. A unit test fails the build on drift
      between the two, so a mismatch cannot ship; the field is here because the
      startup line is still the only per-pod record of the budget the process
      actually computed. `teardown_fence_hold_max_seconds` is the worst case a
      rejoin of a just-ended meeting can be held; `shutdown_release_budget_seconds`
      decides whether a shutdown release happens at all.

> **That WARN will not appear in the error grep above.** §Logs review greps
> `error|panic|fatal`; this one is a `WARN`. *(Since MC-A the value is the profile's
> `MEDIA_AUDIO_FRAME_RATE_HZ`, and since MH-A1 a value other than MH's single supported rate makes
> MH refuse to boot rather than understate. See the header of
> `infra/services/media-codec-profile/profile.env`.)* `mc-service` emits it whenever
> `MEDIA_AUDIO_FRAME_RATE_HZ` is not 50 Hz, because `mh-service` sizes its datagram
> send buffer in bytes against 20 ms frames and will then **understate** its
> reported held latency. A non-50 value is legal (ADR-0036 §3's 40 ms
> signature-overhead mitigation) but is a coordinated change with media-handler,
> not a unilateral one. Grep for it explicitly:
>
> ```bash
> kubectl logs deployment/mc-0 -n dark-tower | grep -i "frame rate"
> kubectl logs deployment/mc-1 -n dark-tower | grep -i "frame rate"
> ```

### 6. Run Smoke Tests

**See [Smoke Tests](#smoke-tests) section below for detailed test procedures.**

Minimum required smoke tests:
- [ ] Health check returns 200 OK
- [ ] Readiness check returns 200 OK
- [ ] Metrics endpoint returns Prometheus format
- [ ] GC heartbeat succeeding
- [ ] Join flow works (WebTransport connect, JoinRequest/Response)

### 7. Verify GC Registration

**Confirm MC is registered and receiving assignments:**

```bash
# Check MC registration in GC database
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers WHERE last_heartbeat > NOW() - INTERVAL '30 seconds' ORDER BY last_heartbeat DESC;"

# Verify MC is marked as healthy
# status should be 'active', last_heartbeat should be recent
```

### 8. Monitor Metrics

**Verify metrics collection:**

```bash
# Port-forward to access metrics endpoint
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

# Fetch metrics
curl http://localhost:8080/metrics

# Kill port-forward
kill %1
```

**Check key metrics:**
- `mc_meetings_active` - Active meeting count
- `mc_connections_active` - Active WebTransport connections
- `mc_actor_mailbox_depth` - Actor mailbox depth (should be low)
- `mc_session_join_duration_seconds` - Session join latency (p95 SLO)
- `mc_redis_latency_seconds` - Redis op latency (p99 <10ms SLO)

### 9. Post-Deployment Checklist

- [ ] All pods Running and Ready
- [ ] Smoke tests pass (health, ready, metrics)
- [ ] No errors in logs (last 5 minutes)
- [ ] Metrics available in Prometheus
- [ ] GC heartbeat succeeding
- [ ] GC has MC marked as 'active'
- [ ] Actor mailbox depths low (<50)
- [ ] Session join duration within SLO (p95 < 2s)
- [ ] No actor panics in metrics

---

## Rollback Procedure

### When to Rollback

**Immediate rollback criteria** (do not wait):

1. **Pod startup failures**
   - Pods stuck in CrashLoopBackOff >2 minutes
   - Pods failing readiness checks consistently
   - GC registration failures
   - Actor system initialization failures

2. **Critical functionality broken**
   - Actor panics occurring
   - High message drop rate (>1%)
   - WebTransport connections failing to establish
   - GC heartbeat failures

3. **Severe performance degradation**
   - p95 session join duration >4s (2x SLO)
   - Redis p99 latency >50ms (5x SLO)
   - Mailbox depth critical (>500)
   - Memory usage >90%

4. **Security issues discovered**
   - Vulnerability in new code
   - Authorization failures
   - Token validation bypass

**Monitoring period before declaring success:**
- Minimum: 15 minutes post-deployment
- Recommended: 1 hour for major changes
- Critical changes: 24 hours with on-call monitoring

### How to Rollback

> **Story 3 cutover first (R-25).** If the build you are leaving contains story 3 and the build you
> are returning to does not, **this procedure's MC-only steps do not apply.** MH, MC, the SDK
> bundle and the config roll back together:
> [`mh-deployment.md` §Story 3 cutover](mh-deployment.md#story-3-cutover-see-each-other-order-skew-detection-rollback),
> layer 3. An MC-only undo across it directs `header_version` 2 into a v3 fleet, which silences
> audio. Within the v3 era, the steps below hold, one version back. For a video-only problem, use
> the [video-off lever](#video-off-lever) before any rollback.
>
> **From 2026-10-25 (ADR-0036 §5 amendment of that date): MC and client deploy and revert together.**

> **Cross-service order for the story-2 task-12 release: roll back MH → MC →
> GC** (MC before GC, or ended meeting ids become unjoinable). The full
> reasoning, and why the MC/MH sender_id rule in `mh-deployment.md` wins where
> the two appear to conflict, is in §Coordination above.

> **What is in the coupled set, and what may NOT be reverted alone.**
> Since the ADR-0036 keys landed, MC's config is a hard startup dependency:
> **the ConfigMap, BOTH Deployments and the image roll together, and none of them
> may be reverted independently.** Concretely, for a rollback:
>
> - **`kubectl rollout undo` is safe** and is the recommended route. It reverts
>   the *whole pod template*, not just the image, so the old image lands with the
>   old `env` block — self-consistent by construction. Reverting only the image
>   (`kubectl set image` to an older tag) is also safe: surplus env is ignored.
> - **Do NOT revert the ConfigMap on its own.** A `git revert` of
>   `infra/services/mc-service/config.env` followed by an apply leaves both
>   Deployments holding a `configMapKeyRef` for a key that no longer exists. The
>   ConfigMap is a `configMapGenerator` with a content hash in its name
>   (ADR-0038 §2), so the apply changes the pod template and **rolls the pods
>   immediately**. The new pods come up `CreateContainerConfigError` with
>   **empty logs** and the rollout stalls. The failure is at least immediate
>   rather than latent, but it is still a failed deploy.
> - **Do not revert the manifests without the image, or apply the image against
>   stale manifests.** Both MC pods fail identically at startup, so the PDB offers
>   no protection: this is a full signalling-plane outage, not a partial one.
>
> Full triage for both failure signals — which have **opposite first steps** —
> and the verified recovery sequence are in §Config-failure triage below.
>
> **Rolling the MC image back across story 2's generation-floor adoption
> leaves every meeting alive across the rollback media-dark** (MH keeps the
> newer installed generation; the old build pushes 1 and never climbs). Bounded
> to those meetings; new meetings are unaffected. The remedy is the one
> exception to "do not restart MH" — see **Carve-out #3** under §Post-Deploy
> Monitoring Checklist: MC↔MH Coordination.
>
> **Rolling MC back below `kek_rotation_debounce_seconds` flips every live
> client to the KEK retention floor.** The old build sends no W, so each client
> falls back to its floor, and
> `dt_client_media_kek_retention_anomalies_total{outcome="floor_substituted"}`
> climbs **fleet-wide**, once per KEK message. This is the **SUPPORTED** path and
> not an incident. Do not chase it, and do not roll further back to "fix" it. The
> counter reads zero outside such a rollback. It is the only cluster-visible
> evidence, because the client's accompanying warning goes to a browser console
> that no operator can see.
>
> **Rolling MC back below story 4 MC-B1 (heartbeat intake).** Safe while MH is
> below MH-2. Once MH-2 is live it is UNSAFE: roll MH back first (MH-2 refuses
> every `RegisterMeeting` carrying the old MC's zero heartbeat interval and every
> cause-less `EndMeeting`, and gets UNIMPLEMENTED heartbeats, counted with no
> action; the owner-silence release that would also end those meetings is
> MH-3's). Two companions: (1) **the alert rule** —
> `MCMediaGenerationDivergence` now reads `outcome!="match"`, and a pre-MC-B1
> image emits `handler_id_mismatch` on every MH restart, which PAGES under it:
> roll `mc-alerts.yaml` back with the image, or expect and silence that page;
> (2) the old image accepts an empty `connection_id` again, so the
> connection_id hard ordering relaxes. The `MC_MH_HEARTBEAT_TIMEOUT_MS`
> ConfigMap key is harmlessly unused by the old image.
>
> **Rolling MC back from story 4 MC-B2 (detection and removal) to MC-B1.**
> Safe: MC-B1 already sets `EndMeetingRequest.cause = MEETING_ENDED` at
> teardown (P1), so MH-2's refusal of a cause-less `EndMeeting` is not
> triggered, and detection state (health, the removed record) is in memory
> only, so nothing persists across the rollback. What to expect: a handler
> MC-B2 already removed was sent `EndMeeting(HEARTBEAT_TIMEOUT|APPLY_LAG)` and
> its clients were told to drop it; MC-B1 never re-adds a handler, so a meeting
> MC-B2 left with no handler stays without media until it ends (the rollback
> restarts MC, which loses those meetings anyway). The new series
> (`mc_media_handler_declarations_total`, `_suspicions_total`,
> `_detection_seconds`, `_removal_releases_total`,
> `mc_media_meetings_without_handlers`) disappear with the image.
>
> **Rolling MC back from story 4 MC-C (crash replacement via GC) to MC-B2.**
> Safe while GC is ≥ GC-3 (and also below it: the older MC ignores
> `RegisterMCResponse.max_exclude_handler_ids`), and the older image ignores
> the two ConfigMap keys `MC_MH_REPLACEMENT_MIN_INTERVAL_MS` /
> `MC_MH_REPLACEMENT_BACKOFF_MAX_MS` (remove them after the image, if at
> all). Replacement state is in memory only and the rollback restarts MC, so
> in-flight `AcquireMediaHandler` calls and Registering grants are lost with
> the meetings. What to expect after the rollback: a meeting that loses a
> handler runs on its survivors, or with no media if none survives, until it
> ends (MC-B2 behaviour — nothing is acquired). The series that disappear
> with the image: `mc_media_handler_migrations_total`,
> `mc_media_handler_migration_duration_seconds`,
> `mc_media_handler_replacement_retry_budget_seconds`,
> `_min_interval_seconds` and `_backoff_max_seconds`. GC side: replacement
> requests stop, so `gc_mh_replacement_requests_total` goes flat.
>
> **Rolling MC back from story 4 MC-F (planned transfer) to MC-C.** Safe: the
> older image ignores the ConfigMap key `MC_PLANNED_TRANSFER_MARGIN_MS`
> (remove it after the image, if at all), and planned-transfer state is in
> memory only. What operators see after the rollback: a draining MH
> (`mh.sh drain`, upgrade, scale-down) is no longer moved off
> make-before-break. It keeps its meetings until its own drain deadline
> (grace − 5 s), then sheds them with `HANDLER_RELEASED`; MC handles each on
> the CRASH path (break-before-make blip, then a replacement via GC). So
> rollouts show `mc_media_handler_declarations_total` with
> `trigger="heartbeat_timeout"` (never `planned_transfer`), crash-trigger
> migrations, and R-33 loss-metric observations, and MH's
> `mh_drain_meetings_shed_total` / "Drain finished" `meetings_shed` > 0. The
> series that stop moving: `trigger="planned_transfer"` on
> `mc_media_handler_migrations_total`, `_declarations_total` and
> `_removal_releases_total`; `mc_media_handler_planned_transfer_margin_seconds`
> disappears with the image.
>
> **Rolling MC back from story 4 MC-E (client-loss evidence) to MC-F.** Safe,
> image-only: the older image ignores the ConfigMap keys
> `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` and
> `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` (remove them after the image, if
> at all); the evidence state is in memory only and never fed routing. The
> series that disappear with the image: `mc_media_handler_loss_clients_total`,
> `_loss_prior_report_ratio`, `_loss_client_lead_seconds`,
> `_client_loss_disagreements_total`, `_first_declarations_total`,
> `mc_media_handlers_in_use` and the two disagreement config gauges (the
> Client-Loss Evidence row on MC Media Handlers goes empty).

**Step 1: Identify previous version**

```bash
# Find previous image version -- read BOTH histories. Revision numbers are
# per-Deployment; mc-0 revision 7 and mc-1 revision 7 are not the same release.
kubectl rollout history deployment/mc-0 -n dark-tower
kubectl rollout history deployment/mc-1 -n dark-tower

# Get image from previous revision
kubectl rollout history deployment/mc-0 -n dark-tower --revision=<MC0_REVISION>
kubectl rollout history deployment/mc-1 -n dark-tower --revision=<MC1_REVISION>
```

**Step 2: Rollback Deployment**

> **Roll back BOTH instances, and treat that as one step.** §Update Container
> Image says the same thing for the forward direction ("mc-0 alone is half of
> MC"); it matters more here, because the state you land in when you stop after
> `mc-0` is a **mixed-version signalling plane** — mc-0 on the old image, mc-1 on
> the new — and it is not self-announcing. Each instance holds its own sessions
> and advertises its own client-facing URL, so GC keeps assigning meetings to
> both: roughly half of new meetings get the behaviour you just rolled back
> **away from**, and the symptom is intermittent and per-meeting rather than a
> clean regression. Do not stop between the two commands to "see if that fixed
> it" — a half-rolled pair cannot answer that question.

```bash
# Rollback to previous revision -- BOTH instances.
kubectl rollout undo deployment/mc-0 -n dark-tower
kubectl rollout undo deployment/mc-1 -n dark-tower

# Or rollback to specific revision. Revision numbers are PER-DEPLOYMENT and are
# not guaranteed to line up between mc-0 and mc-1 -- read each one's own history
# (Step 1, repeated for mc-1) rather than reusing a number across both.
kubectl rollout undo deployment/mc-0 -n dark-tower --to-revision=<MC0_REVISION>
kubectl rollout undo deployment/mc-1 -n dark-tower --to-revision=<MC1_REVISION>

# Monitor rollback -- both, and do not declare success on one.
kubectl rollout status deployment/mc-0 -n dark-tower
kubectl rollout status deployment/mc-1 -n dark-tower

# Confirm both landed on the SAME image before calling it done.
kubectl get pods -n dark-tower -l app=mc-service \
  -o custom-columns=POD:.metadata.name,INSTANCE:.metadata.labels.instance,IMAGE:.spec.containers[0].image
```

**Step 3: Verify rollback success**

```bash
# Check pods running previous version
kubectl get pods -n dark-tower -l app=mc-service -o jsonpath='{.items[*].spec.containers[0].image}'

# Run smoke tests (see Smoke Tests section)
# Verify health, ready, metrics endpoints
```

**Step 4: Re-enable in GC (if drained)**

```bash
# If MC was in draining status, re-enable
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "UPDATE meeting_controllers SET status = 'active' WHERE id = '<MC_ID>';"
```

**Step 5: Post-rollback verification**

- [ ] All pods running previous image version
- [ ] Smoke tests pass
- [ ] Actor mailbox depths normal
- [ ] Message latency within SLO
- [ ] No errors in logs
- [ ] GC heartbeat succeeding

**Step 6: Incident retrospective**

- Document rollback reason
- Create incident report
- Identify root cause
- Update pre-deployment checklist if needed

---

## Configuration Reference

### Environment Variables

| Variable | Required | Description | Default | Example |
|----------|----------|-------------|---------|---------|
| `REDIS_URL` | **Yes** | Redis connection URL for session state. Supplied from the `mc-service-secrets` Secret. | None | `redis://:pw@redis.dark-tower:6379` |
| `MC_BINDING_TOKEN_SECRET` | **Yes** | Base64 master secret for binding-token HMAC (ADR-0023). From `mc-service-secrets`. | None | `openssl rand -base64 32` |
| `AC_ENDPOINT` | **Yes** | Auth Controller endpoint for OAuth token acquisition (ADR-0010). | None | `http://ac-service.dark-tower:8082` |
| `MC_CLIENT_ID` | **Yes** | OAuth client ID for MC's client-credentials flow to AC. | None | `meeting-controller` |
| `MC_CLIENT_SECRET` | **Yes** | OAuth client secret. From `mc-service-secrets`. | None | `<secret>` |
| `AC_JWKS_URL` | **Yes** | AC JWKS endpoint for meeting-token validation. Validated at load: must start `http://` or `https://`. | None | `http://ac-service.dark-tower:8082/.well-known/jwks.json` |
| `MC_TLS_CERT_PATH` | **Yes** | PEM cert for the WebTransport server. File existence is checked at startup, so a *malformed or empty* Secret is a load failure rather than a runtime error. A **missing** `mc-service-tls` never reaches this check — the mount blocks first (`FailedMount`, no logs). | None | `/etc/mc-tls/tls.crt` |
| `MC_TLS_KEY_PATH` | **Yes** | PEM private key for the WebTransport server. Existence checked at startup, with the same caveat as `MC_TLS_CERT_PATH` above. | None | `/etc/mc-tls/tls.key` |
| `MC_GRPC_ADVERTISE_ADDRESS` | **Yes** | Address GC uses to reach this pod's gRPC. Built from the downward-API `POD_IP`. | None | `http://$(POD_IP):50052` |
| `MC_WEBTRANSPORT_ADVERTISE_ADDRESS` | **Yes** | Address GC hands clients for this pod. **Per-instance** — from `mc-0-config` / `mc-1-config`, not the shared ConfigMap. | None | `https://127.0.0.1:4433` |
| `MC_MAX_RECEIVE_SLOTS` *(RETIRED by story 3)* | No — **no longer read by MC**; ref `optional: true` | Superseded by the per-kind caps below (in story 2 it was a **TOTAL** across kinds, rejected an over-cap declaration whole as `slot_count_over_cap`, and was published on the audio slot-cap gauge; none of that happens now). **The key MUST stay in the ConfigMap until the rollback window closes**: a one-version MC rollback to the pre-MC-A image still hard-reads it. Deletion is scheduled in `docs/TODO.md` §Media Path Obligations, "Delete the story-3 retired config keys". Do not tune it. | `8` (in ConfigMap) | `8` |
| `MC_MAX_AUDIO_RECEIVE_SLOTS` *(story 3)* | **Yes** (hard ref) | Max AUDIO receive slots per client (R-1). Each kind is **truncated** to its own cap: excess slots report `SLOT_STATE_UNAVAILABLE`, which is visible, **never a join failure** (this supersedes story 2's whole-declaration reject). Bounds `MIN_AUDIO_RECEIVE_SLOTS..=MAX_AUDIO_RECEIVE_SLOTS` (1..=64) in `crates/mc-service/src/config.rs`; out of range refuses boot. The floor is not 0, because an audio cap of 0 would be a silent total outage that reads as valid config. Read ONCE at startup; live slot vectors are never re-truncated. Carried to clients as the AUDIO entry of `JoinResponse.receive_slot_caps` (tag 15, one entry per kind) and published as `mc_media_receive_slot_cap{media_kind="audio", content_kind="main"}`. | `8` (in ConfigMap) | `8` |
| `MC_MAX_VIDEO_RECEIVE_SLOTS` *(story 3)* | **Yes** (hard ref) | Max VIDEO receive slots per client (R-1), truncated like audio. Bounds `MIN_VIDEO_RECEIVE_SLOTS..=MAX_VIDEO_RECEIVE_SLOTS` (0..=64) in `crates/mc-service/src/config.rs`. **0 is legal: it is the [video-off lever](#video-off-lever)** (R-26). It lives in this ConfigMap only, never in `media-codec-profile`, so changing it rolls MC and never MH. Read once at startup, like audio. Carried as the VIDEO_CAMERA entry of `JoinResponse.receive_slot_caps` (tag 15, one entry per kind) and published as `mc_media_receive_slot_cap{media_kind="video", content_kind="main"}`; 0 confirms the lever. | `4` (in ConfigMap) | `0` = video off |
| `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS` | **Yes** | Per-connection budget on *accepted* capability declarations. Bounds client-driven O(N) meeting-actor work; over-budget declarations are rejected and counted. | `64` (in ConfigMap) | `64` |
| `MC_AUDIO_CODEC`, `MC_AUDIO_MAX_BITRATE_BPS`, `MC_AUDIO_FRAME_RATE_HZ` *(RETIRED by story 3)* | No — **no longer read by MC**; refs `optional: true` | Superseded by `MEDIA_AUDIO_CODEC`, `MEDIA_AUDIO_MAX_BITRATE_BPS` and `MEDIA_AUDIO_FRAME_RATE_HZ` in the shared profile (rows below); the directive clients receive carries the profile's values. **The keys MUST stay in the ConfigMap until the rollback window closes**: a one-version MC rollback to the pre-MC-A image still hard-reads, validates and directs them, so keep them equal to the profile's audio keys until deletion. Deletion is scheduled with `MC_MAX_RECEIVE_SLOTS`. | `opus` / `48000` / `50` (in ConfigMap) | — |
| `MEDIA_VIDEO_CODEC`, `MEDIA_VIDEO_WIDTH`, `MEDIA_VIDEO_HEIGHT`, `MEDIA_VIDEO_FRAME_RATE_HZ`, `MEDIA_VIDEO_MAX_BITRATE_BPS` *(story 3)* | **Yes** (hard ref to ConfigMap **`media-codec-profile`**) | The VP8 send directive MC gives video publishers (R-8). MC reads all eight profile keys. Bounds (`crates/mc-service/src/media_signaling/directive.rs`): VP8 only; width and height `VIDEO_DIMENSION_MIN_PX..=VIDEO_DIMENSION_MAX_PX` (16..=1920); frame rate `VIDEO_FRAME_RATE_MIN_HZ..=VIDEO_FRAME_RATE_MAX_HZ` (1..=60); bitrate `VIDEO_BITRATE_MIN_BPS..=VIDEO_BITRATE_MAX_BPS` (100000..=4000000). An unknown codec or an out-of-bounds value refuses boot and never falls back. MH also derives its video cost from the frame rate and bitrate, so **editing any profile key rolls mc-0 and mc-1 and replaces every MH** (`mh.sh upgrade`) together, and each MH replacement sheds that MH's sessions. | `profile.env` | — |
| `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS` *(story 3, amendment 4)* | **Yes** (hard ref, `mc-service-config`) | The minimum interval between keyframes MC directs (request-driven, and a publisher keying a target with no group in progress); there is no keyframe interval (R-9). Home: MC's own ConfigMap (`infra/services/mc-service/config.env`), so **changing it rolls mc-0/mc-1 only**. MC carries it on the video directive as `min_keyframe_interval_ms` and publishes it as `mc_media_video_keyframe_min_interval_seconds`. It is server-configured and never client-reported; the client honours max(directed, its compiled floor). Bounds `MIN_KEYFRAME_INTERVAL_MS_FLOOR..=MIN_KEYFRAME_INTERVAL_MS_CEILING` (100..=10000) in `directive.rs`; out of range refuses boot. MH does not read it. (It was the profile key `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` until MC-A; that one-time move out of `media-codec-profile` changed the profile hash, so the MC-A deploy also rolled every MH once.) **Rollback:** roll MC back with `kubectl rollout undo` (or `deploy.sh` from the earlier commit), which returns to the retained previous profile generation that still holds the old key. **Never apply a pre-MC-A `mc-N-deployment.yaml` against the current profile**: its hard ref to `MEDIA_VIDEO_MIN_KEYFRAME_INTERVAL_MS` gives `CreateContainerConfigError` on both MC pods. | `500` (in ConfigMap) | `500` |
| `MEDIA_AUDIO_CODEC`, `MEDIA_AUDIO_MAX_BITRATE_BPS`, `MEDIA_AUDIO_FRAME_RATE_HZ` *(story 3)* | **Yes** (hard ref to `media-codec-profile`) | The audio send directive. MC keeps its existing `AUDIO_*` bounds (`directive.rs`). MH-A1 also reads the bitrate and frame rate and **refuses to boot** on a bitrate outside its supported band or a frame rate other than the one its frame duration implies, so the deployable value is the intersection (header of `profile.env`). | `profile.env` | — |
| `MC_MEDIA_CONNECT_SETTLE_MS` | **Yes** | Connect settle window (ADR-0036 §9, story 2 task 20): how long after a participant's FIRST MH-reported connection MC waits before routing on whatever it observed. Ends early once the participant reaches every handler of its meeting. **Not a debounce** — without it an all-connected meeting loses co-location permanently. Validated `100..=10000` at load. Published as `mc_media_connect_settle_window_seconds`. | `1500` (in ConfigMap) | `1500` |
| `MC_MH_HEARTBEAT_TIMEOUT_MS` *(story 4 MC-B1)* | **Yes** (hard ref, `mc-service-config`) | N, MC's ONE handler-liveness value (ADR-0039 §1), in milliseconds. MC sends each MH the heartbeat interval `max(1, floor(N/3))` ms per meeting in `RegisterMeetingRequest.heartbeat_interval_ms` (MH has no cadence config, so the ends cannot drift) and judges liveness against N: a handler with no accepted heartbeat for longer than N (from its first confirmed `RegisterMeeting`) is declared **Dead** and removed. **L = `APPLY_LAG_FACTOR` x N** (`crates/mc-service/src/media_routing/liveness.rs`, factor 2; **12 s at the default**) is derived from it, never configured: a handler whose applied generation stays behind what it owes for longer than L is declared **Impaired** and removed (`apply_lag`). Range `(0, 30000]` (`MIN_MH_HEARTBEAT_TIMEOUT_MS..=MAX_MH_HEARTBEAT_TIMEOUT_MS` in `crates/mc-service/src/config.rs`); out of range or missing refuses boot. **Relationship to MH:** always far below MH's owner-silence release `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (`[300, 3600]` s, default 900): the 300 s floor is ≥ 10× the largest N (30 s), so a live MC always declares a silent MH dead and moves off long before MH releases on owner silence, and MH ends a meeting only on prolonged MC silence, never on a heartbeat hiccup — never shorten owner silence toward N. **MH's QUIC idle timeout (`MH_MAX_IDLE_TIMEOUT_MS`, `mh-deployment.md`) is deployed ≈ N** and published beside it (`mh_media_transport_idle_timeout_seconds` next to `mc_media_handler_heartbeat_deadline_seconds`): the R-33 interpretability condition for the client-loss metric (`mc-incident-response.md` Scenario 24), so retune the two together. **Trade:** lower N detects a dead handler faster but raises false declarations (a declaration is a removal) and shortens the Impaired window L with it, and raises load — every (meeting, handler) pair sends one heartbeat per interval into its meeting actor, i.e. meetings x handlers / (N/3) messages per second; legal but expensive below ~1500 ms, so do not lower it without load testing. Published as `mc_media_handler_heartbeat_deadline_seconds`; a value edit rolls mc-0 and mc-1. | `6000` (in ConfigMap) | `6000` |
| `MC_MEDIA_DIAL_TOKEN_TTL_SECONDS` *(story 4 MC-D)* | **Yes** (hard ref, `mc-service-config`; no Rust default) | Lifetime, in seconds, of the `media_dial` credential MC mints itself (per-meeting Ed25519 key, no AC call; ADR-0039 §5) for every (participant, handler) and puts on every `MediaHandlerSet` entry. MC re-pushes each participant's set every **TTL/3** (the one derivation, `MediaDialTiming::refresh_interval`), jittered EARLIER by up to a tenth of that per participant, never later — so the longest gap is TTL/3 < TTL and a held token expires only after ~3 consecutive dropped pushes. Range `[60, 900]` (`MIN_`/`MAX_MEDIA_DIAL_TOKEN_TTL_SECONDS` in `crates/mc-service/src/config.rs`); **missing, unparseable or out of range refuses boot** with the key named. **Trade:** lower = more signing and push work per meeting (participants x handlers / (TTL/3)); higher = a longer window in which a departed participant's token stays signature-valid (MH still declines it at binding: MC answers `sender_id` 0). **Read once at boot:** changing it restarts the pods, which ends their meetings (and their per-meeting dial keys) — schedule like any MC roll. **Rollout:** apply the ConfigMap and both Deployments with or before the image. A new Deployment against a ConfigMap lacking the key gives `CreateContainerConfigError` (no container runs, empty logs; `kubectl describe pod` names the key); a bare image bump onto the old Deployment (no env wiring for the key) crash-loops on `MissingEnvVar("MC_MEDIA_DIAL_TOKEN_TTL_SECONDS")`. See [§Config-failure triage](#config-failure-triage-three-signals-and-kubectl-logs-is-wrong-for-all-three). **Rollback:** revert the image first, then optionally drop the key (a pre-MC-D image ignores it). Reverting MC-D alone is safe only **until MH-1 is deployed**; after that MH admits only `media_dial` tokens, so an MC-D revert requires reverting MH-1 first or every dial is DECLINED. Published as `mc_media_dial_token_ttl_seconds` and `mc_media_dial_token_refresh_interval_seconds`; a value edit rolls mc-0 and mc-1. | — (required; ConfigMap ships `300`) | `300` |
| `MC_MH_REPLACEMENT_MIN_INTERVAL_MS` *(story 4 MC-C)* | **Yes** (hard ref, `mc-service-config`; no Rust default) | The per-meeting minimum interval, in milliseconds, between `AcquireMediaHandler` attempts after MC removed a dead or impaired handler (ADR-0039 §3; removal never waits on GC), AND the first step of the replacement backoff ladder (it doubles per failed attempt — GC out of capacity or unavailable — up to `MC_MH_REPLACEMENT_BACKOFF_MAX_MS`, with earlier-only jitter into `[step/2, step]`, never below this minimum). Range `[100, 60000]` (`MIN_`/`MAX_MH_REPLACEMENT_MIN_INTERVAL_MS` in `crates/mc-service/src/config.rs`); **missing, unparseable or out of range refuses boot** with the key named. **Under load:** there is NO per-MC limiter (user ruling 2026-10-31), so when a handler dies every one of its meetings asks GC at once; LOWERING this raises GC's Acquire rate during a mass handler loss (meetings x attempts), RAISING it delays each meeting's first replacement and every retry. **Rollout:** apply the ConfigMap and both Deployments with or before the image (same failure modes as the other required keys: [§Config-failure triage](#config-failure-triage-three-signals-and-kubectl-logs-is-wrong-for-all-three)). **Rollback:** an older MC ignores the key, so an image-only rollback is safe. Published as `mc_media_handler_replacement_min_interval_seconds`; feeds the derived retry budget below. A value edit rolls mc-0 and mc-1. | — (required; ConfigMap ships `1000`) | `1000` |
| `MC_MH_REPLACEMENT_BACKOFF_MAX_MS` *(story 4 MC-C)* | **Yes** (hard ref, `mc-service-config`; no Rust default) | The cap, in milliseconds, of the replacement backoff ladder (see the row above). Range `[MC_MH_REPLACEMENT_MIN_INTERVAL_MS, 300000]` — **a cap below the minimum interval refuses boot** (as do missing, unparseable or out of range). **Under load:** RAISING it lengthens how long a meeting awaiting a handler (no survivor) waits between attempts once GC is down or out of capacity, so recovery after GC returns is slower; LOWERING it raises the steady retry rate during a long GC outage. **Derived, never configured — the replacement retry budget** = the ladder up to and including its first capped step + one more capped step + the push worker's initial-registration bound (`pusher::initial_registration_bound`, 48 s): **139 s at the shipped defaults** (1+2+4+8+16+30+30 = 91 s, + 48 s). Published as `mc_media_handler_replacement_retry_budget_seconds`; **the awaiting-handler page bound (`MCMediaMeetingsAwaitingHandler`) derives from it, so editing EITHER key moves the page**. **Rollout / rollback:** as the row above. Published as `mc_media_handler_replacement_backoff_max_seconds`. A value edit rolls mc-0 and mc-1. | — (required; ConfigMap ships `30000`) | `30000` |
| `MC_PLANNED_TRANSFER_MARGIN_MS` *(story 4 MC-F)* | **Yes** (hard ref, `mc-service-config`; no Rust default) | How far ahead, in milliseconds, of a draining handler's deadline MC cuts a planned transfer over (ADR-0039 §4/§7). A draining MH reports DRAINING and `drain_remaining_ms` on its per-meeting heartbeat; MC adds the new handler, waits until every routable participant is MH-reported Connected to it, then removes the old one — or cuts over at **the draining heartbeat's receipt + `drain_remaining_ms` − this margin** (stragglers cut over, outcome `timed_out`). **Relation to MH's deadline:** MH's drain deadline is SIGTERM + `terminationGracePeriodSeconds` − MH's 5 s shutdown margin (published per pod as `mh_drain_window_seconds`; 30 s in base), so MC cuts over at SIGTERM + (grace − 5 s) − margin. The margin must cover the set push, the old handler's worker drain and the `EndMeeting` round trip, so MH releases by `EndMeeting` and not by its own deadline shed. **At or above the MH drain window (minus one heartbeat interval) every planned transfer degrades to an immediate cutover (`timed_out`)** — guarded by env-test 02 `test_planned_transfer_margin_fits_inside_the_mh_drain_window` (min over serving MH pods). Range `[1000, 30000]` (`MIN_`/`MAX_PLANNED_TRANSFER_MARGIN_MS` in `crates/mc-service/src/config.rs`); **missing, unparseable, 0 or out of range refuses boot** with the key named. An untrusted `drain_remaining_ms` is clamped to `mc_media_handler_replacement_retry_budget_seconds`, so no value holds a meeting on a draining handler longer. **Rollout:** apply the ConfigMap and both Deployments with or before the image ([§Config-failure triage](#config-failure-triage-three-signals-and-kubectl-logs-is-wrong-for-all-three)). **Rollback:** an older MC ignores the key (see §Rollback, "Rolling MC back from story 4 MC-F"). Published as `mc_media_handler_planned_transfer_margin_seconds`. A value edit rolls mc-0 and mc-1. | — (required; ConfigMap ships `5000`) | `5000` |
| `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` *(story 4 MC-E)* | **Yes** (hard ref, `mc-service-config`; no Rust default) | The share of a handler's believed-connected clients (MH-reported connectivity) that must report losing it for a **client-loss disagreement** (ADR-0039 §8): at least this fraction AND at least `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` of them reporting loss for longer than N while the handler's heartbeats keep it Live counts one episode on `mc_media_handler_client_loss_disagreements_total`. It detects a forged heartbeat ONLY when the forger echoes the live connection ids: an empty or wrong list retires the believed-connected set through the heartbeat reconcile, so the counter stays 0 — **zero is NOT evidence of no forgery**. **Observational only:** client loss never triggers a removal in this story, so tuning this changes only that counter. Client-asserted: forgeable by ≥ `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` colluding participants; not a trust signal. Range: finite, in `(0, 1]`; **missing, unparseable, NaN, infinite, 0 (or `-0`, or an underflow to 0), negative or above 1 refuses boot** with the key named. **Rollout:** apply the ConfigMap and both Deployments with or before the image ([§Config-failure triage](#config-failure-triage-three-signals-and-kubectl-logs-is-wrong-for-all-three)). **Rollback:** an older MC ignores the key, so an image-only rollback is safe (see §Rollback, "Rolling MC back from story 4 MC-E"). Published as `mc_media_handler_client_loss_disagreement_ratio`. A value edit rolls mc-0 and mc-1. | — (required; ConfigMap ships `0.5`) | `0.5` |
| `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` *(story 4 MC-E)* | **Yes** (hard ref, `mc-service-config`; no Rust default) | The minimum count of a handler's believed-connected clients that must report losing it for a client-loss disagreement (with `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION`; both must be met). **Observational only** (as above). Client-asserted: forgeable by this many colluding participants (or one user joining this many times); not a trust signal. Range `[1, 1000]` (`MIN_`/`MAX_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` in `crates/mc-service/src/config.rs`); **missing, unparseable, 0 or out of range refuses boot** with the key named. **Rollout / rollback:** as `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION`. Published as `mc_media_handler_client_loss_disagreement_min_clients`. A value edit rolls mc-0 and mc-1. | — (required; ConfigMap ships `3`) | `3` |
| `MC_KEK_ROTATION_DEBOUNCE_SECONDS` | **Yes** | W, the meeting KEK rotation debounce window (ADR-0036 §4 Rotation, story 2 task 9). Every roster removal schedules a KEK rotation, debounced to at most once per W measured from the OLDEST un-rotated removal. **W is the exposure bound on a departed participant**, so the value is a security setting, not a tuning one. Carried to clients as `kek_rotation_debounce_seconds`; they derive their previous-KEK retention from it, so changing W moves client key hygiene too. Validated both sides at load; the bounds and their derivation live in `crates/mc-service/src/config.rs` (`MIN_`/`MAX_KEK_ROTATION_DEBOUNCE_SECONDS`). Published as `mc_meeting_kek_rotation_window_seconds`. | `60` (in ConfigMap) | `60` |
| `MC_REGION` | No | Geographic region for this MC. | `us-east-1` | `local` |
| `GC_GRPC_URL` | No | Global Controller gRPC endpoint. | `http://localhost:50051` | `http://gc-service.dark-tower:50051` |
| `MC_WEBTRANSPORT_BIND_ADDRESS` | No | WebTransport (QUIC/UDP) bind address. | `0.0.0.0:4433` | `0.0.0.0:4433` |
| `MC_GRPC_BIND_ADDRESS` | No | gRPC bind address for GC communication. | `0.0.0.0:50052` | `0.0.0.0:50052` |
| `MC_HEALTH_BIND_ADDRESS` | No | Health/metrics bind address. | `0.0.0.0:8081` | `0.0.0.0:8081` |
| `MC_ID` | No | Instance identifier. **MUST be unique per process incarnation — leave it unset.** See the warning below. | `mc-$HOSTNAME-<uuid8>` | *(unset — use the default)* |
| `MC_MAX_MEETINGS` *(required since story 4 MC-A)* | **Yes** (hard ref, `mc-service-config`) | Maximum concurrent meetings; GC's assignment is refused `at_capacity` beyond it. **This is the admission load-shed lever the incident runbooks name. Missing or invalid refuses boot** (R-37): absent, unparseable or `0` is a startup error naming the key. Before story 4 MC-A an unparseable value silently fell back to `1000`, so a typo in this lever was invisible. **A malformed value now CrashLoops the pod: check the value before rolling** (under RollingUpdate the old pod keeps serving). `0` is refused because a cap of 0 refuses every meeting, a silent total outage that reads as valid config. Bounds `MIN_CAPACITY_CAP..=u32::MAX` in `crates/mc-service/src/config.rs`. | none (required) | `1000` (in ConfigMap) |
| `MC_MAX_PARTICIPANTS` *(required since story 4 MC-A)* | **Yes** (hard ref, `mc-service-config`) | Maximum participants across all meetings; the connection-side load-shed lever. Same rules as `MC_MAX_MEETINGS`: **missing or invalid (unparseable or `0`) refuses boot**; before story 4 MC-A a typo silently fell back to `10000`. **Check the value before rolling.** | none (required) | `10000` (in ConfigMap) |
| `MC_CLOCK_SKEW_SECONDS` | No | Clock skew allowance (ADR-0023). Bounded `0..=common::jwt::MAX_CLOCK_SKEW` (600 s, the shared JWT ceiling AC also enforces): an unparseable value or one above 600 refuses boot (since story 4 MC-A; before, a typo silently fell back to the default and an over-ceiling value was silently clamped by the validator). | `5` | `5` |
| `MC_DISCONNECT_GRACE_PERIOD_SECONDS` | No | Participant disconnect grace period (ADR-0023): how long an abruptly disconnected participant stays on the roster for a reconnect. **Enforced by every meeting actor** (since the 2026-11-12 join-window devloop, through `Config::meeting_timers`; before that it was parsed but inert). Echoed as `mc_meeting_disconnect_grace_seconds`. **Configurable DOWNWARD only**: bounded to [0, the binding-token TTL `BINDING_TOKEN_TTL` (`crates/mc-service/src/actors/session.rs`, ADR-0023: 30 s)]. A longer grace serves no reconnect (the binding token has expired) and only keeps a departed participant on the roster, deferring its leave KEK rotation and keeping the meeting alive. Above the ceiling, or unparseable, refuses boot. (`MC_BINDING_TOKEN_TTL_SECONDS` and `MC_NONCE_GRACE_WINDOW_SECONDS` were REMOVED in the 2026-11-12 join-window devloop: neither was ever consumed, so MC no longer reads them and a stale value is ignored.) | `30` | `30` |
| `MC_MEETING_JOIN_WINDOW_SECONDS` | No | A meeting no participant joins within this long of its creation (GC's assignment) ends through the same path as an emptied one (`cause=empty`, `reason=join_window_expired`; GC notified). **Set explicitly in the base `config.env`** (Default column; operator direction 2026-11-13); if the key is ever removed MC falls back to the DERIVED default: meeting-token lifetime cap + JWT exp leeway + SDK join deadline (`crates/mc-service/src/actors/meeting_timers.rs::MAX_JOIN_WINDOW`), which is also the ceiling. Bounds: [`LIFETIME_TICK`, derived ceiling]; outside them, or unparseable, refuses boot. Read by both MC deployments via an `optional: true` ref. Echoed as `mc_meeting_join_window_seconds`; the end lines carry the meeting's monotonic `age_ms`. Keep the window well above the p99 client GC-join → MC-admission latency: a client that is consistently slower than the window fails every retry, because each SDK retry gets a FRESH meeting with the SAME window. Symptom: `join_window_expired` rising together with `mc_session_join_failures_total{error_type=meeting_not_found|teardown_in_progress}`. Remedy: raise the window. Raising it does NOT fix clients that cannot reach MC at all (`MCJoinsNotReachingMC`). Older binaries ignore the key (rollback-safe). | `30` | `10` (Kind overlay `configmap-join-window-patch.yaml`) |
| `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` | No | QUIC max-idle-timeout. **Fail-loud parse**: non-numeric or `0` is rejected at load rather than falling back to the library default (`0` = infinite idle timeout, which leaves crash/network-loss departures undetected). | `10` | `10` |
| `OTEL_ENABLED` | No | Enable MC's OTel SDK span export (R-55). Gated by this explicit flag, NOT by presence of `OTLP_ENDPOINT`. Unrecognised values fail at load. | `false` | `true` |
| `OTLP_ENDPOINT` | No | OTLP-gRPC collector endpoint for MC's own span export. `http://` scheme required. Only consumed when `OTEL_ENABLED=true`. | None | `http://otel-collector.dark-tower:4317` |
| `OTEL_SAMPLE_RATE` | No | Head-sampling ratio in `[0.0, 1.0]`; `init_otel` validates. | `1.0` | `1.0` |
| `DEPLOYMENT_ENVIRONMENT` | No | `deployment.environment` resource attribute (ADR-0011). | `development` | `production` |
| `RUST_LOG` | No | Logging level. Set as a literal in the Deployment, not via ConfigMap. | `info` | `info,mc_service=debug` |

> **DO NOT PIN `MC_ID` PER POD — a stable value turns a rolling deploy into a
> meeting outage (story 2 task 12).** Setting `MC_ID=mc-0` is the tempting
> choice: it matches the pod names, matches every `deployment/mc-0` command in
> this runbook, and reads tidier than a generated string. It is unsafe. MC now
> releases its meetings' handlers on graceful shutdown (`EndMeeting`), and the
> **only** thing preventing a departing pod from releasing meetings its
> successor has already taken over is MH's ownership check — which compares
> `mc_id`. Under the Deployments' RollingUpdate surge the old and new pods run
> concurrently; if they share an `MC_ID`, the departing pod's release **passes**
> that check and drops the successor's live meeting, closing every participant's
> MH connection at once, with no client-visible cause. Leave `MC_ID` unset: the
> default (`mc-$HOSTNAME-<uuid8>`, regenerated on every process start) is unique
> per incarnation, which is the property the safety argument rests on. Nothing
> in `infra/services/mc-service/**` sets it today, and nothing should.
> Cross-check: a departing pod whose release is correctly refused counts
> `mc_media_end_meeting_total{outcome="superseded_by_successor"}`, which is
> expected during a roll — see `mc-incident-response.md` Scenario 20.

**The rows whose Required column reads Yes are the CrashLoop list.** (They are not counted
here: a restated count is what goes stale.) Each is loaded via
`ConfigError::MissingEnvVar` in `crates/mc-service/src/config.rs`, so absence
fails config load before the server binds. The story-3 **hard ref** rows are on that list too;
note that because their `configMapKeyRef` is not `optional`, a key absent from the ConfigMap
surfaces first as a `CreateContainerConfigError` (kubelet, empty logs, no container ever ran)
rather than as a CrashLoop. The retired rows (`MC_MAX_RECEIVE_SLOTS`, `MC_AUDIO_*`) are not on
the list: MC no longer reads them. For Yes rows: the pod enters `CrashLoopBackOff`
and the reason is the first line of the container log. `MC_TLS_CERT_PATH` and
`MC_TLS_KEY_PATH` additionally fail when the *file* is absent — but **that check
is only reachable once the container starts**, so it is the signal for a Secret
that exists and is malformed or empty, **not** for a Secret that is missing. A
missing `mc-service-tls` never gets that far: the volume is not `optional:`, so
kubelet cannot complete the mount and the pod stays Pending in
`ContainerCreating`. See §Config-failure triage, which lists all three signals.

> **Maintenance.** This table is verified against `crates/mc-service/src/config.rs`.
> It previously documented six variables that did not exist in any MC build
> (`GC_REGISTRATION_URL`, `MC_CAPACITY`, `WEBTRANSPORT_BIND_ADDRESS`,
> `HTTP_BIND_ADDRESS`, `ACTOR_MAILBOX_SIZE`, `GC_HEARTBEAT_INTERVAL_SECS`)
> while omitting every genuinely required one — an inverted `Required` column
> on the artifact an operator reads at 2am. `dt-guard env-config` checks
> manifests against `config.rs` but does **not** read this runbook, so nothing
> catches drift here automatically. Re-verify against `config.rs` whenever a
> key is added or removed.

> **(Pre-story-3 history.) `MC_AUDIO_*` had no observable effect until the client honoured the send
> directive** (story task 19). Since story 3 MC-A these keys are not read at all — the profile's
> `MEDIA_AUDIO_*` (rows above) drive the directive. Tuning the retired three changes nothing
> on the current image; they are kept only for the rollback window.

### Video-off lever

**`MC_MAX_VIDEO_RECEIVE_SLOTS=0` turns video off and leaves audio untouched** (story 3 R-26). What it does, its cost (it rolls both MC pods and never MH), its blast radius, its
place **before any rollback**, and how it is verified have one home:
[`mh-deployment.md` §Operator levers](mh-deployment.md#operator-levers-for-media-video-off-and-nothing-else).
This subsection carries only the MC procedure.

- **Procedure.** Make the edit **on the commit that is currently deployed, with nothing else
  changed**. Set `MC_MAX_VIDEO_RECEIVE_SLOTS=0` in `infra/services/mc-service/config.env` and
  apply the environment root (Kind: `dev-cluster deploy` / `infra/kind/scripts/deploy.sh`).
  `deploy.sh` converges **every** workload to the checked-out tree, so any other difference rolls
  too, and an MH difference sheds MH sessions. With only this edit, content addressing rolls mc-0
  and mc-1 and nothing else. Do not `kubectl edit` the live ConfigMap
  ([§Config-failure triage](#config-failure-triage-three-signals-and-kubectl-logs-is-wrong-for-all-three)).
- **When it applies.** MC reads the caps ONCE at startup and never re-truncates a live slot
  vector; the lever applies to declarations made after the MC restart — and that restart
  reconnects everyone, so in practice every participant re-joins under the new cap.
- **Confirm.** `mc_media_receive_slot_cap{media_kind="video", content_kind="main"}` reads **0 on both mc-0 and mc-1**
  (and the startup line's `max_video_receive_slots=0`), and
  `mc_media_slot_states_total{media_kind="video", content_kind="main",slot_state="unavailable"}` rises while every
  other video slot state stays flat.
- **Undo.** Make the reverse edit the same way. It costs another MC roll, and restores directives.
- **A host's server unmute can resume a participant's camera or microphone** (R-18), at any time
  and independent of this lever, unless the participant has client-muted that kind. The
  participant is shown a non-suppressible notice at that moment and can client-mute at once.

### Config-failure triage: three signals, and `kubectl logs` is wrong for all three

Five required keys mean two distinct startup failures with **opposite first
steps**; a third, the TLS Secret, produces neither and is listed with them
because it presents at the same moment and reaches the same operator. What they
have in common is the trap: **the standard `kubectl logs` reflex is wrong for
every one of them.**

| Cause | Signal | Where the answer is |
|---|---|---|
| Deployment has a `configMapKeyRef` the ConfigMap lacks (new Deployment applied against an old ConfigMap) | `CreateContainerConfigError` — **no container ever ran** | `kubectl describe pod`. `logs` is **empty** and `exec` is impossible — an operator reading only those concludes "the pod is wedged" when `describe` names the missing key |
| New image with an old pod template lacking the new `env` entries (`kubectl set image`, a partial apply) | `CrashLoopBackOff` | `kubectl logs --previous` — `ConfigError::MissingEnvVar` names the variable |
| `mc-service-tls` Secret absent (fresh namespace, `apply -k` only — it is created imperatively and is outside the coupled set) | `FailedMount`; pod Pending in `ContainerCreating` | `kubectl describe pod`. The `mc-tls` volume is not `optional:`, so kubelet blocks the mount and the container **never starts** — there is no config load, no `MC_TLS_CERT_PATH` error and no log at all. See §Kubernetes Secrets |

**Three signals, and `kubectl logs` is empty or misleading for all three.** That
is the generalisation worth carrying rather than the rows: the standard first
reflex is wrong every time. `describe pod` is what names the artifact for the
first and third; `logs --previous` is what names it for the second, and plain
`logs` is empty for both of those. The three are also distinguishable *before*
you read anything, by pod phase alone — Pending/`ContainerCreating` is the
Secret, `CreateContainerConfigError` is the ConfigMap key, and a pod that runs
and exits is the stale pod template.


**Rollout ordering.** The manifest change and the code change roll **together**.
The ConfigMap must never be reverted independently of the image, and the image
must never be applied against a stale ConfigMap. The ConfigMaps are
`configMapGenerator` entries in `infra/services/mc-service/kustomization.yaml`,
**content-addressed** (ADR-0038 §2). kustomize puts a hash of the data in each
name and rewrites the pods' references, so **applying a ConfigMap change rolls
exactly the pods that consume it**. A wrong value therefore fails at deploy
time, not at some later restart. Editing the live object (`kubectl edit
configmap`) does nothing useful: nothing references the name you would edit
after the next apply, and the next apply of the environment root overwrites it
anyway. Both MC pods fail identically at startup, so the PDB offers no
protection: this is a full signalling-plane outage, not a partial one.

**A `redis-config` change is now a stateful restart.** Redis consumes a
content-addressed ConfigMap too, so a `infra/services/redis/redis.conf` change
rolls the Redis StatefulSet. In the Kind dev cluster that drops MC session
state; in production it is a planned stateful restart, not a config reload.

**Recovery — applying the ConfigMap alone is NOT enough.** This was observed on
2026-09-03 during the validation of the change that introduced these five keys,
so the sequence below is what actually worked rather than what ought to:

```bash
# 1. Confirm the cause before changing anything.
kubectl get pods -n dark-tower -l app=mc-service          # CrashLoopBackOff?
kubectl logs -n dark-tower deployment/mc-0 --previous | head -5
#   -> Error: MissingEnvVar("MC_MAX_AUDIO_RECEIVE_SLOTS")   (or whichever required key the release added)

# 2. Apply the ENVIRONMENT ROOT, which carries the whole coupled set in one
#    object stream: the ConfigMap supplies the value, and the Deployment supplies
#    the configMapKeyRef that injects it. The ConfigMap is content-addressed, so
#    the apply changes both pod templates and rolls exactly the pods whose
#    configuration changed. There is no separate restart step.
#
#    Do NOT apply a single file or a base directly. `config.env` is a
#    configMapGenerator SOURCE, not a manifest. Applying the service BASE
#    (`kubectl apply -k infra/services/mc-service/`) skips the Kind overlay's
#    patches (OTEL_ENABLED="true" in mc-service-config via
#    infra/kubernetes/overlays/kind/services/mc-service/configmap-otel-patch.yaml)
#    and silently turns MC's span export OFF at the moment you most need it.
#    On a devloop cluster, applying even the plain overlay root reverts the
#    per-cluster advertise addresses. Go through deploy.sh, which chooses the
#    right render (infra/kind/scripts/deploy.sh:apply_env_root):
#
# 2a. Devloop / Kind cluster:
./infra/kind/scripts/deploy.sh
#     (from the devloop container: dev-cluster deploy). This applies the whole
#     environment root and waits for EVERY workload in it, not just MC.

# 3. Confirm the roll (the apply already started it).
kubectl rollout status deployment/mc-0 -n dark-tower
kubectl rollout status deployment/mc-1 -n dark-tower
```

**Fast rollback lever.** `kubectl rollout undo deployment/mc-0 deployment/mc-1
-n dark-tower` returns each pod template to its previous revision, which still
references the previous hash-suffixed ConfigMap. That ConfigMap still exists
because nothing prunes old generations. **Any future pruning of old ConfigMap
generations must spare the ones referenced by retained ReplicaSets or
ControllerRevisions**, or this lever breaks (`CreateContainerConfigError` /
`FailedMount` on undo). This runbook deliberately publishes no cleanup
command. The durable fix is still to change the tree and re-apply.

**On a devloop cluster, `deploy` applies the manifests and the image together.**
`dev-cluster deploy` (`infra/kind/scripts/deploy.sh`) builds every image, runs the
migration Job and applies the whole environment root on every run, and Layer 7
runs it on every gate (ADR-0038) — so a change under `infra/services/mc-service/**`
reaches the cluster with the image that needs it; content-addressed ConfigMaps roll
only what changed. (Before ADR-0038 the image was rebuilt without applying the
manifests, which made the CrashLoop above the guaranteed outcome.) If you see it
now, the cluster was not converged — run `dev-cluster deploy`.

**The symptom may not look like a config problem at all.** In the 2026-09-03
occurrence (pre-ADR-0038) the first error surfaced was a Layer 7
`PRECONDITION_FAILURE` on `port 24500 (prometheus) is already in use`. That was
downstream noise: cluster setup re-ran *because* mc-0 and mc-1 were unhealthy, and
the re-run collided with ports the existing cluster still held. **If a cluster-level
step fails with a port or resource-conflict message, check MC pod health before
believing it.**

### Kubernetes Secrets

**Secret: `mc-service-secrets`** (namespace: `dark-tower`) — supplies three env
vars by `secretKeyRef`:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: mc-service-secrets
  namespace: dark-tower
type: Opaque
stringData:
  REDIS_URL: "redis://:<password>@redis.dark-tower:6379"
  MC_BINDING_TOKEN_SECRET: "<openssl rand -base64 32>"
  MC_CLIENT_SECRET: "<oauth client secret>"
```

**Secret: `mc-service-tls`** (namespace: `dark-tower`) — a *separate* Secret,
mounted as the `mc-tls` volume at `/etc/mc-tls` (mode `0400`), not injected as
env vars. Keys are `tls.crt` and `tls.key`; `MC_TLS_CERT_PATH` /
`MC_TLS_KEY_PATH` point at the mount paths.

> **It is NOT in the kustomization and `apply -k` will not create it.**
> `infra/services/mc-service/kustomization.yaml` lists `secret.yaml`, which
> defines only `mc-service-secrets`. `mc-service-tls` is created *imperatively*
> by `infra/kind/scripts/provision.sh::create_tls_secret` (provision materializes the
> certs with `scripts/generate-dev-certs.sh`, then `kubectl create secret tls`) — so it is
> outside the ConfigMap + Deployments + image coupled set and survives every
> `apply -k`. **On a fresh namespace it must be created before the pods start.**
>
> **The signal is `FailedMount`, and there are NO LOGS.** The `mc-tls` volume is
> not marked `optional:`, so a missing Secret blocks the mount outright: both MC
> pods sit Pending in `ContainerCreating`, the process **never starts**, and
> there is no `MC_TLS_CERT_PATH` error and no container log to read. Only
> `kubectl describe pod` names the Secret. Do **not** go looking for a cert-load
> failure — `Identity::load_pemfiles` failing is a *different* problem that
> requires the Secret to exist and be malformed. See §Config-failure triage.
>
> ```bash
> # Does it exist, and is the cert still valid? (No exec needed -- the runtime
> # image is distroless and has no openssl.)
> kubectl get secret mc-service-tls -n dark-tower
> kubectl get secret mc-service-tls -n dark-tower \
>   -o jsonpath='{.data.tls\.crt}' | base64 -d | openssl x509 -noout -dates
> ```

### Kubernetes ConfigMap

**ConfigMap: `mc-service-config`** (namespace: `dark-tower`) — shared across
instances. It is **generated** from `infra/services/mc-service/config.env` (the
deployed source of truth) by `configMapGenerator`. The live name carries a
content hash (`mc-service-config-<hash>`, ADR-0038 §2); the block below is the
rendered shape.

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mc-service-config
  namespace: dark-tower
data:
  MC_GRPC_BIND_ADDRESS: "0.0.0.0:50052"
  MC_HEALTH_BIND_ADDRESS: "0.0.0.0:8081"
  MC_WEBTRANSPORT_BIND_ADDRESS: "0.0.0.0:4433"
  MC_REGION: "local"
  GC_GRPC_URL: "http://gc-service.dark-tower:50051"
  AC_JWKS_URL: "http://ac-service.dark-tower:8082/.well-known/jwks.json"
  MC_TLS_CERT_PATH: "/etc/mc-tls/tls.crt"
  MC_TLS_KEY_PATH: "/etc/mc-tls/tls.key"
  OTEL_ENABLED: "false"
  OTLP_ENDPOINT: "http://otel-collector.dark-tower:4317"
  OTEL_SAMPLE_RATE: "1.0"
  DEPLOYMENT_ENVIRONMENT: "development"
  # ---- ADR-0036 media signalling (REQUIRED unless marked RETIRED; absent = CrashLoop) ----
  # Documented defaults live in infra/services/mc-service/config.env, not as
  # Rust constants: the load path has no unwrap_or, so THAT file is the only
  # statement of the intended value. THIS BLOCK IS A COPY — do not edit it to
  # change a value; edit config.env and apply the root (see the ordering banner there).
  # Same convention as the mc-alerts.yaml pointer further down this file.
  MC_MAX_AUDIO_RECEIVE_SLOTS: "8"              # story 3; bounds in config.rs
  MC_MAX_VIDEO_RECEIVE_SLOTS: "4"              # story 3; 0 = video-off lever (R-26)
  MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS: "500"     # story 3; 100..=10000, bounds in directive.rs
  MC_MAX_RECEIVE_SLOTS: "8"                    # RETIRED (story 3); no longer read (rollback-window retention)
  MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS: "64"
  MC_AUDIO_CODEC: "opus"                       # RETIRED (story 3); no longer read (rollback-window retention)
  MC_AUDIO_MAX_BITRATE_BPS: "48000"            # RETIRED (story 3); no longer read (rollback-window retention)
  MC_AUDIO_FRAME_RATE_HZ: "50"                 # RETIRED (story 3); no longer read (rollback-window retention)
  # ---- ADR-0036 §9 connect settle window (REQUIRED; absent = CrashLoop) ----
  MC_MEDIA_CONNECT_SETTLE_MS: "1500"           # validated 100..=10000 at load; see config.env
  # ---- ADR-0036 §4 KEK rotation (REQUIRED; absent = CrashLoop) ----
  MC_KEK_ROTATION_DEBOUNCE_SECONDS: "60"       # W; bounds in config.rs; see config.env
  # ---- Capacity caps (R-37, story 4 MC-A; REQUIRED; absent/unparseable/0 = CrashLoop) ----
  MC_MAX_MEETINGS: "1000"
  MC_MAX_PARTICIPANTS: "10000"
```

**Rolling out and back the two required caps (story 4 MC-A).** `mc-service-config` is a
hash-suffixed `configMapGenerator`, so the ConfigMap carrying `MC_MAX_MEETINGS` /
`MC_MAX_PARTICIPANTS` and the Deployments that hard-reference them land in the same kustomize apply
(`deploy.sh`). `kubectl rollout undo` returns to the previous ReplicaSet and its retained ConfigMap
generation, and the pre-MC-A image ignores both keys, so they need no "keep until the rollback
window closes" note. The one bad path is changing the image WITHOUT the manifest (a bare
`kubectl set image`): the new pod refuses boot on the missing key and CrashLoops, while
RollingUpdate keeps the old pod serving. Loud and contained.

**Shared with MH: `media-codec-profile`** (story 3), generated from
`infra/services/media-codec-profile/profile.env`, a generator-only base. It holds the eight `MEDIA_*`
encoding keys MC directs and MH costs against. Its content is not copied here. Read
`profile.env`, whose header carries the membership rule (only values both services must agree on;
a header rule, not a guard) and the roll cost: **any edit rolls mc-0 and mc-1 and replaces every
MH (`mh.sh upgrade`) together.** Kind runs a lower test profile (story 3 R-28), set only by the Kind test-profile patch
`infra/kubernetes/overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml`.
There is no `dev-web.sh` override for the profile: to change it, edit or remove that patch and
redeploy. The patch header is the one statement of the production-quality procedure, including the
MH stream-ceiling check and the restore before Layer 7.

**Per-instance ConfigMaps: `mc-0-config`, `mc-1-config`** — carry only
`MC_WEBTRANSPORT_ADVERTISE_ADDRESS`, which necessarily differs per pod. Both
Deployments use per-key `configMapKeyRef` (no `envFrom`), so **every new key
needs an entry in both `mc-0-deployment.yaml` and `mc-1-deployment.yaml`**.
`dt-guard env-config` enforces this **only for keys `config.rs` declares
required** via `ConfigError::MissingEnvVar` — its per-workload check
(`missing_in_manifest`) is built from those literals. `orphan_configmap_key`
does **not** cover it: that rule fires only when *no* workload references the
key at all, so one of two references satisfies it. A non-required key wired
into one Deployment and forgotten on the other is therefore **guard-green while
the two pods run different values**, which is why these keys are required
with no Rust default. A *required* key referenced by only one CrashLoops that
pod alone, which presents as "the meeting works about half the time" depending
on which pod GC assigned.

### OpenTelemetry (R-55) and Break-glass

MC's OTel SDK is initialized in `main.rs` via `init_otel`, gated by the explicit
`OTEL_ENABLED` flag (see the four `OTEL_*` / `DEPLOYMENT_ENVIRONMENT` env vars
above). The prod base ConfigMap (`infra/services/mc-service/config.env`)
ships `OTEL_ENABLED="false"`; the Kind overlay
(`infra/kubernetes/overlays/kind/services/mc-service/configmap-otel-patch.yaml`)
strategic-merge-patches it to `"true"` for the dev cluster.

**`init_otel` is fail-hard at startup:** with `OTEL_ENABLED=true`, if the
collector at `OTLP_ENDPOINT` is unreachable the pod fails readiness. Enabling
therefore requires ALL of: `OTEL_ENABLED=true` **and** the MC→collector `:4317`
egress NetworkPolicy rule (present in `network-policy.yaml`) **and** a reachable
collector.

**Break-glass / escape hatch** — if the collector is unavailable and MC must
boot without OTel:

1. Set `OTEL_ENABLED=false` in `infra/services/mc-service/config.env` (or, in
   Kind, drop the `configmap-otel-patch.yaml` from the overlay
   `kustomization.yaml`).
2. Apply the environment root (Kind: `./infra/kind/scripts/deploy.sh`, or
   `dev-cluster deploy`). `mc-service-config` is content-addressed, so the apply changes
   **both** MC pod templates (`mc-0`, `mc-1`) and rolls them. There is no
   separate restart step:
   ```bash
   kubectl rollout status deployment/mc-0 -n dark-tower
   kubectl rollout status deployment/mc-1 -n dark-tower
   ```
3. MC boots with the OTel layer off and no collector dependency. Unlike a
   StatefulSet, a bad `OTEL_ENABLED=true` rollout does not cause an outage:
   the old ReplicaSet pods keep serving until the new ones pass readiness, so
   `kubectl rollout undo deployment/mc-0` (and `mc-1`) is also a valid revert.

### Resource Limits

**Current configuration** (from `deployment.yaml`):

```yaml
resources:
  requests:
    cpu: 500m      # 0.5 CPU cores
    memory: 512Mi  # 512 MiB RAM
  limits:
    cpu: 2000m     # 2 CPU cores
    memory: 1Gi    # 1 GiB RAM
```

**Tuning guidance:**
- **requests**: Guaranteed resources, used for scheduling
- **limits**: Maximum resources, pod killed if exceeded
- MC is more CPU-intensive than memory-intensive (message processing)
- Scale horizontally for capacity, not vertically

---

## Common Deployment Issues

### Issue 1: GC Registration Failures

**Symptoms:**
- Pods not reaching Ready state
- Logs show: `Failed to register with GC`, `GC unreachable`
- MC not appearing in GC database

> **Registration is gRPC, not HTTP.** MC calls `RegisterMc` on GC's gRPC server at
> `GC_GRPC_URL` (`crates/mc-service/src/grpc/gc_client.rs::register`). There is
> **no `GC_REGISTRATION_URL`** — that variable is one of the six phantoms named in
> the §Environment Variables maintenance note above and appears in no MC build.
> Do not `curl` a registration endpoint; there isn't one.

**Causes:**
- GC service not running or not healthy
- `GC_GRPC_URL` incorrect in the `mc-service-config` ConfigMap
- NetworkPolicy blocking MC → GC traffic
- GC rejecting registration (capacity, region mismatch)

**Resolution:**

```bash
# Check GC service is running
kubectl get pods -n dark-tower -l app=gc-service

# Verify the gRPC endpoint MC was actually given, read from the pod spec rather
# than the ConfigMap -- the ConfigMap being out of step with what the Deployment
# injects is itself a failure mode, so the artifact under suspicion cannot also
# be the evidence. No shell needed (the runtime image may be distroless).
kubectl set env deployment/mc-0 --list -n dark-tower | grep GC_GRPC_URL
kubectl set env deployment/mc-1 --list -n dark-tower | grep GC_GRPC_URL

# What MC logged at startup, including the effective config line
kubectl logs deployment/mc-0 -n dark-tower --tail=100 | grep -i "register\|gc"
kubectl logs deployment/mc-1 -n dark-tower --tail=100 | grep -i "register\|gc"

# Confirm GC is actually serving gRPC on 50051
kubectl get svc gc-service -n dark-tower -o jsonpath='{.spec.ports}'
```

**Fix:**
1. Ensure GC service is running and ready
2. Correct `GC_GRPC_URL` in `infra/services/mc-service/config.env`, then apply
   the environment root. The ConfigMap is content-addressed, so the apply rolls
   both MC pods; see §Config-failure triage
3. Adjust NetworkPolicy to allow MC → GC egress on **TCP:50051**
   (`infra/services/mc-service/network-policy.yaml`); MC's gRPC *ingress* from GC
   is TCP:50052

### Issue 2: WebTransport Listener Failures

**Symptoms:**
- Pods failing readiness checks
- Logs show: `Failed to bind WebTransport listener`, `Address already in use`
- No WebTransport connections possible

**Causes:**
- Port conflict on host
- TLS certificate/key not mounted
- Invalid TLS configuration
- Previous pod not fully terminated

**Resolution:**

```bash
# Check for port conflicts
kubectl get pods -n dark-tower -l app=mc-service -o wide
# Multiple pods on same node may conflict

# Check TLS secrets are mounted
kubectl describe pod <mc-pod> -n dark-tower | grep -A 5 "Mounts:"

# Check logs for TLS errors
kubectl logs deployment/mc-0 -n dark-tower --tail=100 | grep -i "tls\|cert\|webtransport"
```

**Fix:**
1. Verify TLS secrets exist and are mounted
2. Check pod anti-affinity rules (avoid port conflicts)
3. Wait for old pods to fully terminate before redeployment

### Issue 3: Actor System Initialization Failures

**Symptoms:**
- Pods crash on startup
- Logs show: `Actor system failed to initialize`, `Panic in actor`
- CrashLoopBackOff state

**Causes:**
- Configuration errors
- Resource exhaustion (file descriptors, memory)
- Bug in actor initialization code

**Resolution:**

```bash
# Check previous pod logs (before crash)
kubectl logs <mc-pod> -n dark-tower --previous

# Check resource limits
kubectl describe pod <mc-pod> -n dark-tower | grep -A 5 "Limits:"

# Check events
kubectl get events -n dark-tower --field-selector involvedObject.name=<mc-pod>
```

**Fix:**
1. Review actor configuration
2. Increase resource limits if needed
3. Rollback to previous version if bug introduced

### Issue 4: High Memory Usage After Deployment

**Symptoms:**
- Memory usage growing rapidly after deployment
- Pods approaching memory limit
- Potential OOMKilled events

**Causes:**
- Memory leak in new code
- Increased connection/meeting load
- Actor mailbox buildup

**Resolution:**

```bash
# Monitor memory usage
kubectl top pods -n dark-tower -l app=mc-service

# Check for mailbox buildup
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &
curl http://localhost:8080/metrics | grep mc_actor_mailbox_depth
kill %1

# Check connection count (each connection uses memory)
curl http://localhost:8080/metrics | grep mc_connections_active
```

**Fix:**
1. If mailbox buildup: investigate slow message processing
2. If connections high: verify capacity limits
3. If memory leak: rollback and investigate

---

## Smoke Tests

Run these tests immediately after deployment to verify core functionality.

### Test 1: Health Check (Liveness)

**Purpose:** Verify process is running and responsive.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

# Test health endpoint
curl -i http://localhost:8080/health

# Expected response:
# HTTP/1.1 200 OK
# Content-Length: 2
#
# OK

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Response body: `OK`
- Response time: <100ms

### Test 2: Readiness Check

**Purpose:** Verify actor system ready and GC registered.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

# Test readiness endpoint
curl -i http://localhost:8080/ready

# Expected response:
# HTTP/1.1 200 OK
# Content-Type: application/json
#
# {"status":"ready","gc_registered":true,"actor_system":"healthy"}

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- JSON body with `status: "ready"`
- `gc_registered: true`
- `actor_system: "healthy"`
- Response time: <500ms

### Test 3: Metrics Endpoint

**Purpose:** Verify Prometheus metrics are exposed.

```bash
# Port-forward to pod
kubectl port-forward -n dark-tower deployment/mc-0 8080:8081 &

# Fetch metrics
curl -s http://localhost:8080/metrics | head -50

# Expected output (Prometheus text format):
# # HELP mc_meetings_active Number of active meetings
# # TYPE mc_meetings_active gauge
# mc_meetings_active 0
# # HELP mc_connections_active Number of active WebTransport connections
# # TYPE mc_connections_active gauge
# mc_connections_active 0
# ...

# Kill port-forward
kill %1
```

**Success criteria:**
- HTTP 200 status
- Prometheus text format
- Metrics present: `mc_meetings_active`, `mc_connections_active`, `mc_actor_mailbox_depth`
- Response time: <1s

### Test 4: GC Heartbeat Verification

**Purpose:** Verify MC is registered with GC and heartbeats are succeeding.

```bash
# Check GC database for MC registration
kubectl exec -it deployment/gc-service -n dark-tower -- \
  psql $DATABASE_URL -c "SELECT id, region, capacity, current_sessions, last_heartbeat, status FROM meeting_controllers ORDER BY last_heartbeat DESC LIMIT 5;"

# Expected: MC entry with:
# - status = 'active'
# - last_heartbeat within last 15 seconds
# - capacity and current_sessions reasonable
```

**Success criteria:**
- MC appears in GC database
- Status is 'active'
- last_heartbeat is recent (<15 seconds old)

### Test 5: Join Flow (WebTransport + Signaling)

**Purpose:** Verify the full participant join flow — WebTransport connect, JoinRequest/JoinResponse, and participant notifications.

**Prerequisites:** A valid meeting must exist (created via GC). Obtain a participant JWT from GC's join endpoint.

```bash
# Step 1: Obtain join token via GC
kubectl port-forward -n dark-tower deployment/gc-service 8080:8080 &
JOIN_RESPONSE=$(curl -s http://localhost:8080/api/v1/meetings/TEST-CODE \
  -H "Authorization: Bearer $TOKEN")
MC_URL=$(echo "$JOIN_RESPONSE" | jq -r '.mc_assignment.mc_url')
JOIN_TOKEN=$(echo "$JOIN_RESPONSE" | jq -r '.token')
kill %1

echo "MC URL: $MC_URL"
echo "Join token obtained: $([ -n "$JOIN_TOKEN" ] && echo 'yes' || echo 'no')"

# Step 2: Verify WebTransport connection to MC
# Use a WebTransport test client (e.g., webtransport-cli or browser devtools)
# Connect to: $MC_URL with the join token
# Expected: Connection established, HTTP/3 CONNECT succeeds

# Step 3: Send JoinRequest over WebTransport session
# Using the established WebTransport session, send a JoinRequest protobuf message
# Expected: MC responds with JoinResponse containing:
#   - participant_id (non-empty UUID)
#   - session info (meeting_id, current participants)
#   - media configuration

# Step 4: Verify participant notification
# Other participants in the meeting (if any) should receive a
# ParticipantJoined notification via their WebTransport session
# Expected: Notification contains new participant's display name and ID

# Step 5: Verify join metrics incremented.
# PORT 8081 and Deployment `mc-0`, not `mc-service:8080`: MC_HEALTH_BIND_ADDRESS
# is 0.0.0.0:8081 (config.env) and the pods declare containerPort 8081 —
# there is no listener on 8080, and there is no Deployment named `mc-service`
# (that string is a Service, a PDB and a container name). A `8080:8080` forward
# to it yields connection-refused, which looks identical to "the metric did not
# increment" — and this step is the designated producer for the post-deploy
# "Forwarding policy confirmed live" gate below, so a broken command here makes
# that gate permanently unrunnable rather than merely red.
kubectl port-forward -n dark-tower deployment/mc-0 8081:8081 &
curl -s http://localhost:8081/metrics | grep -E "mc_session_joins_total|mc_session_join_duration|mc_media_policy_pushes_total"
kill %1

# Expected:
# mc_session_joins_total should have incremented
# mc_session_join_duration_seconds should show recent observation
# mc_media_policy_pushes_total{outcome="match"} should have incremented — this join
#   programmed the assigned MH with the meeting's forwarding policy and the handler
#   echoed back the generation MC sent (ADR-0036 §8). This is the producer for the
#   post-deploy "Forwarding policy confirmed live" gate, so running Test 5 answers
#   that gate without a second port-forward.
```

**Success criteria:**
- WebTransport connection established successfully (HTTP/3 CONNECT 200)
- JoinRequest accepted, JoinResponse received with valid participant_id
- Existing participants receive ParticipantJoined notification
- `mc_session_joins_total` counter incremented
- `mc_session_join_duration_seconds` recorded (p95 <500ms SLO)
- No errors in MC logs related to the join flow
- Response time: JoinRequest to JoinResponse <500ms

---

## Monitoring and Verification

### Key Metrics to Monitor Post-Deployment

**Service health:**

```promql
# Pod restart count (should be 0 after initial deployment)
kube_pod_container_status_restarts_total{namespace="dark-tower",pod=~"mc-service-.*"}

# Pod readiness (should be 1 for all pods)
kube_pod_status_ready{namespace="dark-tower",pod=~"mc-service-.*"}
```

**Actor system health:**

```promql
# Mailbox depth by actor type (should be low, <50)
sum by(actor_type) (mc_actor_mailbox_depth)

# Actor panics (should be 0)
sum(increase(mc_actor_panics_total[5m]))

# Messages dropped per second (should be near 0)
sum by(actor_type) (rate(mc_messages_dropped_total[5m]))
```

**Latency:**

```promql
# p95 session join duration, success only (SLO: <2s)
histogram_quantile(0.95, sum by(le) (rate(mc_session_join_duration_seconds_bucket{status="success"}[5m])))

# p99 Redis op latency (SLO: <10ms)
histogram_quantile(0.99, sum by(le) (rate(mc_redis_latency_seconds_bucket[5m])))
```

**Capacity:**

```promql
# Active meetings per pod
sum(mc_meetings_active)

# Active connections per pod
sum(mc_connections_active)
```

**GC integration:**

```promql
# Heartbeat success rate (should be near 100%)
sum(rate(mc_gc_heartbeats_total{status="success"}[5m])) / sum(rate(mc_gc_heartbeats_total[5m]))
```

### Grafana Dashboards

**Recommended dashboards:**
- **MC Overview** - Active meetings, connections, mailbox depth, panics, join flow, MH coordination (incl. teardown)
- **MC Media Path** - ADR-0036 media rows: media routing, client media signalling (incl. server mute), media connectivity, KEK lifecycle
- **MC SLOs** - Session join duration, Redis latency, drop rate, error budget

See `infra/grafana/dashboards/mc-overview.json`, `infra/grafana/dashboards/mc-media.json` and `infra/grafana/dashboards/mc-media-handlers.json` (story-4 handler lifecycle).

### Alerting Rules

**Critical alerts (page on-call):**
- `MCDown` - No MC pods running for >1 minute
- `MCActorPanic` - Any actor panic
- `MCHighMailboxDepthCritical` - Mailbox depth >500 for >2 minutes
- `MCMediaConnectionAllFailed` - Every client media connection attempt failing

`infra/docker/prometheus/rules/mc-alerts.yaml` is the source of truth; this list
is a convenience copy. **Verify against that file before relying on a name here**
— nothing checks this list, and it previously named two page alerts,
`MCHighLatency` and `MCHighMessageDropRate`, that have never existed in the rule
file. Believing a page alert covers you when no rule exists is worse than knowing
you have no alert. (`MCHighJoinLatency`'s own description still refers to "the
aggregate `MCHighLatency` page alert" — that reference is also stale; join
latency is covered at `severity: info` only.)

### Post-Deploy Monitoring Checklist: Join Flow

Use this checklist after any deployment that touches join flow code (WebTransport session handling, JoinRequest/JoinResponse processing, participant notifications, or JWT validation). For routine deployments that do not affect the join path, the general monitoring section above is sufficient.

**15-minute check:**

```promql
# Session join rate (should be > 0 if traffic is flowing)
sum(increase(mc_session_joins_total[5m]))

# Join failure rate (should be < 1%)
sum(rate(mc_session_join_failures_total[5m]))
/ sum(rate(mc_session_joins_total[5m]))

# Join latency p95 (SLO: < 500ms)
histogram_quantile(0.95,
  sum by(le) (rate(mc_session_join_duration_seconds_bucket[5m]))
)
```

- [ ] `mc_session_joins_total` rate stable or increasing (confirms join traffic is flowing)
- [ ] `mc_session_join_duration_seconds` p95 within SLO (<500ms)
- [ ] `mc_session_join_failures_total` not spiking (failure rate <1%)
- [ ] `mc_webtransport_connections_total{status="rejected"}` not elevated vs. pre-deploy baseline
- [ ] `mc_jwt_validations_total{result="failure"}` not elevated vs. pre-deploy baseline
- [ ] No new `MCHighJoinFailureRate` or `MCHighJoinLatency` alerts firing

**1-hour check:**

- [ ] Join success rate trend is stable (not degrading)
- [ ] No pod restarts since deployment completed
- [ ] WebTransport connection rejection rate steady (no upward trend)
- [ ] JWT validation failure rate steady (no upward trend)
- [ ] Logs show no repeated error patterns related to join flow

**4-hour check:**

- [ ] All join flow alerts clear
- [ ] Join latency trend is stable (no drift toward SLO boundary)
- [ ] No anomalous patterns in join failure reasons
- [ ] Actor mailbox depths remain low (<50) under join traffic load

**Rollback criteria** (trigger immediate rollback if any):

- `mc_session_join_failures_total` rate >5% for 10 minutes
- `mc_session_join_duration_seconds` p95 >1s for 5 minutes (2x SLO)
- `mc_webtransport_connections_total{status="rejected"}` rate doubles vs. pre-deploy baseline
- `MCHighJoinFailureRate` or `MCHighJoinLatency` alert fires and does not resolve within 5 minutes

```bash
# Rollback command -- BOTH instances, as one step. Stopping after mc-0 leaves a
# mixed-version signalling plane: GC keeps assigning meetings to both, so about
# half of new meetings still get the behaviour you are rolling back away from,
# intermittently and per-meeting. See §How to Rollback for the full ordering.
kubectl rollout undo deployment/mc-0 -n dark-tower
kubectl rollout undo deployment/mc-1 -n dark-tower

# Note: Active sessions on old pods will drain gracefully (60s).
# New joins will route to rolled-back pods once they register with GC.
```

### Post-Deploy Monitoring Checklist: MC↔MH Coordination (RegisterMeeting + Notifications)

Use this checklist after any deployment that touches the MC↔MH coordination path: `RegisterMeeting` RPC client (`crates/mc-service/src/grpc/mh_client.rs`), participant connectivity (`crates/mc-service/src/media_routing/connectivity.rs`, held by the meeting actor), or MH→MC notification handling. This is the MC-side companion to the MH-side post-deploy checklist; the canonical full checklist (with all four windows — 30-min, 2-hour, 4-hour, 24-hour — and rollback criteria) lives at:

- `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination"

Open that section first if you are deploying mh-service or both services together. The MC-specific spot-checks below let an MC-only engineer (e.g. deploying only an MC client revision) verify the MC half of coordination without flipping runbooks.

**Quick MC-side gates** — for the canonical PromQL, see `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" → "30-minute check". Do not duplicate the queries here; thresholds and emitter-label conventions are owned in one place to avoid silent divergence:

- `mc_register_meeting_total{status="success"}` rate / total > 95% (canonical query in MH runbook). Emitter-label note: `status="success|error"` (NOT `failure`); see `crates/mc-service/src/observability/metrics.rs::record_register_meeting` and call sites in `crates/mc-service/src/grpc/mh_client.rs::register_meeting` — the inherent `impl MhClient` method (NOT the `MhRegister` trait declaration nor its trait-impl; the three success/error/timeout call sites all live in the inherent impl body).

**MC-only signal** (no MH-side equivalent — counts events arriving at MC, regardless of which MH originated them):

```promql
# MH→MC notifications received (sanity: traffic is flowing).
# This counter has no `status` label — it counts arrivals only.
# For MH→MC delivery success rate, use the MH-side `mh_mc_notifications_total`
# (see canonical checklist in mh-deployment.md).
sum by(event_type) (rate(mc_mh_notifications_received_total[5m]))
```

- [ ] `mc_register_meeting_total{status="success"}` rate / total >95% (run the canonical query)
- [ ] `mc_mh_notifications_received_total` rate non-zero (events arriving means MH is reaching MC). **Since story 2 task 20 this is load-bearing, not a sanity check**: these notifications ARE who-hears-whom. A flat `connected` rate while joins continue is "nobody hears anybody".
- [ ] `mc_mh_notifications_unapplied_total{reason="connection_bound_refused"}` flat (expected-empty); `handler_not_in_set` flat unless an MH pod restarted in the window (then it is expected — `mc-incident-response.md` Scenario 18 arm (a))
- [ ] `mc_mh_notifications_unapplied_total{reason="connection_id_missing"}` flat (non-zero = an MH predating `connection_id` is in the fleet: an ordering violation, its sessions are refused — roll that MH forward)
- [ ] `mc_media_handler_heartbeat_deadline_seconds` equals the ConfigMap's `MC_MH_HEARTBEAT_TIMEOUT_MS` / 1000 on every MC pod (a config echo)
- [ ] `mc_media_handler_planned_transfer_margin_seconds` equals the ConfigMap's `MC_PLANNED_TRANSFER_MARGIN_MS` / 1000 on every MC pod and is below every serving MH pod's `mh_drain_window_seconds` minus one heartbeat interval (env-test 02 `test_planned_transfer_margin_fits_inside_the_mh_drain_window` asserts the inequality and `test_config_reported_by_running_mc_matches_configmap` the echo; story 4 MC-F)
- [ ] `mc_media_handler_client_loss_disagreement_ratio` and `mc_media_handler_client_loss_disagreement_min_clients` equal the ConfigMap's `MC_CLIENT_LOSS_DISAGREEMENT_FRACTION` and `MC_CLIENT_LOSS_DISAGREEMENT_MIN_CLIENTS` on every MC pod (env-test 02 `test_config_reported_by_running_mc_matches_configmap`; story 4 MC-E)
- [ ] `mc_media_handler_replacement_min_interval_seconds` and `mc_media_handler_replacement_backoff_max_seconds` equal the ConfigMap's `MC_MH_REPLACEMENT_MIN_INTERVAL_MS` / 1000 and `MC_MH_REPLACEMENT_BACKOFF_MAX_MS` / 1000 on every MC pod, and `mc_media_handler_replacement_retry_budget_seconds` is present (139 at the shipped defaults)
- [ ] **No mass declaration after the roll (story 4 MC-B2 — the rollback trigger):** `increase(mc_media_handler_declarations_total[30m])` is about 0 outside an MH restart, kill or drain window. A fleet-wide rise right after the roll means some MH is not heartbeating (an MH below MH-2 slipped past the pre-deploy gate): re-check that gate and **roll MC back to MC-B1** (safe; see §Rollback). Do NOT drain the MHs.
- [ ] **Discriminator — MC overload, not handler loss:** declarations rising TOGETHER with `mc_messages_dropped_total{actor_type="meeting"}` mean MC's own meeting mailboxes are full. Heartbeats are then dropped as UNAVAILABLE at the intake (`grpc/media_coordination.rs` `route_heartbeat`), never seen, and the handlers age past N. That is MC's problem: leave the MHs alone and treat MC load (or roll back).
- [ ] `mc_media_meetings_without_handlers` returns to 0 on every MC pod. It may be > 0 transiently after an MH loss (a removal left a meeting with no handler while MC acquires a replacement from GC, story 4 MC-C), and must return to 0 within `mc_media_handler_replacement_retry_budget_seconds`. A value held > 0 past that budget is the awaiting-handler page condition (`MCMediaMeetingsAwaitingHandler`, OBS-ALERTS): no replacement capacity reaches the meeting — check `gc_mh_replacement_requests_total{outcome="no_capacity"}` and the MH fleet's health.
- [ ] **Replacement attempts settle (story 4 MC-C):** outside an MH loss window, `increase(mc_media_handler_migrations_total{outcome!="completed"}[30m])` is about 0. `outcome="abandoned"` or `outcome="rate_limited"` at ANY time means read the attempt-end logs on `mc.actor.meeting` ("Media handler replacement attempt ended", `reason=` `superseded` | `excluded_grant` | `add_refused` | `not_owner` | `invalid_request` | `no_assignment` / `empty_*` | `attempt_bound` | `meeting_ended`).
- [ ] `mc_media_handler_heartbeat_anomalies_total{reason=~"apply_refused_invalid|apply_refused_transport_inconsistent"}` is flat: expected-zero MC contract faults (MH refused MC's policy as malformed). Non-zero = escalate to MC.
- [ ] `mc_register_meeting_total{status="handler_replaced"}` read as expected around MH restarts (a later incarnation at a reused address), never as a coordination failure; `status="error"` is the failure signal. Since story 4 MC-B2 an ACTIVE handler whose MH restarted is declared Dead at N (the old incarnation stops heartbeating) and removed — `mc_media_handler_declarations_total{trigger="heartbeat_timeout"}`. A handler that was still Registering (never confirmed) is never declared Dead; instead a `handler_replaced` reply is a TERMINAL refusal of its first `RegisterMeeting`, so its first snapshot settles without an ack and, since story 4 MC-C, MC removes it and replaces it (ADR-0039 §3). What an operator sees: the INFO "Media handler migration started" with `reason=initial_registration_failed` and `trigger=apply_lag`, the removal's release on `mc_media_handler_removal_releases_total{trigger="apply_lag"}` (and MH-side `mh_media_handler_releases_total{trigger="apply_lag"}` if that MH holds the meeting), then a replacement attempt on `mc_media_handler_migrations_total{trigger="apply_lag"}` — never a declaration. The meeting runs on its other handlers meanwhile; a meeting whose ONLY handler hit this is awaiting a handler until the replacement registers. A non-zero rate on new meetings outside an MH restart window is worth checking.
- [ ] `mc_media_connect_settle_window_seconds` equals the ConfigMap's `MC_MEDIA_CONNECT_SETTLE_MS` / 1000 on every MC pod (a config echo; a mismatch is a stale ConfigMap or a pod that did not roll)
- [ ] `mc_meeting_kek_rotation_window_seconds` equals the ConfigMap's `MC_KEK_ROTATION_DEBOUNCE_SECONDS` on every MC pod. Same config-echo reading as above, but for W a mismatch is a **security** fact: W is the exposure bound on a departed participant, and two pods with different W give meetings different bounds depending on which pod owns them
- [ ] `mc_participant_mh_status_total` **failed-share < 0.20** over 30m (R-60; canonical ratio query in MH runbook 30-min check). This is a *ratio*, NOT a `{state="failed"}` increase == 0 check — the counter increments on any single per-MH client hiccup, so a bare `== 0` false-fails every deploy. Any breach → investigate the client→MH media plane per `mc-incident-response.md` §"Scenario 11: Media Connection Failures".
- [ ] No new `MCMediaConnectionAllFailed` alerts firing (`infra/docker/prometheus/rules/mc-alerts.yaml`)
- [ ] No mc-service pod restarts since deploy completed
- [ ] **Forwarding policy confirmed live** (ADR-0036 §8): `increase(mc_media_policy_pushes_total{outcome="match"}[30m]) > 0`. **If this is zero the gate is NOT green — it is unrun**: no meeting was programmed in the window, which is also what a broken deploy looks like. Run §Smoke Tests → [Test 5: Join Flow](#test-5-join-flow-webtransport--signaling) to drive one, then re-check. **Do not tick this on an absent series.**
- [ ] **No unconfirmed pushes**: `increase(mc_media_policy_pushes_total{outcome!="match"}[30m]) == 0`. Since story 4 MC-B1 a reply from another handler incarnation (an MH restart) is never classified into this outcome set — it is `mc_register_meeting_total{status="handler_replaced"}` — so there is no exclusion; the same expression is held by the alert rule and the metrics catalog entry.
- [ ] Read `mc_media_handler_applied_sequence_lag` (p95) **only after** the gate above has fired, for how far behind handlers are. It is per (meeting, handler) per heartbeat, never the detection signal.

> **Why these two PromQL expressions are inline here, against the section preamble.** That preamble says not to duplicate queries, and the rule is about two divergent copies of one query drifting apart. These two series are **MC-only with no MH-side counterpart**, so there is no second copy and nothing to diverge from — unlike the gates above, they are canonically owned here. Do not move or delete them to satisfy the preamble.
>
> **Scope of a green `match`**: it covers **only meetings programmed in the window**. Since story 2, meetings already live when an MC pod rolled are re-programmed by the new process on their next structural change (join, leave, declaration, mute) — via a generation-floor adoption when the handler still holds the old process's number — but a QUIESCENT meeting is not re-proven until something changes; its handlers' liveness and applied generation arrive on their heartbeats instead (`mc_media_handler_applied_sequence_lag`). Do not read `match > 0` as "forwarding is healthy fleet-wide".
>
> **MC-restart floor adoptions do NOT appear in the "No unconfirmed pushes" gate, by design.** A rolled MC pod's first push to a live handler is answered with the pre-restart generation; MC records that on `mc_media_policy_generation_adoptions_total` INSTEAD of `generation_mismatch`, so the gate stays green on a healthy rollout. Expect `increase(mc_media_policy_generation_adoptions_total{outcome="adopted"}[30m])` to be non-zero after an MC roll with live meetings — **at most one per live (meeting, handler)**, which is the value with that bound. The sibling `{outcome="superseded"}` counts restart-floor replies discarded for a newer render (routine churn) and has NO such bound, so do not read the unlabelled total as a meeting count. A FAILED adoption is recorded as `generation_mismatch` and fails the gate, correctly. See the metrics catalog entry for why this is a sibling counter and not a sixth `outcome` value.
- [ ] Cross-check the MH-side checklist (link above) for the full set of MH-side checks (handshake, JWT, timeout, MH→MC delivery success rate, active connections)

**Story 3 video: the 24-hour watch list.** The list, in rise order, and every query live in one
place: [`mh-deployment.md` §Story 3 video: the 24-hour watch list](mh-deployment.md#story-3-video-the-24-hour-watch-list-in-the-order-things-rise-first).
They are not duplicated here. The MC-side items to own at the 24-hour mark:

- [ ] Item 1: `mc_media_policy_pushes_total{outcome="transport_mode_mismatch"}` is zero. If not, do NOT
  roll MC back alone ([Mixed-Transport Rollback Hazard](mh-deployment.md#mixed-transport-rollback-hazard)).
- [ ] Items 4 and 6: the thresholds are scaled by `mc_media_video_keyframe_min_interval_seconds`
  (`MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`), so confirm the gauge reads the configured value on both MC pods.
- [ ] Item 8 (MC half): a server mute shows `mc_media_send_directive_withdrawals_total{reason="server_muted"}`
  rising for a watched kind ([MC Scenario 21](mc-incident-response.md#scenario-21-server-mute-not-enforced) Step 1b).
- [ ] Item 9: the MC policy-push rate under mute churn stays bounded (R-17).
- [ ] Items 10 to 12: the client video series. These are client-reported and anchor in
  `mc-incident-response.md` Scenarios 22 and 23. Check item 12 (a client video series is present at
  all) before trusting items 10 and 11: a dead client pipe leaves those alerts green.

**Rollback (MC half)** *(within the v3 era only; across the story-3 cutover nothing rolls back alone — [§Story 3 cutover](#story-3-cutover-see-each-other-the-mc-view))*: same as the join-flow rollback above — `kubectl rollout undo deployment/mc-0 -n dark-tower` and `kubectl rollout undo deployment/mc-1 -n dark-tower` (MC ships as two per-ordinal Deployments; there is no Deployment named `mc-service` — that string is a Service, a PDB and a container name. Form per `docs/runbooks/mh-deployment.md`). **Carve-out — do NOT roll MC back for sustained `no_applied_generation` (and, from story 3, for `transport_mode_mismatch`: [Mixed-Transport Rollback Hazard](mh-deployment.md#mixed-transport-rollback-hazard)).** If `mc_media_policy_pushes_total{outcome="no_applied_generation"}` is climbing with retries exhausted after a deploy, MH is below the ADR-0036 §8 contract and **the remedy is to roll MH forward, not MC back**: rolling MC back returns it to `policy_generation: 0` registrations, which MH installs nothing for — the pre-change media blackhole, not a fix. If the issue is on the MH side (handshake, JWT, RegisterMeeting timeouts), follow the rollback criteria + `mh-service` rollback documented in `docs/runbooks/mh-deployment.md` §"Post-Deploy Monitoring Checklist: MH WebTransport + MC↔MH Coordination" → "Rollback criteria".

**Carve-out #2 — do NOT roll MC back on a media-session decline spike.** Since
`NotifyParticipantConnectedResponse` gained `sender_id`, MC and MH are a two-sided
contract. An MC binary predating that field never sets it, and because the field is a
bare `uint32` (not `optional`) a still-new MH decodes the absence as `0` — the reject
value — and **declines every media session**. So `kubectl rollout undo deployment/mc-0`
after this contract has landed is a **total media blackout**, not a mitigation.

If `mh_media_session_starts_total{outcome="declined_no_sender_binding"}` is climbing,
**the remedy is to roll MH back (or MC forward), never MC back.** Deploy order is forced:
**MC forward first, MH back first.** Full triage at
`docs/runbooks/mh-incident-response.md` §"Scenario 15: Media Sessions Declining — No
Sender Binding"; ordering at `docs/runbooks/mh-deployment.md` §"Cross-service ordering:
MC and MH are coupled by the sender_id binding contract".

**This is the same mechanism as the `no_applied_generation` carve-out above**, one
contract later: in both cases rolling MC back returns it to a state MH cannot consume,
and in both cases the intuitive action is the destructive one. Recorded separately rather
than left to be inferred from the first.

**Carve-out #3 — rolling MC back ACROSS story 2's generation-floor adoption darkens every live
meeting, and here the remedy IS to restart MH.** Story 2 (task 6) made MC adopt a live handler's
applied generation as a floor after an MC restart (`PolicyGenerations::adopt_floor`) and re-push on
every structural change. An MC build predating that has neither: it restarts with an empty
generation registry and pushes once, at generation 1, when a meeting's first participant joins.
MH's routing table is install-only and still holds the pre-rollback generation K, so it rejects 1
as stale — forever, because the old build never climbs. **Every meeting alive across the rollback
stays media-dark** with no recovery path in the rolled-back code.

- **Blast radius is bounded**: only meetings that were alive on MH across the rollback. A NEW
  `meeting_id` has no MH entry (installed generation 0), so generation 1 applies normally — new
  meetings are unaffected.
- **Remedy — the ONE exception to "DO NOT RESTART MH"**: restart the MH pods (clearing the
  in-memory, install-only routing tables) or roll MC forward again. Every other divergence path in
  these runbooks says in bold not to restart MH, because there it sheds sessions and recovers
  nothing; here the stale installed generation IS the fault and only an MH restart (or an MC build
  that adopts) removes it. Participants in the affected meetings then rejoin (reload the page).
- **How it presents**: `mc_media_policy_pushes_total{outcome="generation_mismatch"}` climbing after
  the rollback with `applied > sent` in the ERROR line, and `MCMediaGenerationDivergence` paging;
  `mc_media_policy_generation_adoptions_total` absent (the old build does not have it).

Same shape as carve-outs #1 and #2: the rolled-back MC returns to a state MH cannot consume. The
difference is the remedy, which is why it is written down rather than inferred.

**Carve-out #4 — rolling MC back ACROSS story 2 task 20 re-introduces the handler split.** A
pre-task-20 MC places each participant on ONE handler round-robin and offers only that handler, so
in any meeting whose handler set has two handlers a TWO-person meeting hears nothing. Rollback is
therefore not a neutral safety action here. It is tolerated by the new SDK (one url offered, one
target). Roll back only for a failure worse than that split; prefer rolling forward. After a
rollback, participants in live meetings rejoin (reload) to be re-offered the old model's single
handler.

---

## Emergency Contacts

**On-Call Rotation:** See PagerDuty schedule

**Escalation:**
1. **L1:** On-call SRE (PagerDuty)
2. **L2:** MC service owner / Backend team lead
3. **L3:** Infrastructure architect

**Related Teams:**
- **GC Team:** For registration and heartbeat issues
- **Infrastructure Team:** For Kubernetes, network, resource issues
- **Media Handler Team:** For MC → MH connectivity issues

---

## References

- **ADR-0010:** Global Controller Architecture (MC registration)
- **ADR-0011:** Observability Framework
- **ADR-0012:** Infrastructure Architecture
- **Source Code:** `crates/mc-service/`
- **Kubernetes Manifests (base):** `infra/services/mc-service/`
- **Kubernetes Manifests (Kind overlay):** `infra/kubernetes/overlays/kind/services/mc-service/`
- **Shared media codec profile:** `infra/services/media-codec-profile/` (story 3)
- **Story 3 cutover:** `docs/user-stories/2026-10-07-see-each-other.md` (R-23, R-25, R-26); deploy-order authority `docs/runbooks/mh-deployment.md` §Story 3 cutover

---

**Last Reviewed:** 2026-03-31
**Next Review:** 2026-04-30
