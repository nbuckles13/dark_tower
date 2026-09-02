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
`scripts/guards/simple/validate-metric-labels.sh` together.

**Authoritative ADRs**:
- [ADR-0011](../decisions/adr-0011-observability-framework.md) — metric
  taxonomy, cardinality budgets, SLO framework.
- [ADR-0031](../decisions/adr-0031-service-owned-dashboards-alerts.md) —
  service-owned alert/dashboard/metric authorship; this doc; the guard.

**Machine enforcement**: `scripts/guards/simple/validate-metric-labels.sh`
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
| `status` | Coarse outcome classification | `success`, `error`, `timeout`, `rejected`, `accepted` |
| `status_code` | Raw HTTP status code | 200-599, bounded (~60 distinct) |
| `endpoint` | Semantic HTTP path (normalized) | Bounded by `normalize_endpoint()` table |
| `operation` | Subsystem-specific verb | Bounded by code (e.g., `select`, `insert`, `update`, `delete`) |
| `error_type` | Bounded error variant | Bounded by the service's `error_type_label()` enum |
| `error_category` | Coarser error class | `authentication`, `authorization`, `cryptographic`, `internal` |
| `event_type` | Bounded event discriminator | Bounded by the event enum (e.g., `connected`, `disconnected`) |
| `region` | Geographic/deployment region | Bounded by cloud region set |
| `pod` | Kubernetes pod identifier | Per-pod — cardinality bounded by fleet size |
| `key_custody` | Who holds media key material | **`operator` — single permitted value.** See §Key custody below. |
| `reason` | Why a **frame** was dropped on the media path | Bounded by `proto/test-vectors/frame-v2.vectors.json` → `reject_reasons`. **Points, never restates** — restating the tokens here would give each one two homes and they would drift. Distinct from `error_type` / `error_category`, which classify why a **service operation** failed: `reason` is per-frame and lives entirely on the media path. See §Frame reject reason below. |

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

**All sixteen `reason` values are emitted individually as `reason` values, without exception
(R-25, R-31). Nothing below bears on that.** This section is where someone will come looking for
permission to coarsen the vocabulary, and everything after this sentence is about a different axis.
R-25 already settled the per-token question on 2026-08-31: *"'bucket' names a FAMILY of `reason`
values, not one collapsed `decode_reject` label"* — collapsing would destroy R-31's only lever, since
`unknown_version` staying individually visible is how a version-skewed rollback is detected, and
would break `sum by(reason)` comparability between MH and the client.

**"Without exception" above is about granularity, not about membership of the drop counter. Those are
different questions and only one of them has the answer "all sixteen".** Which values belong on
`dt_client_media_frames_dropped_total{reason}` is carried by **`drops_frame`** in
`proto/test-vectors/frame-v2.vectors.json` → `reject_reasons`. Fifteen are `true`.
**`wrap_key_id_mismatch` is `false`, and is the only one**: it describes a correctly self-signed frame
whose mis-bound wrapped key the receiver ignores (ADR-0036 §4), so the frame is otherwise processed
and **played**. Counting it as a drop breaks R-25's `received = played + sum(drops)` for a frame that
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
- `no_kek_for_generation`, `no_roster_entry`, `no_transmit_key`, `signature_invalid`,
  `decrypt_failed` and `unwrap_failed` are **receiver-state-dependent**.
- `no_transmit_key` is `layer: "codec"` yet fires on key-store membership, so an attacker choosing
  key ids reads it as *"does this receiver hold key id X?"* — which is why the layer-based version of
  this rule was wrong.
- `signature_invalid` and `decrypt_failed` are byte-determined *given fixed key material*, so they
  carry `has_vector: true` while sitting on the restricted side — which is why the `has_vector`
  version is wrong too.

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

> **Do not reach for the vocabulary guard to enforce this.** Adding `meeting_id` to the Category B
> denylist is **inert against the realistic spelling**: `meeting_id_hash` ends in `_id_hash`, which
> `HASHED_SUFFIXES` exempts via `is_hashed_label()`, so the entry would be defeated by the very
> mechanism it invokes — while reading as coverage. This is ADR-0036 §11's "vocabulary additions
> cannot be cited as the protection" as a concrete instance rather than a general warning.

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

Aggregate distributions are safe. What is prohibited is resolution that reconstructs a *sequence*:
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

> **This is a required end state, not a description of current behaviour.** Today the SDK's implicit
> join label set — `client_version`, `meeting_id_hash`, `org_id` — is threaded from
> `MeetingSession.join` into **every** emission site including the media module
> (`packages/sdk-core/src/media/events.ts`), and `packages/sdk-core/src/media/__tests__/media-transport.test.ts`
> **asserts** `meeting_id_hash` in that label set. So media-path client metrics would inherit a
> meeting identifier **by inertia** unless the exemption is applied deliberately. Tracked in
> `docs/TODO.md`; owner: client.

Stated in the present-tense-required form because the inertia is the whole risk: the label set arrives
by default, and doing nothing is what ships the violation.

### Enforcement reality — read this before citing these rules as coverage

**R1, R2 and R3 are `[reviewer-only]`. No guard enforces them today.**

- `meeting_id` appears in **no** guard vocabulary.
- `meeting_id_hash` is **actively exempted** by `HASHED_SUFFIXES` → `is_hashed_label()`.
- The durable enforcement for the media-path half is by **shape, not vocabulary**: a directory-scoped
  deny of log and metric macros across the whole media path, which catches the offending *form*
  rather than a spelling. Word-boundary vocabulary matching cannot see inside compounds, so a
  prefixed spelling ships clean while the guard reports green.

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

Labels whose key starts with `raw_` are flagged regardless of the suffix.
The `raw_` prefix signals that the author knew the value was sensitive and
opted out of sanitization — a pattern worth surfacing for review. Examples:
`raw_email`, `raw_user_id`, `raw_request_id`.

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
> *cardinality and aggregation* rule and is independent of it. A hashed meeting id ends in `_id_hash`
> and is therefore waved through by `is_hashed_label()` — it is exempted **by construction** — while
> still being prohibited. Read §Media-path identity before adding any meeting-scoped label.

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
land via a PR updating both this file AND `validate-metric-labels.sh`.
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
- Prefix denylist match (`raw_*`).

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
| `raw_*` prefix denylist on label keys | `[guard-enforced]` |
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
| **No meeting identifier — raw or hashed — on any metric** | `[reviewer-only]` — **NOT guard-enforced**; `meeting_id` is in no vocabulary and `meeting_id_hash` is actively exempted by `HASHED_SUFFIXES` |
| No participant / stream identity on media-path labels, log fields, span attributes or **exemplars** | `[reviewer-only]` — the media-path half is enforced by shape via the directory-scoped macro deny, not by vocabulary |

---

## Extension Policy

**PII denylist**: security + observability co-own. Extensions land via a
PR touching both this document AND
`scripts/guards/simple/validate-metric-labels.sh`. Removals require
security sign-off.

**Canonical shared labels**: observability owns the canonical table. Adding
an entry before second-service use is the expected workflow; mark the
bounded-values column or document the bounded-label pattern.

**Allowlist additions** (infrastructure-identity substring matches): avoid
when possible; prefer renaming the label to something that doesn't shadow a
PII token. When an allowlist entry is unavoidable, justify inline.
