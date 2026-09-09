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

> **CONTESTED — see §Open: which series the objective attaches to, below.** The end boundary
> stated in this paragraph is the 2-to-1 outlier against `ingress.rs` and `metrics.rs`, and whether
> the objective attaches to `total` is open, not decided. Read the open item before ratifying anything
> against this measurement point.

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
which to pursue (ADR-0036 §11). A `total` phase exists to serve this objective: quantiles
do not sum, so the SLO number needs its own observation and cannot be reconstructed from the three
phases. **(That `total` is the *right* series for it is CONTESTED — `total` is client-influenced via
`transmit_buffer`; see §Open: which series the objective attaches to.)**

### Open: the target

**No target is ratified.** The `< 30 ms` figure in ADR-0011's historical table predates the
measurement point above and is therefore not ratified *against* it — a threshold and a measurement
point are a pair, and inheriting one without the other is how an unfalsifiable objective survives.

| Field | State |
|---|---|
| SLI | Measurement point **decided**; **which series carries the objective is OPEN** — see below |
| Target | **OPEN** — ratified in **story 8** (performance: targets, benchmarks, tuning) |
| Error budget | **OPEN** — derived from the target; blocked on it |
| Measurement window | 30 days |
| Burn-rate alerts | Shapes decided (>10×/1 h page, >5×/6 h warning); **thresholds blocked on the target** |

Only the target and its derived error budget are unknown. Everything else is decided and written
above, so story 8 ratifies **one number** rather than re-litigating the alerting posture from a blank
table — **with the second open item below settled first, because it decides which series that number
attaches to.**

### Open: which series the objective attaches to — and the SLI-eligible span has no series today

**Read this before ratifying the target.** It supersedes the "A `total` phase exists **solely** to
serve this objective" sentence in the §Decided block above, which is now contested rather than
settled.

**The measurement point stated above is the outlier, two-to-one, against the artifacts.** The end
boundary is written as "the moment the rewritten datagram is **enqueued for transmit**". Two other
artifacts say otherwise, and they agree with each other and with the code:

| Artifact | End of the span |
|---|---|
| `crates/mh-service/src/media/ingress.rs` (`MediaLatencyPhase::Total` recording) | `sent_at`, taken **after** `transport.send_datagram()` returns — sent to network |
| `crates/mh-service/src/observability/metrics.rs` (`MEDIA_FORWARD_OBJECTIVE_SECONDS` doc) | "ingress-from-network to **egress-to-network**" |
| **This file, above** | "**is enqueued for transmit**" |

So this is **not a slip in the code that drifted from a ratified SLI** — the framing that first
suggested itself, and the wrong one. The *intended* span was always ingress-read-complete →
sent-to-network, MH implemented exactly the span its own doc describes, and this document's narrower
sentence is the odd one out. Stating it the other way round would both understate the problem and be
unfair to the authors of a coherent design.

**The problem is what that intended span contains.** `total` spans all three phases, so it includes
`transmit_buffer` — egress-push → send-returned — which MH's own artifacts describe as *"a slow
subscriber"*. A subscriber's behaviour therefore lengthens it. That makes `total`
**client-influenced by construction**.

**This is an EXTENSION of the doctrine below, not a citation of a rule already on the books.** The
`transport_receive_dropped` blockquote in the next section bars *that metric, by name*, and states
the generalizable reasoning: a client-inflatable SLI converts an availability attack into an
**error-budget attack**, and if that budget ever gates a release, into a **client-controllable deploy
block**. Nothing currently written bars `total`. Applying the doctrine to it is a judgement this
document is making here, explicitly, so that a reader is not left believing the rule was already
written down. The §Decided exclusion list — "what it deliberately excludes, because MH does not
control any of it" — points the same way.

**And the SLI-eligible subset cannot be observed today, which is the part most likely to be
missed.** The eligible span is `receive_buffer` + `processing`: MH-internal, not client-influenceable.
Those two phases are *visible* as separate series on the "Media Handler - Media Path" dashboard —
but they are **not available** as the quantity an objective needs. `p95(receive_buffer + processing)`
cannot be constructed from `p95(receive_buffer)` and `p95(processing)`, by the very
quantiles-do-not-sum property that justifies a `total` phase existing at all. So:

> **No series measures the SLI-eligible span. Not `total` (too wide, client-influenced), and not any
> combination of the three phases (quantiles do not sum). Do not ratify a number off the two
> plotted phases — the quantity is not computable from them.**

**Two candidate resolutions, both for story 8:**

1. **Narrow `MediaLatencyPhase::Total`'s recording to end at the egress-queue push** (`queued_at`),
   making it the eligible span and creating the series that does not exist today. This is an MH
   hot-path change, owner `media-handler`, and is **not** in the loopback story.
2. **Re-ratify the measurement point to include `transmit_buffer`** and accept the client influence.
   This is what the tree already reflects, and it is what the `transport_receive_dropped` precedent
   argues against.

**Blast radius of resolution 1** — listed so a narrowing does not silently falsify prose that is
accurate today. Both go false *together*, which is why they are named in one place:

- `crates/mh-service/src/media/ingress.rs` — the `MediaLatencyPhase::Total` recording itself.
- `crates/mh-service/src/observability/metrics.rs` — `MEDIA_FORWARD_OBJECTIVE_SECONDS`'s
  "ingress-from-network to egress-to-network". **Correct today**, which is exactly why it would rot
  unobserved; naming it here is the protection, and it is deliberately *not* edited now.

`transmit_buffer` is **not** made useless by any of this. It keeps the same carve-out this file
grants `transport_receive_dropped`: survivable for a `warning` alert with a rate and a sustained
window, not survivable in an SLI. Do not over-correct by deleting the phase.

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

**The constant landed with the MH forward path (story task 16)**, together with the forward-latency
histogram and the sample-ratio gauge. This section is now a **citation, not a forward reference**:

- `crates/mh-service/src/observability/metrics.rs::MEDIA_FORWARD_OBJECTIVE_SECONDS` — **0.030,
  documented in code as PROVISIONAL pending story 8**, sitting beside
  `MEDIA_FORWARD_LATENCY_BUCKETS`, the same slice `set_buckets_for_metric` registers.
- The bucket-edge assertion exists in both tiers:
  `metrics.rs::tests::media_forward_objective_is_exactly_a_registered_bucket_edge` and
  `crates/mh-service/tests/media_metrics_integration.rs::the_forwarding_objective_is_exactly_one_of_the_registered_bucket_edges`,
  the latter also asserting the slice is non-degenerate and strictly ascending — a `contains` over an
  empty slice passes vacuously.

> **An empty histogram is not a fast one, and on this SLI the two look
> identical.** `mh_media_forward_latency_seconds` records only when frames are
> actually forwarded, so a quantile over an idle or a broken handler is *empty*
> rather than comfortably inside the objective — a distinction that matters most
> on the SLO dashboard, which is the one that gets screenshotted into a status
> update. **Check occupancy before reading the quantile**: `_count` moving at all
> is the precondition, and `mh_media_session_starts_total` then
> `mh_media_frames_dropped_total{reason="transport_receive_dropped"}` name the
> cause when it is not. Stated once in full at
> `docs/observability/metrics/mh-service.md` §Media Forward Path.
>
> (This blockquote previously asserted that MH "declines to start its media tasks
> pending the participant → `sender_id` binding" and therefore reads no data on
> any production pod. That ceased to be true when the binding contract landed at
> story task 24, and nothing detected the staleness — the clause is not derived
> from anything and no guard covers prose. The permanent, mechanism-independent
> half is kept above; the era-specific half is deleted rather than re-dated.)

**The presence of the constant does not ratify the target.** 0.030 is the ADR-0011 figure carried
forward as a placeholder so the mechanism has something to hold; the table above is unchanged and
story 8 still ratifies the number. **No burn-rate alert may rest on it**, per the rule above, and
none ships.

> **`mh_media_frames_dropped_total{reason="transport_receive_dropped"}` is NOT SLI-eligible, and
> this is a hard constraint rather than a current-data caveat.** It is an **upper bound** on
> datagrams MH never read, not a measurement of them: it counts every DATAGRAM frame quinn decoded
> and MH did not consume, which includes frames `wtransport` discarded for a WebTransport
> session-id mismatch. **The value is therefore client-influenceable** — an authenticated client
> can raise the series at will.
>
> That property is survivable for a `warning` alert with a rate and a sustained window, which is
> where it is used. It is **not** survivable in an SLI. A client-inflatable SLI converts an
> availability attack into an **error-budget attack**, and if that budget ever gates a release, into
> a **client-controllable deploy block** — an outside party acquiring a veto over shipping.
>
> Recorded here, and not only in the alert rule that respects it, because story 8's author ratifies
> the forward-latency objective while standing in this file and would otherwise be one hop from a
> constraint they had no reason to go looking for. The full derivation is at
> `docs/observability/metrics/mh-service.md` §Media Forward Path, limitations 2 and 3.

Likewise the **sample ratio**, also landed:
`MH_MEDIA_LATENCY_SAMPLE_RATIO` (optional; default `mh_service::config::DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO`, cited rather than restated) is read once into
`Config::media_latency_sample_ratio`, the sampler is constructed from that field, and
`mh_media_latency_sample_ratio` publishes **that same field** — so the published ratio cannot drift
from the applied one, and a component test asserts the equality. The sampling is **random per frame,
never per-stream deterministic**, which would reconstruct the voice-activity trace ADR-0036 §11
prohibits; a unit test asserts two samplers fed identical arrival patterns produce different sample
sets. See `dashboard-conventions.md` §Periodicity.

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
