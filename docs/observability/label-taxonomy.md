# Label Taxonomy

Conventions for authoring metric labels under ADR-0011 and ADR-0031.

This document covers: shared label names, the PII denylist, cardinality
budgets, the bounded-label pattern, the `# pii-safe: <reason>` escape hatch,
label-cardinality-aware metric design, and the machine-enforced vs
reviewer-only rule index.

Ownership: service specialists own the metric definitions in
`crates/<svc>-service/src/observability/metrics.rs`; observability and
security **co-own** this document and the guard that enforces it. Extensions
to the PII denylist land via a PR touching this file AND
`crates/dt-guard/src/common/pii_vocabulary.rs` together — the vocabulary lives
there, not in `scripts/guards/simple/validate-metric-labels.sh`, which has been
a thin dt-guard wrapper since ADR-0034 §3.

**Authoritative ADRs**:
- [ADR-0011](../decisions/adr-0011-observability-framework.md) — metric
  taxonomy, cardinality budgets, SLO framework.
- [ADR-0031](../decisions/adr-0031-service-owned-dashboards-alerts.md) —
  service-owned alert/dashboard/metric authorship; this doc; the guard.

**Machine enforcement**: `scripts/guards/simple/validate-metric-labels.sh`
(a wrapper; the matcher is `crates/dt-guard/src/metric_labels.rs` and the
vocabulary is `crates/dt-guard/src/common/pii_vocabulary.rs`)
runs on every CI pipeline. Rules in this document are tagged
`[guard-enforced]` or `[reviewer-only]`; see the rule index at the end of
this document for the full enforcement matrix.

---

## Purpose & Scope

Labels are the dimensions along which a metric is queryable. They are also
the primary driver of Prometheus cardinality, and the primary vector for
unintentional PII leakage into a metrics backend that is typically NOT
subject to the same retention and access controls as a database.

This taxonomy exists to:

1. **Prevent PII leakage** by denying PII-flavored label keys and flagging
   obvious PII-flavored label values — without relying solely on reviewer
   vigilance.
2. **Keep cardinality bounded** so that the fleet-wide 5M-series budget from
   ADR-0011 §Cardinality Budget is not blown by a single careless label.
3. **Standardize shared label names** across services so that cross-service
   dashboards (`service_type="mc-service"` in one place,
   `svc_type="mc"` in another) aren't fighting the data model.

Scope: metric labels emitted via the Rust `metrics` crate's `counter!`,
`gauge!`, `histogram!`, `describe_counter!`, `describe_gauge!`, and
`describe_histogram!` macros. Log/trace attributes are covered by the
[no-pii-in-logs](../../scripts/guards/simple/no-pii-in-logs.sh) guard and
tracing conventions, respectively — those are separate concerns with the
same underlying principle.

---

## Shared Label Names `[reviewer-only]`

When a label represents the same concept across services, it SHOULD use the
same canonical name. This is **reviewer-only** — the guard does NOT flag
drift aliases. The choice to keep this reviewer-only (rather than machine-
enforce it) was made deliberately: historically the fleet had drift, and
machine-enforcing would have required a grandfather allowlist whose cost
exceeded the benefit. All previously-tracked drift was resolved through
ADR-0031 FU#3a/b/c (2026-04-18); the reviewer-only posture remains in
case future drift emerges from new services or metric additions.

| Canonical name | Meaning | Bounded values |
|---|---|---|
| `service_type` | Which service is emitting / being called | `ac-service`, `gc-service`, `mc-service`, `mh-service` (or short forms `ac`/`gc`/`mc`/`mh`) |
| `method` | RPC or HTTP method | HTTP verbs (`GET`, `POST`, ...) or gRPC method names (bounded by `.proto`) |
| `status` | Coarse outcome classification, compared **across** services | `success`, `error`, `timeout`, `rejected`, `accepted` |
| `outcome` | Fine-grained result of a single operation, where **each value names a distinct remedy** | Closed enum **per metric**, defined in that metric's catalog entry — deliberately not enumerated here |
| `status_code` | Raw HTTP status code | 200-599, bounded (~60 distinct) |
| `endpoint` | Semantic HTTP path (normalized) | Bounded by `normalize_endpoint()` table |
| `operation` | Subsystem-specific verb | Bounded by code (e.g., `select`, `insert`, `update`, `delete`) |
| `error_type` | Bounded error variant | Bounded by the service's `error_type_label()` enum |
| `error_category` | Coarser error class | `authentication`, `authorization`, `cryptographic`, `internal` |
| `event_type` | Bounded event discriminator | Bounded by the event enum (e.g., `connected`, `disconnected`) |
| `action` | **What operation a request asked for** — the verb, never its result | Closed enum **per metric**, defined in that metric's catalog entry — deliberately not enumerated here |
| `region` | Geographic/deployment region | Bounded by cloud region set |
| `pod` | Kubernetes pod identifier | Per-pod — cardinality bounded by fleet size |
| `key_custody` | Who holds media key material | **`operator` — single permitted value.** See §Key custody below. |
| `reason` | Why a **frame** was dropped on the media path | **ONE label space, TWO families, each with exactly one home.** (a) The *codec* family is bounded by `proto/test-vectors/frame-v2.vectors.json` → `reject_reasons`; (b) the *relay transport-and-routing* family is bounded by `mh_service::observability::metrics::MediaDropReason`. **Points, never restates** — restating either set here would give each token two homes and they would drift. Distinct from `error_type` / `error_category`, which classify why a **service operation** failed: `reason` is per-frame and lives entirely on the media path. See §Frame reject reason and §Media-path transport refusals below. |
| `mode` | Capture source of the LOCAL send path — what feeds this client's encoder | `microphone` (also covers an embedder-injected capture factory), `test_tone` (only the define-gated built-in tone) — a closed enum declared as the type `MediaCaptureSourceMode` in `packages/sdk-core/src/media/setup/mediaMetrics.ts`, carried only by `dt_client_media_capture_source`. **Datapoint-only** (GC `MEDIA_DATAPOINT_EXTRA`, collector `keep_keys`; never on spans or exemplars). **Identity-free**: the value is a build-time property of the bundle (`__DT_TEST_TONE__`), not per user, participant or meeting. The token `test_tone` exists in a bundle only inside the define's gate. |

**`status` vs `outcome`.** Use `status` when the values are the coarse, shared
set above and a responder compares them across services. Use `outcome` when the
value set is *metric-local* and each value points at a different fix — the
per-metric enum then lives in that metric's catalog entry, which is where a
responder reading the series is already looking. Borrowing `status` for a
divergent value set is what this section exists to prevent, since the whole
point of a canonical name is that one selector means one thing fleet-wide.

`outcome` is recorded here as an **existing** convention rather than a new one:
`ac_meeting_token_display_name_total`, `mc_join_display_name_resolved_total` and
`mh_media_policy_applies_total` all emit it, and `internal.proto` names MC's
counterpart of the last of those `outcome` too. The row was missing, which is
the gap this section's own "add before the second service" rule is meant to
close.

**`outcome` is a per-metric vocabulary and carries no cross-metric aggregation contract** `[reviewer-only]`. Unlike `reason`, whose two families have named external homes precisely so `sum by(reason)` compares across the two ends of one hop, `outcome`'s value set is metric-local by definition — so `sum by(outcome)` across two metric names is meaningless by design, and no dashboard panel or alert expression may aggregate `outcome` across metric names. Asked twice in one devloop (story 2 task 7, by test and by the implementing specialist) and recorded here rather than re-decided per task. Nothing enforces this mechanically today; a candidate guard shape and its trigger are filed in `docs/TODO.md` §Guard Coverage Gaps.

**`action` vs `outcome`.** `action` names *what was asked for*; `outcome` names *what happened*. This row **describes usage already in the tree** rather than defining a label for a new metric: `dt_client_media_mute_transitions_total{action}` already carries `{mute, unmute}` for the transition a local user performed at capture (`infra/grafana/dashboards/client-media.json:918`, `docs/runbooks/client-dev-local.md:1283`), and `mc_media_server_mute_requests_total{action}` carries the same values for the verb a host requested. The label names the same *kind* of thing fleet-wide — a requested verb; it does **not** name the same value set, and it says nothing about who decided. The metric name carries both of those. So, exactly as for `outcome`: **`action` is a per-metric vocabulary and carries no cross-metric aggregation contract** `[reviewer-only]` — `sum by(action)` across two metric names is meaningless by design, and no dashboard panel or alert expression may aggregate `action` across metric names. Registered here because MC is the second service to carry it under this meaning, which is what §Adding a new shared label fires on.

**The two conforming users have byte-identical value sets and are NOT a join key — name the pair, do not rely on the general contract above to cover it.** `dt_client_media_mute_transitions_total{action}` and `mc_media_server_mute_requests_total{action}` both emit exactly `{mute, unmute}` about the same user-visible concept, and they are disjoint by construction: **different decider** (the participant's own capture-side toggle vs a host's moderation action), **different enforcement point** (client capture, ADR-0036 §5, vs MH ingress, §7), **different remedy**. That is the same three-way distinction that keeps `mc_media_server_mute_requests_total` separate from `mc_media_mute_requests_total`. Identical values are an invitation in the autocomplete where the contract above is a rule a reader has to go and find, so a "mute activity" panel built from both series silently sums self-mutes with moderation events. State the correspondence by naming these members; never by restating a count.

**`ac_rate_limit_decisions_total{action}` does not satisfy this definition and is not blessed by it.** Its values are `{allowed, rejected}` — a decision reached, which is `outcome`'s meaning — and its own catalog row calls them "Rate limit decision". It predates this row, and after MC lands it is **one non-conforming instance against two conforming ones**. It is recorded under §Non-canonical aliases and migrates under that section's coordinated-migration process; it is **not** evidence that `action` may carry result values, and a new metric must not cite it as precedent. Widening this row to accommodate it was considered and rejected: a definition covering both "what was asked" and "what happened" excludes nothing, which would make `action` the bucket a new metric reaches for instead of choosing between `operation` and `outcome`.

### Declared, not yet carried: `media_kind` and `content_kind` `[reviewer-only]`

Registered here **at declaration** rather than at first emission, because §Adding a new
shared label fires on intent: both are multi-service by construction — MH, MC and the
client SDK will all carry them — so the "add here before the second service" rule
applies now, not in story 3.

| Label | Bounded values | Emitted today? | Lands with |
|---|---|---|---|
| `media_kind` | `audio`, `video` | **No** | Story 3 (video) |
| `content_kind` | `main`, `slides` | **No** | Story 3 (content share) |

Both value sets are **closed and enumerable**, which is the property that makes them
admissible: each is a small fixed set fixed by the media pipeline's own type system, not
a free-form string, so neither can grow unboundedly at runtime. Adding a value is a
protocol change, not a call-site decision.

**They are declared, not carried.** No metric emits either label today. Do not write a
dashboard query or an alert selecting on them — an unmatched selector yields an empty
series rather than an error, so it fails silently. Which metrics will carry them is
recorded in the per-service catalogs (`docs/observability/metrics/*.md` §Media-path
label conventions), which is the home for *which labels a metric carries*; this section
is the home for *what the label is and what values it may take*.

### Non-canonical aliases (flagged by reviewers, not the guard) `[reviewer-only]`

These aliases SHOULD be renamed toward the canonical form during a
coordinated migration (dashboards + alerts + metrics.rs + Alertmanager
config land together). New metrics MUST use the canonical form.

| Alias (avoid in new code) | Canonical |
|---|---|
| `svc_type`, `servicetype`, `service_kind` | `service_type` |
| `http_method`, `verb`, `rpc_method` | `method` |
| `http_status`, `httpstatus`, `statuscode`, `status_num` | `status_code` |
| `aws_region`, `gcp_region`, `datacenter` | `region` |
| `pod_name`, `podname`, `pod_id` | `pod` |

**Value-set drift, as distinct from a name alias**: `ac_rate_limit_decisions_total{action="allowed"|"rejected"}` uses the canonical label *name* with `outcome`'s value semantics. The migration is `action` → `outcome` on that one metric. **Seven coupled artifacts must land together**, which is why it is task-sized rather than a rename: the recording site (`crates/ac-service/src/observability/metrics.rs:171`), the zero-init value array (`:519` — a second in-code home for the value set), two in-module test assertions (`:875`, `:878`), 12 occurrences across `crates/ac-service/tests/rate_limit_metrics_integration.rs`, the catalog row (`docs/observability/metrics/ac-service.md:106-111`), the dashboard expression (`infra/grafana/dashboards/ac-overview.json:2537-2538`) and the runbook selector (`docs/runbooks/ac-service-incident-response.md:326`). Owner: `auth-controller`. Not in scope for the devloop that added the `action` row — flagged there and recorded here so the row's definition is not read as retroactive blessing.

### Adding a new shared label

Shared labels are added here BEFORE they're used in a second service. If
you're about to introduce a label that another service will eventually emit
— stop, add it here first. Mark the bounded-values column if the set is
enumerable; otherwise document the bounded-label pattern you're using
(see below).

### Service-local labels `[reviewer-only]`

Labels that exist only in one service (e.g., MC's `actor_type`,
GC's `controller_type`, MH's `grpc_service`) do not need a taxonomy entry,
but must follow all other rules (snake_case, bounded, no PII).

- **MC's `trigger`** (`mc_meeting_kek_generated_total`, story 2 task 9) — what
  caused a KEK to be generated: `meeting_created` | `participant_left` |
  `sender_space_exhausted` (`media_admission::RotationTrigger::ALL`). Recorded
  here because two neighbours are close enough that a later "cleanup" would
  reach for them, and both would be wrong: `event_type` is already bound to
  connect/disconnect event enums in MC and MH (reusing it breaks "one selector
  means one thing fleet-wide"), and `reason` is reserved for the per-frame
  media-path drop families and must not acquire a third home.

---

## Key custody `[reviewer-only]`

**Key custody is a label, not a boolean** (ADR-0036 §11).

Every service reports **`key_custody=operator`** in logs and metrics — ADR-0036 §11's wording,
unqualified and deliberately so. This is **not** scoped to the media path: the question "what does
this deployment claim about key custody?" must have an answer at every emission site, and an AC or GC
log line with no custody label leaves it unanswered.

| | |
|---|---|
| **Permitted values** | `operator` — **one value, and adding another requires an ADR-0036 §4 amendment.** |
| **Why single-valued** | This is a *constraint*, not a snapshot of today's deployment. The label exists to make the custody posture explicit at every emission site; a second value may only appear when §4's custody model actually changes. |

### No end-to-end or zero-trust boolean, anywhere

**No metric, log, dashboard, or document may carry an end-to-end or zero-trust boolean**, because the
default deployment is neither.

What is true, in both directions (ADR-0036 §4): media is **encrypted between clients**; MH, the
network, and storage **cannot** read it; **MC — and therefore the operator — can**. That is operator
custody, accepted as the user's risk decision. A keyless relay does **not** make the system
end-to-end.

A boolean cannot express that, and the failure mode is not a subtle one: a stat panel reading
`E2E: true` is a **product claim rendered to an operator**, who may repeat it to a customer. The
label carries the truth; a boolean would carry a claim.

> **Scope of this prohibition** — it covers claims of end-to-end **encryption** or **zero-trust** as
> *deployment properties*. It does **not** touch end-to-end **latency**, which is an ordinary
> measurement term: "join-to-first-media is an end-to-end objective" is correct usage and is
> unaffected (see `slos.md`).

> **Carrier list note** — ADR-0036 §11 states "metric, log, or document". This taxonomy additionally
> names **dashboards** explicitly. That widening is deliberate and is a ratchet (strictly stricter,
> relaxes nothing), recorded here so a future reader does not take the extra carrier for a
> transcription error and "correct" it back down. Dashboards are named because a Grafana panel is the
> most likely place someone renders a custody boolean, and because a dashboard JSON is not obviously
> a "document" to the person adding a panel.

**Canonical home**: this section. `docs/API_CONTRACTS.md` states the contract-level accuracy claim and
the prohibition for integrators; ADR-0036 §11 is the decision record. On divergence about the *label
mechanics* — name, permitted values, which surfaces carry it — this file wins.

---

## Frame reject reason `[reviewer-only]`

**All eighteen `reason` values are emitted individually as `reason` values, without exception
(R-25, R-31). Nothing below bears on that.** This section is where someone will come looking for
permission to coarsen the vocabulary, and everything after this sentence is about a different axis.
R-25 already settled the per-token question on 2026-08-31: *"'bucket' names a FAMILY of `reason`
values, not one collapsed `decode_reject` label"* — collapsing would destroy R-31's only lever, since
`unknown_version` staying individually visible is how a version-skewed rollback is detected, and
would break `sum by(reason)` comparability between MH and the client.

**"Without exception" above is about granularity, not about membership of the drop counter. Those are
different questions and only one of them has the answer "all eighteen".** Which values belong on
`dt_client_media_frames_dropped_total{reason}` is carried by **`drops_frame`** in
`proto/test-vectors/frame-v2.vectors.json` → `reject_reasons`. Seventeen are `true`.
**`wrap_key_id_mismatch` is `false`, and is the only one**: it describes a correctly self-signed frame
whose unusable wrapped key the receiver ignores (ADR-0036 §4), so the frame is otherwise processed
and **played**. (The token *name* says mis-bound; the receiver cannot actually tell a mis-bound wrap
from a wrong KEK, and the realistic production cause is a KEK-generation skew — see the
`wrap_key_id_mismatch` bullet under §The discriminator below before triaging on it. That does not
change its `drops_frame` status, which is what this paragraph is about.)
Counting it as a drop breaks R-25's `received = played + sum(drops)` for a frame that
*was* played — an identity that fails silently, only in aggregate, and long after the label set is
frozen. Stated here rather than left to the array for the same reason this section gives about R2:
**the reader who needs the constraint is the one least likely to go read the other artifact first.**

**Separately: no `reason` value may be joined with a participant, meeting, or stream dimension.**
Today that holds for every token under R2's voice-activity-trace argument. Where a token's firing
depends on **receiver-held state** — key material, roster contents, key-store membership — a second
and independent argument also applies: for a party who can inject frames, the counter is an **oracle
over the receiver's key state**, and the frame's author already knows every byte-determined outcome,
so only the state-dependent ones tell them anything new.

**The discriminator is byte-determined versus receiver-state-dependent — *not* codec-versus-crypto,
and not `has_vector`.** Those are all *nearly* the same distinction and none of them is it:

- The eight structural rejects are **byte-determined**.
- `no_kek_for_generation`, `kek_generation_stale`, `no_roster_entry`, `no_transmit_key`,
  `signature_invalid`, `decrypt_failed`, `unwrap_failed`, `replay_detected`,
  `wrap_key_id_mismatch` and `sender_not_assigned` are **receiver-state-dependent**.
- **Eight and ten is all eighteen: the two families PARTITION the vocabulary.** (Sixteen until story 2
  task 7 added `kek_generation_stale` and `sender_not_assigned`, both receiver-state-dependent.)
  Stated because an earlier revision of this list classified fourteen and left `replay_detected` and
  `wrap_key_id_mismatch` in neither — and an unclassified token is not read as unclassified, it is
  read as byte-determined and therefore safe to slice, which is the permissive answer arrived at by
  omission. If a token is added to `reject_reasons`, it lands in one of these two families here or
  this section is wrong.
- `no_transmit_key` is `layer: "codec"` yet fires on key-store membership, so an attacker choosing
  key ids reads it as *"does this receiver hold key id X?"* — which is why the layer-based version of
  this rule was wrong. The new `layer: "assignment"` value happens to align with this family, and
  that alignment is a coincidence of naming rather than a rule. The layer axis still names the
  receiver's **processing stage** (`codec` = the bytes, `crypto` = the verification, `key` = the held
  key material, `assignment` = the held slot-assignment set), and `no_transmit_key` remains the
  standing counterexample. Classify by this section, never by `layer`.
- `signature_invalid` and `decrypt_failed` are byte-determined *given fixed key material*, so they
  carry `has_vector: true` while sitting on the restricted side — which is why the `has_vector`
  version is wrong too.
- `replay_detected` fires on the sliding window, which is receiver state by definition: a party
  replaying a captured frame reads it as *"has this receiver already advanced past sequence N?"*
- `kek_generation_stale` fires only when the receiver no longer retains the frame's generation, so a
  party replaying a captured pre-rotation frame reads it as *"is this receiver still retaining
  generation N?"* — and because retention is derived from W, a positive answer also times the
  receiver's position relative to the last rotation. Aggregate counting with no sender dimension is
  what keeps that from being a per-sender oracle, exactly as for `replay_detected`.
- `sender_not_assigned` is decided entirely by receiver-held slot-assignment state, so an injected
  frame from sender X reads as *"does this receiver hold sender X in its assignment set?"* It is the
  clearest member of this family: the frame parsed, verified **and** decrypted before the gate ran,
  so nothing about the bytes selects it.
- `wrap_key_id_mismatch` is the subtlest of the set, and the one whose **name points away from its
  firing condition**. The receiver cannot detect a mis-bound wrap. The wrap's bound key id is nowhere
  on the wire — the key-bearing block is `kek_generation`, 32 wrapped bytes and a 16-byte tag, and
  the binding exists only as the seal-time AAD — so the unwrap site observes one bit, a GCM tag
  mismatch, exactly as it does for a wrong KEK. What selects this token over `unwrap_failed` is
  **receiver key-cache state**: the unwrap failed *and* a usable transmit key for that key id is
  already held, so the frame plays anyway. Two consequences, and the second is the operational one.
  (1) It sits on the restricted side above, notwithstanding that `drops_frame: false` keeps it off
  `dt_client_media_frames_dropped_total` — the oracle argument is about what a token *reveals*, not
  which counter carries it. (2) **Its realistic production cause is a KEK-generation skew** on a
  key-bearing frame from a sender whose transmit key the receiver already holds — *not* an attacker
  or a peer mis-binding wraps. Triage on that; do not read the token name as a cause. A rename toward
  the observable is filed as a **protocol-owned follow-up** — it is a `proto/**` GSA edit plus a
  `crates/media-vector-gen` regenerate plus the g16 `spec_anchor`, which requires the token verbatim
  in the story file — and this paragraph is written to be replaced when that lands.

  **UPDATED 2026-09-08 (story task 19): the closure trigger has FIRED and the rename was ruled OUT of
  that devloop.** This value is now operator-facing for the first time, as an `outcome` on
  `dt_client_media_key_wrap_outcomes_total` — previously it was only a vector-row outcome nobody saw.
  **Proposed target spelling for protocol's rename: `unwrap_failed_key_held`**, recorded here and at
  `docs/TODO.md`'s entry so the spin-out inherits a name rather than coining a third, and so
  `unwrap_failed` (drop) and `unwrap_failed_key_held` (kept) read as the two halves of one AEAD
  failure — which is what this section already says they are, in two names that currently hide it.
  **Why the rename was ruled out rather than done**: renaming `expected.outcome` while `reject_reason`
  on the same vectors row keeps the old spelling would put **two names for one condition inside the
  SSoT**, strictly worse than either endpoint; and renaming both is not a larger version of that, it
  **is** the spin-out. Note the value is pinned **twice** in the vectors file — as `reject_reason` and
  again as `vectors[].expected.outcome` — a second site the TODO entry's scope list did not name.

  **Until the rename lands, triage on the condition and not the token.** The client catalog entry
  leads with the observable — the unwrap failed (a one-bit GCM tag mismatch, which is all the receiver
  gets) **and** a usable transmit key for that key id was already held, so the frame plays — and names
  the realistic production cause as a **KEK-generation skew**, not a peer mis-binding wraps.

State the rule in its own terms. Do not derive it from a neighbouring field.

**The trigger this pre-answers, because the rule is currently inert.** R2 already bars identity
dimensions from every media-path label, so both families are aggregate-only today and nothing about
emission changes. A rule that reads as inert invites *"why is this here, can we drop it?"* just as one
that reads as new invites *"was this always true?"*. The request it exists to answer is: *"the codec
tokens are harmless — can we slice just those by meeting?"* **No, on different grounds:** the oracle
argument does not reach them, but R2's trace argument still does.

**`unwrap_failed`'s cost, stated beside its benefit.** It is the only token yielding a *positive
confirmation* about receiver key state — the KEK unwrap succeeded, so the prober's KEK generation
matches the receiver's — which is strictly more than the others leak. It is a direct consequence of
splitting `decrypt_failed` into unwrap-versus-payload, that split is still right (two AES-GCM
failures on one receive path with opposite remedies), and stating the cost is what stops it being
cited later as unqualified precedent.

**The permitted partner set is safe as a SET, not as three individually approved labels.** `reason`
is safe because its permitted partners (`client_version`, `org_id`) are non-identifying — not because
it is intrinsically harmless. It is one factor of the oracle, and the allow-list's job is to deny it a
partner. A reader who concludes "these are individually safe" adds a fourth by the same reasoning,
which is how the oracle gets rebuilt out of permitted labels.

**`org_id` residual `[reviewer-only]`:** `org_id` is non-identifying only **at scale**. A
single-tenant org, or one whose meetings do not overlap in time, makes it quasi-identifying, and
`reason × org_id` then reconstructs a per-meeting distinguisher from permitted labels. This residual
is **not guardable** — it depends on deployment shape, which no guard can see — so this clause is its
entire control, and it is marked as such because a residual documented inside an enforced-looking
rule inherits the rule's credibility without its enforcement.

**Inertia, in the present-tense-required form (ADR-0036 R3).** The SDK's implicit join label set is
threaded from `MeetingSession.join` into **every** emission site including the media module, so
`reason × meeting_id_hash` — the oracle — is the **default**. It arrives by doing nothing. A task-19
implementer must act to prevent it; the violation ships unless they do.

**Following the pointer:** `frame-v2.vectors.json` is a **test-vector fixture**. Its `crypto` blocks
carry synthetic, test-only key material (including a field named `identity_private_seed_hex`) whose
values are structured low-entropy patterns, not CSPRNG output, and nothing in it derives from any real
environment or secret. Read the `reject_reasons` array; do not file an incident on the rest.

**References, cited rather than restated** so they cannot drift: R2 (media-path label policy and the
voice-activity-trace argument), the task-#7 meeting-dimension bar, and ADR-0036 R3's inertia note.

## Media-path transport refusals `[reviewer-only]`

**One `reason` label space, two families, and a relay is a first-class member of
both.** The previous section governs the codec family. This one governs the
tokens a **relay** adds to the same label space: the conditions under which MH
did not forward a frame that had nothing wrong with its bytes.

`media-protocol`'s `reject_reasons!` macro is the governing text and already
says this — its eight tokens are *"the structural / parse subset of a **shared**
`reason` label space"* — and `RejectReason::producible_by()` names
`rewrite_relay_region` as a producing entry point, which is a relay call.
Recorded here because an earlier draft of this file described the two families
as occupying *disjoint domains*, and that framing was **withdrawn**: MH does
produce codec tokens, verbatim, and the disjointness that matters is between
*spellings*, not between *producers*.

**Home of the relay family**: `mh_service::observability::metrics::MediaDropReason`
— a closed enum with an `ALL` array and a wildcard-free `as_str`, so a typo or a
new value is a compile error rather than a time series discovered in
production. Operator meaning lives in `docs/observability/metrics/mh-service.md`
§Media Forward Path. Neither is restated here.

Three rules bind that family, and only the first is about spelling:

1. **No relay token may collide with a codec token.** If a condition is one the
   codec already names, the relay emits `RejectReason::as_str()` verbatim rather
   than re-spelling it. The one deliberate near-miss is `oversize_datagram`
   versus the codec's `payload_length_exceeds_max`: the codec token means the
   declared *length field* exceeded the maximum during header validation, and
   the relay token means the *received datagram's byte length* did, before any
   parse. Same constant, two checks, two conditions, two tokens.
2. **A relay may never emit a crypto- or key-layer token.** `signature_invalid`,
   `decrypt_failed`, `unwrap_failed`, `replay_detected`, `wrap_key_id_mismatch`,
   `no_kek_for_generation`, `kek_generation_stale`, `no_roster_entry`,
   `no_transmit_key` are the crypto/key members of the
   *receiver-state-dependent* family of the previous section, and a relay holds
   no receiver state and never opens a frame. A relay series carrying one of
   them asserts a verification the relay is structurally incapable of
   performing, and an operator reads it as "the relay validates frames" — after
   which someone relies on a control that does not exist. Note this is a
   **stronger** bar than the oracle argument that governs the client: it is not
   that the label would leak, it is that the value would be false.
   The family's `assignment`-layer member, `sender_not_assigned`, is barred
   from relay series too, **for a different reason**: MH is not structurally
   blind to assignment — it already emits `no_subscriber`, "a policy is
   installed but no egress edge names this sender", which is the server-side
   analogue of the client's slot-edge gate. The bar is rule 1's collision
   argument across layers: `sender_not_assigned` is the CLIENT's spelling of a
   condition MH already names `no_subscriber`, and MH re-spelling it would merge
   drops from two different layers under one `sum by(reason)` token.
3. **Each relay token carries exactly one `direction`.** A token that could
   legitimately occur in both directions is two conditions wearing one name;
   pairing the direction with the token at its definition site is what keeps the
   two labels from disagreeing.

The executable bar for rule 1 and rule 2 is
`crates/mh-service/tests/media_metrics_integration.rs`, which reads every
token from the vector file **as data** and asserts MH's own vocabulary is
disjoint from them. It reads the file rather than the Rust enum deliberately:
the crypto and key tokens have **no Rust home at all**, so a test against
`ALL_REJECT_REASONS` would let a relay-local enum define `replay_detected` and
pass cleanly.

### Permitted partner: `direction` `[reviewer-only]`

`direction` ∈ {`ingress`, `egress`} is an admitted partner of `reason` **on
relay media-path metrics only**, and the reasoning does not travel.

**It is pipeline-relative, never participant-relative.** `ingress` is
publisher→relay and `egress` is relay→subscriber. The participant-relative
reading — `uplink` / `downlink` — is **barred**. Both readings are 2-valued and
a catalog entry cannot tell them apart, so the choice looks cosmetic; it is not.
Only the pipeline-relative one is *structurally incapable* of growing a third
value that individuates a participant, and the property that matters is that
incapacity rather than the current arity.

**It is admitted because the relay is keyless.** The oracle argument of the
previous section is about *receiver key-cache state*, and a relay holds none: it
never opens a frame, so no partner label can turn its counters into a probe of
key material. `direction` therefore adds no factor to an oracle that does not
exist on this component.

**This acceptance does not generalise to the client's counter** (story task 19).
The client sits on exactly the state the oracle argument is about, and
`reason × direction` there is a different question with a different answer. Do
not cite this block as precedent for `dt_client_media_frames_dropped_total`.

## Media-path identity `[reviewer-only]`

**Read this before adding any label, log field, span attribute, or exemplar on a media-carrying
path.**

These are **rules, stated as rules**. ADR-0036 §11 requires them to be stated rather than derived,
because **four specialists independently had to be corrected on this during the design debate** — a
rule that every author must re-derive from cardinality principles will not survive contact with a
deadline.

### R1 — No meeting identifier on any metric, raw or hashed

**No metric anywhere in this design carries a meeting identifier**, in any form.

A **hashed** meeting id is barred on exactly the same footing as a raw one. Hashing addresses
*identifiability of the string*; it changes neither of the two things that bar it:

1. **Cardinality** — a hash has identical cardinality to its input. One series per meeting either way.
2. **Per-meeting aggregation** — and this is the one people miss: in a **two-person meeting,
   per-meeting aggregation is nearly per-stream**, so a two-stream series is de-anonymising by
   inspection. Two-person is the *common* case, not the corner case.

> **The Category B denylist cannot enforce this; the prefix denylist can, for Rust metric labels
> only.** Adding `meeting_id` to **Category B** is **inert against the realistic spelling**:
> `meeting_id_hash` ends in `_id_hash`, which `HASHED_SUFFIXES` exempts via `is_hashed_label()`, so a
> Category B entry would be defeated by the very mechanism it invokes — while reading as coverage.
> `meeting_id` is therefore carried in **`PII_PREFIX_DENYLIST`** instead, which `pii_token_hit`
> evaluates *before* `LABEL_ALLOWLIST` and before `is_hashed_label()` — so it catches
> `meeting_id_hash` and every other `meeting_id*` spelling. **The partition is the whole of the
> fix**: the prefix list has one consumer (`metric_labels`), so the bar lands on Rust *metric labels*
> without firing on the legitimate control-plane `meeting_id = %meeting_id` *log* fields a Category B
> entry would break across MC/GC/MH.
>
> **This covers Rust metric labels only, and it is bypassable.** The scanner reads `crates/`, so the
> TS client SDK's grandfathered join label set is structurally out of reach, and a *trailing*
> compound (`x_meeting_id`) is not a prefix match. Both remain `[reviewer-only]`. The
> `PiiCategory::Prefix` arm is also gated on `pii_safe.is_none()`, so `# pii-safe: <reason>`
> suppresses it — only Category A is non-bypassable. ADR-0036 §11's "vocabulary additions cannot be
> cited as the protection" therefore still holds against the *unqualified* claim: this entry is
> reviewer-gated coverage for one surface, not coverage for the rule's stated scope.

**Grandfathered exception — closed, enumerated, and not extended.** The ADR-0028 join-flow metrics
that already carry the client SDK's implicit join label set (including `meeting_id_hash`) are
grandfathered **as a set**. The set is closed: it is not an appendable carve-out list, no new metric
joins it, and every metric this design adds — plus every future metric on any media-carrying path —
is under the bar.

### R2 — No participant or stream identity on the media path

**No media-path metric label, log field, span attribute, or exemplar carries participant or stream
identity**, or any per-frame dimension.

**The aggregation floor is `pod`.** *(Deliberately stricter than ADR-0036 §11's "pod or service
level" — a ratchet, chosen because service-level aggregation across a two-pod fleet is close enough
to pod-level to offer no real protection while sounding like a choice. Flagged for the same reason as
the carrier-list widening above: every delta from §11 in this file is stated, so a future reconciler
can tell intentional from accidental.)*

Aggregate distributions are safe **only where they actually aggregate**, and that must be asserted of
the *observable*, not of the label set: a metric partitioned no finer than `pod` still reconstructs a
single stream's sequence whenever the pod carries one stream — which is not a corner case but a
solo or two-person meeting (story 1 shipped exactly this as its loopback shape; loopback is gone since
story 2, the low-occupancy case is not) and a routine low-occupancy production state. So the rule is:
**no per-frame time-ordered sequence for any single stream, including where per-stream isolation
arises from low occupancy rather than from a label.** What is prohibited is resolution that
reconstructs a *sequence*:
per-frame size and timing for a single stream is **the voice-activity trace** — who spoke, in what
order, for how long, and who interrupted whom — against ADR-0036 §11's stated adversary set: MH,
whoever compromises MH, and a curious operator; **not** a network observer, since these fields sit
inside QUIC with TLS terminated at MH.

**Exemplars are the fourth surface and are the easy one to miss.** A Prometheus exemplar is not a
metric label, a log field, or a span attribute — it hangs off a histogram bucket with its own label
set — so a rule phrased against the other three does not reach it. A per-stream exemplar on the MH
forward-latency histogram would reconstruct exactly the sequence this rule protects, and that
histogram is where someone will reach for one, because it is where the SLO is measured. ADR-0036 §11
rejects exemplars by name: they "relocate retention into the trace backend rather than solving it".

**Sampling must be random, not deterministic per stream.** The natural modulo implementation meets the
CPU budget while perfectly reconstructing the sequence.

**A log level is not an acceptable gate.** The incident motivating a level change is the same incident
producing the sensitive trace.

Per-participant resolution, when genuinely needed, lives in ADR-0036 §11's armed, meeting-scoped,
auto-expiring in-memory ring buffer dumped on demand — with **MC's assignment state answering *who***,
at investigation time, in a system that legitimately holds that mapping.

### R3 — Client media-path metrics carry only `client_version` and `org_id`

Media-path metrics emitted by the client SDK **must** carry only `client_version` and `org_id`.

Plus `key_custody=operator` (§Key custody). That is the whole set.

> **LANDED 2026-09-08 (story task 19). This paragraph previously read "a required end state, not a
> description of current behaviour" and that form is now retired** — leaving it would make this file
> assert a violation that no longer exists, which is the same defect class R3 exists to prevent, one
> level up.
>
> **How it landed matters more than that it landed**, because the mechanism is the reusable part.
> Media labels are built by an **allow-list projection**, `mediaMetricLabels` in
> `packages/sdk-core/src/media/setup/mediaMetrics.ts`, whose constructor takes **two named strings**
> rather than a `MetricLabels` bag. So `meeting_id_hash` is not merely absent from media metrics — it
> is **unrepresentable at that boundary**. A deletion-based helper (`{...labels, meeting_id_hash:
> undefined}`) would have satisfied the rule on the day and been one refactor away from re-inheriting
> a newly-added join label; an allow-list cannot be.
>
> **The inertia argument this paragraph used to make was correct and is preserved as the reason for
> that shape, not deleted with the condition it described.** The join label set still arrives by
> default at every non-media emission site; what changed is that the media path no longer has a
> parameter through which it could arrive.
>
> **One metric deliberately still carries it.** `dt_client_mh_connection_total` keeps
> `meeting_id_hash` as a grandfathered ADR-0028 join-flow metric, and
> `media/__tests__/media-transport.test.ts` keeps the assertion pinning that — annotated at both
> sites, because it and the R3 negative test assert opposite things and are both correct. See
> §The grandfathered set below.

### R4 — Every media-path series is identity-free and carries `key_custody=operator`

**Story 2 (R-28) restates R1-R3 as one checkable rule.** No media-path series carries a meeting,
participant, sender, key, slot or stream identity. Every media-path series carries
`key_custody="operator"` (the value `crates/common/src/observability/labels.rs::KEY_CUSTODY_OPERATOR`
publishes; §Key custody).

**What "media-path series" means (defined here once; the consumers cite this).** A series whose
metric name begins with one of these prefixes:

```media-path-prefixes
mh_media_
mc_media_
mc_meeting_kek_
mc_meeting_sender_ids_
dt_client_media_
dt_client_time_to_first_media_frame_ms
```

The last entry is a single exported client media metric outside the `dt_client_media_` prefix. The
hygiene kernel's `MEDIA_PATH_PREFIXES` is pinned to this block by a unit test.

`mc_meeting_sender_ids_issued_max` is IN by decision: it is the ADR-0036 §2 sender-handle space. It
observes no frame, but it is the dimension R2 exists to keep out of labels, so it stays under the
same bar. `dt_client_mh_connection_total` and the other ADR-0028 join-flow metrics are OUT. They are
the closed grandfathered set (§The grandfathered set), and the collector's name allowlist never
exports them.

**The identity vocabulary is ONE policy with two consumers, and this fenced block is its only home.**

- `crates/dt-guard/src/client_metrics_export.rs` (`identity_key_forwarded`) applies it at build time
  to the keys the collector keeps and the keys GC forwards.
- `crates/env-tests/src/fixtures/metric_hygiene.rs` (`Rule::MediaPathIdentity`) applies it to the
  stored series.

`dt-guard` reads this block at RUNTIME (no copy, with a positive control), so it cannot drift. The
kernel keeps a const copy, and its unit test parses this block and fails if the copy differs. Edit
the block and the kernel consts together.

```identity-label-policy
containment = meeting, participant, sender, session, slot, stream
segment = user, key, kek, id, hash
exempt = key_custody, org_id, slot_state
```

**One case table for both matchers.** Both consumers iterate this table in their unit tests, so a
matcher that drifts goes red in whichever crate drifted. Add a row here, never to one crate's test.

```identity-label-cases
fire = participant_id, meeting_id_hash, stream_id, key_id, kek_id, meetingId, session.id, slot_index, slot_id, x_sender_idx, user, frame_hash, user-id, key-id, sender-id, frame-hash
pass = key_custody, org_id, slot_state, client_version, reason, outcome, action, source, mode, trigger, payload_kind, instance, job, le, otel_scope_name, otel_scope_version, valid, monkey_business, direction, status
```

**Matcher semantics.**

- `containment` nouns match anywhere in the lower-cased key. Compounds like `senderIndex` or
  `x_stream_id` are exactly what a word-boundary matcher misses (see the kernel's `contains_ci`
  note).
- `segment` tokens are short enough that containment would false-fire (`id` inside `valid`, `key`
  inside `monkey`). They match only a whole segment after splitting on every non-alphanumeric character and on
  camelCase boundaries (the same splitter as `dt-guard`'s `ts_retained_credentials::segments`) (`key_id`, `session.id`, `meetingId`).
- `exempt` entries are **exact keys**, never segments or prefixes. So `slot_index` and `slot_id`
  still fire while `slot_state` passes.

The three exemptions:

- `key_custody`: the custody declaration itself, with a single permitted value. It is not a key
  identity.
- `org_id`: the tenant. GC overwrites the client-sent value from the authenticated claims (R3 permits
  it; `crates/gc-service/src/services/telemetry_filter.rs::stamp_org_id`). A tenant is not a meeting
  or participant identity.
- `slot_state`: the bounded eight-value state vocabulary from
  `crates/mc-service/src/media_signaling/assignments.rs::slot_state_label`. It carries no slot index.

**`meeting` overlaps `MEETING_ID_SUBSTRINGS`** in the hygiene kernel (R1). Both fire on
`meeting_id_hash`. That is deliberate: R1 is the older, broader rule over every metric, and this
block is R1 plus R2 restricted to the media path.

**On a client series, `instance` names the collector pod, not a browser.** Client series are scraped
from the collector under `honor_labels: false`, so `instance` and `job` are the collector's target
labels. Grouping a `dt_client_*` expression `by (instance)` groups by collector, never by
participant. `dt-guard client-metrics-export` flags that shape in a loaded alert rule
(`client_alert_groups_by_instance`).

**Suffix rule: `_threshold` is reserved for values that something enforces or alerts on.** A
published advisory value takes a descriptive suffix instead. The canonical case is
`mh_media_egress_stream_ceiling_recommended_min`, which is advisory and read by no alert. The
reason: a `_threshold` spelling tells a responder that something automatic is watching, and an
advisory value spelled that way reads as coverage that does not exist.

**Story-2 label values** (bounded vocabularies; each is defined in code and catalogued in the
service catalog):

| Series | Label | Values (source) |
|---|---|---|
| `mc_meeting_kek_generated_total` | `trigger` | `crates/mc-service/src/media_admission/rotation.rs` trigger `label()` |
| `mc_meeting_kek_pushes_total` | `outcome` | per recipient (`crates/mc-service/src/observability/metrics.rs`) |
| `mc_participant_outbound_messages_dropped_total` | `payload_kind` | including the KEK-update kind |
| `mh_media_frames_dropped_total` | `reason` | gains `server_muted` (`MediaDropReason`) |
| `mh_media_stream_admission_total` | `outcome` | admission outcomes |
| `dt_client_media_kek_updates_total` | `source` | `MEDIA_KEK_SOURCES`, including `kek_update` (two values, deliberately; see `docs/observability/metrics/client.md`) |
| `dt_client_media_capture_source` | `mode` | the `MediaCaptureSourceMode` type (a type, not a const, so `test_tone` never reaches a production bundle; `packages/sdk-core/src/media/setup/mediaMetrics.ts`) |
| `dt_client_media_frames_dropped_total` | `reason` | the split key-generation reasons `no_kek_for_generation` and `kek_generation_stale` |

The authoritative value lists are the code enums named in each catalog entry. This table says which
label each series gained; it is not a second copy of the values.

### No per-handler dimension on client media metrics — asked twice, answered once

**Standing ruling (2026-09-25, story 2 task 20).** Since a client may hold sessions to several
handlers (ADR-0036 §9), "frames are arriving, but not from handler X" and "which lane is dropping"
became natural questions. They were asked twice in one devloop — by `operations` on the send side and
by `client` on the receive side. **The answer is no, and it is not close**, so it is recorded here
rather than re-decided per task.

1. **R3 is closed and this would reopen it.** The set is `client_version` + `org_id` + `key_custody`.
   A handler dimension is not an addition to a list; it is a change to the allow-list projection's
   *shape* (`mediaMetricLabels` takes two named strings), which is the mechanism that makes the rule
   structural instead of documentary.
2. **The cardinality argument does not save it, and this is the part that surprises people.** A
   handler id is bounded (2 today) and carries no participant identity, so it looks safe by the R1/R2
   tests. It is not: `org_id` × handler at the 10 s client export cadence, in an org small enough to
   hold one meeting, is a per-stream arrival time series — **the voice-activity trace R2 exists to
   prevent**, reached by a different route. Client-side aggregation has no `pod` floor to hide behind;
   `org_id` *is* the floor, and a small org is not a corner case.
3. **The question is already answerable, on the other end of the hop.** Each handler is a pod, and
   MH's own media counters are per-pod by construction — so "is handler X forwarding anything" is
   read at handler X's own `mh_media_frames_forwarded_total`, and the cross-end comparison documented
   in `docs/observability/metrics/client.md` closes the loop without a client-side label. A
   client-side split would buy a *second*, weaker view of a question the server already answers
   exactly.
4. **What is genuinely client-only** — a lane whose directed target has no transport — is carried by
   `dt_client_media_send_dropped_total{reason="not_connected"}`, which is a bounded reason token, not
   a handler identity. That is the shape to reach for: **when per-handler detail is wanted, find the
   bounded outcome that distinguishes the cases, not the identity that enumerates them.**

**If this is ever reopened**, it needs the small-org aggregation argument in (2) answered, not just
the cardinality one — and the trigger would be a case the server end genuinely cannot see, which (3)
has not yet produced.

### The grandfathered set, and the line that decides membership

**The test is what a metric OBSERVES, not which directory it lives in.** Connect-lifecycle — once per
connection, at setup, before any frame exists — is grandfathered. Media-carrying — per frame, per
stream, or on the media data path — is under the bar.

A directory-shaped rule fails in **both** directions, and only one of those failures is the one people
picture. It would wrongly bar `dt_client_mh_connection_total`, which lives in `media/MediaTransport.ts`
and is grandfathered anyway; and it would wrongly **permit** a media counter placed outside the media
tree. The second is the failure that will actually happen, and a directory rule licenses it.

The roster is frozen with a named member list in `docs/observability/metrics/client.md` — a closed
exception with no roster opens by analogy, because the next author decides their metric is "join-flow
enough". The obligation to keep it frozen has **no mechanical enforcement**; it is tracked at
`docs/TODO.md` D5, held open for that reason rather than closed as completed work.

### Enforcement reality — read this before citing these rules as coverage

**R2 and R3 are `[reviewer-only]`. R1 is `[guard-enforced, bypassable]` for Rust metric labels and `[reviewer-only]` everywhere else.** Read the scope bullets before citing any of them as coverage.

- **R1, Rust metric labels — `[guard-enforced, bypassable]`.** `meeting_id` is in `PII_PREFIX_DENYLIST` (`crates/dt-guard/src/common/pii_vocabulary.rs`), whose single consumer is `metric_labels.rs::pii_token_hit`. The prefix scan runs first — before Category A, before `LABEL_ALLOWLIST`, before `is_hashed_label()` — so `meeting_id_hash` is **caught rather than exempted**, and a violation reports as `PiiCategory::Prefix`. A test pins that category *specifically*: relocating the term to Category B would keep a laxer "a finding was raised" test green while silently restoring the hashed exemption. **It is bypassable.** The `PiiCategory::Prefix` arm is gated on `pii_safe.is_none()`, so `# pii-safe: <reason>` suppresses it; only Category A is non-bypassable. The residual control is that the reason is `[reviewer-gated]` (security) and sits in the diff — this entry converts a silent pass into a reviewed one, which is not the same as making R1 unbreakable.
- **R1, everywhere else — `[reviewer-only]`.** The match is `starts_with` and the scanner reads `crates/`. A **trailing** compound (`x_meeting_id`) is not a prefix match, and the TS client SDK — including the ADR-0028 grandfathered join label set that legitimately carries `meeting_id_hash` — is structurally out of a `crates/`-only scanner's reach. R1's stated scope is "any metric anywhere in this design"; the guard covers one surface of it, bypassably.
- **R1 on log fields, deliberately unguarded.** The prefix partition was chosen over Category B precisely so the bar lands on metric labels alone: `meeting_id = %meeting_id` is a legitimate control-plane *log* field across MC/GC/MH, and a Category B entry would break it. The partition is the fix, not an implementation detail of it.
- `sender_id_hash` is exempted by the same mechanism and is **not** covered by the Category B
  `sender_id` entry — `is_hashed_label()` is tested *before* the Category B lookup in
  `pii_token_hit`. **TRIGGER, not an inventory line**: that entry is sound today *only* because no
  `sender_id_hash` exists and the realistic spelling is the plain one. **If a `sender_id_hash` is
  ever proposed, that premise is void** — the entry becomes inert in the R1 sense, and the
  hashed-exemption carve-out (tracked in `docs/TODO.md` §Observability Debt) must land before the
  hashed spelling ships. The carve-out is deferred deliberately: `is_hashed_label` is a primitive
  `metric_labels` reads for *every* Category B term, so changing it is guard machinery, not a
  vocabulary edit.
- `#[instrument]` span parameters are covered by **neither** entry: `instrument_skip_all` reads
  Category A only. The span bar is `skip_all` discipline, reviewer-enforced.
- The durable enforcement for the media-path half is by **shape, not vocabulary**: a directory-scoped
  deny of log and metric macros across the whole media path, which catches the offending *form*
  rather than a spelling. Word-boundary vocabulary matching cannot see inside compounds, so a
  prefixed spelling ships clean while the guard reports green.

#### When vocabulary can be the control at all

The `[guard-enforced]` / `[reviewer-only]` annotations above are decided case by case, with no rule
behind them. This is the rule. It exists because "add the term to the denylist" is the reflex answer
and is wrong more often than it is right.

**Ask whether the property is a property of names.**

- If the violation is a **term appearing** — a known credential spelling, a named PII field — then
  vocabulary can be the control, and matcher quality is the only remaining question.
- If the violation is a **shape arriving** — a dimension reaching a persisted label, a macro reachable
  from a hot path, a credential entering any sink — then **no matcher over names can reach it at any
  quality**, and the control must be arrival-shaped: a constructor or type-level allow-list, or a
  directory-scoped deny. A vocabulary entry against a shape-harm reports clean while the violation
  ships.

**The branches are not exclusive, and where they overlap the arrival-shaped control wins.** A
credential leak is both a term appearing and a shape arriving; the second framing is the one that
survives a spelling nobody enumerated. Vocabulary is the *primary* control only where the violation is
term-shaped **and no arrival surface exists to constrain**. Where one exists, constrain it and keep the
vocabulary for classification and triage — never cited as the protection.

**"No arrival surface exists" is a claim requiring evidence, not a default.** An arrival surface is a
constructor every value of the kind must pass through, a directory boundary, or a type. Before
concluding none exists, ask whether one can be **created** — and where the cost is bounded, creating
one is the answer. ADR-0036 §11 is the precedent: it did not accept the tree's boundaries as given, it
**imposed a layout constraint** (lifecycle, setup and teardown as *siblings* of the hot path rather
than children) precisely so a directory boundary and the hot-path boundary would coincide and a
shape-deny could exist at all. The surface was manufactured because the control required one. Without
this clause the rule degrades into "use vocabulary whenever the alternative is work", which inverts it.

**Three worked instances, all in this tree, all different arrival surfaces:**

| Harm | Arrival surface | Vocabulary's role |
|------|-----------------|-------------------|
| KEK / transmit key reaching a log or sink | The §11 directory-scoped deny (`docs/TODO.md` §Credential-leak guard, item (a)) | **Demoted.** No word list in `checks.md` at all; terms went to `pii_vocabulary.rs` CATEGORY_A as classification |
| Log or metric macro reachable per frame | Directory boundary, **manufactured** by §11's sibling layout | None — the harm is not name-shaped |
| Identity dimension reaching a media metric label | The `mediaMetricLabels` constructor (two named strings); `media/__tests__/hotPathLayout.test.ts` asserts it is the only label constructor under `media/**` | None — see below |

**Why the third case admits no vocabulary at all, stated because the obvious argument for it is
wrong.** The tempting reason is "the word-boundary matcher cannot see inside compounds" — true, but
**not dispositive**, because `segments()` (`crates/dt-guard/src/ts_retained_credentials.rs:198`) exists
for exactly that case and a reader who knows the guard tree will say so. The argument that holds is one
level up: **the harm is the label's dimensionality, not its spelling.** Nothing stops a module emitting
`(sender_id, stream, generation)` under `ctx`, `sid`, or `k` — names no vocabulary would or should
contain, carrying all three barred dimensions just as completely as `key_id` would. A name-matcher
catches only the spellings someone anticipated; the constructor allow-list catches the **arrival**,
whatever it is called. That is §11's *"vocabulary additions cannot be cited as the protection"* in the
form that forecloses the family, rather than the form that invites a better matcher.

**And the harm there is retention and aggregation, not confidentiality.** The key id is clear-header
data MH reads on every frame to route — claiming it is secret would be plainly false to the reader
whose agreement this entry needs, and the real argument would go down with the overclaim. The actual
grounds: `key_id` decomposes to `(sender_id, stream, generation)`, so **one label carries all three
barred dimensions in a field that does not read as an identifier**; its `generation` component advances
on every rotation, making a key id on a label a per-sender **rekey-timing series**; and per D5 any
participant or stream dimension joined to the reject vocabulary is what makes those `reason` values a
decryption oracle — which does not care which directory the label was constructed in.

Recorded plainly so nobody cites a conventions file as a control. See `docs/TODO.md` for the gap
between these rules' stated scope ("anywhere in this design") and what the directory-scoped deny
actually covers.

---

## PII / Secret Denylist `[guard-enforced]`

The guard rejects label KEYS matching any of these tokens. The list is split
into two categories with different bypass semantics.

### Category A: Secrets `[guard-enforced, non-bypassable]`

**Per ADR-0011 §PII & Cardinality (lines 156–161), these are
non-negotiable.** Placing a credential into a metric label exfiltrates it
to the metrics backend, which typically has weaker access controls than
the secret store. `# pii-safe` **cannot** whitelist a Category A label —
if you see a Category A flag, remove the label; do not rationalize it.

| Token | Rationale |
|---|---|
| `password`, `passwd` | Plaintext credential. |
| `api_key`, `apikey` | Service credential. |
| `secret` | Generic credential marker. |
| `token` | Bare `token` label key almost always means "the actual token value" — catastrophic. Legitimate `token_*` labels describing a flavor or subsystem are allowlisted below. |
| `bearer_token`, `access_token`, `refresh_token`, `session_token`, `id_token` | OAuth / JWT bearer credentials. |
| `private_key`, `privkey`, `signing_key` | Asymmetric key material. |
| `jwt` | Full JWT string; label value would leak the credential. |
| `auth_header`, `authorization` | `Authorization:` header contents. |
| `meeting_kek`, `transmit_key` | ADR-0036 §4 media key material. Bare `kek` is deliberately **not** a token: `metric_labels`' single-word path splits on `_`, so it would false-positive on a plausible `kek_generation` label — which is metadata identifying *which* key, never key material. Classified into `NON_CREDENTIAL_TOKENS` for the retained-credential partition (a client is an *entitled* long-lived holder under §4), which does **not** weaken the log/label/span coverage. |

**Category A allowlist** (narrow; co-owned with security):

| Allowed label | Meaning |
|---|---|
| `token_type` | Bounded enum: `meeting`, `guest`, `service`. Used by `record_jwt_validation()` in GC, MC, MH. |

Policy: only label keys with actual production usage belong here.
Speculative forward-compat entries invert the "rename before extend"
posture — new `token_*` labels must justify their allowlist entry on first
use, not before. Additions require security + observability co-owner
sign-off. A new `token_*` label that isn't listed will fire `label_secret`
and block the PR, forcing the author to justify the addition in review.

### Category B: User PII `[guard-enforced, bypassable]`

Personal identifiers. Hashed forms (`user_id_hash`, `email_sha256`, etc.)
are allowed; the `# pii-safe: <reason>` escape hatch also applies for
documented false positives.

| Token | Rationale |
|---|---|
| `email` | Direct personal identifier. |
| `phone`, `phone_number` | Direct personal identifier. |
| `display_name` | User-chosen identifier; often correlates to real name. |
| `user_id` (raw) | Stable cross-session identifier. Hashed form (`user_id_hash`) is allowed. |
| `sender_id` | ADR-0036 §2/§4 per-meeting media sender handle. Restores coverage the deleted proto `user_id` field provided incidentally. **Not the inert case R1 warns about** — see §Enforcement reality: for `meeting_id` the realistic spelling is the hashed one, so a plain *Category B* entry is defeated on arrival, which is why `meeting_id` is carried as a **prefix** entry instead; for `sender_id` the realistic spelling is the *plain* one, because nothing in the tree hashes a sender id, so Category B holds. Inverse of that case, not an instance of it — **and the premise is load-bearing**: see the §Enforcement reality TRIGGER, which voids this row if a `sender_id_hash` spelling is ever proposed. |
| `username`, `nickname`, `handle` | Account identifiers. |
| `name` | Too broad to assume safe; `hostname` / `filename` allowlisted by specific exception. |
| `address`, `postal_code`, `zip`, `zipcode` | Location PII. |
| `ip`, `ip_addr`, `ipv4`, `ipv6` | IP addresses are PII under GDPR and a common exfiltration target. |
| `device_id` | Stable per-device identifier; correlates to account. |
| `user_agent` | Fingerprints browser + OS; cardinality hazard and PII. |
| `fingerprint` | Canvas/browser fingerprint — deliberate tracking identifier. |
| `latitude`, `longitude`, `geolocation`, `geoip` | Geolocation PII. |
| `ssn` | Social security / national ID numbers. |
| `dob` | Date of birth. |
| `passport`, `driver_license` | Government identifiers. |
| `credit_card`, `card_number`, `cvv` | Payment PII (PCI-DSS concern). |

### Prefix denylist `[guard-enforced, bypassable]`

Labels whose key starts with a denylisted prefix are flagged regardless of the suffix. **The prefix
scan runs before Category A, before `LABEL_ALLOWLIST` and before `is_hashed_label()`** — that
ordering is the point of the partition, because it is what a hashed spelling cannot slip past.

| Prefix | Why it is a prefix rather than a Category B token |
|---|---|
| `raw_` | Signals the author knew the value was sensitive and opted out of sanitization — a pattern worth surfacing for review. Examples: `raw_email`, `raw_user_id`, `raw_request_id`. |
| `meeting_id` | R1 (§Media-path identity, ADR-0036 §11). A Category B entry would be **defeated on arrival** by `is_hashed_label()`, since the realistic spelling is `meeting_id_hash`. Prefix placement also confines the bar to metric labels — the prefix list's single consumer is `metric_labels` — leaving the legitimate control-plane `meeting_id = %meeting_id` **log** field untouched. |

The two entries share a mechanism, not a rationale: `raw_` surfaces a deliberate opt-out for review;
`meeting_id` enforces a prohibition. Both are suppressible by `# pii-safe: <reason>` — see §R1 for
why that makes R1 a reviewer-gated control on this surface rather than an absolute one.

### Match semantics `[guard-enforced]`

- Case-insensitive.
- Exact-label-match (`"email"`) or component match (`"user_email"` →
  `email`; `"client_ip"` → `ip`; `"raw_email"` → `email`).
- Multi-word tokens (`ip_addr`, `display_name`, `device_id`) match as
  substrings of the whole key.

### Allowlist (substring false positives) `[guard-enforced]`

The guard allows these keys even though they contain a denylist token as a
substring, because they name infrastructure-identity concepts rather than
user-identity:

- `hostname`, `filename`, `pathname`, `typename`, `nameservice`

To add a new allowlist entry, modify `LABEL_ALLOWLIST` in the guard AND
justify it here. Default posture: prefer renaming the label over extending
the allowlist.

### Hashed / opaque suffixes allowed (Category B only) `[guard-enforced]`

Labels whose key ends with `_hash`, `_hashed`, `_id_hash`, `_sha256`, or
`_digest` are treated as opaque and NOT flagged for Category B (user-PII).
Category A (secrets) tokens remain denied regardless of suffix — you should
never hash a credential into a label; just don't include it.

This is the canonical way to keep a per-user label in a metric without
leaking the identifier:

```rust
counter!(
    "svc_user_actions_total",
    "user_id_hash" => hasher::hash(user_id),
    "action" => action.to_string()
).increment(1);
```

> **This exemption does NOT cover `meeting_id_hash`, or any meeting identifier.** The suffix rule
> above is a *PII* exemption; the meeting-identifier prohibition in §Media-path identity is a
> *cardinality and aggregation* rule and is independent of it. On its own, `is_hashed_label()` would
> wave a hashed meeting id through — it ends in `_id_hash` — but `meeting_id` sits in
> `PII_PREFIX_DENYLIST`, which `pii_token_hit` evaluates **before** the suffix rule is ever consulted,
> so `meeting_id_hash` is rejected as a Rust metric label. The prohibition does not depend on that
> ordering; on that one surface it is now guard-backed by it, subject to the `# pii-safe` bypass that
> applies to every non-Category-A finding. Read §Media-path identity before adding any
> meeting-scoped label.

**Reviewer-level requirements** `[reviewer-only]` for hashed labels:

- Hash must be cryptographic (SHA-256 or better). Truncated MD5 / simple
  string hashing is NOT sufficient — it doesn't prevent a determined
  attacker with a user-ID list from recovering the identifier.
- The hash input SHOULD include a server-side secret salt if the identifier
  space is small enough to brute-force.
- Cardinality impact of a hashed user ID is still `N` where `N` is the user
  count — the hash bounds *leakage*, not *cardinality*. If cardinality is a
  concern, see §Label-cardinality-aware metric design.

### Extension policy `[reviewer-gated]`

Security reviewers may require additions to the denylist at review time
(e.g., a new privacy-regulated field, a fresh incident learning). Additions
land via a PR updating both this file AND
`crates/dt-guard/src/common/pii_vocabulary.rs` — the vocabulary lives there, not in
`scripts/guards/simple/validate-metric-labels.sh`, which has been a thin dt-guard wrapper
since ADR-0034 §3.
Removals require security sign-off on the PR.

---

## Cardinality Budgets (ADR-0011)

Three layers of budget, each with a different enforcement mechanism.

### Per-metric combinations: ≤ 1000 `[reviewer-only]`

The product of label-value cardinalities for a single metric must not exceed
1,000. This is the ADR-0011 figure and is NOT source-checked — it's a design
constraint reviewers enforce.

**Examples**:
- `status` (3) × `method` (7) × `endpoint` (~10 bounded) = 210 combinations. OK.
- `status` (3) × `user_id_hash` (unbounded in practice) = cardinality
  explosion. Not OK, even though the hash is PII-safe.

### Per-label-value length: ≤ 64 chars `[guard-enforced]`

Any string LITERAL label value longer than 64 characters is flagged. This
catches:

- Long human-readable messages (`"User did not have permission to do X..."`).
- Concatenated identifiers.
- Serialized error payloads.

Runtime-bound label values (`variable.to_string()`) can't be length-checked
at parse time — those rely on the author binding the variable to a bounded
set per §Bounded-label pattern.

### Fleet-wide series: 5M `[runtime-enforced]`

The 5M fleet-wide series budget from ADR-0011 is enforced at the Prometheus
scrape layer (`/metrics` endpoint cardinality limits + `sample_limit` in the
scrape config). The guard cannot source-check it. When you see a
`sample_limit` violation at runtime, the fix is almost always a label
cardinality audit.

---

## Bounded-Label Pattern `[reviewer-only]`

When a label value is derived from an unbounded source (user input, a
typed-string field, an error variant), the canonical pattern is to bind it
to a bounded enum via a method that returns a `&'static str`.

**Example** (from `crates/mc-service/src/errors.rs`):

```rust
impl McError {
    pub fn error_type_label(&self) -> &'static str {
        match self {
            McError::JwtValidation(_) => "jwt_validation",
            McError::MeetingNotFound(_) => "meeting_not_found",
            McError::CapacityExceeded => "mc_capacity_exceeded",
            McError::Internal(_) => "internal",
            // ... exhaustive match
        }
    }
}
```

**Usage**:

```rust
counter!(
    "mc_session_join_failures_total",
    "error_type" => err.error_type_label().to_string()
).increment(1);
```

**Properties**:
- Exhaustive match — compiler forces every variant to produce a label.
- Bounded — the cardinality is the error-enum variant count.
- Stable — the label set changes only when code changes, never at runtime.
- Reviewable — `git diff` shows the label set explicitly.

**Similar precedents**:
- `crates/mh-service/src/errors.rs:error_type_label()`
- GC's `normalize_endpoint()` in `metrics.rs` — binds an unbounded path to
  a bounded endpoint-pattern set.
- MC's `actor_type` — bound by the `ActorType` enum.

### Anti-pattern: using a raw typed string

```rust
// DO NOT — unbounded cardinality source
counter!(
    "svc_errors_total",
    "error_type" => format!("{:?}", err)  // Debug output includes data
).increment(1);
```

Every unique error payload produces a unique series. The guard can't catch
this pattern directly but reviewers MUST.

---

## `# pii-safe: <reason>` Escape Hatch `[reviewer-gated]`

When a PII-denylist match is a false positive, add a `# pii-safe: <reason>`
marker on the macro invocation line OR the line immediately preceding it:

```rust
// pii-safe: internal admin-service identifiers are public organizational names
counter!(
    "svc_admin_actions_total",
    "display_name" => admin_display_name.to_string()
).increment(1);
```

Both `#` and `//` prefixes are accepted (Rust-native `//` is preferred).

### What the escape hatch suppresses `[guard-enforced]`

- Category B PII-denylist match on a label key (user-PII).
- Prefix denylist match (`raw_*`, `meeting_id*`). **For `meeting_id*` this suppresses a prohibition, not a false positive** — ADR-0036 §11 states R1 absolutely, so a hatch here is an exception to a rule rather than a correction to a mismatch. Expect security to hold the reason to that standard; see §R1.

### What it does NOT suppress `[guard-enforced]`

- **Category A secret-denylist match** — credentials in labels are
  non-negotiable per ADR-0011. Adding `# pii-safe` will NOT silence a
  Category A flag; the guard emits `label_secret` in that case. The fix
  is to remove the label, not to whitelist it.
- Literal-value-length (> 64 chars) — cardinality is independent of PII.
- Obviously-unbounded value expressions (`Uuid::new_v4()`, `request_path`) —
  same reason.
- Label-naming hygiene (uppercase, punctuation) — this is a style issue,
  not a safety issue.

### Reason requirements `[guard-enforced]`

- ≥ 10 characters.
- Not in the lazy set `{test, tmp, todo, fixme, wip}`.
- Must explain *why* the label is safe, not just claim it.

### Review process `[reviewer-gated]`

Every new `# pii-safe` marker gets scrutinized during PR review by the
security reviewer. The bar for a passing reason:

- **Specific**: "admin usernames are public organizational names, documented
  in ADR-0017" — good. "this is fine" — rejected.
- **Dated**: include approximate review date when the reason is a judgment
  call; reasons become stale.
- **Traceable**: reference the ADR, PR, or incident that established the
  policy when applicable.

If a `# pii-safe` reason becomes obsolete, remove the marker. Stale safety
rationales are worse than no rationale at all.

---

## Label-Cardinality-Aware Metric Design `[reviewer-only]`

When a "natural" label would be too cardinal, choose one of these shapes:

### 1. Hash the identifier

Already covered under §Hashed / opaque suffixes. Bounds *leakage*, not
*cardinality*. Useful when you need per-user investigations but not
per-user dashboards.

### 2. Bucket the value

Map a continuous or high-cardinality value to a small bucket set:

```rust
fn session_duration_bucket(d: Duration) -> &'static str {
    match d.as_secs() {
        0..=59 => "under_1m",
        60..=599 => "1m_to_10m",
        600..=3599 => "10m_to_1h",
        _ => "over_1h",
    }
}
```

`session_duration_bucket` produces 4 values no matter how many sessions.

### 3. Promote to trace attribute

If the value's utility is "I need to find THIS session's flow", it belongs
on a trace span, not a metric label. Traces handle high-cardinality
dimensions; metrics handle bounded aggregations. This is the correct place
for `request_id`, `session_id`, raw `user_id`.

### 4. Drop the label entirely

Often a metric's utility is undamaged by dropping a label. "Total failures
per error type" doesn't need `user_id`; add it only when the aggregate
series is the answer to a real operational question.

---

## Machine-Enforced vs Reviewer-Only Rule Index

| Rule | Enforcement |
|---|---|
| Category A secret denylist on label keys | `[guard-enforced, non-bypassable]` |
| Category B user-PII denylist on label keys | `[guard-enforced]` |
| Prefix denylist on label keys (`raw_*`, `meeting_id*`) | `[guard-enforced, bypassable]` — scanned before Category A/B and before `is_hashed_label()`; suppressible by `# pii-safe`, unlike Category A |
| Hashed/opaque suffix allow for Category B (`_hash`, `_sha256`, ...) | `[guard-enforced]` |
| Infrastructure-identity allowlist (`hostname`, etc.) | `[guard-enforced]` |
| Canonical shared-label names (prefer over drift aliases) | `[reviewer-only]` |
| snake_case label keys | `[guard-enforced]` |
| snake_case metric names | `[guard-enforced]` |
| Metric-name length ≤ 64 chars | `[guard-enforced]` |
| Label-value literal length ≤ 64 chars | `[guard-enforced]` |
| Obviously-unbounded value sources (`Uuid::*`, `request_path`, `SystemTime::now`) | `[guard-enforced]` |
| `# pii-safe: <reason>` escape hatch | `[guard-enforced]` (parsed and honored) |
| Reason ≥ 10 chars, not lazy | `[guard-enforced]` |
| Per-metric combinations ≤ 1000 | `[reviewer-only]` (design constraint) |
| Fleet-wide 5M series budget | `[runtime-enforced]` (Prometheus scrape layer) |
| Bounded-label pattern for unbounded sources | `[reviewer-only]` |
| Cryptographic strength of hashing (SHA-256+) | `[reviewer-only]` |
| Salt on small-identifier-space hashes | `[reviewer-only]` |
| Debug-format label values (`format!("{:?}", err)`) | `[reviewer-only]` |
| `# pii-safe` reason specificity / staleness | `[reviewer-gated]` (security review) |
| Denylist extensions | `[reviewer-gated]` (security + observability co-owned) |
| Allowlist extensions | `[reviewer-gated]` (prefer rename over allowlist) |
| Shared-label-name additions | `[reviewer-only]` (land here before second-service use) |
| `key_custody` bounded to the single value `operator` | `[reviewer-only]` (ADR-0036 §4 amendment required to extend) |
| No end-to-end / zero-trust boolean on any carrier | `[reviewer-only]` |
| **No meeting identifier — raw or hashed — on any metric** | **Split.** `[guard-enforced, bypassable]` for Rust metric labels — `meeting_id` in `PII_PREFIX_DENYLIST`, scanned before `is_hashed_label()` so `meeting_id_hash` is caught, but suppressible by `# pii-safe` (only Category A is not). `[reviewer-only]` for the TS client SDK (scanner reads `crates/`) and for trailing compounds like `x_meeting_id` (`starts_with`, not segment matching). See §Enforcement reality. |
| No participant / stream identity on media-path labels, log fields, span attributes or **exemplars** | `[reviewer-only]` — the media-path half is enforced by shape via the directory-scoped macro deny, not by vocabulary |

---

## Extension Policy

**PII denylist**: security + observability co-own. Extensions land via a
PR touching both this document AND
`crates/dt-guard/src/common/pii_vocabulary.rs` — the vocabulary lives there,
not in `scripts/guards/simple/validate-metric-labels.sh`, which has been a thin
dt-guard wrapper since ADR-0034 §3. Removals require security sign-off.

**Canonical shared labels**: observability owns the canonical table. Adding
an entry before second-service use is the expected workflow; mark the
bounded-values column or document the bounded-label pattern.

**Allowlist additions** (infrastructure-identity substring matches): avoid
when possible; prefer renaming the label to something that doesn't shadow a
PII token. When an allowlist entry is unavoidable, justify inline.
