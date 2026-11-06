# Devloop Output: internal-proto key-material guard — exact allow-list construct (empty)

**Date**: 2026-11-06
**Task**: Story 4 task I-4 — exact (message, field) allow-list construct for check 1 of `internal-proto-no-key-material`, landing EMPTY (R-15 guard half, ADR-0039 §5)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8a0fb2aa1480d3b03f532cddc87855c91d2a883c` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — diff is dt-guard machinery; no credential-bearing type, Debug/Display over key/payload, or client credential-lifetime surface` |

---

## Task Overview

### Objective
Check 1 of `crates/dt-guard/src/internal_proto_no_key_material.rs` rejects every `bytes` field. Add an exact, enumerated (FQ message, field) allow-list consulted only by check 1, with vacuity checks (entry must resolve to an existing `bytes` field on that exact message). Ship the list empty; P1 adds the single security-owned entry.

### Scope
- **Service(s)**: none (tooling crate `dt-guard`)
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED — ADR-0039 §5 already decides the contract.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/dt-guard/src/internal_proto_no_key_material.rs` | Mine | — |
| `crates/dt-guard/src/main.rs` (subcommand doc line only) | Mine | — |
| `crates/dt-guard/src/common/proto_lex.rs` (hoisted policy-free field reader) | Mine (dt-guard machinery) | — |
| `crates/dt-guard/src/proto_per_kind_fields.rs` (ported onto the hoisted reader; policy content untouched) | Mine (machinery only; protocol owns its policy content, which does not change) | — |
| `scripts/guards/simple/validate-internal-proto-no-key-material.sh` (header comment only) | Mine | — |
| `docs/runbooks/devloop-validation.md` (guard row: new token) | Not mine, Minor-judgment | operations (reviewer on panel) |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` (one proto-lexer line) | Not mine, Mechanical (requested by owner) | dry-reviewer |

---

## Planning

### Mechanism restatement
"A policy guard's exemption list must be exact, enumerated, structurally resolved and self-validating (every entry proves it still points at what it exempts)." The sibling instance in this same module is `ALLOWED_TYPES` (already exact-FQN, enums-only, vacuity on unresolved). No wider same-owner class is opened by this task; the new construct mirrors that precedent rather than inventing a new idiom.

### Shared field reader (DRY Gate-1 ask: one home, two consumers)
Hoist `proto_per_kind_fields::field_name()` + its package/message-stack walker into `common::proto_lex`, policy-free:
`pub struct Field { line, text /* normalised statement */, message /* pkg.Outer.Inner, oneof-transparent */, oneof: Option<String>, label: Option<String>, ty /* label stripped */, name, has_options: bool }`, `pub fn field_decl(statement) -> Option<..>` (the existing recogniser, now returning label/ty/name/options instead of discarding them), `pub fn fields(tokens) -> Vec<Field>`. Port `proto_per_kind_fields::check_with` onto `fields()` (reserved count via `scopes()` non-root statements; per-file field count = `fields().len()`); its policy content, findings and FQ subjects are byte-identical. `field_recogniser_excludes_non_fields` MOVES to proto_lex (assertions kept, plus label/ty/options assertions); a `fields()` unit test pins oneof-transparent nesting paths, oneof name, package-less files and non-field scopes.

### Construct (one home: `internal_proto_no_key_material.rs`)
- `pub struct AllowedBytesField { pub message: &'static str, pub field: &'static str }` and `pub const ALLOWED_BYTES_FIELDS: &[AllowedBytesField] = &[];` inside the existing security-content block, header comment: membership = @security content; shape + evaluation = @infrastructure machinery; never string-encode a key to dodge the guard (ADR-0039 §5).
- `check_with` gains one parameter `allowed_bytes_fields: &[AllowedBytesField]`; `run()` passes the const. No CLI flag, env var, marker, or config file (S7).
- Resolution (S1, test 2): entries resolve to TOP-LEVEL messages of internal.proto only: the entry message must be exactly `dark_tower.internal.v1.<Short>` with no further dot (full-string compare, never `rsplit`) AND a field record must carry exactly that `message`. A nested-message FQN, a wrong-package FQN, or a same-named message nested elsewhere never resolves (stale finding).
- Resolution is a lookup over `proto_lex::fields()` of internal.proto: candidate fields = `f.message == entry.message && f.name == entry.field` (exact string equality; since `message` is pkg + message chain, a nested or wrong-package FQN can never equal a top-level one). A candidate with `oneof.is_some()` does not satisfy the entry (S4, stated policy: stricter). Tokenised name, never substring (S3: `x_v2`/`x_listed` never match).
- Exempt shape (S2): exactly `bytes <field> = <N>` (N any intLit the shared recogniser accepts — deliberately not narrowed further), no label, no `[options]`. `optional`, `repeated`, `map<…, bytes>`, `google.protobuf.BytesValue`, `string`, any options => the entry is a finding, never a silent pass and never a plain red only.
- Exempt-shape predicate on the `Field` record: `label.is_none() && oneof.is_none() && ty == "bytes" && !has_options` (so `map<K, bytes>` and `google.protobuf.BytesValue` fail on `ty`).
- Evaluation (S5): a VALID entry (exactly one candidate, exempt shape) yields its one `(line, text)`; in `scan_internal` the BytesField push is skipped only for a STATEMENT equal to that `(line, text)` — never a header; checks 2 and 3 run on that same statement unchanged.
- Entry validation runs unconditionally after identity markers pass (S6), new rule `Rule::KeyAllowlistInvalid` = `internal-proto-key-allowlist-invalid`, subject names the entry `(message, field)` plus the reason: message not found as a top-level message of internal.proto; field not declared directly on that message; field declared N>1 times (S3); field not plain `bytes <field> = <N>` (cites the declaration); duplicate list entry. Its own VIOLATION text (remedy: fix or remove the entry with @security; never restore by string-encoding), distinct from `internal-proto-scan-vacuous` ("restore the input" is the wrong remedy). An invalid entry exempts nothing.
- Rationale re-derived (S8): `RULE_TEXT` = ADR-0036 §4 / ADR-0039 §5 "no secret or media-key material on the internal contract; exactly allow-listed public verification keys only"; module doc (check 1 + allow-list + new token), `main.rs` subcommand doc, wrapper header comment.

### Tests (in-module, test-local lists; every negative paired with its positive on the same planted tree)
Fixture: plant `bytes listed = 90;` in `RegisterMeetingRequest`; entry `(dark_tower.internal.v1.RegisterMeetingRequest, listed)`.
1. listed passes / same tree + empty list => exactly one BytesField at the planted line.
2. same field name on a DIFFERENT top-level message => red; same name in a message NESTED in the listed message => red; `message Other { message RegisterMeetingRequest { bytes listed = 1; } }` => red (and entry still resolves to the real top-level one).
3. wrong-package entry `dark_tower.signaling.v1.RegisterMeetingRequest` => stale; nested FQN `...RegisterMeetingRequest.Inner` => stale; unknown message => stale; positive arm: real entry on unmutated planted tree => green.
4. field renamed (`listed` -> `listed_v2`) => stale AND the renamed bytes field reds; field deleted => stale only.
5. non-bytes: `string listed`, `repeated bytes listed`, `optional bytes listed`, `map<string, bytes> listed`, `google.protobuf.BytesValue listed`, `bytes listed = 90 [deprecated = true]`, oneof member => stale (plus the content red where the statement still carries bytes).
6. prefix/suffix names (`listed_v2`, `x_listed`) red; duplicate declaration => stale; duplicate list entry => stale; multi-entry: good + stale => exactly the stale finding.
7. only check 1 consults the list: a listed `bytes` statement carrying a non-allowlisted ref ... (exact shape forbids refs on the statement, so instead: listed field green + a `dark_tower.signaling.v1.MeetingKekUpdate` planted in the same message still reds check 2); rpc-header `BytesValue` stays red with the list present.
8. `guard:ignore` marker beside a listed-looking unlisted field still reds; neutered-matcher proof unchanged and non-vacuous.
9. shipped list: `the_real_tree_is_clean_and_non_vacuous` runs through `ALLOWED_BYTES_FIELDS` (real const); every existing test unchanged. **Open conflict for Lead:** @security S7 asks for `assert!(ALLOWED_BYTES_FIELDS.is_empty())` so P1's entry is a deliberate test diff; @test #5 asks not to add it (rigidity). Proposal: add it, named `shipped_key_allowlist_is_reviewed_content` with a comment that P1 replaces it with an exact-membership assertion (security owns membership, so the membership edit should be visible in a test diff).

### Gate-1 amendments (after reviewer input)
- **Token renamed** (code-reviewer 3): `internal-proto-key-allowlist-invalid` / `Rule::KeyAllowlistInvalid` — it covers wrong shape, duplicates, and missing targets, not only staleness. Subject format is fixed (observability): `(message=<FQN>, field=<name>): <reason>`, reason from the closed set `message-not-found`, `field-not-found`, `duplicate-declaration`, `non-plain-bytes`, `duplicate-entry`; the non-plain-bytes VIOLATION text prints the required shape literally `bytes <field> = <N>` (no label, no options, not a oneof member) and cites the offending declaration's line.
- **Direct-in-message only** (security Q1): `Field` carries `direct_in_message: bool` = the immediate enclosing scope is `message …` AND no non-message/non-oneof scope sits anywhere on the stack. The shared walker yields fields only from scopes whose immediate kind is message or oneof (existing per_kind behaviour: fields inside `extend`/`service`/other scopes are not fields), so an `extend Foo { bytes listed = 100; }` inside the listed message is never a candidate; resolution additionally requires `direct_in_message`. Test: planted `extend` inside RegisterMeetingRequest with the entry listed => the planted bytes statement reds and the entry is `field-not-found`; positive arm: the real direct field green. (Note for @protocol, not changed here: per_kind does not scan fields inside `extend`; recorded under Issues, it is pre-existing and changing it is per_kind policy.)
- **`Field.text`/`line`** copied verbatim from the lexer's `Token::Statement` (code-reviewer 1), doc-commented; per_kind parity quirks (a)-(d) preserved byte-identically; per_kind tests pass unmodified except the MOVED recogniser test (assertions verbatim).
- **Resolver in its own fn** (code-reviewer 6): `resolve_key_allowlist(&[Field], &[AllowedBytesField]) -> (exemptions: Vec<(usize, String)>, Vec<Finding>)`; `scan_internal` receives only exemptions. `Report.key_allowlist_entries` counter, printed on the SCOPE line (code-reviewer 5).
- **REASON precedence** (operations 2): Vacuous, then KeyAllowlistInvalid, then content tokens; stated in module doc + runbook, pinned in the rename test.
- **Runbook** (operations 1, DRY c): guard row (§6) AND §8 catalogue gain the new token as its own class; token classes are NAMED, not counted ("Five tokens" wording replaced); "no bypass" sentence gains: the allow-list is a compiled-in, security-owned const, not a runtime override.
- **Shipped-list pin** (security S7; test withdrew objection; code-reviewer leans against, defers): `assert_eq!(ALLOWED_BYTES_FIELDS, &[])` named `shipped_key_allowlist_is_reviewed_content`; doc comment: P1 rewrites it to the exact one-entry list. Security rationale on record: membership is security-owned content and must arrive as a deliberate, reviewed test diff, never only as a const edit.
- **DRY docs**: proto_lex module doc names `fields()` as the ONE field reader (consumers listed); `docs/specialist-knowledge/dry-reviewer/INDEX.md` proto-lexer line gains it; per_kind keeps no private stack and no `field_name` re-export.

### E2E
No change to `proto_ports_e2e.rs`: the binary only sees the shipped (empty) list and there is deliberately no override, so the allow path is not reachable end-to-end without adding a bypass. `a_bytes_field_fails_with_its_token` already proves check 1 with the shipped list stays red through the binary, and `both_guards_pass_on_the_real_repository` proves the real repo is green. The new token is wired via `Rule::id()` through the same `run()` path, covered in-module.

### Obligations carried to P1 (proto/ is MUST-NOT here)
P1, in the commit that adds the entry: reword `proto/dark_tower/internal/v1/internal.proto` header rule 1 ("NO `bytes` FIELD, EVER") to the re-derived rule; `docs/API_CONTRACTS.md` line ~330 and ADR-0036 §4 cross-reference to ADR-0039 §5; `docs/TODO.md` ~1667 description. While the list is empty those statements remain literally true, so they are not stale until P1.

### Runbook
`docs/runbooks/devloop-validation.md`: the §6 guard row AND the §8 symptom catalogue gain `internal-proto-key-allowlist-invalid` as its own class (allow-list maintenance, implementer lane, remedy via @security). Token classes are NAMED, not counted: the "Five tokens" wording is replaced, not incremented.

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

Lead ruling (S7 emptiness pin): ADOPTED — membership is security-owned content and security asked for the tripwire; test withdrew, code-reviewer deferred. P1 replaces it with an exact-membership assertion. Classification-sanity guard: `STATUS=OK`.

---

## Pre-Work

None

---

## Implementation Summary

- `common::proto_lex` gains the ONE field reader: `field_decl()` (the former `proto_per_kind_fields::field_name()` recogniser, now returning label / type / name / options) and `fields()` (package-prefixed, oneof-transparent message path, oneof name, `direct_in_message`; `line`/`text` copied verbatim from `Token::Statement`). Statements in non-message/non-oneof scopes (`extend`, `enum`, `service`, ...) are never fields.
- `proto_per_kind_fields` ported onto it: its private package/message stack, its `Scope` enum and `field_name()` are deleted (no re-export). `reserved` count via `scopes()` non-root statements (any depth, enums included). Subjects byte-identical; every per_kind test passes unmodified; `field_recogniser_excludes_non_fields` moved to proto_lex with every assertion verbatim (through a local `field_name` shim over `field_decl`).
- `internal_proto_no_key_material`: `AllowedBytesField { message, field }` + `ALLOWED_BYTES_FIELDS = &[]` (header: security-owned membership, infrastructure-owned shape/evaluation, never string-encode a key — ADR-0039 §5); `resolve_key_allowlist()` validates every entry every run and returns the `(line, text)` of each valid entry's single declaration; `scan_internal` skips the BytesField push only for a STATEMENT equal to one of those (headers never), checks 2 and 3 unchanged; new `Rule::KeyAllowlistInvalid` = `internal-proto-key-allowlist-invalid` with subject `(message=<FQN>, field=<name>): <reason>`, reasons `message-not-found` / `field-not-found` / `duplicate-declaration` / `non-plain-bytes` / `duplicate-entry`; REASON precedence `precedence()` = Vacuous < KeyAllowlistInvalid < content; `Report.key_allowlist_entries` on the SCOPE line; `INTERNAL_PACKAGE` const shared by the identity marker and resolution; `RULE_TEXT` + module doc re-derived.
- Review iteration 1: exemption keyed on statement IDENTITY `(scope index, statement index)` exposed by `proto_lex::Field::{scope, statement}` (was `(line, text)`, which two distinct same-line statements could share — test F1 / security SEC-1); line-0 entry findings point at `ALLOWED_BYTES_FIELDS` in this module (`location()`, also for `--explain`), declaration findings at `internal.proto:<N>` (obs F1 / ops F1); literal-string subject pins for all five reasons (obs F2); `proto_lex::top_level()` is the ONE top-level-declaration lookup, used by `enum_values`, identity markers, entry resolution and `enum_boundary` (DRY F1); rpc-header case pinned with `assert_eq!` (test F2); tautological helper assertion removed and `identity_markers` marker hoisted (code-reviewer F1/F2).
- SEC-2: `resolve_key_allowlist` cross-checks each would-be exemption id against the real `scopes()` (top-level `message <Short>` scope, statement == `(line, text)`); a mismatch exempts nothing and is `internal-proto-scan-vacuous` (machinery fault), documented in runbook class (b).
- Rationale re-derived in `main.rs` subcommand doc and the wrapper header; runbook guard row + §8 catalogue row; infrastructure + dry-reviewer INDEX pointers.

---

## Files Modified

- `crates/dt-guard/src/common/proto_lex.rs`
- `crates/dt-guard/src/proto_per_kind_fields.rs`
- `crates/dt-guard/src/internal_proto_no_key_material.rs`
- `crates/dt-guard/src/main.rs`
- `scripts/guards/simple/validate-internal-proto-no-key-material.sh`
- `docs/runbooks/devloop-validation.md`
- `docs/specialist-knowledge/infrastructure/INDEX.md`
- `docs/specialist-knowledge/dry-reviewer/INDEX.md`

Not modified (by design): `crates/dt-guard/tests/proto_ports_e2e.rs` (see Planning §E2E), `proto/` (MUST NOT).

---

## Devloop Verification Steps

- `cargo test -p dt-guard`: BEFORE 1093 passed (lib 903); AFTER 1106 passed (lib 916), 0 failed. +13 = 11 new guard tests + 2 new proto_lex tests (`field_decl_reports_label_type_and_options`, `fields_carry_oneof_transparent_paths_and_skip_other_scopes`); the recogniser test MOVED (net 0). No test deleted or weakened; `every_bytes_spelling_hits` and `the_real_tree_is_clean_and_non_vacuous` run against the shipped list; per_kind's in-module + e2e tests unchanged.
- Iteration 1 review fixes: AFTER 1111 passed (+5: same-line identity collision (test F1 / security SEC-1 a,b,c), literal subjects per reason (obs F2), location of line-0 entries (obs F1 / ops), `top_level()` unit test (DRY F1)); layer-fast green again.
- Review iteration 1b: AFTER 1112 passed (+1 `a_drifted_declaration_id_exempts_nothing`, security SEC-2); the const-location test now checks the const's first-line prefix (test F3) and `ALLOWED_BYTES_FIELDS_AT` is captured above the list's doc block so entries never move it (obs note); layer-fast green.
- Real repo: `SCOPE: … 0 key allow-list entries; 0 findings` / `STATUS=OK REASON=internal-proto-no-key-material-clean`.
- `./scripts/layer-fast.sh`: green (layers 1-6 OK). First run caught `clippy::indexing_slicing` (two slices; fixed with `take`/`get`) and `scope_drift_planned_untouched` for the e2e row (row removed — the file is untouched by design).

---

### Gate 2 (layer-all.sh)

`TOTAL_RESULT=OK` — L1 OK, L2 OK, L3 OK, L4 OK, L5 OK, L6 OK (audit SKIPPED-NO-DIFF no-dep-changes), L7 OK (env-tests-passed + browser E2E) in 1320s. First attempt was killed by the Lead's tool time limit mid-L7 (not a failure); rerun green.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | SEC-1 same-line (line,text) exemption-key bypass → statement identity; SEC-2 fields()/scopes() index drift → re-verified, fails as scan-vacuous |
| Test | RESOLVED-FIXED | 3 | 3 | 0 | F1 = SEC-1; F2 rpc-header `.any()` → exact set; F3 location test pinned `= &[]` |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | F1 `internal.proto:0` location → const location; F2 literal subjects per reason |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | F1 tautological helper assertion; F2 per-statement `format!` |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | F1 four hand-rolled top-level lookups → `proto_lex::top_level()`; extraction opportunities: none |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | F1 = obs F1 (line-0 location) |
| Semantic Guard | — | — | — | — | not spawned (no check surface in diff) |

Extend-scope question (Lead → security): check 1 scans every statement and header of every scope including `extend`; the exemption reader skipping `extend` is fail-closed. Not a key-material gap.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `8a0fb2aa1480d3b03f532cddc87855c91d2a883c`
2. `git diff 8a0fb2aa..HEAD`
3. Safe-revert unit — **Answer**: the single I-4 commit; no runtime, config or deploy surface. It includes the `proto_per_kind_fields` port onto `proto_lex::fields()`, so a revert touches a second guard (both return to their prior, equally green state). Once P1 lands, revert P1 FIRST: P1's entry uses the `AllowedBytesField` construct and would not compile without it.

---

## Issues Encountered & Resolutions

- Pre-existing, NOT changed (per_kind policy is @protocol's): `proto_per_kind_fields` does not scan field declarations inside `extend` scopes (they are not fields of the enclosing message for the shared reader). No in-tree proto uses `extend`. Changing it alters that guard's policy, so it is surfaced, not silently widened.
- Layer-fast first run: `indexing_slicing` deny + cross-boundary scope drift; both fixed in tree.

---

## Lessons Learned

1. An exemption keyed on (line, text) is not statement identity — two declarations can share a line. Key exemptions on the lexer's own statement identity and cross-check it against the scanner's numbering.
2. Location pointers for findings with no source declaration must name the real fix site (the const), never a fabricated `file:0`.
