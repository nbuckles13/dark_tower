# Devloop Output: Disambiguate GC's Three Meeting-Refusal Causes

**Date**: 2026-08-14
**Task**: Make GC distinguish cap-exhausted / organization-missing / organization-inactive when meeting creation is refused (story R-6)
**Specialist**: global-controller
**Mode**: Agent Teams (v2)
**Branch**: `feature/story-runner-hardening`
**Duration**: ~5h (setup 21:30Z 2026-08-14 → commit 01:10Z 2026-08-15)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `0533d4cf4d57439d84332d01a16f8d6e499296be` |
| Branch | `feature/story-runner-hardening` |
| Headless | yes (`DEVLOOP_HEADLESS=1`, run-story task #2) |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test | `test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |
| Database (conditional) | `database` (spawned) |

**Conditional reviewer rationale**: the change touches repository SQL (`repositories/meetings.rs` CTE)
and the implementer is `global-controller`, not `database` — per SKILL.md §Team Composition, Database
joins as the conditional domain reviewer.

### Gate 1 — Plan Confirmations

Classification-sanity guard (run by Lead before "Plan approved"):
`scripts/guards/simple/validate-cross-boundary-classification.sh` →
`STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

**Gate 1 CLOSED 2026-08-14 — all eight confirmed. "Plan approved" issued.** Guard re-run against the
revised table (three rows added during the gate): `STATUS=OK`.

| Reviewer | Plan Status | Note |
|----------|-------------|------|
| Security | confirmed | No classification upgrades. Ruled 500 on org-not-provisioned satisfies the coarse-response requirement *better* than 403 — a 403 asserts an authorization decision about a caller who is correctly authenticated and authorized. Raised S-6 (pre-existing cap race): doc-comment fix now, locking strategy spun out to @database. |
| Test | confirmed | 9 findings, 9 accepted, 0 deferred. T-1 (the `LEAST` capping branch has never executed in any test), T-2(b) (the `metric_label()` middle link), T-6/7/8 (adopted from @database), T-9 (compile-forced enumeration). |
| Observability | confirmed | Hunk-ACK for `docs/observability/metrics/gc-service.md`. 9-of-10 label budget accepted with an explicit headroom note. Owns and writes the `gc-alerts.yaml` hunk. |
| Code Quality | confirmed | CQ-1 (new panic surface: `map_row_to_meeting` `Row::get` on all-NULL refusal rows → `try_get` + `Result`, ADR-0002) and CQ-2 (`#[expect(dead_code)]` reason names `Conflict`, self-retiring) both accepted, plus four minors. |
| DRY | confirmed | Tested the "zero production callers" claim rather than accepting it — it holds. Swept 18 zero-row sites vs the plan's 6; no additional class members. 2 ADR-0019 extraction entries at verdict time. |
| Operations | confirmed | **Withdrew O-2 on measurement** (no 5xx-only alert exists anywhere in `infra/docker/prometheus/rules/`). Held P-1 and P-5, both accepted. Runbook hunk-ACK conditional on a nine-item Gate 3 contract. |
| Semantic Guard | confirmed | Finding A (the `(500, INTERNAL_ERROR)` collision) reached independently of @operations' P-1. B/C/D returned as checkable commitments. |
| Database | confirmed | SQL verified independently, not merely recognised: `fetch_one` sound, wCTE executes once, INNER join closes the `LEAST`-NULL hazard, no migration/index/`.sqlx`. Pre-flagged verdict will be RESOLVED-DEFERRED on the accepted S-6 spin-out. |

### Defects the gate caught in the plan

Five, of which two would have shipped a false contract into task #3:

1. **Decision 4's cross-task contract was unreachable.** AC's `org_extraction` resolves the org by subdomain filtered `is_active = true` and fails closed, so a never-provisioned org dies at **token acquisition** and never reaches GC. A task-#3 detector keyed on a GC create status would have been permanently dead code, and the real failure would have landed on the suite lane as an unattributed auth error — the exact misattribution R-7 exists to prevent.
2. **`(500, INTERNAL_ERROR)` was never a distinct tuple.** `create_meeting` already emits it from RNG failure (`handlers/meetings.rs:199-202`, `:207-210`) and collision-retry exhaustion (`:254`, `:270`). The first fix for a three-way collapse had reintroduced the same collapse one layer out, in error-code space, against causes outside R-6's frame. Fixed with a dedicated `ORGANIZATION_NOT_PROVISIONED` code.
3. **The restructured CTE opened a new panic surface** — the always-one-row outer SELECT returns all-NULL `i.*` columns on every refusal path, and `map_row_to_meeting` decodes with panicking `Row::get`.
4. **The `org_inactive` justification was false.** There is no org lifecycle in this repo at all — zero sites setting `is_active = false`, zero deleting an org, no org CRUD. Replaced with the modeled-vs-unmodeled-state axis.
5. **`repositories/meetings.rs:8`'s TOCTOU claim is overstated** — true against application-level check-then-insert, false against two concurrent executions of the statement.

### What this change actually delivers (corrected at Gate 1)

R-6's justification in the story is partly wrong, and the correction is being written into
`docs/user-stories/2026-08-11-story-runner-hardening.md` Task 3 notes rather than only here — task #3
is implemented against that file. **This change does not give task #3 a provisioning-fault signal**;
a provisioning failure cannot reach `POST /api/v1/meetings`. What it delivers is that
`403 + ORGANIZATION_MEETING_LIMIT_EXCEEDED` now means a real cap and nothing else, so the runner can
trust a cap signal instead of suspecting a provisioning fault behind it — **removal of a false
positive, not addition of a signal**. Task #3's operator lane must come from Phase 1 verifying
provisioning directly, which is where `layer7.sh:403` already puts the only infra lane.

**Open at Gate 1 — Decision 4 is factually wrong (raised by @database).** Decision 4 tells task #3
that a provisioning fault is observable as `500` + `INTERNAL_ERROR` on `POST /api/v1/meetings`. It
cannot be: that route sits behind `require_user_auth` (`routes/mod.rs:137,145-148`), and AC only mints
`UserClaims` after `org_extraction` resolves the Host subdomain via `get_by_subdomain`, which filters
`is_active = true` and fails closed (`ac-service/src/middleware/org_extraction.rs:113-121`). An
unprovisioned org therefore fails at **token acquisition** and never reaches GC. A task-#3 detector
keyed on GC's 500 would be silently dead, and the real failure would surface as an unattributed auth
error on the suite lane — the exact misattribution R-7 exists to prevent. The 500 lane is correctly
"org deleted under a live token", not "org never provisioned". Prose correction required before
"Plan approved"; no SQL or Rust impact.

**Scope additions accepted at Gate 1** (must be reflected in the classification table before Gate 2,
or the Layer A scope-drift guard will flag them):
- `infra/docker/prometheus/rules/gc-alerts.yaml` — new `GCMeetingCreationProvisioningFault` rule, written by @observability as their own hunk (Minor-judgment, Owner observability), in addition to the pre-agreed `description` fix.
- `docs/runbooks/gc-deployment.md:1232,1243` — stale `error_type="forbidden"` selectors that mean role denial after this change. Was in no classification table row when found; @operations took the file and it now has one.
- `crates/gc-service/src/repositories/meetings.rs:8` — doc-comment correction per S-6 (the atomic-CTE claim is true against application-level check-then-insert, not against two concurrent executions of the statement).

---

## Task Overview

### Objective

`MeetingsRepository::create_meeting_with_limit_check` returns `Ok(None)` for three unrelated
conditions — the org's concurrent-meeting cap is full, the org row does not exist, the org is
inactive — because the `INSERT ... SELECT FROM org_limits, current_count WHERE cnt < max` yields
zero rows in all three cases. `create_meeting()` maps `Ok(None)` to a single
`403 "Organization meeting limit exceeded"`.

Give each cause a distinct observable outcome so a provisioning failure cannot be mistaken for a
full cap. Source requirement: `docs/user-stories/2026-08-11-story-runner-hardening.md` R-6.

### Scope
- **Service(s)**: gc-service (GC)
- **Schema**: No (query shape only; no migration)
- **Cross-cutting**: No — GC-internal, though task #3 (layer-7 per-run org provisioning) depends on
  the operator-lane distinction landing here.

### Debate Decision
NOT NEEDED — the requirement is already decided in the accepted story (R-6); this is a
single-service behavior change with no cross-service contract impact.

---

## Cross-Boundary Classification

Every file this plan intends to change gets a row, per SKILL.md §Cross-Boundary Edits.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/gc-service/src/repositories/meetings.rs` | Mine | — |
| `crates/gc-service/src/repositories/mod.rs` | Mine | — (re-exports for the two new types) |
| `crates/gc-service/src/handlers/meetings.rs` | Mine | — |
| `crates/gc-service/src/errors.rs` | Mine | — |
| `crates/gc-service/src/models/mod.rs` | Mine | — (added at Gate 3 for @security S-7: hand-rolled `Debug` redacting `join_token_secret`) |
| `crates/gc-service/src/observability/metrics.rs` | Mine | — (label-value *names* pre-agreed with observability; recording site is GC-owned) |
| `crates/gc-service/tests/meeting_create_tests.rs` | Mine | — |
| `crates/gc-service/tests/meeting_creation_metrics_integration.rs` | Mine | — |
| `crates/gc-service/tests/db_metrics_integration.rs` | Mine | — |
| `docs/API_CONTRACTS.md` | Mine | — (GC owns the public REST surface; REST only, no `proto/`) |
| `docs/TODO.md` | Mine | — (two spin-out entries: the concurrent-cap race, owner database; and the classification-table NOT-TOUCHED gap, owner operations + database) |
| `infra/grafana/dashboards/gc-overview.json` (panel `:2796` `description` only — no query/threshold/layout change) | Mine | — (service-owned dashboard per ADR-0031, observability cross-cutting reviewer; added at Gate 3 after @observability found the stale prose) |
| `docs/specialist-knowledge/global-controller/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-08-14-gc-meeting-refusal-causes/main.md` | Mine | — |
| `docs/observability/metrics/gc-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/alerts.md` | Not mine, Minor-judgment — @observability's hunk (catalog entry for the new alert rule, added after @operations found it had none) | observability |
| `infra/docker/prometheus/rules/gc-alerts.yaml` | Not mine, Minor-judgment — **@observability implements this hunk themselves** (`GCMeetingCreationFailureRate` description fix + new `GCMeetingCreationProvisioningFault` rule keyed on the token-TTL window). Listed so Layer A scope-drift sees it. I owe them only the frozen label values, which are final. | observability |
| `docs/runbooks/gc-deployment.md` (`:1232`, `:1243` — stale `error_type="forbidden"` post-deploy checks; `:1243` becomes *actively false* after this change) | Not mine, Minor-judgment | operations |
| `docs/user-stories/2026-08-11-story-runner-hardening.md` ("Task 3 notes" — one line: provisioning failure surfaces at AC token acquisition, not GC meeting creation) | Not mine, Minor-judgment | operations |
| `docs/runbooks/gc-incident-response.md` (Scenario 8 Symptoms + Common Root Causes) | Not mine, Minor-judgment | operations |
No path in this changeset is inside a Guarded Shared Area: no `proto/**`, no `crates/common/src/{jwt,meeting_token,token_manager,secret}.rs`, no `crates/ac-service/src/{jwks,token,crypto,audit}/**`, no migration.

### Deliberately NOT touched

@database asked for these to be explicit rather than left as omissions, and that reasoning stands —
but they cannot live in the table above. `validate-cross-boundary-scope` parses **every** table row as
a planned file and flags plan entries the diff does not touch, so a "NOT TOUCHED" row is
indistinguishable from an unfulfilled plan entry. The table has no vocabulary for deliberate absence.
Recorded here instead, with the original reasoning preserved verbatim:

- **`migrations/**`** — no schema change needed; `organizations.is_active` already exists
  (`migrations/20250118000001_initial_schema.sql:14`). Called out because `migrations/**` is a Guarded
  Shared Area by the ADR-0024 §6.4 *criterion* (schema evolution) even though the enumerated path
  reads `db/migrations/**`, which matches zero tracked files in this repo. The criterion governs, not
  the glob.
- **`sqlx-data.json` / `.sqlx/`** — GC uses runtime `sqlx::query()` with `.bind()` and has zero
  `query!` macros, so there is no offline data to regenerate (confirmed by @database).

**This section is the interim, not the endpoint** (@database, correcting my first justification). I had
argued against an in-table `NOT-TOUCHED` marker on the grounds that it would make the guard "parse
intent from prose." That is wrong, and it should not be what story-close records: a marker would be a
**structured token**, and the assertion behind it — *no diff path matches this glob* — is a
set-membership test on `git diff --name-only` with no intent-parsing anywhere. It is **more**
mechanically checkable than the positive case the guard already runs, and cheaper.

The honest trade is that a prose section swaps a machine-checked claim for a human-checked one. Nothing
stops a future devloop carrying "deliberately not touched: `migrations/**`" while its diff touches
`migrations/`. This loop is safe because the diff genuinely doesn't — and Gate 2's scope guard *proved*
it by flagging the row — but that is a property of this diff, not of the design. It matters here
specifically because `migrations/**` is a **Guarded Shared Area**: untouched-ness is exactly the claim
that ought to be verifiable, and prose is the one direction that makes it less so.

Durable fix recorded as **"the scope guard grows a `NOT-TOUCHED` assertion"**, with this section as the
interim. Filed under `docs/TODO.md` §From ADR-0024 §6 Amendment. Not overclaimed: such an assertion is
**vacuously true for a glob matching nothing**, so it would *not* have caught the `db/migrations/**`
dead-glob problem — that needs @operations' separate ≥1-tracked-file check. Two distinct holes; the
marker closes one of them.

---

## Planning

### Mechanism restatement (wider class check)

Instance-language (the task): *"the meeting-create CTE collapses three refusal causes."*

Mechanism-language: **a query whose zero-row result encodes several unrelated preconditions, leaving
the caller to invent a single cause for all of them.** The bug is not "the wrong 403" — it is that
`Option`/`bool` is being used as the return type of an operation with a multi-valued refusal domain.

Same-owner siblings in GC, surveyed:

| Site | Collapse? | In scope? |
|------|-----------|-----------|
| `repositories/meetings.rs:create_meeting_with_limit_check` | Yes — 3 causes → `None` | **Yes** (this task) |
| `repositories/meetings.rs:activate_meeting` | Yes — `None` collapses meeting-missing / already-active / ended / cancelled | No — `#[allow(dead_code)]`, zero production callers, therefore no observable outcome to disambiguate |
| `repositories/participants.rs:remove_participant` | Yes — `bool` collapses meeting-missing / never-joined / already-left | No — zero production callers |
| `handlers/meetings.rs:find_meeting_by_code` / `find_meeting_by_id` / `update_meeting_settings_in_db` | No — single precondition each (row absent → 404) | No |
| `crates/ac-service/src/repositories/organizations.rs:get_by_subdomain` | Yes — identical `WHERE ... AND is_active = true` collapse | No — different owner (auth-controller); @dry-reviewer is TODO-tracking it as an ADR-0019 extraction |

**Conclusion surfaced to reviewers:** the wider class exists, but every other GC member of it is
currently unreachable from production, so the task's framing is correctly sized *today*. What
generalises is the **shape**, not the fix: `participants.rs:38` and `:84` both name
`create_meeting_with_limit_check` as the pattern the future join-capacity CTE must copy. Those two
doc pointers stay accurate — the function keeps its name and its single-statement atomicity — and the
new outcome enum is the thing that future CTE should mirror.

### Decision 1 — one statement, not two (S-1, O-4, @database 1/2)

The cause is resolved **inside the existing CTE**. No pre-flight `SELECT`, no follow-up diagnostic
query. The restructured statement always returns **exactly one row**, so the repository moves from
`fetch_optional` to `fetch_one` and "empty result" stops being a representable state:

```sql
WITH org_row AS (
    SELECT org_id, is_active, max_concurrent_meetings, max_participants_per_meeting
    FROM organizations WHERE org_id = $1
),
current_count AS (
    SELECT COUNT(*) AS cnt FROM meetings
    WHERE org_id = $1 AND status IN ('scheduled', 'active')
),
inserted AS (
    INSERT INTO meetings (...)
    SELECT $1, $2, $3, $4, $5,
           LEAST($6, o.max_participants_per_meeting),
           $7, $8, $9, $10, $11, $12, $13, 'scheduled'
    FROM org_row o, current_count c            -- INNER join, deliberate (see LEAST footgun)
    WHERE o.is_active AND c.cnt < o.max_concurrent_meetings
    RETURNING <unchanged 21-column list>
)
SELECT
    CASE WHEN i.meeting_id IS NOT NULL THEN 'created'
         WHEN o.org_id IS NULL          THEN 'org_not_provisioned'
         WHEN NOT o.is_active           THEN 'org_inactive'
         ELSE 'org_limit' END AS outcome,   -- literal == metric label value, all three
    i.*
FROM (SELECT 1) AS _one
LEFT JOIN org_row  o ON true
LEFT JOIN inserted i ON true
```

Properties this preserves and why they matter:

- **Atomicity / no TOCTOU** — the cap check and the INSERT remain one statement. The security claim
  at `repositories/meetings.rs:8` stays true. (S-1, @database 1.)
- **Classification cannot disagree with the decision** — the diagnostic branches read the *same* CTE
  (`org_row`) the INSERT consulted, under one snapshot. A follow-up SELECT could report a stale cause;
  this cannot. Strictly better than the "acceptable" fallback shape S-1 offered.
- **One round trip on the success path** (O-4), unchanged.
- **`LEAST` footgun avoided** — the INSERT's source stays an INNER join (`FROM org_row o, current_count c`).
  `LEAST($6, NULL)` returns `$6` in Postgres, so a LEFT JOIN there would silently uncap
  `max_participants` for a missing org. LEFT JOINs appear **only** in the outer diagnostic SELECT.
  `LEAST($6, o.max_participants_per_meeting)` semantics are byte-identical to today — which task #3
  depends on, since an env-test asserts `max_participants_per_meeting = 100`. (@database 4.)
- **`is_active` moves out of the org lookup and into the INSERT's `WHERE`** — that filter is precisely
  what collapsed missing and inactive. Insert-path behaviour is unchanged: an inactive org produces no
  candidate row either way. (@database 3.)
- **Fully parameterized** — all 13 binds keep their positions and their `// $N` comments; the outcome
  discriminator is a SQL literal in a `CASE`, never interpolated. (@database 8.)
- **No migration, no new index, no `.sqlx` regeneration.** (@database 6/7.)

**Cause precedence, stated deliberately** (@database 5): `org_not_provisioned` → `org_inactive` →
`cap_exhausted`. An inactive org that is also at cap reports *inactive*. Org state is the
provisioning-level, more actionable fact, and this ordering is exactly what stops "a provisioning bug
looks like a full cap." Documented in the doc comment, not only in the `CASE`.

### Decision 2 — typed outcome, and the SQL string never reaches a label (@observability addendum, @dry 4, @code-reviewer 1)

```rust
/// Why creation was refused. One home for the cause taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingRefusal {
    OrganizationNotProvisioned,
    OrganizationInactive,
    CapacityExhausted,
}

impl MeetingRefusal {
    /// Bounded `&'static str` for the `error_type` metric label (ADR-0011).
    pub fn metric_label(self) -> &'static str { /* exhaustive match */ }
}

pub enum CreateMeetingOutcome {
    Created(Box<MeetingRow>),
    Refused(MeetingRefusal),
    MeetingCodeTaken,   // secondary, see Decision 5
}
```

- The DB discriminant string is parsed in **one** place, by an exhaustive `match` on `&str`. The `_`
  arm emits `error!` ("create_meeting returned an unrecognised outcome discriminant") and returns
  `GcError::Database` — the string is **never** passed through, and the fallback is deliberately an
  **error class, never a business cause**. Bucketing drift as cap-exhausted would silently recreate
  R-6's bug in a form that looks deliberate and survives review. So a future fourth `WHEN` arm cannot
  mint a new time series: it lands on the existing bounded `db_error` label and 500s loudly.
  (@observability correction + @database addendum A, agreed.)
- Label strings are derived from the enum via `metric_label()`, not retyped at call sites — same
  precedent as `GcError::error_type_label()`. (@dry 4.)
- Handler `match` is exhaustive, so a fourth cause is a compile error at the mapping site. (@code-reviewer 1.)
- `Created` is boxed: `MeetingRow` is 21 fields, far over clippy's `large_enum_variant` 200-byte delta
  against a 1-byte `MeetingRefusal`. (@code-reviewer 1.)
- Cause type is deliberately *not* meeting-specific in shape — the future join-capacity CTE can define
  its own `ParticipantRefusal` with the same three-arm structure. Sharing one enum across two different
  cause domains would be false reuse. (@dry 5.)

### Decision 3 — the wire contract (R-6's "distinguishable from the response alone")

The discriminator is the **HTTP status + the machine-readable `error.code`**, never the message prose.

| Cause | HTTP | `error.code` | client-visible `message` | `error_type` label | log |
|-------|------|--------------|--------------------------|--------------------|-----|
| cap exhausted | 403 | `ORGANIZATION_MEETING_LIMIT_EXCEEDED` | `Organization meeting limit exceeded` | `org_limit` | `warn` |
| org inactive | 403 | `ORGANIZATION_INACTIVE` | `Organization is not active` | `org_inactive` | `warn` |
| org not provisioned | **500** | **`ORGANIZATION_NOT_PROVISIONED`** | `An internal error occurred` (fixed generic) | `org_not_provisioned` | `error` |
| *(unchanged)* role denial | 403 | `FORBIDDEN` | `Insufficient permissions to create meetings` | `forbidden` | `warn` |

**Three** new `GcError` variants, all **unit** variants (no `String` payload):

```rust
#[error("Organization meeting limit exceeded")] OrgMeetingLimitExceeded,  // 403
#[error("Organization is not active")]          OrgInactive,              // 403
#[error("Organization is not provisioned")]     OrgNotProvisioned,        // 500, generic body
```

Unit variants make it *structurally impossible* to leak an org UUID, subdomain, cap value or live
count into a body that `errors.rs:138` echoes verbatim (S-3). `status_code()`, `error_type_label()`
and the `IntoResponse` `code` string all derive from the variant — one home per fact.

`OrgNotProvisioned` behaves like `GcError::Database`: a **distinct envelope `code`** plus a **fixed
generic `message`** and a server-side `tracing::error!` carrying `org_id`. Precedent is adjacent —
`Database` → `500` + `DATABASE_ERROR` + "An internal database error occurred" (`errors.rs:118-127`).

> **Revised at Gate 1 after @operations P-1.** My first draft reused `GcError::Internal` here, giving
> `(500, INTERNAL_ERROR)`. That is **not a distinct tuple on this endpoint**: `INTERNAL_ERROR` is the
> envelope code for *every* `GcError::Internal`, and `POST /api/v1/meetings` already emits it from
> code-collision exhaustion (`handlers/meetings.rs:254`, `:270`) and RNG failure (`:201`, `:209`).
> R-6 would have gone unmet for this cause — a caller could not tell a missing org from an exhausted
> retry loop — and task #3 would have routed two genuine *implementer* bugs onto the operator lane,
> which is R-4's defect with the lanes swapped, inside the story that exists to make lanes
> trustworthy. A distinct code with a non-revealing body costs the security argument nothing.

**Rationale for 500 on org-not-provisioned — and the one place I am overriding a reviewer constraint.**
@security's addendum asks for a coarse client response here (5xx or 401, detail in logs/metrics only);
@operations O-2 asks that none of the three become 5xx, on the grounds that it is a self-inflicted
page. I measured the alert rules before choosing:

- `GCHighErrorRate` (`gc-alerts.yaml:33`, severity `page`), `GCErrorBudgetBurnRateCritical` (`:101`,
  `page`) and `GCErrorBudgetBurnRateWarning` (`:250`) all count
  `gc_http_requests_total{status_code=~"[45].."}` — **4xx and 5xx contribute identically**. `grep` for
  a 5xx-only alert in `gc-alerts.yaml` returns nothing.
- So 403-vs-500 changes **no** alert numerator, and O-2's paging mechanism does not distinguish the two.
  The remaining substance of O-2 — "must not look retryable" — is satisfied: 500 is not 503, there is
  no `Retry-After`, and `packages/sdk-core/src/http/parse.ts` has **no retry loop** (only `Retry-After`
  *parsing*, for 429). @test's retry-masking concern checks out clean.
- Client ripple is nil: `MeetingError.fromResponse` (`packages/sdk-core/src/errors/MeetingError.ts:28`)
  maps 500 to the generic `MeetingError` and forwards `serverCode`. The 404 hazard @operations flagged
  (`MeetingNotFoundError` on a *create* call) is avoided precisely because I did **not** pick 404.
- A valid AC-signed token whose `org_id` has no row is a server-side state fault, and — per
  @database's Gate 1 analysis — it can mean **only one thing: an org row was deleted out-of-band
  under a live credential.** It is *not* reachable from a provisioning gap, because AC's
  `org_extraction` middleware fails closed: it resolves the Host subdomain through
  `organizations::get_by_subdomain`, which filters `WHERE subdomain = $1 AND is_active = true`
  (`ac-service/src/repositories/organizations.rs:43`) and returns
  `AcError::NotFound` (`middleware/org_extraction.rs:113-121`). An unprovisioned or inactive org
  therefore never yields a token at all, so the request never reaches GC.
  `grep -rn "DELETE FROM organizations" crates/ scripts/ infra/` returns **nothing** — no code path
  deletes an org — so the only producer is manual SQL or a test fixture. AC and GC share one
  `organizations` table, so this is not two datastores disagreeing; it is a row deleted (cascading its
  users away via `users.org_id ON DELETE CASCADE`, `initial_schema.sql:26`) while a signed token
  referencing it keeps validating for up to `TOKEN_EXPIRY_SECONDS = 3600`, with GC's auth middleware
  doing no DB lookup. Bounded ≤1h window, provably zero steady state. A referential-integrity
  violation that the FK cannot express, because a JWT is not a foreign key. Nothing the caller did is
  wrong and nothing the caller can do fixes it. "Fail loudly; never mask" says the honest code is 5xx;
  a 403 would present state corruption as a routine authorization decision.

I am recording this as my domain call under the task's explicit grant, with @operations' objection
answered by measurement rather than dismissed. **@operations: if the alert-regex evidence changes your
read, say so at Gate 1 and I will move it to 403 + `ORGANIZATION_NOT_PROVISIONED` — that variant is a
three-line change and everything else in the plan is unaffected.**

### Decision 4 — what this contract does and does not give task #3

> **Rewritten at Gate 1 after @database's and @test's finding.** My first draft told task #3 to key its
> `PRECONDITION_FAILURE` / exit-2 detector on `500` from `POST /api/v1/meetings`, described as "a
> provisioning fault." **That detector could never fire for the failure R-7 actually has**, and
> shipping the prose would have handed task #3 a gate that passes by never running — the anti-pattern
> this very story names in R-5.

**Why an unprovisioned org never reaches GC.** `POST /api/v1/meetings` sits behind `require_user_auth`
(`routes/mod.rs:135-148`) and needs `UserClaims`. AC mints those only after `org_extraction` resolves
the Host subdomain through `organizations::get_by_subdomain`, which filters
`WHERE subdomain = $1 AND is_active = true` (`ac-service/src/repositories/organizations.rs:43`) and
**fails closed** with `AcError::NotFound` (`ac-service/src/middleware/org_extraction.rs:112-121`). So
if R-7's per-run org is missing *or* inactive, the run dies at **token acquisition**. Both non-cap
causes here are reachable only when the org row changes state *after* a token was minted and within
the ≤1h TTL.

**What this task therefore delivers:** response-level distinguishability of the three refusals, so a
refusal that *does* reach GC is never misread as a full cap. That is R-6, in full.

**What task #3 must do instead:** key its operator-lane detector on **AC token acquisition failing**,
not on any GC meeting-create status. Stated here so task #3 reads it off the plan rather than
rediscovering it.

The response contract, pinned by `test_meeting_refusal_causes_are_pairwise_distinct`:

| Condition | HTTP | `error.code` | reading | reachable when |
|---|---|---|---|---|
| org row missing (out-of-band deletion) | 500 | `ORGANIZATION_NOT_PROVISIONED` | unmodeled state — DB mutated outside any code path | ≤1h decay window after the row is deleted under a live token |
| org deactivated mid-run | 403 | `ORGANIZATION_INACTIVE` | modeled state — `is_active` has defined semantics | ≤1h decay window after deactivation under a live token |
| genuine cap exhaustion | 403 | `ORGANIZATION_MEETING_LIMIT_EXCEEDED` | routine policy refusal | always |

**Stated plainly, what R-6 buys** (@operations P-5 point 1): `403 + ORGANIZATION_MEETING_LIMIT_EXCEEDED`
now means a genuine cap **and nothing else**. That is the removal of a false positive — the runner can
trust a cap signal instead of treating it as possibly-a-provisioning-fault. What it does **not** buy is
a positive provisioning-fault signal, for the reachability reason above.

**Where task #3's operator-lane signal must come from** (@operations P-5 point 3): Phase 1 verifying
provisioning directly — org exists, `is_active`, token obtainable — *before* the suites run. Failing
that check is the `PRECONDITION_FAILURE` / exit 2. Phase 1 is already the only infra lane
(`scripts/layer7.sh:403`), and detecting a bad org before burning a suite run beats inferring it from
a mid-suite HTTP response. I am putting this one-liner into
`docs/user-stories/2026-08-11-story-runner-hardening.md` under "Task 3 notes", because that is the
file task #3's implementer reads — a devloop output is not.

**Why 403 for inactive and 500 for missing, corrected** (@database's second finding): my first draft
justified the split as "routine business event vs fault." That premise is false — `grep` finds **zero**
sites setting `organizations.is_active = false` anywhere in the repo, and zero deleting an org, so
neither is currently reachable from any application flow. The justification that survives is
**modeled vs unmodeled state**: `is_active` is a designed column whose semantics the schema and AC's
queries both honour, so 403 is honest even though nothing writes it today; a live signed token whose
org row does not exist is a referential-integrity violation the schema *cannot* express, so 500 is
honest. That distinction holds regardless of reachability.

### Decision 5 — secondary fix, explicitly droppable: meeting-code collision detection

`handlers/meetings.rs:242` classifies a code collision by `e.contains("unique constraint") || e.contains("duplicate key")`
— English-prose matching, the same defect class R-6 removes, sitting in the very `match` I am editing.
@code-reviewer 4 invited folding it in. The repository will classify with SQLSTATE + constraint name
(`23505` on `meetings_org_code_unique`, per `migrations/20250118000001_initial_schema.sql:63`) and
return `CreateMeetingOutcome::MeetingCodeTaken`; the handler matches the variant. Any *other* 23505
still falls through to `GcError::Database`, so nothing is silently swallowed.

Care taken: the 23505 path keeps emitting `record_db_query("create_meeting", "error", …)`, so the
existing driven assertion at `tests/db_metrics_integration.rs:411` (which expects `status="error"` on
a duplicate code) stays green — the statement did fail; only its *classification* moves out of prose.

**This is separable.** If Gate 1 would rather keep R-6 minimal, say so and I will drop it wholesale;
nothing else in the plan depends on it.

### Decision 6 — logging (O-5, @observability 5)

The `Ok(None)` arm emits **no log at all** today. Three distinct **static** messages (no cause
interpolated into a format string, so log-based grouping works), all
`target: "gc.handlers.meetings"` with `org_id` and `user_id` as structured fields:

- `warn!` — "Meeting creation refused: organization concurrent-meeting limit reached"
- `warn!` — "Meeting creation refused: organization is not active"
- `error!` — "Meeting creation refused: organization referenced by a valid token does not exist"

`org_id` goes in log fields **only** — never a metric label (S-5, @observability 6). It reaches the
label position nowhere; the only label values are `&'static str` from `MeetingRefusal::metric_label()`.

### Decision 7 — metric taxonomy

New `error_type` values on the **existing** `gc_meeting_creation_failures_total` — no new metric name
(@observability 2, avoids `metric_no_dashboard` / `metric_no_catalog` / `uncovered_metric`).
Dashboard panel `gc-overview.json:2875` is already `sum by(error_type)`, so it picks them up with zero
dashboard edits.

Post-change value set, **9 values** (ADR-0011 cap is 10 — 1 slot headroom, flagged as tight):

`bad_request`, `unauthorized`, `forbidden`, `code_collision`, `db_error`, `internal`,
**`org_limit`**, **`org_inactive`**, **`org_not_provisioned`**

`forbidden` is thereby narrowed to role denial only — cap exhaustion moves off it, resolving the
overload @observability 3 flagged. Alert `GCMeetingCreationFailureRate` keeps its expr, severity and
`for:`; only its `description` annotation changes, because it currently attributes failures to
"database issues, org limit exhaustion, or code collision problems" — the exact conflation R-6 removes.

### Decision 8 — S-2: pin that the org is token-derived

The whole safety argument (no enumeration oracle) rests on `org_id` coming from `user_claims.org_id`
with no client-supplied selector. Two things land:

1. A comment at the classification site stating the distinction is safe **only** because `org_id` is
   token-derived, and that adding any org selector to `CreateMeetingRequest` turns these three
   outcomes into a live cross-tenant oracle.
2. `test_org_selector_cannot_be_supplied_by_client` — (a) a body carrying `orgId` / `org_id` is
   rejected 400 by `deny_unknown_fields`; (b) a two-org fixture where the caller's token org is at cap
   and a *different* org is healthy still yields the cap outcome, proving the probed org follows the
   token and nothing else.

### Test plan

Each new HTTP test must be **measured** red at `0533d4c`, not assumed. Method: apply the test-only
hunks, revert the `src/` hunks, run, paste the actual failure line into §Devloop Verification Steps.
All tests use `#[sqlx::test(migrations = "../../migrations")]` per-test databases with unique
subdomains — no shared state, no ordering assumptions.

`crates/gc-service/tests/meeting_create_tests.rs`:

| Test | Asserts | Red at `0533d4c` because |
|------|---------|--------------------------|
| `test_create_meeting_org_limit_exceeded` *(tightened)* | 403 + `code == ORGANIZATION_MEETING_LIMIT_EXCEEDED` | today emits `code == "FORBIDDEN"` |
| `test_create_meeting_org_inactive_returns_organization_inactive` | 403 + `code == ORGANIZATION_INACTIVE` | today emits `FORBIDDEN` + limit message |
| `test_create_meeting_org_not_provisioned_returns_internal` | 500 + `code == INTERNAL_ERROR` + body contains no org identifier | today returns 403 |
| `test_meeting_refusal_causes_are_pairwise_distinct` | the three `(status, code)` pairs are pairwise distinct | today all three are `(403, FORBIDDEN)` |
| `test_org_selector_cannot_be_supplied_by_client` | S-2 (a) + (b) | new coverage |

Note on org-missing fixture honesty (@test 5): `users.org_id` FKs to `organizations`, so an absent org
means an absent user. This does **not** produce an FK violation, because the INSERT's `SELECT ... FROM
org_row o, current_count c` yields **zero candidate rows** when the org is missing — the insert never
executes, `created_by_user_id` is never written. The outer diagnostic SELECT still returns exactly one
row carrying `org_not_provisioned`. The test therefore asserts the *specific* expected code, never
"is an error". Order of checks: the CTE decides everything in one statement; the `CASE` precedence in
Decision 1 is what orders the *reporting*.

Metric / error-taxonomy coverage:

- `tests/meeting_creation_metrics_integration.rs:32-39` — add the three values to `ALL_ERROR_TYPES` so
  each is driven and gets sibling `assert_delta(0)` adjacency (@observability 7, @test 6).
- `src/observability/metrics.rs:1206-1233` — extend the in-src cluster enumeration identically.
- `src/errors.rs` unit tests — `status_code()`, `error_type_label()` and `into_response()` body/code
  for both new variants.
- `tests/db_metrics_integration.rs:~126, ~411, ~432` — update call sites for the new return type; the
  `status="error"` assertion on the duplicate-code path stays as-is (Decision 5).
- Repo-level `#[sqlx::test]` cases asserting `MeetingRefusal` variants directly — **additive**, not a
  substitute for the HTTP tests, since R-6 is worded "from the response alone" (@test 7).

### Gate 1 revisions — findings accepted from reviewers

All accepted; nothing deferred. Grouped by what they change.

**Wire contract (accepted from @operations P-1, @semantic-guard A — identical finding, independently
reached).** `(500, INTERNAL_ERROR)` was not a distinct tuple on this endpoint. `POST /api/v1/meetings`
already emits it from RNG failure (`handlers/meetings.rs:199-202`, `:207-210` → `:899`, `:935`) and
collision-retry exhaustion (`:254`, `:270`). Third unit variant added: `GcError::OrgNotProvisioned` →
`(500, "ORGANIZATION_NOT_PROVISIONED", "An internal error occurred")`. Body message stays fixed and
generic, so @security's coarse-body constraint survives; an `error.code` is a bounded constant, not an
identifier. Costs zero ADR-0011 label budget — this is `error.code`, not `error_type`.

**Panic surface (accepted from @code-reviewer CQ-1, blocking, ADR-0002).** The always-one-row design
means the outer SELECT returns a row with every `i.*` column NULL on all three refusal paths.
`map_row_to_meeting` (`repositories/meetings.rs:242-266`) decodes with `row.get`, which **panics** on
decode failure. Reachability is currently blocked only by `CASE`-arm discipline — the same "unreachable
by construction" argument Decision 2's `_` arm exists to reject, so the posture must be consistent.
`map_row_to_meeting` converts to `try_get` throughout and returns `Result<MeetingRow, GcError>`; the
four call sites (`repositories/meetings.rs:135`, `handlers/meetings.rs:782`, `:795`, `:845`) become
`Ok(map_row_to_meeting(row)?)`.

**Error-context preservation (accepted from @semantic-guard B).** The `_` arm returns
`GcError::Database(format!("create_meeting returned an unrecognised outcome discriminant: {observed}"))`
— the observed discriminant travels *in* the returned error, not only in the `error!`. `errors.rs:120-127`
logs `Database`'s detail server-side and hands the client the fixed generic string, and the discriminant
is our own bounded SQL literal, never user data. The guarantee is reworded to what it actually is:
**the string never reaches a metric label.**

**DB-layer metric semantics (accepted from @semantic-guard C).** All three refusals record
`record_db_query("create_meeting", "success", …)` — a `fetch_one` returning an `org_limit` row is a
query that worked. Only statement failure records `"error"`. `sqlx::Error::RowNotFound` stays in the
`Err(e)` arm (→ `"error"` + `db_error` + loud 500) and is **never** special-cased into a business
cause: if it fires, the "exactly one row" invariant is broken and must be reported as such.

**Recording-exit completeness (accepted from @semantic-guard D).** Enumerated so the enum rewrite
cannot silently drop one: each of the three `Refused(_)` arms records
`record_meeting_creation("error", Some(refusal.metric_label()), duration)` and **returns** (not
`continue`); `MeetingCodeTaken` keeps *both* the in-loop exhaustion exit at `:251-257` and the
post-loop `meeting_row.ok_or_else` exit at `:267-271` — the latter is the one that disappears without
a compile error when `Option<_>` becomes enum-driven.

**Overstated security claim (accepted from @security S-6, doc half).** `repositories/meetings.rs:8`
claims the CTE "prevents TOCTOU race on concurrent meeting limit." True against an application-level
check-then-insert across two round trips; **not** true against two concurrent executions of this
statement — under READ COMMITTED, `current_count` takes no row lock and no constraint enforces the cap,
so two sessions can both count 9 against a cap of 10 and both insert. My change neither causes nor
worsens this (the predicate is semantically identical), but the plan asserted the claim "stays true",
so I am correcting the comment: the CTE eliminates the application-level check-then-insert race; the
cap is **best-effort under concurrent statements**. Closing the race needs `FOR UPDATE` / `SERIALIZABLE`
/ advisory lock — each serialises creation per-org with throughput and deadlock consequences — so it is
a **spin-out to `docs/TODO.md`**, owner @database co-signed by @security, accepted in advance by both.
Not attempted in this diff. @database has accepted ownership and notes this makes their verdict
RESOLVED-DEFERRED rather than CLEAR, per protocol.

Doc comment takes @database's exact wording rather than my vaguer "best-effort", which understates what
*is* guaranteed and overstates what isn't: single-statement execution removes the application-level
check-then-insert window (no separate `SELECT` round trip during which the count can change); it does
**not** serialise concurrent executions — under READ COMMITTED `current_count` takes no locks, so N
concurrent creates against a cap of M can overshoot to M+N-1; the cap is advisory, not enforced by a
constraint; see `docs/TODO.md`. The TODO entry must scope the fix as **the pattern, not this call site**:
`participants.rs:38` and `:84` both name this function as the model for the future join-capacity CTE, so
that CTE would inherit the same race for *participant* capacity, where an overshoot matters more than a
soft meeting-count limit.

**Cap-test cluster comment (@database, raised by @test — the other half of the S-6 doc fix).** This PR
ships a cluster of green cap assertions (`meeting_create_tests.rs:432` plus the new two-org and
`'active'` cases) that **all drive sequentially**: they exercise the cap *value*, never its enforcement
under concurrency. Sequential-ness is invisible unless you already know to look, so the PR would read as
"the cap is well covered" exactly while we defer the finding that it is not enforced. One comment on the
cluster (one, not three scattered) naming the mechanism, pointing at `docs/TODO.md`, and closing with
*"A green cap suite is not evidence the cap holds under load."* That last sentence is the load-bearing
one — it stops a future reader inferring coverage that is not there.

**Round-trip test (@database, on the `CASE`-literal realignment).** Aligning the SQL literals to the
label values has a subtle cost: previously a future edit that bypassed `metric_label()` and passed the
raw discriminant into the label would emit `cap_exhausted`, mismatching the catalog and therefore being
**detectable by observation**; now the identical strings make that same bypass emit exactly the correct
value, undetectably. The guardrail still works but stops being self-evidencing, and the string set now
lives in two places across a boundary with no derivation available. Proportionate answer, added: a
three-line **round-trip unit test** — for each variant, the discriminant the repository parses maps to
that variant, and `metric_label()` maps back to the same string. No fixture, no database. It pins the
coincidence as intentional, so a later edit to either side reds a test instead of silently un-aligning.

**Justification correction for the 403/500 split (accepted from @database).** `grep` finds **zero**
sites setting `organizations.is_active = false` and zero deleting an org, anywhere in the repo — the
only `DELETE FROM organizations` hit is an unimplemented snippet in `docs/debates/2025-01-testing-strategy.md:775`.
So neither non-cap cause is reachable from any application flow, and "routine business event vs fault"
was a false premise. The surviving justification is **modeled vs unmodeled state** (see Decision 4).

**Doc-comment debt in the file I am editing (accepted from @code-reviewer CQ-3/CQ-4).**
`repositories/meetings.rs:33` ("Returns `Some(MeetingRow)` on success, `None` if org limit exceeded")
is the origin sentence of R-6; rewritten to enumerate the full refusal domain and state the precedence.
No rename of `create_meeting_with_limit_check` — @dry's `participants.rs:38`/`:84` pointer argument
stands and rename churn would cost more than it buys; the doc comment carries what the name no longer
does.

**Small, folded in:** `#[expect(dead_code, reason = "Conflict is the only variant with no constructor; every other variant is live")]`
so the attribute self-retires the day someone constructs a `Conflict` (@code-reviewer CQ-2 — they
counted construction sites for all twelve variants; `Conflict` is the only zero). `Debug` derived on
`CreateMeetingOutcome`, not just `MeetingRefusal` (CQ-5). SQL `CASE` literals aligned to the metric
label values (`'org_limit'`, `'org_inactive'`, `'org_not_provisioned'`) so grepping either side finds
both (@observability nit). The `LEAST($6, NULL)` reasoning goes in a **code comment** on the
`FROM org_row o, current_count c` line, not only in this plan — the plan gets archived, the comment is
what stops the regression (@security).

### Test plan additions accepted at Gate 1

- **T-1 (@test) — the `LEAST` cap branch has never executed in a test.** Verified: `meeting_create_tests.rs`
  requests 25 / 50 / 1 / default against a schema default of 100, so `LEAST` has always returned the
  request. A hazard this plan is proud of avoiding, with nothing pinning it, is a hazard that gets
  reintroduced. New `#[sqlx::test]`: org with `max_participants_per_meeting = 10`, request
  `maxParticipants = 500`, assert returned **and persisted** value is 10. Green before and after — its
  job is to make the restructure's most dangerous silent failure mode observable at all.
- **T-2(b) (@test) — the cause→label chain needs its middle link.** `ALL_ERROR_TYPES` is a wrapper-level
  mirror (its own doc comment says so) and proves only that the wrapper emits what it is handed. Adding
  a unit test on `MeetingRefusal::metric_label()` covering all three variants, so the chain is
  (a) cause→variant [repo-level `#[sqlx::test]`, **all three** causes], (b) variant→label [new unit
  test], (c) variant→status/code [HTTP tests]. The handler passes `refusal.metric_label()` and never a
  retyped literal — if any call site retypes, link (b) stops binding.
- **T-3 (@test) — tabulate expected-red *and* expected-green-today per test** (per sub-assertion for
  the selector test, whose part (a) passes today), and paste output for both classes. Otherwise a green
  measurement reads as a broken one, or a sub-assertion quietly goes unmeasured.
- **T-5 / CQ-6 — Decision 5 needs its own coverage.** `db_metrics_integration.rs:411` was green before
  the classifier existed and stays green if the classifier is wrong; it cannot see the classification.
  Two additions: assert the returned value is `CreateMeetingOutcome::MeetingCodeTaken` at that call
  site (converts the hardcoded `"meetings_org_code_unique"`↔migration coupling into a tested
  invariant), and make the classifier a **pure function over (SQLSTATE, constraint name)** with a unit
  test over both inputs — including a 23505 on some *other* constraint falling through to
  `GcError::Database` rather than being retried.
- **P-4(a) (@operations) — pin "exactly one row".** The repo-level cases cover all three refusals
  **plus** success, so the `fetch_one` invariant is tested rather than asserted in a comment.
- **T-6 (@test) — `current_count`'s `org_id = $1` predicate is unguarded.** Verified: every test in
  `meeting_create_tests.rs` uses exactly one org, and `#[sqlx::test]` isolates databases, so **no test
  in the repo has two orgs where one holds meetings.** Drop `WHERE org_id = $1` from `current_count`
  and nothing reds — while the regression is cross-tenant count bleed (org A refused at its cap
  because org B has meetings). I am moving that predicate into a restructured CTE, so this is the
  cheapest moment to pin it. Test: org A cap 2 with zero meetings, org B with 2+ meetings, create in
  A → expect **201**.
- **T-7 (@test) — the `'active'` arm of `status IN ('scheduled','active')` executes but is unasserted.**
  `create_test_meeting_directly` hard-codes `'scheduled'`; `db_metrics_integration.rs:103` does insert
  an `'active'` meeting and `:126` does run the CTE, so the arm evaluates — but against a cap of 10
  with a count of 1, so dropping `'active'` takes the count 1 → 0 and that test still passes. Fix:
  parameterize `create_test_meeting_directly` with a status and add an at-cap case built from
  `'active'` rows. This matters most for task #3, whose whole premise (R-7) is that live meetings
  accumulate toward the cap — a regressed `'active'` arm would make R-7's fix look successful while
  the cap quietly stopped counting the meetings that matter most.
- **T-8 (@test) — assert "refusal ⇒ no row", not only the status code.** The rewrite splits one
  condition across two places (the INSERT's `WHERE` and the outer `CASE`). Pairwise distinctness
  catches most drift, but not the nasty direction: INSERT succeeds while the `CASE` reports a refusal,
  so the caller gets a 403 *and* a meeting exists. One `SELECT COUNT(*) FROM meetings WHERE org_id = $1`
  assertion per refusal case.
- **T-1 refinement (@test)** — assert the **stored** value, not only the response body; a regression
  could echo the requested value while storing something else, and the row is what task #3's env-test
  depends on. Read-back pattern already exists at `meeting_create_tests.rs:508-523`.
- **O-7 (@observability, correcting their own point 7) — `ALL_ERROR_TYPES` growth does not pin the new
  values.** That test hands the wrapper a string literal from the array and asserts the wrapper records
  it; it never constructs a `MeetingRefusal` and never calls `metric_label()`. If
  `OrgNotProvisioned::metric_label()` returned `"org_absent"` tomorrow it would still pass, while the
  catalog, the dashboard legend and the alert matcher all pointed at a value nothing emits — and a
  selector matching nothing fails **silently**. Both mitigations land:
  - *Option 1* = @test's T-2(b), already accepted: direct per-variant assertion on `metric_label()`.
  - *Option 2*, the end-to-end form — **taken, but not via `MetricAssertion`.** `MetricAssertion::snapshot()`
    binds a **thread-local** recorder (`set_default_local_recorder`, `common/src/observability/testing.rs:439-449`,
    and `MetricSnapshot` is `!Send`), whereas the handler runs on a spawned Axum server task on a
    different thread — a snapshot taken in the test body would capture nothing and the assertion would
    be vacuous, which is the same defect one layer down. Instead the handler tests scrape the test
    server's own `/metrics` endpoint (`routes/mod.rs:131`) after the request and assert the
    `gc_meeting_creation_failures_total{error_type="…"}` series is present and non-zero. That is
    strictly stronger for the stated purpose: it exercises the real recorder, the real handler thread
    and the **real exposition format the alert matcher reads**, and it catches a mis-wired `match` arm
    sending the right cause to the wrong label. Presence-and-non-zero rather than exact delta, because
    the Prometheus recorder is process-global across the binary while `#[sqlx::test]` databases are not.
  - `ALL_ERROR_TYPES` growth is **retained** — the sibling `assert_delta(0)` adjacency is still a
    label-swap catcher within the wrapper. It just does not stand in for pinning the vocabulary.
  Recorded for @observability's `docs/TODO.md` §Observability Debt entry: the vocabulary now lives in
  four places (enum, this test, catalog, alert matcher) with none of the latter three mechanically tied
  to the first. This change widens that by three values; the assertions above are the local mitigation
  until a drift guard exists.
- **T-9 (@test) — the uniqueness test must not be able to narrow silently.** The `errors.rs` test
  asserting the envelope `code` is unique across every `GcError` variant is the anti-re-collapse guard
  for this whole change — so if it enumerates variants by hand (house style at `errors.rs:291-330`), a
  fourteenth variant leaves it passing while no longer covering what it claims. That is R-6's own defect
  shape: a check whose scope stops matching reality. Making the enumeration **compile-forced** — a
  helper with an exhaustive `match` over `&GcError` beside the list, and the test asserting the list
  covers every arm — so a new variant is a compile error plus a red assert until it is added. An
  anti-re-collapse guard that quietly narrows is worse than none, because it reads as covered.
- **CQ-7 (@code-reviewer, flagged for review; committing now anyway).** The cause taxonomy will have
  two homes — `MeetingRefusal` (repository layer, HTTP-ignorant) and the three new `GcError` variants —
  so: (1) the mapping lives in exactly one place, `impl From<MeetingRefusal> for GcError` with an
  exhaustive match, not an inline `match` in the handler, so a fourth cause is a compile error at one
  conversion site; (2) `GcError::error_type_label()` returns the **same** strings as
  `MeetingRefusal::metric_label()` (`org_limit`, `org_inactive`, `org_not_provisioned`), so correlating
  `gc_meeting_creation_failures_total` against the HTTP error metrics stays mechanical for whoever is
  on call.
- **How the unreachable states are manufactured (@database's third finding).** No application flow
  produces either non-cap state, so the tests build them directly. Repo level: random `Uuid` for
  missing; `INSERT ... is_active = false` for inactive. HTTP level: `TestCreateMeetingServer` mints its
  own tokens with a test keypair against a mocked JWKS (`meeting_create_tests.rs:77-88`), so it can sign
  a `UserClaims` for an `org_id` that was never inserted — no AC involvement, no create-then-delete
  dance. Stating it because "assert all three pairs in one test" otherwise assumes a reachability that
  does not exist.

### Widened sibling sweep (@dry-reviewer's ask — record it as swept, not sampled)

My original survey enumerated 6 of the 18 zero-row-result sites in GC. @dry-reviewer swept the rest and
found **no additional class members**; folding their per-family reasoning in so the record is checkable:

- `meeting_assignments.rs:89 get_healthy_assignment` — `None` covers no-row / ended / MC-unhealthy /
  heartbeat-stale / MC-missing-via-JOIN, and *looks* like the mechanism. It is not: the caller
  (`services/mc_assignment.rs:138`) consumes it as "no reusable healthy assignment," and all five
  conditions genuinely mean that one thing. R-6's defect is *unrelated* causes sharing an outcome;
  this is *related* causes correctly sharing one.
- `meeting_assignments.rs:204 atomic_assign` — already disambiguates zero-row via a follow-up
  `get_current_assignment`, with a distinct `error!` and a distinct `ServiceUnavailable` for the
  residual. Arguably the in-repo precedent for what this task builds.
- `rows_affected()` sites (`end_assignment`, `end_stale_assignments`, `cleanup_old_assignments`,
  `update_heartbeat`, `mark_stale_*`, `update_load_report`) — bulk-maintenance counts, not refusal
  decisions; no caller invents a cause from them.
- `get_controller:316` / `get_handler:344` and the three handler helpers — single key predicate each.

**Not touching `services/mc_assignment.rs`** (confirmed for @dry). Their new observation there —
`McRejectionReason` mapped through two parallel `match`es, one exhaustive for the label and one ending
in `_ =>` for the message, so a new variant silently inherits the generic message — is the *fix pattern*
applied inconsistently, not the R-6 mechanism, so it is a legitimate TODO rather than a same-owner
sibling. Theirs to track at verdict.

### Runbook / doc surfaces going stale (owner-gated)

**Final spec (@operations, 2026-08-14) — supersedes the four-item list below; my Gate 3 hunk-ACK is
against exactly this.** Two files: `gc-incident-response.md` and `gc-deployment.md`.
*Anchor constraint, non-negotiable:* `### Scenario 8: Meeting Creation Limit Exhaustion` stays
byte-identical — `GCMeetingCreationFailureRate` and @observability's new rule both bind to
`#scenario-8-meeting-creation-limit-exhaustion` and the guard enforces anchor existence
(`docs/observability/alert-conventions.md:388`). Open the section with a one-line note that the title is
retained *because* alert anchors point at it, so the next reader doesn't read it as drift.
1. Symptoms `:882`/`:883` — `error_type="forbidden"` no longer means limit exhaustion (cap is
   `org_limit`; `forbidden` narrows to role denial), and the never-emitted grep string is replaced with
   Decision 6's three static messages **quoted verbatim**. @operations will diff these against the Rust
   at Gate 3 — one wrong grep must not be traded for another.
2. Common Root Causes — two new entries carrying `(status, code, error_type, log message)`, plus a
   Diagnosis step that *extracts* `org_id`; existing steps 4-6 need an `<ORG_ID>` the runbook gives no
   way to obtain, and Decision 6's log field is the first time it is obtainable.
3. `org_not_provisioned` causes **ordered by likelihood, not drama**: (i) **GC and AC pointed at
   different databases — check first**; `ac-service-secrets` and `gc-service-secrets` are two
   independent Secrets each carrying its own `DATABASE_URL`
   (`infra/services/ac-service/statefulset.yaml:42-46`, `infra/services/gc-service/deployment.yaml:107-111`,
   `gc-service/secret.yaml:14`) with nothing enforcing agreement — the same value in two places, which
   our own conventions say will drift; (ii) a DB reset/restore between token issuance and use — directly
   relevant, since "rerun" is the documented recovery for several lanes; (iii) out-of-band SQL, real but
   least likely as no tooling exists; (iv) cross-environment token reuse, lowest confidence and a
   **security** incident if ever confirmed — route to @security, do not triage here.
4. Scenario 5 cross-reference (the item @operations cares most about): its 5xx branch today says
   *"If recent deployment correlates with error spike: `kubectl rollout undo deployment/gc-service`"*.
   `500 + ORGANIZATION_NOT_PROVISIONED` is data-caused; a rollback cannot fix it and burns a deploy
   cycle mid-incident, and the correlation will look maximally convincing precisely because the label
   first appears when this change deploys. Say plainly: do not `rollout undo`; go to Scenario 8.
5. **Drop the "legitimate offboarding" framing** — retracted by @observability and confirmed by
   @database's sweep: there is no legitimate org-state change in this system. The duration window
   discriminates between **test-manufactured blips** (seconds-to-minutes; layer-7 exercises these paths
   and task #3 adds per-run org provisioning) and a **sustained real condition**.
6. State that **permanent absence is healthy** for `org_inactive` and `org_not_provisioned` — no data on
   those series is not a broken exporter or a bad scrape config. Counterintuitive enough to need a
   sentence.
7. New-label-value note per the Scenario-5 precedent (`~:575`) for all three of `org_limit`,
   `org_inactive`, `org_not_provisioned` — a new series with no historical baseline is not an incident
   on its own, and all three are new on a metric documented as having 6 values.
8. `gc-deployment.md` `:1232` and especially `:1243` — *"No limit exhaustion patterns (no spike in
   `error_type="forbidden"`)"* is a 4-hour post-deploy checkbox that will tick **green during a real cap
   exhaustion** once `org_limit` exists. A check that passes because its selector matches nothing is
   worse than no check, because it is recorded as having run. Repoint both at `org_limit`, and add
   `org_inactive|org_not_provisioned` to the checklist — their appearance right after a deploy is the
   highest-signal moment for cause (i).

**Final Scenario 8 prose (@operations, 2026-08-14) — replaces item 3 above; items 1, 2, 4, 6, 7, 8 stand
and item 5 is folded in.** Alert header becomes `**Alert**: GCMeetingCreationOrgStateInvalid`
(@observability renamed it; the old `…ProvisioningFault` named a cause the alert structurally cannot
observe), with `GCMeetingCreationFailureRate` still listed — both bind to this anchor.

Cause ordering for `org_not_provisioned`, most likely **first**; both @database and @observability
withdrew "out-of-band SQL" as the lead:
1. **The database was reset or restored.** `infra/docker/postgres/init.sql:116` seeds the `devtest` org
   with column list `(subdomain, display_name, plan_tier)` — **no `org_id`** — so it takes
   `DEFAULT gen_random_uuid()`. Every volume wipe mints a *new* `org_id` while previously-issued tokens
   still carry the old one: validly signed, referencing a row that no longer exists. Zero manual SQL,
   triggered by the most routine dev action there is. (`ON CONFLICT (subdomain) DO NOTHING` means a
   restart against a surviving volume is fine — this is specifically a reset.)
2. **GC and AC pointed at different databases** — two independently-authored connection strings for one
   logical database (`gc-service/secret.yaml:14`, `ac-service/statefulset.yaml:42-46`). @database is
   filing this as its own config-SSoT defect.
3. **Restore-from-backup timing** — a restore landing between token issuance and use.
4. **Out-of-band SQL mutation** — real, but no tooling exists for it, so it is the last hypothesis.

Cross-environment token reuse is dropped entirely: divergent JWKS fails signature validation before this
branch is reachable, so it would only compete for attention.

Triage paragraph (@operations' prose, verbatim intent — three regimes and a discriminator): the series is
**permanently absent in normal operation**, so no data means healthy; the **bounded regime** decays within
the 3600s TTL because AC cannot mint replacements (`organizations.rs:43` filters `is_active`,
`org_extraction.rs:112-121` fails closed, no refresh flow exists) and blips of seconds-to-minutes are
usually test-manufactured; the **unbounded regime** — different databases, or GC's database reset — fires
indefinitely because AC's row is healthy and AC keeps minting valid tokens forever, **behaving correctly
throughout**. Discriminator: still firing beyond ~1h15m (1h TTL + 15m rate window) ⇒ unbounded regime ⇒
check both `DATABASE_URL` targets and whether the database was reset, *first*; only if both are clean
suspect AC's fail-closed filter, and page @auth-controller at that point, not before. Also state that
`for: 15m` buys debounce and a settle window but does **not** filter single events, since
`increase(...[1h]) > 0` stays true for an hour after one occurrence — an oncall who reads a fired alert
as implying multiple events will size the incident wrong.

**Item 9 (@operations, 2026-08-14) — the contract is nine items, not eight.** One sentence in Scenario 8,
placed beside item 6's "permanent absence is healthy" line, because the two only work together:

> This rule has no automated exerciser: it is expected never to fire in production, it is not reliably
> exercised in development, and there is no alert-rule unit-test harness in this repository. **The fact
> that it has never fired is not evidence that it works.** If it fires, treat the signal as real and
> follow the cause table; do not assume a misconfigured rule.

Three verified facts behind it: (a) **no dev exerciser** — @observability retracted "dev volume wipes
exercise this" after @database's push-back, since the producer needs a token minted *before* a wipe and
used *after* it, and both layer-7 and the automated suites re-login post-reset; the realistic producer is
a human holding a browser session across a `docker compose down -v`. Surrounding prose must **not** imply
routine exercise — that would be an unbacked coverage claim, the same defect as item 8's
`gc-deployment.md` checkbox. (b) Expected permanently absent in production (item 6). (c) **No rule-test
harness** — zero `promtool` references under `.github/workflows/`, `scripts/` or `infra/`, no rule-test
fixtures; `scripts/guards/simple/validate-alert-rules.sh` shims to `dt-guard`'s static policy check
(`crates/dt-guard/src/alert_rules.rs`), which validates `runbook_url` / `severity` / `for:` presence —
that a rule is *well-formed*, never that it *fires*. Proof in this loop: the original `for: 2h` was
structurally unfirable and nothing in the pipeline would have caught it; it was caught by reading it once.
Same class as the story's own Deferred entry on the ten `scripts/**/*.test.sh` files with no invocation
site. The tooling fix (promtool rule unit tests across `gc-alerts.yaml` and `mc-alerts.yaml`) is **not in
this diff** — @operations files it under `docs/TODO.md` §Observability Debt at verdict time as a
pre-existing gap, explicitly not a deferral against this work.

**Folded into item 1**: `error_type="forbidden"` does not disappear, it **changes meaning** — it now
denotes role denial only. State that explicitly rather than only repointing selectors: a label that
silently changes meaning is worse than one that vanishes, because nothing breaks and an oncall carrying
the old association keeps producing confident wrong reads. Post-change mapping is `org_limit` /
`org_inactive` / `org_not_provisioned` / `forbidden`; the "Meeting Creation Failures by Type" panel in
`gc-overview.json` shows all four with no edit needed (it groups rather than enumerates), so it is safe
to reference by name in the diagnosis steps.

Superseded: `GCMeetingCreationProvisioningFault` was **misnamed**
under the corrected model — it does not detect a provisioning fault (those never reach GC), it detects a
live token referencing an org row GC cannot see. Whatever name ships, Scenario 8's `**Alert**:` header
must name it; a runbook naming a nonexistent alert is item 1's defect again.

<details><summary>Superseded four-item list (kept for the record)</summary>

- `docs/runbooks/gc-incident-response.md` — @operations granted a conditional hunk-ACK on four items:
  (1) Scenario 8 Symptoms drops `error_type="forbidden"` == limit exhaustion and replaces the
  never-emitted grep string with the three real static log messages, quoted verbatim; (2) Common Root
  Causes gains org-not-provisioned and org-inactive entries with their
  `(status, code, error_type, log message)` tuples and a Diagnosis step using the `org_id` log field
  (existing steps 4-6 need an `<ORG_ID>` they have no way to obtain today); (3) a Scenario 5
  cross-reference — `500 + ORGANIZATION_NOT_PROVISIONED` on `POST /api/v1/meetings` is a data-plane
  invariant violation, **do not `rollout undo`**, go to Scenario 8 (without this, the runbook's 5xx
  branch tells an oncall to roll back a deploy that cannot be the cause, and the correlation will be
  spurious *and* compelling because the label first appears when this change deploys); (4) the
  new-label-value note following the Scenario-5 precedent at `~:575`.
- `docs/runbooks/gc-deployment.md:1232` and `:1243` — @observability found these post-deploy checks key
  on `error_type="forbidden"` for limit exhaustion. After this change `:1243` is *actively false*: a
  deploy that exhausts an org's cap leaves the checkbox green. The file is in nobody's table.
  **Asked @operations to take it**; if they decline it comes into mine rather than nobody's.
- `docs/user-stories/2026-08-11-story-runner-hardening.md` — task #3's implementer reads that file, not
  this one. @operations granted the hunk-ACK on condition the line states the **mechanism**, not just
  the conclusion, or it reads as an assertion the next implementer can talk themselves out of. Four
  parts: (a) provisioning failure surfaces at AC token acquisition, not GC meeting creation; (b) because
  AC resolves the org by subdomain filtered `is_active = true` and fails closed; (c) therefore Phase 1
  must verify provisioning directly — org exists, `is_active`, token obtainable; (d) R-6's contribution
  is that a 403 cap signal can now be trusted as a real cap. **Same edit also corrects R-7's premise
  sentence** (@database), which claims a provisioning bug "becomes indistinguishable from a full cap" —
  it never could, and leaving it would let the sentence be cited later as evidence this gap was closed.
  Same fact in two places; fixing only one leaves the other citable.
- Pending on @observability: whether they ship a provisioning-fault alert rule and under what name, so
  Scenario 8's `**Alert**:` header and the new root-cause entry name a rule that exists (@operations P-3).

### Answers to the direct questions asked at Gate 1

- **@code-reviewer 4** — the collision arm still fires: the sqlx error text is unchanged by the CTE
  restructure, and Decision 5 replaces prose matching with SQLSTATE anyway.
- **@code-reviewer 5** — `errors.rs:27`'s bare `#[allow(dead_code)] // Variants will be used in Phase 2+`
  becomes `#[expect(dead_code, reason = "…")]` in the same edit, so it cannot mask whether the new
  variants are wired up. No `unwrap`/`expect`/`panic` on any new path (ADR-0002).
- **@database 9** — `Refused(_)` and `MeetingCodeTaken` both terminate the retry loop correctly:
  `Refused` returns early exactly as `Ok(None)` does today, so a missing org does not burn all three
  `MAX_CODE_COLLISION_RETRIES` attempts.
- **@observability 9 / @operations O-6** — `docs/runbooks/gc-incident-response.md:883` documents a log
  line (`meeting creation forbidden: org concurrent meeting limit reached`) that exists nowhere in
  `crates/`. Decision 6 creates real log lines for the first time. @operations has asked for the
  Scenario 8 update in this PR (O-6), so it is in the table as Minor-judgment with Owner `operations`;
  I will make the edit and need their hunk-ACK at Gate 1 and Gate 3.
- **@dry 2** — no third `Organization` model is created. Nothing is added to `crates/common/`. GC does
  not call AC's repository. The org columns are read inline by the CTE; no struct at all.

---

## Pre-Work

None — working tree clean at `0533d4c`.

---

## Implementation Summary

`create_meeting_with_limit_check` no longer returns `Option<MeetingRow>`. The CTE is restructured to
return **exactly one row always**, carrying the cause in a `CASE` column, and the repository returns
`CreateMeetingOutcome { Created(Box<MeetingRow>), Refused(MeetingRefusal), MeetingCodeTaken }`. Three
new `GcError` unit variants give each cause a distinct `(status, error.code)` on the wire and a
distinct bounded `error_type` metric label. The refusal path, which previously emitted no log at all,
now emits one static message per cause with `org_id`/`user_id` as structured fields.

| Cause | HTTP | `error.code` | `error_type` | log |
|---|---|---|---|---|
| cap exhausted | 403 | `ORGANIZATION_MEETING_LIMIT_EXCEEDED` | `org_limit` | warn |
| org inactive | 403 | `ORGANIZATION_INACTIVE` | `org_inactive` | warn |
| org not provisioned | 500 | `ORGANIZATION_NOT_PROVISIONED` | `org_not_provisioned` | error |
| role denial (unchanged) | 403 | `FORBIDDEN` | `forbidden` | warn |

Secondary: meeting-code collisions are classified by SQLSTATE `23505` + constraint
`meetings_org_code_unique` instead of by matching the English text of the driver's error message.

---

## Files Modified

Final, at freeze (post-Gate-3 findings):

```
 crates/gc-service/src/errors.rs                    | 256 ++++++++++-
 crates/gc-service/src/handlers/meetings.rs         |  69 ++-
 crates/gc-service/src/models/mod.rs                | 106 ++++-
 crates/gc-service/src/observability/metrics.rs     |  40 +-
 crates/gc-service/src/repositories/meetings.rs     | 445 +++++++++++++++---
 crates/gc-service/src/repositories/mod.rs          |   2 +-
 crates/gc-service/tests/db_metrics_integration.rs  |  17 +-
 crates/gc-service/tests/meeting_create_tests.rs    | 499 ++++++++++++++++++++-
 .../tests/meeting_creation_metrics_integration.rs  |  17 +
 docs/API_CONTRACTS.md                              |  20 +
 docs/TODO.md                                       |  77 ++++
 docs/observability/alerts.md                       |  41 +-
 docs/observability/metrics/gc-service.md           |  36 +-
 docs/runbooks/gc-deployment.md                     |  25 +-
 docs/runbooks/gc-incident-response.md              | 157 ++++++-
 .../global-controller/INDEX.md                     |   2 +-
 .../2026-08-11-story-runner-hardening.md           |   6 +-
 infra/docker/prometheus/rules/gc-alerts.yaml       |  33 +-
 infra/grafana/dashboards/gc-overview.json          |   2 +-
 19 files changed, 1715 insertions(+), 135 deletions(-)
```
Plus untracked `docs/devloop-outputs/2026-08-14-gc-meeting-refusal-causes/`.

Two files in the stat are **not mine and were written by their owners** during this loop:
`infra/docker/prometheus/rules/gc-alerts.yaml` and `docs/observability/alerts.md` (@observability),
`docs/runbooks/gc-deployment.md` (@operations). All three have classification rows.

**Production code**
- `crates/gc-service/src/repositories/meetings.rs` — `MeetingRefusal` + `CreateMeetingOutcome`, restructured CTE, `fetch_one`, SQLSTATE classifier, `map_row_to_meeting` → `try_get`, 6 unit tests
- `crates/gc-service/src/repositories/mod.rs` — re-exports
- `crates/gc-service/src/errors.rs` — 3 unit variants, `From<MeetingRefusal>`, variant-scoped `dead_code` allow, 6 tests
- `crates/gc-service/src/handlers/meetings.rs` — exhaustive outcome match, per-cause logs + metric labels, 3 `map_row_to_meeting` call sites
- `crates/gc-service/src/observability/metrics.rs` — `error_type` doc (9 values), de-duplicated cluster test list

**Tests**
- `crates/gc-service/tests/meeting_create_tests.rs` — 6 new tests, 1 tightened, 3 new fixtures, `/metrics` scrape helper
- `crates/gc-service/tests/db_metrics_integration.rs` — collision now asserts `MeetingCodeTaken`
- `crates/gc-service/tests/meeting_creation_metrics_integration.rs` — 3 label values + scope note

**Docs**
- `docs/observability/metrics/gc-service.md` *(Owner: observability)* — value list, cardinality 6→9, absence semantics, budget table
- `docs/runbooks/gc-incident-response.md` *(Owner: operations)* — Scenario 8 nine-item contract, Scenario 5 rollback warning, changelog
- `docs/API_CONTRACTS.md`, `docs/user-stories/2026-08-11-story-runner-hardening.md` *(Owner: operations)*, `docs/specialist-knowledge/global-controller/INDEX.md`, `docs/TODO.md`

---

## Key Changes by File

**`crates/gc-service/src/repositories/meetings.rs`** — the mechanism fix. `MeetingRefusal` (3 variants
+ `metric_label()` + `from_discriminant()`) and `CreateMeetingOutcome` (`Created(Box<MeetingRow>)` /
`Refused` / `MeetingCodeTaken`). CTE restructured to `org_row` + `current_count` + `inserted`, with a
diagnostic outer `SELECT ... FROM (SELECT 1) LEFT JOIN org_row LEFT JOIN inserted` that returns exactly
one row always — hence `fetch_optional` → `fetch_one`. The INSERT's source stays an INNER join, with the
`LEAST($6, NULL)` reasoning as a code comment. `classify_insert_error(sqlstate, constraint)` is a pure
function replacing prose-matching on the driver's error text. `map_row_to_meeting` → `try_get`,
returning `Result` (ADR-0002). Doc comments rewritten: full refusal domain, reporting precedence, and
the corrected concurrency guarantee. 6 unit tests including the discriminant↔label round-trip.

**`crates/gc-service/src/errors.rs`** — `OrgMeetingLimitExceeded` / `OrgInactive` /
`OrgNotProvisioned`, all **unit** variants so no identifier can reach a body that `IntoResponse` echoes.
`impl From<MeetingRefusal> for GcError` is the single mapping site. `#[allow(dead_code)]` narrowed from
the whole enum to the `Conflict` variant. Tests: `one_of_each_variant()` with a compile-forced
exhaustive `match`, `every_variant_has_a_distinct_error_code`, pairwise-distinctness, and per-variant
response shapes.

**`crates/gc-service/src/handlers/meetings.rs`** — `Ok(None)` → 403 replaced by an exhaustive match on
`CreateMeetingOutcome`; per-cause static log messages (`org_id`/`user_id` as fields, `error!` for
not-provisioned); metric label from `refusal.metric_label()`, never a literal. Both collision exits
preserved. Four log messages reworded at Gate 2 (see below). Three `map_row_to_meeting` call sites.

**`crates/gc-service/src/observability/metrics.rs`** — `error_type` documented as 9 bounded values with
the `forbidden` narrowing called out; the in-src cluster test's two duplicate label lists collapsed to
one local `const`.

**`crates/gc-service/tests/meeting_create_tests.rs`** — 6 new tests (org-inactive, org-not-provisioned,
pairwise-distinctness, org-selector, `'active'`-counting, cross-tenant scoping, participant-cap), 1
tightened (cap exhaustion now asserts `code`), 3 new fixtures, a `/metrics` scrape helper (thread-local
`MetricAssertion` cannot see the server thread), and the cap-cluster concurrency caveat.

**`docs/runbooks/gc-incident-response.md`** — Scenario 8 rewritten to serve three causes: cause table
with verbatim log messages, `forbidden`-changed-meaning callout, absence semantics, no-exerciser
warning, ordered `org_not_provisioned` causes, duration discriminator, remediation scoped to `org_limit`
with do-not-run guards, and a Scenario 5 warning against rolling back a data-caused 5xx.

---

## Devloop Verification Steps

### Measured red at `0533d4c` (@test T-3)

Method: apply test-only hunks, `git stash push crates/gc-service/src/`, run, `git stash pop`. Actual
output, not reconstructed.

| Test | Class | Result at `0533d4c` |
|---|---|---|
| `test_create_meeting_org_limit_exceeded` | red | `left: "FORBIDDEN"` / `right: "ORGANIZATION_MEETING_LIMIT_EXCEEDED"` — *Cap exhaustion must be distinguishable from role denial* |
| `test_create_meeting_org_inactive_returns_organization_inactive` | red | `left: "FORBIDDEN"` / `right: "ORGANIZATION_INACTIVE"` — *A deactivated org must not report as a full cap* |
| `test_create_meeting_org_not_provisioned_is_distinct` | red | `left: 403` / `right: 500` — *An org row missing under a valid token is a server-side state fault* |
| `test_meeting_refusal_causes_are_pairwise_distinct` | red | `'cap exhausted' and 'org inactive' both respond (403, FORBIDDEN) — the refusal causes have re-collapsed` |
| `test_active_meetings_count_toward_org_limit` | **mixed** | red on the `code` half (`"FORBIDDEN"` vs `"ORGANIZATION_MEETING_LIMIT_EXCEEDED"`); the `403` half and the `'active'`-counting behaviour pass today |
| `test_org_selector_cannot_be_supplied_by_client` | **mixed** | part (a) `orgId`/`org_id` → 400 **passes today**; part (b) red at `:774` (`"FORBIDDEN"` vs `"ORGANIZATION_MEETING_LIMIT_EXCEEDED"`) |
| `test_org_limit_counts_only_own_org_meetings` | **green today** | regression pin for T-6 — passes before and after; its job is that `WHERE org_id = $1` cannot move silently |
| `test_max_participants_capped_at_org_limit` | **green today** | regression pin for T-1 — the `LEAST` cap branch had never executed in any test |

Baseline totals at `0533d4c`: `15 passed; 6 failed`. Two of the six are the mixed-class tests above,
so four tests are wholly red and two are red only on their new assertions.

### Green after

- `cargo test -p gc-service` — **32 test binaries, 0 failures** (365 in-src unit tests; 21 in
  `meeting_create_tests`, 18 in `db_metrics_integration`).
- `cargo clippy -p gc-service --all-targets` — clean, no warnings.
- `cargo fmt --all` — applied, clean.

### One implementation defect caught by the tests, recorded

`test_create_meeting_org_limit_exceeded` initially failed **after** the implementation landed
(`cap exhaustion must emit error_type="org_limit"`) because I captured the `before` metric sample
*after* issuing the request, so `after == before`. The other five scrape assertions were ordered
correctly. Fixed by moving the sample above the request. Worth recording because it is exactly the
vacuity mode @observability predicted for bare-presence assertions — the before/after form caught my
own ordering bug on the first run.

### Gate 2 — all three attempts

Recorded in full rather than compressed to "passed on attempt 3": two of the three failures were
artifacts of how the pipeline was run, not of the diff, and that distinction is the useful part.

| Attempt | Result | What actually happened |
|---|---|---|
| **1** | L1 OK, L2 OK, **L3 FAIL**, L4 OK (186s), **L5 FAIL**, L6 N/A, **L7 FAIL** | **Real failures, all mine except L7.** L3: three guards — `validate-knowledge-index` (INDEX at 76 lines, max 75), `validate-cross-boundary-scope` (4 violations: 2 inbound-drift rows missing, 2 NOT-TOUCHED rows read as unfulfilled plan entries), `no-secrets-in-logs` (4 pre-existing FPs surfaced because the file entered the diff). L5: six clippy errors in my diff (3 × `needless_question_mark`, 2 × `panic`, 1 × `indexing_slicing`). L7: **not mine** — browser E2E failed on gitignored `buf generate` output (`signaling_pb.js`), runbook F5 reproduced inside the pipeline, Lead-handled. |
| **2** | L1 OK (975s), L2 OK, **L3 FAIL**, L4 OK (954s), L5 **OK**, L6 N/A, **L7 PRECONDITION_FAILURE** | **No real failure in the diff.** L3 reported `STATUS=FAIL REASON=guard-timeout-validate-kustomize` — a *timeout*, not a violation, on a layer that took 713s against 15s on a quiet machine. The pipeline ran concurrently with my `cargo test`/`clippy`; L1 went 10s → 975s and L4 186s → 954s. Every guard that returned a verdict passed, and L5 — the only layer that had genuinely failed on this code — came back OK. L7 was `PRECONDITION_FAILURE REASON=cluster-setup-failed`: operator lane, does not consume an attempt, Lead-handled. The codegen fix from attempt 1 worked — the browser-lane failure was gone and L7 failed earlier, at cluster bring-up. |
| **3** | **exit 0** — L1 OK (1s, cached), L2 OK (1s), L3 OK (13s), L4 OK (164s), L5 OK (0s), L6 N/A (1s), L7 OK (438s) | Quiet machine. Both Phase-2 suites ran and passed: `env-tests-passed` **and** `browser-e2e-passed` (8 passed). |

### What the committed tree is actually validated against

Stated as a split claim, because "everything green" would be false in a way this devloop has spent
itself cataloguing:

- **Layers 1–6: green on the post-edit tree.** Re-run individually by the Lead after the Gate 3 comment
  fix. Deliberately stronger than the L3+L5 scope @test's observability argument justified — L4 is
  ~164s on a quiet machine, and the cheaper claim was not worth the ambiguity.
- **Layer 7: green on env-tests, FAIL on browser-e2e, against the *pre-edit* source.** Adjudicated as
  R-7, not as a defect in this diff.
- **The two source states differ by one comment** in a `#[cfg(test)]` module — a change no layer 4 or 7
  lane can observe.

**Layer 7 was deliberately not re-run, and that is a decision with a reason rather than an omission.**
`demo` is at 10/10 concurrent meetings with nothing in production setting `actual_end_time`, so the next
browser run starts saturated and fails *earlier* than this one. A re-run would not re-validate anything;
it would produce a fresh failure with a different cause, one that reads as a regression in this diff
while actually being R-7 — task #3's premise demonstrating itself inside our own gate. It also means
attempt 3's `browser-e2e-passed` was the **last clean browser run available on this cluster until task
#3 lands**: a green describing a state no longer reachable.

**Note on the earlier attempt-3 green.** `crates/gc-service/src/models/mod.rs` entered
the diff *after* attempt 3, during Gate 3, for @security's S-7. Three reviewers independently flagged
that the working tree mutated during review and that attempt 3's exit-0 was therefore stale. The Lead
froze the tree rather than commit on it and re-validated against it — see the split claim above. Noted
here because "Gate 2 passed" would otherwise be exactly the kind of claim this devloop spent itself
cataloguing — true when made, and quietly read as covering more than it does.

A later correction, recorded because it is the same defect from the other direction: the Lead labelled
the frozen tree with a **whole-diff** hash for a claim that was only ever about `crates/`. Verified
after @semantic-guard handed over the discrepancy rather than an inference — the source tree had been
settled since 00:06:12Z and genuinely predates the validation run, so L1–L6 green *does* describe the
committed code. What moved three times was `docs/TODO.md`, as @database, @operations and
@code-reviewer wrote their verdict-time entries: reviewers doing exactly what they were asked, under a
freeze that had not carved out the one file they were told to write to. The freeze was under-specified;
nobody violated it.

### Gate 2 attempt 1 — my local verification reported success without looking

I reported "clippy clean" before Gate 2. Layer 5 then failed on six clippy errors in my own diff. The
report was false for **two independent reasons**, either of which alone would have produced it:

1. **Clippy result caching.** I ran `cargo check` first, then `cargo clippy`. The second invocation was
   fresh-cached and emitted **nothing at all** — not even replayed diagnostics. Zero output read as
   zero findings.
2. **`grep -cE "^(warning|error)"` against ANSI-colored output.** Cargo emits colour escapes *before*
   the word, so `^error` never anchors. Even on an uncached run with real errors on screen, my filter
   would have counted zero.

So the check could not have passed information through in either state. This is the loop's own theme
applied to my tooling — **a check reporting success because its scope never reached the thing it
named**, exactly like `ALL_ERROR_TYPES`, the ten unwired `*.test.sh` files, and the recovery-verification
comment watching a series that could not move. I recorded that pattern three times for other people's
surfaces during this devloop and then shipped it in my own verification step.

**Correct invocation, used from now on**: run the layer script (`./scripts/layer5.sh`) rather than a
hand-rolled `cargo clippy` + grep. It is the same command Gate 2 runs, has no caching gap I can create,
and needs no output filtering. Cheap, and it removes the class rather than this instance.

What the cache and the grep hid, all in my diff:
- 3 × `clippy::needless_question_mark` — `Ok(map_row_to_meeting(row)?)` from the CQ-1 conversion
- 2 × `clippy::panic` (ADR-0002 denies `panic!`; `clippy.toml` has no `allow-panic-in-tests`) —
  in my new `errors.rs` uniqueness test and the `error_status_and_code` test helper
- 1 × `clippy::indexing_slicing` — `body["error"]["code"]` in `error_status_and_code`.
  `allow-indexing-slicing-in-tests` reaches `#[test]` bodies only, **not** test *helper* functions —
  which `clippy.toml` documents at its head and I did not read.

### `#[expect(dead_code)]` could not be used — narrower fix taken

CQ-2 called for `#[expect(dead_code, reason = "Conflict…")]` replacing the enum-wide `#[allow]`.
`cargo check` then reported `this lint expectation is unfulfilled`: `main.rs` re-declares these
modules privately, so `dead_code` fires for the **binary** target but not for the **library**, where
`GcError` is `pub` and therefore never dead. An `#[expect]` warns in the lib build. Resolution: a
`#[allow(dead_code)]` scoped **to the `Conflict` variant only**, with the reason recorded in a doc
comment — strictly narrower than the enum-wide blanket it replaces, which was masking all fifteen
variants. This is the case where the attribute earned its keep by failing loudly: the original
`#[allow]` had been silently doing nothing for the lib the whole time.

---

## Code Review Results

### Gate 3 Verdicts (all eight received)

| Reviewer | Verdict | Findings | Fixed | Deferred / Spun-out |
|----------|---------|----------|-------|---------------------|
| Security | RESOLVED-DEFERRED | 2 | 2 (in-diff halves) | 2 spin-outs accepted |
| Test | RESOLVED-FIXED | 12 | 12 | 0 |
| Observability | RESOLVED-FIXED | 6 | 6 | 0 |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 |
| DRY | RESOLVED-DEFERRED | 0 | 0 | 2 ADR-0019 extraction opportunities |
| Operations | RESOLVED-FIXED | 6 | 6 | 0 |
| Semantic Guard | CLEAR (native SAFE) | 0 | — | 0 |
| Database | RESOLVED-DEFERRED | 3 | 2 | 1 spin-out accepted |

**Reading the three RESOLVED-DEFERRED verdicts correctly**: none of them is a defect left in
@implementer's work. Security's two and Database's one are the *same* two spin-outs (S-6's concurrent
cap race, counted by both its raiser and its accepting owner) plus the GSA dead-glob entry — all
pre-existing conditions this diff exposed. DRY's is the protocol's mechanical rule that a non-empty
§Accepted Deferrals forces the verdict, applied to two ADR-0019 extraction opportunities that never
entered the fix-or-defer flow. Zero findings raised against this diff were refused.

**Findings the panel caught that the diff would otherwise have shipped**, in rough order of cost:

1. **Decision 4's cross-task contract was unreachable** (@database, @test, @operations, independently).
   Task #3 would have wired its operator-lane detector to a GC status that a provisioning failure can
   never produce, shipped green, and left the fault landing on the suite lane unattributed.
2. **`(500, INTERNAL_ERROR)` was never a distinct tuple** (@semantic-guard and @operations,
   independently, from opposite directions). The first fix for a three-way collapse had reintroduced
   the same collapse one layer out in error-code space.
3. **A new panic surface** — the always-one-row CTE makes an all-NULL row reachable on every refusal
   path, decoded by panicking `Row::get` (@code-reviewer, ADR-0002).
4. **`join_token_secret` reachable through a derived `Debug`** (@security) — pre-existing, made more
   inviting by this diff, fixed at the root and now test-pinned.
5. **Three silently-green regressions** — the tenant-scoping predicate, the `'active'` cap arm, and
   the never-executed `LEAST` capping branch (@test, mutation-verified against a live database).

---

## Gate 2 / Validation Record — Final Disposition

**FINAL DISPOSITION (2026-08-15T01:34:30Z): `GATE2=PASS`, `LAYER_ALL_EXIT=0`.** All seven layers OK
on the committed tree — L1 OK, L2 OK, L3 OK (14s), L4 OK (177s), L5 OK, L6 N/A, **L7 OK (469s)** with
both Phase-2 suites passing. The browser E2E failure described below was **resolved by resetting the
dev cluster**, not by adjudication.

**This is the strongest available proof that the failure was environment state and not this diff**:
same code, `dev-cluster teardown` + `setup`, browser suite green. The adjudication below was correct
but is no longer load-bearing — it is retained because the reasoning and the measurement matter for
task #3, and because a record that quietly deletes a red it later cleared is exactly the kind of
artifact this loop spent itself cataloguing.

**Why the reset rather than a hand-authored verdict.** The pre-commit Gate-2 hook refused the commit
on `GATE2=FAIL`, correctly. `scripts/lang/_gate2_binding.sh` notes in its own header that a
well-formed PASS verdict can be hand-authored — its threat model is anti-drift, not anti-forgery. The
gate was therefore trivially bypassable, and bypassing it would have been the single worst act
available in a devloop whose entire subject is controls that report success without observing
anything. Resetting the environment was the only honest route to a real PASS, and @test had already
named the state correctly: stuck, needing a reset rather than intervention.

**Environment note for task #3**: the reset means the `demo` org's meeting count and the `devtest`
org's 133 unended meetings are gone. Task #3 starts from a clean cluster, so it must **not** read the
absence of accumulation as evidence that R-7 is fixed.

---

### Pre-reset record (retained deliberately)

The following describes validation *before* the cluster reset. It is deliberately **not** stated as
"all green":

- **Layers 1–6: OK** on the post-edit tree, `crates/`-only diff hash
  `6c68cea77dd4e2b81d5ca46a556ae2f8a2656fa92a2187cbdb894f38983a230d`, verified byte-identical before
  and after the run (`SOURCE_STABLE_DURING_RUN`).
- **Layer 7 env-tests: OK**, against the pre-edit source, which differs from the committed source by
  one comment inside a `#[cfg(test)]` module.
- **Layer 7 browser E2E: FAIL** (`STATUS=FAIL REASON=browser-e2e-failed`), 4 passed / 4 failed, every
  failure `MeetingForbiddenError: Organization meeting limit exceeded`. **Adjudicated as R-7 cap
  exhaustion, not a defect in this diff** — see below. This is recorded as a FAIL, and the
  adjudication is recorded as an adjudication.
- **Layer 7 was deliberately not re-run after the final comment edit.** This is a decision with a
  reason, not an omission: `demo` sits at 10/10 live meetings with nothing in production setting
  `actual_end_time`, so a browser re-run starts saturated and fails *earlier*, producing a fresh
  failure that would read as a regression in this diff while being R-7.

### The browser-E2E adjudication, with its evidence

@test measured the cluster rather than inferring: the `demo` org holds exactly **10 live meetings
against a cap of 10** — seven created by Gate 2 attempt 3's own `browser-e2e-passed` run at 23:48 and
three by this run at 00:46 — with `actual_end_time` **NULL on all ten**. @operations supplied the
discriminator that closes the "but you also rewrote the refusal path" rebuttal: **the Rust env-tests
passed, creating meetings successfully, in the same execution where the browser suite was refused.**
A create path returning `Created` for one suite and `org_limit` for another is org-state-specific by
construction.

**TRIGGER (modelled on the `pnpm audit` precedent in `docs/TODO.md`, which exists because this kind of
reasoning decayed once before)**: this adjudication is **not** precedent for a second. Task #3 must
re-run the browser suite green against a freshly provisioned org before it closes; if it cannot,
escalate rather than adjudicate again.

**Why committing rather than escalating is correct**: task #3 fixes R-7 and *depends on task #2*.
Escalating here would block the only thing that can fix the condition being escalated.

**What this incidentally proves**: GC classified the refusal as cap-exhausted and ground truth
independently confirmed it — R-6 validated end-to-end in a live cluster against a real cap, which is
stronger evidence than anything in the test suite. And it was only diagnosable in one read *because*
R-6 removed a false positive: `ORGANIZATION_MEETING_LIMIT_EXCEEDED` can now be trusted to mean a real
cap. Under the story's original (incorrect) premise, the reader would have gone hunting a provisioning
fault.

---

## Accepted Deferrals

- `docs/TODO.md` §Code Quality — org concurrent-meeting cap advisory, not enforced (spin-out)
- `docs/TODO.md` §Cross-Service Duplication (DRY) — org exists-and-active implemented three times
- `docs/TODO.md` §Cross-Service Duplication (DRY) — McRejectionReason via two parallel matches
- `docs/TODO.md` §Cross-Boundary / guards — ADR-0024 §6.4 schema glob matches zero files
- `docs/TODO.md` §Layer 7 Playwright Lane — browser lane mis-lanes operator preconditions
- `docs/TODO.md` §Guard Timeout vs Violation — timeouts fabricate failures that look real
- `docs/TODO.md` §Guards — guard:ignore inert for 32 of 37 dt-guard modules
- `docs/TODO.md` §Code Quality — 175 bare allow attributes carry no reason
- `docs/TODO.md` §Observability Debt — label-value drift unguarded on every surface
- `docs/TODO.md` §Port Constant Scattering — split DATABASE_URL, no agreement guard

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `0533d4cf4d57439d84332d01a16f8d6e499296be`
2. Review all changes: `git diff 0533d4c..HEAD`
3. Soft reset (preserves changes): `git reset --soft 0533d4c`
4. Hard reset (clean revert): `git reset --hard 0533d4c`

**Ordering constraint.** Task #3 (layer-7 per-run org provisioning) consumes this diff's response
contract — the `(status, error.code)` tuples. Once #3 has landed, rolling GC back to `0533d4c` alone
silently returns layer-7 to attributing organization-state faults to the suite lane. Roll back both, or
neither.

---

## Issues Encountered & Resolutions

### Issue 1: Gate 2 attempt 1 — three real failures
**Problem**: L3 (`validate-knowledge-index` 76/75 lines; `validate-cross-boundary-scope` drift in both
directions; `no-secrets-in-logs` on four pre-existing lines newly in scope), L5 (six clippy errors),
L7 (browser E2E).
**Resolution**: All fixed. The scope-drift half needed judgment rather than a mechanical fix — the
classification table has no vocabulary for deliberate absence, so @database's NOT-TOUCHED rows moved
to prose beneath it, with the durable fix (a structured `NOT-TOUCHED` assertion) recorded in TODO.

### Issue 2: The implementer's local "clippy clean" was false, twice over
**Problem**: Clippy result caching meant a post-`cargo check` run emitted nothing; separately,
`grep -cE "^(warning|error)"` never anchored because cargo emits ANSI escapes before the word. Either
alone produces a confident false green.
**Resolution**: Local verification now uses `./scripts/layer5.sh` — the command the gate runs.
Generalised by @test as *verify with the command the gate runs, never a hand-rolled equivalent*,
because a similar-but-separate check has its own scope and its own bugs, and its bugs all fail open.

### Issue 3: Gate 2 attempt 2 — the Lead manufactured a false red
**Problem**: `STATUS=FAIL REASON=guard-timeout-validate-kustomize` on a layer that takes 13s quiet and
took 713s under load. L1 10s→975s, L4 186s→954s. The Lead launched the pipeline into the middle of the
implementer's cargo jobs; it cost a real Gate 2 attempt on a clean diff.
**Resolution**: Re-ran on a quiet machine. Recorded in TODO as a distinct category — a gate that can
*invent* its own red rather than miss one.

### Issue 4: Layer 7's browser lane failed on a missing build artifact
**Problem**: `packages/sdk-core/src/proto/` is gitignored `buf generate` output that no pipeline step
produces; `layer7.sh` invokes the suite via `pnpm --filter`, bypassing the Nx task graph — documented
failure F5 in `docs/runbooks/client-dev-local.md:694-712`, reproduced inside the pipeline. Reported on
the *implementer* lane having run zero tests.
**Resolution**: Lead ran codegen manually to unblock. **Not fixed** — recorded by @operations in TODO.
A green Gate 2 is not evidence that lane works in a fresh container.

### Issue 5: The reviewer panel converged on a moving tree
**Problem**: The working tree mutated four times during Gate 3; two reviewers read transiently-broken
intermediate states, and `models/mod.rs` entered the diff *after* the green pipeline the Lead was
quoting as current evidence.
**Resolution**: @semantic-guard escalated rather than letting a stale green carry the gate. Freeze
declared, all outstanding findings landed, full re-validation with the diff hash captured before and
after. The Lead's own instance is recorded in §Lessons Learned rather than folded in anonymously.

### Issue 6: The Lead's first use of the hash mechanism was itself mis-scoped
**Problem**: Adopted "quote the result with the hash it was computed against", then quoted a
*whole-diff* hash for a claim only ever about `crates/` — reporting a mismatch that wasn't one.
**Resolution**: Verified what actually moved (`docs/TODO.md` only, at reviewers' verdict time, under a
freeze that hadn't carved out the file they'd been told to write to — under-specified by the Lead, not
violated by them). Per @implementer, the gap is in the mechanism as stated, not only its use: it does
not say which subset the hash covers. Fix is structural — name the scope alongside the hash.

### Issue 7: Layer 7 browser E2E — R-7 cap exhaustion
**Problem**: 4 passed / 4 failed, all `Organization meeting limit exceeded`.
**Resolution**: Adjudicated as R-7, not a defect in this diff, on measured evidence. See §Gate 2 /
Validation Record for the evidence, the TRIGGER clause, and why committing rather than escalating is
correct given task #3 depends on task #2.

---

## Lessons Learned

### One defect, four forms — three are stale artifacts, the fourth is not

R-6's defect is *a signal meaning less, or other, than the reader takes it to mean*. It turned up four
times in this loop at increasing distance from the code, and the fourth is a different failure mode
from the first three:

| Form | Instance | Found by | Mitigation |
|---|---|---|---|
| **Query** | The CTE collapsing three causes into zero rows | the task itself | the fix |
| **Selector** | `gc-deployment.md:1243` checking `error_type="forbidden"` for cap exhaustion — would tick green *during* a real exhaustion | @operations, sweeping selectors | drift guard |
| **Prose** | `gc-overview.json:2796` describing the disambiguation panel as "org limit exhaustion or DB issues" | @observability, reading their own surface after the selector form | drift guard |
| **Selector inside a comment** | `gc-incident-response.md:1090` — the *recovery-verification* step told an operator to watch `error_type="forbidden"` drop to zero | @operations, re-sweeping after the prose form | neither sweep reaches it |
| **Scope** | "Zero dashboard edits needed" | noticed only in hindsight | *a habit, not a guard* |

The first four are stale artifacts: written once, left behind, in principle mechanically findable.
Two things rank them by cost, and they are different things:

- **Prose beats selectors for damage at the moment of triage.** A stale selector fails **silent**; a
  stale description asserts something false with the authority of appearing on the panel built to
  answer that question.
- **Position beats form.** The comment-form instance is the worst of the four not because of its
  grammar but because of *where it sat*: a stale **symptom** under-reports and gets investigated,
  while a stale **verification step** reports success and closes the incident (@operations). It also
  evades both sweeps by construction — it is not in a query, so a selector sweep misses it; it is not
  prose, so a prose sweep misses it.

**The fourth is not a stale artifact at all**, which is why it is the keeper. "Zero dashboard edits
needed" was accurate when said, is still accurate, and was correctly scoped to the *query*. Nothing
went stale and nobody wrote anything false. The qualifier simply stopped being carried by the sentence
as it propagated, and the broader reading is what cleared the file from review. Same shape as the
runbook's `**Alert**:` header naming a rule that did not exist, and as `ALL_ERROR_TYPES` proving
something narrower than its name suggests.

Mitigation differs accordingly: state the qualifier where it cannot be dropped — *"the query needs no
change"*, not *"the dashboard needs no edits"*. That is a habit rather than a guard, which makes it
harder to enforce and worth naming explicitly. Carried to story-close in that form.

### A guard timeout is indistinguishable from a guard violation in the STATUS line

While re-verifying, Layer 3 returned `STATUS=FAIL REASON=guard-timeout-api-version-check` — with
`api-version-check` **passing in 0.029s** when run in isolation seconds later. Cause: I had concurrent
`cargo test` / `cargo clippy` jobs loading the machine. Same layer, quiet machine: `RESULT=OK` in 15s
versus 184s timing out.

Nothing was wrong with the code or the guard. But the failure is *shaped* like a real violation — a
`STATUS=FAIL` with a `REASON` token — and an implementer who took it at face value would have gone
hunting for an API-version defect that does not exist, or, worse, "fixed" something to make it go away.
It is the inverse of this loop's recurring defect: instead of a check reporting success without looking,
a check reporting failure for a reason unrelated to what it checks. Both cost the same thing — the
signal stops meaning what its reader takes it to mean.

Cheap mitigation on my side, adopted: **do not run layer scripts concurrently with cargo jobs.** Worth
flagging to @operations for story-close as a possible guard-runner observation — a timeout REASON and a
violation REASON deserve to be distinguishable at a glance, since only one of them is the implementer's
to fix (the runbook already draws exactly this line for Layer 7's `PRECONDITION_FAILURE` vs `FAIL`).

#### The same defect from the orchestrator's side — and it cost a real attempt (@team-lead)

My version above was cheap: I noticed the artifact, isolated the guard, and lost minutes. The mirror
version was not. Gate 2 **attempt 2** failed on `STATUS=FAIL REASON=guard-timeout-validate-kustomize` —
again a timeout, not a violation — because the Lead launched the full pipeline into the middle of my
`cargo test` / `clippy` runs. The contention is measurable in the layer durations:

| Layer | Attempt 1 (quiet) | Attempt 2 (contended) |
|---|---|---|
| L1 | 10s | **975s** |
| L3 | 15s (my local run) | **713s**, timed out |
| L4 | 186s | **954s** |

Every guard that actually reported a verdict passed, and L5 — the only layer that had genuinely failed
on this code — came back OK. So one of three Gate 2 attempts was consumed by machine load, on a diff
with nothing wrong in it.

The symmetry is the point, and it is why this is recorded rather than shrugged off. I flagged that a
timeout is *shaped* like a violation and that an implementer taking it at face value would hunt a
defect that does not exist. The orchestrator then did the complementary version: read a green local L3
from me and started a pipeline that **manufactured** the false signal it was about to read. Neither
mistake was a misreading of a result — both were acting on a signal whose validity silently depended on
a precondition (an unloaded machine) that nothing in the signal expresses.

That is the loop's theme at the level of the *measuring apparatus* rather than the code: the pipeline's
timeouts are fixed wall-clock thresholds with **no load awareness**, so a busy machine fabricates
failures indistinguishable in the STATUS line from real ones. Two mitigations, neither in this diff:
distinguish timeout REASONs from violation REASONs at a glance (the runbook already draws exactly this
line one layer up, `PRECONDITION_FAILURE` vs `FAIL`), and give the orchestrator and implementer a
convention for not running load-generating work concurrently. For attempt 3 the second is being handled
by hand: I am idle until the Lead confirms the pipeline is finished.

### `no-secrets-in-logs`: four pre-existing false positives, and why rewording was the only mechanism

Gate 2 flagged four lines in `handlers/meetings.rs` as `[secret_in_tracing_field]`. All four are
**byte-identical to `HEAD`** — they entered the diff only because the guard scans whole changed files,
so the file becoming part of the diff is what exposed them. Another instance of the theme: the guard
had been reporting success on these lines because its scope never reached them, not because they were
clean.

**Why they are false positives.** Check 4 (`rust_log_secrets.rs:229-243`) fires when a line contains
`tracing::`, an `=`, a `%`/`?` marker, and *any* Category-A vocabulary word **anywhere on the line —
including inside the human-readable message string**. All four logged `error = %e` where `e` is a
`uuid::Error` or `ring::error::Unspecified`. No secret is in scope, let alone logged; the vocabulary
match was on prose like "Failed to parse org_id from token".

Notably Check 2 has a documented tightening for exactly this class — `SECRET_IN_LOG_SHAPE_RE` requires
one of three interpolation shapes, added after FPs on `token = ?req.token.id()`. Check 4 kept the loose
"word anywhere on line" test. Raised with @security and @semantic-guard as a guard-quality
observation; not fixed here, since `dt-guard` is not mine and the FP class is pre-existing.

**Why not `guard:ignore`.** It is inert for this rule. `rust_log_secrets.rs` does not import
`crate::ignore`, and `ignore.rs:57-59` names its three consumers as cite-extract, alert-rules and
metric-labels. An annotation would have looked like a justified suppression while suppressing nothing —
so the choice was made by mechanism availability, not preference. Worth stating, because "annotate the
false positive" is the reflex and it would have silently failed.

**Rewording, and the check that it was safe.** Each message now names the *claim* that failed to parse
rather than the credential it came from, which is more precise, not merely guard-shaped:
`"Failed to parse org_id from token"` → `"Failed to parse org_id claim"`; likewise the user-org and
user-ID variants; and `"Failed to generate random bytes for join token secret"` →
`"CSPRNG fill failed while generating the join credential"`.

Before rewording I grepped `docs/`, `infra/` and `scripts/` for all four strings: **zero hits**, so no
runbook or alert keys on them. That check was not optional — this devloop had just repaired a runbook
that grepped a log line which never existed, and silently invalidating four more greps while fixing
that class would have been the same defect committed knowingly.

### The practice that actually found them (@operations)

All four stale-artifact forms were found the same way: **someone re-swept their own surface after
seeing a form they had not been looking for.** @operations swept selectors; @observability found the
prose form in their own file afterwards; @operations then found the comment form in theirs. Each sweep
was of the finder's *own* files, triggered by a representation nobody had enumerated in advance.

That is a repeatable practice rather than luck, and it is the mitigation for exactly the forms a guard
cannot reach — which here is most of them. Stated as a rule: **when a new representation of a known
defect appears, every owner re-sweeps their own surfaces for that representation specifically.** It is
cheaper than a guard and it works on prose and comments, where a guard does not.

### Why contract scoping could not have prevented F-1

@operations initially attributed the comment-form miss to their own contract being scoped to
"Symptoms." The more useful attribution is mine — I rewrote four parts of that scenario and read past
the block an operator actually executes. The generalisable lesson is **"read the whole section, not
the items on the list,"** not "write a better list": a contract enumerates what its author already
knows to look for, so it structurally cannot cover a form nobody has seen yet. Contract scoping will
always be imperfect in exactly this direction, which is why the re-sweep practice above is the load-
bearing half.

### A count of one is not evidence of safety

The dashboard sweep found exactly one stale panel. But `:2572` and `:2681` survived because they
describe **shape** ("attempts by status", "latency percentiles"), not **meaning** — and a description
that encodes no semantics cannot go stale when semantics change. The count of one measures how many
panels had ever said anything substantive enough to become wrong, not how well the surface is
protected. Worth separating the result from the reason it came out that way.

### A check whose scope was narrower than its name — six instances in one loop

The single most repeated finding, and the reason it earns an entry is not the count. **None of these
was caught by the check itself.** Every one was caught by a person reading a result and going to look:

| Instance | Named | Actual scope |
|---|---|---|
| `ALL_ERROR_TYPES` | pins the `error_type` vocabulary | drives the *wrapper* with string literals; never constructs a `MeetingRefusal` |
| `current_count`'s `org_id = $1` | scopes the cap to one org | no fixture had two orgs where one held meetings — deleting the predicate red nothing |
| `status IN ('scheduled','active')` | counts live meetings toward the cap | the `'active'` arm evaluated, but never at the cap boundary |
| `LEAST($6, max_participants_per_meeting)` | caps participants at the org limit | **never executed** — every test requested fewer than the default |
| my `cargo clippy` + `grep '^error'` | "clippy clean" | cached (no output) *and* anchored against ANSI-coloured output |
| **R-6 itself** | `403 "limit exceeded"` | three unrelated causes |

A reader who takes away "write more tests" has missed it. The takeaway is that **a green check's scope
is the thing to verify, and the check cannot tell you its own scope** (@test's framing).

**The most dangerous member of the family is not on that list, because it never shipped.** The
completeness assertion I removed from `one_of_each_variant` — `assert_eq!(seen.len(), all.len())` — is
**tautological given the loop**: one insert per element, each already asserted to succeed, so it can
never fail. A future reader restoring it would not pay a rediscovery cost; they would add a check that
*cannot fail* while believing they had closed the completeness gap — CQ-9's defect reintroduced by
someone being careful, and harder to catch the second time because it arrives backed by deliberate
effort. That hazard is why `:404-407` became a pointer rather than a deletion (@code-reviewer).

**The mirror-image failure happened too, at the orchestrator.** The Lead adopted "quote the result with
the hash it was computed against," then quoted a **whole-diff** hash for a claim that was only ever
about `crates/` — a label whose scope was *wider* than its claim, reporting a mismatch that wasn't one.
@semantic-guard's mechanism caught it on first use by handing over the discrepancy rather than the
inference; the mechanism worked and its application was what was wrong. Both directions cost the same
thing: the signal stops meaning what its reader takes it to mean.

**And the fix generalises past comments** (@code-reviewer): the remedy for two descriptions of one
mechanism is not "delete one" but **make one of them stop being a description**. A pointer cannot
disagree with what it points at — the same move as deriving the metric label from the enum instead of
retyping it, and as the runbook quoting the log messages verbatim instead of paraphrasing them.

### Guards that read as covering more than they do

Three instances in one loop, all caught by reviewers rather than by me:
- `ALL_ERROR_TYPES` growth appears to pin the new label vocabulary; it hands the wrapper a string
  literal and would pass unchanged if `metric_label()` drifted (@observability's own retraction).
- A hand-enumerated `GcError` uniqueness test would keep passing while silently narrowing as variants
  are added (@test, T-9) — R-6's defect shape *inside the guard against R-6*. Fixed by making the
  enumeration compile-forced.
- The cap-test cluster reads as thorough coverage while driving entirely sequentially, so it exercises
  the cap's value and never its enforcement (@database). Fixed with a comment, since the underlying
  race is a tracked spin-out.

The common tell: a check whose passing outcome is also the outcome of the mechanism never running, or
never running on the thing the name implies. Same class the story itself records for the ten
`scripts/**/*.test.sh` files with no invocation site.

### Reviewers changed two contracts, not just wording

Two findings would have shipped a false contract into task #3: Decision 4 keyed a detector on a GC
status that AC's fail-closed `org_extraction` makes unreachable (@database, @test, @operations
converging independently), and `(500, INTERNAL_ERROR)` was not a distinct tuple because RNG failure
and collision exhaustion already emit it (@operations and @semantic-guard, independently). Both were
found by checking whether the world matched what the plan asserted, rather than by reading the plan
for internal consistency — which it had.
