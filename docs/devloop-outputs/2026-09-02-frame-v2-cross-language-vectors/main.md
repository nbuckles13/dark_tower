# Devloop Output: Cross-language frame v2 test vectors, non-production Rust reference generator, external sframe-wg gate, drift guard, ADR-0027/0036 amendments

**Date**: 2026-09-02
**Task**: Story task #8 — `proto/test-vectors/frame-v2.vectors.json` as SSoT for the v2 frame format across the Rust and TypeScript codecs; protocol-owned NON-PRODUCTION Rust reference generator; external sframe-wg vector gate; ADR-0033-wired drift guard; ADR-0027 + ADR-0036 amendment-table edits.
**Specialist**: protocol (paired with client)
**Mode**: Agent Teams (v2), full, headless (`DEVLOOP_HEADLESS=1`)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1fbd87b024117eb792c4894329d687eabe8274ab` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (protocol, opus) |
| Implementing Specialist | `protocol` |
| Paired Specialist | `paired-client` (client, opus) |
| Iteration | `2` (one Gate-2 failure: Layer 5, 28 clippy errors) |
| Security | RESOLVED-FIXED |
| Test | RESOLVED-DEFERRED |
| Observability | RESOLVED-FIXED |
| Code Quality | RESOLVED-FIXED |
| DRY | RESOLVED-DEFERRED |
| Operations | RESOLVED-DEFERRED |
| Semantic Guard | CLEAR |

---

## Task Overview

### Objective

Verbatim task description: `/tmp/devloop/story-runner/2026-08-27-hear-yourself-through-handler/task-8.prompt` (copied to §Appendix: Task Prompt below).

### Scope
- **Service(s)**: `proto/` (test-vectors, GSA), `crates/media-protocol/` (GSA), a new protocol-owned reference generator crate, `packages/sdk-core/` (TS codec conformance), `scripts/` (drift guard + ADR-0033 wiring), `docs/decisions/adr-0027`, `docs/decisions/adr-0036`.
- **Schema**: No
- **Cross-cutting**: Yes — Rust + TypeScript + proto + guards + ADRs

### Debate Decision
NOT NEEDED — ADR-0036 and ADR-0028 already mandate the cross-language vector gate; the two ADR edits this task lands were pre-approved in the story file (§Revisions / "ADR corrections landed with this story").

---

## Cross-Boundary Classification

Every path the plan touches gets a row, not only the cross-boundary ones. `proto/**` and
`crates/media-protocol/**` are ADR-0024 §6.4 enumerated Guarded Shared Areas, so **no row in this
diff is `Mechanical`**. The path-independent GSA rule is about the **primitives, not the document**
(`cross-boundary-ownership.yaml:16-22`: *"call-site usages become GSA wherever they appear"*), so it
reaches `docs/decisions/adr-0027-approved-crypto.md`, ADR-0036's §4 crypto hunk **and
`crates/media-vector-gen/**`**. Not enumerable, so the Layer-B guard structurally cannot produce
those rows and the Gate 1/3 reviewer applies the §6.4 criterion by hand — which is exactly how
@security caught that an earlier draft of this preamble read the rule too narrowly, marking a crate
made entirely of ADR-0027 call sites as `Mine / —`.

| Path | Classification | Owner (if not mine) | Note |
|------|----------------|---------------------|------|
| `proto/test-vectors/frame-v2.vectors.json` | Mine | — | The SSoT this task exists to create **GSA `proto/**` (ADR-0024 §6.4) — mine as protocol owner; classification cell kept to the bare token because `cross_boundary_classification.rs:139` skips a row only on an exact `Mine` match, so an inline annotation there silently disables the skip.** |
| `proto/test-vectors/README.md` | Mine | — | Directory contract + the "vector contradicting a ruling is a defect" escalation rule **GSA `proto/**` (ADR-0024 §6.4) — mine as protocol owner; classification cell kept to the bare token because `cross_boundary_classification.rs:139` skips a row only on an exact `Mine` match, so an inline annotation there silently disables the skip.** |
| `proto/test-vectors/external/sframe-wg/test-vectors.json` | Mine | — | **Verbatim** upstream bytes; located + independently reproduced by @paired-client, committed by me because the path is mine **GSA `proto/**` (ADR-0024 §6.4) — mine as protocol owner; classification cell kept to the bare token because `cross_boundary_classification.rs:139` skips a row only on an exact `Mine` match, so an inline annotation there silently disables the skip.** |
| `proto/test-vectors/external/sframe-wg/manifest.json` | Mine | — | The **shared row selector** (`cipher_suites: [4, 5]`), read by both harnesses so neither writes its own predicate. Scope-pinned here because two hand-written predicates could gate different subsets while both report green **GSA `proto/**` (ADR-0024 §6.4) — mine as protocol owner; classification cell kept to the bare token because `cross_boundary_classification.rs:139` skips a row only on an exact `Mine` match, so an inline annotation there silently disables the skip.** |
| `proto/test-vectors/external/sframe-wg/PROVENANCE.md` | Mine | — | URL, full 40-char SHA, path, upstream SHA-256, licence text verbatim, precedence-ladder outcome **GSA `proto/**` (ADR-0024 §6.4) — mine as protocol owner; classification cell kept to the bare token because `cross_boundary_classification.rs:139` skips a row only on an exact `Mine` match, so an inline annotation there silently disables the skip.** |
| `crates/media-vector-gen/**` (new crate) | **Not mine alone, Domain-judgment** (path-independent GSA) | security | Non-production Rust reference generator + its four test files. **GSA because it is nothing but ADR-0027 call sites**: `ring::hkdf::HKDF_SHA512`, `ring::hmac` `HMAC_SHA512`, AES-256-GCM and Ed25519 signing — four ADR-0027 table entries, the densest concentration of them anywhere in this diff. `cross-boundary-ownership.yaml:16-22`: *"call-site usages become GSA wherever they appear"* — path-independent, not enumerable, so the Layer-B guard structurally cannot produce this row. ADR-0027's Participants names security as Primary owner. Carries an `Approved-Cross-Boundary: security …` trailer for the crypto call sites |
| `crates/media-protocol/src/frame.rs` | **Not mine alone, Domain-judgment** (GSA) | media-handler | `docs/TODO.md` §Media Path Obligations items **(b)**, **(c)**, **(d)**. **B1 GRANTED (lead ruling R1)** — @media-handler added to the roster, so the co-sign premise is now true rather than assumed. Their confirmation is not automatic: any `frame.rs` hunk they object to comes out and is re-filed rather than argued down. Also gains a note at the `reject_reasons!` anchor explaining `has_vector` and the `decrypt_failed`/`unwrap_failed` split **Owner cell names media-handler alone** — the column answers *whose involvement this row needs that protocol does not already supply*. Protocol implements; **media-handler co-signs (intersection rule, ADR-0024 §6.4)**. `cross-boundary-ownership.yaml`'s own header says intersection enforcement "remains Gate 1 human-review territory", so the guard cannot express a co-sign and must not be asked to: the co-sign lives in @media-handler's Gate 1 confirmation (given), their Gate 3 Ownership Lens, and the `Approved-Cross-Boundary:` trailer. |
| `crates/media-protocol/tests/frame_properties.rs` | **Not mine alone, Domain-judgment** (GSA) | media-handler | Delete the third `MAX_PAYLOAD_BYTES` literal home at `:338` (@dry-reviewer #1); guard g2 supersedes it. In scope per R1. **Owner cell names media-handler alone** — the column answers *whose involvement this row needs that protocol does not already supply*. Protocol implements; **media-handler co-signs (intersection rule, ADR-0024 §6.4)**. `cross-boundary-ownership.yaml`'s own header says intersection enforcement "remains Gate 1 human-review territory", so the guard cannot express a co-sign and must not be asked to: the co-sign lives in @media-handler's Gate 1 confirmation (given), their Gate 3 Ownership Lens, and the `Approved-Cross-Boundary:` trailer. |
| `Cargo.toml` (workspace `members`) | Mine | — | Add `crates/media-vector-gen`. Cold-busts `cargo chef` recipe (OPS-6a). |
| `scripts/guards/simple/validate-frame-vectors.sh` | Not mine, **Minor-judgment** | test + operations (co-sign) | The Layer-3 drift guard |
| `crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs` | **Not mine, Minor-judgment** | infrastructure | Pins the `NOTE ` emission contract the row below establishes: asserts the allowlist line carries `NOTE ` and that a PASSING allowlist-only run emits **no** `^WARN ` line. Anchored `starts_with("WARN ")` to match the exit-0 arm's `grep -E "^WARN "`, deliberately **not** the crate's `survives_run_guards_filter` helper, which models the *failure* arm's unanchored pattern. `crates/dt-guard/tests/**` is infrastructure **by the Lead's ADR-0034 ruling** (not by the co-signer's own assertion) — same owner as the hunk, covered by their trailer |
| `crates/dt-guard/src/no_insecure_browser_flags.rs` | **Not mine, Minor-judgment** | infrastructure | @operations F2. Downgrades the allowlisted-mention emission `WARN ` → `NOTE ` and corrects a comment the OPS-8 fix falsified. **Not Mechanical despite looking like a one-word change**: it alters a guard's *emission contract*, and `docs/runbooks/devloop-validation.md` §6.3.1 documents dt-guard's `WARN` semantics. `crates/dt-guard/**` is absent from `cross-boundary-ownership.yaml`, so no Owner is derivable and I will not self-assign one for a path I do not own — @operations reads it as @infrastructure (dt-guard tooling), with @security optional. |
| `scripts/guards/run-guards.sh` | Not mine, **Minor-judgment** | test + operations (co-sign) | OPS-8(a): ~4 lines in `classify_guard_exit`'s **exit-0** arm surfacing `^WARN ` lines (line-anchored, trailing space; **no `NOTICE`** — a second severity token with no other emitter, no consumer and no runbook rows would fork the vocabulary; the inertness-vs-coverage-hole distinction lives in g14's message text, which already names the ungated codec and its closer) from *passing* guards, mirroring the failure arm's idiom **including the load-bearing `|| true`**. Not a special case for this diff — it closes a documented gap: `devloop-validation.md` §6.3's `ts-no-retained-credentials` contract (*"A clean run must be a WARN-free run … do not ignore it because the layer passed"*) is **unenforceable today**, because a WARN from a passing guard is discarded on exactly this arm |
| `scripts/guards/run-guards.test.sh` | Not mine, **Minor-judgment** | test + operations (co-sign) | Pins the new exit-0 arm with a stub guard that exits 0 after printing a `WARN ` line. Already wired at `layer3.sh` as `run-guards-selftest` |
| `scripts/guards/validate-frame-vectors.test.sh` | Not mine, **Minor-judgment** | test (co-sign) | Self-test driving every FAIL/vacuity branch. **Not** under `simple/` — `run-guards.sh` `find -name '*.sh'` would auto-run it as a guard |
| `scripts/layer3.sh` | Not mine, **Minor-judgment** | test + operations (co-sign) | One `run_and_emit` line wiring the self-test, matching the five existing precedents |
| `docs/decisions/adr-0027-approved-crypto.md` | **Not mine, Domain-judgment** (path-independent GSA) | security | Key-derivation row → HKDF-SHA256 **and** HKDF-SHA512 (`ring::hkdf::HKDF_SHA512`). Carries an `Approved-Cross-Boundary: security ...` trailer. `cross-boundary-ownership.yaml` states in its own header that this rule is **not enumerable** and must be applied by hand at Gate 1/3 — so this row exists precisely because no guard will produce it |
| `docs/decisions/adr-0036-media-flow.md` | **Not mine alone, Domain-judgment** (path-independent GSA for the crypto hunk) | security | Two hunks: (i) amendment-table `ADR-0027` row replacement (pre-approved in the story file); (ii) the **§4 KEK-wrap-nonce sentence** (lead ruling R2, worded as a clarification). Both carry an `Approved-Cross-Boundary: security ...` trailer. The §4 hunk also cites ADR-0011→`slos.md` as the in-tree precedent for an ADR deferring a normative value to a live file, and borrows `slos.md`'s explicitness about *which* file wins — that precedence being undefined for months was the actual defect there |
| `docs/observability/label-taxonomy.md` | **Not mine, Domain-judgment** | observability + security (co-owned per the file's own header) | Amended scope after @security's co-sign conditions, ACKed by @observability: (i) one shared-label row for `reason`, **pointing at `frame-v2.vectors.json` → `reject_reasons`, never restating tokens**, with a clause disambiguating it from `error_type`/`error_category` (why a *frame* was dropped vs why a *service operation* failed — three near-synonymous keys in one table); (ii) the pointer scoped to the **array**, not the file, plus a clause marking the file a test-vector fixture with **synthetic, test-only** key material, so a reader following the pointer to a field named `identity_private_seed_hex` does not file an incident — a false alarm stood down teaches people to ignore the next one; (iii) a short `## Frame reject reason` section that **cites** §R2 and the task-#7 meeting bar rather than restating them (restating would give the rule two homes and the next person to tighten one would not know to tighten the other) and carries only what is genuinely new — the **rationale**: reject reason joined to a target dimension is a **decryption oracle**, an attacker injecting crafted frames at a victim and reading which crypto layer rejected each probe, with the `unwrap_failed`/`decrypt_failed` split *increasing* that oracle's resolution. Modelled on `## Key custody`'s "Why single-valued" cell. @security's sentence kept: *"no participant dimension" without "because it becomes a decryption oracle" reads as a cardinality rule, and cardinality rules get traded away under pressure*. **Also cites R3's inertia note**, which changes the section's job: R3 records that the SDK's implicit join label set (`client_version`, `meeting_id_hash`, `org_id`) is threaded from `MeetingSession.join` into **every** emission site including the media module. So `reason × meeting_id_hash` — the oracle — is the **default**, arriving by doing nothing, not a slice someone adds later. The section therefore warns a task-19 implementer that the violation **ships unless they act**, in R3's own present-tense-required register (*"stated in the present-tense-required form because the inertia is the whole risk"*). **Safety is stated as a property of the SET, not of any member** (@security): `reason` is safe because its permitted partners (`client_version`, `org_id`) are non-identifying, not because it is intrinsically harmless — it is one factor of the oracle and the allow-list's job is to deny it a partner. A reader who concludes "these three are individually safe" adds a fourth by the same reasoning, which is how the oracle gets rebuilt out of permitted labels. One clause marked **`[reviewer-only]`**: `org_id` is non-identifying only **at scale** — a single-tenant org, or one whose meetings do not overlap in time, makes it quasi-identifying and `reason × org_id` reconstructs a per-meeting distinguisher from permitted labels. That residual is **not guardable** (it depends on deployment shape, which no guard can see), so the clause is its entire control and must be marked as such: a residual documented inside an enforced-looking rule inherits the rule's credibility without its enforcement |
| `docs/runbooks/devloop-validation.md` | Not mine, **Minor-judgment** | operations | §6.3 REASON-token rows + §8 symptom rows. **Rows state the lane** (OPS-9): vacuity bails exit nonzero, so they hit `classify_guard_exit`'s `*)` arm → `FAILED_GUARDS` → **implementer lane**; a guard subprocess cannot self-declare a lane, and §6.3.1 item 4 already documents that asymmetry for dt-guard. The fifteen tokens are **split into two sub-classes** the way the runbook splits `release-build-profile-*`, because the two halves need **opposite first actions**: *causable by an ordinary edit, the message is the fix* (`vectors-unparseable`, `banner-missing`, `zero-rows`, `generator-crate-missing`, `todo-tracking-entry-missing`, `suite-list-empty`, `gated-by-key-missing`, `manifest-missing`, `suite-yields-zero-rows`, `zero-key-fields-found`) vs *the guard's own extraction broke — start at the `sed`, not the input tree* (`rust-const-not-found`, `rust-version-const-not-found`, `rust-flag-consts-not-found`, `reject-macro-not-found`, `ts-site-not-found`, `codec-list-undeclared`, `key-field-pattern-undeclared`, `provenance-digest-not-found`, `external-vectors-missing`, `vectors-missing`). The runbook records that getting this split wrong is how environmental advice "actively misleads"; fifteen tokens under one generic row would reproduce that |
| `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` | **Not mine, Domain-judgment** | @team-lead (ruling R3) + @paired-client | **Covers task 15 AND task 19.** Task-15 manifest-prompt edit. `status`/`deps`/`slug`/`tag` untouched. R3 authorised three changes (consume-don't-vendor; close the guard's TS leg; assert row 16 as cache state); @paired-client handed over three more; I flagged the growth rather than absorbing it and **@team-lead ruled FOLD ALL SIX** — the cap was on opinions, not on facts the next implementer cannot derive — (4) drop `identity_private_seed_hex` if the deterministic-signature assertion does not land (@security approved the field *only* on that condition, so the instruction must travel with the deferral); (5) HKDF-Extract must be explicit, with WebCrypto's zero-length-HMAC-key rejection resolved as 64 zero bytes and the block-size-padding equivalence commented at the site; (6) the AES-256 key-length hard-reject, because WebCrypto provides **no backstop** (a 16-byte AES-GCM `importKey` succeeds silently) and that absence is what a future reader assumes exists. All three are empirical, non-inferable from ADR or RFC, and each has a wrong-looking-correct trap . **Finding I (@security): the row now covers the task-19 prompt too** (`:420`), which already pins `dt_client_media_frames_dropped_total{reason}` to a vocabulary that diverges from what task 8 freezes. Verified verbatim on disk. Two prose fixes, `status`/`deps`/`slug`/`tag` untouched: `replay` → `replay_detected`, and `decrypt_failed` split into `decrypt_failed` + `unwrap_failed` with O3's one-line rationale — `unwrap_failed` exists *because of* a split agreed inside this devloop, **after** that prompt was written, so the task-19 implementer would emit exactly the conflation the split prevents. **The third divergence (`decode_reject` instructed as a `reason` value) is **resolved by R-25 at `:53` of the same file**, which forbids that literal verbatim — so it is a prose fix after all, citing R-25's own correction. The one genuinely open sub-clause (neither-key-nor-wrap, no token in either vocabulary) ships as an **explicit open pointer naming task 19** unless ratified cheaply — per @team-lead, an invented token in a GSA path is worse than a named gap** — see §Planning |
| `docs/TODO.md` | Mine (entries I close) **+ exactly one new entry** | — | Tick §Media Path Obligations (a)(b)(c)(d) and the escalation-rule entry. Add **one** new entry: the **D-1 tracking entry g14 asserts**, carrying a stable `<!-- frame-vectors-ts-leg -->` marker rather than prose — g14 greps the marker, so `/close-story` ticking `[x]` satisfies it rather than breaking it, and a RENAMED marker reds (the self-test drives exactly that case). **The §Observability Debt GC/oracle entry is NOT mine and I write nothing on it**: @security has taken it as reviewer spin-out tracking and holds the corrected spec (count-don't-drop rather than metric-name-aware filtering; `telemetry_filter.rs:155` binds `metric` at the loop head, so `metric.name` is available at every `filter_attrs` call site and it is parameter passing, not redesign). An earlier draft carried that entry's full text here, which would have produced either two drifting entries for one debt or — worse, because silent — none, with @security and me each deferring to the other. @observability caught it by re-reading their own rows and verifies at review that it exists **exactly once**, under §Observability Debt, owner observability |
| `packages/sdk-core/src/media/frame/__tests__/external-anchor.test.ts` | **Not mine, Domain-judgment** | @paired-client + security | The external anchor check: our TS key schedule against the vendored 0x0005 and 0x0004 rows. **No TS codec and no span arithmetic** — see §7a's invariant |
| `packages/sdk-core/src/media/frame/__tests__/sframe-key-schedule.ts` | **Not mine, Domain-judgment** | @paired-client + security | RFC 9605 §4.4 schedule, authored from the spec text without reading the Rust generator. **No TS codec and no span arithmetic** — see §7a's invariant |
| `packages/sdk-core/src/media/frame/__tests__/vendored.ts` | **Not mine, Domain-judgment** | @paired-client + security | Loads the vendored artifact and the shared `manifest.json` selector. **No TS codec and no span arithmetic** — see §7a's invariant |
| `packages/sdk-core/src/media/frame/__tests__/vendored.test.ts` | **Not mine, Domain-judgment** | @paired-client + security | Anti-vacuity self-test for the selector, including that per-suite selection rejects where 'at least one row overall' would pass. **No TS codec and no span arithmetic** — see §7a's invariant |
| `packages/sdk-core/src/media/frame/__tests__/hex.ts` | **Not mine, Domain-judgment** | @paired-client + security | Hex helpers. **No TS codec and no span arithmetic** — see §7a's invariant |

> **Deliberately absent: `packages/sdk-core/src/media/frame/**`.** An earlier draft carried a row for
> it noting "TS v2 codec + SFrame/Ed25519/KEK-unwrap primitives"; @dry-reviewer caught that the row
> was a standing **licence** for exactly what @team-lead's R3 ruling forbids, and that "I write no
> TypeScript" does not withdraw it — the row's Owner is @paired-client and they could. The table, not
> the prose 450 lines below it, is what Gate 3 and `validate-cross-boundary-classification.sh` read,
> so an invariant at §7a and a contradicting row in the same document is worse than no invariant: a
> reader who checks the table concludes the question was considered. Task 8 touches no non-test file
> under that path, so the row is **deleted rather than rescoped** — which makes any future diff there
> an *unclassified* path the guard catches, instead of a pre-authorised one it does not.

---

## Planning

### 0. Mechanism restatement (wider class than the task names)

**Instance language**: "cross-language frame v2 test vectors."

**Mechanism language**: *A wire contract implemented independently in two languages, where both
implementations are exercised only by a loop that contains both of them, is unverified — agreement
is self-agreement. The repair is a third, differently-authored derivation, anchored to an external
artifact wherever one exists.*

The restated mechanism produces two same-owner siblings the task does not name. Surfacing both;
neither is in scope here.

1. **`signaling.proto` / `internal.proto` have the identical shape and no byte-level gate.**
   `crates/proto-gen/tests/*_roundtrip.rs` (Rust/prost) and
   `packages/proto-gen/scripts/verify-codegen.sh` (TS/protobuf-es) are **presence/absence** oracles,
   not cross-language byte vectors. A prost-vs-protobuf-es disagreement on, e.g., optional-field
   presence encoding would be invisible in exactly the way ADR-0036 §2 describes. **Lower risk than
   the hand-rolled binary format** — protobuf has an external canonical spec and two mature
   independently-authored libraries, so the correlated-error probability is far lower — which is why
   I am naming it rather than scoping it. Not filed as a TODO unless a reviewer wants it.
2. **`max_payload_bytes` is one instance of "one wire constant, two language homes, no SSoT."**
   The mechanism generalises to *every* constant both codecs need. `frame.rs`'s own ANCHOR comments
   already demand this generalisation in-tree (`LEGAL_FLAG_MASK`: "the TypeScript codec derives its
   mask from that file rather than hardcoding one"; `EXT_REGISTRY`: "both codecs derive
   `MAX_EXT_BYTES` from the pinned registry"). **This one I am scoping in**: the vectors file carries
   a `wire_constants` block and an `extension_registry` block, not a lone `max_payload_bytes`.

### Blockers raised at Gate 1 — all resolved by @team-lead (R1–R3)

- **B1 — `crates/media-protocol/**` co-sign. RESOLVED: GRANTED (R1).** @media-handler added to the roster as a conditional domain reviewer scoped to `crates/media-protocol/**`; all four TODO items land. Original raise: ADR-0024 §6.4 + `cross-boundary-ownership.yaml:34`
  make that path owned by **protocol AND media-handler**, requiring co-sign at Gate 1 *and* Gate 3.
  **@media-handler is not on this team roster.** `docs/TODO.md` §Media Path Obligations explicitly
  defers items (a)–(d) *into task 8* on the stated grounds that "task 8 is a `media-protocol` task
  where that co-sign is already being paid" — which is only true if media-handler is on the team.
  Request: add `@media-handler` as a reviewer. Fallback if refused: I drop TODO items (b)(c)(d) and
  the `frame_properties.rs` pin deletion from this task and re-file them, landing only (a) — which
  is `proto/**`-only and needs no media-handler co-sign. **I will not edit a GSA path without its
  co-owner.** Everything else in the plan is unaffected either way.
- **B2 — a third ADR edit (D1 below).** The task names two ADR edits. Implementation found ADR-0036
  §4's KEK-wrap nonce genuinely underspecified (@paired-client raised it independently as their
  gap 4.1). Leaving it to two implementations is the exact correlated-error shape this task exists to
  close, so per CLAUDE.md "fix, don't defer" I propose landing a one-sentence §4 amendment here
  rather than filing it. Needs @security sign-off and @team-lead's scope nod.
- **B3 — task 8 vs task 15 split.** Endorsing @paired-client's proposal verbatim: **task 8** lands
  the TS codec + crypto primitives + vector-conformance harness + external gate; **task 15** lands
  the surrounding stack (key cache, KEK-generation retention window, replay window state, roster
  lookup, drop counters, `MediaTransport` integration). No code written twice, and this task's
  deliverable stays real.
- **B4 — vendored-artifact licence.** `sframe-wg/sframe`'s `LICENSE.md` at `025d568` is **not a
  licence text**; it is three lines pointing at the MLS working group's CONTRIBUTING.md. This would
  be the repo's first vendored third-party file. Flagging to @security/@operations/@team-lead rather
  than vendoring silently (OPS-5). My reading: the file is IETF WG output derived from RFC 9605,
  reproducible under BCP 78 / the IETF Trust Legal Provisions, and vendoring IETF test vectors is
  routine. Recording the upstream licence text **verbatim** in `PROVENANCE.md` so a future reader
  evaluates the actual words rather than my summary. Proceeding unless told otherwise.

### 1. Answers on the record (@security Q1/Q2, @paired-client gaps 4.1–4.4)

**Q1 — is 0x0005 present with usable derivations at `025d568`? YES. Concrete finding, verified by
me by cloning the repo and by re-deriving the schedule in an independent scratch script; separately
and independently reproduced by @paired-client.**

- Full SHA: `025d568a506937c901af8e7f0a663f39aeaf67ad`, `sframe-wg/sframe`, subject "Update test
  vectors".
- File `test-vectors/test-vectors.json`, 35121 bytes, SHA-256
  `b8d35efd41749567427cb9ae52d9a7362904154978ff6a5fb20c0258a8ffdec1`.
- `sframe` array, 5 rows, `cipher_suite` 1..5. **Row `cipher_suite: 5` is present** and carries every
  field the gate needs: `base_key`, `kid`, `ctr`, `sframe_key_label`, `sframe_salt_label`,
  `sframe_secret`, `sframe_key`, `sframe_salt`, `nonce`, `aad`, `pt`, `ct`.
- `sframe_secret` is **64 bytes** — HKDF-SHA512, exactly as the ciphersuite forces. `sframe_key` is
  32 (Nk), `sframe_salt` 12 (Nn). No enc_key/auth_key split: the derived key length *is* the AEAD key
  length.
- Re-derived independently and matched byte-for-byte:
  `PRK = HKDF-Extract(salt = "" (empty), IKM = base_key)` under SHA-512;
  `sframe_key = HKDF-Expand(PRK, "SFrame 1.0 Secret key " || KID_be8 || CS_be2, 32)`;
  `sframe_salt = HKDF-Expand(PRK, "SFrame 1.0 Secret salt " || KID_be8 || CS_be2, 12)`;
  `nonce = sframe_salt XOR CTR_be12`.
- **Precedence outcome: the primary branch. No 0x0004 fallback, no third-party cross-check.** This
  outcome is recorded in `PROVENANCE.md` (not only here), together with the ladder that was *not*
  taken, per OPS-5(c) and CLAUDE.md fail-loudly.
- Adopting @paired-client's addition: **also gate 0x0004**, because it is the identical code path at
  a different hash and it turns the "PRK is 64 not 32" assertion from a claim about our own code into
  a *tested contrast* (0x0004's `sframe_secret` is 32 bytes).

**Q2 — where does the generator get the AAD span / signed range / detached-tag split?**
**From the ADR-0036 spec text, arithmetically, in `crates/media-vector-gen/src/spec.rs`; then
*cross-asserted* against `media-protocol`'s parser-walk-derived ranges. Never ported from
TypeScript, and I have not read the TS codec.** Precisely:

- `spec.rs` computes `publisher_region_end = PUBLISHER_FIXED_PREFIX_SIZE + (key_bearing ?
  WRAPPED_TRANSMIT_KEY_SIZE : 0) + EXT_LENGTH_FIELD_SIZE + ext_len` from §2's field list, importing
  the *sizes* from `media_protocol::frame` (@dry-reviewer #3 — sizes are not one of the three things
  under independent test) but doing the *span arithmetic* itself.
- It then asserts equality against `MediaFrameView::publisher_region_range()` /
  `signed_ranges()` obtained by decoding the frame it just built. That gives **three** independent
  derivations of the span pinned to one value: my ADR-derived arithmetic, media-protocol's
  parser walk, and @paired-client's TS. `frame.rs`'s own `publisher_region()` doc says this crate
  "cannot establish that the span is *correct*" because its fixtures come from its own encoder —
  the generator's arithmetic is the outside derivation that closes exactly that hole.
- Not a "second parser": it is encode-side offset arithmetic in a non-production crate whose only
  use is to *assert agreement*, never to parse hostile bytes. Flagging it explicitly for
  @code-reviewer since `frame.rs` warns against second parses.

**D1 (client gap 4.1) — KEK-wrap nonce. RULING + ADR amendment (B2).**
`wrap_nonce = 0x00000000 || key_id_be8` — a 12-byte GCM nonce, **zero-prefixed**, key id
right-aligned. Rationale: SFrame's own nonce rule right-aligns a big-endian counter in the nonce
(`salt XOR CTR_be12`), so the codebase gets **one** padding rule rather than two. Injective in
`key_id`, so §4's stated reduction ("nonce uniqueness under the KEK reduces to key-id uniqueness")
holds exactly as written. Wrap AAD = the 8 KID bytes alone, per §4 "the key id as associated data".
Pinned per-row in the vectors *and* amended into ADR-0036 §4.

**D2 (client gap 4.2) — `transmit_key_hex` is the SFrame `base_key`, not the derived AEAD key.**
Confirmed. The key id encodes (sender, stream, generation) and the schedule derives key+salt from
(base_key, KID); if the transmit key were the raw AEAD key the KID would not feed the derivation and
the external 0x0005 gate would test a path we never execute. Field name stays `transmit_key_hex`
(task-mandated) with an explicit `_comment` saying it is the base key, and each row's `derived` block
additionally pins `sframe_key_hex` and `sframe_salt_hex` so the per-row derivation is visible.

**D3 (client gap 4.3) — the `ext_length` u16 IS inside the AAD.** Confirmed against the
implementation, not just prose: `Layout::publisher_region_end() = ext_start + ext_len` where
`ext_start` is *after* the length field, so `0..publisher_region_end` includes it. It must be
included: the signed input is discontiguous and `ext_length` is what authenticates the split point,
so excluding it would leave the length prefix rewritable by the relay. Span =
`10 + (key_bearing ? 50 : 0) + 2 + ext_len`, matching @security #5.

**D4 (client gap/§3) — detached-tag split: `key_id(8) || tag(16) || ciphertext(n)`.** I reached this
independently from §2's "SFrame object: key id and authentication tag in its own clear header" plus
`SFRAME_OBJECT_OVERHEAD_BYTES = KEY_ID_BYTES + AEAD_TAG_BYTES = 24`; @paired-client reached the same
from the same sentence without seeing my reading. **Recorded as an independent convergence, not a
converged-on-first-answer** — and `payload_length = 24 + plaintext_len` is therefore known *before*
sealing, which is what makes the construction order in §4 below possible at all.

**D5 (client gap 4.4) — yes, pin the near-miss.** Rows carry
`naive_contiguous_signed_input_hex`: the byte string a `frame[0..payload_end]` bug would have signed
(i.e. including the 6 relay bytes). The guard asserts it **differs** from `signed_input_hex` on every
row that carries it, so the near-miss is documented and non-degenerate rather than merely avoided.

### 2. Vector-file shape

Top level (stable key order, deterministic emission):

- `_non_production` — banner string; the guard asserts it is present and non-empty (this is one of
  the mechanical protections against a "dead Rust crypto" cleanup, D8).
- `_comment_kind_is_not_a_metric_label` — @observability O4, verbatim: *`kind` classifies the fixture
  row and is never a metric label value; `reject_reason` is the label taxonomy.*
- `_comment_fixture_keys` — every key value is a synthetic, structured, low-entropy fixture
  (`0x10+i`, `0x20+i`, …), chosen so a human reader sees at a glance it is not CSPRNG output; nothing
  in this file derives from any real environment or secret (@security #10).
- `schema_version`, `header_version` (== `PROTOCOL_VERSION`, guarded — TODO item (a)),
  `cipher_suite_id` (number `5`) + `cipher_suite_name` (`"AES_256_GCM_SHA512_128"`),
  `max_payload_bytes` (JSON **number**).
- `wire_constants` — `legal_flag_mask`, `signature_bytes`, `aead_tag_bytes`, `key_id_bytes`,
  `relay_region_bytes`, `publisher_fixed_prefix_bytes`, `wrapped_transmit_key_bytes`,
  `ext_length_field_bytes`, `max_ext_bytes`, and the KID layout widths
  `key_id_layout: {sender_id_bits: 16, stream_bits: 8, generation_bits: 40}`.
- `extension_registry` — `[{type, value_len, accepted: {byte_range: {min, max}}}]` plus
  `unknown_type_is_rejected: true`, `duplicate_type_is_rejected: true`,
  `ascending_type_order_required: true` (the ANCHOR at `EXT_REGISTRY` demands the *rejection
  semantics*, not just the table).
- `reject_reasons` — array of `{token, layer: "codec"|"crypto"|"key", drops_frame: bool, has_vector: bool}`. **One closed
  set, tagged by owning layer** (@dry-reviewer): a flat *untagged* union would let each side assert
  only `⊆`, which stays green when a structural variant is **deleted** from the `reject_reasons!`
  macro — downgrading a compile-time impossibility to a subset check exactly at the language
  boundary. Tagged, my guard asserts `{token | layer=="codec"}` **equals** `ALL_REJECT_REASONS`, and
  @paired-client asserts `{token | layer=="crypto"}` equals their closed set. `drops_frame`
  (@observability O3 option (a)) is derived once here and read by both sides; `wrap_key_id_mismatch`
  is the only `false`, which is what keeps a *played* frame out of task 19's single flat drop counter
  and preserves R-25's `received = played + sum(drops)` identity.
  **`has_vector` settles @observability's scoping question, as a field rather than a sentence.** The
  array is the **complete** R-25 label space, so nobody reading the most authoritative-looking token
  list in the repo gets a short one. `no_kek_for_generation` and `no_roster_entry` (`layer: "key"`)
  are `has_vector: false`: they are functions of receiver key-store and roster state, not of frame
  bytes, so no "these bytes → this outcome" row can exist for them. @paired-client's build-time
  exhaustiveness check filters on `has_vector == true`, so it needs no fabricated error arm — which
  is what would have quietly re-introduced the fallback they designed out. The guard then asserts
  what a scope *sentence* could not: **every `has_vector: true` token appears in at least one row,
  and every `has_vector: false` token appears in exactly zero rows.** Both readings stop being
  available.
  **Scope boundary, and an extension rule with a named owner** (@observability). The array is the
  complete R-25 label space, and a completeness *claim* is only as good as the rule that maintains
  it — the guard can check `has_vector` ↔ rows and set-equality against `ALL_REJECT_REASONS`, but
  nothing machine-readable says what R-25's space contains, so completeness stays reviewer-only. The
  honest cost of choosing completeness over a scoped subset is that a gap becomes a **false**
  completeness claim rather than an advertised subset. Two mitigations, both cheap:
  (i) the file states the rule — *any new value of the client or MH media-drop `reason` label MUST be
  added here before it is emitted; @observability owns the taxonomy (ADR-0011), protocol owns the
  file* — which converts "someone forgot" into "someone violated a written rule", reviewable at the
  gate where the new reason lands; and (ii) @observability has committed to wiring the documented
  bounded-values in `docs/observability/metrics/*.md` to be guard-compared against this array when the
  drop counter lands at tasks 19/22 — the check that actually catches an omission, in their lane.
  **And the rule says what belongs, not only that things must be added**, because a completeness claim
  makes the array an attractor: *in scope = reasons for which the frame is the subject* — the receive
  path from wire bytes through decode, verify, unwrap, decrypt, and the key/roster lookups keyed off
  frame fields. *Out of scope = a drop caused by receiver-side resource state after the frame is
  successfully opened* (a story-3 jitter-buffer overflow or decoder error). Without that line, a
  purely client-side observability label would have to change a GSA `proto/**` path under protocol
  co-sign, and the friction pushes someone to simply not add it — reintroducing the omission through
  the side door.
  **`has_vector` and `layer` are independent axes, and the guard must not encode the implication in
  either direction** (@paired-client, sharpened by @observability). `has_vector` answers *"can a byte
  string determine this outcome?"*; `layer` answers *"which subsystem owns the remedy?"* — orthogonal
  questions. `unwrap_failed` is the worked example, stated next to the field definitions:
  byte-determined (`has_vector: true`) while the remedy is key distribution. The hazard is subtle and
  @observability's correction makes it **more** urgent, not less: as authored,
  `layer == "key"` implies `has_vector == false` holds **by coincidence** across every row. A visibly
  violated coupling gets no traction; an accidentally true one is what a future reader verifies
  exhaustively against the data in front of them and then encodes as a derivation — correctly against
  the file, wrongly against the design. The breaker is one classification judgment away, on a row I am
  authoring now: `wrap_key_id_mismatch` is a plausible reclassification to `layer: "key"` (nothing
  cryptographic fails there; it is a key-id comparison), at which point the coupling breaks with
  `has_vector: true`. So the guard asserts `has_vector` against row counts and `layer` against nothing
  but its own closed value set — **no cross-field derivation, and specifically no "sanity check" that
  key-layer tokens have no rows, which is the bug wearing a guard's costume.**
- `vectors` — the rows.

Per row: `name`, `description`, `kind`, `frame_hex`, `offsets`
(`publisher_region_len`, `relay_region_offset`, `payload_offset`, `signature_offset` —
@paired-client §6, so a span disagreement names an offset rather than surfacing as an opaque tag
mismatch 300 bytes later), `decoded` (`version`, `flags{...}`, `payload_length`, `stream_sequence`,
`kek_generation`, `stream_id`, `hop_sequence`, `extensions[]`, `key_id` as a **16-hex-char string**,
`key_id_decomposed: {sender_id: <number u16>, stream: <number u8>, generation: <hex string>}`),
`derived` (`signed_input_hex`, `aead_aad_hex`, `sframe_nonce_hex`, `sframe_key_hex`,
`sframe_salt_hex`, `wrap_nonce_hex`, `wrap_aad_hex`, and where applicable
`naive_contiguous_signed_input_hex`), `crypto` (`identity_public_hex`, `identity_private_seed_hex`,
`kek_hex`, `transmit_key_hex`, `plaintext_hex`), and for reject rows `reject_reason` +
`expected` (row-kind-specific booleans).

Two shape rulings, both @paired-client's asks: **every integer wider than 32 bits is a lowercase
unprefixed even-length hex string** (`key_id`, `generation`); everything ≤32 bits is a JSON number.
No `null`s anywhere — an absent field beats a null for a TS discriminated union over `kind`.

`identity_private_seed_hex` is a **fifth** crypto field beyond the four the task lists, **approved by
@security under four conditions, all met**. Reason for it: without the Ed25519 seed the vectors gate
only the *receive* path (verify) and never the *send* path (sign) — and the untested half is where a
construction-order bug (signing over the wrong span) lives. Ed25519 is deterministic per RFC 8032, so
a pinned seed makes signature bytes reproducible.

1. **Named `identity_private_seed_hex`, not `identity_seed_hex`** — "seed" alone does not signal
   *private key* to a skimmer, and naming is the only control this file gets (condition 3 explains
   why).
2. Value from the declared synthetic pattern, same as every other fixture.
3. **The guard enforces the synthetic-pattern property mechanically** (guard check **g13**). No
   `dt-guard` module scans `.json` — every credential module is extension-scoped to
   `.rs`/`.ts`/`.tsx`/`.svelte` — so `no-hardcoded-secrets.sh` will never look at this file and the
   only control today would be authorship discipline, which is not a control. g13 asserts every
   key-material field matches the **declared pattern predicate**, structurally derived rather than an
   allowlist of current values, so it survives adding a row and fires on exactly the change that
   matters: someone regenerating with real CSPRNG material and committing it.
4. **The seed is load-bearing inside task 8, not dormant.** `rust_codec_conformance.rs` reads
   `identity_private_seed_hex` **from the JSON** (not from `fixtures.rs`), re-signs, and asserts byte
   equality against the row's pinned `signature_hex`. An unused private seed in a checked-in file is
   strictly worse than no seed — the risk with none of the benefit — so it earns its place or it does
   not land.

### 3. Row inventory

| # | name | kind | Proves |
|---|------|------|--------|
| 1 | `aad_span_no_key_no_ext` | roundtrip | AAD span, combination 1 of 4 |
| 2 | `aad_span_key_no_ext` | roundtrip | combination 2 |
| 3 | `aad_span_no_key_with_ext` | roundtrip | combination 3 |
| 4 | `aad_span_key_with_ext` | roundtrip | combination 4 — the loopback-traffic shape; 1–3 never occur in loopback and are where the two hand-written span computations can silently diverge |
| 5 | `key_id_all_max_roundtrip` | roundtrip | `0xFFFF` / `0xFF` / `2^40−1`, and that the KID survives encode→decode→nonce unchanged |
| 6 | `decode_ok_min_payload` | decode_ok | smallest legal frame |
| 7–12 | one per structural reject reason exercised on-wire | decode_reject | the 8-token structural vocabulary, asserted against `ALL_REJECT_REASONS` |
| 13 | `tamper_publisher_region` | verify_reject | mutated publisher region → `signature_invalid` |
| 14 | `tamper_signature` | verify_reject | mutated signature → `signature_invalid` |
| 15 | `insider_forgery_other_sender_key_id` | verify_reject | **ADR Assumption 4.** Forger genuinely holds the victim's transmit key; the SFrame object is validly sealed under the victim's KID and **would decrypt** if verification were skipped. Row carries `expected.would_decrypt_if_verification_skipped: true` and `expected.plaintext_if_verification_skipped_hex`, and **both harnesses assert that property** — otherwise the row proves the signature layer load-bearing only by assertion (@security #6) |
| 16 | `wrap_for_different_key_id` | wrap_binding | Correctly self-signed, verification **PASSES**, frame is otherwise processed; the mis-bound wrap is **ignored, not cached**. `expected: {signature_valid: true, frame_dropped: false, wrap_cached: false, decrypts: true, outcome: "wrap_key_id_mismatch"}`. Asserted as **cache state**, not as a reject reason (@security #6, @observability O3) |
| 17 | `decrypt_reject_wrong_transmit_key` | decrypt_reject | `decrypt_failed` — the **SFrame payload** decrypt (transmit key, `sframe_nonce`, AAD = publisher region) |
| 17b | `unwrap_reject_wrong_kek` | decrypt_reject | `unwrap_failed` — the **KEK unwrap** (meeting KEK, nonce = padded KID, AAD = the 8 KID bytes). @observability's split: two AES-GCM decrypts on one receive path routing to **opposite teams** (unwrap → key distribution, MC and the roster path; payload → key schedule or sender). One `decrypt_reject` kind carrying two `reason` values is the kind-vs-reason separation doing its job |
| 18 | `replay_same_stream_sequence` | replay_reject | `replay_detected`; a second row identical to a prior row's `frame_hex` |
| 21a / 21b | `wrap_determinism_seq_lo` / `wrap_determinism_seq_hi` | roundtrip | @security finding E. Two key-bearing frames sharing one key id (same sender/stream/generation) at different `stream_sequence`; their 50-byte wrapped-key blocks are asserted **byte-identical**. This is the only row pair that asserts ADR-0036 §4's *"one transmit key wraps to one ciphertext, byte-identical from frame to frame within a generation"* — the sentence that is the whole argument for a derived-not-carried wrap nonce. Catches two self-consistent single-language bugs nothing else here catches: a randomised wrap nonce (field differs per frame; since the nonce is not carried, no receiver could ever open it) and a wrap nonce derived from `stream_sequence` instead of the key id (opens fine in loopback, destroys the key-id-uniqueness reduction §4's nonce safety rests on) |
| 20 | `ext_salience_out_of_range` | decode_reject | @security's catch: `EXT_REGISTRY`'s accepted set `0..=100` is a trust boundary (§7 — selection signals are publisher-supplied and untrusted) with no cross-language vector today. Salience `101` → `extensions_malformed` |
| 19 | `full_frame_compose` | full_frame | decode → verify → unwrap → decrypt → plaintext, asserted end to end |

### 4. Generator: `crates/media-vector-gen` (new workspace crate)

Labelled non-production structurally, not by a note: crate name, `publish = false`, a `//! # NOT
PRODUCTION CODE` banner as the first line of the crate docs, a `NON_PRODUCTION_BANNER` const
**emitted into the JSON** so removing the label reds the guard, and a `#[doc]`/comment at each of the
three crypto call sites.

Module split:

- `spec.rs` — **the three irreplaceable computations**, each carrying a comment naming ADR-0036 §2/§3/§4
  as its derivation source and stating that it is deliberately a second implementation which
  @dry-reviewer has ruled must not be collapsed. Construction order is **typestate, not a comment**
  (@security #9): `PublisherRegion` is only obtainable from `PublisherRegionDraft::finalize(payload_len)`,
  and `aad()` exists only on `PublisherRegion` — so slicing the AAD before `payload_length` is filled
  **does not typecheck**. `payload_len` is itself produced by `sframe_object_len(plaintext_len)` so it
  cannot be guessed.
- `kid.rs` — checked packer. `u16::try_from(sender_id)`, `stream <= u8::MAX`, `generation < 1 << 40`,
  each returning `Err`, bounds derived from the **named constants** landing in `frame.rs` per TODO
  item (d) (@dry-reviewer #4), never from three inline literals. **Every nonce and wrap-AAD in the
  file is computed from the 8 KID bytes sliced out of the *encoded frame*, never from a value
  re-packed out of the decomposed fields** — so a receive-side decomposition or re-pack bug surfaces
  as a crypto mismatch instead of cancelling out (@security #8). The slice site carries that reason
  as a comment.
- `schedule.rs` — RFC 9605 §4.4 key schedule. HKDF-Extract computed **explicitly** as
  `ring::hmac::sign(HMAC_SHA512, salt="", ikm=base_key)` so the PRK bytes are observable (ring's
  `hkdf::Prk` is opaque and would make "assert the PRK length" unassertable); asserted `== 64` with a
  comment naming 32 as the SHA-256 value and the correlated-error signature. Expand via
  `ring::hkdf::HKDF_SHA512` (ADR-0027-approved algorithm, named literally), cross-checked against
  `Salt::new(HKDF_SHA512, &[]).extract(...)` so the explicit and library forms are proved equal.
  Direct-GCM form, **no** enc_key/auth_key split, with a negative assertion that the derived key
  length equals the AEAD key length (32) rather than 32+auth.
  **The external-gate entry point takes a raw `[u8; 8]` KID and cannot reach our packer** (@security
  constraint A, structural not conventional). The external row's `kid` is `291`, which under our
  layout decomposes to `sender_id = 0` — the value `docs/TODO.md` records as reserved-invalid because
  a shared zero is N colliding key ids, i.e. two senders at one nonce under one KEK. With the raw-byte
  signature there is no code path where our layout could be interposed, so a future layout change
  cannot break an external gate that has nothing to do with our layout (and be misread as an upstream
  problem).
  **The 0x0004 contrast terminates at the key-schedule output** (@security constraint C): assert hash,
  PRK length 32, derived key length **16**, and stop. It never reaches a seal/open call, plus an
  explicit assertion that the AEAD constructor **rejects** any key length other than 32. Otherwise we
  would have built an AES-128 acceptance path reachable by anything that can influence a suite id, in
  order to test that we do not have one. ADR-0036's amendment table permits AES-128-GCM "only for
  SFrame interop", and we do none.
- `fixtures.rs` — synthetic keys, structured low-entropy patterns.
- `rows.rs`, `json.rs`, `main.rs`.

**Framing is NOT re-implemented** (@dry-reviewer #3): the generator calls
`media_protocol::codec::encode_frame(&MediaFrameParts)` and imports every layout size from
`media_protocol::frame`. Determinism (OPS-7): no `rand`, no `getrandom`, no `SystemTime`; Ed25519
from fixed seeds; serde struct field order gives byte-stable output, so regenerating with nothing
changed leaves `git diff` empty. Dependencies restricted to crates already in
`[workspace.dependencies]` — `ring`, `hex`, `serde`, `serde_json` — so no new `cargo audit` surface
(OPS-6b). No `tracing`, so the generator cannot pull telemetry into `media-protocol`'s dependency
graph (@observability O12).

Rejected: the `Cargo.toml` `exclude` route used by the two fuzz workspaces (OPS-6a). Excluded crates
are not compiled by the pipeline, which is the standing complaint in `docs/TODO.md` about exactly
those two — an excluded generator could rot silently and take the gate with it.

### 5. Layer-3 guard vs Layer-4 tests — the split (OPS-1)

The guard budget is **30s** with a hard `--kill-after`, and Layer 3 + Layer 6 share a 90s p95. So the
guard **never invokes `cargo` or `pnpm`**. It is pure `jq` + `sed` over text, and it signals by exit
code + `VIOLATION:` / `ERROR:` lines only — **no `STATUS=` line**, since `tee_collect_statuses`
would swallow it and an unrecognised enum maps to exit 2 (OPS-3). Shape follows
`validate-slug-class-sync.sh` and sources `scripts/guards/common.sh` rather than re-copying helpers
(@dry-reviewer #6). Bash, not a `dt-guard` subcommand: ADR-0034 makes `dt-guard` the direction of
travel for *policy/AST* guards over source trees; this is a JSON-vs-declaration value pin with no AST
walk, and the four in-tree precedents for that exact shape are all bash. Stated so the choice is
visible rather than defaulted.

**Layer 3 — `scripts/guards/simple/validate-frame-vectors.sh`:**

| # | Assertion | Vacuity bail token |
|---|-----------|--------------------|
| g1 | Vectors file exists, parses, carries `_non_production`, `schema_version` | `vectors-missing` / `vectors-unparseable` / `banner-missing` |
| g2 | `max_payload_bytes` == `MAX_PAYLOAD_BYTES` extracted from `frame.rs` | `rust-const-not-found` |
| g3 | `header_version` == `PROTOCOL_VERSION` (TODO item (a)'s "with drift-guard coverage" clause) | `rust-version-const-not-found` |
| g4 | `wire_constants.legal_flag_mask` == OR of the three flag constants in `frame.rs` | `rust-flag-consts-not-found` |
| g5 | `{token \| layer == "codec" && has_vector == true}` **EQUALS** the `=> "token"` set in the `reject_reasons!` block — as sets, mechanically, **equality not subset** (@dry-reviewer, @observability O1/O2). Subset stays green when a variant is *deleted* from the macro; only equality makes omission unrepresentable across the language boundary, which is what the macro achieves within Rust. No fourth hardcoded list anywhere. **The `has_vector == true` scoping is @security's correction, not a weakening**: `ALL_REJECT_REASONS` is exactly the byte-determined codec rejects, so equality still holds over the whole set it can speak for, and deleting a variant from the macro still reds. Without the scoping, adding the codec-family-but-state-dependent token of §7c would break g5 and the reflex fix would be to relax equality to subset — trading a real check for an accommodation | `reject-macro-not-found` |
| g5b | Independently of g5: every `has_vector: true` token appears in ≥1 row, every `has_vector: false` token appears in **exactly zero** rows. Orthogonal to g5 and does not substitute for it — a codec-layer token could be present with a row while a *different* codec-layer token vanished from both the enum and the array | `zero-rows` |
| g6 | Every token matches `^[a-z][a-z0-9_]*$`, unique, stable order (@observability O5) | — |
| g7 | Per row: `signed_input_hex == aead_aad_hex || payload_hex`; `aead_aad_hex` is a **strict, strictly-shorter prefix** of `signed_input_hex`; neither covers the relay region or the signature (checked against `offsets`) (@security #5) | `zero-rows` |
| g8 | Per row: `frame_hex` length == `signature_offset + 64`; all hex fields lowercase, unprefixed, even-length | — |
| g9 | `naive_contiguous_signed_input_hex` **differs** from `signed_input_hex` wherever present (D5) | — |
| g10 | Vendored external file SHA-256 == the digest recorded in `PROVENANCE.md` (@security #3) | `external-vectors-missing` / `provenance-digest-not-found` |
| g11 | `crates/media-vector-gen/Cargo.toml` exists **and** the path is in `[workspace] members` — the mechanical anti-deletion tripwire (OPS-7, @security #9) | `generator-crate-missing` |
| g12 | **Runs iff `gated_by.typescript == true`** — inert *by declaration*, not by absence, which is why it is implemented now rather than at task 15 (an absent check never starts running when a flag flips). **Two checks, different claims.** *Negative:* declared TS sites must not hardcode `1048576`, `1_048_576` or `0x0005`. *Positive:* each site must demonstrably **reference the vectors** (`frame-v2.vectors`, `legal_flag_mask`, `max_payload_bytes` or `cipher_suite_id`). The positive form exists because **`legal_flag_mask` is `7`, which is not expressible as a banned literal** — and it is the one constant `frame.rs:263-265` promises in terms, so a negative-only check would have left the single explicitly-made promise unenforced. It also catches `1024 * 1024`, which satisfies the negative check. Sites are **enumerated**, never globbed | `ts-site-not-found` |
| g7b | **`derived` must describe the bytes in `frame_hex`.** g7 checks the derived block's internal consistency and its *length*; it never compared it to the frame, so a row could carry another frame's spans and pass everything (@security finding J — six rows did). Rows whose frame does not decode legitimately keep the base frame's spans, so the exemption is **declared per row** via `expected.derived_describes`, **never scoped by `kind`**: a future roundtrip row that diverges reds rather than being covered by its neighbours' exemption. A stale declaration on a matching row also reds | `derived-note-missing` |
| g13 | **Exhaustive partition of the `crypto` block, plus a pin on which half each field lands in** (@semantic-guard's correction, hardened by @security finding H). The file declares `key_material_fields` and `derived_public_fields`; g13 asserts (i) together they **partition the `crypto` block exactly** — a member in neither, or in both, is a hard fail; (ii) every `key_material_fields` member matches the declared synthetic-pattern predicate; (iii) **a name-segment vocabulary predicate** — any `crypto` member whose name segments include `key`, `secret`, `seed` or `kek` **must** be in `key_material_fields`, using `segments()` at `crates/dt-guard/src/ts_retained_credentials.rs:198` because `\bkey\b` cannot see inside `transmit_key_hex`; and (iv) an **explicit pin** that `kek_hex`, `transmit_key_hex` and `identity_private_seed_hex` are `key_material_fields` members. @security #10 + seed condition 3: no `dt-guard` module scans `.json`, so without this the only control is authorship discipline, which is not a control | `crypto-field-unclassified` / `key-field-misclassified` / `key-field-pattern-undeclared` / `crypto-block-missing` |
| g14 | **Declared** codec list `["rust", "typescript"]` — never discovered. For each: `gated_by[codec] == true` ⇒ its conformance marker must exist or the guard **fails naming the missing side**; `gated_by[codec] == false` ⇒ unmissable non-suppressible banner naming the ungated codec and its closer (story task 15), plus an asserted matching `docs/TODO.md` entry. Hard-fails on drift in **either** direction: flag flipped without conformance present, or conformance present while the flag says false. Also asserts `cross_language_property_established == (all gated_by values true)` | `codec-list-undeclared` / `gated-by-key-missing` / `todo-tracking-entry-missing` |
| g16 | **Crypto-token spelling** (@observability; scope corrected by @paired-client). Each crypto/key-layer token declares `fleet_spelling` (`null`, or the **label it shares**) and optionally `spec_anchor` (a path whose text must contain the token verbatim). g16 asserts: non-null `fleet_spelling` ⇒ the exact string is **present** in `docs/observability/metrics/**` and `crates/*/src/observability/`; `null` ⇒ **absent** from them; non-null `spec_anchor` ⇒ the exact string is **present** at that path. **Typo-catching coverage is 2 of 5 — see the honesty table below; the absence arm cannot catch a typo and is not claimed to** | `fleet-catalog-not-found` / `fleet-spelling-unverifiable` / `spec-anchor-not-found` |
| g15 | **External-row selector anti-vacuity** — a *different* failure surface from g10's digest, so it is its own row: a valid digest must not be allowed to imply a valid selection. `manifest.json`'s `cipher_suites` is non-empty; **each enumerated suite individually** yields ≥ its pinned minimum row count in the vendored file — per-suite, never "≥1 row overall", because the aggregate form passes when 0x0005 vanishes and 0x0004 survives, which is exactly the degradation the anchor exists to catch | `manifest-missing` / `suite-list-empty` / `suite-yields-zero-rows` |

Every bail prints `ERROR: PRECONDITION [<token>]` and exits nonzero — **no skip branch anywhere**
(@observability O10, OPS-2). Failure output names the vector **row**, the **field**, the byte offset
of the first difference, both lengths, and the exact regenerate command (OPS-4, @observability O9);
for key- and ciphertext-valued fields — **including `identity_private_seed_hex`** — it prints offset
and lengths **only**, never the hex, so key-shaped material never reaches CI logs, which travel
further than the repo because they get pasted into tickets (@observability O8, tightened).

**Two near-identical-looking rules that must not be harmonised later, in either direction**
(@observability). (i) O9's first-differing-**offset** in guard output is *build-time* diagnosis over
*synthetic fixtures*, no attacker in the loop, and the gate needs it to stay diagnosable — it is
**not** the same surface as a *runtime metric* revealing which byte range disagreed, which would be
attacker-observable and is barred. (ii) The `reject_reason` **regex + uniqueness** checks (O5) bound
*well-formedness*; a **membership** check deliberately does **not** land here, because on the vectors
side the array **is** the set — a membership check would compare the array to itself and report green
forever, which is worse than no check because it reads as coverage. Emission-side membership is
@observability's at tasks 19/22.

**Finding H: the partition guarded the declaration's shape, not its content.** @security caught that
(i) and (ii) alone are defeated by a one-line edit in the file being guarded: move `kek_hex` from
`key_material_fields` into `derived_public_fields` and the partition still holds, the synthetic-pattern
check is skipped, and g13 stays green — which is exactly the regenerate-with-real-CSPRNG-material case
it exists to catch. Same failure class as `^[a-z][a-z0-9_]*$` bounding a token's *form* rather than
the set's *membership*.

Taking **both** remedies they offered rather than either, because each covers what the other misses.
(iii) the **segment vocabulary predicate** generalises to fields that do not exist yet, which the
explicit pin cannot; (iv) the **explicit pin** survives a rename, which the vocabulary predicate
cannot — renaming `kek_hex` to `meeting_wrap_hex` defeats (iii) alone. Together the open-world
property survives for genuinely new fields while today's three become un-reclassifiable. Checked the
scope: `identity_public_hex` and `plaintext_hex` have no matching segment, and `sframe_key_hex` lives
in the `derived` block rather than `crypto`, so it is outside g13 entirely and the predicate does not
demand synthetic structure from a real derived value.

**Why the five crypto tokens get g16 now rather than a reviewer until task 15** (@observability).
Their finding is right and sharper than it first reads: between task 8 and task 15 those five
(`signature_invalid`, `unwrap_failed`, `decrypt_failed`, `replay_detected`, `wrap_key_id_mismatch`)
are covered by the regex, uniqueness, and `has_vector` → ≥1 row — **all three of which pass happily
on a misspelling**. `signature_invald` is well-formed, unique, appears in rows, and ships green. And
it does not self-correct at 15, because @paired-client's exhaustiveness check reads tokens *from the
vectors*, so TS adopts the typo and both sides agree on it. That is the task prompt's own *"vectors
validate a shared error instead of catching it"* — applied to the token spelling rather than to the
bytes, and by then frozen in a GSA path and cited by the taxonomy row, so correcting it becomes a
wire-vocabulary change needing co-sign.

I checked the fleet rather than reasoning about it. **`signature_invalid` has a live spelling** —
`docs/observability/metrics/gc-service.md:408` and
`crates/mh-service/src/observability/metrics.rs:230,363`, as a `failure_reason` value on JWT
validation. The other four have **zero** occurrences anywhere in the tree: genuinely new, no
comparator, nothing a guard could check them against.

So the split is not "guard vs reviewer" but "which of the two applies to which token", and the file
declares it per token rather than leaving it to a habit. `fleet_spelling` naming the **shared label**
rather than a bare boolean is what keeps `reuses_fleet_spelling: true` from degenerating into "grep
found it somewhere": the reuse of `signature_invalid` across JWT validation and Ed25519 frame
verification is deliberate and the meanings are compatible ("a signature did not verify"), and that
claim should be reviewable at the declaration. Note `signature_invalid` is **not** one of
`codec.rs`'s eight, so its "no downstream layer may reuse one of these tokens with a different
meaning" rule is not engaged.

**g16's typo coverage is 2 of 5, and the file says so rather than letting a reader infer the class is
handled** (@paired-client's correction, which I had glossed). The absence arm is satisfied *more
easily* by a typo than by the correct spelling — `signature_invald` has zero fleet occurrences, so
`absent` holds — meaning that for `fleet_spelling: null` tokens g16 is **structurally incapable** of
firing on the defect it was built for. That is not a flaw in g16 (absence is the right assertion for a
token with no anchor); it is a coverage boundary, and it falls exactly where a typo is *most* likely,
on the four genuinely-new tokens:

| Token | Anchor | Typo caught? | Control |
|---|---|---|---|
| `signature_invalid` | `fleet_spelling` → live fleet value | Yes — presence fails | g16 |
| `wrap_key_id_mismatch` | `spec_anchor` → the token verbatim in the story file (`:332`) | Yes — presence fails | g16 |
| `decrypt_failed` | none | No | Gate-3 human reading; @observability's task-22 catalog cross-reference |
| `unwrap_failed` | none | No | as above |
| `replay_detected` | none | No | as above |

`wrap_key_id_mismatch` is covered because the task prompt mandates the spelling verbatim, which is an
external referent the other three lack — @paired-client's find. `spec-anchor-not-found` is its own bail
token so "the token was misspelled" and "the anchor moved" read differently at triage; the anchor is a
manifest prompt, so the second is a live possibility rather than a theoretical one.

**The three uncovered tokens rest on human review, named as such**, with @observability committed to
Gate-3 character-by-character reading and the task-22 catalog cross-reference as the mechanical closer.

### 7c. `decode_reject` — resolved by R-25's own recorded correction, with one sub-clause still open

Confirming @security's finding I surfaced a **third** divergence they did not name; I escalated it as a
design question, and @observability then found the answer already on disk. Recording the sequence
because the escalation was right and the resolution beat it.

Task 19's prompt (`:420`) instructs `decode_reject` as a single `reason` value. **R-25 at line 53 of
the same file forbids that literal**, verbatim:

> *"**'bucket' names a FAMILY of `reason` values, not one collapsed `decode_reject` label** — the
> codec's per-token values are emitted individually, so `sum by(reason)` stays comparable between MH
> and the client and R-31's `unknown_version` stays individually visible; recorded 2026-08-31 during
> task 2, stated by client and observability, because the literal one-label reading contradicts R-31."*

So the file carries the correction at `:53` and the uncorrected instruction at `:420`. **Settled: the
eight structural tokens are emitted individually**, and my "complete R-25 label space" claim is correct
as written — it was task 19's prompt that was wrong, not the array. I had read ADR-0036 §4's
*"counted in the decode-reject bucket"* as possible support for the umbrella; R-25 records that exact
reading being litigated and rejected on 2026-08-31, which is why escalating beat picking. Collapsing
would have reverted a settled correction, destroyed R-31's only lever (`unknown_version` staying
individually visible is how a version-skewed rollback is detected), broken `sum by(reason)`
comparability with MH, and instantiated the kind-vs-reason conflation O4 exists to prevent —
`decode_reject` is a **`kind`** value in the vectors file, not a `reason` value.

**Still genuinely open, and I am not inventing it from the protocol chair.** The prompt's trailing
clause — *"a frame with neither key nor wrap is a protocol violation counted as `decode_reject`, not a
third key reason"* — names a condition with **no token in either vocabulary**. It is not a codec
reject: the frame decodes cleanly, and every one of the eight is a parse failure. ADR-0036 §4 requires
it be distinguishable from `no_kek_for_generation` / `no_roster_entry` because *"firing means an
invariant broke rather than a state the system passes through"*.

I proposed `unknown_key_id` with `layer: "key"`. **@security corrected the layer and their ADR reading
beats mine**: §4:8's *"not a third reason"* means not a third **key-material** reason alongside
`no_kek_for_generation` / `no_roster_entry`, and the very next clause puts it *"in the decode-reject
bucket alongside unknown flag bits"* — which under R-25's family reading means it wants its own token
**in the codec family**, not a key-layer token and not a collapse into an existing one. Their
detection argument is the stronger half: sustained firing indicates **a sender omitting the
key-bearing flag**, a distinguishable misbehaviour that folding into a shared token destroys.

**Settled: `no_transmit_key`, `layer: "codec"`, `drops_frame: true`, `has_vector: false`,
`fleet_spelling: null`.** Name from @observability, layer from @security, each on the argument that
belongs to them.

*Name* — @observability's objection to `unknown_key_id` holds and is layer-independent: `unknown_` is
already load-bearing for parse (`unknown_version`), so the prefix currently tells an operator which
layer they are in; and `unknown_key_id` reads as *the key id was malformed*, which is structural,
when the meaning is *I hold no material for it*, which is availability.

*Layer* — I had proposed `"key"` and @observability ratified that, but both of us were reading §4:8's
*"not a third reason"* without its antecedent. The preceding paragraph enumerates *"Two reasons
exist"* — no KEK, no roster entry — so "not a third reason" means **not a third member of that pair**,
and the same sentence then places it *"in the decode-reject bucket alongside unknown flag bits."*
The ADR assigns the family explicitly. @security's reading is textual and beats both of ours.

*Distinctness* — @observability's point that it survives either answer is right and is the durable
part: three distinct missing things (no cached transmit key / no identity key for the sender / no KEK
for the generation), three distinct remedies.

**@observability's objection 2 — answered by the ADR, and their instinct to ask was correct.** They
asked whether the token is transient at join (a subscriber arriving mid-stream before the next
key-bearing frame), which would make *"firing means an invariant broke"* false and produce an alert
that pages on every join wave, gets muted in a week, and is silent when it finally means what the doc
says. §4 names exactly that mechanism and excludes it: *"Under the cadence above it cannot occur:
audio carries the key on every frame, and a video subscriber enters a group at its start (§7), whose
first frame is the keyframe and is never discardable. The only paths to it are defects — a sender
omitting the flag, a reader starting mid-stream."* So **defect-only is correct**, the §4 cadence is
the excluding mechanism they hypothesised, and *"a reader starting mid-stream"* is named by the ADR as
a defect rather than a normal join. Their conditional stands: the answer goes in the token's
`operator_meaning` **with the cadence cited**, so an operator reads *why* it cannot fire normally
rather than being asserted at — and so that if the cadence is ever relaxed, the justification visibly
depends on it.

`has_vector: false` is forced — the condition is receiver-state-dependent (*is this key id known?*),
so no byte string determines it. That is what makes it **codec-family but not a Rust codec variant**,
and hence the g5 scoping above. @security flagged the trap: without scoping g5 in advance, this token
breaks the equality and the reflex fix is to relax equality to subset, trading a real check for an
accommodation. Fixing g5 *before* the token exists is the difference.

**Fallback per @team-lead**: if it is not settled cheaply, the clause ships as an explicit open
pointer naming task 19 and the completeness claim is qualified rather than asserted — an invented
token in a GSA path is worse than a named gap. What it must not do is ship saying `decode_reject`.

**@security's ruling on the substance, recorded so it is not over-applied.** No security objection to
per-token codec-layer emission: the codec rejects describe failures to parse a **public, documented
wire format**, so an attacker holding the spec — which is in this repository — can determine offline
which structural check any byte string fails. Distinguishing them discloses nothing they lack, and
there is no oracle surface at the codec layer. **The aggregate-only condition binds at the crypto
layer only**: `signature_invalid` / `decrypt_failed` / `unwrap_failed` distinguish *which key material
was wrong*, which is attacker-relevant — that is why O3's split matters and why those three must never
be joined with participant, meeting or stream identity. One rule, two layers, different consequences.

**The task-19 prompt edit is therefore three changes, not two**: `replay` → `replay_detected`;
`decrypt_failed` → `decrypt_failed` + `unwrap_failed` with O3's one-liner; and `decode_reject` → the
eight structural tokens emitted individually, **citing R-25's own correction** so the next reader sees
why rather than reading it as a fourth opinion.

**Why this lands in task 8 rather than task 19.** @observability accepted the completeness framing over
their own scoped-subset proposal *because* it was machine-checked. Landing it against a prompt that
guarantees three divergences would make it the weaker option after all — a false completeness claim
rather than an advertised subset, which is the exact failure the framing was chosen to avoid. And g16
structurally cannot catch any of the three: it asserts against fleet **emission sites**, and a manifest
prompt is not one — it is the text that *creates* the emission site.

**Third instance of the stale-manifest-prompt hazard on this story**, and it is a distinct family from
the R3 inertia cases: *the violation ships because someone did exactly what they were told, from text
that went stale after they were told it.* It has a named mechanism the inertia cases lack — the story
runner feeds the prompt, and nobody re-reads the requirement it was derived from. Recorded in
§Lessons Learned in that form.

**Recording the trade rather than only the residual** (@paired-client): deriving the task-15 token set
*from the vectors* is a deliberate design choice, not an oversight — it is what prevents **drift**, and
it costs typo-detection. Drift silently breaks cross-language agreement; a typo is
cosmetic-to-operational (a misspelled dashboard label, a runbook grep that finds nothing). The trade is
right and would be made again. It is written down because the next person may otherwise rely on a check
that is structurally blind to a whole defect class.

**g14's closure criterion carries @observability's clause**: closing the TS leg requires not only
that the mapping is structurally exhaustive but that the crypto tokens match their fleet spellings.
Exhaustiveness proves every error has an arm; it cannot prove the token on that arm is the right
string, because both sides read it from the same file. "The TS leg is live" is necessary and not
sufficient for this specific risk.

**Why g13 partitions rather than pattern-matches field names.** My first design discovered key
material *by suffix*, and @semantic-guard showed it cannot work in either direction. The file is full
of `_hex` fields that are **derived crypto outputs which MUST look random** — `signed_input_hex`,
`aead_aad_hex`, `sframe_nonce_hex`, `sframe_key_hex`, `sframe_salt_hex`, `wrap_nonce_hex`,
`wrap_aad_hex`, `frame_hex`, `signature_hex`, `naive_contiguous_signed_input_hex` — so a `_hex`
predicate would demand synthetic structure from them and **defeat the external gate**. Narrowing the
suffix instead silently misses a future secret-bearing field, which is the exact real-material-committed
failure the check exists to catch. And the three secrets (`kek_hex`, `transmit_key_hex`,
`identity_private_seed_hex`) share no clean suffix that excludes `identity_public_hex` and
`plaintext_hex`.

Scoping to the `crypto` block alone also fails on its own: `identity_public_hex` is an Ed25519 public
key **derived from the seed**, so it is necessarily random-looking and cannot satisfy a synthetic
pattern. And a bare declared `key_material_fields` list is vacuous — a future secret is protected only
if someone remembers to add it.

The **partition** closes both holes. Adding any field to the `crypto` block without classifying it is
a hard fail (`crypto-field-unclassified`), so omission is impossible; and the only escape from the
pattern check is to explicitly declare a field public, which is a visible, reviewable act rather than
a silence. Structural, SSoT-in-the-file, and it survives adding a row.

**Key material must not leak from the generator's own failure paths either** (@security, via
@observability). Reusing the in-tree pattern rather than inventing one:
`crates/media-protocol/tests/reject_reasons.rs::decode_error_display_leaks_no_buffer_bytes` renders
the failure surface and asserts key sentinels are absent. The likeliest leak here is **not** a `Debug`
derive — it is an `assert_eq!` in my own tests dumping both sides of a key comparison, which is
exactly the shape that pattern catches.

**Runtime and shape** (OPS-11): each section is a **single `jq` program** emitting all its
comparisons at once, not one spawn per field per row — process spawns dominate a bash+jq guard and
per-field spawning is the shape that walks into a 124 two stories from now as the row count grows.
`command -v jq` and `command -v sha256sum` are preconditions with their own named bail tokens
(OPS-10): `jq` is in `infra/devloop/Dockerfile:31` and on `ubuntu-latest`, but **no existing guard
uses it**, so an image regression would otherwise surface as exit 127 classified as a violation,
pointing triage at the diff. The **measured** wall-clock of the guard and its self-test is recorded in
§Devloop Verification Steps and in the §6.3 runbook row, following the `ts-no-retained-credentials`
precedent (*"~9ms over 62 files"*) — §6.3's timeout triage turns on whether runtime *jumped* versus
its normal cost, and without a recorded baseline there is no data behind that fork.

**This guard is full-tree and always-run**, so g10 (external digest), g11 (workspace membership), g13
(key-field pattern) and g14 (TODO anchor) can red a devloop whose diff never touched them (OPS-12).
The §6.3 row carries the `no-retained-credentials` sentence verbatim: *"…it can fail your devloop for
a violation your diff did not introduce. That is intended… Do not go hunting your own change for the
cause — read the file:line."* **g14 matches an explicit stable marker**, never prose — the
`<!-- slug-class-sync: … -->` idiom in `validate-slug-class-sync.sh` is the in-tree precedent — so
the failure message reads *"the tracking anchor moved or was removed"* rather than *"TODO entry
missing"*, and `/close-story` ticking the entry `[x]` **satisfies** g14 rather than breaking it (the
marker, not the checkbox, is what g14 reads). Flipping `gated_by.typescript` to `true` is what
retires the marker requirement, under the same hard-fail-on-drift as the flag itself.

**g14's self-test is enumerated rather than left to "drives every branch"** (@test's condition,
carried by @paired-client). A guard can enforce both directions correctly while its self-test
exercises one, and a later refactor then breaks the unexercised arm with the self-test still green —
"an untested arm and a passing arm are indistinguishable", the shape this round has now hit four
times. So the fixtures are named, not counted:

| # | Fixture | Expected |
|---|---------|----------|
| 1 | `gated_by.typescript: true`, conformance marker **absent** | **RED**, naming the missing side |
| 2 | conformance marker **present**, `gated_by.typescript: false` | **RED**, naming the stale flag |
| 3 | `gated_by.typescript: false`, marker absent (the **task-8 state**) | **PASS** + `^WARN ` banner emitted |
| 4 | `gated_by.typescript: true`, marker present (the **task-15 state**) | **PASS**, no banner |
| 5 | `cross_language_property_established: true` while any `gated_by` value is false | **RED** |
| 6 | fixture 3's banner asserted against `^WARN ` via `assert_marker` | pins **leg (i)** — that g14 emits the prefix, which @test's synthetic stub in `run-guards.test.sh` cannot reach |

Fixtures 3 and 4 are not padding: without 3 the "passes with a banner" behaviour is unpinned and a
later reader "fixing" g14 to exit nonzero breaks nothing visible, and without 4 the guard could red
on the task-15 state and nobody learns until task 15.

`scripts/guards/validate-frame-vectors.test.sh` drives every FAIL and every vacuity branch against
synthetic trees via a `DEVLOOP_TEST`-gated root seam, wired explicitly in `layer3.sh` alongside the
five existing precedents. It lives one directory **above** `guards/simple/` because `run-guards.sh`'s
`find -name '*.sh'` would otherwise auto-run it as a guard.

**Layer 4 — Rust (`scripts/lang/rust/test.sh` → `cargo test`), in `crates/media-vector-gen/tests/`:**

- `external_sframe_gate.rs` — feeds the **external** row's `base_key`/`kid`/`ctr` into **our**
  schedule and compares against the **external** expected `sframe_secret`/`sframe_key`/`sframe_salt`/
  `nonce`, and the external `pt`/`aad`/`ct` through our GCM call. Nothing our generator produced
  appears in this test (@security #4a). Runs for both 0x0005 and 0x0004.
- `kid_packer.rs` — `sender_id = 65536` → `Err` (with the comment: aliasing to 0 is a KID collision,
  hence (key, nonce) reuse, hence authentication-key recovery under GCM, not merely confidentiality
  loss); `stream = 256` → `Err`; `generation = 2^40` → `Err`, adjacent to the all-max row's
  `2^40 − 1` success (@security #7).
- `vectors_are_current.rs` — regenerates in memory and byte-compares against the committed file, so
  a hand-edited vectors file reds (@test #7) and so does a broken generator.
- `rust_codec_conformance.rs` — **proves it consumed every row** (@test): asserts
  `rows_consumed == rows.len()`, and dispatches on `kind` with **no catch-all** — an unrecognised
  `kind` fails the test rather than falling through green, which is the row-nothing-reads failure
  mode. The TS mirror is built the same way at task 15. It decodes every row with `media-protocol`
  and asserts the decoded
  breakdown, the spans, and the `reject_reason` for every reject row. **This lives in the generator
  crate, not in `media-protocol`**, so `media-protocol` gains no `serde_json`/`hex` dev-dependency
  and the GSA crate stays untouched by the harness.

**Layer 4 — TypeScript (`scripts/lang/ts/test.sh`), @paired-client:** the mirror-image conformance
plus the external gate consumed from the same single vendored path. Nothing lands only in
`scripts/lang/proto/test.sh`, which is a registered intentional-gap placeholder (@test #4).

### 6. Vendoring

**One copy, one path, verbatim** (@dry-reviewer #7, @security #3): the whole upstream
`test-vectors.json` is committed byte-for-byte at
`proto/test-vectors/external/sframe-wg/test-vectors.json`, so the recorded SHA-256 covers *verbatim
upstream bytes* and there is no extraction transform to audit. Deliberately **not** hand-subsetted to
the two GCM rows, though @paired-client proposed it: a subset needs a committed extraction script to
stay auditable, and the file is 35 KB. `manifest.json`'s selector carries **anti-vacuity** (@dry-reviewer, @paired-client): the guard fails
on an empty suite list, fails if **any** enumerated suite yields zero rows — per-suite, never "≥1 row
overall", because the aggregate form passes when 0x0005 vanishes and 0x0004 survives, which is
precisely the degradation the anchor exists to catch — and pins a minimum row count per suite.

**The upstream-row selector is declared as data, not written twice**
(@dry-reviewer #3, @paired-client): `proto/test-vectors/external/sframe-wg/manifest.json` carries
`cipher_suites: [4, 5]` and *both* harnesses read it. A selector is not one of the three protected
computations, so two hand-written predicates could gate against different subsets while both report
green. It lives in the external directory rather than in `frame-v2.vectors.json` because it describes
the *external* artifact's scope, and folding it into our file would blur the provenance split this
task is built on.

**Digest recomputation is owned by the guard (g10), not by either harness** — @paired-client drops
theirs. One recomputation, one failure site, and it runs in Layer 3 where it costs nothing.

The 0x0004 row is present as the **SHA-256 contrast case** that makes "PRK is 64 bytes, hash is
SHA-512" falsifiable rather than self-referential. It is **not** the precedence-ladder fallback:
0x0005 is present, so nothing is degraded and nothing is declared as such. `PROVENANCE.md` records the URL, the **full 40-char** SHA, the upstream path, the upstream
file's SHA-256, the retrieval date, the upstream licence text **verbatim** (see B4), the precedence
outcome and the ladder not taken, and — per @paired-client — an explicit note that the 289-row
`header` array gates nothing of ours, so a later reader does not read its presence as relevance or
its absence as oversight. No network access at validation time anywhere (OPS-5a).

### 7. What the external gate does and does not cover — stated in the file, not only here

Recorded in the file as an explicit **in/out list**, not prose (@security constraint B) — an
under-stated scope invites a redundant gate later, an over-stated one invites someone to skip a real
one.

**In scope (four things, one more than the task text implies):** the HKDF-SHA512 key schedule
(extract, both expands, the exact label bytes), **the nonce derivation**, the AES-256-GCM primitive,
and the 128-bit tag length. The nonce is @security's addition and it is right: RFC 9605's nonce is
`salt XOR BE12(counter)` and ours is `salt XOR left-zero-padded BE32(stream_sequence)` — zero-extending
a BE32 into 12 bytes **is** BE12 of the same integer, so the constructions are identical, and the
external row's `ctr = 17767` exceeds 16 bits, so multi-byte placement is genuinely exercised.

**Out of scope — and this half matters more:** our SFrame clear-header shape, our AAD span, our
signed range, our detached-tag split, our KEK unwrap, and the entire frame header. It gates **none**
of our framing: our SFrame object is `key_id(8) || tag(16) || ct`, RFC 9605's is `config || KID || CTR`;
our AAD is the publisher region, RFC 9605's is `sframe_header || metadata`; the external rows have no
relay region, no signature, no wrap and no extensions. So **the AAD span, the signed range and the
detached-tag split are gated only by the Rust generator against the TS codec** — the irreplaceable
three, undiminished by the external anchor. This paragraph goes in `proto/test-vectors/README.md` so
nobody reads across.

### 7a. The one-TypeScript-implementation invariant (@team-lead R3, verbatim)

> Exactly one TypeScript implementation of the three protected computations (AAD span, signed range,
> detached-tag split) exists in the tree; it is production code; and the drift guard drives that code.

Written here because the default is that nobody states it. Its operational consequence for task 8:
**no span arithmetic anywhere in @paired-client's harness** — that constraint is what stops the
harness quietly becoming a second TypeScript implementation, which would make the drift guard compare
two artifacts neither of which is the shipped codec. Task 8's `packages/**` footprint is therefore the
external anchor check (key schedule + GCM primitive) and an independent cross-check of my generator's
row values, and nothing else.

### 7b. The TypeScript leg is absent at task 8, and the guard must say so in data

@paired-client has withdrawn the TS codec from task 8 (the task-15 manifest prompt at story line 385
specifies the whole TS stack **including conformance against every row of these vectors**, so it is
gated, just later). @team-lead owns that boundary; I am not contesting it. But it has a consequence I
own: **at the end of task 8 the cross-language property is not established.** The vectors are gated by
the Rust reference on one side and by nothing on the other.

That is acceptable as sequencing. What is not acceptable is a guard that reports green because it
iterated over a set containing one codec. `git show 8bba6da` records this repo's own generalised
lesson: *"any assertion whose cost scales with the artifact it guards weakens on precisely the inputs
that most need checking, and does so quietly, because the failure mode is an empty result rather than
a wrong one."* A conformance guard that loops over *discovered* codecs has exactly that shape — absent
TS means zero iterations means pass.

So the claim lives in **data**, not in a skip branch, and the guard fails on any inconsistency between
the claim and reality (guard check **g14**):

- The vectors file carries `gated_by: {"rust": true, "typescript": false}` and
  `cross_language_property_established: false`. **The file never claims a green it does not have.**
- The guard holds an **explicit declared codec list** — `["rust", "typescript"]` — not a discovered
  one. For each: if `gated_by[codec] == true`, its conformance marker must exist or the guard
  **fails naming the missing side**. If `gated_by[codec] == false`, the guard prints an unmissable,
  non-suppressible banner naming the ungated codec and its **named closer (story task 15)**, and
  asserts a matching tracking entry exists in `docs/TODO.md`.
- Flipping `gated_by.typescript` to `true` without the TS conformance present is a hard fail; landing
  TS conformance while the flag still says `false` is also a hard fail. The two cannot drift.

**The banner's `WARN ` prefix is load-bearing, not formatting** (@operations + @test, settled anchor
`^WARN ` at column 1, no `NOTICE`). `WARN ` is already the repo's established family —
`scripts/layer-all.sh` emits `WARN BUDGET_BREACH` / `WARN BUDGET_TOTAL_BREACH` /
`WARN FAIL_FAST_OVERRIDE_IGNORED` with the comment at `:138` saying the token is shaped so
`grep 'WARN '` finds the family, and dt-guard's `WARN dt-guard auxiliary skip:` is the same shape; a
second severity token with no other emitter would be a fork, not a feature. The codec name **and** the
closer go on the **first** line, because `run-guards.sh` surfaces only `WARN `-prefixed lines and
applies `head -5`. `run-guards.sh:194`'s failure arm needs no change — `^WARN ` is a strict subset of
its unanchored pattern, so the line surfaces on both arms and there is no divergence-by-exit-code trap.

**The channel has three parts and each self-test pins the leg it lives with.** (i) g14 emits the
prefix; (ii) the runner's exit-0 grep anchors on it; (iii) @test's stub-guard self-test pins the
runner. Because @test's stub is synthetic, (iii) pins (ii) **only** — so if g14's wording is later
edited and loses the prefix, every test stays green and the banner silently vanishes again, the
original bug wearing a different hat, landing on whoever's devloop is next rather than on whoever
broke it. So `validate-frame-vectors.test.sh` asserts g14's banner matches `^WARN ` as one more case
in a file that has to drive every branch anyway, using `assert_marker` from
`scripts/lang/_test_helpers.sh` on both sides so the two assertions read as one contract rather than
two similar-looking greps. The guard header says the prefix is load-bearing alongside the `8bba6da`
reasoning, because a future maintainer "cleaning up" the prefix is the realistic way this breaks.

**The loud-skip path is non-suppressible** (@security): no env var, no `--allow-missing-codec`, no
config key that turns it into a normal pass. The moment a suppression switch exists, the fastest route
to a green pipeline is to set it — and this guard's whole value is that the fastest route to green is
landing task 15. Same reasoning as the audit-suppression governance in
`scripts/audit-suppressions-check.sh` (fail-secure to zero, drift-checked).

**Three non-obvious invariants live in g14, each of which fails silently if "simplified", so the
guard header carries them as a short numbered block rather than prose** (@operations) — a maintainer
should see three separate things not to do:

1. **The codec list is declared, never discovered.** A runner that iterates over the codecs it finds
   reports success when it finds none; the absent side is indistinguishable from the passing side
   because both produce zero failures. (`8bba6da` reasoning, below.)
2. **The `WARN ` prefix is load-bearing, not formatting.** Without it `run-guards.sh`'s exit-0 arm
   discards the banner and g14 becomes a silent pass — the exact failure class point 1 cites.
3. **Passing-with-a-banner is deliberate; do not make it exit nonzero until task 15.** This is the
   one a well-intentioned reader will get wrong, because g14 visibly *knows* the cross-language
   property is unestablished and passes anyway. Redding every devloop until task 15 would make
   **deleting the guard** the fastest route to green, which **inverts** @security's non-suppressibility
   argument rather than serving it.

**The guard's header comment carries the *why*, not just the rule**, so a future maintainer does not
"simplify" the declared list into a directory scan. @security supplied the text; three things are kept
verbatim: the literal `8bba6da` reference so the reader can go read the incident, the "empty result
rather than a wrong one" sentence because that is the actual mechanism, and the explicit naming of
**story task 15 as the closer** — so an unsatisfied `typescript` entry reads as a scheduled obligation
rather than as a bug in the guard, which is what would otherwise get it deleted.

That is the difference between a deferral that closes and one that does not (@dry-reviewer #2,
@security finding F). Silence is the one option not available.

**Stated explicitly and carried into §Accepted Deferrals: ADR-0036 Assumption 4 is NOT discharged by
this task.** The ADR says the header "is not frozen in code until they have run"; the insider-forgery
regression only proves the signature layer load-bearing once a **second** implementation rejects the
frame. Authoring row 15 is not running the test. Nothing in this devloop output may imply the header
is frozen at the end of task 8 — that would be a false green at the ADR level, and the kind that
surfaces months later.

### 8. Sequencing

1. Land the ADR edits first (@security reviews the on-disk hunks at Gate 1; ADR-0027's table is
   applied manually by a reviewer, so it must exist on disk before Gate 3, not be assumed).
2. `frame.rs` TODO items (b)(c)(d) — **gated on B1**.
3. Generator crate + external gate test; verify the gate passes before generating a single row.
4. Generate the vectors; Rust conformance.
5. Guard + self-test + `layer3.sh` + runbook rows.
6. Hand the file to @paired-client (already in flight — shape agreed above) and converge only on
   *disagreements found by the vectors*, never by showing each other code.
7. `docs/TODO.md` closures.

---

## Cross-Boundary Approvals (trailers for the final commit)

Recorded verbatim as each owner earns it, so the final commit carries the exact text they approved
rather than a paraphrase. A trailer is added **only** after that owner has read the hunk on disk —
never against a plan.

**Two rules, both learned the hard way in this devloop.**

1. **The reason clause must name the AUTHORITY, not just the what** (ADR-0024 §6.7; worked example at
   `.claude/skills/devloop/review-protocol.md:223` — *"The reason clause … names the authority
   (ADR-0011), not just the what"*). A trailer that describes the edit is a changelog line, not an
   attestation. This matters most exactly where ownership was not derivable from the path: the trailer
   is then the one durable artifact in `git log` recording **why that owner was entitled to co-sign**.
2. **The implementer does not author a co-signer's attestation.** Where the text below is the
   owner's own words it is marked RATIFIED; where I drafted it, it is marked **DRAFT — NOT
   RATIFIED** and must be replaced by the owner's text before commit. An implementer-written trailer
   signed by an owner who never wrote it is the same authority inversion as an implementer
   self-assigning ownership, one layer down. I made that error once here (see infrastructure below)
   and it is marked rather than silently repaired, because the two remaining drafts have the identical
   defect and would otherwise read as ratified.

**RATIFIED — @observability's own wording, supplied verbatim in their Gate-3 verdict and transcribed
unaltered.** (Unmarked in the first draft of this section: the convention above left one of its own
four subjects without a status, which @infrastructure flagged — a rule whose first application leaves
a subject unmarked reads as "marking is optional." Marked now; the omission is recorded rather than
quietly filled, since an unmarked trailer is exactly the ambiguity the convention exists to remove.)

```
Approved-Cross-Boundary: observability reason-label row points at the vectors array per label-taxonomy.md's own shared-label rule; no token restated
```

- **observability** — earned 2026-09-02 against thirteen pre-registered criteria. Hunk scope: the
  shared-label table row and the `## Frame reject reason` section in
  `docs/observability/label-taxonomy.md`.
- **security** — **earned 2026-09-02, RESOLVED-FIXED (findings A–K, all fixed).** Hunk scope:
  `adr-0027-approved-crypto.md`; `adr-0036-media-flow.md` amendment row **and** §4 wrap-nonce;
  `label-taxonomy.md` as co-owner; `crates/media-vector-gen/**` call sites. Read as bytes on disk,
  not against the plan. Their criterion 5 — `diff` of the vendored `test-vectors.json` against bytes
  they fetched from upstream **before any vendored copy existed** — is the one structurally
  judgment-free check in this entire review, and it came back clean (exit 0, sha256
  `b8d35efd…8ffdec1`). They flagged, unprompted, that on the `label-taxonomy.md` hunk they are a
  **confirmation, not a second independent read**, because @observability's item-7 result reached
  them first.

**RATIFIED — @security's own wording, supplied verbatim in their Gate-3 verdict and transcribed
unaltered by @team-lead.** Transcribed by the Lead rather than the implementer, per rule 2 above:
the text arrived directly from the owner and was copied without edit.

```
Approved-Cross-Boundary: security ADR-0027 key-derivation row broadened to HKDF-SHA512 per ADR-0024 §6.4 path-independent crypto-primitive GSA; security is ADR-0027's named primary owner
Approved-Cross-Boundary: security ADR-0036 amendment-table row and §4 wrap-nonce clarification; records the derivation ADR-0036 §4 left unpinned, per ADR-0024 §6.4
Approved-Cross-Boundary: security label-taxonomy.md `reason` row and Frame reject reason section as co-owner per that file's own ownership header
Approved-Cross-Boundary: security crates/media-vector-gen ADR-0027 primitive call sites (HKDF_SHA512, HMAC_SHA512, AES-256-GCM, Ed25519) per ADR-0024 §6.4 "wherever referenced"
```
- **media-handler** — earned 2026-09-02, **CLEAR, no findings**. Hunk scope:
  `crates/media-protocol/src/frame.rs` (TODO items b/c/d) and
  `crates/media-protocol/tests/frame_properties.rs`. Verified independently against the working
  tree: `KEY_ID_BYTES` unchanged at 8 so there is no wire shift and the relay path is untouched;
  `MAX_PAYLOAD_BYTES` home, value and derivation unchanged with g2's extraction confirmed
  **non-vacuous**; `codec.rs` **byte-identical**, so MH's emittable label space is exactly
  unchanged; no crypto dependency added to `media-protocol` and the generator edge is
  one-directional.

**RATIFIED — @media-handler's own wording, supplied 2026-09-02 and transcribed verbatim.**

```
Approved-Cross-Boundary: media-handler crates/media-protocol/** co-ownership per ADR-0024 §6.4 (cross-boundary-ownership.yaml:34); verified no wire-format shift and codec.rs byte-identical so MH's ALL_REJECT_REASONS emittable reject-reason label space is unchanged
```

- **test** (RATIFIED) and **operations** (RATIFIED) — separate trailers over the `scripts/**` rows: `validate-frame-vectors.sh`, its
  `.test.sh`, `layer3.sh` wiring, and the `run-guards.sh` exit-0 arm + its self-test pin. @test
  verified the runner-leg pin observed-failing; @operations ran the same in an isolated copy and
  filed F1–F6, all resolved. Trailer to land once both send their verdict:

**Two trailers, not one.** My draft composed test and operations into a single line; @test ruled they
must be separate, because the two co-signs cover different concerns and name different authorities —
test's is ADR-0034 test-reliability (guards require self-tests driving their FAIL branches),
operations' is runner behaviour / pipeline budget / STATUS-emission semantics. A composed trailer
would have had one signature standing behind an authority its signer never claimed.

**RATIFIED — @test's own wording, supplied 2026-09-02 and transcribed verbatim.**

```
Approved-Cross-Boundary: test ADR-0034 test-reliability self-test ownership; frame-vector guard FAIL/vacuity branches pinned by self-tests I ran, run-guards exit-0 WARN pin observed failing on revert
```

Note how the observed-failing claim is **bounded to what its author personally observed**: the
self-tests they ran, plus the *one* pin they reverted and watched go red. It makes no blanket claim,
and in particular says nothing about @infrastructure's second `NOTE ` assertion, which is an unfailed
pin. That is the corrected shape of the overstatement my draft carried.

**RATIFIED — @operations' own wording, supplied 2026-09-02 and transcribed verbatim.** Every clause
is something they personally ran, and it deliberately asserts **nothing** about @infrastructure's
`NOTE ` pin — that is another owner's, and an unfailed one. That restraint is the corrected shape of
the over-claim in my own draft.

```
Approved-Cross-Boundary: operations ADR-0033 §4 fast-tier budget + §6 STATUS/exit-code contract; exit-0 ^WARN arm verified surfacing end-to-end from a passing guard and its pin observed failing on revert, ^WARN a strict subset of the failure-arm pattern so lanes cannot diverge by exit code, pipefail sentinel preserved, Layer 3+6 re-measured at ~52s within the 90s budget
```

- **infrastructure** — earned 2026-09-02, co-signed **unconditionally**, for
  `crates/dt-guard/src/no_insecure_browser_flags.rs` (F2 `WARN`→`NOTE`) and
  `crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs`. **The Lead ruled that infrastructure owns
  `crates/dt-guard/**` under ADR-0034 and dispatched them as the designated owner to co-sign; they did
  not self-identify.** An earlier draft of this line said they had, which was wrong and materially so:
  a reviewer who declares themselves owner and then co-signs their own boundary is exactly the
  circularity three reviewers refused when they blocked *my* self-assignment, and recording it that
  way would have collapsed the trailer's authority to the co-signer's own say-so. The authority is the
  Lead's ruling; infrastructure is the co-signing owner under it. They also filed one finding against
  their own co-signed hunk — the `NOTE ` contract had no test — which is fixed. **Precisely: the
  *first* assertion (`NOTE ` prefix present) was observed failing — regress the emission and it reds.
  The *second* (`no ^WARN ` line on a passing allowlist-only run) is defense-in-depth against a
  different regression — a stray `^WARN ` from another producer in this guard's output — and both
  live in one `#[test]`, so reverting to `WARN ` trips the first and never reaches it. By this
  devloop's own standing rule that is an UNFAILED PIN, and it is recorded as one rather than
  described as observed-failing. Observing it would need a fixture emitting a stray `^WARN `;
  @infrastructure recorded it without asking for it at the final gate, and I agree that is the right
  trade — but the claim is now scoped to what was actually demonstrated.** They then **retracted
  their own recommended follow-up** after reading the manifest (see `docs/TODO.md`).
  **Trailer text below is theirs verbatim, RATIFIED.** An earlier draft of this section carried a
  trailer I had written for them — it named no authority (ADR-0034 absent, runbook §6.3.1 absent) and
  re-asserted the "observed-failing" overstatement that the prose twelve lines above had already
  scoped. Replaced with their exact text on their instruction. Recorded rather than quietly fixed
  because it is the self-identification error one layer down: there I let an owner appear to author
  their own mandate, here I authored an owner's attestation. Both invert who is speaking.

**RATIFIED — @infrastructure's own wording, supplied 2026-09-02 and transcribed verbatim after they
rejected the version I had drafted for them (see the bullet above).**

```
Approved-Cross-Boundary: infrastructure ADR-0034 dt-guard pipeline ownership; NOTE demotion preserves runbook 6.3.1 WARN=coverage-hole semantics
```



## ADR Edits — flagged for human attention at story close

Three ADR edits land in this changeset. **They do not all have the same authority, and a human closing
the story should see that distinction without reconstructing it** (@team-lead R2, condition 2).

| # | Edit | Authority |
|---|------|-----------|
| 1 | `adr-0027-approved-crypto.md` — key-derivation row broadened to **HKDF-SHA256 and HKDF-SHA512** (`ring::hkdf::HKDF_SHA512`, forced by SFrame ciphersuite 0x0005) | **Pre-approved in the story file.** Named in the task prompt as approved and "LANDED BY THIS TASK". @security confirms the hunk at Gate 1 and Gate 3 (path-independent GSA). |
| 2 | `adr-0036-media-flow.md` — amendment-table row `ADR-0027: no amendment needed` replaced accordingly | **Pre-approved in the story file.** Same provenance as #1. |
| 3 | `adr-0036-media-flow.md` §4 — one sentence recording the **KEK-wrap nonce derivation** (`0x00000000 \|\| key_id_be8`, key id as AAD), adjacent to "the wrap nonce is derived, not carried" | **A Lead scope call made in a headless run (R2).** Not in the task prompt. Worded as a **clarification, not a decision**: it *records* the derivation the story contract already pins in two places (story §Security "frozen crypto contract", and the task-15 manifest prompt) and decides nothing new — which is what makes it in-scope for a devloop rather than a `/debate`. Raised independently by @paired-client and @security; the argument for landing it is that a normative derivation whose only homes are a story line and a manifest prompt is one archival away from having none, and `docs/TODO.md` records an incident on **this very story** where a stale manifest prompt contradicted the shipped contract. |

## Pre-Work

None.

---

## Implementation Summary

Landed in three commits (gate last and alone, per §Rollback Procedure).

**The deliverable.** `proto/test-vectors/frame-v2.vectors.json` — **23 rows**, generated by
`crates/media-vector-gen` and committed. Every `has_vector: true` reject token has at least one row;
every `has_vector: false` token has exactly zero. The four AAD-span combinations, the all-max key id,
the wrap-determinism pair, the wrap-binding row, the insider forgery, all eight structural rejects,
the salience-out-of-range row, the tamper pair, both crypto-reject rows, the replay row, and the
composed `full_frame` row.

**The external anchor is on its primary branch and verified before a single row was generated.**
`crates/media-vector-gen/tests/external_sframe_gate.rs` drives the vendored 0x0005 row's
`base_key`/`kid`/`ctr` through our schedule and matches the upstream `sframe_secret` (64 bytes),
`sframe_key`, `sframe_salt`, `nonce`, and the AES-256-GCM `ct` byte for byte. Nothing our generator
produced appears in that test. The 0x0004 contrast terminates at key-schedule output and never
reaches a seal call, alongside an assertion that the AEAD **rejects** every key length except 32.

**The three protected computations** live in `spec.rs`, computed arithmetically from ADR-0036 §2/§3/§4
and cross-asserted against `media-protocol`'s parser-walk ranges — three independent derivations
pinned to one value. Construction order is **typestate, not a comment**: `PublisherRegion` is only
reachable through `PublisherRegionDraft::finalize(payload_len)` and `aad()` exists only on the
finalized type, so slicing the AAD before `payload_length` is written **does not typecheck**.
`tests/smoke_roundtrip.rs` then opens a generated frame using **only wire bytes** — decode, verify,
unwrap, decrypt — which is the check a single-language round-trip structurally cannot perform.

**The guard** is 16 checks (g1–g16), pure `jq` + `sed`, **~0.6s**, no `cargo`, no `pnpm`, no `STATUS=`
line. Its self-test drives **33 cases**: every vacuity precondition, every violation, and g14's four
arms — including the two pass states, because the obvious "fixes" are wrong in opposite directions.

**Two guard bugs the self-test caught before review**, both silent-pass shapes:
`gated_by.typescript` read through jq's `//` operator returned the fallback (jq treats `false` as
absent), and the TODO marker was matched as a substring, so a **renamed** anchor still passed.

**Three findings fixed at their source rather than suppressed.** `rust_secrets` flagged
`token: "no_transmit_key".to_string()`; restructured through a helper taking `&str` so the pattern is
gone rather than ignored — a `guard:ignore` on a file that genuinely handles key material is a bad
precedent. `validate-todo-tracking` flagged the §Accepted Deferrals table; converted to pointer
bullets, with each body now living with its owner. `media-protocol`'s own
`cast_possible_truncation` deny caught a `KEY_ID_BYTES as u32` in my new const assertion; the bit
constants became `usize` so the assertion needs no cast at all.

---

## Files Modified

**Created**
- `proto/test-vectors/frame-v2.vectors.json` — the SSoT (23 rows)
- `proto/test-vectors/README.md` — directory contract, escalation rule, provenance split
- `proto/test-vectors/external/sframe-wg/{test-vectors.json,manifest.json,PROVENANCE.md}` — verbatim upstream at `025d568`, the shared selector, and provenance
- `crates/media-vector-gen/` — NON-PRODUCTION generator: `lib`, `spec`, `schedule`, `crypto`, `kid`, `fixtures`, `rows`, `mutate`, `inventory`, `json`, `error`, `main`
- `crates/media-vector-gen/tests/` — `external_sframe_gate`, `kid_packer`, `smoke_roundtrip`, `vectors_are_current`, `rust_codec_conformance`
- `scripts/guards/simple/validate-frame-vectors.sh` + `scripts/guards/validate-frame-vectors.test.sh`

**Modified**
- `crates/media-protocol/src/frame.rs` — TODO (b) `PROTOCOL_VERSION` anchor, (c) `Debug` safety invariant, (d) `KEY_ID_*_BITS` named constants + fill assertion
- `crates/media-protocol/tests/frame_properties.rs` — third `MAX_PAYLOAD_BYTES` home deleted
- `Cargo.toml` — workspace member
- `scripts/guards/run-guards.sh` — exit-0 arm surfaces `^WARN ` (OPS-8a); `scripts/layer3.sh` — self-test wired
- `docs/decisions/adr-0027-approved-crypto.md`, `docs/decisions/adr-0036-media-flow.md` — three ADR edits
- `docs/observability/label-taxonomy.md` — `reason` row + `## Frame reject reason`
- `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` — task-15 (six) and task-19 (three) prompt corrections
- `docs/TODO.md`, `docs/runbooks/devloop-validation.md`

---

## Devloop Verification Steps

| Check | Result |
|---|---|
| `cargo test -p media-vector-gen -p media-protocol` | green — external gate, KID packer, smoke round-trip, freshness, conformance |
| `cargo clippy --workspace --all-targets -- -D warnings` (**the exact Layer-5 command**) | clean. My first pass used `--all-targets` **without** `-D warnings`, so 28 pedantic warnings never failed and I reported ready against a weaker command than the gate runs |
| `./scripts/guards/run-guards.sh` | no `FAILED`; `WARN frame-vectors:` delivered from the **passing** guard, proving the OPS-8a channel |
| `./scripts/guards/validate-frame-vectors.test.sh` | 33/33 |
| `validate-cross-boundary-classification.sh` | `STATUS=OK` |
| `validate-story-manifest.sh` | `STATUS=OK` after both prompt edits |
| Regenerate twice, `git diff` | empty — deterministic |
| Every `g\d+` in this document resolves to a guard-check row | verified |

---

## Divergences (paired client ↔ protocol cross-check)

This section is present whether or not anything is in it. An empty one states
"we looked and found nothing"; an absent one is indistinguishable from nobody
having checked. It records divergences **even after they are resolved**,
because a resolved divergence is a *located ambiguity in the specification*,
and silently fixing it discards the signal that should become an ADR
clarification (@security's escalation clause 3).

### How the check was run, and the limits of its evidence

@paired-client derived every span and crypto value independently from
ADR-0036 §2/§3/§4 **before** the generator's rows existed — the derivations are
timestamped in the Gate-1 message log — then compared. Derive first, compare
second. **356 field comparisons across all 23 rows.**

**The evidence is weaker than that number sounds, and this is the honest
statement of it.** Per @team-lead ruling R3, no span arithmetic persists in
`packages/**`, so the cross-check ran as a scratch script outside the
repository (`/tmp/xcheck/crosscheck.mjs`). **It cannot be re-run from the tree
and it cannot be audited.** A Gate 3 reviewer reading "agreed on every row" has
to take @paired-client's word for it. This is the best independence available
in this task attached to the weakest available evidence, and the asymmetry is
deliberate — the alternative (committing a second TypeScript implementation of
the three protected computations) is the thing the invariant in §7a forbids.

It also protects only the vectors *as authored at task 8*. It provides no
protection against later drift in either direction; that is the drift guard's
job, and its TypeScript leg does not exist until task 15 (§7b).

### Agreements

The three protected computations agree on every row where they are well
defined: all four AAD-span combinations including the publisher-region span
arithmetic; the **discontiguous** signed range `publisher ‖ payload` skipping
the six relay bytes; `naive_contiguous_signed_input_hex` differing from the
real signed input on every row; and the detached-tag split
`key_id(8) ‖ tag(16) ‖ ciphertext`. Also agreeing: the HKDF-SHA512 schedule and
both labels, `sframe_key` / `sframe_salt`, PRK = 64 bytes, the SFrame nonce
`salt XOR BE12(stream_sequence)`, the KEK unwrap
(nonce `0x00000000 ‖ key_id_be8`, AAD = the eight KID bytes) recovering
`transmit_key_hex` exactly, Ed25519 verification, the all-max KID round trip,
wrap determinism across two sequences, `wrap_binding`, `full_frame`, and all
four crypto-reject rows.

### D-0 — reviewer error, not a generator divergence (recorded because it is evidence *for* the file)

@paired-client initially scored `insider_forgery_other_sender_key_id` as
divergent, having verified against the row's `crypto.identity_public_hex`
(Bob's — the forger's) rather than `expected.verify_against_public_hex`
(Alice's — the sender the frame claims, resolved from `key_id.sender_id`).

The row's own note anticipates exactly this: *"Verifying against the row's own
crypto block would SUCCEED and invert the test."* The documentation caught a
real reviewer mistake in the field. Recorded because defensive annotation that
demonstrably works is worth more evidence than annotation that merely reads
well.

### D-1 — `decode_reject` rows carried `offsets`/`derived` blocks unbound to their own `frame_hex` (8 rows) — ESCALATED, **RESOLVED** by `g7b`

**As first reported, this finding was wrong in two ways.** Both corrections came
from @dry-reviewer at Gate 3 and were verified in the tree, not accepted on
report. They are recorded rather than silently overwritten, because the *shape*
of the errors is the reusable part.

- **The count was the wrong set.** Reported as six; the affected set is
  **eight**. Six was the number that *diverged in the cross-check*.
  `decode_reject_truncated` and `decode_reject_trailing_bytes` agreed only
  because their mutations happen not to touch the publisher region — luck, not
  a different property. Reporting the *detectable* subset as though it were the
  *affected* set is a bad habit in an escalation: an auditor sweeping "the six"
  would have missed precisely the two rows whose problem was invisible.
- **"Unasserted" was flatly wrong.** `validate-frame-vectors.sh:165` runs
  `.vectors[] as $r` with **no `kind` filter**, so g7 already enforced internal
  consistency on reject rows. The real gap was narrower: the blocks were
  **self-consistent but unbound to the row's own bytes** — they could have
  described a different frame entirely. The binding was absent *by design*: g8
  (the frame-length identity) carries the guard's only
  `select(.kind != "decode_reject")`, because a reject row is mutated precisely
  so it is not a complete frame. That exclusion is load-bearing. "Unasserted"
  would have sent someone to write a check that already existed while missing
  the one that did not.

**Resolution — `g7b`, which is better than the fix this reviewer proposed.**
The proposal was to *omit* the blocks on reject rows. What landed instead
compares `frame_hex[0 .. publisher_region_len*2]` against `derived.aead_aad_hex`
on every row; requires `expected.derived_describes = "base_frame_before_mutation"`
where they differ; reds on a **stale** declaration in the opposite direction;
and exempts **per row, never by `kind`**, so a future `roundtrip` row that
diverges reds instead of being covered by its neighbours' exemption. All eight
rows declare it, the required `_comment_derived_on_reject_rows` note is present,
and the guard runs clean apart from the expected TypeScript-leg WARN. Declaring
the convention keeps genuinely useful information while making it checkable
rather than inferred; omission would have discarded it.

**Residual (minor, for @security's call).** Two of the eight declarations are
**vacuous**: `decode_reject_truncated` and `decode_reject_trailing_bytes`
declare `base_frame_before_mutation` while their spans *do* match `frame_hex`.
`g7b`'s staleness arm carries `and ($r.kind != "decode_reject")`, so it cannot
catch them — the guard's own comment states the exemption is "per row, never
scoped by `kind`", which holds for the divergence direction but not the
staleness one.

### D-2 — `derived.sframe_nonce_hex` carries two meanings that 22 of 23 rows cannot distinguish — ESCALATED, **STILL OPEN**

On `tamper_publisher_region` (mutation `frame[STREAM_SEQUENCE_OFFSET] ^= 0x01`)
the pinned nonce is the **seal-time** nonce, from the original sequence;
recomputing from the shipped `frame_hex` gives a different value. Both are
meaningful — the sequence is the nonce input *and* is signed *and* sits in the
AAD, which is the row's point — but the field means "the nonce used when
sealing", not "the nonce a receiver computes from these bytes", and nothing
disambiguates it because the two coincide on every other row.

**`g7b` does not reach this.** The row is `kind: verify_reject`, declares no
`derived_describes`, and passes — because `g7b` compares only `aead_aad_hex`,
which on this row correctly reflects the **post**-mutation bytes. So within one
row two derived fields follow opposite conventions, and the new declaration
mechanism is scoped to the AAD span. A task-15 TypeScript conformance asserting
`sframe_nonce_hex == computeNonce(frame_hex)` still fails here. Small fix:
extend `derived_describes` to name which fields it covers, or pin the seal-time
nonce under a distinct name.

### The pattern behind D-1, D-2 and this reviewer's own `hex.ts` defect

Credited to @dry-reviewer, who named it: `derived.*_hex` and `offsets.*` are two
encodings of the same span information, so a row where neither is bound to the
bytes carries **two mutually-agreeing descriptions of a frame that may not
exist — and their agreement with each other is exactly what makes the absence of
an external anchor invisible.**

That is structurally identical to this reviewer's `hex.ts` defect (a comment
asserting sole ownership that was correct about every copy it could see) and to
the `PROVENANCE.md` control that was documented before it was built. Three
instances in one task of *correctness between copies concealing a missing
referent*. It generalises past duplicated helpers: it is the question to ask of
any pair of derived fields.

Both D-1 and D-2 went to @team-lead and @security **before** any discussion with
@implementer, per the binding escalation rule. D-1 in particular looks obvious
enough to "just fix", which is the case the rule exists for.

## Code Review Results

Ten reviewers. **No ESCALATED verdict survived to Gate 3** — @security escalated twice (findings J
then K) and both flipped to RESOLVED-FIXED on re-verification, which is the escalation mechanism
working rather than failing.

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 11 (A–K) | 11 | 0 | Escalated on J, then on K when J's fix proved span-scoped rather than class-scoped. Criterion 5 (byte-diff against upstream bytes fetched *before* the vendoring existed) is the one structurally judgment-free check in the review. Self-declared as a **confirmation, not a second independent read** on the taxonomy hunk. |
| Test | RESOLVED-DEFERRED | 3 (T-1..T-3) | 1 | 0 (2 spun out) | Linchpin held from Gate 1 to commit: g14 must red in **both** directions with a two-arm self-test. Found T-1, the OPS-8 pin that was dead code, and personally watched it go red on revert. D-1/D-2/D-3 accepted and mechanically tracked. |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | Found the `drops_frame` omission on the **second** pass, in exactly the place they had pre-registered as their highest-risk reading — a sentence that satisfied their own criterion as written. Referred the RESOLVED-FIXED/DEFERRED classification upward rather than take the reading convenient to them. |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | Caught a new path entering the changeset *after* their verdict and amended rather than let a stale "table matches the changeset" claim stand. |
| DRY | RESOLVED-DEFERRED | 4 | 4 | 0 (1 extraction filed) | Found g12 specified-but-absent. Recorded the inversion **and its boundary** so a future DRY pass cannot collapse the deliberate Rust↔TS duplication — including a fourth pair (the key schedules) their own Gate-1 list had missed. |
| Operations | RESOLVED-DEFERRED | 6 (F1–F6) | 6 | 0 | OPS-8 is the find of the devloop: the g14 banner, D-1's entire runtime mitigation, had **no delivery channel**. Fixing it surfaced a pre-existing R-17 coverage hole nobody knew about. F6 was their own fix widening the diff into an unclassified path — "a reviewer-requested fix is still a diff." |
| Semantic Guard | CLEAR (native SAFE) | 0 | — | — | Re-confirmed against the grown diff rather than letting a snapshot verdict stand. Verified @security's finding-H fix to g13 rather than accepting it. |
| Media Handler | CLEAR | 0 | — | — | Conditional GSA co-owner added at Gate 1 (B1/R1). Verified `codec.rs` **byte-identical** — the change that would have cost MH most, and invisible unless someone looked. |
| Infrastructure | RESOLVED-FIXED | 1 | 1 | 0 | Conditional owner added at Gate 3 for `crates/dt-guard/**`. **Retracted their own recommendation** after reading the file they had recommended editing, and refused a rewritten trailer — which surfaced that three owner attestations were implementer-authored. |
| Paired Client | RESOLVED-FIXED | 2 (own files) | 2 | 0 | 356 field comparisons across 23 rows; the three protected computations agree everywhere they are well defined. Reversed their own scope position after reading task 15's manifest prompt. |

**Gate-3 note on independence.** @observability and @security each declared, unprompted and against
their own interest, that @security's read of the taxonomy hunk is a **confirmation rather than a
second independent read** — @observability's item-7 result reached them first. Two reviewers each
disclosing the same contamination from opposite sides is a better record than either report alone,
and the agreement between them should be weighted as one read plus a confirmation.

---

## Divergences

Where the shipped code differs from what §Planning specified, and why. Cited by
`packages/sdk-core/src/media/frame/__tests__/sframe-key-schedule.ts` and by @dry-reviewer's Gate 3
read; the section existed in neither until both pointed at it, which is itself the finding.

| # | Plan said | Shipped | Why |
|---|-----------|---------|-----|
| V-1 | The guard "sources `scripts/guards/common.sh` rather than re-copying helpers" (§5, @dry-reviewer #6). | It sources nothing and defines its own `fail`/`precondition`/counter. | @code-reviewer found the `source` line loaded a file **none of whose helpers the guard calls**, under `2>/dev/null \|\| true` so its disappearance would go unnoticed — dead code plus a plan claim the code contradicted. Deleted rather than converted to real reuse: a 370-line pure-`jq`/`sed` guard is genuinely self-contained, and "no dependency" is the honest state. The comment at the former site records this so it is not re-added. |
| V-2 | `g12` "runs iff `gated_by.typescript == true`", inert **by declaration**. | Present and gated exactly as specified — but it was **absent entirely** until @dry-reviewer's Gate 3 read. | Recorded as a divergence rather than quietly fixed, because the failure is the interesting part: the plan row read as covered, the tree had no `g12` and no `ts-site-not-found` token, and *nothing was red*, so nothing would have forced its construction when task 15 flipped the flag. Inert-by-declaration and inert-by-absence are indistinguishable from a green run — the same shape as the missing TS leg, one level down. Now implemented with both branches driven by the self-test. |
| V-3 | Per-row `derived` blocks describe their own row. | On rows whose frame does not decode, the **whole `derived` block** describes the base frame it was mutated from. | Correct behaviour — a frame that fails to decode has no publisher region, no meaningful AAD and no readable counter — but it was undocumented and `g7` was structurally unable to detect it (@security finding J). Now **declared per row** via `expected.derived_describes`, never scoped by `kind`, with `g7b` asserting byte correspondence on every row that does not carry the declaration. **Counts, stated precisely because they differ by question:** *six* rows diverge on the AAD span; *eight* declare the exemption, the two extra being `decode_reject_truncated` and `decode_reject_trailing_bytes`, whose mutations fall outside the publisher region so their spans match by coincidence. Their declarations are vacuous but true, which is why `g7b`'s staleness arm is `kind`-scoped — a deliberate, documented exception, not a lapse. |
| V-5 | (not planned) | `sframe_nonce_hex` is recomputed from the mutated frame, and a Rust conformance assertion pins `nonce == salt XOR BE12(stream_sequence)` on **every** decoding row. | @security finding K: `tamper_publisher_region` mutates offset 6, inside `stream_sequence`, so its counter reads `16777223` while its pinned nonce was still the one computed from `7` — the row asserted a nonce its own frame cannot produce, and any TS harness deriving the nonce from `frame.stream_sequence` would have disagreed. **This was finding J again, not a cousin**: J's fix was scoped to the AAD span, so the row was honest about its AAD and silent about its nonce — fixing the instance rather than the class. The declaration is now **block-scoped**, every frame-dependent value is recomputed, and the invariant is checked in Rust rather than left to convention. It lands on a nonce, where §2's entire argument for `stream_sequence` immutability is that rewriting it breaks decryption. |
| V-4 | §Rollback commit 3 carries the gate alone. | Unchanged, but now also carries `run-guards.test.sh`'s WARN-arm pin. | The pin constrains the `run-guards.sh` exit-0 arm, which is already in commit 3; keeping them together preserves `git revert <commit 3>` as a clean gate-only rollback. |

---

## Accepted Deferrals

- `docs/TODO.md` §Story Workflow Follow-ups — OPS-13 gate-only revert unbuildable (one-commit binding)

Pointer bullets only — each body lives with its owner, not here, so there is one home per deferral.

- **D-1 — the drift guard's TypeScript leg** (closer: story task 15). Body and enforcement contract: `docs/TODO.md` §Media Path Obligations, marker `frame-vectors-ts-leg`, which guard check g14 greps.
- **D-2 — ADR-0036 Assumption 4 is NOT discharged**; authoring the insider-forgery row is not running the test. Carried in the same `docs/TODO.md` entry as D-1, since the same task closes both.
- **D-3 — row 16's cache-state assertion** (`wrap_cached: false` observed as key-cache state). Pinned in `proto/test-vectors/frame-v2.vectors.json` at `wrap_for_different_key_id.expected`; consumed at story task 15 per its manifest prompt.
- **D-4 — none.** Blocker B1 was granted, so TODO items (a)–(d) and the `frame_properties.rs` deletion all land here rather than being re-filed.

## Rollback Procedure

**Commit ordering is part of the rollback design (OPS-13).** The durable risk in this changeset is
not the branch — it is a **new always-run Layer-3 gate that runs on every devloop forever**. If the
guard turns out to be wrong (a false positive on g13's pattern predicate, a g14 anchor that moves, a
124 on a grown row set), an operator must be able to remove the gate **without** unwinding the
vectors, the generator crate, the ADR edits or the `frame.rs` hunks. The repo has already made this
call once and `docs/runbooks/devloop-validation.md` records the reasoning: *"If the **guard itself**
is wrong, revert the guard commit — it is a separate commit by design and ordered after the client
fix precisely so this works."*

This composes with the non-suppressibility decision: with no suppression switch **by design**
(@security), a clean revert path is the *only* escape hatch, so it has to actually exist.

**NOT EXECUTED — the pipeline structurally cannot express this, and the mitigation is lost.**
@team-lead attempted the three-commit ordering at Gate 3 and it is not achievable in this repo
today. Two independent mechanisms each bind a devloop's validation to a **single commit against the
pre-devloop HEAD**:

1. **The Gate-2 pre-commit signature binding** (`scripts/lang/_gate2_binding.sh:710`) recomputes a
   signature over the whole changed set vs HEAD and requires it to equal the recorded verdict's.
   Commit 1 removed 31 files from "changed vs HEAD", so the signature for commit 2 no longer
   matched and the hook blocked with `validated but not staged` for every file commit 1 had landed.
2. **The Layer-3 scope-drift guard** (`validate-cross-boundary-scope`) then reds with
   `scope_drift_planned_untouched` for those same paths — after commit 1 they are in the plan's
   classification table but absent from the effective diff, which is exactly the drift the guard
   exists to catch.

Re-running `layer-all.sh` between commits does not resolve it: the second run reds at Layer 3 for
reason (2). The only ways through were to bypass a gate (`--no-verify`, or suppressing the guard),
which CLAUDE.md's fail-loudly convention forbids and which this devloop spent its entire review
arguing against — a gate bypassed to land the commit that adds a gate would be the sharpest possible
instance of the pattern catalogued in §Lessons Learned.

**So this landed as ONE commit, and the gate-only revert path described below does not exist.**
`git revert <the single commit>` removes the vectors, the generator, the ADR edits and the
`frame.rs` hunks along with the guard. That is a real loss of the mitigation @operations required at
Gate 3 and @team-lead granted (OPS-13): with no suppression switch **by design**, a clean gate-only
revert was meant to be the only escape hatch, and it is now unavailable. An operator who needs to
remove this always-run gate must hand-revert the four gate paths
(`scripts/guards/simple/validate-frame-vectors.sh`, `scripts/guards/validate-frame-vectors.test.sh`,
the `scripts/layer3.sh` wiring, and the `run-guards.sh` exit-0 arm + its pin) rather than revert a
commit. Recorded in `docs/TODO.md` under §Accepted Deferrals, owner operations — the underlying
one-devloop-one-commit assumption is ADR-0033/ADR-0035 territory and task-sized, not a fix to make
at a commit gate.

Commit order **as designed but not executed**:

1. Vectors, vendored external subset + provenance + manifest, generator crate, `Cargo.toml`.
2. `frame.rs` / `frame_properties.rs` (TODO (a)–(d)), the three ADR edits, `label-taxonomy.md`, the
   task-15 manifest edit, `docs/TODO.md`, runbook rows.
**Commit mechanics** (@team-lead): the devloop metadata trailers (Devloop / Specialist / Mode /
Verdicts, and the `Approved-Cross-Boundary:` trailers) goes on the **final** commit; commits 1 and 2
carry the devloop slug `2026-09-02-frame-v2-cross-language-vectors` in their body so the set greps as
one unit. That is the part that makes `git revert <commit 3>` safe to *find* six weeks later — **a
revert target nobody can locate is the same failure as not having one.**

3. **LAST, separately: the gate.** `scripts/guards/simple/validate-frame-vectors.sh`,
   `scripts/guards/validate-frame-vectors.test.sh`, `scripts/layer3.sh`, and the OPS-8(a)
   `run-guards.sh` exit-0 arm + its self-test pin.

Rollback:

1. Start commit: `1fbd87b024117eb792c4894329d687eabe8274ab`
2. `git diff 1fbd87b..HEAD`
3. **Gate only** (the expected case): `git revert <commit 3>`. Vectors, generator, ADRs and
   `frame.rs` all survive; the cross-language artifacts remain, ungated, which is a *recoverable*
   state rather than a lost one.
4. Whole changeset: `git reset --soft 1fbd87b` (or `--hard`).
5. No migrations, no infrastructure manifests in this changeset. The vendored external file is inert
   committed data, never fetched, reversible by deleting one path — so if @security rules against
   B4, g10 and g15 come out with it, which is a further argument for the ordering above.

---

## Issues Encountered & Resolutions

**Gate 2 attempt 1: Layer 5 red, 28 clippy findings, all in `crates/media-vector-gen`.**

*Cause of the disagreement.* `scripts/lang/rust/lint.sh:7` runs
`cargo clippy --workspace --all-targets -- **-D warnings**`. I ran the first two-thirds of that and
read "no errors" as clean — but 14 of the 28 were *warnings*, invisible without the promotion flag.
Verifying against an approximation of a gate command is the same shape as the four silent-reader
defects in §Lessons Learned: the check ran, produced an empty result, and an empty result looks like
a pass. **Fixed by running the exact command, and §Devloop Verification Steps now records the command
verbatim rather than a description of it.**

*The two substantive findings, fixed at source with no suppression.*

1. **`indexing`/`slicing may panic` ×11** across `mutate.rs` and `spec.rs`. Fixed properly rather
   than allowed, because the lint earns its keep precisely here: this crate computes byte spans for a
   living, so a panic *inside* span arithmetic is the failure mode it exists to detect. Every offset
   involved — `ext_length_field_range`, `signature_range`, `relay_region_offset` — is derived from a
   **decoded header**, which is exactly the attacker-influenced quantity the codec treats as a
   parsing trust boundary; indexing them put an unchecked read on the far side of a boundary the
   codec was built to hold. `mutate.rs` now routes every mutation through four checked primitives
   (`set_byte`, `xor_byte`, `set_range`, `prefix`) that return a named `GenError` carrying the
   offset, the length and the frame size. `SframeObject::key_id` and `naive_contiguous_signed_input`
   became fallible. Net: a layout change producing an out-of-range offset now names the offset
   instead of panicking into `core::slice::index`.
2. **`casting usize to u8 may truncate`** in `fixtures::pattern`. The coordinator's point stands —
   shipping a truncating cast in the crate that landed a *checked* KID packer specifically because
   masking aliases 65536 to 0 would be indefensible. The `const fn` loop needed **two** forbidden
   things (the cast and an index) with no const-callable alternative to either, so it was replaced by
   a `pattern!` macro enumerating the 32 offsets: no cast, no index, still `const`, and the
   consecutive-run property is now visible in the expansion rather than implied by a loop. A unit
   test pins that the expansion stays exactly `KEY_LEN` long so the macro cannot drift from the
   constant.

*One change made and then reverted, recorded because the reverted version was the tempting one.*
`struct_excessive_bools` on `ExtensionRegistry`'s four rejection-semantics flags. I first grouped
them into a nested struct — which **changed the published JSON shape**, altering a cross-language
contract to satisfy a style lint, and did not even silence the lint, since the nested struct still
held four `bool`s. Reverted to the flat shape with one `#[expect]` carrying the real justification:
each field is a JSON key read by name across two languages, and the lint's actual hazard (positional
construction transposing adjacent `bool`s) cannot occur at a single named construction site.

*Verification that the refactor changed nothing.* The regenerated vector file is **byte-identical**
to the pre-lint-fix output — checked by diffing against a saved copy, not by re-running the tests
that would have passed either way.

**Suppressions in the whole crate: two, both `#[expect]` with reasons** (`struct_excessive_bools`
above, and `too_many_lines` on `build_frame`, whose linear order *is* the invariant under test).
Neither is on `indexing_slicing` or `cast_possible_truncation`.

**Layer 4 `N/A` — not touched, on the coordinator's instruction and their evidence.** Both real test
lanes reported `OK`; the aggregate lands on `N/A` because proto's registered intentional-gap
placeholder folds into it. Pre-existing wrapper behaviour, aggregation logic untouched by this diff.
Worth noting it *is* another instance of the silent-reader family — an aggregate reporting `N/A`
while swallowing two `OK`s — but it is operations' to fix and widening this diff for it would be
wrong.

---

## Lessons Learned

1. **Believing a numbered check exists is the evidence it would have shipped missing.** I told
   @dry-reviewer "the anti-vacuity is in g10", believing it. g10 was the vendored-file digest check;
   the external-row selector anti-vacuity existed only in prose 50 lines away and was in no row of the
   guard-check table. The generalisation is not "check your references" — it is that **an enumerated
   table is the artifact that gets built, and a commitment living only in adjacent prose does not
   reach it.** The same defect had already occurred once in this plan, in the Cross-Boundary
   Classification table, and I fixed it there without noticing the pattern applied one section down.
   @dry-reviewer found all three (g13, g14, g15) by grepping the prose for `g\d+` and checking each
   resolved to a row — so that grep is now a step in §Devloop Verification Steps and the guard
   enumerates its checks by number, rather than the check living in a reviewer's habits.

2. **Two positions endorsed and then withdrawn, both recorded rather than quietly restated.**
   (a) I endorsed @paired-client's first proposal to land the TS codec in task 8; they reversed after
   reading task 15's manifest prompt, and the settled position is better — task 15 already mandates
   conformance against these vectors, so landing the codec here would deliberately manufacture the
   stale-manifest-prompt condition that this story's own `docs/TODO.md` incident warns about.
   (b) The g10 misattribution above. Recording the withdrawal *and its reason* costs a paragraph and
   is what lets a later reader tell a changed mind from an inconsistency.

3. **A mitigation can reproduce the defect it mitigates, one level up.** §7b cites `8bba6da` —
   *"the failure mode is an empty result rather than a wrong one"* — to justify g14's declared codec
   list. @operations then showed that g14's banner was captured into `$OUTPUT` by `run-guards.sh:217`
   and discarded by the exit-0 arm at `:146`, so the only *runtime* signal that the cross-language
   property was unestablished went to a black hole on every run. Quoting the lesson is not applying
   it: the control needed a **delivery channel**, and nobody checks whether a `WARN` line has one
   because passing guards are not read.

4. **The stale-manifest-prompt hazard is a distinct failure family, and this is its third recorded
   instance on one story** (task 10 in `docs/TODO.md`, task 15 via ruling R3, task 19 via @security's
   finding I and @observability's third divergence). Named mechanism, which is the only part that
   generalises: **the story runner feeds the prompt, so a correction recorded in the requirements
   prose has no path back into the instruction that is actually executed.** R-25 was corrected at
   line 53 on 2026-08-31; task 19's prompt at line 420 of the same file still carries the
   pre-correction text, and an implementer following their brief *correctly* reverts the correction.
   Distinct from the R3 inertia family — there the violation ships because nobody acts; here it ships
   because someone did exactly what they were told, from text that went stale after they were told
   it. Three instances is not bad luck. Both reviewers who found it did so by **grepping wider than
   the claim they were checking**, which is the only technique that works when the stale text reads
   as a deliberate instruction rather than as a near-miss.

5. **A guard's skip condition can be defeated by making a cell more informative.**
   `cross_boundary_classification.rs:139` skips a row only on an exact `Mine` match. My cells read
   `Mine (GSA proto/**)` — strictly more informative to a human, and it silently disabled the skip,
   so five genuinely-owned rows fell through to the missing-Owner rule. The fix is not less
   information but information *in the parsed column versus the prose column*: bare token in the
   classification cell, annotation in the Note. Same shape as the `^WARN ` prefix being load-bearing
   rather than formatting.

6. **Six instances of one family, and a detection ratio worth stating: a checker that silently reads
   less than it appears to.** Named together because separately they read as unrelated bugs, and the
   family is the finding. **The discriminator is strict — each failure produces an EMPTY result
   rather than a WRONG one**, and an empty result is what a passing check looks like.

   | # | Failure | Caught by |
   |---|---------|-----------|
   | 1 | `run-guards.sh` discarded `WARN ` from passing guards, so a control existed with no delivery channel | @operations (human) |
   | 2 | A markdown Note with a literal newline truncated the classification table, hiding six rows | **sibling control** (scope guard flagged the orphaned paths) |
   | 3 | Gate-2 clippy run without `-D warnings`, hiding 14 findings | **the intended gate** |
   | 4 | The OPS-8 pin sat below `report_results` and never ran | @test (human) |
   | 5 | `g12` did not exist, while the plan described it as gated-and-inert | @dry-reviewer (human) |
   | 6 | The `NOTE ` emission contract was documented but pinned by no test | @infrastructure (human) |

   **Five of six were caught by a human or a sibling control; one by the mechanism intended to catch
   it.** That ratio — not the individual bugs — is the honest measure of where this pipeline's
   coverage of the class actually sits, and it is uncomfortable in the right direction: the controls
   built to catch silent failure mostly did not, and review did.

   **The count was verified independently, and the verification changed it.** @operations arrived at
   six from their lane; I recounted from mine and reached the same six by the same discriminator,
   having first written "four, plus a fifth" — my looser version was both undercounted *and*
   miscategorised. It had lumped in jq's `//` returning a fallback on `false`, a marker matched as a
   substring, @observability's `grep -c .` counting lines, and findings J/K pinning the wrong bytes.
   **Those are a different family: they produce a WRONG result, not an empty one.** The distinction
   matters because the two need opposite remedies — a wrong result is findable by asserting the right
   value, while an empty result is findable only by proving the check ran at all, which is why "a pin
   never observed failing is not a pin" is the counter-measure for this family and not for that one.

   Two convergent counts using one discriminator is also the method: the number was worth checking
   precisely because it is the kind of number that would otherwise be transcribed.

   **The truncation case in detail**, because it is the sharpest. `markdown_table.rs:68-73` ends the
   table at the first non-`|` content line, so a Note I had wrapped across lines made **six rows
   after it invisible** to both Layer A and Layer B — and Layer B still reported `STATUS=OK`, because
   it validated only the rows it could see. The table looked correct to a human and was correct as
   prose. Two properties made it findable: the *other* guard flagged the now-unclassified paths, and
   its message named paths I knew were in the table. Had only Layer B existed, this would have
   shipped as a clean pass over a truncated table.

7. **A reviewer-requested fix is still a diff.** @operations' F2 asked me to downgrade a noisy
   `WARN ` in `crates/dt-guard/src/no_insecure_browser_flags.rs`. The fix was correct and I made it —
   and it silently widened the changeset into a path with no classification row, which
   `validate-cross-boundary-scope` caught on the next run. The reflex that fails here is treating a
   finding's remedy as belonging to the finding rather than to the diff: the classification table
   does not know a change was requested by a reviewer. Worth noting where it landed — this is the
   same guard family whose sibling was defeated hours earlier by a truncated table, and this time it
   caught the drift first try, before Gate 3.

8. **`false // "MISSING"` returns `"MISSING"` in jq**, so my g14 precondition fired on the one value
   it exists to read. Caught by the self-test, not by review. The rule that generalises: an
   "absent-or-default" idiom is unsafe on any field whose legitimate value set includes the language's
   falsy values, and `gated_by.typescript` is `false` **by design** for the whole life of this
   deferral. Use `has()`.

9. **A guard that matches a tracking marker as a substring passes on a renamed marker.** My first
   g14 grepped `frame-vectors-ts-leg`, which still matches `frame-vectors-ts-leg-MOVED`. The
   self-test's rename case caught it. Anchors intended to be *found* must be matched whole
   (`grep -qF -- '<!-- ... -->'`), or the guard degrades to "something vaguely similar exists".

10. **The implementer must not author a co-signer's attestation — and "more informative" is the
    disguise it arrives in.** I wrote @infrastructure's `Approved-Cross-Boundary` trailer myself
    instead of transcribing theirs. My version was *longer and more descriptive*, which is exactly why
    it survived my own review: it read as an improvement. It failed the one rule the reason clause
    exists to satisfy (ADR-0024 §6.7 — name the **authority**, not the what), dropping ADR-0034, the
    sole record of why infrastructure could co-sign a path with no derivable infrastructure claim. It
    also re-asserted a test-provenance overstatement in the commit message twelve lines below the
    prose where I had just retracted it — and **the commit message is the artifact that survives**,
    since most readers hit `git log` and never open this file. Two generalisations, both costly:
    *(a)* scoping a claim in one place while leaving it standing in a more durable place is not a
    retraction; *(b)* this is the self-identification error one layer down — there I let an owner
    appear to author their own mandate, here I authored an owner's attestation. Both invert who is
    speaking, and both read as deference while doing the opposite. Fixed at the class: the roster
    states the authorship rule, and the two other implementer-drafted trailers were marked
    **DRAFT — NOT RATIFIED** rather than silently repaired, because they carried the identical defect
    and would otherwise have read as earned. **All five are now RATIFIED in their owners' own words** —
    marking them is what produced them: every owner asked supplied text within one round, and each
    version differed from my draft in ways I would not have guessed. @test's and @operations' both
    bound their observed-failing claims to *what that signer personally ran*, and @operations'
    deliberately asserts nothing about @infrastructure's pin. That is the shape an attestation takes:
    a claim about the signer's own observation, not about the changeset. The corollary is the cheap
    one — **asking costs a round; composing costs the attestation's meaning.**

    A second-order instance landed inside the fix itself: @test caught that my draft had *composed*
    test and operations into one trailer, which is the same inversion but invisible, since a joint
    line reads as jointly-authored. And @infrastructure caught that the convention's own first
    application left @observability's trailer unmarked — a rule that silently repairs its own first
    violation teaches nothing.

11. **A residual documented inside an enforced-looking rule inherits the rule's credibility without
   its enforcement** (@security, via @observability, on `org_id`'s at-scale assumption). Hence the
   `[reviewer-only]` marking. This is the same defect class as the whole task, at document scale.

---

## Appendix: Task Prompt

```text
Create `proto/test-vectors/frame-v2.vectors.json` as the single source of truth for the v2 frame format across the Rust and TypeScript codecs (ADR-0028 mandated `proto/test-vectors/`, which does not exist; ADR-0036 §2 "Cross-language test vectors gate both implementations"). JSON, lowercase unprefixed even-length hex; top level carries schema_version, max_payload_bytes (SSoT for MAX_PAYLOAD_BYTES — a guard asserts the Rust const equals this and the TS codec asserts likewise; neither hardcodes independently), cipher_suite_id (0x0005, AES_256_GCM_SHA512_128, RFC 9605 §8.1), and a vectors array. Each row: name, description, kind (roundtrip, decode_ok, decode_reject, verify_reject, decrypt_reject, replay_reject, wrap_binding, full_frame), full on-wire hex, decoded field breakdown (key_id as a 16-hex-char string never a JS number, plus decomposed sender_id u16 / stream u8 / generation u40 — layout `sender_id(16) | stream(8) | generation(40)`, chosen over a 32-bit sender so that generation is effectively inexhaustible under one KEK — no generation ceiling guard or vector is needed; the generator's KID packer uses checked per-field conversion — `u16::try_from(sender_id)`, `stream < 2^8`, `generation < 2^40` — erroring rather than masking or shifting, because a masking pack aliases 65536 to 0 (KID collision → nonce reuse) and a bare shift overflows into adjacent fields; include the all-max round-trip row (0xFFFF / 0xFF / 2^40−1), and assert each row's expected sframe_nonce and KEK-unwrap AAD against the KID bytes taken from the encoded frame, not against a value re-packed from the decomposed fields — so a receive-side re-pack or decomposition bug surfaces as a crypto mismatch and unit assertions that each out-of-range value errors instead of producing a KID), a derived block pinning signed_input_hex (publisher||payload) and aead_aad_hex (publisher-region ONLY — deliberately a SUBSET of the signed range; pinning both as separate named fields is what makes the AAD-vs-signature distinction un-conflatable) and sframe_nonce_hex, a crypto block (identity_public_hex, kek_hex, transmit_key_hex, plaintext_hex) for self-contained reproducibility, and for reject rows a shared cross-language reject_reason enum asserted on BOTH sides. Author a protocol-owned Rust reference generator producing the canonical bytes: it is DELIBERATELY NON-PRODUCTION and must be labelled so in code and in its crate docs — no Rust component seals/signs/verifies/decrypts a media frame in production (MH keyless §4/§7, MC never encrypts), so it is the ONLY independent check on the TypeScript crypto, otherwise unverifiable because story 1 and story 2 both encrypt and decrypt in the same TypeScript (a self-consistent TS crypto error passes every acceptance criterion and survives until a non-TS implementation appears); a later cleanup must not delete it as dead Rust crypto. Its irreplaceable job is exactly three computations with no other independent check — the AAD span, the signed range, and the detached-tag split — and it MUST compute all three from the ADR-0036 spec, never by mirroring the TypeScript codec, or the vectors validate a shared error instead of catching it. The SFrame key schedule and GCM primitive are NOT authored by the generator: anchor them to the external sframe-wg SFrame test vectors (RFC 9605's referenced JSON) at commit 025d568, vendored into proto/test-vectors with provenance recorded; gate our HKDF key schedule against the external 0x0005 vectors as a distinct no-correlated-error gate. HKDF is SHA-512 throughout (the 0x0005 suite forces it), so the PRK is 64 bytes not 32, and the derivation is the direct GCM form with no enc_key/auth_key split — the ADR-0027 amendment adding HKDF-SHA512 is approved and is LANDED BY THIS TASK (see below) — a Gate 1/3 reviewer applies ADR-0027's table manually, so the edit must exist on disk, not be assumed; assert PRK length and hash explicitly. Precedence if 0x0005 is absent at that commit: gate against 0x0004 as a partial check of the identical code path; only if neither GCM suite has usable derivations drop to a third-party SFrame cross-check, stated explicitly rather than proceeding on independent authorship alone. Include an executable drift guard wired into the ADR-0033 pipeline that fails if either codec disagrees with the vectors; a tamper-vector pair (mutated publisher region, mutated signature, both rejected); an insider-forgery verify-reject row (a frame carrying a wrapped key under another sender's key id, authored so the forger genuinely holds the victim's transmit key and it WOULD decrypt if verification were skipped — proving the signature layer is load-bearing, ADR Assumption 4); a wrap-binding row with outcome wrap_key_id_mismatch (a correctly self-signed frame carrying a wrap for a DIFFERENT key id, which the receiver ignores rather than caches, §4); the four AAD-span combinations (key-bearing set/clear × extensions present/absent — three never appear in loopback traffic and are exactly where the two hand-written span computations can silently diverge); and one full_frame composed row asserting decode→verify→unwrap→decrypt→plaintext. Construction order is load-bearing: fill the entire publisher region including payload_length BEFORE slicing the AAD, or the frame is unopenable by any other implementation and no single-language round-trip catches it. Also land the two ADR edits this ciphersuite forces: broaden ADR-0027's key-derivation row to HKDF-SHA256 and HKDF-SHA512 (`ring::hkdf::HKDF_SHA512`, required by the SFrame ciphersuite), and replace ADR-0036's "ADR-0027: no amendment needed" amendment-table row accordingly. Pair with client (authors the TS codec + TS conformance against these vectors and locates/vendors the external 0x0005 subset), pair with security (reviews the four AAD-span rows, the two forgery rows, and the SHA-512 key-schedule gate), pair with test (fixture discipline + pipeline wiring). Provenance split — external-sourced crypto rows, our-generated framing rows, both externally gated at one pinned commit — is the strongest arrangement against correlated RFC-misreading.

```
