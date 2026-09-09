# Devloop Output: Media dashboards, metric catalog sections, method-label collapse, telemetry-hygiene env-test, coverage-gap debt

**Date**: 2026-09-09
**Task**: ADR-0036 loopback media path — Grafana dashboards (`mh-media.json`, `client-media.json`), media sections in the MH/MC/client metric catalogs, `mh_grpc_requests_total` `method`-label collapse, Rust telemetry-hygiene env-test, and two `docs/TODO.md` debt notes.
**Specialist**: observability
**Mode**: Agent Teams (v2) — full, HEADLESS (run-story task #22)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~95m (setup 08:32 → commit 10:07, 2026-09-09)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `80945b22a84d1063654937ab05bd2b9886a63071` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `complete` |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-FIXED` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `confirmed` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `confirmed` |

---

## Task Overview

### Objective

See the story manifest entry for task 22 in `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` (verbatim prompt). In brief: make every media-path metric defined by this story visible (panel) and documented (catalog entry), freeze the media label scheme with the catalogs as its only home, retire stale `method` label values, prove hygiene on a live cluster, and record the two known static-coverage gaps as debt.

### Scope
- **Service(s)**: MH, MC, client SDK (docs + dashboards); env-tests
- **Schema**: No
- **Cross-cutting**: Yes (MH `metrics.rs` doc comments, `mh-overview.json`, env-tests)

### Debate Decision
NOT NEEDED — the design decisions (label scheme, phase split, §11 prohibitions) are already ratified in ADR-0036 §11 and ADR-0011; this task records and renders them.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. **Every** file the plan touches has a row, including
verification-only rows carrying an expected **NO EDIT** — they are listed so a
reviewer can see the edit was considered and declined, not overlooked.

**Ownership basis, stated because two ADRs disagree and @code-reviewer is right
to press on it.** ADR-0011 §Documentation Ownership gives `infra/grafana/dashboards/`
to Observability and the metric catalogs to "Observability + Service specialists".
ADR-0031 (later, Accepted) refines *per-service* artifacts to the service
specialist, naming `infra/grafana/dashboards/<svc>-overview.json`,
`crates/<svc>/src/observability/metrics.rs`, `docs/observability/metrics/<svc>.md`
and `infra/docker/prometheus/rules/<svc>-alerts.yaml` — and leaves
`errors-overview.json`, fleet-level SLO views, and the **conventions set** with
Observability. I resolve it the conservative way: **anything ADR-0031 names for a
service specialist is classified cross-boundary with that specialist as Owner,
even where the filename is not literally `<svc>-overview.json`.** Two consequences
I am flagging rather than absorbing:

- Neither `media-handler` nor `client` is on this review team, so the
  Domain-judgment rows into their domains need an owner ACK or a Paired flag.
  The story manifest allocates task 22 to `observability` (ADR-0035 makes the
  manifest the allocation authority), which I read as the story-level delegation —
  but that is a **team-lead ruling to make explicit**, not mine to assume.
- For the alert rule files I follow ADR-0011 + this task's own wording
  (`operations` owns them), which is also what my navigation warns three reviewers
  previously got wrong in both directions. I edit none of them.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/grafana/dashboards/mh-media.json` (new) | **Not mine** / Domain-judgment | `media-handler` (ADR-0031 per-service dashboard). Owner-ACKed. |
| `infra/grafana/dashboards/client-media.json` (new) | **Not mine** / Domain-judgment | `client` (ADR-0031 per-service dashboard). Owner-ACKed. |
| `infra/grafana/dashboards/mh-overview.json` | **Not mine** / Minor-judgment | `media-handler`. Panel 36 narrowed to `sum by(direction) (increase(...))` per @observability's ADR-0029 ruling; panel 35 description-only (positional → title reference). Authorised by @main ruling 1; owner-ACKed. The `:1769` `method` statement is untouched — already correct. |
| `infra/grafana/dashboards/mh-slos.json` | **Not mine** / Minor-judgment — **description-only** | `media-handler`, who requested this hunk as owner. The query narrowing was **dropped in full** (premise false — O-1; and G1-1 requires all four phases). Panel 7's query is byte-identical. |
| `crates/mh-service/src/observability/metrics.rs` | **Not mine** / **Minor-judgment** (upgraded at Gate 1) — one doc-comment hunk, owner-specified verbatim | `media-handler`. The `method` collapse remains **verify-and-report, no edit** (task 13 landed it at `:152-173`). The `direction()` doc fix is the only hunk. **Not Mechanical**: the sed-test fails (enumeration → rule changes what the comment asserts) and no guard reads doc prose — which is why the `11 → 13` rot survived. Owner-origination answers a tier; it does not determine one. |
| `docs/observability/metrics/mh-service.md` | **Not mine** / Domain-judgment | `media-handler` (ADR-0031) — co-owned with me under ADR-0011. Owner-ACKed. |
| `docs/observability/metrics/mc-service.md` | **Not mine** / Domain-judgment | `meeting-controller` (ADR-0031) — co-owned with me under ADR-0011. Owner-ACKed. |
| `docs/observability/metrics/client.md` | **Not mine** / Domain-judgment | `client` (ADR-0031) — co-owned with me under ADR-0011. Owner-ACKed. |
| `docs/observability/slos.md` | Mine / Domain-judgment | — Added at Gate 1 (G1-1): a second open item under §Open: the target. |
| `docs/observability/label-taxonomy.md` | Co-owned / Minor-judgment | observability + security. Two shared-label rows for `media_kind` / `content_kind`, **requested by @security at Gate 1**. Pointer-shaped; no restatement of `direction` or §Key custody. |
| `docs/observability/dashboards.md` | Mine / Minor-judgment | — (ADR-0031 "conventions set" row.) Sections + ownership rows for both new boards, plus the §Kubernetes rewrite (@infrastructure I-2). |
| `infra/grafana/kustomization.yaml` | **Not mine** / **Minor-judgment** (upgraded at Gate 1 per @code-reviewer) | `infrastructure`. Two changes of different character in one file: appending `mh-media.json` to the existing `grafana-dashboards-mh` group is a Mechanical list-append R-20 forces on basename; creating the `grafana-dashboards-client` group is **not** — its `options.labels` block is an uncovered file-type × change-pattern, which ADR-0024 §6.2 defaults to Minor-judgment, and omitting it passes CI green while the dashboard never mounts. **The file's tier is the higher of the two.** Owner-ACKed; @operations verifies the labels block directly at Gate 2. |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` | Mine / Minor-judgment | — Hoist of `fetch_all_series` + new pure `response_to_series` with FIRE fixtures (@dry-reviewer D-1). |
| `crates/env-tests/tests/30_observability.rs` | Mine / Domain-judgment | — Paired with `test`. |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` | Mine / **Minor-judgment** (upgraded at Gate 1 per @dry-reviewer C-3b) | — Re-point to the hoisted helper **and split its doc comment**; deciding which half is call-site reasoning is a judgment call, not a structure-preserving edit. No assertion change. |

### Considered and declined — NO EDIT

Deliberately **not** a table row each: `dt-guard cross-boundary-scope` reads the table
above as the set of paths this diff touches, so listing an untouched path there would red
`scope_drift_planned_untouched`. They are recorded here so the decision is visible as
*considered and declined* rather than overlooked.

- **`crates/mh-service/src/observability/metrics.rs` — the `method`-collapse block**
  (`media-handler`). Task 13 already landed it, with the reasoning the prompt asks for.
  Verified at `:152-173`; no edit. (The file *is* in the table for a different, unrelated
  hunk.)
- **`infra/grafana/dashboards/mh-overview.json:1769`** — the `method` roster in the panel
  description is already correct. Untouched. (Same: file is in the table for panel 35/36.)
- **`crates/env-tests/Cargo.toml`** — no new dependency. The `key_custody` literals are
  restated deliberately rather than imported from `common` (@dry-reviewer D-2).
- **`packages/sdk-core/src/media/**`** (`client`) — read-only verification. The task asks
  me to *cite* `events.ts`'s label threading, not change it.
- **`infra/docker/prometheus/rules/*-alerts.yaml` and `docs/observability/alerts.md`**
  (`operations`) — inspected and reported; not edited. `MCMediaMissingKeyMaterial`'s
  expression is confirmed correct by inspection, and `alert_rules.rs` pins each `expr`
  byte-for-byte against its `alerts.md` entry, so even a reformat would red.

No path here is an ADR-0024 §6.4 Guarded Shared Area: none appears in
`scripts/guards/simple/cross-boundary-ownership.yaml` and none meets a §6.4
criterion. No `Mechanical` row sits on a GSA path.

---

## Planning

### Mechanism restatement (wider class than the task names)

The task is written in instance-language: *these* metrics need *these* panels and
*these* catalog rows, and *this* `method` label roster is stale. Restated as a
mechanism:

> **Every restatement of a metric's label scheme outside its catalog is an
> unguarded copy.** `dt-guard application-metrics` / `dashboard-panels` match
> metric *names* only (`metric_no_dashboard`, `metric_not_in_catalog`,
> `dashboard_metric_missing`), and `alert_rules.rs` pins an alert `expr`
> byte-for-byte against its `alerts.md` entry. **Nothing in the tree compares a
> label key or a label value to anything.** So a label roster written anywhere
> other than the catalog decays silently, and the decay is invisible in CI.

That class is wider than the three sites the task enumerates. Same-owner siblings
already in tree, found while grounding:

1. `infra/grafana/dashboards/mh-overview.json:1769` — a panel description
   restating the `method` roster in prose. It is *correct today*, which is exactly
   why it is the archetype: it was correct when written, too.
2. `docs/observability/metrics/mh-service.md:591` — a `reason`/`direction` triage
   table restating MH's 13-token `MediaDropReason` vocabulary.
3. `docs/runbooks/mh-incident-response.md` §8 rows keyed to env-test triage
   strings (operations-owned).
4. The env-test literals I am about to write (`key_custody`, `operator`, the
   reason tokens).

I am **not** widening scope to fix all four — (3) is operations', and (1)/(2) are
accurate today. What I do instead is the cheap structural half: the two NEW
dashboards' panel descriptions **cite** the catalog section rather than restate the
roster, so this diff adds **zero** new copies of the frozen scheme. That is the
only decision available to me that scales; enumerate-and-fix does not.

Surfaced because if reviewers want the general remedy (a label-value-aware guard
mode) it is a task, not a clause of this one — better declined explicitly than
absorbed silently.

### What is already done at HEAD (verified, not assumed)

Three clauses of the task were satisfied by prior tasks. Reporting rather than
re-landing:

- **`method` collapse — DONE at all three named sites.** `metrics.rs:152-173`
  (`GRPC_METHOD_REGISTER_MEETING` const, collapsed doc comment, and
  `record_grpc_request` no longer taking the parameter), `mh-overview.json:1769`,
  and `mh-service.md:220-225` all carry the single-value statement *and* the
  "single-valued by design / gains fields rather than sibling RPCs" reasoning. A
  `grep` for `route_media|stream_telemetry` across `docs/` + `infra/` returns only
  those deliberate "retired and can never appear" sentences plus the story
  manifest. **Action: verify, report, no edit.**
- **MH + MC media metrics already have panels and catalog rows.**
  `dt-guard application-metrics --root /work` is **green at HEAD**
  (`STATUS=OK REASON=application-metrics-clean`). So `metric_no_dashboard` /
  `metric_no_catalog` force nothing here; the two new dashboards are additive, and
  my Layer-3 risk is the *reverse* direction (`dashboard_metric_missing`) from a
  typo in a new panel.
- **client.md's not-yet-*emitted* caveat is already retired** (task 19; recorded as
  retired at `client.md:14-21`).

### Decision 1 — `mh-media.json` earns its place by narrowing its neighbours

MH media metrics are today spread across `mh-overview.json` (panels 32, 35, 36,
37, 38) and `mh-slos.json` (panels 7, 8, 9). A third dashboard that re-plots the
same series is a duplication finding waiting to happen. So every panel in
`mh-media.json` answers a question no existing panel answers, and I make one
narrowing edit so no question has two homes:

- **`mh-slos.json` panel 7 narrows to `phase="total"`.** Today it is
  `sum by(le, phase)` across all four phases with the 30 ms objective threshold
  drawn across all of them — three of which the objective is not about. After the
  narrowing, the SLO panel plots the one series the ADR-0011 objective is
  *defined* on, and the three-phase triage view lives in `mh-media.json`. This is
  exactly the task's "`total` exists only to serve the objective, because
  quantiles do not sum".
- **`mh-media.json`'s latency panel renders the three component phases only**
  (`phase=~"receive_buffer|processing|transmit_buffer"`), never an aggregate.
  @operations' point 4 is adopted: `MediaLatencyPhase::ALL` has **four** members
  and `total` is the trap — it is excluded from this panel by selector, and the
  description says where it lives and why, so an operator cannot read the total
  first and lose the which-of-three-remedies signal.
  The description carries the phase-split rationale: a single total series is the
  cheapest thing to build and would foreclose the story-8 phase-triage runbook
  scenario, because §11 decomposes into three phases *that have different
  remedies* and an undifferentiated total does not tell an operator which to
  pursue; the story-8 latency alert gets its own scenario and is never folded into
  egress exhaustion.
- I do **not** re-plot the sample-ratio or egress-queue-depth gauges — already
  homed (`mh-slos` 8/9, `mh-overview` 37). `mh-media.json` links to them.
- **No egress budget / capacity / ceiling / admission-threshold panel or prose.**
  Those do not exist in this story (egress chain is story 2).
  `mh_media_egress_queue_depth` is a *depth*, not a budget, and is already
  panelled — I add none.

Planned `mh-media.json` panels:

| # | Title | Query shape |
|---|-------|-------------|
| 1 | Media Forward Latency by Phase (p95) — receive-buffer / processing / transmit-buffer | `histogram_quantile(0.95, sum by(le, phase) (rate(mh_media_forward_latency_seconds_bucket{phase=~"receive_buffer\|processing\|transmit_buffer"}[$__rate_interval])))` |
| 2 | Media Forward Latency by Phase (p99) | same, `0.99` |
| 3 | Ingress Drops by Reason | `sum by(reason) (increase(mh_media_frames_dropped_total{direction="ingress"}[$__rate_interval]))` — see the ADR-0029 ruling below |
| 4 | Egress Drops by Reason | `sum by(reason) (increase(mh_media_frames_dropped_total{direction="egress"}[$__rate_interval]))` |
| 5 | Zero-Forever Invariant Drops (stat, `noValue: 0`) | `sum(increase(mh_media_frames_dropped_total{reason=~"transport_send_refused\|relay_rewrite_failed"}[$__range]))` |
| 7 | Egress Delivery Ratio | the ratio already documented at `mh-service.md:526-529`, cited in the description as its source |

Panels 3, 4 and 5 carry the empty-panel description the task requires: a Prometheus
counter materialises no series until first incremented, so an empty drop panel
means **no drops occurred and that is the expected healthy loopback state** — not a
broken pipe. Stat panel 5 gets `noValue: 0`. **No `or vector(0)` anywhere**: it
fabricates a series and flattens the by-reason breakdown operations' triage
depends on.

**@observability's and @operations' correction is adopted: the description names
BOTH declared-but-unreachable-until-video tokens, not one.**
`MediaDropReason::PartialFrameDiscard` ("UNREACHABLE UNTIL VIDEO",
`mh-service.md:595`) *and* `MediaDropReason::StreamRateLimited` ("UNREACHABLE UNTIL
UNI-STREAM/VIDEO … in the same sense as PartialFrameDiscard and for the same
reason", `mh-service.md:606`). The task prompt says "the one reason"; the code
declares two, and naming one advertises the other as a live detector — an operator
triaging a permanently-flat series would go hunting a nonexistent bug.

### Decision 2 — `client-media.json`, and the two different kinds of empty

Panels for all fourteen client media metrics — the thirteen `dt_client_media_*`
plus `dt_client_time_to_first_media_frame_ms` — organised around the receive-path
accounting identity `received = accepted + sum(drops by reason)` that
`client.md:254` already ratifies.

Not guard-forced (`SERVICE_METRIC_PREFIX_RE` never extracts a `dt_client_*` name)
and **not dropped**: per ADR-0036 §11 the client drop counters are the *only*
signal for a join or rotation path that has silently stopped delivering keys.

**@observability point (b) is the design constraint here.** Every panel renders
"No data" today for a reason that has nothing to do with healthy-empty drop
panels: the OTLP collector's metrics pipeline is `receivers: [otlp] /
exporters: [debug]` and no Prometheus job scrapes it (re-verified at HEAD). So the
dashboard gets a **row-0 text panel** stating that flatly, and the drop panels'
descriptions explicitly contrast the two empty states:

> Empty here does **not** yet mean "no drops occurred". Today it means the export
> hop is unwired (see the banner). When it lands, empty means no drops — the
> healthy state.

Without that contrast, the honest healthy-empty wording I am required to write
becomes an actively wrong reading of an unwired dashboard. Per @operations, no
panel prose will assert the series exists in Prometheus today, and the
OTLP→Prometheus `_total`-suffix normalisation residual is inherited and named,
not re-litigated.

### Decision 3 — client.md's queryability block is SHARPENED, not deleted

The task says to "retire `client.md`'s current warning that no `dt_client_*` series
exists in any backend, which this story makes obsolete for the media metrics."

**I am declining the delete. This is a superseded-instruction record, not a
deferral.** The premise is false at HEAD, verified three ways:
`infra/services/otel-collector/configmap.yaml` still has
`metrics: receivers: [otlp] / exporters: [debug]`; no Prometheus job scrapes the
collector; and `docs/TODO.md:778` records the statement as carried at five sites
with "**Do not remove any of them without wiring the exporter.**" Nothing in tasks
13, 16, 19 or 21 wired an exporter. Deleting a true warning because a prompt
predicted it would be false is precisely the masked failure the working
conventions forbid — and it would leave `MCMediaMissingKeyMaterial` reading as
covered when it is structurally incapable of firing.

What the prompt is reaching for — the older *declared-but-not-yet-emitted* caveat —
was already retired by task 19 and is recorded as retired at `client.md:14-21`.
Emitted-and-exported is not queryable; the prompt conflates the two.

Planned edit: keep the block, **sharpen** it so a reader cannot infer "not
queryable" means "not implemented" — state that the media metrics are emitted,
exported, catalogued and now dashboarded, and that the exporter is the sole
remaining hop, pointing at `client-media.json` as the artifact waiting on it. The
block's own delete-condition ("when the exporter lands, not before") is preserved
verbatim.

### Decision 4 — catalog media sections (the frozen-scheme home)

A `## Media-path label conventions` section in each of `mh-service.md`,
`mc-service.md`, `client.md`, stating once — in the catalog and nowhere else:

> **REVISED at Gate 1 after @security's Concerns A and B. I was wrong, and the
> correction is structural, not cosmetic — see Decision 9.** The split is:
> **shared label name + bounded value set + why it is bounded → `label-taxonomy.md`;
> which labels a given metric carries → the catalog.** `key_custody` and
> `direction` already demonstrate exactly that split in tree. My original plan
> would have put a bounded value set in three catalogs at once, which is the drift
> surface I was refusing to create elsewhere in the same plan.

- **Declared, not yet carried**: `media_kind` (`audio` | `video`) and
  `content_kind` (`main` | `slides`) arrive on media-path counters when video and
  content share land (story 3). **The bounded value sets live in
  `label-taxonomy.md` (one home, added now — see Decision 9); the catalogs point
  at it** and say only which metrics will carry them. Framed as declared, never
  as live dimensions (@security 4).
- **Transmit/receive convention: the catalogs POINT, they do not restate.**
  `label-taxonomy.md` §Permitted partner: `direction` (`:362-386`) already homes
  the whole convention — pipeline-relative `ingress`/`egress`, the
  participant-relative `uplink`/`downlink` reading **barred** with its structural
  reason (only the pipeline-relative reading is incapable of growing a third value
  that individuates a participant), why it is admitted at all (the relay is
  keyless, so no partner label can make its counters a key-state oracle), and
  rule 3's one-direction-per-token point. A catalog restatement that dropped
  either the barred-`uplink` clause or the does-not-generalise clause would be a
  **partial copy that reads as complete** — which is precisely how the client
  counter would grow a `direction` label from a catalog that looked authoritative.
- **What stays in the catalog** is metric-specific arithmetic, not convention:
  `mh-service.md` keeps "each `MediaDropReason` token maps to exactly one
  direction, which is why the counter carries 13 MH-local series rather than 26",
  because that is a fact about one metric.
- **Corollary, adopted before it became a review finding**: `client-media.json`
  gets **no `by (reason, direction)` panel**, and `client.md` must not present
  `direction` as available on `dt_client_media_frames_dropped_total`. The taxonomy
  names that exact metric as outside the acceptance — the client sits on the very
  receiver-key-cache state the oracle argument is about, so `reason × direction`
  there is a different question with a different answer.
- **Which labels each metric carries is frozen with the catalog as its only home.**
  Any change to a *metric's* label set routes through the catalog; any change to a
  *shared label's* name or value set routes through the taxonomy. Never
  peer-to-peer between an emitter and a dashboard.

For `client.md`, **adopting @observability's correction**: the task prompt says the
client media metrics carry "only `client_version` and `org_id`". The emitting code
(`mediaMetricLabels`, `packages/sdk-core/src/media/setup/mediaMetrics.ts`) returns
**three** — `client_version`, `org_id`, `key_custody`. "Only two" would be false
against the wire and contradict R-26. The honest split, which is what I write:

> **Identity dimensions: exactly two** — `client_version`, `org_id`. Nothing else,
> and specifically **no meeting, participant or stream dimension, hashed or
> otherwise** (ADR-0036 §11 R1). Plus one **fixed non-identity** label,
> `key_custody=operator` (§4), which is not a dimension in the cardinality sense:
> it has exactly one value by construction.

with §11's flat prohibition cited and its reasoning restated — a hashed meeting id
has **identical cardinality and identical per-meeting aggregation** to a raw one,
so hashing buys nothing the rule is about — and the concrete inertia risk named:
`packages/sdk-core/src/media/events.ts` already threads the join-flow implicit
label set into the media module, so `meeting_id_hash` would attach to every media
metric by default. The media set is built by **allow-list**, never by spreading and
pruning the join set. `meeting_id_hash` stays grandfathered for the ADR-0028
join-flow set *as a closed set*, which nothing joins.

### Decision 5 — dashboard registration (and the failure the guard misses)

Both dashboards go into `infra/grafana/kustomization.yaml`: `mh-media.json` into the
existing `grafana-dashboards-mh` generator; `client-media.json` into a **new**
`grafana-dashboards-client` generator.

**@operations point 3 adopted and explicitly planned**: `dt-guard kustomize` R-20
(`dashboard_orphan`) is bidirectional on **basenames only** and does not check
`options.labels`. The Grafana sidecar (`kiwigrid/k8s-sidecar`,
`LABEL=grafana_dashboard`) mounts only ConfigMaps carrying `grafana_dashboard: "1"`.
A new group that lists the file but omits

```yaml
    options:
      labels:
        grafana_dashboard: "1"
```

passes Layer 3 green, lands in the repo, and never appears in Grafana. The new
`grafana-dashboards-client` group **will** carry it, matching all five existing
groups.

`dashboards.md` gets sections and ownership-table rows for both. Host-side action
to be recorded separately in the devloop record, not done by me:
`kubectl apply -k infra/kubernetes/overlays/kind/observability/` plus sidecar
pickup, since `deploy_observability()` only runs during full setup.

### Decision 6 — env-test scope: two net-new assertions, zero re-assertions

`crates/env-tests/tests/32_media_metric_hygiene.rs` (task 23) already owns "**no
meeting-id label on any media metric**" — in fact over all four Rust jobs, which is
*stronger* than the media-only form my brief asks for, with a presence anchor and a
one-fetch-both-assertions non-vacuity structure. **I will not re-assert it**, and I
will not add a weaker second copy. `30_observability.rs` gets exactly the two
things 32 does not cover:

- **(a) `key_custody=operator` presence** on MH media-path series.
- **(c) drop-by-reason counters exist with the expected labels.**

@test's question 3 is the sharp one and the answer is favourable:
`resolve_media_handles()` (`metrics.rs:1170`) resolves the **full label
cross-product** through the `counter!` macro — which *returns* a handle, hence
registers — for all 13 `MediaDropReason` tokens plus every `ALL_REJECT_REASONS`
codec token, and it is called unconditionally at MH startup (`main.rs:367`). So on
any running MH pod with no media flowing, every
`mh_media_frames_dropped_total{reason,direction,key_custody}` series exists **at
zero**. Presence is a legitimate gate, not a flake — the same property
`32_media_metric_hygiene.rs`'s module header already relies on for its anchor.

MC's media metrics are lazily created, so — matching 32's deliberate asymmetry — MC
is **recorded** (`eprintln!`), never gated. An MC-shaped presence gate would red on
every idle run and be muted within weeks, which is how a control dies.

Structure, following 32's discipline (@test 2/4, @security 6):

- **One fetch**, feeding every assertion, so the anchor *guarantees* the predicates
  had input.
- **Anchor before for-all**: `mh_media_frames_forwarded_total` must be in the
  fetched set, asserted first.
- **Separate non-vacuity assertion** that the fetch returned series at all.
- **Distinct triage tokens** so an empty result cannot be triaged as a policy
  violation and "fixed" by relaxing the predicate — reusing 32's
  `Triage MH scrape/metric-registration` and adding
  `Triage media label-presence violation`.

Positive control for each gated assertion: the anchor series itself carries
`key_custody=operator`, so if the label were dropped fleet-wide the anchor lands in
the *failing* set rather than going absent — the assertion cannot pass by
inspecting nothing.

Reason-token expectation: assert that the fetched set **contains all 13 MH-local
`MediaDropReason` tokens**, each with a `direction` in `{ingress, egress}` and
`key_custody=operator` — not set *equality*, because the codec family legitimately
adds 16 more from `ALL_REJECT_REASONS` and equality would red on a codec token
added upstream.

### Decision 7 — duplication answers (@dry-reviewer 1, 2, 3)

1. **`key_custody` literals: option (b) — restated deliberately, with the comment.**
   `crates/env-tests/` links zero service crates by design, and more decisively an
   env-test's subject is the **deployed artifact**: a test written against the same
   constant as the emitter passes whatever the constant becomes. That is the
   `proto-gen/tests/internal_roundtrip.rs` `65_535` precedent. `common`'s
   `labels.rs` already carries `#[test] key_custody_literals_are_stable` pinning
   the spelling at its home, so a rename reds there and here, from two directions.
   The comment says all of this so it cannot read as an accidental third home.
   **No new `Cargo.toml` dependency.**
2. **`fetch_all_series` is hoisted, not copied** — into
   `crates/env-tests/src/fixtures/metric_hygiene.rs`, which already owns `Series`,
   `check_series` and `SERVICE_JOBS`. `32_media_metric_hygiene.rs` re-points at it
   (mechanical, no assertion change). Its doc comment's argument — that fetching
   ONCE is *structural*, being what makes the anchor guarantee the predicate had
   input — travels with it, because that reasoning is the reason the function
   exists.
3. **No fourth job-set home.** The extension lands in `30_observability.rs`, which
   already has `EXPECTED_SERVICES`; the media selector derives its job list from
   that rather than declaring a new one.
4. **New panel descriptions cite the catalog, never restate the roster** — see the
   mechanism restatement.
5. **No panel mirrors an alert `expr` verbatim.** `mh-media.json` panel 5's
   invariant stat is deliberately `increase(...[$__range])` over the dashboard
   range, not the `rate(...)`-over-`5m` shape an alert uses.

### Decision 8 — the two `docs/TODO.md` debt notes

**(i) `dt_client_*` has zero static guard coverage.** §Observability Debt, with the
wrong-fix trap named: `SERVICE_METRIC_PREFIX_RE` builds its alternation from
`CANONICAL_SERVICES` (`crates/dt-guard/src/common/services.rs`), giving
`\b((?:ac|gc|mc|mh)_[a-z][a-z0-9_]*)`. Underscore is a word character, so
`dt_client_media_frames_dropped_total` is never extracted and a typo in
`MCMediaMissingKeyMaterial`'s expression passes CI green while the alert never
fires. **The wrong fix**: adding `dt_client` to the alternation. It is the same
word-boundary property ADR-0036 §11 describes for the deny-guard vocabulary, but
§11's segment-splitting remedy **does not transfer**.

@code-reviewer's point 3 is adopted: my first statement of the trap named the
*second* failure, not the first, and the note must be accurate against the source
or it wastes the time of the person it is written for. The alternation is derived
from `CANONICAL_SERVICES` at `Lazy::new` (`crates/dt-guard/src/common/services.rs`: `CANONICAL_SERVICES` at `:45`, `SERVICE_METRIC_PREFIX_RE`'s `Lazy::new` at `:65`),
so "add `dt_client` to the alternation" means **editing `CANONICAL_SERVICES`** — and
that array is a **guarded release-premise mirror, not a free-to-edit metrics list**
(its own doc comment at `:41-44` says so, and `release_build_profile`'s
`canonical_services_roster_drift` rule derives from it). A `dt_client` entry is not
a `crates/*-service` workspace member, so **`canonical_services_roster_drift` fires
first**; `alert_metric_missing` is what they would hit only after defeating that.
The note states both, in that order. The real fix is a TypeScript-aware metric source for the guard:
task-sized, out of scope. This is why operations' "does it apply" step (the counter
arriving in Prometheus from a real browser) is load-bearing rather than
belt-and-braces.

**Sweep-completeness — folded into note (i) as a SIBLING CLAUSE, not a third entry.**
@observability asked for a standalone `docs/TODO.md` line; @media-handler corrected the
shape and both agree the correction is right, so this is a clause of the existing note
rather than a new one. The reason is the discipline this whole task is about: note (i)
already records that **nothing in the tree compares a label key or value to the artifact
it restates**, and sweep-completeness is the *general form* of that same gap — a second
entry would be a second home for a theme that already has one. **This also removes the
scope deviation**: still two debt notes, as the brief specifies.

The clause: `ALL`'s length is compile-checked; every prose copy is unchecked; and task
26 demonstrated that **even the commit writing the policy can leave a member behind**
(`metrics.rs:989` survived a sweep that deleted `mh-service.md`'s integer in the same
change). So the instrument is a guard, not better prose — a correctly-diagnosed class
with an incomplete sweep is not fixed by restating the policy.

**Ownership is SPLIT, per CLAUDE.md's guard-ownership rule** — @observability initially
wrote "owner observability" and corrected themselves: **guard machinery — matcher
implementations and the shape of the constructs policy is expressed in — is
`infrastructure`**; what is mine is the **policy content**: what counts as a drifted
restatement, which prose constructs are in scope, and that a compile-checked `ALL`
length is the authority a prose copy must not contradict. The same split is applied to
note (i)'s label-roster half, which must not read as observability-owned end to end.

**(ii) bcrypt bucket-fidelity is advertised but not asserted.** Test-coverage debt.
`crates/ac-service/tests/bcrypt_metrics_integration.rs` **advertises
bucket-fidelity it does not assert** (@test's wording, adopted) — the bodies assert
`observation_count` and unobserved/adjacency only; nothing reads a bucket.
`DEFAULT_BCRYPT_COST` is a production constant whose lowering would move timings
outside the 50 ms–1000 ms bucket design **with no failing test**. Task-sized, not a
one-liner: `crates/common/src/observability/testing.rs` has no bucket introspection
at all, so proving observations land in the designed range needs a new method on a
helper shared by every service. Contrast stated in the note so the two are not
conflated: this story's bucket-objective binding in `mh-service`
(`MEDIA_FORWARD_OBJECTIVE_SECONDS` ∈ `MEDIA_FORWARD_LATENCY_BUCKETS`) is a pure
**constant-membership** assertion needing no introspection.

### Decision 9 — `label-taxonomy.md`: REVISED to a minimal shared-label edit

**My original position was "no edit", on the grounds that the task freezes the
media label scheme with the catalog as its only home and a taxonomy entry would
be a second home. @security showed that reasoning is inverted, and I concede it.**

The taxonomy is not a second home for a shared label — it is the **first** one.
§Adding a new shared label says so in its own words: *"Shared labels are added
here BEFORE they're used in a second service. If you're about to introduce a label
that another service will eventually emit — stop, add it here first."*
`media_kind` and `content_kind` are multi-service **by construction** (I am
declaring them on MH, MC and client counters simultaneously), so that rule fires
now, at declaration, not in story 3. Declaring the bounded value sets in three
catalogs would have been three homes for one value set — the exact drift surface
this plan refuses to create in Decision 4, applied in the opposite direction. The
precedent is already in tree twice: `key_custody` and `direction` each have a
taxonomy row *and* per-catalog entries, with the split that resolves it.

Planned edit, minimal and pointer-shaped:

- One shared-label entry each for `media_kind` (`audio` | `video`) and
  `content_kind` (`main` | `slides`), marked **declared for story 3, not carried
  today**, with the bounded value set and why it is bounded.
- **No restatement** of the `direction` convention or the §Key custody rules —
  those are already homed there, and the catalogs will cite them.

@security is the security half of the co-ownership and has explicitly asked for
this edit, so it is a requested hunk on a co-owned file, not me reaching into
someone else's. The file's row in the classification table moves from
**expected NO EDIT** to **Not mine / Minor-judgment, Owner: observability +
security (co-owned), requested by @security**.

### Decision 10 — the operations confirmation (independently reached, then matched)

I inspected `MCMediaMissingKeyMaterial`
(`infra/docker/prometheus/rules/mc-alerts.yaml:427-445`) against the emitting code
before @operations' message arrived, and we agree on every identifier:

- `dt_client_media_frames_dropped_total` — matches `mediaMetrics.ts:296`.
- `dt_client_media_frames_received_total` — matches the emitter and the catalog.
- label key `reason` — matches `{ ...this.#base, reason }`.
- values `no_kek_for_generation`, `no_roster_entry` — both in `RejectReason` /
  `ALL_REJECT_REASONS` (`rejectReason.ts:98-99`, `:121-122`), both **reachable
  today** (`receivePath.ts:504`, `pipeline/ingress.ts:249`), and `frameDropped()`
  passes `FrameRejectedError.rejectReason` through **verbatim**, so the label values
  are those same strings.
- denominator `received` is the attempts sum under the ratified receive-path
  identity (`client.md:254`).

**Expression confirmed correct by inspection; no change requested to
`mc-alerts.yaml`.** Per @operations, if a catalog entry I write disagrees with any
of those four spellings, the catalog is wrong and I route it to them before
changing either.

### Gate 1 revisions (raised by reviewers, verified by me, folded in)

Nine findings across @security, @dry-reviewer and @observability. I verified each
against the tree rather than accepting it, and one of them resolves differently
than proposed.

**G1-1 (blocking, @observability) — RESOLVED THEIR WAY. My first analysis was
wrong and I am recording why, because the way it was wrong is the interesting part.**

The finding: `slos.md:138-141` says the MH media SLI ends at "**enqueued for
transmit**", while `MediaLatencyPhase::Total` records `sent_at - received_at` with
`sent_at` taken *after* `transport.send_datagram()` returns
(`media/ingress.rs:243-251`) — one phase wider.

**My first resolution was that `total` IS the SLI and the prose is merely
ambiguous.** I argued it from internal consistency: the same section says the SLI
decomposes into three phases *including transmit buffer*, and that `total` exists
"**solely** to serve this objective" — so an SLI ending at `queued_at` would
exclude a phase it claims to decompose into, and `total` would not serve the
objective at all. `mh-slos.json` panel 7's description corroborates the span
("`transmit_buffer` (egress push -> send returned)").

**That reasoning counts sentences. @observability's counter-argument applies a
ratified principle, and a principle beats a majority of sentences.** I verified it
at `slos.md:224-238`, and it is stronger than they even needed:
`mh_media_frames_dropped_total{reason="transport_receive_dropped"}` is ruled **not
SLI-eligible** as a **hard constraint**, because it is client-influenceable — "a
client-inflatable SLI converts an availability attack into an **error-budget
attack**, and if that budget ever gates a release, into a **client-controllable
deploy block**." Now apply that rule to `total`: it includes `transmit_buffer`,
which MH's own artifacts describe as **"a slow subscriber"** — client-influenced by
construction. And `slos.md:141-143`'s exclusion list already excludes that class
explicitly, "because MH does not control any of it". So `total` as recorded today
is not SLI-eligible under this file's own rule, and pinning the SLO panel to it
walks into the constraint the file spends fifteen lines erecting.

They also pre-emptively discarded the argument I would have reached for next:
"quantiles do not sum" establishes only that the objective needs its *own*
observation. It is **span-agnostic** — an SLI stopping at `queued_at` would need a
dedicated series too — so it cannot license `total`'s current span. Correct, and it
is the argument I had leaned on.

**So the file is internally inconsistent, not drifted, and there is no typo-level
fix.** One of two ratified-sounding statements has to lose, and that is a design
call `slos.md` explicitly reserves for story 8. Deciding it inside this devloop
would be exactly the quiet pre-emption the constraint block exists to prevent.

**Resolution adopted (theirs, in full):**
(a) **`mh-slos.json` panel 7 is left entirely alone** — all four phases, description
unchanged. Nothing is pinned as the SLI, which is the honest state. This also
resolves @media-handler's stale-description concern and @code-reviewer's
Minor-vs-Domain question on that hunk: **the hunk no longer exists**, so there is
nothing to classify and nothing to ACK.
(b) **`slos.md` gains a SECOND open item** under §Open: the target — the measurement
point and `MediaLatencyPhase::Total`'s span disagree by the transmit-buffer phase;
`total` as recorded is client-influenced and therefore not SLI-eligible under this
file's own rule; and the two candidate resolutions named (narrow `Total`'s recording
to end at `queued_at` — MH-owned, a hot-path change, **not this story**; or
re-ratify the measurement point to include transmit-buffer and accept the client
influence, which the `transport_receive_dropped` precedent argues against). It goes
in that file for the same reason the constraint block gives: story 8's author
ratifies the number while standing in it.
(c) **The dedup is achieved from the other end** — the three-phase triage view moves
to `mh-media.json`, and its description does **not** restate the per-phase remedy
roster (which already lives on panel 7 and in `mh-service.md` §Media Forward Path).
It states *why* the split exists — the task's actual requirement — and cites the
catalog for the per-phase remedies. No second roster copy, and no MH-owned edit.

Net effect on the table: `docs/observability/slos.md` added as **Mine**; the
`mh-slos.json` *query* narrowing drops entirely. **Corrected by the second round:**
that file is **not** NO EDIT — it retains an owner-requested **description-only**
hunk (R-1).

**G1-2 / G1-3 (@observability) — three `mh-media.json` panels failed my own stated
bar. Adopted, with G1-3 doing most of the work.**

They dumped `mh-overview.json` and found panel 36 is
`sum by(reason, direction) (rate(mh_media_frames_dropped_total[...]))` — my panels
3 and 4 pre-split by direction — and panel 35 is the forwarded-by-direction series
my panel 6 takes the ingress arm of. That is duplication, and my "every panel
answers a question no existing panel answers" claim did not survive the check.

G1-3 is the better remedy and I had the classification wrong: ADR-0029 Category A
and `dashboard-conventions.md` §Counter metrics call for
`increase($__rate_interval)` on discrete event counts, reserving `rate()` for
ratios and per-second normalised series. Drops in a healthy loopback are rare
discrete events, and at the **deployed** 15 s scrape cadence a single drop renders
as ~0.003/s on a `rate()` panel — reading as zero on the panel whose entire
purpose is that a drop is visible. So panels 3 and 4 become
`increase($__rate_interval)`: a readable-integer triage view that panel 36
genuinely does not provide, which discharges the duplication and fixes a real
legibility defect at the same time.

> **SUPERSEDED by §Gate 1, final round. Kept, not deleted — the reasoning trail
> matters, and the fact that this row reversed TWICE is itself informative to a
> later reader.** @observability's ADR-0029 ruling narrows `mh-overview.json`
> panel 36 after all, so that row is a **REAL EDIT** (see the table). The
> present-tense prose below misled @code-reviewer into reconciling a row that was
> already correct — one reviewer-cycle spent on a stale sentence, and the **third**
> time this devloop has hit that shape (`client.md`'s not-queryable block,
> `dashboard-conventions.md`'s D1 blockquote, and `MediaSessionStartOutcome::Started`'s
> docstring that "misdirected three sessions"). A record that is accurate at the end
> but carries a confident, undated, present-tense wrong statement in the middle **is
> the artifact, not a draft of it.**

Panel 6 has no such answer, so **panel 6 is dropped.** Panels 5 and 7 already carry
`forwarded` inside the invariant stat and the delivery ratio, so the media board
does not lose its denominator context. Rather than narrow `mh-overview.json`'s
panels 35/36 — which would widen the MH-owned surface for no signal gain now that
3/4 are a different presentation — I keep them as the overview's at-a-glance
`rate()` view and say so in the media panels' descriptions. **I am not inheriting
the house-style `rate()` issue on 35/36 and not fixing it either; @dry-reviewer is
already filing that class as an extraction opportunity.**

*This reversed the classification-table change I made for G1-2* — **and was itself
reversed again by the final round: `mh-overview.json` IS edited.** See the marker
above.

**G1-4 (@observability) — SUPERSEDED by O-1 below.** Their reasoning was correct
*conditional on the narrowing happening*. The narrowing is dropped, so panel 7's
description stays attached to the panel it accurately describes, and nothing is
stranded. What survives is their underlying point — the phase-split rationale must
not get a second home — and it is handled by splitting the *content*, not the
panel: see O-1.

**O-1 (@operations) — my justification for the `mh-slos.json` narrowing was
FACTUALLY FALSE. Edit dropped in full.**

I claimed panel 7 draws "the 30 ms objective threshold across four series, three of
which the objective is not about". @operations dumped the panel and found no
threshold at all. **I verified it myself rather than taking it:**

```
thresholdsStyle: {'mode': 'off'}
thresholds:      {"mode": "absolute", "steps": [{"color": "green", "value": null}]}
```

One null-valued green step, rendering explicitly off. The 30 ms figure appears only
in the description, and there it appears as a *warning* that the objective is
provisional and that no burn-rate alert may rest on it. **The defect I justified the
edit with does not exist in the artifact.** I am recording that plainly rather than
softening it, because a devloop record that justifies an out-of-scope edit to a
deployed board with an imaginary defect cannot be audited later, and the next reader
would inherit a false belief about what panel 7 looked like.

**Edit dropped in full** — and it is now over-determined, since G1-1 independently
requires panel 7 to keep all four phases. Their reason 3 decides it on its own
terms too: panel 7's description carries a sampled-one-in-N caveat and a
flat-panel-is-ambiguous discriminator ladder that apply to `total` **and** to the
three phases equally. Narrowing would not let me split that description — it would
force me to duplicate most of it onto `mh-media.json`. **Net copies of the 3am
content go up, not down**, which is the opposite of the one-home argument I was
making. Their reasons 1 and 4 (their story-8 scenario can point at `mh-media.json`
directly; an operator backing out a bad new dashboard should not have to
hand-split a live SLO board out of the commit at 3am) hold as well. Their reason 2
(absent owner) was overtaken by @main spawning `media-handler` as a conditional
domain reviewer, so it is no longer load-bearing — I am dropping the edit on 1, 3
and 4.

**How the phase-split rationale avoids a second home without the narrowing.**
Panel 7's description states it in **SLO terms** — why a `total` observation has to
exist at all, since quantiles do not sum. `mh-media.json`'s latency panel states it
in **triage terms** — which phase implies which remedy, and why an operator reads
this board rather than that one. Those are two different questions with two
different answers, not one paragraph written twice, and each is attached to the
panel that answers it. The task's requirement to "put the reason for the phase
split in the panel description" is met by the triage form.

**G1-5 (@observability) — adopted.** The empty-panel + video-unreachable clause
goes on **every** drop panel, including both client drop counters
(`dt_client_media_frames_dropped_total`, `dt_client_media_send_dropped_total`).
No client token is video-gated, and the descriptions will **say that explicitly** —
silence is indistinguishable from having forgotten, and a reader arriving from the
MH panel will go looking for the clause.

**G1-6 (@observability) — adopted; guard-enforced panel shape.** Verified in
`crates/dt-guard/src/dashboard_panels.rs`: `panel_unit` exempts only `row` and
`logs` (`:423`, `:439`), `hardcoded_datasource` applies to panels *and* targets
(`:459-495`), and the `$__rate_interval` rule is non-SLO-dashboard scoped
(`:526-547`). Neither new file matches `*-slos.json`, so `$__rate_interval` /
`$__range` only, no hard-coded windows. **The row-0 text banner on
`client-media.json` is the trap**: `text` is neither `row` nor `logs`, so it needs
a `fieldConfig.defaults.unit` and a templated datasource or it reds Layer 3. A
`$datasource` template variable is declared in both files.

**G1-7 (@observability) — adopted, and it is the right instinct.** Green-at-HEAD
proves nothing about two brand-new files: if the guard's dashboard discovery does
not pick them up it reports green having inspected nothing new — review-protocol
§Assertion Vacuity mechanism 5. **Positive control before I claim Layer 3 green**:
deliberately corrupt one metric name in one new panel, confirm
`dashboard_metric_missing` fires *naming that file*, revert, re-run. I will report
the **failing** output, not only the passing one.

**C-1 (@dry-reviewer) — adopted, and it caught me contradicting a rule the sibling
catalog already ratifies.** My planned wording restated "13 MH-local series rather
than 26". `MediaDropReason::ALL: [Self; 13]` (`metrics.rs:949`) is compile-checked;
a prose `13` beside it is an unchecked copy and `26` is derived from it, so one new
token rots both — in the section whose whole purpose is to be the frozen scheme's
only home. `mc-service.md` already ratifies the rule ("**Deliberately no restated
integer** … a prose count is an unchecked copy"; the number drifted four times in
one devloop). **Both integers dropped.** Replacement states the rule instead, which
cannot rot and is strictly more informative: *"each token maps to exactly one
direction, so the counter carries one series per token, not one per
token×direction pair."*

**C-2 (@dry-reviewer) — adopted.** `mh-service.md:590-608` already enumerates all
13 `reason` tokens with their `direction`. The new conventions section **cites that
table** and does not re-enumerate; otherwise the frozen scheme gets a second home
*inside the same file*, at the shortest possible distance.

**C-3(a) (@dry-reviewer) — adopted.** The env-test's reason roster is a **named
`&[&str]`** with the deliberate-restatement comment (same treatment as
`key_custody`) and **no integer**. Assertion is `missing.is_empty()` with the
missing tokens named in the message — never `assert_eq!(found.len(), 13)`, which
reds on a legitimate 14th token while telling the reader nothing about which one,
and is C-1's unchecked copy wearing a test's clothes.

**C-3(b) (@dry-reviewer) — adopted, and their correction is sharper than my
original reasoning.** I had said the "fetching ONCE is structural" argument travels
with the hoisted function "because that reasoning is the reason the function
exists". That is wrong. The function exists because two files need to turn a
Prometheus response into `Vec<Series>` — mechanics, one home. But "one fetch feeds
**both** assertions, so the anchor guarantees the predicate had input" is a property
of a **call site**: the kernel returns a `Vec` and cannot make any caller use one
fetch for two assertions. A third caller could fetch twice while the kernel's doc
still asserted a guarantee that held nowhere — a false SSoT of *reasoning*, which
hides the fork instead of closing it. **Split**: mechanics + fail-loud contract at
the kernel; the one-fetch-both-assertions argument stays at each call site, 32
keeping its own and `30_observability.rs` writing its own for its own assertion
pair. Two call-site arguments about two different assertion pairs are two facts,
not one duplicated fact.

### Gate 1, second round: the two owner-requested hunks

Both changed after the owners weighed in. Recording the reversals explicitly,
because each is a row I had previously written as NO EDIT.

**R-1 — `mh-slos.json` panel 7 becomes a description-only edit (was: no edit).**

@observability's G1-4 **inverted** once the query narrowing was dropped, and they
caught it before I built the mirror-image problem. Original risk: narrowing strands
three-phase prose on a one-phase panel. New risk: the query does *not* narrow, so
panel 7 keeps a full three-phase rationale — and the task requires me to put that
same rationale in `mh-media.json`'s latency panel. Appending a pointer and leaving
panel 7 alone writes the second copy my own mechanism restatement forbids, and
unlike `mh-overview.json:1769` this one would be **created by this diff**.

The split is on what-the-panel-plots vs why-the-split-exists:
- **Stays on panel 7** (it plots four series and must name them): the phase
  *definitions* — `receive_buffer` (received → ingress pop), `processing` (ingress
  pop → egress push), `transmit_buffer` (egress push → send returned), and that
  `total` is a fourth series. Plus the sampled-one-in-N caveat and the
  flat-panel-is-ambiguous discriminator ladder, untouched — @operations' reason 3
  was that those apply to all four phases equally, and they still do.
- **Moves to `mh-media.json`'s latency panel** (the task's requirement makes this
  the single home): the *rationale* — different remedies per phase, an
  undifferentiated total not telling an operator which to pursue, and that a single
  total series would foreclose the story-8 phase-triage scenario. Panel 7 points
  at it.

**The second half of R-1 is the one that matters.** Panel 7 currently asserts flatly
that "`total` exists solely to serve the objective, because quantiles do not sum".
As of the `slos.md` open item, **that sentence is contested** — the open item records
that `total` includes `transmit_buffer`, is client-influenced, and is not SLI-eligible
under `slos.md:224-238`. Leaving it unqualified would mean this devloop **closes one
drift and opens another, on the same object, in the same change**, with the dashboard
carrying the tree's most confident wrong statement about what `total` is for. So the
sentence becomes: `total` is a fourth series spanning received → send-returned;
whether it is the series the objective attaches to is **open** — see
`docs/observability/slos.md` §Open: the target. It points; it does not restate.

Corollary I am carrying into the `slos.md` edit: the open item must sit **adjacent to
or explicitly superseding** `slos.md:154-157`'s own "exists solely to serve this
objective" sentence, not elsewhere in the file silently contradicting it.

**@operations' correction to how this is cited, adopted — it is the same failure we
just fixed on panel 7.** `slos.md:224-238` bars `transport_receive_dropped` **by
name** and states the generalizable doctrine; it does **not** itself bar `total`.
Applying it to `total` is a sound *extension* of that doctrine, not a citation of a
rule already on the books. So the open item must **state the `total` case in its own
words** — a bare pointer to a blockquote about a different metric would read, to the
next person, as though the rule were already written for `total` when it is not. The
mechanism is stated explicitly: `transmit_buffer` spans egress-push →
transport-send-returned, so a slow subscriber lengthens it, so `total` inherits
client-influenceability from that phase.

**G1-10 — the open item's FRAMING is inverted, and this is the single most important
correction of Gate 1.** @observability found it while conceding the phantom third
home, in the very line @media-handler cited to refute them. I verified it:
`metrics.rs:1029` says the histogram measures "MH-internal **ingress-from-network to
egress-to-network**". That is a *third, independent* statement of the measurement
point — a different claim from the "solely" sentence — and it agrees with the code,
not with `slos.md`:

| Artifact | Span end |
|---|---|
| `media/ingress.rs:249` — `Total` = received → `send_datagram()` returned | **sent to network** |
| `metrics.rs:1029` — "ingress-from-network to egress-to-network" | **sent to network** |
| `slos.md:138-141` — "the moment the rewritten datagram **is enqueued for transmit**" | **enqueued** |

**`slos.md` is the outlier, two-to-one.** Both my original analysis and
@observability's G1-1 framed this as "the code drifted from the ratified SLI". **It is
the reverse.** The intended span was always received → sent-to-network; `slos.md`'s
narrower sentence is the odd one out.

That materially changes what the open item must say. It is **not** reconciling a slip
in the code — that framing would understate the problem *and* be unfair to the MH
authors, who implemented exactly the span their own doc describes. The harder, true
statement is: **the intended span includes `transmit_buffer` — which MH's own
artifacts call "a slow subscriber" — and is therefore client-influenced by
construction.** Resolution (b) is effectively what the tree already reflects, and (b)
is precisely what is unsuitable under `slos.md:224-238`'s doctrine. **The design is
coherent; it is coherent around a span that cannot safely carry an error budget.**

**Blast-radius entry, no edit to anyone's file**: the open item names `metrics.rs:1029`
as *moving with the resolution*. If story 8 takes (a) and narrows `Total` to end at
`queued_at`, that sentence silently becomes false. It is **accurate today**, which is
exactly why it would go stale unobserved — the archetype §Mechanism-restatement already
names. Listed, not edited; @media-handler endorses that framing because it protects
their file rather than churning it.

**Owner verification of the load-bearing step, recorded because of how it was
obtained.** @media-handler did not concur from a relay — they read `ingress.rs:249`
themselves and confirmed `Total = sent_at - received_at` with `sent_at` taken *after*
`send_datagram()` returns `Ok`, and `TransmitBuffer = sent_at - queued_at` in the same
block, so **`Total` structurally includes `transmit_buffer`**. That step was previously
@observability's inference; it is now **three independent readings** (their grep, mine,
and the owner's code read). Given that @observability had already been wrong once about
this same file, the standard is the point: after a relay is shown to be unreliable, the
next claim about that file gets read at the source rather than forwarded.

Two owner concurrences logged:
- **No `metrics.rs` edit**, owner-concurred: `:1026-1041` correctly describes the span
  the code measures, so churning it would **be** the drift rather than fix it.
- **The blast-radius listing is owner-ACKed**: if story 8 narrows `Total` to
  `queued_at`, `metrics.rs:1029` and the `ingress.rs:249` recording go false
  **together**, so listing them in one place is the protection that file needs.
  `slos.md` is mine and needs no trailer — the concurrence is recorded as evidence the
  naming was owner-reviewed rather than asserted.

**The story-8 consequence, recorded where its author will hit it (@operations):**

1. **The objective cannot be ratified against `total`.** The SLI-eligible subset is
   `receive_buffer` + `processing` — MH-internal and not client-influenceable. This
   is a **live trap today**: story 8's author walks in, finds a `total` phase whose
   stated purpose is "serve the objective", and ratifies 30 ms against the one
   series barred from carrying it. That is exactly what panel 7's description says
   `total` is for, which is why R-1's qualification of that sentence is not cosmetic.
2. **`transmit_buffer` is not thereby useless** — it gets the same carve-out
   `slos.md` grants `transport_receive_dropped`: survivable for a `warning` alert
   with a rate and a sustained window, not survivable in an SLI. Stated so nobody
   over-corrects into deleting the phase.
3. **This makes the three-phase panel more load-bearing than triage alone made it.**
   `mh-media.json` becomes the only place the SLI-eligible subset is separable from
   the client-influenceable one — a second, independent reason the phases must never
   be aggregated there, and a **stronger** reason than mine, because a violation is a
   security property rather than a triage inconvenience. **It goes in
   `mh-media.json`'s latency panel description**, not only in the story-8 scenario:
   the dashboard is where someone stands when they are tempted to aggregate.

**G1-9 (@observability) — adopted, and it corrects the sentence I had just added at
their own suggestion.** They raised it against their own finding, which is the
reason it is right.

I was about to write that `mh-media.json` is "the only place the SLI-eligible subset
is separable from the client-influenceable one". **The subset is *visible* there; it
is not *available* there — and the gap between those two words is the
quantile-summing error.** `p95(receive_buffer + processing)` cannot be obtained from
`p95(receive_buffer)` and `p95(processing)`. That is the very "quantiles do not sum"
property that justifies `total` existing at all, and it bites the eligible subset
exactly as hard as the full span.

**The true state is sharper and worse than my sentence implied: there is NO series
measuring the eligible span today.** Not `total` (too wide, client-influenced); not
any combination of the three (quantiles do not sum). The eligible span is currently
**unobserved** — which is precisely why candidate resolution (a), narrowing `Total`'s
recording to end at `queued_at`, is the *real* fix rather than a tidier alternative
to (b).

The hazard lands on exactly the reader I was writing for: a story-8 author standing
at that panel, told the eligible subset is "separable here", could reasonably
conclude the SLI is obtainable from the two series and ratify a number off them — a
number computed from a quantity that **cannot be computed**, made to look
authoritative by coming off the SLO-adjacent board. My sentence was aimed at
preventing an aggregation and could have induced a worse one.

**Required wording, in BOTH homes** (the panel description and the `slos.md` open
item, since the open item is where story 8's author will actually be standing): the
eligible span has **no series today**; name **both** reasons it has none; and point
at resolution (a) as what would create one. The security framing survives — a
violation there is a security property, not a lost convenience — but it must sit **on
top of** "and you cannot construct it here either", or it reads as an invitation.

**"Third home" — CLAIMED, CHECKED, DOES NOT EXIST. Two homes, not three.**
@observability reported that `metrics.rs`'s `MediaLatencyPhase` doc comment carries
the same "exists solely to serve this objective" claim, and I relayed it to
@media-handler. **@media-handler read their own file and it is not there**; I then
grepped `solely|serve this objective|serve the objective` across all three artifacts
and confirmed: the phrase occurs **only** at `mh-slos.json:450` and `slos.md:154`.
`metrics.rs`'s `MediaLatencyPhase` doc (`:713-717`) makes the *different-remedies*
argument, the `Total` variant (`:727`) is a bare span definition, and
`MEDIA_FORWARD_OBJECTIVE_SECONDS` (`:1026-1041`) is **already explicitly
provisional** — "not ratified against this measurement point … Story 8 ratifies …
No burn-rate alert rests on it."

So `metrics.rs` needs **no** qualifying edit, and its only hunk remains the one-line
`direction()` fix. Recording the non-finding rather than dropping it silently:
qualifying an already-provisional doc would be churn, and editing toward a third home
that is not there risks degrading wording that is already right. This is the same
class of error as the two I made — a confident reference to something the artifact
does not contain — arriving from a reviewer this time, and caught only because the
owner read their own file instead of accepting the relay.

**R-2 — `metrics.rs` gains one doc-comment hunk (was: no edit), owner-specified.**

@media-handler, as owner, supplied the exact replacement for the stale `direction()`
doc comment (~`:988`) and asked me to land it. Their reasoning is the same rule
@dry-reviewer's C-1 applies to my catalog, and it is why they explicitly told me
**not** to swap `11`/`22` for `13`/`26`: the file already documents that a restated
cardinality integer was DELETED rather than updated, because a prose copy of a
compile-checked count re-drifts at the next token — so resetting the integers just
re-arms the trap at token 14. The semantic point is one-series-per-token, which needs
no integer at all.

Landing verbatim as supplied:

```
    /// Paired with the token at its definition site, so `reason` and
    /// `direction` cannot disagree: each token has exactly ONE direction, which
    /// is why the counter carries one MH-local series per token rather than two.
    /// A token that could legitimately occur in both directions would be two
    /// conditions wearing one name.
```

**G1-11 (@observability) — already resolved, by independent convergence on the same
fix.** They raised the `:989` stale count separately and independently prescribed the
**deletion-shaped** remedy rather than `11 → 13`, citing `ALL`'s own doc comment
("prose restating it is an unchecked copy"; the ordinal "deliberately not named").
@media-handler, as owner, had already specified exactly that wording. Two reviewers,
arriving from opposite directions — one from the file's stated policy, one from
owning the file — landing on identical text is the strongest evidence available that
the fix is right, and it is recorded rather than collapsed into "already planned".

**Their characterisation of the instance is new and sharper than mine, and it changes
what the Lessons Learned entry should say.** Story task 26 added two tokens; *that
same commit* wrote `ALL`'s warning about restated ordinals **and** deleted
`mh-service.md`'s restated cardinality integer — and missed `:989`. So this is not
"someone didn't know better". It is **a correctly-diagnosed class with an incomplete
sweep**: the policy was written and applied in two places, and the third was left
behind. That is a materially different failure from the other four in the set, and it
argues the **sweep** is what needs a guard, not the individual sentence.

The `method`-collapse block is untouched and remains verify-and-report. Note the
consistency this produces across three files in one diff: `metrics.rs`, my catalog
sections (C-1) and the env-test roster (C-3a) all now state the correspondence by
**naming members or stating the rule**, and none of them restates a count.

### Gate 1, final round: the ADR-0029 ruling and the overview split

@observability, as ADR-0029 owner, issued a **spec** (not options) that resolves
G1-2 and G1-3 together. @code-reviewer had independently flagged the same thing.
Taken as given:

1. **`mh-media.json` panels 3/4** → `sum by(reason) (increase(mh_media_frames_dropped_total{direction="ingress"}[$__rate_interval]))`
   and its egress twin, unit `short`. The sharpened reason is worth recording: at
   the **deployed** 15 s cadence a single drop under `rate()` renders ~0.003/s —
   not merely small, but **identical to healthy**, at the exact moment the panel
   has something to say, in a loopback story whose expected drop count is zero.
2. **`mh-overview.json` panel 36 narrows to `sum by(direction) (increase(...))`**,
   description pointing at `mh-media.json` for the by-reason breakdown. **This is
   how G1-2 discharges, and the mechanism matters**: it does not delete a
   duplicate, it splits one question into two — the overview answers *whether* we
   are dropping and on which side, the media board answers *why*. Plotting one
   series on two boards at two transforms would still be two homes for one
   question. It also fixes the reads-as-zero defect on the board an oncall opens
   **first**, rather than leaving the broken rendering where it does most damage
   and building a correct copy elsewhere. Panel 36 is explicitly **not** retired —
   stripping the drop signal from the first board reached would defeat the
   overview's job of saying *something is wrong, go look there*.
3. **`mh-media.json` panel 6 is dropped** — the ingress arm of overview panel 35,
   whose forwarded denominator is already inside panel 7's ratio. G1-2's third leg
   closes by subtraction.

**Final `mh-media.json` panel list is SIX**: latency p95, latency p99, ingress
drops by reason, egress drops by reason, zero-forever invariant stat, egress
delivery ratio.

**@operations' four constraints on the overview edit, all accepted:**

1. **Both addends stay on the overview.** Panel 35's own description warns: "THIS IS
   THE DROP-RATE DENOMINATOR … build any drop rate on the ATTEMPTS SUM (forwarded +
   dropped), never on one addend", and `MHMediaEgressQueueOverflowRate`
   (`alerts.md:841-851`) builds its ratio exactly that way. Since panel 36 is
   narrowed rather than moved, the overview keeps **both** addends and the alert's
   arithmetic stays reproducible on the board a responder lands on. Had I moved
   drops off the overview entirely — my earlier instinct — that arithmetic would
   have been reproducible on neither board alone.
2. **The narrowed panel 36 names `mh-media.json` by dashboard title.** Without it a
   responder either stops at "drops are happening" or writes ad-hoc PromQL at 3am.
3. **Panel 35's positional cross-reference is converted to a title reference.** It
   currently says the discriminator is "Media Session Starts by Outcome, **the first
   panel in this row**" — true only by layout (panel 38 at `y=84,x=0`, panel 35 at
   `y=84,x=12`), falsified by any `gridPos` change, and caught by nothing. Panel 36
   already names it by title; 35 is converted to match.
4. **The flat-panel ambiguity ladder is preserved** on whichever board keeps a drop
   panel — which is now both — and applies to `mh-media.json`'s equivalent panel too.

No doc cites any of the three affected panel titles (@operations grepped `docs/`),
so retitling breaks no link; `mh-incident-response.md:769`/`:778` cite two *other*
panels, which this sweep does not touch.

### `client-media.json` banner: acceptance criteria (@operations)

Recorded as criteria so they are not a surprise at review:

- The board is **currently non-functional in its entirety** — not "panels may be
  empty". Every panel is in the unwired state today.
- **The phrase "no data means no drops" must not appear on this board.** It is the
  *required* wording on `mh-media.json`'s drop panels and the exact misread here:
  it would turn a broken pipeline into a clean bill of health. Two boards, opposite
  required phrasing — the copy-paste between them is the hazard, and it is named
  here so it is checked rather than assumed.
- Name the discriminator for later, once both states are possible: **series-absent
  vs series-present-at-zero** (`count(dt_client_media_frames_received_total)`
  returning no data vs a number).
- Name the remedy and its tracking entry (`docs/TODO.md` §Observability Debt,
  collector metrics export).
- **State the condition under which the banner is removed** — when the collector
  exports and a job scrapes it. A "this board doesn't work" notice that outlives the
  fix trains responders to skip the board, and would be the last thing anyone
  deletes.
- No end-to-end or zero-trust claim.

@operations independently re-verified the premise (`otel-collector/configmap.yaml:57-59`
is `exporters: [debug]`; `prometheus.yml` has exactly four jobs, none scraping the
collector), which is a third confirmation that keeping `client.md`'s warning is right.

**I will not touch `docs/observability/alerts.md`**: `alert_rules.rs` pins each
`expr` byte-for-byte against its rule file, so even a reformat reds. `mc-alerts.yaml`
needs no change from me.

### Gate 1: infrastructure's two findings (both accepted, both fix-now)

@infrastructure prototype-built the proposed generator group against the real base
(`kubectl kustomize` over a temp copy with a stub `client-media.json`) and confirmed
the shape: labels land, `disableNameSuffixHash` keeps the name stable, the top-level
`namespace:` puts it in the sidecar's `NAMESPACE`, RBAC already covers it, and no
downstream edit is needed. Neither finding is against the hunk; both are against
what surrounds it.

**I-1 — my recorded host-side action was incomplete, and the omission reproduces
the exact silent failure Decision 5 exists to prevent.** I had written
`kubectl apply -k …` "plus sidecar pickup". **There is no pickup.** Three in-tree
facts they supplied and I am recording because each is individually
counter-intuitive:

1. `generatorOptions: disableNameSuffixHash: true` (`infra/grafana/kustomization.yaml:13-14`)
   means generated ConfigMap names are **stable**, so changing a group's contents
   does not change the Deployment spec and **does not roll the pod**. (The
   `prometheus-config` generator is the deliberate opposite — its header comment
   says the hash is kept precisely *because* it rolls the pod. Grafana is the
   contrasting case and gets no rollout.)
2. The k8s-sidecar is an **initContainer** with `METHOD: LIST`
   (`infra/grafana/deployment.yaml:19-42`). It lists **once** at pod start, writes
   into the `emptyDir`, and exits. **It does not watch.**
3. `grafana-dashboards-client` is a brand-new ConfigMap the Deployment never
   references; it reaches Grafana only through that initContainer's label LIST.

So `apply -k` alone yields a correct ConfigMap with a correct label and **nothing
new in Grafana** — the same silent no-show as a missing labels block, arriving one
step later. `deploy_observability()` gets away with it only because full setup
creates the pod fresh; it issues no restart. **@infrastructure's sharper framing,
adopted**: the labels block and the pod restart are **the same failure with two
causes** — a dashboard that is registered, guarded, committed, and **invisible** —
and **R-20 covers neither**. I-1 was therefore not a separate concern from
Decision 5 but its missing second half. The remaining action is **two actions with a readiness wait
on the second** — not three co-equal commands (@infrastructure's correction; read as
three, a reader drops the one that looks redundant). `rollout status` exists so the
step **fails loudly** rather than returning before Grafana is back, mirroring how
`deploy_observability()` pairs every deploy with a wait. Citing `docs/LOCAL_DEVELOPMENT.md:555` ("Grafana Not Showing
Dashboards") as the existing SSoT for the gesture rather than inventing one:

```
kubectl apply -k infra/kubernetes/overlays/kind/observability/
kubectl rollout restart deployment/grafana -n dark-tower-observability
kubectl rollout status  deployment/grafana -n dark-tower-observability --timeout=300s
```

**I-2 — `docs/observability/dashboards.md` §Kubernetes is stale and actively
instructs the mistake Decision 5 guards against. Fix-now: the file is already in my
changeset and the content is mine.** Verified against the tree; today it claims the
setup script "dynamically discovers" dashboards (it does not — `setup.sh` runs
`kubectl apply -k` against a **static** `configMapGenerator` list), references a
`grafana-dashboards-common` group that **does not exist**, says files are applied
`--server-side`, and — the harmful part — tells the reader to "**simply place the
JSON file** … no script edits required". That is flatly wrong: `dt-guard kustomize`
R-20 turns Layer 3 **red**, and a reader who then improvises a generator group from
this prose has no reason to know about `options.labels`.

**That paragraph is the closest thing the repo has to a human-side control over
R-20's `options.labels` blind spot, and it currently points the wrong way.** Being
the observability author of the very decision that identified the blind spot while
leaving its documentation inverted would be the worst outcome available. Rewritten
to the real mechanism: (a) add the JSON; (b) register the basename in the
`grafana-dashboards-<prefix>` group, creating the group if absent; (c) **a new group
MUST carry `options.labels.grafana_dashboard: "1"` or the dashboard is invisible to
the sidecar and no guard will tell you**; (d) R-20 enforces (b) bidirectionally but
**not** (c).

Not mine and not taken on: extending R-20 to assert the labels block is task-sized
`infrastructure`-owned machinery. @infrastructure is filing that TODO themselves, so
it does not become a third debt note in this changeset.

### Gate 1: @dry-reviewer's D-1 and D-2 (both adopted)

**D-1 — split the CODE along the same seam as the doc, both halves in the kernel.**
C-3(b) fixed the doc; the code needs the same cut. The hoisted body contains

```rust
let name = r.metric.get("__name__").cloned().unwrap_or_else(|| "<unnamed>".to_string());
```

and **two of the three consumers are name-keyed** — `32`'s anchor
(`s.name == MH_ANCHOR_METRIC`) and `scrape_reachable_client_series`
(`starts_with(CLIENT_METRIC_PREFIX)`). Both go **false-negative** on a series with no
`__name__`. `check_series` is unaffected (it iterates labels regardless of name), so
there it degrades triage rather than opening a hole — recording the accurate blast
radius rather than the alarming one. But for the anchor it is review-protocol
**mechanism 5**: the subject observes nothing and reports clean, **inside the suite
that exists to demonstrate against exactly that**. There is also a redundant
double-strip of `__name__` (filtered during `collect`, then `remove`d again) that no
test pins in either direction.

None of it has ever been exercised, because the mapping lives in a cluster-gated file
`cargo test` cannot reach — **the same root cause `32`'s own header documents for the
escaped-JSON relabel bug** ("It had no unit coverage precisely because it sat in this
cluster-gated file"). Hoisting it whole would move untested code without making it
testable, which is the trap.

So: `pub fn response_to_series(&QueryResponse) -> Vec<Series>` — pure, FIRE-fixturable,
with a fixture for the missing-`__name__` case. **My call as metric owner: it is
LOUD**, on the same grounds `results_to_instance_map` panics on a result row with no
`instance` label — refuse the key you cannot form rather than bucket it under a
sentinel, because a sentinel key is indistinguishable from a real miss at exactly the
assertion that depends on the key. The redundant double-strip is resolved to one strip
with a fixture pinning it.

**Both halves stay in `metric_hygiene.rs`** — thin `fetch_all_series` delegating to
pure `response_to_series` — per their correction to their own earlier message. Pushing
the I/O wrapper into the test files would duplicate the query-and-panic across `30`
and `32`, which is the duplication we started from. This mirrors the seam
`fixtures/metrics.rs` already models (`instance_counter_map` over
`results_to_instance_map`), and differs from the relabel check's shape (I/O in the
test file) for a concrete reason: **that one has one caller, this one has two.** A
sentence at the seam notes that the *parse* is what needed the always-on lane and the
I/O is a deliberately trivial wrapper, so the module's first I/O function does not
read as erosion of a kernel whose header advertises purity.

**D-2 — the `key_custody` comment must not lead with the limb that will be refuted.**
Adopted, and their scenario is the part I did not have.

My first limb was "`crates/env-tests/` links zero service crates by design". That is
**false as a reason**: `common` is not a service crate, and env-tests already depends
on `media-protocol` and `proto-gen` on exactly the shared-library-not-a-service ground
that would admit `common` too. A reader who notices will conclude the whole argument
was wrong and "fix" the literals into an import — and **the comment is the only thing
standing between these literals and that edit**, so a refutable limb stated first is a
fuse in it.

The comment therefore **leads with the artifact limb, and with the scenario that makes
it decisive**: `labels.rs`'s `key_custody_literals_are_stable` only reds on a rename
that *leaves the pin alone*. Someone doing an **intentional** rename updates the const
and its pin in the same commit — which is what a normal refactor looks like — and at
that moment an *importing* env-test renames itself too and stays green, while a
wire-visible break in a label every media dashboard and the `MCMediaMissingKeyMaterial`
chain select on ships with a **fully green suite**. The restated literal is the only
artifact in the tree that reds there. The dependency-hygiene limb is dropped.

**Panel 35, answering their Gate-2 question up front**: it **is** edited, and
description-only. @main's ruling named 35/36; @observability's ADR-0029 ruling narrows
**36's query**, and @operations' item 3 converts **35's description** from a positional
cross-reference ("Media Session Starts by Outcome, *the first panel in this row*" —
true only by layout, falsified by any `gridPos` change, caught by nothing) to a title
reference matching the form 36 already uses. So: 36 = query + description, 35 =
description only. Their planned re-sweep of all eight existing MH panels against my six
is welcome — Decision 1's "no panel answers an existing question" premise **demonstrably
missed panel 36**, so the enumeration has earned an independent check rather than trust.

**Both flipped rows are already corrected in the table** (`mh-overview.json` →
Minor-judgment REAL EDIT; `metrics.rs` → Mechanical, one owner-specified hunk), so
Gate 3 checks the commit against a table that describes it.

**The `metrics.rs` stale-count find is evidence, not tidying — recording it as such.**
"11 MH-local series rather than 22" rotted at the 11→13 growth **in the file that
defines the compile-checked `ALL: [Self; 13]`**, and nobody noticed. C-1 was not a
hypothetical about token 14: the trap had already fired and the rot was sitting in the
tree. Three files (`metrics.rs`, the catalogs, the env-test roster), three encodings,
one discipline, **zero counts** — this is a control, and the record says so rather than
letting it read as "we removed some numbers".

### Owner conditions accepted at Gate 1

- **@meeting-controller**: `mc-service.md`'s section will state **plainly that MC
  carries no `direction` label today** — MC's transmit and receive paths are
  *separate, single-direction metrics* (`mc_media_send_directives_total` vs
  `mc_media_receive_capability_declarations_total`), so there is no MC counter where
  both directions exist and the label is absent by construction. Written as
  not-applicable, never as a label MC might grow. They independently confirmed the
  lazy-creation fact my env-test asymmetry rests on.
- **@media-handler**: their stale-description concern on `mh-slos.json` panel 7 is
  moot — that hunk is dropped (G1-1). Their out-of-scope observation is recorded
  here for a future MH touch: **`metrics.rs`'s `direction()` doc comment (~`:988`)
  still says "11 MH-local series rather than 22", stale from the 11→13 token
  growth.** Not fixed here: `metrics.rs` is a NO-EDIT MH-owned file in this plan and
  the line pre-dates this diff. Note that per @dry-reviewer's C-1 my catalog will
  carry **no** restated integer either — so this diff does not add a third copy of
  that arithmetic, correct or otherwise.

### Validation

`dt-guard application-metrics --root /work`, `dt-guard kustomize`,
`dt-guard dashboard-panels`, `scripts/layer3.sh`, and `cargo test -p env-tests`
for the hoisted kernel's always-on unit lane. The new env-test assertions are
cluster-gated (Layer 7).

### Reviewer plan status

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Infrastructure (conditional owner) | confirmed — Owner ACK |
| Media Handler (conditional owner) | confirmed — Owner ACK |
| Client (conditional owner) | confirmed — Owner ACK |
| Meeting Controller (conditional owner) | confirmed — Owner ACK |

### Team-Lead Rulings (Gate 1)

1. **Ownership.** ADR-0011 gives `infra/grafana/dashboards/` and the metric
   catalogs to Observability; ADR-0031 (later, Accepted) refines *per-service*
   artifacts to the service specialist. The implementer classified conservatively
   toward ADR-0031, leaving Domain-judgment rows into `media-handler`, `client`
   and `meeting-controller` — none of whom were on the panel.
   **Ruling: (a) AND (b).** The story manifest's allocation of task 22 to
   `observability` stands as the story-level delegation (ADR-0035 makes the
   manifest the allocation authority, and the decomposition named these files
   knowing whose they were), so the task is not re-routed. But delegation alone
   does not satisfy ADR-0024 §6.3 for a Domain-judgment row, so `media-handler`,
   `client` and `meeting-controller` were spawned as **conditional domain
   reviewers** (the mechanism §Team Composition provides for Database/Protocol),
   ACKing their hunks at Gate 1 and Gate 3 and appearing in the Gate 3 verdict
   table. No classification was downgraded.
2. **`mh-overview.json` edit authorised.** @observability found panel 36 already
   plots the by-reason drop breakdown the new panels re-plot; narrowing panels
   35/36 is the fix. That row flips from "Verification only — NO EDIT" to a real
   MH-owned edit, re-rowed Minor-judgment with `media-handler` ACKing.
3. **`client.md` delete declined — refusal upheld.** See §Superseded Task-Prompt
   Instructions.
4. **`method` collapse stays verify-and-report, no edit** (task 13 landed it);
   file:line evidence recorded so the verification is auditable.
5. **The SLO measurement-point drift is in scope, fix-now.** `docs/observability/slos.md:138-141`
   declares the MH media SLI ending at "enqueued for transmit"; `MediaLatencyPhase::Total`
   (`crates/mh-service/src/media/ingress.rs:249`) records received → send-returned,
   one phase wider. The `phase="total"` narrowing promotes that latent drift into
   the pinned SLO series and story 8 ratifies a number against it. Observability-owned,
   in-domain, small — does not meet the deferral burden of proof.

---

## Pre-Work

None.

---

## Implementation Summary

Two new Grafana dashboards, media label-convention sections in three catalogs, a
declared-shared-label registration, an SLO open item, two `docs/TODO.md` debt notes,
and a two-assertion env-test extension. Two clauses of the task brief were
**verified-and-not-executed** because prior tasks had already satisfied them, and one
was **declined** because its premise is false against the tree (see §Superseded
Task-Prompt Instructions).

### Dashboards

- **`infra/grafana/dashboards/mh-media.json`** (new, uid `mh-media`, 6 panels) — the
  media-path triage board. Latency p95/p99 across the three component phases with
  `total` excluded by selector; ingress and egress drops by reason; a zero-forever
  invariant stat with `noValue: 0`; egress delivery ratio built on the attempts sum.
  Drop panels use `increase($__rate_interval)` per ADR-0029 Category A, carry **no**
  `or vector(0)`, and state that an empty panel means no drops occurred — the expected
  healthy loopback state — while naming **both** reasons that cannot fire until video
  (`partial_frame_discard`, `stream_rate_limited`).
- **`infra/grafana/dashboards/client-media.json`** (new, uid `client-media`, banner +
  10 panels) — all fourteen client media metrics, organised around the receive-path
  accounting identity. A row-0 banner AND the dashboard-level `description` state the board is
  non-functional in its entirety (the description so it shows in the Grafana board list
  before the board is opened; the banner for once inside). The banner names the
  discriminator for later (`count(dt_client_media_frames_received_total)`
  returning no data vs a number), names the remedy and its TODO entry, and states the
  condition for deleting the banner. **The phrase "no data means no drops" appears
  nowhere on this board** — it is the required wording on `mh-media.json` and the exact
  misread here.
- **`mh-overview.json`** — panel 36 narrowed to `sum by(direction) (increase(...))` and
  retitled "Media Frame Drops by Direction", pointing at the media board for the
  by-reason breakdown; panel 35's positional "the first panel in this row" converted to
  a title reference. Both drop-rate addends stay on this board so
  `MHMediaEgressQueueOverflowRate`'s ratio remains reconstructible where its
  `runbook_url` lands a responder.
- **`mh-slos.json`** — panel 7 **description only**; the query is byte-identical. Phase
  definitions stay, the why-the-split-exists rationale moves to `mh-media.json`, and the
  contested "`total` exists solely to serve the objective" sentence becomes a pointer at
  the new `slos.md` open item.
- **`infra/grafana/kustomization.yaml`** — `mh-media.json` appended to the existing MH
  group; new `grafana-dashboards-client` group carrying `options.labels.grafana_dashboard: "1"`,
  with an in-file comment explaining that R-20 does not check that block.

### Documentation

- **`§Media-path label conventions`** added to `mh-service.md`, `mc-service.md` and
  `client.md`: `media_kind`/`content_kind` as declared-not-carried; the transmit/receive
  convention **cited** from `label-taxonomy.md` rather than restated; and per-service
  consequences. MC's section states plainly that MC carries **no** `direction` label,
  by construction, because its transmit and receive paths are separate single-direction
  metrics. Client's states two identity dimensions plus one fixed non-identity
  `key_custody`, with §11's flat prohibition and the `events.ts` inertia risk named.
  **No restated integers anywhere** — the reason↔direction relationship is stated as a
  rule.
- **`label-taxonomy.md`** — `media_kind` and `content_kind` registered as shared labels
  at declaration, per §Adding a new shared label. No restatement of `direction` or
  §Key custody.
- **`slos.md`** — a second open item recording that this file is the **2-to-1 outlier**
  on the measurement point, that `total` is client-influenced via `transmit_buffer` and
  therefore not SLI-eligible under this file's own doctrine (written as an **extension**
  of that doctrine, not a citation), that **no series measures the eligible span today**
  because quantiles do not sum, and both candidate resolutions with a blast-radius list.
- **`client.md`** — the not-queryable block **sharpened, not deleted**; delete-condition
  preserved verbatim.
- **`dashboards.md`** — sections and ownership rows for both boards, plus a rewrite of
  the stale §Kubernetes block, which claimed auto-discovery that does not exist and told
  readers to add a dashboard in a way that turns Layer 3 red.
- **`docs/TODO.md`** — two notes: `dt_client_*`'s zero static guard coverage (with the
  roster-drift-fires-first wrong-fix trap), and bcrypt bucket-fidelity advertised but not
  asserted. Sweep-completeness folded into the first as a sibling clause.

### Code

- **`crates/mh-service/src/observability/metrics.rs`** — one owner-specified doc-comment
  hunk on `direction()`, replacing a stale count with the rule.
- **`crates/env-tests/src/fixtures/metric_hygiene.rs`** — new pure `response_to_series`
  (**panics** on a missing `__name__` rather than bucketing under a sentinel) plus a thin
  `fetch_all_series` I/O wrapper, with three FIRE fixtures including a `#[should_panic]`
  for the refusal.
- **`crates/env-tests/tests/32_media_metric_hygiene.rs`** — re-pointed at the kernel; the
  one-fetch-both-assertions argument **stays at the call site**.
- **`crates/env-tests/tests/30_observability.rs`** — new
  `media_path_metrics_carry_key_custody_and_every_drop_reason_is_registered`: MH gated on
  `key_custody=operator` presence and contains-all-13 drop reasons with bounded
  `direction`; MC recorded, never gated. Re-asserts nothing `32` owns.

---

## Files Modified

| File | Change |
|---|---|
| `infra/grafana/dashboards/mh-media.json` | **New** — 6-panel media triage board |
| `infra/grafana/dashboards/client-media.json` | **New** — banner + 10 panels |
| `infra/grafana/dashboards/mh-overview.json` | Panel 36 narrowed + retitled; panel 35 description |
| `infra/grafana/dashboards/mh-slos.json` | Panel 7 description only (query byte-identical) |
| `infra/grafana/kustomization.yaml` | `mh-media.json` registered; new labelled client group |
| `docs/observability/metrics/mh-service.md` | §Media-path label conventions |
| `docs/observability/metrics/mc-service.md` | §Media-path label conventions (no `direction` on MC) |
| `docs/observability/metrics/client.md` | §Media-path label conventions; queryability block sharpened |
| `docs/observability/label-taxonomy.md` | `media_kind` / `content_kind` shared-label rows |
| `docs/observability/slos.md` | Second open item + blast radius |
| `docs/observability/dashboards.md` | Two board sections, ownership rows, §Kubernetes rewrite |
| `docs/TODO.md` | Two debt notes |
| `crates/mh-service/src/observability/metrics.rs` | `direction()` doc comment (owner-specified) |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` | `response_to_series` + `fetch_all_series` + fixtures |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` | Re-point to kernel; doc split |
| `crates/env-tests/tests/30_observability.rs` | Media label-presence test |

---

## Devloop Verification Steps

**Gate 2 verdict: PASS.** Run under `DEVLOOP_FAIL_FAST=0` (headless/unattended
caller, so all seven layers are evaluated in one pass rather than fail-fast).

```
LAYER=1 RESULT=OK  DURATION=5
LAYER=2 RESULT=OK  DURATION=2
LAYER=3 RESULT=OK  DURATION=48
LAYER=4 RESULT=N/A DURATION=225
LAYER=5 RESULT=OK  DURATION=1
LAYER=6 RESULT=N/A DURATION=2
LAYER=7 RESULT=OK  DURATION=575
TOTAL_DURATION=858 TOTAL_RESULT=N/A
```

**No layer failed.** `TOTAL_RESULT=N/A` is worst-child aggregation over two
self-justifying `N/A`s, not a gap in coverage. Both are proto's registered
intentional-gap placeholder wrappers emitting `REASON=not-applicable-to-this-lang`
(ADR-0033 §6) — proto has no `test.sh` or `audit.sh` by design. Re-run
individually to confirm the languages that DO have wrappers all passed inside them:

- Layer 4: `STATUS=OK REASON=cargo-test-passed`, `STATUS=OK REASON=nx-test-passed`,
  then `N/A REASON=test-aggregate-na` from the proto placeholder.
- Layer 6: `STATUS=OK REASON=cargo-audit-passed`, `STATUS=OK REASON=pnpm-audit-passed`,
  `STATUS=OK REASON=buf-breaking-passed`, then `N/A REASON=audit-aggregate-na`.

Neither is a `FAIL-MISSING-VERB` and neither is a `NOT-RUN`, so no implementer
action is owed and nothing is unmeasured.

**Layer 7** ran its full suite: Rust env-tests `STATUS=OK REASON=env-tests-passed`,
browser E2E `STATUS=OK REASON=browser-e2e-passed` with 10/10 Playwright tests
passing. The media-loopback spec observed a 30 ms round trip to first decoded frame
(sent=28 accepted=28 over 16 samples).

**Layer 3** guards all green, including `application-metrics`, `dashboard-panels`
(16 files), `kustomize`, `metric-labels`, `alert-rules-policy`,
`media-telemetry-deny`, `rust-no-pii-in-logs`, `todo-tracking`,
`cross-boundary-scope` (`no-drift` — no file in the diff went unlisted in the plan,
and no plan entry went untouched) and `cross-boundary-classification` (33 files).

**Guard positive control** (required by @observability and @security, since both
dashboards are new files and a green guard over a file it never read proves
nothing — review-protocol vacuity mechanism 5): corrupting a metric name in
`mh-media.json` fires `dashboard_metric_missing` naming that file; reverted, back
to `OK`. The matching **negative control** demonstrates the `dt_client_*` blind
spot empirically — the same corruption in `client-media.json` leaves
`STATUS=OK REASON=application-metrics-clean`, a corrupted metric name sitting in
the tree with a green guard. That is what makes the `docs/TODO.md` debt note a
reproducible demonstration rather than a claim.

## Superseded Task-Prompt Instructions

Recorded per @main's ruling so nobody re-applies them at story close. **Not** §Accepted
Deferrals entries — nothing was found and left in the diff.

1. **"Retire `client.md`'s warning that no `dt_client_*` series exists in any backend."
   DECLINED — the premise is false at HEAD.** Verified three ways:
   `infra/services/otel-collector/configmap.yaml` metrics pipeline is
   `receivers: [otlp]` / `exporters: [debug]`; `infra/docker/prometheus/prometheus.yml`
   has exactly four jobs and none scrapes the collector; `docs/TODO.md` records the
   statement at five sites with "do not remove any of them without wiring the exporter".
   Nothing in tasks 13/16/19/21 wired an exporter. The retirement the prompt is reaching
   for — the older *not-yet-EMITTED* caveat — task 19 already did, and `client.md:14-21`
   records it as retired. **Emitted-and-exported is not queryable**; the prompt conflates
   them. Block sharpened instead, delete-condition preserved verbatim. Independently
   reached by @observability, who said a delete would be an escalate.
2. **"Collapse the documented `method` label values." ALREADY DONE by task 13.**
   Verified at `crates/mh-service/src/observability/metrics.rs:152-173`
   (`GRPC_METHOD_REGISTER_MEETING`, collapsed doc comment, parameter removed from
   `record_grpc_request`), `infra/grafana/dashboards/mh-overview.json:1769`, and
   `docs/observability/metrics/mh-service.md:220-225` — each already carrying the
   "single-valued by design / gains fields rather than sibling RPCs" reasoning the prompt
   asks for. `grep -rn 'route_media|stream_telemetry' docs/ infra/` returns only the
   deliberate "retired and can never appear" sentences plus the story manifest. No edit.
3. **"Name the one reason declared but unreachable until video." OFF BY ONE.** The code
   declares **two** (`partial_frame_discard`, `stream_rate_limited`). Both are named;
   naming one would advertise the other as a live detector.
4. **"The client catalog records that media-path metrics carry only `client_version` and
   `org_id`." FALSE AGAINST THE WIRE.** `mediaMetricLabels` returns three;
   `key_custody` is required by R-26. Written as two identity dimensions plus one fixed
   non-identity label.

---

## Accepted Deferrals

None — every reviewer finding was fixed in this changeset.

Pointers to task-sized debt recorded in `docs/TODO.md` — out of scope by the brief's own terms, not deferred findings against this diff:

- `docs/TODO.md` §Observability Debt — TypeScript-aware metric source for `dt-guard`.
- `docs/TODO.md` §Observability Debt — sweep enforcement for restated rosters and counts.
- `docs/TODO.md` §Test Debt — bcrypt bucket-introspection helper.
- `docs/TODO.md` — extending R-20 to assert `options.labels`; filed by `infrastructure`.

---

## Rollback Procedure

1. Start commit: `80945b22a84d1063654937ab05bd2b9886a63071`
2. `git diff 80945b22..HEAD`
3. `git reset --soft 80945b22` (preserves changes) or `--hard` (clean revert)
4. No schema changes.

### Remaining host-side actions (source edits are complete; these are not)

Deploying the dashboards to a **running** cluster takes two actions plus a readiness
wait. `apply` alone is a no-op for dashboard visibility:
`generatorOptions.disableNameSuffixHash: true` keeps the ConfigMap names stable so the
Grafana Deployment spec does not change and the pod does not roll, and the k8s-sidecar is
a `METHOD: LIST` **initContainer** that lists once at pod start and exits — it does not
watch.

```bash
kubectl apply -k infra/kubernetes/overlays/kind/observability/
kubectl rollout restart deployment/grafana -n dark-tower-observability
kubectl rollout status  deployment/grafana -n dark-tower-observability --timeout=300s
```

The `rollout status` is the readiness wait on the restart, not a third co-equal action —
it is there so the step fails loudly rather than returning before Grafana is back. See
`docs/LOCAL_DEVELOPMENT.md` §"Grafana Not Showing Dashboards".

`mh-slos.json` and `mh-overview.json` are independently revertible changes to **deployed**
artifacts: back either out on its own without touching the two new files.

---

## Issues Encountered & Resolutions

| Issue | Resolution |
|---|---|
| Three task-prompt clauses were already done or false against the tree | Verified each rather than executing it; recorded in §Superseded Task-Prompt Instructions |
| `dt-guard cross-boundary-scope` red on the classification table | The table is read as *the set of paths this diff touches*. Split rows with descriptive suffixes produced non-paths, and NO-EDIT rows produced `scope_drift_planned_untouched`. Restructured: table = edited paths only; declined edits moved to a prose list under the same heading |
| `metric_hygiene.rs` import paths | `QueryResponse`/`QueryData`/`QueryResult` live in `fixtures::metrics`, not re-exported at `fixtures` |

---

## Lessons Learned

### A confident reference to something the artifact does not contain — five instances in one task

No guard in the tree compares prose to the thing it describes. Every instance below was
caught by a human reading the artifact, or would not have been caught at all.

1. **Mine.** I justified narrowing `mh-slos.json` panel 7 by "the 30 ms objective
   threshold drawn across three series the objective isn't defined on." Panel 7 has
   `thresholdsStyle: {"mode": "off"}` and one null-valued green step. **No threshold is
   drawn at all.** Caught by @operations dumping the panel.
2. **Mine.** I classified `infra/grafana/kustomization.yaml` Mechanical partly on "forced
   bidirectionally by R-20." R-20 forces the basename listing and **never inspects
   `options.labels`** — the one part whose omission silently hides a dashboard. Caught by
   @operations, sharpened by @code-reviewer into a tier upgrade.
3. **Mine.** I recorded the host-side step as `apply -k` "plus sidecar pickup". **There is
   no pickup** — stable ConfigMap names do not roll the pod, and the sidecar is a
   one-shot `METHOD: LIST` initContainer. Caught by @infrastructure.
4. **A reviewer's.** @observability reported, in quotation marks, that
   `metrics.rs`'s `MediaLatencyPhase` doc carried a contested sentence. `grep` returns
   zero hits. **Caught only because @media-handler read their own file instead of
   accepting the relay** — otherwise I would have churned an already-careful doc comment
   on their say-so.
5. **The tree's.** `metrics.rs` carried "11 MH-local series rather than 22", stale since
   the 11→13 token growth, **in the file that defines the compile-checked
   `ALL: [Self; 13]`**.

**Two mechanisms, seen from opposite sides.** *The relay is where a confident wrong
reference gets laundered into an edit* (instance 4, from the implementer's side), and *a
reviewer's domain confidence is where their cross-domain errors come from* — @observability
was wrong three times in this Gate, each time applying a rule confidently one step outside
where they had actually read it, and every correction came from an owner rather than from
a check. That is the argument for the conditional-owner route @main opened: **owner-reads-own-artifact
is not a courtesy step, it is the only detector that exists.**

### A confident statement about an artifact, made without reading it back — and one was a *report*

Gate 2/3 extended the pattern above past five instances, and the last three sharpen it:

6. **@media-handler, panel 5.** A blanket "counted per edge" that my own catalog
   (`mh-service.md:618`) contradicts for `no_subscriber` (per frame). Fixed.
7. **My completion report claimed a `client-media.json` change that a crashed script
   never wrote — TWICE.** My first `client-media.json` edit set the dashboard-level
   `description` and the config-source cite, then hit a `KeyError` on the banner
   (`options.content`, not `content`) and **crashed before `json.dump`**, so neither
   was saved; a later banner-only script reloaded the file fresh. I then reported both
   as applied. @security and @observability independently caught the missing
   `description` by reading the file; @operations caught the missing config cite the
   same way — "grep, not report" — and each noted they had nearly accepted it on my
   word.

8. **@dry-reviewer, panel 8 — a correspondence stated as a count.** "Four of its
   five spellings match MH's `MediaDropReason`" is accurate today, but it is C-1 in the
   *one file with zero static guard coverage* (`dt-guard` never extracts a `dt_client_*`
   name) and with no forcing function reaching it — the SDK's `toEqual` test drags a
   developer to `mediaMetrics.ts`, never to this panel. Rewritten to name the exception
   (`not_connected`) and point at the roster's home, so it cannot rot on a sixth token.
   Caught only because the disclosure of instance 7 prompted @dry-reviewer to re-read a
   file their earlier verdict had already cleared.

**A clean verdict is itself a report, and it goes stale when the artifact moves under
it.** @dry-reviewer's Gate-2 sweep gave `client-media.json` a metric-name pass; F-3 would
have escaped even had the file been final, and was caught only by a re-read triggered by
instance 7's disclosure. Nothing in the review protocol pins a verdict to a commit, so a
post-verdict edit falls silently *inside* an ACK's apparent scope rather than visibly
outside it. Filed as process debt in `docs/TODO.md` §Process / Review-Protocol: a verdict
should name its as-of commit. This is the reviewer-side mirror of the report/artifact
class — the same mechanism, one level up.

**This is the more dangerous surface, and it is the reason the task made the
`dt_client_*` alert an inspection-by-reading obligation.** The other instances were
prose *in the tree*, where a later reader can still trip over them. A completion report
is preserved nowhere — it is caught only if a reviewer reads the artifact instead of the
message, which is exactly what happened three times here. The generalisable remedy is
the one the Gate-2 guard control already demonstrates for metric names and which I failed
to apply to my own edits: **read the artifact back after asserting something about it.**
A silent `json.dump` that never runs leaves the in-memory object correct and the file
unchanged, and nothing but a re-read distinguishes the two.

### Instance 5 is a different failure from the other four, and it changes the remedy

Story task 26 added the two tokens, and **that same commit** wrote `ALL`'s warning against
naming ordinals *and* deleted `mh-service.md`'s restated cardinality integer — then left
`:989` behind. This is not "nobody knew the rule". It is **a correctly-diagnosed class
with an incomplete sweep**, which means better prose is not the instrument: the *sweep*
wants a guard. Recorded in `docs/TODO.md` as the general form of the same gap the
label-roster half describes — one mechanism, two surfaces, one entry.

### A finding raised against its own author was the most valuable one

@observability's G1-9 landed *after* they had confirmed Gate 1, and it corrected a sentence
I had added at their own suggestion: that `mh-media.json` makes the SLI-eligible subset
"separable". It is **visible** there, not **available** — `p95(receive_buffer + processing)`
cannot be built from the two plotted quantiles, by the very quantiles-do-not-sum property
that justifies `total` existing. A sentence written to prevent an aggregation would have
induced a worse error: a story-8 author ratifying a number off a quantity that cannot be
computed, made authoritative by coming off an SLO-adjacent board.

### An investigation that inverted

G1-1 began as "the code drifted from the ratified SLI". Three artifacts later it was the
reverse: `slos.md` is the **2-to-1 outlier**, MH implemented exactly the span its own doc
describes, and the design is coherent — coherent around a span that cannot safely carry an
error budget. The lesson is about framing cost, not about who was right: writing it as a
code slip would have understated the problem *and* been unfair to the authors, and would
have produced a "fix the code" task where the real question is a ratification reserved for
story 8. My own first analysis argued from counting sentences; @observability's argued from
a ratified principle. **A principle beats a majority of sentences.**

### Empirical demonstration beat assertion twice

The `dashboard_metric_missing` positive control turned "the guard is green" into "the guard
sees these files and fires on them", and the matching negative control turned the
`dt_client_*` blind spot from a claim in a TODO note into a reproducible two-line
demonstration. Both were cheap. Neither would have happened without reviewers asking for
the *failing* output rather than the passing one.
