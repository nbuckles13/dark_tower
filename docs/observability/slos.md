# Service Level Objectives

**Owners**: Observability + Operations (jointly, per ADR-0011:40)
**Status**: Live register
**Related**: ADR-0011 (observability framework), ADR-0036 (media flow), `alert-conventions.md`, `alerts.md`

---

## Authority

**This file is authoritative for SLO targets.** ADR-0011:40 delegates "Current SLO targets" to this
path, and ADR-0011:104 defers to it explicitly. ADR-0011's own *Initial SLO Targets* table is the
**initial/historical** record of what that ADR proposed — retained for provenance, superseded here.
Where the two differ, this file wins.

That precedence is stated because it was previously undefined: this file was referenced by ADR-0011,
by `alerts.md`, by two runbooks and by `mh-alerts.yaml`, and **did not exist**, so ADR-0011's table
was the de-facto register by default while formally being labelled "initial". Two live target tables
with no stated precedence is a 3am failure.

### This register is authoritative but NOT yet complete

**An absent row means NOT-YET-RATIFIED INTO THIS FILE. It does NOT mean there is no objective.**
A live alert whose target is absent here is still a live alert, and its threshold is still enforced.

This file is authoritative for the targets it *carries*. It is not yet a complete inventory: SLO
targets are also asserted in `docs/observability/metrics/*.md`, `dashboards.md`, `alerts.md`,
`alert-conventions.md`, and in the dedicated `*-slos.json` compliance dashboards, and roughly sixteen
such restatements have **not** been reconciled into the register. Known live omissions, each backed
by a shipped artifact:

| Omitted objective | Live artifact |
|---|---|
| GC MC-assignment p95 < 20 ms | **Pages today** — `GCMCAssignmentSlow` (`gc-alerts.yaml`); `gc-slos.json` "MC Assignment SLO" panels |
| AC availability / error-rate | `ac-slos.json` — "Availability SLO – Error Budget Remaining", "Availability SLO – Uptime", "Error Rate SLO – Current" |
| MC heartbeat and token-refresh success | `mc-slos.json` — "GC Heartbeat SLO – Success Rate", "Token Refresh SLO – Success Rate / Current p99" |
| MH **GC-heartbeat RPC** latency p95 < 100 ms | `metrics/mh-service.md:51` (`mh_gc_heartbeat_latency_seconds`) — a control-plane RPC objective, unrelated to the media path |

**Stated explicitly because the precedence rule above would otherwise mislead.** "Where the two
differ, this file wins" is correct for a *carried* target and wrong for an *absent* one: an operator
paged by `GCMCAssignmentSlow`, consulting this register for the objective behind the 20 ms threshold
and finding no row, would conclude the page is an unratified guess when it is a shipped objective.
That inverts the triage.

This is the failure this file names two sections down — "an absent row is indistinguishable from a
decision not to have an SLO" — and until the backfill lands, the register is subject to it. Saying so
is the interim guard; **backfill is tracked in `docs/TODO.md` §Observability Debt (D8)**, owner
observability + operations, with the service specialists as per-target ratifiers.

**Do not close the gap by bulk-copying those literals into this register.** That promotes unratified
dashboard and catalog thresholds to fleet SLOs by clerical action — which is how the pre-amendment
`>30ms` MH figure became load-bearing in the first place. Each target needs per-objective
ratification with both owners signing off, per §Joint ownership below. D8 carries the full list.

### Joint ownership, stated because it is a failure mode

This file is owned by **observability and operations together**, which means it is nobody's sole
property and both parties can assume the other holds it. That assumption is part of why it went
unwritten for months (`docs/TODO.md` §Observability Debt records the case). Concretely: the **SLI
definition and measurement point** are observability's half; the **error budget, burn-rate posture
and paging consequence** are operations'. Neither half is complete alone, and a target lands only
when both have signed off.

---

## How to read an entry

Per ADR-0011's *SLO Structure (REQUIRED)*, every SLO defines five things. An entry missing any of
them is incomplete, and this file says so in the entry rather than omitting the row — an absent row
is indistinguishable from a decision not to have an SLO, which is precisely how a paging gap
survives behind a green dashboard.

| Field | Meaning |
|---|---|
| **SLI** | The metric being measured, and *where* it is measured |
| **Target** | The threshold |
| **Error budget** | Acceptable failure rate, derived from the target |
| **Measurement window** | 30 days unless stated |
| **Burn-rate alerts** | When budget consumption pages |

**Burn-rate policy** — **ADR-0011 owns this pair; the restatement below is a convenience copy and
ADR-0011 wins if they ever differ.** This file's precedence (above) covers SLO *targets*; a burn-rate
multiplier is fleet-wide *policy*, not a per-service target, so it is deliberately excluded from that
precedence and left with the ADR. Stated because the two encodings would otherwise disagree with no
named winner.

From ADR-0011, *Error Budget Burn Rate Alerts (REQUIRED)*, unchanged:

- **Critical / page** — >10× burn rate sustained 1 h (30-day budget exhausted in <3 days)
- **Warning** — >5× burn rate sustained 6 h (budget exhausted in <6 days)

See `alert-conventions.md` §Burn-Rate Alert Shapes for the rule shapes, and its
§Non-Zero-Denominator Guard for the conjunct every ratio-based rule needs.

---

## Live register

### Auth Controller

| Operation | SLI | Target | Window | Notes |
|---|---|---|---|---|
| Token issuance | p99 latency | < 350 ms | 30 d | Bcrypt ~250 ms + DB ~50 ms |
| Token validation | p99 latency | < 50 ms | 30 d | Signature verification only |

### Global Controller

| Operation | SLI | Target | Window | Notes |
|---|---|---|---|---|
| Request (regional) | p95 latency | < 200 ms | 30 d | DB query + routing |
| Availability | success ratio | 99.9% | 30 d | Encoded in the `GCErrorBudgetBurnRate*` rules as `/ 0.001` |

> **Known defect in the GC availability numerator** — its error class is `status_code=~"[45].."`, so
> client-attributable 4xx (an unauthenticated probe, a role denial, a guest hitting
> `allow_guests=false`) burn the availability budget and can page, even though the service is working
> correctly. Tracked in `docs/TODO.md`; changing a shipped SLO numerator moves the baseline and makes
> burn-rate history non-comparable, so it needs its own scoping rather than riding an unrelated change.

### Meeting Controller

| Operation | SLI | Target | Window | Notes |
|---|---|---|---|---|
| Session join | p99 latency | < 500 ms | 30 d | WebTransport handshake ~200 ms |

### Media Handler

| Operation | SLI | Target | Window | Notes |
|---|---|---|---|---|
| Media-path audio forwarding | p99 latency, ingress-read-complete → egress-enqueued | **OPEN — ratified in story 8** | 30 d | Measurement point decided; see below |
| ~~Audio jitter~~ | — | **STRUCK — unmeasurable** | — | See below |

---

## MH media-path forwarding SLO

### Decided: the measurement point

The SLI is the latency from **ingress-read-complete to egress-enqueued** — from the moment MH has
finished reading a datagram off the network, to the moment the rewritten datagram is enqueued for
transmit.

**What it deliberately excludes**, because MH does not control any of it: network transit in either
direction, client-side capture and encode, client-side jitter buffering and playout. What remains is
exactly the span MH is accountable for, which is the only span an MH objective can fairly assert.

This settles a real ambiguity rather than restating one. The predecessor objective (ADR-0011's
`MH | Audio forwarding | p99 latency | < 30ms`) had **no defined measurement point**, so it was
unfalsifiable — "forwarding latency" could name any of several spans, and any measurement could be
argued into or out of compliance. ADR-0036's amendments table requires the redefinition; ADR-0011
now carries it as a dated amendment.

The SLI is observed **decomposed into three phases** — receive buffer, processing/routing, transmit
buffer — because they have different remedies and an undifferentiated total does not tell an operator
which to pursue (ADR-0036 §11). A `total` phase exists **solely** to serve this objective: quantiles
do not sum, so the SLO number needs its own observation and cannot be reconstructed from the three
phases.

### Open: the target

**No target is ratified.** The `< 30 ms` figure in ADR-0011's historical table predates the
measurement point above and is therefore not ratified *against* it — a threshold and a measurement
point are a pair, and inheriting one without the other is how an unfalsifiable objective survives.

| Field | State |
|---|---|
| SLI | **Decided** (above) |
| Target | **OPEN** — ratified in **story 8** (performance: targets, benchmarks, tuning) |
| Error budget | **OPEN** — derived from the target; blocked on it |
| Measurement window | 30 days |
| Burn-rate alerts | Shapes decided (>10×/1 h page, >5×/6 h warning); **thresholds blocked on the target** |

Only the target and its derived error budget are unknown. Everything else is decided and written
above, so story 8 ratifies **one number** rather than re-litigating the alerting posture from a blank
table.

> **No MH burn-rate alert may be authored until the target is ratified.** Cargo-culting a 10×/5×
> pair without an authoritative target produces a threshold with no principled justification. This
> is the same rule, in the same words, as the deliberate-omission note in
> `infra/docker/prometheus/rules/mh-alerts.yaml`. One rule, one source: if this statement and that
> comment ever disagree, this file is authoritative and the comment is the copy to fix.

### Where the number will live when it lands

When story 8 ratifies the target, its single source of truth is a **named constant in
`crates/mh-service/src/observability/metrics.rs`**, placed beside the forward-latency histogram's
bucket slice, with a unit test asserting the constant equals one of the bucket edges — so a later
change to the figure cannot silently drift the objective into a bucket interpolation.

**That constant does not exist yet.** It lands with the MH forward path (story task 16), together
with the forward-latency histogram itself and the sample-ratio gauge. Until then this section is a
forward reference, not a citation: do not expect to `grep` it in the current tree.

Likewise the **sample ratio**. The histogram is observed one-in-N with **random** sampling — never
per-stream deterministic, which would reconstruct the voice-activity trace ADR-0036 §11 prohibits —
and the ratio is published as a gauge reading the same configuration value the sampler reads, so the
published ratio cannot drift from the applied one. Gauge and config key land in task 16; see
`dashboard-conventions.md` §Periodicity.

---

## Struck: MH audio jitter

The ADR-0011 objective `MH | Audio jitter | p99 | < 20ms` is **struck as unmeasurable**, per
ADR-0036's amendments table.

MH **forwards and does not buffer**. It has no jitter buffer whose behaviour could be observed.
Perceived jitter is a property of the **client-side** jitter buffer, and jitter-buffer design is out
of scope for this system today. An objective that no component can measure is worse than no
objective: it reads as coverage, it cannot fail, and it produces dangling references — the
`MHHighJitter` example in ADR-0011's alert-naming convention was one, now corrected.

A client-side successor is deferred together with jitter-buffer design. No metric named `*jitter*` is
emitted by any service, and no dashboard panel or alert rule references one.

---

## Join-to-first-media: an objective, never a gate

The headline user-facing measure of the media path is **join to first media received**. It is an
**objective**, and it is **never a quality gate**. This is a decision (ADR-0036 §10), not an
omission, and the reasoning is recorded here because the natural instinct is to assert it in a test:

1. A wall-clock end-to-end assertion on a local cluster is a **permanent flake** — shared CI
   hardware, contended schedulers, and a real network stack put the tail outside any threshold tight
   enough to be meaningful.
2. Under ADR-0028's flaky-test policy a flake is quarantined within 24 h and then **fixed or deleted
   within a sprint**. A permanent flake is by definition unfixable.
3. So it exits quarantine by **deletion** — and the headline objective ends with **zero** durable
   coverage, which is strictly worse than never having gated it.

The number is therefore **observed, not asserted**: the client records first-media-received, and MH
records the decomposed forward latency above.

### What *is* gated, so "never a gate" is not read as "unverified"

The media path's correctness is gated **structurally** — on invariants that are deterministic and do
not depend on wall-clock timing. The story-1 instance (requirement R-2) is the clearest:

> Client mute stops all media leaving the device within one frame, **asserted at the client transport
> seam on egress count staying flat** — never by absence of audible output.

That is a counter assertion, not a timing assertion: it is deterministic, it cannot flake on a loaded
machine, and it fails loudly when broken. The general rule this expresses — **gate the structural
invariant, observe the wall-clock number** — is why the objective can safely be non-gating. See
ADR-0036 §10 for the full tier breakdown (Tier 1a pure-function gates, Tier 1b control-plane and
forward-path gates, Tier 2 regression deltas, Tier 3 non-gating objective).

---

## Open items

| Item | Blocked on | Owner |
|---|---|---|
| MH media-path forwarding **target** and its error budget | Story 8 (performance) | observability + operations |
| MH burn-rate alert pair | The target above | media-handler (rule), observability + operations (reviewers) |
| `MCRegisterMeetingFailureRate` re-assert-failure alert | The MH target, then the handler-restart story | meeting-controller (rule), media-handler + observability + operations (reviewers) |
| GC availability numerator counts client-attributable 4xx | Own scoping — moves a shipped baseline | observability |
| No SLO for the MC→MH control plane's applied-generation echo | Handler-restart story | observability + operations |

Debt and rationale for each: `docs/TODO.md` §Observability Debt.
