# Devloop Output: MC KEK issuance, roster identity key, sender_id allocation

**Date**: 2026-09-02
**Task**: Story task 10 — MC-side ADR-0036 admission groundwork: meeting KEK, roster `identity_public_key`, non-recycling `sender_id` allocator, credential-leak guard scope extension
**Specialist**: meeting-controller
**Mode**: Agent Teams (v2), full
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: 2026-09-02 (resumed after an interrupted session; Gate 2 x2, Gate 3 with 9 reviewers)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `612bc379269dbbc383333592ef8cf053cdddcb50` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Headless | yes (`DEVLOOP_HEADLESS=1`, run-story task #10) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `meeting-controller` |
| Iteration | `3` |
| Security | `security` (spawned) |
| Test | `test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |
| Protocol (conditional) | `protocol` (spawned) |
| Infrastructure (conditional) | `infrastructure` (spawned) |

---

## Task Overview

### Objective
Implement in `crates/mc-service` the key and identity groundwork the ADR-0036 media path needs at participant admission: per-meeting AES-256 KEK (in-memory only) with a u16 generation counter, roster `identity_public_key` validation/publication (trust-on-first-use, no cnf check this story), and a monotonic non-recycling 16-bit `sender_id` allocator that fails closed on exhaustion. Plus extend the credential-leak semantic guard check definition to cover KEK/transmit-key material on the MC→MH contract and in MC logs.

### Scope
- **Service(s)**: mc-service (primary); `scripts/guards/semantic/checks.md` (guard definition, paired with security)
- **Schema**: No
- **Cross-cutting**: Yes — security (guard + key custody), protocol (wire shapes already landed by task 3), observability (no key material in telemetry; `key_custody=operator`)

### Debate Decision
NOT NEEDED — ADR-0036 already records the decisions (KEK model §4, 16-bit sender §2/§4, R-35 invariant, deferred cnf binding to story 2).

### Stated gaps (frozen by the task, not defects)
- No cnf thumbprint binding presented key → meeting token. Deferred to story 2. Security floor: trust-on-first-use ⇒ **same-keyholder consistency, never a verified identity**.
- KEK rotation / KEK-epoch namespace reclaim deferred to a later story; this story ships allocation discipline + fail-closed reject only.
- Positive-leak guard fixture supplied by the test specialist in a separate task.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. Every file in the intended final diff has a row. GSA paths per
`scripts/guards/simple/cross-boundary-ownership.yaml` are called out in the Notes column.

| Path | Classification | Owner (if not mine) | Note |
|------|----------------|---------------------|------|
| `crates/mc-service/src/media_admission/mod.rs` | Mine | — | NEW. Module doc carries the ADR-0036 §4 security floor. |
| `crates/mc-service/src/media_admission/kek.rs` | Mine | — | NEW. `MeetingKek`, `MeetingKeyState`, `MEETING_KEK_BYTES`. |
| `crates/mc-service/src/media_admission/identity_key.rs` | Mine | — | NEW. `IdentityPublicKey`, exact-length parse. |
| `crates/mc-service/src/media_admission/sender_id.rs` | Mine | — | NEW. `SenderId(NonZeroU16)`, `SenderIdAllocator`. |
| `crates/mc-service/src/lib.rs` | Mine | — | Register `media_admission` module. |
| `crates/mc-service/src/actors/meeting.rs` | Mine | — | KEK + allocator state, `handle_join`, roster, reconnect continuity. |
| `crates/mc-service/src/actors/messages.rs` | Mine | — | `JoinConnection`/`ConnectionJoin`/`JoinResult`/`ParticipantInfo` fields. |
| `crates/mc-service/src/actors/controller.rs` | Mine | — | Forward the typed identity key through the routing hop. |
| `crates/mc-service/src/webtransport/connection.rs` | Mine | — | Boundary validation + `build_join_response` population. |
| `crates/mc-service/src/webtransport/handler.rs` | Mine | — | Roster fields on the `ParticipantJoined` fan-out. |
| `crates/mc-service/src/actors/participant.rs` | Mine | — | Unplanned, mechanical: one `ParticipantInfo` literal in a `#[cfg(test)]` module gains the two new fields. Surfaced by `cross-boundary-scope`, not by me — the guard did its job. |
| `crates/mc-service/tests/disconnect_latency_integration.rs` | Mine | — | Unplanned, mechanical: `MeetingActor::spawn` is now fallible (KEK generation) and `connection_join` takes the identity key, so 1 spawn site + 4 join sites update. Also surfaced by `cross-boundary-scope`. |
| `crates/mc-service/src/errors.rs` | Mine | — | Two new `McError` variants + code/label/client-message arms. |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — | `record_meeting_kek_generated()`, `key_custody` consts, `MetricAssertion` test. |
| `crates/mc-service/Cargo.toml` | Mine | — | `test-seams` feature + self dev-dependency (allocator-cursor seam). |
| `crates/mc-service/tests/media_admission_integration.rs` | Mine | — | NEW. The six required integration tests. |
| `crates/mc-service/tests/join_tests.rs` | Mine | — | 4 `JoinRequest` construction sites gain a valid key. |
| `crates/mc-service/tests/webtransport_accept_loop_integration.rs` | Mine | — | 1 construction site. |
| `crates/mc-service/tests/media_connection_update_integration.rs` | Mine | — | 1 construction site. |
| `crates/mc-service/tests/otel_webtransport_integration.rs` | Mine | — | 1 construction site. |
| `crates/mc-service/tests/common/mod.rs` | Mine | — | Re-export of the shared test-key helper. |
| `crates/mc-test-utils/src/lib.rs` | Mine | — | Register `media` test-fixture module. |
| `crates/mc-test-utils/src/media.rs` | Mine | — | NEW. `sample_identity_public_key()` — one home for the 32-byte test key. |
| `crates/env-tests/src/fixtures/mod.rs` | Not mine, Mechanical | infrastructure | Register `media` fixture module — one `pub mod` line. |
| `crates/env-tests/src/fixtures/media.rs` | Not mine, Minor-judgment | infrastructure | NEW. env-tests' own fixture home for the 32-byte key (@dry-reviewer #2): env-tests must NOT dev-dep on `mc-test-utils`, which depends on `mc-service` — linking the service under black-box cluster test inverts the ADR-0028 layering. Carries an `ANCHOR (DRY)` naming the `mc-test-utils` helper as source. |
| `crates/env-tests/tests/24_join_flow.rs` | Mine | — | 2 construction sites. MC-owned env-test (story task 13 extends it later). |
| `crates/env-tests/tests/26_mh_quic.rs` | Not mine, Mechanical | media-handler | 2 construction sites; identical one-line field fill, no judgment. Would otherwise red at Layer 7. |
| `docs/observability/metrics/mc-service.md` | Mine | — | MC's own catalog (ADR-0031 service-owned). @observability reviews content. |
| `infra/grafana/dashboards/mc-overview.json` | Mine | — | MC's own dashboard (ADR-0031 service-owned). Required by `dt-guard metric_no_dashboard`. |
| `docs/runbooks/mc-incident-response.md` | Mine | — | Scenario 8 root-cause rows only (OPS-2). No numbered scenario — 15/16 stay reserved for task 21. |
| `docs/runbooks/mc-deployment.md` | Mine | — | Extend the existing wire-lockstep coordination item (OPS-3). |
| `docs/TODO.md` | Mine | — | §Media Path Obligations: KEK-epoch reset, reconnect KEK re-issue, `supported_codecs` cap allocation, e2e/zero-trust-boolean guard gap. Plus **updating entry :618** to record the CLAUDE.md interim landing — **not closing it**; the general non-GSA path→specialist map still needs a micro-debate. |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Mine | — | New code locations + test file. |
| `docs/devloop-outputs/2026-09-02-mc-kek-identity-sender-id/main.md` | Mine | — | This file (Loop State table left to @team-lead). |
| `CLAUDE.md` | Not mine, Minor-judgment | infrastructure | @infrastructure's verbatim paragraph, placed after the `operations` table row (`CLAUDE.md:54`) and before the `Definitions:` line. **Not a flat `crates/dt-guard/** → infrastructure` row** — they ruled that actively wrong and guaranteed to recur in the opposite direction; the machinery-vs-content split is the point. Text recorded in §Decisions 5; placed verbatim, not paraphrased. |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Mine | — | Gate 3 (@operations OPS-C, Lead ruling 2). NEW alert `MCSenderIdSpaceExhausted`. MC-owned per ADR-0031. |
| `crates/mc-service/src/grpc/mc_service.rs` | Mine | — | Gate 3 (@observability OBS-6). `mc_meeting_assignments_total` at four exit points + `rejection_reason_label`. |
| `crates/mc-service/tests/meeting_assignment_metrics_integration.rs` | Mine | — | NEW, Gate 3 (OBS-6). Wrapper-invocation coverage; follows the documented `redis_metrics_integration.rs` precedent. |
| `crates/dt-guard/src/metric_labels.rs` | Not mine, Domain-judgment | observability, security | Gate 3 (@observability OBS-5). **Tests only** — behavioural pins over existing constructs. Content, not machinery, on @infrastructure's own ruling. |
| `scripts/guards/semantic/checks.md` | Not mine, Domain-judgment | semantic-guard (paired), co-sign security | Defines a control's detection scope — judgment-bearing prose. NOT in the GSA manifest, so the Layer-B guard is silent on it; routed via `--paired-with` anyway. |
| `crates/dt-guard/src/release_build_profile.rs` | Not mine, Minor-judgment | infrastructure, security | **Doc comment only, no logic.** This diff falsified its "the control being protected does not exist in tree yet… no `compile_error!` and no `debug_assertions` usage anywhere under `crates/`" — `mc-service/src/lib.rs`'s `test-seams` `compile_error!` is now exactly that control, and is the only such usage outside dt-guard. Corrected because my change invalidated it, at @security's ask; guard re-run clean. |
| `crates/dt-guard/src/common/pii_vocabulary.rs` | Not mine, Domain-judgment | observability, security | CATEGORY_A additions (`meeting_kek`, `transmit_key`) into **`NON_CREDENTIAL_TOKENS`**, plus OBS-1's `sender_id` → CATEGORY_B, plus INFRA-1's stale-prose fix. **@infrastructure ruled themselves OFF this trailer set** (see §Decisions 5) — this hunk *satisfies* the module's machinery constructs rather than *changing* them. Unfrozen. No `Approved-Cross-Boundary: infrastructure`. |
| `docs/observability/label-taxonomy.md` | Not mine, Domain-judgment | observability, security | §PII/Secret Denylist rows, landing with the guard edit per the taxonomy's own Extension policy. |

**No `proto/**`, no `proto-gen/**`, no `crates/media-protocol/**`, no `crates/common/**` rows.**
This diff makes **zero proto edits** and **zero proto-gen edits** — the task-3 contract is
consumed as-is. `media-protocol` is *read* for its anchor constants (`KEY_ID_SENDER_ID_BITS`,
`KEK_GENERATION_FIELD_BYTES`); reading a const is not an edit, so no GSA trailer is triggered.
`crates/common/src/secret.rs` is reused as-is (`SecretBox`), not extended — same reasoning.

**Drop-if-contested:** the three cross-boundary rows are separable. If @observability/@security
would rather the `pii_vocabulary.rs` + `label-taxonomy.md` pair land in the observability lane, I
will drop those two rows and file the gap in `docs/TODO.md` instead; they are not named in my task
prompt, only `scripts/guards/semantic/checks.md` is.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Protocol (conditional) | confirmed |
| Infrastructure (conditional, added mid-planning for OPS-11) | confirmed (scoped to `CLAUDE.md` + TODO's **"`crates/dt-guard/**` has no owner derivable from the path"** entry) |

### Lead rulings during planning (2026-09-02)

1. **Empty `identity_public_key` is REJECTED** — fail closed. Empty is not exact-length, therefore malformed: same `IdentityKeyInvalid` variant, same byte-identical generic message, same `identity_key_invalid` label. Consequence taken in full (OPS-3): an old client sending no key is a hard join rejection, not a degraded media path, and the mid-branch window before client tasks 15/19/20 is a total join outage — the wire-lockstep item in `docs/runbooks/mc-deployment.md` §Coordination is amended in this diff.
2. **`ParticipantCapabilities.supported_codecs` over-cap is FILED, not taken.** `signaling.proto:130-138` misassigns its enforcement to task 10; the remedy needs a configured cap nobody has chosen (design decision — the valid deferral pattern). Filed in `docs/TODO.md` §Media Path Obligations per the `supported_header_versions` precedent at the frame-header-version-floor TODO entry. No proto edit; the comment repoint is named as protocol-owned follow-up.
3. **`test-seams` cargo feature** approved in principle (bar: compile-time absence in a production build); implementer must first justify it against a narrower seam, and @code-reviewer rules on the final shape. **No KEK seam of any kind.**
4. **`mc_meeting_kek_generated_total{key_custody="operator"}` stays**, and the `crates/dt-guard/src/common/pii_vocabulary.rs` + `docs/observability/label-taxonomy.md` rows **stay in this diff** rather than being dropped and filed — the `\bkek\b` word-boundary trap means the naive version ships as inert coverage, which is a masked failure, not a scope question. Hunk-level ACKs required from observability + security.

Lead also backed, and @code-reviewer/@dry-reviewer/@protocol independently agreed: **reading a GSA constant and instantiating an exported GSA type are not edits** and trigger no trailer. `media-protocol` anchors are read; `common::secret::SecretBox` is reused as-is. Zero `proto/**` and zero `proto-gen/**` rows.

---

## Planning

### Mechanism restatement (and what it widened to)

**Instance-language** (the task): MC generates a KEK, validates an Ed25519 key, allocates a
`sender_id`.

**Mechanism-language**: *MC's join path is the single trust boundary where client-supplied
`JoinRequest` bytes become meeting state that MC republishes to every other participant, and MC's
per-meeting actor is the sole issuing authority for the per-meeting facts the media path binds keys
to.* Three disciplines fall out, one per fact:

| Fact | Discipline | Failure if absent |
|---|---|---|
| Meeting KEK | secret MC generates and must never let escape | operator-custody claim becomes false |
| `identity_public_key` | client-supplied blob, shape-checked before republication | a malformed blob fans out to every roster |
| `sender_id` | bounded monotonic namespace, never reused | AES-GCM (key, nonce) collision — R-35 |

**What the restatement surfaced — one same-owner sibling the task does not name.**
`ParticipantCapabilities.supported_codecs` (`signaling.proto:130-138`) says verbatim: *"Server-side
configuration caps it and MC REJECTS an over-cap declaration… **Enforcement owner: story task 10
(join-path validation)**."* That is the same mechanism (unbounded client-supplied field on
`JoinRequest`, validated at the same boundary, published into the same meeting state) and the proto
names *this* task as its owner. It is not in my prompt.

I am **not** silently taking it, and I am **not** silently dropping it. Recommendation to
@team-lead: file it in `docs/TODO.md §Media Path Obligations` as an explicitly-unallocated
obligation and let @protocol repoint the proto comment in their own lane — the exact remedy already
applied at `docs/TODO.md`'s **"The frame-header-version allowlist and minimum-version floor"** entry to `supported_header_versions`, where the identical dangling binding
cost a Gate 3. Reason not to fold it in here: it needs a config knob, which under OPS-4 drags in a
ConfigMap key, startup validation and a `mc-deployment.md` §Configuration Reference row. If
@team-lead rules it in, I will take it with those artifacts.

I am explicitly **not** taking `supported_header_versions`' allowlist + minimum floor —
`docs/TODO.md`'s **"The frame-header-version allowlist and minimum-version floor"** entry records that its allocation is a story-decomposition call for the Lead, not
something a specialist can self-assign.

### Design

**New module `crates/mc-service/src/media_admission/`** (`mod.rs` + three files). One home for the
three admission facts, so the security floor is stated once at the top of `mod.rs` and every type
below it inherits the context.

**1. `kek.rs` — `MeetingKek` / `MeetingKeyState`**

- `MeetingKek(SecretBox<[u8; MEETING_KEK_BYTES]>)` — `common::secret::SecretBox`, reused as-is
  (@security A.1, @dry-reviewer #3). Redacted `Debug` and zeroize-on-drop come from the type, so
  item 3 of the credential-leak check is **structurally** satisfied rather than reviewer-enforced.
  Answering @observability's question directly: **not** a plain `[u8; 32]`, and **not** a
  hand-rolled redacting newtype either — a derived `Debug` over `SecretBox` cannot be got wrong by
  a later author, whereas a hand-rolled one can.
- `MEETING_KEK_BYTES: usize = 32`, defined here as the first production home, with a
  `#[test] fn meeting_kek_length_matches_aes_256_gcm()` asserting equality with
  `ring::aead::AES_256_GCM.key_len()` — `key_len()` is not `const` in ring 0.17, so a test is the
  drift guard, per CLAUDE.md "derive one from the other, or add a guard" (@dry-reviewer #2). A
  comment records @dry-reviewer's boundary C: this 32 is **not**
  `frame.rs::WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES`'s 32.
- `MeetingKek::generate(&SystemRandom) -> Result<Self, KekGenerationFailed>` — `ring::rand::SystemRandom`,
  `fill()`'s `Result` propagated. **No `unwrap`/`expect`, no fallback, no partial/zero/default key**
  (@security A.2, ADR-0002). Failure fails meeting-actor creation.
- **No seeded/deterministic RNG seam of any kind**, `cfg(test)` or otherwise (@security A.2 — I
  agree that would be the worst thing this task could ship). Tests assert *properties* (length,
  non-zero, two meetings differ), never a fixed value.
- `MeetingKeyState { kek: Arc<MeetingKek>, generation: u16 }`. `Arc` so the join path clones a
  handle, not the key bytes — fewer copies of key material in memory, and it keeps `JoinResult:
  Clone` (which `SecretBox` alone would break). Generation starts at **0**: the proto states 0 is a
  legal first generation and forbids a `!= 0` sentinel reading. A `const _: () = assert!(...)` ties
  the `u16` to `frame.rs::KEK_GENERATION_FIELD_BYTES * 8 == u16::BITS`.
- Doc comment at the generation point states OPS-6: the KEK dies with the meeting actor; an MC
  restart or ownership move means every participant needs a **fresh join** to get the new one;
  **this story ships no KEK push and no re-attach delivery path.**

**2. `identity_key.rs` — `IdentityPublicKey`**

- `IdentityPublicKey([u8; IDENTITY_PUBLIC_KEY_BYTES])`, `IDENTITY_PUBLIC_KEY_BYTES = 32`, first
  production home. @protocol verified there is **no** existing Ed25519 public-key length constant
  anywhere in the tree — unlike the sender width and the KEK-generation width, which MUST derive
  from `media-protocol/frame.rs`, this one has no upstream anchor, so MC instantiating the literal
  once is correct. The `ANCHOR (DRY)` comment points at **both** proto sites the length governs —
  `JoinRequest.identity_public_key` (tag 7) and `Participant.identity_public_key` (tag 6), both of
  which say "Exactly 32 bytes" in prose today — and states explicitly that the reciprocal
  proto-side anchor is a **named, protocol-owned follow-up deferred only because the zero-proto-edit
  line forbids the comment edit in this diff**, not an omission. @protocol owns and tracks it.
  Phrasing follows `docs/TODO.md §Media Path Obligations` entry (b), where `PROTOCOL_VERSION` gained
  a reciprocal `ANCHOR` line.
- `IdentityPublicKey::try_from_bytes(&[u8])` → `Err(IdentityPublicKeyRejected)`, a **unit struct
  with one message**. Exact length only, and **"well-formed" must not reach the code** (@security,
  resolving @protocol's cross-lane flag). The task prompt's phrase "well-formed Ed25519 public key"
  is barred from any function name or doc comment: what this builds is an exact-length check on an
  **opaque 32-byte blob** and it must say so. No `validate_ed25519_public_key`, no `..._well_formed`.
  Same overclaim bar as attested/verified/trusted, applied to the *check* rather than the key.
- The constructor doc states the negative explicitly: exact-length only; **not** a point-on-curve
  check, **not** small-order rejection, **not** canonical-encoding validation. Recorded as a stated
  gap **with its trigger**, alongside the cnf gap: *"…none is required while the roster key is
  trust-on-first-use: a degenerate key costs only its own publisher the same-keyholder property, and
  MC could publish an arbitrary key anyway with no cnf binding. This stops being true when the cnf
  binding lands — an attested small-order key would be forgeable by anyone — so point and
  small-order validation are required no later than that story."*
- The reason length-only is accepted is the **dependency cost and the unclaimed property**, not the
  oracle argument. @security explicitly withdrew their earlier "a curve-validity error would be a
  second distinguishable outcome" rationale as wrong-headed — a second cause folded into the same
  generic error and the same single `identity_key_invalid` label is not an oracle at all. The real
  grounds: there is no Ed25519 crate in the workspace and `ring` has no standalone point-validation
  API, so this would mean a new crypto dependency in mc-service (audit dep-change gate plus
  ADR-0027's path-independent approved-crypto GSA); and the residual is contained — an invalid point
  breaks only the publisher's own signatures, a small-order key only lets others forge frames as
  *that* participant, and a malicious MC gains nothing it cannot already do without the cnf binding.
  The non-oracle requirement itself is unchanged and unconditional: one error value, one message, one
  label, byte-identical across all malformed cases.
- **Non-oracle, both directions**: one error value, one client-facing message, and **one**
  `error_type` label — absent, short, and long are indistinguishable in the response *and* in
  telemetry (@security A.5, @observability's `identity_key_invalid` preference).
- **Empty/absent is REJECTED.** `ParticipantInfo.identity_public_key` is therefore
  `IdentityPublicKey`, **not** `Option<…>` — "a non-32-byte value on the roster" becomes
  structurally unreachable rather than checked. This is the one design decision I want an explicit
  Gate-1 ruling on; see §Open decision below.
- Security floor stated verbatim in the type's doc comment: no `cnf` thumbprint check this story;
  the roster key is accepted **trust-on-first-use**; a verifying signature proves only
  **same-keyholder consistency**, never a verified identity; the client-validated AC attestation is
  story 2. No `attested` / `verified` / `trusted` in any name.

**3. `sender_id.rs` — `SenderId` / `SenderIdAllocator`**

- `SenderId(NonZeroU16)`. The **type is the bound**: `NonZeroU16` excludes 0 by construction, so
  there is no `u16::try_from` anywhere on this path, no `as`, no `wrapping_*`, no `%`
  (@security A.4). A `const _: () = assert!(KEY_ID_SENDER_ID_BITS == u16::BITS as usize)` ties
  `NonZeroU16` to `crates/media-protocol/src/frame.rs::KEY_ID_SENDER_ID_BITS` — that is the SSoT
  link @code-reviewer #4 and @dry-reviewer #1 asked for, and it means **no local 16 / 65535 / `1 <<
  16` literal exists at all**. First MC import of the already-declared `media-protocol` dep.
- Allocator state is a single `next: Option<NonZeroU16>` cursor, seeded to `NonZeroU16::MIN` (1):

  ```rust
  let id = self.next.ok_or(SenderIdSpaceExhausted)?;          // exhausted => reject, never allocate
  self.next = id.get().checked_add(1).and_then(NonZeroU16::new); // None at the wall
  ```

  `checked_add` before the next allocation, `None` is the wall, and the wall is a **reject**. No
  warn-and-continue, no wrap, no truncate. @dry-reviewer boundary A honored: this 65535 is not
  `kek_generation`'s — no shared constant, no shared helper, and the two are not even the same
  *kind* of thing here (one is a type-level bound, the other a counter width assert).
- **OPS-5, answered: option (b).** The allocator is an O(1) cursor — no per-participant map, no
  retained departed entries, no unbounded state. A departed participant is removed from
  `self.participants` and its `sender_id` is simply never issued again. **Reconnect continuity is a
  different path**: `handle_reconnect` resolves ADR-0023 `correlation_id` → live participant, which
  still holds its `sender_id`, so continuity is preserved *by the participant not being removed*,
  not by any recovery map. A reconnect past eviction is a fresh join and gets a **new** `sender_id`
  — permitted (non-recycling), and stated in the code comment so nobody later "fixes" it into
  recycling.
- **@security A.3, answered: the continuity key is ADR-0023 `correlation_id` + HMAC `binding_token`**
  (`SessionBindingManager::validate`), never a bare client-supplied `participant_id`. Nothing else
  can reach a live participant's `sender_id`.
- **OPS-7 (joint operations + observability, not optional)**: `allocate()` returns
  `Allocation { sender_id, high_watermark_crossed: bool }`. The **fired-flag is state on the meeting
  actor**, not recomputed from the counter, so no path can re-fire it — @observability's amendment;
  reconnect should not advance the allocator at all, and holding the flag on the actor makes that
  robust rather than incidental. **Edge-triggered, one-shot at the crossing.** Threshold is a named
  const (`SENDER_ID_HIGH_WATERMARK_PERCENT = 90`) with the reasoning in a comment, and the comment
  states the honest purpose: this is a **forensic / post-incident reconstruction** signal, *not* a
  proactive-action signal, because the remedy at 90% and at 100% is identical.
  Because the rationale is "sudden or gradual", the line carries **cumulative admissions, remaining
  namespace, and elapsed-since-meeting-create** — answerable from the single line without joining
  back to the meeting-create record. No key material, no `identity_public_key`, no `sender_id`
  value. It rides the meeting actor's existing `#[instrument(skip_all, fields(meeting_id =
  %self.meeting_id))]` span, so meeting scope is free.

**4. Wiring**

- **Validation site**: `webtransport/connection.rs`, at the same trust boundary that already
  length-bounds `participant_name` — *before* any actor interaction. Parse-don't-validate: a typed
  `IdentityPublicKey` travels through `JoinConnection` → `ConnectionJoin` → `handle_join`, so the
  actor cannot receive an invalid one.
- **Allocation + KEK read**: `handle_join` in the meeting actor. `JoinResult` gains
  `sender_id: SenderId`, `meeting_kek: Arc<MeetingKek>`, `kek_generation: u16`. `JoinResult` keeps
  its derived `Debug` — safe, because `SecretBox`'s `Debug` is redacted and `MeetingKek` has no
  other field.
- **Wire fill**: `build_join_response` sets `sender_id: Some(u32::from(id.get()))`,
  `meeting_kek: kek.expose().to_vec()`, `kek_generation: u32::from(generation)`. Roster
  `Participant` entries get both `sender_id` and `identity_public_key` from `ParticipantInfo`
  (@protocol: both populated on every entry, so key-id → `sender_id` → roster → key resolves).
  `handler.rs::encode_participant_update` gets the same two fields for the `ParticipantJoined`
  fan-out.
- **KEK never crosses MC→MH**: no change to `grpc/mh_client.rs`, `grpc/mc_service.rs`,
  `grpc/media_coordination.rs`, or any `dark_tower.internal.v1` construction. I will state this as
  a verified negative at Gate 2 with the grep that shows it (@security B.7).

**5. Errors (OPS-1 + @observability item 1)**

Two new `McError` variants, each with its own bounded `error_type_label()`. Neither folds into
`InvalidArgument` or `Internal` — answering @dry-reviewer's question: **not** reusing
`InvalidArgument`, because `mc-incident-response.md` Scenario 8 triages join failures *exclusively*
by that label and a shared value routes the responder to "may require code fix or rollback" for two
conditions that are neither.

| Variant | `error_code()` | `error_type_label()` | `client_message()` |
|---|---|---|---|
| `IdentityKeyInvalid` | 1 `INVALID_REQUEST` | `identity_key_invalid` | `"Invalid join request"` |
| `SenderIdSpaceExhausted` | 7 `CAPACITY_EXCEEDED` | `sender_id_space_exhausted` | `"Meeting is at capacity"` |

The client-facing genericness and the server-side specificity are deliberately split: the label
goes to Prometheus, not to the client, so it is not an oracle. `"Meeting is at capacity"` is
*byte-identical* to `MeetingCapacityExceeded`'s so the client cannot distinguish them either.

**OPS-8 — the wall needs its own localizing log line.** The exhaustion reject reaches
`record_session_join` through the `Ok(Err(e))` arm in `connection.rs` (~line 440), but that arm's
`warn!` and its enclosing span carry **`connection_id` only** (`connection.rs`'s connection-span `#[instrument]` is
`#[instrument(skip_all, name = "mc.webtransport.connection", fields(connection_id = ...))]` — no
`meeting_id`), and the metric correctly carries no meeting identifier. So at the wall an operator
would get a counter increment and a connection id and **could not tell which meeting exhausted**.
The OPS-7 watermark line does carry `meeting_id`, but it is the *gradual* signal and can be
arbitrarily far in the past — for the long-lived-meeting case, potentially outside log retention —
so leaning on it re-couples two signals @operations deliberately kept separate. Fix, one line and
free: emit the reject `warn!` **inside `handle_join`**, which is already
`#[instrument(skip_all, fields(meeting_id = %self.meeting_id))]` (`actors/meeting.rs::handle_join`) and which
I am editing anyway. Content mirrors the watermark line's forensic shape (cumulative admissions,
elapsed-since-create) so the two read as one story. No new label, no new metric, no new artifact.
The `connection.rs` arm is unchanged — this is additional, not a move.

**@observability item 3 — the metrics path.** Both rejects reach an existing
`record_session_join("failure", Some(e.error_type_label()), join_start.elapsed())` site in
`webtransport/connection.rs`: the identity-key reject uses the boundary early-return arm (the same
shape as the `participant_name` too-long arm, ~line 300), and the exhaustion reject propagates as
an `Err` out of `handle_join` into the existing `Ok(Err(e))` arm of the `join_rx.await` match
(~line 440). No new counter for either — `mc_session_join_failures_total{error_type}` already
carries them.

**6. Telemetry — `key_custody=operator`, first emission site in the tree**

One new counter: **`mc_meeting_kek_generated_total{key_custody="operator"}`**, incremented once per
meeting-actor creation. This is the story's named "(MC) KEK issuance counter" and the honest first
home for `key_custody`. It is also forward-compatible: when rotation lands it counts rotations and
genuinely diverges from meeting creation.

**OBS-2 — it is NOT the Scenario-16 "Missing Key Material" signal, and neither the plan, the catalog
row, nor the panel description may say it is.** It increments unconditionally, once per
meeting-actor creation, so it is identically the meeting-creation count; there is no path where the
actor exists and the increment does not happen. A "missing key material" condition would have to
show as an *absence*, which needs a second series to divide by, and MC has no meeting-creation
counter (`mc_meetings_active` is a gauge) — so the inference is not computable even in principle.
Worse, it is misdirected: the real not-provisioned condition is defined on the **join-response**
side (`signaling.proto:420` — "the not-provisioned signal is `meeting_kek` not being exactly 32
bytes"), a per-join condition. A Scenario-16 responder sent here would be watching a series that
cannot move differently in the failure case, and a false capability in a runbook consulted under
time pressure is worse than a missing one.

**The claim was wrong twice, and the second half came from @operations.** Scenario 16 was never meant
to have an MC-side signal at all: `docs/user-stories/2026-08-27-hear-yourself-through-handler.md:446`
keys it on **client-side** frames-dropped-by-reason counters ("no KEK for the carried generation, and
no roster entry for the sender"), and the prompt states outright that the alert fires client-side
while the remedy lives in MC's delivery path — which is precisely why it sits in MC's runbook.
Client-side **by design**, not by omission; task 21 already depends on tasks 19/20 for it.

**Catalog and panel wording: KEK issuance rate and the `key_custody=operator` carrier, nothing more —
and no mention of Scenario 16 even to disclaim it** (@observability). A disclaimer would reintroduce
the association the finding exists to sever, in the one document a responder greps.

@observability's debt entry is scoped to the reusable, true statement — *"MC has no meeting-creation
counter, so an absence-of-KEK-issuance inference is not computable from MC's metric surface"* — and
deliberately **not** to "Scenario 16 needs an MC-side signal", which would send a future author to
build a join-path metric that §11's meeting-identifier rule then makes awkward.

Three artifacts land with it: catalog row in
`docs/observability/metrics/mc-service.md`, a panel in `infra/grafana/dashboards/mc-overview.json`,
and a `crates/mc-service/tests/**` reference — satisfying `metric_no_catalog`, `metric_no_dashboard`
and ADR-0032 `metric-coverage`.

- Label key and value are **`&'static str` consts** (`KEY_CUSTODY_LABEL` / `KEY_CUSTODY_OPERATOR`)
  in `observability/metrics.rs`, with a "promote to `crates/common/src/observability/` at the second
  consumer" comment — @observability explicitly allowed the MC-local placement. **Never** derived
  from config, a feature flag, deployment mode, or whether a KEK happens to exist: it is a
  constraint, not a snapshot.
- **Scope, per @observability's amendment**: `key_custody` lands on **key-custody-relevant
  emission sites only** — the KEK-generation counter and its `info!` line. I am deliberately *not*
  adding it to unrelated MC log lines; the label-taxonomy's unscoped "every emission site" wording
  is filed as debt by @observability. Fleet-wide rollout is R-26 / observability task 22.
- **No end-to-end or zero-trust boolean** anywhere — metric, log, span, dashboard, doc comment.
- **No `meeting_id` and no `meeting_id_hash` label** on anything I add (@observability + OPS-7,
  settled). *Which meeting* is answered by the meeting actor's existing span field.
- `sender_id`, `identity_public_key` and the KEK appear in **no** metric label and **no** span
  attribute. Any `#[instrument]` I touch keeps `skip_all`.

**7. Test seam for exhaustion (@test's question)**

`crates/mc-service/Cargo.toml` gains a non-default feature `test-seams`, enabled only via a
self dev-dependency (`mc-service = { path = ".", features = ["test-seams"] }`). Behind it,
`SenderIdAllocator::resuming_from(next: Option<NonZeroU16>)` and a `MeetingActor::spawn` variant
that accepts a pre-seeded allocator. **The seam does not exist in a production build** — it is a
compile-time absence, not a runtime branch. Tests seed the cursor at 65534 and perform two joins
plus a third; no 65535-join loop, so the Layer-4 budget is untouched. There is **no** KEK seam
(@security A.2).

### Integration tests — `crates/mc-service/tests/media_admission_integration.rs`

Six tests. None of the names contains `attested` / `verified` / `trusted`; each carries a comment
stating the honest floor.

1. `kek_is_generated_at_meeting_create_and_returned_in_join_response` — reads `meeting_kek` **out
   of the decoded `JoinResponse`**, asserts `len() == 32`, asserts not all-zero, asserts
   `kek_generation == 0`; and asserts two different meetings yield **different** KEKs (the
   in-memory, per-meeting, randomly-generated property, without pinning a value).
2. `malformed_identity_key_is_rejected_with_a_generic_error` — table-driven over
   `[]`, `[0u8; 31]`, `[0u8; 33]`, `[0u8; 64]`. Asserts an `ErrorMessage` comes back, asserts
   `code == ERROR_CODE_INVALID_REQUEST`, and asserts **all four cases produce the byte-identical
   message** — that equality assertion *is* the non-oracle check @test asked for, and it is
   stronger than asserting the message lacks particular words.
3. `sender_id_is_not_recycled_across_a_leave_and_a_later_join` — A joins (id₁), A leaves, B joins
   (id₂). Asserts `id₂ != id₁` **and** `id₂ > id₁` (monotonic), and that A's id is not present on
   the post-leave roster.
4. `sender_id_survives_a_reconnect_for_the_same_participant` — **distinct from 3** (@test). Joins,
   reconnects via ADR-0023 `correlation_id` + `binding_token`, asserts the *same* `sender_id`.
   Comment states continuity ≠ recycling and names the continuity key.
5. `sender_id_exhaustion_rejects_the_admission` — seeds the cursor at 65534 via the `test-seams`
   constructor. Two joins succeed (65534, 65535); the third is **rejected**. Asserts an
   `ErrorMessage` with `ERROR_CODE_CAPACITY_EXCEEDED`, asserts the participant is **not** on the
   roster, and asserts `mc_session_join_failures_total{error_type="sender_id_space_exhausted"}`
   incremented by 1 via `MetricAssertion` (@observability item 7 — pinning the label a refactor is
   most likely to collapse into `internal`).
6. `roster_carries_the_joiner_identity_public_key_and_sender_id` — a second participant's
   `JoinResponse.existing_participants` entry carries both fields, byte-equal to what the first
   participant sent. Comment: this is the key-id → `sender_id` → roster → key chain, and it proves
   **same-keyholder consistency only**.

Unit tests alongside the types: allocator monotonicity/exhaustion arithmetic, the
`MEETING_KEK_BYTES == AES_256_GCM.key_len()` drift guard, the two `const _` width asserts,
`IdentityPublicKey` length rejection, and `errors.rs` code/label/message arms for the two new
variants. Fixtures use `[0x42u8; 32]`-style non-secret patterns; no real key material anywhere.

### `scripts/guards/semantic/checks.md` (paired with @security, owner @semantic-guard)

I am adopting **@security's proposed wording verbatim** as the base — both edits, including the
line-9 language-scope fix, which @semantic-guard independently required (their item 6) and which is
the drift trap. Two additions from @semantic-guard's list folded in:

- Their item 4 — **relayed frame bytes are explicitly out of scope**: a wrapped transmit key
  travelling client→client inside a frame payload MH relays opaquely is not a finding; the check
  fires on key material as a **distinct field/param** in an MC→MH control message or in MC
  telemetry.
- Their item 5 — **fixture-buildability**: name one concrete positive shape (`kek: [u8; 32]` or the
  `MeetingKek` newtype added as a field on an MC→MH request; `debug!(?kek)`) and one concrete
  negative (`debug!(kek_generation = gen, sender_id = id)`), so the test specialist's separate
  fixture task has both poles to build against.

**@semantic-guard's REQUIRED refinement to item 13, taken.** As @security first drafted it, the
clause "a `#[derive(Debug)]` newly added to a struct that transitively reaches key material" fires
on `MeetingKek(SecretBox<[u8; 32]>)` itself **and** on `JoinResult` — the exact two types this
design declares safe, and the one pattern in this very diff. A check with an inverted verdict on its
own reference implementation is how a check gets ignored. Item 13 gains:

> **Redacting wrappers break the transitive reach.** A `#[derive(Debug)]` is SAFE when every field
> on the path to key bytes is a redacting secret wrapper whose own `Debug` redacts —
> `common::secret::SecretBox` is the in-tree one. It is a finding only when the derive would print
> raw key bytes because a field on the path is a raw `[u8; N]`, `Vec<u8>`, or another non-redacting
> type. A hand-rolled `Debug`/`Display` that prints the bytes is always a finding, wrapper or not.
>
> Verify the wrapper's `Debug` actually redacts rather than trusting a `Secret`-shaped name — the
> same judge-the-value rule as above, and it fails in both directions: a newtype over `SecretBox` is
> safe however it is named, and a newtype named `SecretKey` over a raw `[u8; 32]` is not.
>
> Scope: `Debug`/`Display` only. It does NOT extend to `Serialize` or other serialization derives —
> those are a different sink, under item 12.

and the item-5 fixture poles gain that axis: POSITIVE (must fire) `#[derive(Debug)]` on
`struct Foo { kek: [u8; 32] }`; NEGATIVE (must not fire) `#[derive(Debug)]` on
`struct Foo { kek: MeetingKek }` where `MeetingKek` wraps `SecretBox`.

**@semantic-guard's condition on exclusion (a), taken.** Keeping the no-end-to-end/zero-trust-boolean
rule out of Credential Leak is right, but §11 still requires it be enforced *somewhere*, and landing
it in no named check is precisely the unguarded "not by review" state §11's coverage-demonstration
section warns about. A `docs/TODO.md` entry records it: *the end-to-end / zero-trust boolean
overclaim needs its own named guard or check — not credential-leak.* That the code in this diff does
not carry such a boolean is a property of this diff, not a guard.

**The landed `checks.md` must contain exactly these five things** for both content-bound ACKs to
transfer (@security's rule, applied to their own late amendment too): (1) the line-9 preamble
amendment; (2) the *Rust — MC key custody (items 11-13)* subsection with its entitlement table,
boundary-crossing framing, judge-the-value clause, SAFE block and no word list; (3) item (i) with
the **narrowed** opaque-relay wording; (4) item (ii)'s fixture poles; (5) the item-13 carve-out and
its pole pair. If the landed text differs, the ACKs do not transfer and it becomes a review finding.

**Coupling to note**: the item-13 carve-out's premise is that `MeetingKek` really is backed by a
redacting wrapper. It is — `SecretBox<[u8; 32]>` compiles and its derived `Debug` renders
`MeetingKek(SecretBox<[u8; 32]>([REDACTED]))`, verified before committing to it. Had it not
compiled and forced a raw `[u8; 32]` with a hand-rolled `Debug`, the negative pole would be wrong
and item 13 *should* fire on `MeetingKek`. The check text and the newtype are right together.

I agree with @security's two exclusions: **no vocabulary word list in the check** (§11 forecloses
it), and the **no-end-to-end/zero-trust-boolean rule does not go in Credential Leak** — it is an
overclaim predicate, not an exfiltration one. If it is to be mechanised it needs its own named
check, which is not this task.

### Vocabulary floor (`pii_vocabulary.rs` + `label-taxonomy.md`) — @observability item 5

Add `meeting_kek` and `transmit_key` to `PII_TOKENS_CATEGORY_A`, plus the matching
`docs/observability/label-taxonomy.md` §PII/Secret Denylist rows, landed together per the taxonomy's
own Extension policy.

**Partition classification: `NON_CREDENTIAL_TOKENS`, NOT `CREDENTIAL_TOKENS`.** @security exercised
their co-sign on this and they are right. `CREDENTIAL_TOKENS`' own doc asks *"is this field name a
secret whose holder should stop holding it?"* — for the meeting KEK the answer is **no**: ADR-0036
§4 makes the client an **entitled long-lived holder** that caches the KEK for the meeting and must
*retain the previous KEK across a rotation window*. `CREDENTIAL_TOKENS` feeds
`ts_retained_credentials` via `is_credential_field`, whose `matches_subset` is a contiguous
segment-window match, so `meeting_kek` → `[meeting, kek]` would fire Gate 1 (Retention) on
`meetingKek` / `currentMeetingKekBytes` the moment client tasks 15/19/20 land an SDK session field —
a guard failing on behaviour the ADR *mandates*, resolved either by an allowlist entry (which
silently kills the Rust-side coverage too) or by rename-to-evade. `NON_CREDENTIAL_TOKENS` is
documented as existing for exactly this case and is empty today; these are its first legitimate
inhabitants and `partition_is_total` stays satisfied. Entry comments name the **question**, not just
the term. @security's ACK is scoped to this placement and does not cover a `CREDENTIAL_TOKENS` one.

**Bare `kek` excluded — @security's stronger reason, not the inertness one.** Inertness is true for
the three word-boundary consumers (`\b(alternation)\b` with `_` as a word character means `\bkek\b`
matches neither `meeting_kek` nor `kek_generation`). But `metric_labels` does **not** use word
boundaries: its single-word path (`metric_labels.rs:543-551`) splits the label on `_` and tests set
membership, so `kek_generation` → `{kek, generation}` → contains `kek` → **Category A hit**. Bare
`kek` would therefore *false-positive on a plausible `kek_generation` label* — a spelling the
checks.md SAFE list names as metadata-not-material. (@observability's correction to the phrasing:
§11 mandates `key_custody`, not `kek_generation`, and no such label exists today.) Inert where you want it,
false-positive where you don't. The comment names the matcher family, per the module's own
`accessToken` precedent that a redundancy claim not naming a matcher family is not a claim.

**OBS-1 — `sender_id` into `PII_TOKENS_CATEGORY_B`** (not A: it is not a secret, and
miscategorising it would drag it into the partition and the TS retained-credential consumer for no
reason), with the matching `label-taxonomy.md` §Category B row. @security independently confirmed
the premise: `sender_id` matches nothing in the vocabulary today — it splits to `{sender, id}`,
CATEGORY_B has no bare `id` entry, and no multi-word entry is a substring of it. The coverage the
deleted proto tag-2 `user_id` provided incidentally is genuinely unmitigated right now. Four parts:

*Part 1* — the entry itself, in both homes.

*Part 2 — the load-bearing comment, without which the entry looks like a mistake my reviewer's own
doc warns against.* `label-taxonomy.md:264-268` block-quotes, of this exact mechanism: "Do not reach
for the vocabulary guard to enforce this. Adding `meeting_id` to the Category B denylist is inert
against the realistic spelling…" The entry comment must explain why that does not foreclose this,
or a future reader resolves the apparent contradiction by deleting the entry. **R1's argument turns
on *realistic*.** For `meeting_id` the realistic spelling IS the hashed one — the SDK hashes meeting
ids and the grandfathered ADR-0028 join metrics already emit `meeting_id_hash` — so a plain
`meeting_id` entry is defeated on arrival. For `sender_id` the realistic spelling is the **plain**
one: nothing in the tree hashes a sender id, no `sender_id_hash` exists, and the accidental form is
`"sender_id" => id.to_string()` in a label position. The entry bites the spelling that will actually
be written; the exempted spelling is the hypothetical one. **Inverse of the `meeting_id` case, not
an instance of it.**

*Part 3 — the §Enforcement reality line is a TRIGGER, not an inventory entry* (@observability's
amendment, adopting @security's catch). Part 2's reasoning rests on a fact about the tree *today* —
nothing hashes a sender id — and that is exactly the kind of premise that quietly stops being true.
If someone later introduces a `sender_id_hash`, the CATEGORY_B entry silently becomes the inert case
R1 warns about and the justification that made it correct becomes false with nothing firing. So the
line names the condition that voids the reasoning, giving the note something to *do*:

> `sender_id_hash` is exempted by `is_hashed_label()` and is **not** covered by the CATEGORY_B
> `sender_id` entry. The entry is sound today only because no `sender_id_hash` exists and the
> realistic spelling is the plain one. **If a `sender_id_hash` is ever proposed, that premise is
> void** — the entry becomes inert in the R1 sense, and the hashed-exemption carve-out (tracked
> under §Observability Debt) must land before the hashed spelling ships.

This is the weakest form of ADR-0036's "prefer structural impossibility over a control that has to
notice": structural needs the carve-out, which is deliberately deferred, so the fallback is to make
the premise's expiry explicit rather than leave the next reader to re-derive it. @observability
writes the same trigger into the debt entry, so both artifacts name one condition rather than two.

*Part 2 citations, not assertions*: `meeting_id_hash` is genuinely emitted today by the SDK at
`packages/sdk-core/src/session/MeetingSession.ts:279` and `packages/sdk-core/src/media/events.ts:44`
— that is what makes the hashed spelling the *realistic* one for `meeting_id` and a plain CATEGORY_B
entry inert on arrival. A cited fact survives a skeptical reader; an asserted one invites the
deletion the comment exists to prevent.

*Confirmed non-hazard*: unlike `meeting_kek`, `sender_id` in CATEGORY_B creates no entitled-holder
false positive and no `partition_is_total` build break — CATEGORY_B feeds `rust_pii`, `ts_pii` and
`metric_labels` only, is not read by `ts_retained_credentials`, and carries no partition obligation.
The hashed exemption above is the only subtlety.

*Part 4 — coverage stated by matcher shape*, never as blanket protection: `metric_labels` covers it
*including compounds* (`token_hit_in_set`'s multi-word substring pass catches `pinned_sender_id`) —
the "never a metric label" bar; `rust_pii` covers `sender_id` and the `sender_id = %x` tracing-field
shape but **not** `pinned_sender_id`, because it builds `\b(alternation)\b` and `_` is a word
character; **`sender_id_hash` is not covered at all**; and **span attributes via `#[instrument]`
params are not covered**, because `instrument_skip_all` reads CATEGORY_A only — so the span bar
stays `skip_all` discipline and reviewer-enforced.

*Explicitly out of scope, and I am not filing a TODO for it* (@observability is filing it themselves
under §Observability Debt): carving `sender_id` out of `is_hashed_label`. @security argued the
hashed-suffix exemption is wrong for a 16-bit domain — 65535 preimages makes the hash a reversible
pseudonym, and §11 bars the dimension rather than the spelling — and @observability agrees on the
substance while declining it here, because it introduces a new exemption-denylist concept into a
primitive `metric_labels` reads for every Category B term and changes a shared contract in a
co-owned file. Nothing in this diff creates a `sender_id_hash`.

**Named gap, recorded rather than left silent** (@security): under word-boundary matching
`\btransmit_key\b` does **not** match `wrapped_transmit_key` — a leading `_` is a word character, so
there is no boundary. `metric_labels` covers it via its substring path; `rust_log_secrets` and
`instrument_skip_all` do not. MC never holds a wrapped transmit key, so no entry is added in this
task, but the comment names the limit so the next author does not assume coverage.

**Both homes, every term** (@dry-reviewer): every addition lands in `pii_vocabulary.rs` *and* in
`label-taxonomy.md`. The two are an already-drifted mirror pair — the doc's §Category A table is
missing five entries (`pwd`, `cred`, `bearer`, `auth_code`, `accessToken`) that never followed the
Wave-2 and task-11 additions. That drift is pre-existing, @dry-reviewer is filing it plus the
missing Rust↔markdown drift guard in `docs/TODO.md §Cross-Service Duplication`, and I am **not**
backfilling the five — but I will not make it six.

**`NON_CREDENTIAL_TOKENS` is inert at runtime, and the comment must say so** (@observability): it
has no consumer but `partition_is_total_over_category_a` — hence its `expect(dead_code)` — the terms
remain full CATEGORY_A members, and `rust_log_secrets` / `instrument_skip_all` / `metric_labels` all
still read them. Without that sentence, "we filed the KEK under non-credential" reads alarming and
invites a reclassification that would re-break it.

**`checks.md` must NOT restate the tokens** (@dry-reviewer): that file says so about itself at line
72. The task asks me to extend the check's **scope**, so the text describes scope and intent and
references the vocabulary by path. Naming `meeting_kek` / `transmit_key` once in prose as an
illustration is fine; enumerating them as a list a reviewer is meant to match against is not — the
test is whether a future reader would treat the text as a checklist to keep in sync.

**INFRA-1 (must fix) — two prose sites in `pii_vocabulary.rs` go FALSE the moment the entries land**,
and neither was in my plan. (1) The `NON_CREDENTIAL_TOKENS` doc comment: *"Empty today — every
CATEGORY_A term classifies into one of the other two buckets."* (2) The `expect` reason string:
*"deliberately-empty third bucket of a total partition…"*. Both get rewritten to "first inhabitants"
framing. This is exactly the rot the module's own `CORRECTED 2026-07-30` note exists to record, so
leaving it is worse here than in an ordinary file. **Do NOT remove the
`#[cfg_attr(not(test), expect(dead_code, …))]` attribute** — it is still fulfilled after the change
(there is still no non-test reader: `partition_is_total_over_category_a` is `cfg(test)`, and
`ts_retained_credentials.rs:100` imports only `CATEGORY_A_ALLOWLIST`, `CREDENTIAL_TOKENS`,
`SESSION_TOKENS`, `STEM_EXPANSIONS`), so removing it reds the non-test build. **Only the reason
string changes.**

**INFRA-2 — the unreachability is by TWO independent mechanisms, and the second is the landmine.**
My plan said `NON_CREDENTIAL_TOKENS` "has no consumer but `partition_is_total_over_category_a`".
True but weak. A member of that bucket is unreachable from `ts_retained_credentials` because (a) it
is never passed as a `subset` — `:267` and `:271` pass only `CREDENTIAL_TOKENS` / `SESSION_TOKENS`;
**and** (b) `spellings()` (`:230-240`) iterates
`CREDENTIAL_TOKENS.iter().chain(SESSION_TOKENS.iter())` and returns `Vec::new()` for anything else.
(b) is why this earns a sentence in the bucket doc: a future author who wires
`NON_CREDENTIAL_TOKENS` into a `matches_subset` call site expecting coverage gets **silence, not
coverage** — `spellings()` returns empty, the loop body never executes, and the guard compiles, runs
and matches nothing. That is the file's signature failure shape: an entry that reads as coverage and
provides none. The doc will say members are unreachable from the segment consumer by construction,
and that wiring the bucket in without also extending `spellings()` yields silent non-detection.

The comment also says plainly that **the whole addition is a floor, not the control** — ADR-0036
§11: "Vocabulary additions cannot be cited as the protection." The `sender_id_hash` caveat
(`HASHED_SUFFIXES` / `is_hashed_label()` exempts it by construction) is stated rather than left to be
discovered. I will run `dt-guard rust-log-secrets` / `instrument-skip-all` / `metric-labels` after
the addition and report any new hits; the most likely false positive is
`crates/proto-gen/src/lib.rs`'s `.field("meeting_kek", &RedactedLen(...))`.

### Test-fixture homes (@dry-reviewer #2)

The 32-byte key needed by 11 `JoinRequest` construction sites gets **two** homes, not one and not
eleven. MC's tests use `crates/mc-test-utils/src/media.rs::sample_identity_public_key()`. env-tests'
4 sites use a new `crates/env-tests/src/fixtures/media.rs` carrying an `ANCHOR (DRY)` naming that
helper as the source. **@infrastructure verified the layering argument at source rather than taking
it** — `crates/env-tests/Cargo.toml` links exactly one local crate (`proto-gen`, dev-dependencies)
and zero service crates, so an `mc-test-utils` dev-dep would be the first service crate ever linked
there. Both rows confirmed. Their style note, taken: `fixtures/mod.rs` gets a bare `pub mod media;`
and **no `pub use` re-export** — the existing re-export block is for client *types* (`AuthClient`,
`GcClient`, `PrometheusClient`), and a bare const helper does not need one. env-tests deliberately does **not** dev-dep on `mc-test-utils`: that crate
depends on `mc-service`, so the dep would link the entire service into the build graph of a suite
whose whole purpose is black-box validation against deployed artifacts — a layering inversion, not
just weight. env-tests links exactly one local crate today (`proto-gen`) and zero service crates,
and that thinness is the ADR-0028 division of responsibility; precedent for env-tests deriving its
own value is `fixtures/auth_client.rs::resolve_org_subdomain()`. @dry-reviewer records the residual
two-home encoding in `docs/TODO.md §Cross-Service Duplication` under the ADR-0019 DRY exception, so
it does not enter my fix-or-defer flow.

### Operations artifacts

- **OPS-2** — `mc-incident-response.md` Scenario 8 "Common Root Causes" gains two rows (not a
  numbered scenario; 15/16 stay reserved for task 21). The exhaustion row states the unattractive
  truth: **the only remediation is to end and restart the meeting**, because the KEK-epoch reset
  that would reclaim the namespace is deferred with all KEK rotation. It also states the exposure
  correctly: 65535 is **cumulative lifetime admissions per meeting, not concurrent**, so
  `MC_MAX_PARTICIPANTS` does not bound it; a long-lived high-churn meeting is the exposure.
- **OPS-3** — `mc-deployment.md` §Pre-Deployment Checklist → Coordination: the existing wire-lockstep
  item is extended to say the split-deploy failure is now also a **loud join refusal**, and the safe
  order is client-capability-first / MC-requires-second (an old MC ignores an unknown field; a new
  MC rejects an old client). Browser SDK ships per page load, so in-flight tabs on an old bundle are
  the realistic exposure.
- **OPS-3, branch window, stated here and in the runbook**: from this task until client tasks
  15/19/20 land, **deploying this branch to a devloop cluster is a total join outage**, not a
  degraded media path. Stated, not solved. **@infrastructure's addition**: the note must say
  explicitly that a devloop cluster on this branch will **fail Layer 7's join-flow env-tests by
  design** during the window, so an operator hitting it recognises it as expected rather than
  filing an infra incident.
- **OPS-9 — the branch-window note carries an explicit removal trigger**, because a permanent
  runbook line encoding a branch-lifetime condition becomes false the moment tasks 19/20 land and
  misleading after merge. It follows the in-tree idiom of the `proto/buf.yaml` carve-out entry in
  `docs/TODO.md`, which states both its restore trigger and its completion signal: the note names
  the retiring condition (client tasks 15/19/20 landed, i.e. the SDK sends `identity_public_key`)
  and says plainly that the note is to be **deleted** at that point. The durable half is a dated
  `docs/TODO.md §Media Path Obligations` entry pointing at the runbook line, so the trigger lives
  somewhere that gets re-read.
- **OPS-4 — rollback is redeploy-only.** No migration, no persisted state, no data change, MTTR one
  rollout; the KEK is in-memory and the allocator is per-actor. **No new config knob is introduced**,
  so no ConfigMap key, no startup validation and no §Configuration Reference row are owed. (If
  @team-lead rules the `supported_codecs` cap in, that changes and I will land all four artifacts.)
- **OPS-6 — KEK loss consequence** is written at the point of generation, as above.

### Answers to the two direct questions

- **@observability, KEK representation**: `SecretBox<[u8; 32]>` inside a `MeetingKek` newtype with a
  **derived** `Debug`. Item 3 is structurally prevented, not reviewer-enforced.
- **@operations OPS-6, does a WebTransport reconnect go through the join path?** Today, **yes — and
  only that**. `MeetingActorHandle::connection_reconnect` has **no caller outside
  `actors/meeting.rs` and `actors/messages.rs`** (verified by grep); the ADR-0023 reconnect path is
  implemented at the actor level but is not yet wired to the WebTransport accept path, so every
  live connection goes through `handle_join` and receives a `JoinResponse`. Consequences: (a) KEK
  delivery and reconnect continuity are **the same path** today; (b) a browser "reconnect" is
  currently a fresh join with a fresh `participant_id` and therefore a **new** `sender_id` — which
  is non-recycling and correct, just not continuity; (c) continuity is exercised at the actor level,
  which is where test 4 drives it. Because the KEK never rotates in this story, the §4 "reconnect
  re-issues the current KEK" rule is satisfied vacuously — the client already holds the only KEK
  there is. When rotation lands, `ReconnectResult` must carry the KEK; filed in `docs/TODO.md` so
  it is an obligation rather than an assumption.

### Decisions ruled at Gate 1

**1. Empty `identity_public_key` — REJECT at Gate 1; REVERSED during implementation; RE-RULED
accept-absent by @team-lead at Gate 3.**

> **⚠ THIS RULING WAS OVERTURNED. The shipped behaviour is accept-absent.** The Gate-1 text is kept
> verbatim below because the reversal is the useful part of the record — an overturned ruling and a
> quietly-unfollowed one look identical six months out, and only one of them is acceptable. See
> §Ruling 1, reversed and re-ruled immediately after it.

*Gate-1 text, superseded:* **(@team-lead ruling; @security's binding position after
their retraction agrees).** Empty is not of exact length, so it is malformed: same
`IdentityKeyInvalid` variant, same byte-identical generic message, same `identity_key_invalid`
label. No accept-empty branch and no separate wording — the four-case byte-identical assertion
covers it unchanged. `ParticipantInfo.identity_public_key` is a non-`Option` `IdentityPublicKey`, so
after this task the join path cannot produce a keyless roster entry at all. The proto's "Empty means
NO KEY PUBLISHED" is a rule for *consumers* reading a roster, not a licence for MC to admit a
keyless participant. @security briefly proposed an `Option` accept-absent design and then retracted
it; the joins-admitted-without-identity-key counter that rode with it is dropped, because after this
ruling there is no such state to observe. The deployment-ordering cost is handled where it belongs —
`docs/runbooks/mc-deployment.md` §Coordination — rather than by loosening validation. OPS-3 taken in
full.

**Ruling 1, reversed and re-ruled (Gate 3, 2026-09-02).** The shipped code implements
**accept-absent**: `parse_join_field` returns `Ok(None)` for length 0, the field is
`Option<IdentityPublicKey>` through the chain, a keyless participant is admitted, and
`mc_join_identity_key_presence_total{presence}` counts the population.

*What happened, stated plainly.* The reversal was made **during implementation** and recorded in
`docs/TODO.md`, in the `parse_join_field` doc comment and on the dashboard — but **not** in this
document, which carried the ruling. So for the length of the loop the tree described two
contradictory designs and the ruling record described the one that had not shipped. That is a
process defect independent of which direction is right, and it is recorded in §Lessons Learned.

*Why accept-absent is the ruled design.* Six seats converged. `signaling.proto` defines empty as NO
KEY PUBLISHED and instructs consumers to *"fail closed and MUST NOT fall back to accepting unsigned
frames"* — a clause that **presupposes a keyless participant can reach a roster**, so reject-empty
makes MC's own already-landed contract dead prose (@protocol; proto is the wire source of truth).
Proto3 bare `bytes` cannot distinguish absent from empty, and `sender_id` one field over *is*
`optional uint32` — the author reached for presence semantics one field over and deliberately did
not here. On the security math, reject buys nothing against an adversary: validation is length-only
with no proof of possession and no `cnf` binding this story, so a hostile client is admitted anyway
with 32 random bytes it holds no private half for. Reject excludes exactly one population — honest
clients that have not implemented the field.

*What the reject did buy, and where it went.* Reject made "a keyless roster entry" structurally
unreachable, so no consumer could fail open on one. That protection is **transferred, not dropped**,
to the `docs/TODO.md` §Media Path Obligations entry titled *"The SDK MUST fail closed on a roster
entry with no identity key — DROP frames, never SKIP verification"* — owned (client), triggered
(story task 19, before the SDK verifies any frame), with the required empty-branch DROP test named.
That entry is now the **entire** replacement control and says so in its own text, per @security's
Condition 2. **The `mc_join_identity_key_presence_total` counter is NOT dropped** — the Gate-1 text
above says it is, and that is superseded: it exists, it is load-bearing as the only MC-side measure
of the keyless population, and the TODO entry cites it.

**2. `supported_codecs` over-cap sibling — file it, do not take it (@team-lead ruling).** A
`docs/TODO.md §Media Path Obligations` entry naming the field, `signaling.proto:130-138`, the
misassignment of enforcement to task 10, and that the remedy needs a *configured* cap — following
the `supported_header_versions` precedent at the frame-header-version-floor TODO entry exactly. The proto-comment repoint is named
as **protocol-owned** inside the entry; `proto/**` stays untouched. Pointer bullet under
§Accepted Deferrals. **The diff must NOT grow `config.rs` / ConfigMap / runbook rows for it**
(@code-reviewer's scope-drift condition).

**3. `test-seams` feature — ACCEPTED by @code-reviewer under @team-lead's delegation.** The
narrower alternatives were checked first and neither reaches test 5's assertion. A `#[cfg(test)]`
seam is invisible to integration tests, because `tests/` compile against `mc-service` as an
**external** crate; reaching the end-to-end reject from a `cfg(test)` seam would degrade test 5 from
an admission-level reject (`handle_join` → `JoinResponse` →
`mc_session_join_failures_total{error_type}`) to bare allocator arithmetic, or force a duplicate
harness. `record_session_join(...)` is called from `webtransport/connection.rs`, **not** from the
meeting actor, so an actor-level test cannot observe the label @observability and @operations both
required be pinned. Lowering the ceiling via production config would contaminate production topology
with a test-only bound — strictly worse. Three conditions, all verified at Gate 2:
  (a) every seam item — `SenderIdAllocator::resuming_from` and the pre-seeded-allocator
      `MeetingActor::spawn` variant — is `#[cfg(feature = "test-seams")]`, and a plain
      `cargo build -p mc-service` compiles none of it;
  (b) the seam is **exactly** those two items — no ceiling override, no other injectables, and
      **no KEK seam of any kind** (@security A.2, @team-lead non-negotiable);
  (c) the feature is non-default and enabled only via the self dev-dependency.
  **Plus OPS-10**: `#[cfg(all(feature = "test-seams", not(debug_assertions)))] compile_error!(...)`,
  so a release build with the seam on **fails to compile**. ADR-0036 §11 rules on exactly this shape
  for the dev-only per-frame tracing feature — "a control that has to notice fails silently for
  anyone building outside the pipeline; a compile error has nothing to notice". Verified free:
  `scripts/lang/rust/test.sh:144` runs a plain `cargo test` with no `--release`, and the only
  `--release` builds in the pipeline are `dt-guard` / `dt-story`, neither of which is `mc-service`.

**4. `mc_meeting_kek_generated_total{key_custody="operator"}` and the guard-vocabulary rows — both
kept (@team-lead ruling).** Metric with all three artifacts; `pii_vocabulary.rs` +
`label-taxonomy.md` stay in this diff with **hunk-level** `Approved-Cross-Boundary` ACKs from
@observability and @security (a generic plan confirmation does not satisfy this).

**5. OPS-11 — RULED, then NARROWED by the owner: trailer set is `observability` + `security`.**
@team-lead first ruled three owners and spawned @infrastructure. @infrastructure then examined the
basis and **ruled themselves off the trailer set**, which is the outcome recorded here. No
`Approved-Cross-Boundary: infrastructure` on the commit — having ruled they are not a required
confirmer, that breadcrumb would say the opposite of what happened. The file is **unfrozen**.

The reasoning, kept because it is the reusable part and because it corrects something both
@operations and I had taken at face value: **ADR-0034 lines 263-294 do not establish anyone's
standing ownership.** They are a dated Wave/Day construction schedule under `## Implementation
Status` with every row `❌ Pending` — a build assignment, not an ownership registry, and reading it
as one is a category error. It does not establish the counter-claim either: line 285's Owner column
reads `infrastructure` with observability/security in a `(pair: …)` parenthetical, and line 270
shows the table *moves* the Owner column when ownership genuinely differs. Read as the schedule it
is, it says nothing about standing ownership for anyone. The Lead's devloop ruling stands on its own
authority; the ADR does not corroborate it.

**The line that actually decides it is in the file, not an ADR** — and it corrects my own
"vocabulary contents only" framing, which @infrastructure judged an understatement.
`pii_vocabulary.rs` is not a data file: `partition_is_total_over_category_a` makes a CATEGORY_A
addition build-breaking, `NON_CREDENTIAL_TOKENS` carries a compile-configuration attribute, and the
module's own rule is that reachability be stated by matcher shape across four matcher
implementations. **But this hunk *satisfies* those constructs rather than *changing* them**, and
§Vocabulary floor already carries the machinery analysis at full depth with line citations. That
distinction — *changes* a machinery construct versus merely *satisfies* one — is what
@infrastructure is encoding in CLAUDE.md so the next dt-guard change does not re-derive it in either
direction. A diff editing `matches_subset`, `segments()`, `is_hashed_label`, or the partition's
shape **is** a machinery change and infrastructure is a required reviewer on it.

Two other the dt-guard-ownership TODO entry points are unchanged and still binding: dt-guard is **not** a GSA (no
ADR-0024 §6.4 criterion), so this is §6.3 Domain-judgment and not a §6.4 intersection; and the
`cross-boundary-ownership.yaml` "fix" stays **forbidden** — it is a GSA mirror, `gsa_sync.rs:196-205`
rejects stray keys, and populating the five mirrors would wrongly declare dt-guard Guarded. Not
touched.

**Two artifacts land verbatim as @infrastructure wrote them** — I place, I do not paraphrase:
- **CLAUDE.md**: a one-paragraph *Guard-crate ownership (interim)* note, inserted after the
  `operations` table row (`CLAUDE.md:54`) and before the `Definitions:` line, splitting
  `crates/dt-guard/**` by what the edit changes rather than by path.
- **`docs/TODO.md`'s **"`crates/dt-guard/**` has no owner derivable from the path"** entry**: full replacement, entry **stays open** (`- [ ]` unticked), recording the
  interim landing, the ADR-0034 self-correction by the seat that wrote the original wording, and
  what remains task-sized — `cross_boundary_scope.rs` reads owners from `main.md` tables, not from
  any manifest, so there is still no general non-GSA path→specialist map, and the CLAUDE.md note is
  prose a human reads rather than something a guard enforces.

Both texts are held until "Plan approved" — the Lead's option-(a) authorisation covers
`checks.md` only.

---

## Pre-Work

None — task 3 (protocol reshape of `signaling.proto`) is already landed and committed; this devloop consumes the existing generated types and adds no proto edits.

---

## Implementation Summary

**New module `crates/mc-service/src/media_admission/`** — `kek.rs`, `identity_key.rs`, `sender_id.rs`,
with the ADR-0036 §4 security floor stated once in `mod.rs`.

- **`MeetingKek(SecretBox<[u8; 32]>)`**, `ring::rand::SystemRandom`, derived `Debug` (redaction and
  zeroize come from the type, so item 3 of the credential-leak check is structurally satisfied).
  Generated in `MeetingActor::spawn`, which is now fallible and **fails closed** — no fallback RNG,
  no default key, no meeting. `MEETING_KEK_BYTES` is drift-guarded against
  `ring::aead::AES_256_GCM.key_len()` by a `#[test]` (`key_len()` is not `const`).
- **`IdentityPublicKey`** — three-state parse at the WebTransport trust boundary, before any actor
  interaction: length 0 → `Ok(None)` (the contract's NO KEY PUBLISHED state, **admitted**), 32 →
  `Ok(Some)`, anything else → `Err`. One error value, one client message, one metric label across
  **every rejected length** — absent is not among them; it is counted on
  `mc_join_identity_key_presence_total{presence="absent"}`. "Well-formed" appears in no function
  name or doc comment: this is a length check on an opaque 32-byte blob and says so.
- **`SenderId(NonZeroU16)` + `SenderIdAllocator`** — the *type* is the bound, tied to
  `media_protocol::frame::KEY_ID_SENDER_ID_BITS` by a `const _` assert, so no local `16`/`65535`
  literal exists. `Option<NonZeroU16>` cursor; `checked_add` is the wall and the wall is a reject.
  No `u16::try_from`, no `as`, no `wrapping_*`, no `%`.

**Wiring**: typed identity key threads `JoinConnection` → `ConnectionJoin` → `handle_join`
(parse-don't-validate). `JoinResult` gains `sender_id`, `Arc<MeetingKek>`, `kek_generation`.
`build_join_response` and `handler.rs::encode_participant_update` populate both roster fields, so the
key-id → `sender_id` → roster → key chain resolves for the fan-out as well as the join response.

**Errors**: `IdentityKeyInvalid` (code 1, `identity_key_invalid`) and `SenderIdSpaceExhausted`
(code 7, `sender_id_space_exhausted`). Client strings are generic and the exhaustion one is
byte-identical to `MeetingCapacityExceeded`'s; only the Prometheus label separates them.

**Telemetry**: `mc_meeting_kek_generated_total{key_custody="operator"}` with catalog row, dashboard
panel and test reference. No end-to-end or zero-trust boolean anywhere. OPS-8's reject `warn!` is
emitted inside `handle_join` so it inherits the `meeting_id` span field.

**Verified negatives** (evidence for @security B.7 / @test Q3):
- `git diff --name-only` matches no `proto/**`, `proto-gen/**`, `media-protocol/**`, `common/**`.
- No `MeetingKek` reference in `grpc/` or `redis/`; no `Serialize`; exactly **one** production
  `expose()` call site — `build_join_response` in `webtransport/connection.rs`, the join response.
- OPS-10 demonstrated: `--release --features test-seams` **fails to compile**; `--release` alone
  succeeds.

**Tests**: 392 passing in mc-service. Six required integration tests plus the actor-level reconnect
continuity test and 13 module unit tests.

---

## Files Modified

30 modified + 5 new (see the Cross-Boundary Classification table). Two rows —
`actors/participant.rs` and `tests/disconnect_latency_integration.rs` — were **added after the fact**,
surfaced by `dt-guard cross-boundary-scope` rather than by me. Both are mechanical fallout from the
`ParticipantInfo` and `MeetingActor::spawn` signature changes. Recording that the guard caught them,
not the author.

---

## Devloop Verification Steps

| Check | Result |
|---|---|
| `cargo test -p mc-service` | 392 passed, 0 failed |
| `cargo clippy -p mc-service -p mc-test-utils -p env-tests --all-targets` | clean |
| `cargo build --release` (no seam) | succeeds |
| `cargo build --release --features test-seams` | **fails to compile** (OPS-10, as designed) |
| `dt-guard` sweep (17 subcommands) | all OK |
| `metric-coverage` | caught the uncovered KEK metric before I did; fixed, now `all-covered` |
| `cross-boundary-scope` | caught 2 unlisted files; rows added, now `no-drift` |
| `knowledge-index` | caught a size violation; condensed, now clean |

@team-lead runs `./scripts/layer-all.sh` as the gate — the above is iteration, not a substitute.

---

## Code Review Results

### Gate 3 verdicts (iteration 3, 2026-09-02)

| Reviewer | Verdict | Note |
|---|---|---|
| Security | RESOLVED-DEFERRED | All 5 findings fixed (F1-F4 + F5). Verdict class is forced by the two **accepted deferrals** — join rate limiting / `jti` replay, and the `AssignMeetingWithMh` metric before it was reversed to fix-now. No finding was declined. |
| Test | RESOLVED-FIXED | Watermark branch accepted as adequately covered (log-only, edge-trigger unit-tested); OBS-6 tracking clause fixed. |
| Observability | RESOLVED-FIXED | OBS-1..7. OBS-7 (the GC `success`/`accepted` mirror break) resolved by aligning MC to GC. |
| Code Quality | RESOLVED-FIXED | Findings 1-3. Finding 2 fixed in a different shape than specified, accepted as "strictly better". |
| DRY | RESOLVED-DEFERRED | **Nothing was deferred by me**: 6 raised, 5 fixed, **1 (D5) withdrawn by the reviewer as their own false positive**. The verdict class is forced mechanically because §Accepted Deferrals counts *DRY extraction opportunities* and three sit there. Recorded here so the class is not misread as a finding against the work. |
| Operations | RESOLVED-FIXED | OPS-A..E. `for: 0m` on the new alert accepted as in-house precedent (`MCActorPanic`), not an analogy. |
| Semantic Guard | CLEAR | — |
| Protocol | RESOLVED-DEFERRED | Active findings fixed in-diff; two protocol-owned proto-comment repoints deferred structurally by the zero-proto-edit line, both TODO-tracked. |
| Infrastructure | RESOLVED (F-INFRA-1..4, 6) | F-INFRA-5 (`CLAUDE.md`) **escalated, not deferred** — see below. |

### Two attribution corrections, recorded because the reviewers asked for them

- **@security F3** — raised by @security, **resolved by @observability's argument**, documented by
  me. @security's original message credited me with the reasoning; I corrected it unprompted and
  they have amended their verdict. The decisive point — that conditioning the presence counter on
  admission would blank the series during exactly the `sender_id_space_exhausted` incident in which
  an operator reads it — is @observability's, and neither of @security's two proposed options was as
  good. What is mine is only the catalog wording that the denominator was *chosen rather than
  inherited*.
- **F-INFRA-5** (`CLAUDE.md`'s blanket "not a Guarded Shared Area") — I declined to action this on
  @infrastructure's instruction, because a peer agent's message cannot authorise editing
  project-level configuration. @infrastructure declined for the identical reason and correctly
  refused to let authorship of the paragraph move the boundary. **@team-lead made the edit.** I
  agreed with the finding on the merits and put that on the record rather than staying silent.

### Pre-assembled evidence (Gate 2)

Evidence pre-assembled below for @security's three carried items, each in their
two-check form — confirm the control **exists**, then confirm its **scope reaches the claim**.
Gathered before Start Review rather than in response to it, so the reviewer reads output rather than
assertions.

Original text follows. Pending Gate 3. Evidence pre-assembled below for @security's three carried items, each in their
two-check form — confirm the control **exists**, then confirm its **scope reaches the claim**.
Gathered before Start Review rather than in response to it, so the reviewer reads output rather than
assertions.

**Item 1 — `test-seams` compile-time absence. CLAIM NARROWED after @security's scope challenge.**
- *Exists*: `cargo build -p mc-service --release --features test-seams` → `error: the test-seams
  feature exposes the sender-id exhaustion bypass and must never be enabled in a release build`.
- *Scope reaches*: all 10 seam items are `#[cfg(feature = "test-seams")]` (or
  `#[cfg(any(test, feature = ...))]`) across `sender_id.rs`, `meeting.rs`, `controller.rs`,
  `messages.rs` — no runtime branch anywhere. `nm` over the default-build rlib finds **0** symbols
  matching `resuming_from` / `spawn_with_sender_id_cursor` / `create_meeting_with_sender_id_cursor`.
- **Where my original claim over-reached, and it did.** @security asked what the `compile_error!` is
  *gated on*, noting the `nm` check covers the *default* build and not every non-test build. The
  predicate is `not(debug_assertions)` — a **proxy** for "test-ish build", not a direct test. The
  honest statement is: *the seam is permitted iff `debug_assertions` is on*. It rejects
  `--release` today only because `[profile.release]` does not set `debug-assertions`, so it defaults
  off. **Residual**: `[profile.release] debug-assertions = true`, or a custom profile inheriting
  release with it set — both ordinary debugging practices — satisfy neither branch and would compile
  the seam into an optimised binary. **No profile in this workspace does that** (verified:
  `debug-assertions` appears in no `Cargo.toml` or `.cargo/config.toml`), and two further conditions
  still stand — the feature is non-default and enabled only via the self dev-dependency, so reaching
  that state also requires explicitly passing `--features test-seams`.
- **Resolution — and the narrowed premise turned out to be *already guarded*, which neither of us
  knew.** @security ruled against the `build.rs` and found that
  `crates/dt-guard/src/release_build_profile.rs` (wired at
  `scripts/guards/simple/validate-release-build-profile.sh`) exists precisely to assert "no shipped
  artifact turns `debug_assertions` on", across three channels: `[profile.release]` /
  `[profile.release.package.*]`, `-C debug-assertions` in RUSTFLAGS, and
  `CARGO_PROFILE_<PROFILE>_DEBUG_ASSERTIONS` via Dockerfile `ENV`/`ARG` or CI `env:`. **The exact gap
  hypothesised is that guard's first rule**, and the other two channels close routes neither of us
  had enumerated. `lib.rs` now cites it, and carries its caveat verbatim in spirit: **it is a CI
  check and does nothing for anyone building outside the pipeline**. Final ceiling on the claim:
  *the predicate is `debug_assertions`; the premise that shipped artifacts have it off is asserted
  in-pipeline across three channels; outside the pipeline nothing asserts it.*
- **`build.rs` rejected**, on @security's ruling and their reasoning: it duplicates an already-guarded
  premise, changes the build graph mid-gate, and `build.rs` is an enumerated GSA path. The structural
  successor is named in that guard's own docs — `#[cfg(all(feature = "release-artifact",
  debug_assertions))] compile_error!` with service Dockerfiles passing `--features release-artifact`,
  observing the effective cfg rather than its causes. `lib.rs` points at that rather than inventing a
  parallel mechanism. @security's own note on declining their suggestion: it would have been the
  *"it's small, just do it"* reasoning refused twice already this gate.
- **This is the two-check practice finding a defect in my own evidence**: the control existed and I
  had verified it; its scope did not reach the claim I made for it. @security named the sub-species —
  **evidence that is real but over-generalised** — as the one that survives an honest reviewer
  checking the command was actually run.

**Items 2 and 3 — pre-checked against @security's scope questions; both hold.**
- *Item 2, prose not just names*: swept comments as well as identifiers, and the loaded terms
  @security named beyond the banned three. Every `proves` is bounded (`proves only that… never
  proves who`); `authentication-key recovery` is AES-GCM terminology, not an identity claim; the one
  `guarantee` is about metric timing. The two banned-word hits are both negations.
- *Item 3, is `expose()` the only door*: `MeetingKek`'s **entire** public surface is `generate()` and
  `expose()`. **No** `Deref`, `AsRef`, `Borrow`, `Into`, `From`, `Clone`, `Copy`, `Index` or
  `ToOwned` impl; the sole derive is `Debug`, which redacts via `SecretBox`. So the call-site count
  measures the only door. The `internal::v1` sweep was crate-wide, not directory-scoped: all 12
  references sit in `main.rs` and `grpc/`, and `media_admission/` constructs no internal message at
  all.

**Item 2 — the reconnect test name implies no shipped guarantee.**
- *Exists*: `sender_id_continuity_across_actor_level_reconnect` — named for the seam it drives.
- *Scope reaches*: `connection_reconnect` has **0** callers outside `actors/`, so the path is not
  production-reachable; the doc comment says so in those words, adds that a browser reconnect is
  today a fresh join with a **new** `sender_id`, and names the continuity key as `correlation_id` +
  HMAC `binding_token`. Zero `attested`/`verified`/`trusted` across every new test and helper name.

**Item 3 — no key material crosses the MC→MH contract.**
- *Exists*: exactly **one** production `expose()` call site in the whole crate —
  `webtransport/connection.rs`'s `build_join_response`, the join response to a participant MC just admitted.
- *Scope reaches*: **0** hits for `MeetingKek`/`meeting_kek` in `grpc/`, `redis/` or
  `mh_connection_registry.rs`; **0** of the 12 `internal::v1::` references mention key material;
  **0** serialization or persistence hits on `MeetingKek`; and `git diff --stat` over those three
  MC→MH surfaces is **empty** — this diff did not touch them at all, which is a stronger negative
  than "touched them safely".

---

---

## Accepted Deferrals

Pointers only — every body lives in `docs/TODO.md` or the cited source, which is its single home.

- Filed: "KEK rotation, the KEK-epoch `sender_id` reset, and reconnect KEK re-issue are all deferred" (§Media Path Obligations)
- Filed: "`ParticipantCapabilities.supported_codecs` over-cap rejection is assigned to a task that did not implement it"
- Filed: "The end-to-end / zero-trust boolean overclaim has no named guard or check"
- Filed: "Credential-leak items 11-13 do not reach the MH frame-forwarding path"
- Updated not closed: "dt-guard `pii_vocabulary.rs` CATEGORY_A does not contain `wrapped_key`" — `transmit_key`/`meeting_kek` landed, `wrapped_key` did not
- Updated not closed: "`crates/dt-guard/**` has no owner derivable from the path" — CLAUDE.md interim landed, general map still open
- Elsewhere: `cnf` binding + Ed25519 point/small-order validation — trigger stated at `IdentityPublicKey::try_from_bytes`, per @security
- Elsewhere: `is_hashed_label` carve-out for a future `sender_id_hash` — @observability §Observability Debt; trigger also in `label-taxonomy.md` §Enforcement reality
- Elsewhere: F6 half (b), TypeScript-side KEK sink control — written by @security, **not touched by me**; exposure window open now, not pending
- NOT debt, permanent properties: roster key gives same-keyholder consistency never verified identity; MC can read media (accepted custody, ADR-0024 §5.7), never end-to-end and never zero-trust


---

---

## Rollback Procedure

1. Start commit: `612bc379269dbbc383333592ef8cf053cdddcb50`
2. Review: `git diff 612bc379..HEAD`
3. Soft reset: `git reset --soft 612bc379`
4. Hard reset: `git reset --hard 612bc379`
5. No schema changes; no infra manifests.

---

## Issues Encountered & Resolutions

### Gate-2 catches (see §Lessons Learned 1 for the five-mechanism decomposition)

**Gate 2, iteration 1 → 2 (mechanical).** Layer 2 came back `FAIL REASON=cargo-fmt-failed` on a
single diff: `media_admission/identity_key.rs:149`, where `parse_join_field`'s signature was split
across lines but fits on one. Fixed with `cargo fmt --all` (no hand-editing around rustfmt);
`cargo fmt --all --check` and `./scripts/layer2.sh` are now clean and `cargo check -p mc-service
--all-targets` still compiles. No other file was reformatted. Layers 4 and 6 `N/A` are the
documented intentional-gap wrappers, not regressions.


**F4/F5 — the citation and the entry drifted apart, twice.** F4 reported the MH scope-extension TODO
entry as cited-and-absent; by the time I acted on it, the entry existed. **Both readings were
correct, and the timing is the interesting part**: @security verified `git status --short
docs/TODO.md` as *unmodified* with the phrase greps returning 0, so at the moment of the finding
TODO.md was byte-identical to HEAD and the entry genuinely did not exist (`git show
HEAD:docs/TODO.md` → 0 hits confirms it was added in this working tree). My docs pass landed after.
The finding stays recorded as a real instance of the cited-and-absent mechanism rather than being
written off as reviewer error — **and not writing a second entry was the correct response**, since a
duplicate is the two-divergent-entries-about-one-gap outcome. The lesson is narrower than "verify
before flagging": a working-tree finding has a timestamp, and the fix is to re-check at apply time,
which is what happened.

What *was* wrong was mine: the clause carried **two overlapping citing sentences with different
triggers** ("story task 16" vs "parses the wrapped-key field"), left behind by my own earlier
substitution — I replaced a sub-clause and left the sentence it superseded. Merged to one, keeping
the parses-the-field trigger, which is strictly tighter: task 16 could ship forwarding without ever
parsing that field, letting a task-16-bound trigger pass unmet — the F6 failure mode one level
down. F5's
`(see D4)` pointed at the meeting-**identifier** prohibition rather than key material; re-pointed to
the two entries that actually substantiate the claim, **by title rather than by line number**. Then
retitling the vocabulary entry (because `transmit_key` landed in this diff and `wrapped_key` did not)
broke the fresh citation to it — caught by a mechanical resolve-every-citation loop, not by re-reading.

### Gate-1 catches that changed the DESIGN, not the prose

**OPS-8 (@operations) — the exhaustion reject was unlocalizable.** The plan routed the reject through
the `Ok(Err(e))` arm in `webtransport/connection.rs` (~line 440), which reaches
`record_session_join` correctly. But that arm's `warn!` and its enclosing span carry
**`connection_id` only** (`connection.rs`'s connection-span `#[instrument]` — no `meeting_id`), and the metric correctly carries no
meeting identifier under §11. So at the moment the 16-bit space hit the wall, an operator would have
had a counter increment and a connection id and **no way to tell which meeting exhausted**. Leaning
on the OPS-7 watermark line to localize it does not work either: that line is the *gradual* signal
and can be arbitrarily far in the past — for the long-lived-meeting case potentially outside log
retention — so it re-couples two signals @operations deliberately kept separate. **Resolution**: emit
the reject `warn!` inside `handle_join`, which is already
`#[instrument(skip_all, fields(meeting_id = %self.meeting_id))]`. One line, no new label, no new
metric, no new artifact; the `connection.rs` arm is unchanged. Found by reading the span field lists
rather than the call graph.

**OBS-2 (@observability, then @operations) — a false runbook capability, wrong twice.** The plan
claimed `mc_meeting_kek_generated_total` as the Scenario-16 "Missing Key Material" signal. It cannot
be: it increments unconditionally once per meeting-actor creation, so it is *identically* the
meeting-creation count, and the absence-shaped inference needs a second series to divide by that MC
does not have (`mc_meetings_active` is a gauge). @operations then found the second error — Scenario
16 was never meant to have an MC-side signal at all; the story keys it on client-side
frames-dropped-by-reason counters **by design**. **Resolution**: claim struck from the plan, the
catalog and the panel, with no disclaiming reference to Scenario 16 either (a disclaimer would
reintroduce the association in the one document a responder greps). The metric stays as the honest
`key_custody` carrier and is forward-compatible once rotation lands. This is the catch most likely to
have cost someone at 3am, and it was found at Gate 1 by a reviewer checking a claim I asserted rather
than derived.

**@semantic-guard — item 13 had an inverted verdict on its own reference implementation.** The draft
check text would have fired on `MeetingKek(SecretBox<[u8; 32]>)` and on `JoinResult` — the exact two
types this design declares safe, and the one pattern in this very diff. **Resolution**: the
redacting-wrapper carve-out, plus @security's two tightenings (verify redaction *behaviour*, not a
`Secret`-shaped name; scope to `Debug`/`Display`, leaving `Serialize` under item 12) and a fixture
pole pair on that axis. A check with an inverted verdict on its reference implementation is how a
check gets ignored.

### Process issues

**Gate-1 sequencing deadlock.** @semantic-guard's owner ACK is content-bound and needed the landed
`checks.md` bytes; Gate 1 needed the ACK; the no-implementation-before-approval rule blocked the
file. Routed to @team-lead rather than resolved unilaterally — ruled option (a), authorising the
`checks.md` hunk **only**, ahead of "Plan approved". A message-pasted substitute was rejected as
precisely the substitution the content-binding exists to prevent.

**OPS-11 — dt-guard ownership, second recurrence.** `docs/TODO.md`'s **"`crates/dt-guard/**` has no owner derivable from the path"** entry already recorded that
`crates/dt-guard/**` yields no derivable owner and that this would recur. It did, on this task.
Ruled: three owners, @infrastructure spawned, and the entry's own named interim fix (a
`crates/dt-guard/**` line in CLAUDE.md's Specialists table) lands in this diff under CLAUDE.md's
"Fix, don't defer" rule. The entry is updated, **not closed**.

---

## Lessons Learned

### 1. The gate's primary finding: prose that reads as coverage and fails against the filesystem

Eight apply-failures, decomposed by @security into **five distinct mechanisms** rather than a tidier
single cause — the untidiness is the finding:

| Mechanism | Shape |
|---|---|
| Scope too narrow | The rule's stated reach exceeds what its scope line can see. |
| Inert but reads as coverage | The control exists and matches nothing that will realistically be written. |
| Cited and absent | A citation points at a control, or an entry, that does not exist. |
| Fires on the correct form | The control would flag the very spelling the design mandates. |
| Trigger fired, unmet | A condition's trigger has already occurred and nothing acted on it. |

**Several were introduced by the remediation of the previous one.** They are unified not by mechanism
but by the fact that **every one reads as coverage in prose and fails against the filesystem.**

**The operative practice is two checks, not one.** Confirm the cited control **exists**; then confirm
its **scope reaches the claim**. Passing the first and skipping the second is how item (i) shipped.

**@security's caveat, which must travel with the lesson**: item (i)'s cause was an over-broad
*exclusion*, not an under-broad *scope*. The remedies are **opposite**. A reader who generalises this
to "doesn't reach → widen the scope" would have made exactly the cross-boundary mistake we refused
twice in this devloop (see §3).

**A sixth mechanism, added at the close: evidence that is real but over-generalised.** Distinct from
cited-and-absent, and more durable, because it *survives* an honest reviewer confirming the command
was run. `cargo build --release --features test-seams` genuinely errors — but I drew from it "the
seam cannot be enabled in release", when what it showed was "the seam is rejected when
`debug_assertions` is off". A true command, a claim broader than its output. **Applying the two
checks to your own evidence, not only to the code, is what catches it.**

### The single most repeatable trap: **the mechanical check needs its own scope check**

Three of the last four instances were bugs in a *verification*, not in the thing verified. All the
same mechanism — the check ran, returned clean, and its **scope did not reach the claim being made
with it**:

- **`| head` truncation** (@security). A sweep for `debug_assertions` under `crates/*/src/` returned
  only dt-guard hits and appeared to contradict my stated predicate; the pipe truncated
  alphabetically before reaching `mc-service`.
- **Line-wrap blindness** (@security). A line-oriented `grep` for a superseded sentence returned zero
  and was about to be reported as "kept verbatim is false" — the phrase was there, split across a
  `//!` wrap. Re-run wrap-agnostically (strip `//!`, join, then match), it was present and labelled.
- **A true command, over-generalised** (mine). `cargo build --release --features test-seams` really
  does error; the claim drawn from it was broader than what it showed.

**A zero result reads identically to a clean result.** That is what makes this the most repeatable
trap in the gate, and it caught the reviewer who named the mechanism — twice, in consecutive
messages, while running a gate about exactly this. Neither of us lacked context or motivation.
Practical form: `grep` over wrapped prose and any pipeline ending in `head` are controls that
silently under-reach; before reporting a zero, confirm the check *could* have returned non-zero.

**Final count: thirteen.** The last was a malformed `cfg` I wrote *into the guard module whose
purpose is precisely-stated premises*, quoting the exact control it protects, in a form that would
give anyone copying it a syntax error. Small, in the worst possible place, and found by reading the
quote against the source rather than trusting that I had transcribed it.

**Corollary, and the claim I first made about it was itself an instance.** After those edits my own
`lib.rs` reference had drifted from 78 to 104 — the `(see line 72)` failure a third time. I replaced
the drifting ones and then asserted *"every line-number citation in main.md is now a symbol or file
reference; verified zero remain."* **False.** My sweep pattern was
`(lib|connection|meeting|sender_id|kek)\.rs:[0-9]+` — it *structurally could not match*
`docs/TODO.md:618`, `CLAUDE.md:54`, or the eight others. A true action ("I removed the ones I found")
generalised into a false universal, from a check that could not have failed. **Mechanism six, mine,
one message after naming it.**

@security caught it, and the re-run with a validated pattern found **17 occurrences, 12 distinct**.

**The useful rule is not "never cite a line" — it is: a line citation into a file THIS DIFF MUTATES
is the hazard; into an unmodified file it is stable.** Applied here, and it was not hypothetical:

- `docs/TODO.md:635` was **already broken**. It pointed at the `supported_header_versions` precedent;
  my own insertions into §Media Path Obligations pushed that entry to 643, and 635 now lands on an
  unrelated reject-reason counter. Live breakage, caused by this diff, plausible-looking, and
  reported by @security as *accurate* — because that check confirmed the line resolved to
  **something**, not that it resolved to **what main.md claims about it**. Mechanism six again, in
  the verification of mechanism six.
- All `docs/TODO.md` citations are now **entry titles**, the remedy already applied in `checks.md`.
- The remaining eight point into files this diff does not touch (`CLAUDE.md`, `metric_labels.rs`,
  `label-taxonomy.md`, `gsa_sync.rs`, the sdk-core files). **Recorded as a known residual, not
  claimed as zero** — dormant, same class, and not worth churning.

**A positive control must be constructed, not borrowed from history** (@security, after their own
near-miss: a `git show HEAD:` baseline returned 0 for a defect introduced and fixed entirely within
the working tree, so the control would have "confirmed" the fix either way). History may not contain
the defect.

*Demonstrated again while closing F5.* Fixing the vocabulary TODO entry meant retitling it — which
silently broke the `checks.md` citation pointing at that title. My own resolve-every-citation loop
caught it before handoff. **Fixing an entry and breaking the citation to it is the same defect class
one level up**, and it happened inside the fix for that defect class. Mechanical resolution beats
re-reading: a `grep -cF` over each cited title takes seconds and does not get tired.

### The Gate-3 finding: an admission property was changed by implementation and never by ruling

**The defect was not the direction; it was the route.** The Gate-1 ruling said reject-empty. The
implementation shipped accept-absent. The reasoning for the change was sound, was written down at
length, and was recorded in `docs/TODO.md`, in the `parse_join_field` doc comment and on the
dashboard. What never happened was a **ruling**. So the tree carried two contradictory designs, and
the document that carries rulings described the one that had not shipped.

**Why that is worse than an ordinary doc drift, and the shape to recognise.** Six operator-facing
artifacts followed the *un-shipped* design, and one of them — `mc-deployment.md`'s TEMPORARY item —
told an operator that Layer 7's join-flow env-tests *"fail BY DESIGN … do not file [an incident], and
do not 'fix' it by relaxing MC's validation."* Every clause was false: the validation had already
shipped relaxed, and **this loop's own Gate 2 passed Layer 7**, which is the disproof sitting inside
the same gate that let the text through. A runbook line pre-authorising a red gate is a standing
licence to wave through a real regression, in the lane where join breakage surfaces first, and it
survives merge into a file nobody re-reads. @operations then showed it was wrong under *both*
candidate rulings — under reject-empty the Rust env-tests would still have passed, because this diff
already fixed them; what would have failed is the browser E2E, a different lane. So it named the
wrong lane in one direction and a non-event in the other.

**What actually caught it**: six reviewer seats independently reading the record *against the tree*
rather than against each other. No guard caught it and none could have — `cross-boundary-scope`,
`metric-coverage` and `todo-tracking` all validate structure, and the structure was fine. The
falsehood was semantic agreement between prose and code.

**One of those gaps IS mechanisable, and it is now filed as one gap rather than as four slips.**
`todo-tracking` validates the shape of `docs/TODO.md` entries but resolves no citation *into* the
file, so prose may cite a §section and entry title that does not exist and every layer stays green.
This loop hit that shape four separate times — two citations in `label-taxonomy.md` and
`pii_vocabulary.rs` to a §Observability Debt entry that had not been written yet, `mc-deployment.md`'s
TEMPORARY item pointing at a §Media Path Obligations entry that never existed, and the F4/F5 pair in
§Issues Encountered. Four occurrences by careful authors in one diff is the signature of a missing
check, not of carelessness, which is why it is filed once against `infrastructure` with a trigger
rather than corrected four times and forgotten.

**The transferable rule.** *A ratified decision may be reversed by implementation only if the
reversal is itself ratified and recorded where the original ruling lives.* Recording the new
reasoning somewhere true — a TODO entry, a doc comment — is necessary and **not sufficient**, because
an overturned ruling and a quietly-unfollowed one are indistinguishable in an audit, and the reader
who most needs to tell them apart is the one deciding whether to "restore" the original. The
correction here deliberately keeps the Gate-1 text verbatim under an overturned banner rather than
rewriting it to read as though it always said accept.

**Second-order**: when a structural control is traded for a documented obligation, the obligation's
wording becomes load-bearing in a way it was not when written. `docs/TODO.md`'s SDK fail-closed entry
went from "one of several protections" to "the entire control" without its text changing — so it now
states its own non-negotiability, forcing a future editor to overrule it rather than merely not
notice it.

### A reversal has a fan-out, and the fan-out is never one file

**This loop made the same mistake twice, in opposite directions, and neither instance was caught by a
guard.**

1. A **Gate-1 ruling** (reject-empty) was reversed at implementation. The new reasoning was written
   down in `docs/TODO.md`, the `parse_join_field` doc comment and the dashboard — and **six**
   operator-facing artifacts were left asserting the old state, including a runbook line
   pre-authorising a red Layer 7.
2. A **filed item** (the `AssignMeetingWithMh` metric) was reversed the other way, from FILE to
   FIX-NOW, once @observability checked the premise and found GC already pages on this. The metric,
   its catalog row, its panel and its test all landed — and **one** artifact was left asserting the
   old state: Scenario 6's Check line still said *"MC has no metric on this RPC path; see
   `docs/TODO.md`"*, while the TODO entry cited that very runbook row as evidence the gap was closed.
   Two artifacts pointing at each other, each asserting the opposite.

**The durable rule is not "check the runbooks."** It is: *a reversal has a fan-out, and the fan-out
is never one file — enumerate every artifact that asserted the old state before declaring the
reversal done.* The direction of the reversal is irrelevant; tightening a decision strands stale
artifacts exactly as reliably as relaxing one, and the second instance here was a *good* reversal
that still left a responder pointed at a log grep and told the dashboard was empty.

**@security's formulation of why this recurs, which is the part worth keeping**: *the artifact that
records a decision is the least likely to be updated when the decision changes, because it reads as
history rather than as state.* `main.md` §Decisions, a runbook root cause and a TODO entry all read
as records of something settled, so a reader changing the decision does not experience them as
things that are now wrong. That is why both instances were found by a reviewer re-reading the tree
against the code and neither by `cross-boundary-scope`, `metric-coverage` or `todo-tracking` — all
three validate structure, and the structure was correct both times.

### 2. Three mechanical guards caught what a careful author missed

Not a clean sweep, and the difference matters:

- `metric-coverage` — `mc_meeting_kek_generated_total` had no test reference. I had not noticed.
- `cross-boundary-scope` — two files I had edited (`actors/participant.rs`,
  `tests/disconnect_latency_integration.rs`) were absent from the classification table.
- `knowledge-index` — INDEX.md exceeded its size limit.

Three independent controls doing exactly what they exist for, **on a diff written by someone trying
hard and paying attention**. The value of a mechanical control is not that it catches careless work;
it is that it catches careful work, which is the only kind that gets this far. A clean sweep would
have been a weaker result than this, because it would not have demonstrated the controls firing.

Same lesson from the other side: I wrote a `(see line 72)` cross-reference into `checks.md` that my
own 43-line insertion invalidated — a stale positional reference **inside the paragraph about single
sources of truth**. Cite by stable anchor, never by line number. Three separate citation defects in
one devloop, all the same shape.

### 3. "It's small" is not a reason to cross an ownership boundary

@security proposed extending the credential-leak check's scope into `crates/mh-service/**` and the
`crates/media-protocol/**` GSA to close a real, correctly-diagnosed gap. It was **one line**, it
**closed a genuine hole**, and it would have **unblocked the ACK immediately**. The fact that neither
`media-protocol`'s owners (protocol + media-handler) nor `mh-service`'s were reviewers on this
devloop is the whole objection, and it does not get smaller as the diff gets smaller.

It went to the owner (@semantic-guard) with the reviewer-set fact attached, rather than being
decided. Both owners then converged on stating the limit honestly plus a trigger-bound TODO — a
better outcome than the edit, because it is the honest one.

> **@security, verbatim:** *"'it's small' is the same sunk-cost framing the review protocol names as
> an anti-pattern, just aimed at a scope line."*

The same reasoning was applied twice more: declining `supported_codecs` (needs a config knob and its
ops artifacts — @team-lead's call, not a specialist's), and declining F6's TypeScript-side control
(wrong crate, wrong owner). A specialist's scope is not defended by refusing large things; it is
defended by refusing small convenient ones.

### 4. A control's cited basis must be checked against the tree, not read

The replacement wording for the scope clause cited a directory-scoped macro deny as what holds the
MH-side telemetry gap. That guard **does not exist**: no `crates/mh-service/src/media/` directory, no
`disallowed-macros` key anywhere in the repo, no `deny`/`forbid` in `mh-service`, and `docs/TODO.md`
D4 is an *open item about its absence*. Caught by @security; verified independently against the
filesystem before any owner's ruling was touched, because overriding an owner on an assertion is
itself the anti-pattern.

---

