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
enforce it) was made deliberately: the current fleet has drift (see §Current
Drift below), and machine-enforcing now would require a grandfather
allowlist whose cost exceeds the benefit. Coordinated renames are tracked
in TODO.md under "ADR-0031 label-canonicalization follow-ups".

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

### Current Drift (non-blocking)

As of 2026-04-17, the fleet has two remaining pieces of canonical-label
drift. Each is tracked in TODO.md; the renames are lower-priority than
ADR-0031 prereq completion and require coordinated migrations (PromQL
queries in dashboards and alert rules reference these labels).

1. **AC `path` + `status_code` ↔ GC `endpoint` + `status` (categorized)** —
   proposed canonical: `endpoint` (semantic path) + `status_code` (raw HTTP
   code). GC's `status` is a categorized string (`success`/`error`/
   `timeout`) derived from `status_code`; both have independent utility but
   the path-ish label name should align across services.
2. **MC + MH `event` ↔ AC `event_type`** — proposed canonical: `event_type`.

Resolved: MC bare `type` on heartbeat metrics was renamed to
`heartbeat_type` (FU#3b).

Each rename has PromQL ripple into dashboards (landed in c10dde2) and
alert rules (landed in f5f53f8); the owning service specialist drives
the migration. See TODO.md §"ADR-0031 label-canonicalization follow-ups".

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
