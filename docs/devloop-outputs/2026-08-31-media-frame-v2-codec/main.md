# Devloop Output: media-protocol Frame Header v2 Codec

**Date**: 2026-08-31
**Task**: Implement version 2 of the media frame binary codec in `crates/media-protocol` per ADR-0036 §2 + Appendix — publisher/relay header split, fail-closed flag validation, no reserved bytes, zero-copy decode, derived size constants, v2 fuzz corpus.
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, HEADLESS (run-story task #2)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6b4056efe07bd15928965066de6c6baba574c5f6` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (protocol) |
| Implementing Specialist | `protocol` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Media Handler (GSA co-signer) | `media-handler` |
| Client (owner, `client/INDEX.md` hunk) | `client` |

---

## Task Overview

### Objective
Replace the v1 42-byte media frame header with the ADR-0036 §2 v2 format: a publisher region (authenticated end-to-end), a relay region (MH-rewritable, excluded from signature/AAD), the opaque SFrame-object payload, and a trailing 64-byte Ed25519 signature. Decode is zero-copy and fail-closed: every byte and bit is decoded or rejected.

### Scope
- **Service(s)**: `crates/media-protocol` (consumed by mh-service, mc-service)
- **Schema**: No
- **Cross-cutting**: Yes — `crates/media-protocol/**` is a Guarded Shared Area (ADR-0024 §6.4: SFU protocol semantics, protocol + MH co-sign). Media Handler added as a co-signing reviewer.

### Debate Decision
NOT NEEDED — ADR-0036 (Accepted) is the governing design; this task implements its §2 + Appendix wire specification. Story task #2 of `docs/user-stories/2026-08-27-hear-yourself-through-handler.md`.

---

## Cross-Boundary Classification

`crates/media-protocol/**` is a **Guarded Shared Area** (ADR-0024 §6.4 — SFU protocol
semantics, owners `[protocol, media-handler]`). I am the protocol owner; **@media-handler
co-signs**. Per §6.4 no GSA row may be `Mechanical`, and every GSA row names the co-signing
owner. Co-sign mechanism: commit trailer `Approved-Cross-Boundary: media-handler` after
@media-handler's Gate-1 confirmation, re-confirmed at Gate 3.

| Path | Classification | Owner |
|---|---|---|
| `crates/media-protocol/src/frame.rs` | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/src/codec.rs` | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/src/extensions.rs` (new) | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/src/lib.rs` | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/src/stream.rs` (deleted) | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/Cargo.toml` | Minor-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/tests/byte_coverage.rs` (new) | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/tests/reject_reasons.rs` (new) | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/tests/frame_properties.rs` (new) | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/tests/corpus_seeds.rs` (new) | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/tests/common/mod.rs` (new) | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs` | Domain-judgment (GSA co-sign) | media-handler |
| `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs` | Domain-judgment (GSA co-sign) | media-handler |
| `docs/specialist-knowledge/protocol/INDEX.md` | Mine | protocol |
| `docs/specialist-knowledge/client/INDEX.md` | Minor-judgment | client |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Minor-judgment | media-handler |
| `docs/TODO.md` | Minor-judgment | protocol |
| `docs/WEBTRANSPORT_FLOW.md` | Minor-judgment | protocol |
| `docs/FUZZING.md` | Minor-judgment | test |
| `docs/FUZZ_THIS_CHECKLIST.md` | Minor-judgment | test |
| `docs/devloop-outputs/2026-08-31-media-frame-v2-codec/main.md` | Mine | protocol |
| `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` (R-25 note, Lead edit) | Minor-judgment | observability |

**Scope drift, caught by `validate-cross-boundary-scope` at Gate 2 and corrected here.** The plan
listed three test files; five shipped. `reject_reasons.rs` and `frame_properties.rs` were split out
of what the plan described as one test surface, and I did not update the table when I split them.
Both are inside the GSA, so both are Domain-judgment with @media-handler as co-signing owner —
Mechanical is disallowed there. `docs/specialist-knowledge/media-handler/INDEX.md` is also added:
retiring `src/stream.rs` left a stale pointer in **another specialist's** index, which
`validate-knowledge-index` caught. Neither was a judgement call I made and recorded badly; both were
files I touched without going back to the table. The guards are the reason they are here rather than
discovered at Gate 3.

Notes on the non-GSA rows:
- `docs/specialist-knowledge/client/INDEX.md:61` reads "42-byte binary frame format" — a stale
  pointer once v1 retires. Reclassified from Mechanical to **Minor-judgment** at @security's Gate-1
  ask: rewriting it to describe v2 is a *concept substitution*, not a key-rename, and no guard
  covers the prose, so the Mechanical criterion's guard-coverage half is unmet.
- `docs/TODO.md` gains two durable cross-task entries (see Revision 1 items R-4 and R-11).
- `docs/WEBTRANSPORT_FLOW.md:535` carries a JavaScript example encoding v1 fields
  (`frame_type`, `timestamp`, `end_of_frame`) that §2 deletes. Rewriting it is protocol's call:
  **Minor-judgment**.
- `docs/FUZZING.md` / `docs/FUZZ_THIS_CHECKLIST.md` describe the seed corpus for these targets;
  they gain the v2 regeneration command and seed list. Paired with @test.

- `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` — **edited by @team-lead, not the
  implementer.** R-25 read "signature/decrypt/replay/decode rejects in the decode-reject bucket".
  @client and @observability read "bucket" as naming
  a *family* of `reason` values, but the literal reading — one collapsed `decode_reject` label — is
  available and contradicts R-31's requirement that `unknown_version` be individually visible. Left
  unrecorded it would surface in story task 22 as two dashboards with misaligned reason axes. An
  interpretation note was added rather than the requirement rewritten, so the requirement's own
  wording is untouched. Owner is @observability (metric taxonomy, and they raised it); confirmed by
  @client's answer that the SDK emits per-token values enumerated from `ALL_REJECT_REASONS` with no
  collapse. The `dt-story` YAML manifest block was NOT touched. **Attribution corrected 2026-08-31:**
  the note and this entry originally credited three specialists including @media-handler, who never
  stated a position on R-25 (their only adjacent message was on salience token mapping).
  @observability flagged that they were the likely source of the overcount and asked for it trimmed
  rather than left to harden — the same source-checking standard they had just applied to
  @security's adversary-set claim, turned on a claim they introduced themselves. Trimmed to the two
  who actually stated it.

**Not touched** (stated so the absence is deliberate): `proto/**` (that is story tasks 3/4/8),
any `packages/**` TypeScript (story task 15), `crates/mh-service/**` and `crates/mc-service/**`
(both declare the `media-protocol` path dependency but contain **zero** `use media_protocol::`
sites — independently verified by @dry-reviewer and @operations — so retiring v1 breaks no caller
and no consumer migration is in scope).

---

## Gate 1 — Plan Approval (Lead record)

Approved by @team-lead after two revision rounds. `validate-cross-boundary-classification.sh`
returned `STATUS=OK REASON=cross-boundary-classification-clean-1-files` on the re-synced table.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed (native: SAFE-to-proceed) |
| Media Handler (GSA co-signer) | confirmed — **CO-SIGNS** the `crates/media-protocol/**` change |
| Client (owner, `client/INDEX.md` hunk only) | added late; confirming in parallel |

### Lead rulings on decisions routed to the Lead

1. **Reject-reason vocabulary: approved at eight tokens** — `trailing_bytes` added, and
   `extensions_too_large` split from `extensions_malformed`. The task text's six-token list is the
   starting vocabulary; the task states the header is not frozen and that the vocabulary is defined
   by the vectors task (story task 8), which the same implementer owns. Both additions are therefore
   free now and a coordinated cross-language change later. `trailing_bytes` closes the §2
   reserved-byte covert channel restated one field to the right: bytes after the signature are
   covered by no signature, so a compromised MH can append while the frame still verifies.
2. **`payload_length_exceeds_max` / `_exceeds_available` naming: keep the task-text spellings.**
   @observability is correct that they break the fleet's past-participle convention. Re-spelling an
   item the task text enumerates verbatim is a different act from extending the list. The rename
   stays open as a deliberate cross-language change in task 8, and the task-7 taxonomy records the
   exception with a pointer to task 8 as the forum — so a future author finds a live option rather
   than a settled precedent. The two new tokens are noun phrases, so no half-rename results.
3. **`MAX_EXT_BYTES`: derived from the extension registry, not a literal 64** — CLAUDE.md's
   "derive one from the other" outranks a hand-picked headroom number, and deriving turns a check
   that could never fire into one that can. @media-handler re-confirmed derived over their own
   earlier 64; @security proposed it; @observability then ruled on the coupling it created with
   their own token (below) and struck their original justification rather than softening it.
   Required under either endpoint: the `extensions_too_large` rustdoc documents it as
   **hostile-input detection, never a capacity signal** — "raise the bound" is never correct
   remediation for a fail-closed parser limit, because raising it widens the publisher-controlled
   header span this task exists to close.
4. **Four carried obligations get a `docs/TODO.md` home in this diff**, not only a prompt-carry
   (a prompt-carry alone is a control that has to notice, and it evaporates if a downstream task is
   re-scoped): the `stream_id` receiver-validation obligation (tasks 15/19); the `Ok(None)`
   slow-subscriber memory/liveness bound (task 16); the `dt-guard` `pii_vocabulary` CATEGORY_A gap
   (`transmit_key`/`wrapped_key` absent, so a wrapped-key log in the consumer crates passes the
   mechanical guard clean); and the fuzz-workspace exclusion, naming **both**
   `crates/ac-service/fuzz` and `crates/media-protocol/fuzz` verbatim.

### Note on the planning limit

Planning exceeded the skill's 30-minute guidance (~45 min, 2 revision rounds of the permitted 3).
Recorded rather than silently ignored. Not escalated: the gate was converging, not stuck — it
closed with all confirmations and produced two design-level fixes (the parser differential and the
publisher-controlled fast-path offset) that were exploitable as originally planned.

### Design-level defects caught at Gate 1, before any code

- **Parser differential** (@security P1, adopted as a co-sign precondition by @media-handler):
  `rewrite_relay_region` was specified as a second, weaker parser over the same layout as
  `decode_*` and `peek_frame_len`. Two parsers agree on well-formed input and diverge on exactly
  the malformed inputs an attacker constructs. Resolved by one private `parse_layout()` behind all
  four entry points. It would have survived every test originally planned.
- **`FIXED_AUDIO_RELAY_REGION_OFFSET = 62`** (@media-handler R-2 and @security P2, found
  independently): correct only when the key-bearing flag is set *and* `ext_length == 0`, both
  publisher-controlled. One extension on an audio frame would have MH write six bytes over another
  publisher's signed region — any participant could silently kill any other's audio at the relay.
  Deleted outright with no replacement, since @media-handler confirmed no consumer needs a raw offset.
- **Coverage theatre in the corpus generator** (@test R-15): seed assertions in an `examples/`
  binary are compiled by `clippy --all-targets` but never executed by `cargo test`. Moved to
  `tests/corpus_seeds.rs`.
- **Stream-path shortfall treated as terminal** (@security P3 / @observability OB-A): would have
  made a frame spanning a read boundary reset the stream — a remotely triggerable stream kill that
  would have read as correct, documented behaviour. Now `Ok(None)`, with the reason set
  transport-conditioned (8 tokens on datagrams, 5 on streams) and a per-variant "producible by"
  annotation, so task 21 cannot write an alert whose selector can never match.
- **`ALL_REJECT_REASONS` as `&[&str]`** (@dry-reviewer): would have written the wire tokens twice in
  Rust; a typo in the second copy propagates into task 8's vectors, task 18's fixtures and task 22's
  catalog, which then agree with each other and disagree with the codec while the drift guard reports
  clean. Now `&[RejectReason]` with a wildcard-free exhaustive match so a missing token fails to compile.

### Open cross-task items carried out of this devloop

- **R-25/R-31 comparability** (@observability): whether the client collapses the eight codec tokens
  into one `decode_reject` bucket or emits them individually. Unresolvable inside this crate; needs
  @media-handler and @client before story task 22, not at it.
- **Task 8 must pin the extension registry itself** in the cross-language vectors, not just
  `max_payload_bytes` — deriving `MAX_EXT_BYTES` moves the load-bearing fact out of a literal and
  into the registry, so an unpinned registry is worse than the constant it replaced. @security adds
  that the pin must cover **rejection semantics**: a negative vector (unknown type byte, must
  reject), since a TypeScript decoder can pin an identical registry and still skip unknown types.
- **Registry entries must declare a fixed value length, or a canonical variable-length encoding**
  where length is a function of the value (@security). A bare "maximum length" reopens the hole
  ruling 2(a) closed: non-minimal encodings make the length byte a publisher-chosen free variable
  that passes every other check, and byte-identical roundtrip fuzzing structurally cannot see it.
  No-op for v2 (type `0x01` is fixed at one byte), which is why it is written down now.

---

## Gate 2 — Validation (Lead record)

Command: `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (unattended caller → all seven layers run in
one pass, per ADR-0033 §4).

### Attempt 2 of 3 — PASS

```
LAYER=1 RESULT=OK   DURATION=4
LAYER=2 RESULT=OK   DURATION=1
LAYER=3 RESULT=OK   DURATION=25
LAYER=4 RESULT=N/A  DURATION=183
LAYER=5 RESULT=OK   DURATION=2
LAYER=6 RESULT=N/A  DURATION=2
LAYER=7 RESULT=OK   DURATION=459
TOTAL_DURATION=676 TOTAL_RESULT=N/A   (exit 0)
```

**Read as PASS.** No `FAIL` in any layer. Layer 4's `N/A` aggregates `cargo-test-passed` (OK) +
`nx-test-passed` (OK) + proto `STATUS=N/A REASON=not-applicable-to-this-lang` — the registered
intentional-gap placeholder (proto has no `test.sh`), which ADR-0033 §6 makes self-justifying: the
wrapper's own `REASON=` is the justification and the implementer owes no separate explanation.
Layer 6 is the same shape (`audit-aggregate-na`, with `cargo-audit-passed` and the documented
`SKIPPED-NO-DIFF no-dep-changes` dep-manifest gate). Layer 7 ran its full suite: `env-tests-passed`
**and** `browser-e2e-passed`.

One non-blocking warning: `WARN BUDGET_BREACH LAYER=4 DURATION=183 BUDGET=20` — first full build of
the rewritten crate plus its five test surfaces. A warning, not a gate failure; recorded rather than
dropped.

### Attempt 1 of 3 — FAIL (counted, not voided)

```
LAYER=1 RESULT=FAIL   LAYER=3 RESULT=FAIL   LAYER=4 RESULT=FAIL
LAYER=5 RESULT=FAIL   LAYER=6 RESULT=N/A    LAYER=7 RESULT=PRECONDITION_FAILURE
TOTAL_RESULT=PRECONDITION_FAILURE (exit 2)
```

This run raced a half-applied implementer edit (a patch script aborted after writing `frame.rs` but
before `codec.rs`, leaving `error[E0063]: missing field wrapped_key_offset`). **Counted as attempt 1
rather than voided**, because it surfaced three violations that had nothing to do with the race and
that the implementer would not have found alone. Voiding a true result because its timing was
inconvenient is the kind of masking CLAUDE.md forbids.

- Layers 1/4/5 — all the single `E0063`.
- Layer 3 `validate-cross-boundary-scope` — **scope drift**: `tests/frame_properties.rs` and
  `tests/reject_reasons.rs` were in the diff with no plan row (split out of what the plan called one
  test surface). Layer A doing exactly its job.
- Layer 3 `validate-knowledge-index` — `docs/specialist-knowledge/media-handler/INDEX.md` held a
  `[stale_pointer]` to the deleted `crates/media-protocol/src/stream.rs` (v1 retired, a *different*
  specialist's INDEX still pointed at it), and `docs/specialist-knowledge/protocol/INDEX.md` was a
  `[size_violation]` at 79 lines against a 75 cap.

Layer 7's `PRECONDITION_FAILURE REASON=cluster-rebuild-failed` was **not** claimed as the operator
lane: the cluster rebuild compiles mc/mh, which depend on this crate, so it was downstream of the
compile break rather than an independent infrastructure fault. Re-judged fresh on attempt 2, where
it passed.

All five issues were fixed before attempt 2; the `protocol/INDEX.md` overflow was resolved by
consolidating duplicate pointers, not by raising the cap.

---

## Gate 3 — Final Approval (Lead record)

Post-review re-validation on the final tree (the review round changed code substantially, so the
earlier green run was not authoritative for what is being committed):

```
LAYER=1 OK  LAYER=2 OK  LAYER=3 OK  LAYER=4 N/A  LAYER=5 OK  LAYER=6 N/A  LAYER=7 OK
TOTAL_DURATION=654 TOTAL_RESULT=N/A  (exit 0)
```

No `FAIL` in any layer; the `N/A`s are the documented proto intentional-gap placeholder (Layer 4)
and the audit aggregate (Layer 6). Layer 7 green including browser E2E.

### Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 12 | 12 | 0 | Compiled the workspace-excluded fuzz targets personally; verified the call graph rather than accepting a grep |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | Accepted the parse-order boundary rather than escalating; reordering would have changed the wire contract |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | All five were plan-document defects; the code was clean |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | `#[allow]`→`#[expect]` was not clippy-catchable — a genuine manual catch |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | See §Accepted Deferrals for the extraction-opportunity pointer (not a deferral — Lead ruling below) |
| Operations | RESOLVED-DEFERRED | 1 | 1 | 0 (1 spin-out) | Diff is clean; the spin-out is a pre-existing gap outside this changeset |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | Native verdict: **SAFE** |
| Media Handler (GSA co-signer) | RESOLVED-FIXED | 1 | 1 | 0 | **CO-SIGNS** — trailer `Approved-Cross-Boundary: media-handler` |
| Client (owner, `client/INDEX.md` hunk) | RESOLVED-FIXED | 2 | 2 | 0 | Cross-boundary hunk re-confirmed at Gate 3 per §6.3 |

**Total: 28 findings, 28 fixed, 0 deferred, 1 spin-out.** No escalations.

### Lead ruling — DRY verdict classification

@dry-reviewer surfaced that `review-protocol.md` §Verdict Format, read literally, makes any filed
DRY extraction opportunity force RESOLVED-DEFERRED. **Ruled RESOLVED-FIXED.** The taxonomy asks
whether *this reviewer left a finding of theirs in the diff*; all three of theirs were fixed. Their
`docs/TODO.md` entry is pre-existing duplication in `mc-service`/`mh-service` that was never in this
changeset. Their reductio decided it: under the literal reading a DRY reviewer could never return
RESOLVED-FIXED while filing any unrelated opportunity, making the verdict a measure of how much
unrelated debt they happened to notice rather than of what this diff left behind. They proposed a
concrete wording patch whose load-bearing clause closes the obvious abuse — *if a DRY finding was
about code in this diff and was not fixed, it is a deferral, not an extraction opportunity; the test
is location, not severity.* Carried to story-close reflection.

### Why @operations is RESOLVED-DEFERRED while the diff is clean

Their one review finding was fixed. The verdict reflects a single accepted **spin-out**: the root
`Cargo.toml:22-25` exclusion of **both** `crates/ac-service/fuzz` and `crates/media-protocol/fuzz`
from the pipeline, plus `cargo-fuzz` being absent from the container. Pre-existing, in a file this
PR does not touch, scoped out at their own Gate 1 as toolchain work owned by test + infrastructure
(story task 23). Recorded here rather than left to a later audit. Nothing about it argues against
landing this commit.

### The defects this review caught that no gate could

- **`producible_by()` made a false claim in the mis-triage direction** (@operations). `rewrite_relay_region`
  is a *fourth* fallible entry point and MH calls it on stream-carried video, so `truncated` and
  `payload_length_exceeds_available` were labelled datagram-only while being reachable on the video
  path. An operator seeing `truncated` on MH's video counter would have consulted the annotation,
  read "impossible here", and burned the incident. Found by asking what an operator would conclude,
  not whether the annotation was self-consistent. Now an entry-point model with
  `reachable_on_stream_carried_frames()`, an `operator_meaning` that forks on entry point, and a
  cross-check over all four entry points instead of two — the control that should have caught it was
  itself only checking two thirds of its own claim.
- **`MediaFrameParts` derived `Debug` over raw payload and signature** (@semantic-guard), contradicting
  a Gate-1 guarantee, and the redaction test passed green because it never formatted that type — the
  derive rendered the very sentinels the test exists to catch.
- **A lint-driven refactor silently undid an accepted security finding** (@security). A `cargo fmt`
  reflow tripped `too_many_lines`; the resulting split had the helper return parsed fields while the
  caller independently recomputed the resume offset — two claims to where the publisher region
  continues. Clippy was *satisfied* by the split and every gate was green. Caught by a reviewer
  querying a stray slash in a prose status message.
- **A latent second offset authority in `build_view`** (found by @implementer applying @security's
  stated property to their own code). It *agreed* with the cursor and would have diverged only once a
  conditional field was added ahead of the wrapped key — which is why 47 tests, a byte-coverage proof
  and a roundtrip all passed over it.
- **A completeness test that checked only duplicates** (@dry-reviewer). Its name and comment both
  asserted completeness; a ninth token would have compiled clean at eight entries and gone silently
  missing from task 8's vectors, task 18's fixtures and task 22's catalog — three artifacts agreeing
  with each other and disagreeing with the codec, drift guard green.
- **`MAX_HEADER_BYTES` stated as 65 in the section that defines it** (@observability), omitting
  `RELAY_REGION_SIZE`, against a shipped 71. Tasks 15 and 16 both size buffers from it.

### Honest coverage boundary

**No fuzzing was executed.** `cargo-fuzz` is absent from the container (@operations verified
independently with `cargo fuzz --version`). Both targets are **compile-checked** — a real
`cargo +nightly check --all-targets` inside the fuzz workspace, exit 0 — and nothing was fuzzed.
What carries the coverage instead is `tests/corpus_seeds.rs`: 558 encoder-derived seeds, each
asserted for its expected decode outcome and driven through all four entry points, executing under
`cargo test`. Separately, **every fixture is encoder-built**, so the suite proves the codec
*self-consistent and fail-closed*, **not correct** — an encoder and decoder sharing one layout
misreading passes all 48 tests, and in the specific bad case signatures would verify while covering
the wrong bytes. Only story task 8's frozen, externally anchored vectors can catch that.

### Cross-task obligations carried out of this devloop

Recorded in `docs/TODO.md` §Media Path Obligations with named target tasks and delete-when-landed
markers, each with a rustdoc counterpart so it travels with the wire format. These are constraints
the codec structurally *cannot* enforce, not deferred findings: receiver-side `stream_id` validation
(tasks 15/19); the `Ok(None)` slow-subscriber bound (task 16 **and** the SDK video-stream reader —
@client caught that the original routing named only MH); the `dt-guard` `pii_vocabulary` CATEGORY_A
gap for consumer crates; the vectors-versus-ruling precedence rule; and @media-handler's commitment
that task 16 label its reject counter by entry point.

---

## Planning

### Problem restated as a mechanism (per the devloop instruction)

Instance framing: *"implement version 2 of the media frame header."*

Mechanism framing: **every byte and every bit arriving from an untrusted peer must be decoded into
a field some receiver inspects or rejected outright — and any span a signature or an AEAD covers
must be a *slice of the received bytes*, never a re-serialization of a parsed struct.** The v1
codec violates both halves: six unvalidated reserved bytes plus a flag parser that masks undefined
bits (a covert channel out of a compromised MH), and an allocate-and-copy decode that invites a
"re-encode it and verify that" verifier.

Widening the class: the *authenticated-range* half has same-mechanism siblings outside this crate —
notably JWS verification, where the signing input must be the received `header.payload` substring
rather than re-serialized JSON. `crates/common/src/jwt.rs` / `meeting_token.rs` are a different GSA
(`[auth-controller, security]`), so this is **surfaced, not claimed and not in scope**: I have not
audited them and am not asserting a defect. Recorded here because the mechanism is wider than the
task's nouns, per the planning instruction. The *skipped-bytes* half has no same-owner sibling I
can find: protobuf's unknown-field preservation is prost's semantics, not ours to fail closed on.

Everything else in the task genuinely is one file's worth of wire format, so the task's framing is
not materially narrower than the problem. No scope expansion requested.

### Wire layout (ADR-0036 §2 + Appendix), big-endian throughout

```
off  size  field                       region      constant
  0     1  version (= 2)               publisher  |
  1     1  flags                       publisher  | PUBLISHER_FIXED_PREFIX_SIZE = 10
  2     4  payload_length : u32        publisher  |
  6     4  stream_sequence : u32       publisher  |
 10    50  wrapped transmit key        publisher    WRAPPED_TRANSMIT_KEY_SIZE = 50
             kek_generation : u16        (KEK_GENERATION_FIELD_BYTES = 2)
             wrapped_key    : 32 B       (WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES = 32)
             wrap_tag       : 16 B       (AEAD_TAG_BYTES = 16)
           — present IFF flags bit2 (key-bearing); absent entirely otherwise
  +     2  ext_length : u16            publisher    EXT_LENGTH_FIELD_SIZE = 2
  +     N  TLV extensions (N = ext_length)          MAX_EXT_BYTES (derived, R-22)
  ================================================ end of publisher region (AAD span)
  +     2  stream_id : u16             relay      | RELAY_REGION_SIZE = 6
  +     4  hop_sequence : u32          relay      |
  +   len  payload (opaque SFrame object)          MAX_PAYLOAD_BYTES = 1 048 576
  +    64  Ed25519 signature                       SIGNATURE_SIZE = 64
```

Flags: bit0 `INDEPENDENTLY_DECODABLE` (0x01), bit1 `DISCARDABLE` (0x02), bit2 `KEY_BEARING` (0x04).
`LEGAL_FLAG_MASK = 0b0000_0111`; `flags & !LEGAL_FLAG_MASK != 0` ⇒ reject. No reserved bytes.

No fixed relay-region offset constant is exported, and no checked accessor either — see R-2. The
offset moves with the key-bearing flag and the extension length, both publisher-controlled, so it is
derived per frame inside the parser and nowhere else.

Derived, no literals: `WRAPPED_TRANSMIT_KEY_SIZE = KEK_GENERATION_FIELD_BYTES +
WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES + AEAD_TAG_BYTES` (= 50) and
`SFRAME_OBJECT_OVERHEAD_BYTES = KEY_ID_BYTES + AEAD_TAG_BYTES` (= 24, size-only, **no SFrame
parsing in this crate**). `AEAD_TAG_BYTES = 16` and `KEY_ID_BYTES = 8` are each defined **exactly
once in the workspace** (@dry-reviewer confirmed no prior home), carrying a note that they are the
SFrame-0x0005 *media wire* sizes and deliberately **not** shared with `ac-service`'s at-rest
storage-envelope `16` (different concept; sharing would falsely couple the media ciphersuite to
AC's storage format). One value-pin test each, marked as an intentional pin, not a second
definition. Every size constant carries its unit in the name (O-10).

### API shape

**No crypto dependency in this crate.** It exposes byte ranges and opaque fields; Ed25519
verification and the SFrame open happen in the caller (TypeScript in production; the task-8
reference generator in tests). Answer to @test Q1: **decode does not verify the signature.**

```rust
// frame.rs
pub struct MediaFrameView<'a> { /* borrowed; PartialEq; hand-rolled redacting Debug */ }
impl<'a> MediaFrameView<'a> {
    pub fn version(&self) -> u8;
    pub fn flags(&self) -> FrameFlags;              // typed booleans, no raw bits escape
    pub fn stream_sequence(&self) -> u32;
    pub fn wrapped_transmit_key(&self) -> Option<WrappedTransmitKey<'a>>;
    pub fn extensions(&self) -> Extensions<'a>;     // validated at decode; iteration cannot fail
    pub fn stream_id(&self) -> u16;
    pub fn hop_sequence(&self) -> u32;
    pub fn payload(&self) -> &'a [u8];              // slice of the input, never a copy
    pub fn signature(&self) -> &'a [u8; SIGNATURE_SIZE];
    // Authenticated spans — slices, never re-serialized (§3/§4):
    pub fn publisher_region(&self) -> &'a [u8];     // == the AEAD associated data, exactly
    pub fn signed_ranges(&self) -> [&'a [u8]; 2];   // [publisher_region, payload], in that order
    // Offsets, for a caller that owns the frame as `Bytes` and wants refcount slices:
    pub fn publisher_region_range(&self) -> Range<usize>;
    pub fn relay_region_offset(&self) -> usize;
    pub fn payload_range(&self) -> Range<usize>;
    pub fn signature_range(&self) -> Range<usize>;
    pub fn encoded_len(&self) -> usize;
}

// codec.rs
pub fn decode_datagram(data: &[u8]) -> Result<MediaFrameView<'_>, DecodeError>;
pub fn decode_stream_frame(data: &[u8]) -> Result<Option<MediaFrameView<'_>>, DecodeError>;
pub fn peek_frame_len(data: &[u8]) -> Result<Option<usize>, DecodeError>;
pub fn rewrite_relay_region(frame: &mut [u8], stream_id: u16, hop_sequence: u32)
    -> Result<(), DecodeError>;
pub fn encode_frame(parts: &MediaFrameParts<'_>) -> Result<Bytes, EncodeError>;
```

**`decode_datagram` vs `decode_stream_frame` — the split @security (#3), @operations (Ask 3) and
@observability (O-2) all converged on independently.**
- `decode_datagram` is *exact*: one frame per datagram (§1), so any byte after the signature is a
  reject, not slack. Ignoring a tail would be the v1 reserved-byte channel wearing a new hat.
- `decode_stream_frame` returns `Ok(None)` for **"this buffer is a strict prefix of a possibly-valid
  frame"** — an *incomplete* outcome that is **not a reject**, carries **no reason token**, and must
  not be counted on a drop counter. `Err` means **no extension of this buffer can decode**
  (terminal). The rule is decidable: a shortfall against a *declared* length is incomplete; a
  violation of a *limit* or of *structure* (unknown version, illegal flag bit, `payload_length >
  MAX_PAYLOAD_BYTES`, `ext_length > MAX_EXT_BYTES`, malformed TLV within a fully-present ext region)
  is terminal. That is exactly O-2's "incomplete is a distinct non-reject outcome" and Ops Ask 3's
  retryable-vs-permanent fork, expressed in the type rather than in prose.
- `peek_frame_len` is the **reader-side pre-allocation boundary** (@security #9, @media-handler #5,
  ADR-0036 §2's "wrong in the reader that calls it"): given a partial buffer it returns
  `Ok(None)` (need more header bytes), `Ok(Some(total_len))` once the length is knowable, or `Err`
  — enforcing `MAX_PAYLOAD_BYTES` *before* the caller reserves anything. Since decode is zero-copy
  it never allocates at all; `peek_frame_len` is where the bound earns its keep. Tested directly,
  and driven from the `codec_decode` fuzz target.

**`rewrite_relay_region`** is @media-handler's #1: parse the header prefix to locate the region
(fail-closed), then write 6 bytes in place — no allocation, no decode→mutate→re-encode, publisher
region and signature provably untouched (the relay region is excluded from both). It lives here so
the offset arithmetic has one home (@dry-reviewer #4) and MH never recomputes a layout offset.
Borrow model (@media-handler #4): `MediaFrameView` borrows immutably, so MH either drops the view
before rewriting or calls `rewrite_relay_region` directly on its owned `&mut [u8]` — the function
re-derives the offset itself, so no view need be held across the mutation.

**Zero-copy is structural, not incidental** (@test #2): the decode signature borrows `&'a [u8]` and
every field is a sub-slice of it. There is no code path that could allocate a payload; a copy is
not merely absent, it is unrepresentable. For a caller holding `Bytes`, the `*_range()` accessors
give refcount-sharing slices without this crate touching `Bytes` on the decode path.

**Encoding is canonical** (@test #3, @dry-reviewer #5): fields are written in one fixed order and
extensions are supplied typed and emitted in ascending type order, so `encode(decode(x)) == x`
byte-for-byte for every `x` that decodes. The roundtrip fuzz target asserts **byte identity** plus
`PartialEq` on the view, rather than a hand-maintained field list. `encode_frame` carries a rustdoc
warning that it must never be used to reconstruct bytes for verification, and there is deliberately
**no** `signed_bytes()` / `to_bytes()` method on the decoded view (@security #13).

### Reject-reason vocabulary

One enum, one exhaustive `match` returning `&'static str`, written exactly once
(@dry-reviewer #2, @observability O-5); `DecodeError::reason() -> RejectReason`;
`RejectReason::as_str() -> &'static str`; `pub const ALL_REJECT_REASONS: &[&str]` for task 8's
vectors, task 18's fixtures and task 22's catalog to enumerate mechanically. Never derived from
variant identifiers (a rename must not silently rename a wire token). Tests assert
`err.reason()`, never a copied literal. `ANCHOR (DRY):` comments on both `MAX_PAYLOAD_BYTES` and
the vocabulary naming `proto/test-vectors/frame-v2.vectors.json` as the cross-language SSoT that
task 8 lands.

**Normative evaluation order** (@observability O-1 — without a pinned order Rust and TypeScript can
both "correctly reject" the same bytes with different tokens while the drift guard reports clean).

Stated as a **reason x entry-point -> outcome table**, and **only** as that table. The numbered list
that stood here was deleted rather than renumbered: a positional reference into a list is the same
transcription-fragile form as a hand-written count, and Plan Revision 1 already cited it by ordinal.
Nothing outside this table references an evaluation step by position — cite the **reason token**.
Order remains a column, because order is genuinely ordinal data and is load-bearing: `unknown_version`
before the flag check, flags before the length checks, and `payload_length > MAX_PAYLOAD_BYTES`
before anything sizes a buffer. A renumbering slip surviving into task 15's TypeScript port could
land a length check after an allocation, which is precisely the defect §2 says a fuzzed decode
function cannot see.

| # | Condition | `decode_datagram` | `decode_stream_frame` |
|---|---|---|---|
| 1 | buffer holds no bytes | `truncated` | `Ok(None)` |
| 2 | `version != 2` | `unknown_version` | `unknown_version` |
| 3 | buffer shorter than `PUBLISHER_FIXED_PREFIX_SIZE` | `truncated` | `Ok(None)` |
| 4 | `flags & !LEGAL_FLAG_MASK != 0` | `reserved_flag_bit_set` | `reserved_flag_bit_set` |
| 5 | `payload_length > MAX_PAYLOAD_BYTES` | `payload_length_exceeds_max` | `payload_length_exceeds_max` |
| 6 | key-bearing set, fewer than `WRAPPED_TRANSMIT_KEY_SIZE` bytes remain | `truncated` | `Ok(None)` |
| 7 | fewer than `EXT_LENGTH_FIELD_SIZE` bytes remain | `truncated` | `Ok(None)` |
| 8 | `ext_length > MAX_EXT_BYTES` | `extensions_too_large` | `extensions_too_large` |
| 9 | fewer than `ext_length` bytes remain | `truncated` | `Ok(None)` |
| 10 | the TLV walk fails **within a fully-present extension region** | `extensions_malformed` | `extensions_malformed` |
| 11 | fewer than `RELAY_REGION_SIZE` bytes remain | `truncated` | `Ok(None)` |
| 12 | fewer than `SIGNATURE_SIZE` bytes remain after the relay region | `truncated` | `Ok(None)` |
| 13 | `payload_length` exceeds what remains once the signature is reserved | `payload_length_exceeds_available` | `Ok(None)` |
| 14 | bytes remain after the signature | `trailing_bytes` | `Ok(Some(..))`, `encoded_len` short of the buffer |

Rows 8 and 9/10 are deliberately separate, and the qualifier in row 10 is load-bearing: *"the TLV
walk fails"* is ambiguous between a grammar violation in a region that is **entirely present**
(terminal on both paths) and the walk running out of bytes mid-region (a shortfall against the
declared `ext_length`, therefore `Ok(None)` on the stream path). Conflate them and a video frame
whose read boundary lands inside the extension region dies as a permanent structural fault.

One multi-condition test per adjacent pair, plus a behavioural cross-check of the right-hand columns
against `RejectReason::producible_by()` — see R-26.

**Two naming exceptions, also flagged not decided.** @observability O-4 notes
`payload_length_exceeds_max` / `payload_length_exceeds_available` are verb phrases with no
precedent in the tree (`meeting_capacity_exceeded`, `db_write_failed`). My task text mandates those
exact spellings and task 8 freezes them cross-language, so I plan to **keep them as specified** and
record the convention exception in @observability's taxonomy row — deviating from an explicit task
enumeration is the larger risk. Both are snake_case and ≤64 chars, so the `metric_labels` guard
passes. @team-lead to rule if @observability prefers the rename.

**Deliberate collapse, recorded** (@operations Ask 4): a key-bearing frame that ends before its
50-byte field yields plain `truncated`, **not** a distinct key-path token. Reasoning, written into
the rustdoc: a truncated wrapped-key field is a transport or encoder fault, whereas the key-path
signal §11 actually wants is the client-side `no_kek_for_generation` / `no_roster_entry` counter
(R-25). Task 21's Scenario 16 ladder can state that plainly rather than promising a discriminator
that does not exist. Similarly `extensions_malformed` is a **deliberate** collapse of several
sub-causes for label-cardinality reasons, with the sub-cause available only in `Display`
(@observability O-9).

**Scope boundary of the vocabulary** (@observability O-3): rustdoc states these are the
*structural/parse* subset of a shared `reason` label space; signature, decrypt, replay,
`no_kek_for_generation` and `no_roster_entry` belong to the crypto and key layers, and no
downstream layer may reuse one of these tokens with a different meaning. Per-variant rustdoc gives
the **operator** meaning (what happened / what to check) for @operations to lift into task 21's
triage ladder, plus the consumer recovery per class (@operations Ask 5): drop-the-frame on a
datagram; reset-the-stream on a length lie, which desynchronizes stream framing and cannot be
resynced; **never** tear down a connection.

### Extensions (the one place ADR-0036 leaves the shape open)

§2/§7 mandate a publisher-set TLV section but do not specify the TLV encoding. I must define it and
task 8 will freeze it into the vectors, so I am stating it explicitly rather than burying it:

- Entry = `type: u8`, `length: u8`, `value: length` bytes. The region must be consumed **exactly**.
- **Unknown types are rejected, not skipped.** @security #4 and @observability O-9 both land here,
  and §2 is decisive: *"The version field is the extension path, and fail-closed is chosen
  deliberately over forward compatibility."* A length-prefixed skip is the reserved-byte channel
  with a length field. §7's motivation for TLV survives intact — it is about not spending a fixed
  salience byte on every video frame and about carrying several signals, not about tolerating
  unknown ones.
- Duplicate types rejected; **ascending type order enforced** (@security #4 — unconstrained
  ordering of *n* entries is ~log2(n!) publisher-controlled bits per frame, and the check is one
  comparison in the parse loop); zero-length padding entries rejected.
- `MAX_EXT_BYTES`, checked **before** the region is sliced or walked (@security #5,
  @observability O-9), because `ext_length` is a `u16` and unbounded would permit a 64 KiB extension
  region on an ~80-byte Opus frame. **Derived from the registry — see R-22**, not a chosen number. A
  separate max-entry-count is redundant: known-types-only plus no-duplicates bounds the count at the
  registry size.
- **v2 registry has exactly one entry**: `0x01 = publisher-declared salience`, value length exactly
  1 byte — the one concrete signal §7 names. Untrusted by construction (§7: "publisher-supplied and
  untrusted"; the MC-assigned priority group is the bound, and that is story 5's selector, not this
  crate's). Loopback carries zero extensions. Task 8's four AAD-span rows (key-bearing × extensions)
  need a representable extensions-present frame, which is why the registry cannot be empty.
  **@media-handler: this is your selector's input shape — flag now if it should differ.**
- Answer to @test Q2: extensions are **validated and typed at decode**, so `extensions_malformed`
  is detected inside decode and the returned iterator cannot fail.

### Invariants recorded in the codec (the task says "two" and lists three — all three land)

1. **`stream_sequence` is per (sender, stream) and must NOT reset at a generation boundary.** The
   transmit-key generation is monotonic **per sender** and shared across that sender's streams —
   two different counters with different scopes. Resetting the sequence at a generation bump is the
   §2-forbidden silent-nonce-repeat path; under AES-GCM a nonce repeat is authentication-key
   recovery, not merely confidentiality loss (§4 "Nonce-reuse invariants are structural"). Cited to
   §2 and §4 at the field.
2. **AAD and signed ranges are slices of the received buffer, never re-serialized from a parsed
   struct.** With the corollary @security #13 asked to be written down: the signed input is the
   *ordered pair* (publisher region, payload), which are **non-contiguous** because the relay region
   sits between them, and that concatenation is unambiguous **only because `payload_length` and
   `ext_length` are themselves inside the signed publisher region** — so nobody later "fixes" it by
   adding a length prefix or reordering. The AAD is the publisher region **alone**, a deliberate
   strict subset of the signed range.
3. **Any field packed into a narrower wire width uses checked conversion sized to the field** —
   never a masking cast, never a bare shift; out-of-range values error. v2's own field types already
   match the wire widths, so this is defensive against a future widening; the sharp instance is the
   sender-side KID packer that task 8 authors (masking aliases sender 65536 to 0 → KID collision →
   nonce reuse).

Invariant 3 is additionally **enforced mechanically, not just commented**: crate-level
`#![deny(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]`
under a `-D warnings` pipeline, so an `as` narrowing cast on this path is a build failure. v1's
`#[expect(clippy::cast_possible_truncation)]` at `codec.rs:56` does not survive.

### No-panic and overflow discipline (ADR-0002; @operations Ask 2)

Every wire-derived bound goes through `checked_add` accumulated in **`usize`**, never in the
field's own width — `payload_length: u32` at max, plus 50, plus 64, plus `ext_length` overflows a
`u32` accumulator, and in release mode the wrapping sum is *small* and passes a naive bounds check.
That is an exploitable shape, not a theoretical one. All slicing via `get(a..b)`, never `&buf[a..b]`
(the workspace already denies `clippy::indexing_slicing`), and no `Buf::advance`. Blast radius if
wrong: decode runs per frame in MH's forward path, so one panic is a pod crash taking every meeting
on that pod with it — which is why the fuzz targets get **run**, not merely updated.

### Deletions (v1 retirement — no orphaned v1 surface)

`MediaFrame`, `MediaFrame::HEADER_SIZE = 42`, `FrameType` (§2: the independently-decodable flag
*replaces* a media-type field; MH must not know audio from video — §7), `FrameFlags::to_u16 /
from_u16` (the mask-away-unknown-bits idiom under replacement — @security #1), `CodecError` with
its `InvalidFormat(String)` (a `String` in a codec error is attacker-controlled bytes routed at
logs — @security #16), and `encode_frame`/`decode_frame` in their v1 forms.

**No v1 decode arm and no version negotiation** (@security #2, @operations Ask 1): the version byte
exists at offset 0 for *future* dispatch, but v1 is deleted outright. Rustdoc records why — at this
commit `media-protocol` had no consumer in any service and nothing was deployed speaking v1, and
per §2 the version is meeting-wide and MC-directed so a mixed-version meeting cannot arise. A live
v1 branch would be a downgrade surface with no legitimate caller.

`src/stream.rs` is **deleted** (@operations Ask 6, @code-reviewer #6). It is unreferenced v1-shaped
state whose every field contradicts v2: `next_sequence: u64` (v2 has two 32-bit sequences with
different scopes), `stream_id: u32` (v2 relay `stream_id` is u16), and `is_audio: bool` — a
media-type field §2 deliberately removes. Leaving it beside a v2 codec is a legibility trap, and
inventing v2-shaped state no consumer uses would be speculative. It also satisfies @security #14's
"no reset API on a sequence counter" structurally. **@media-handler**: if task 16 wants a
`HopSequence` counter type here instead (no reset API, `checked_add` exhaustion returning `Err`
rather than `wrapping_add` — @security #14), say so at Gate 1 and I will land it in this diff
rather than have you build it in task 16.

### Key material and log hygiene

`Debug` is **hand-rolled**, never derived, on every type holding wire bytes: `MediaFrameView`
redacts `payload`, `signature` and the wrapped key; `WrappedTransmitKey` renders `kek_generation`
(a field the receiver genuinely inspects for §4 rotation) and `<redacted 32 B>` / `<redacted 16 B>`
for the rest. Accessors are named `expose_wrapped_key()` / `expose_wrap_tag()`, mirroring
`common::secret::SecretBox::expose_secret()`, so reaching for the bytes is a visible act.

**Why not `common::secret::SecretBox` as @observability O-7 asked**: `SecretBox` is owned, and the
wrapped key is a borrowed slice into the input buffer. Boxing it would allocate per frame — and
*every* audio frame is key-bearing (§4 cadence), so that is an allocation on the hottest path in
the system, defeating the zero-copy requirement. The redacting `Debug` gives the same
unrenderable-by-construction property at zero cost. @observability's underlying point stands and I
have taken it: the `pii_vocabulary.rs` CATEGORY_A guard does **not** contain `transmit_key`, so the
guard would report clean — the protection has to be structural, and it is. Flagged to
@semantic-guard.

**No `tracing`, no metrics, no logging in this crate — enforced structurally by removing the
`tracing` dependency**, so a log macro here is a compile error rather than a rustdoc request
(@observability, @operations Ask 5). `Display` on errors carries scalars only: the version byte,
the offending flag bitmask, declared and available lengths — never payload bytes, key bytes, the
raw extension buffer, or the key id. Module docs state that only `reason()` is safe as a media-path
log field or metric label, because §11 forbids per-frame size dimensions on the media path (the
time-ordered size sequence *is* the voice-activity trace).

`Cargo.toml`: drop the unused `tokio`, `serde`, `tracing`, `common` and `criterion` dependencies
(there is no `benches/`). Remaining: `bytes` + `thiserror`. This is hygiene *and* the mechanism
above.

### The unauthenticated relay region, stated and bounded (@security #7)

`stream_id` + `hop_sequence` are 6 bytes per frame a compromised MH writes freely and nobody
authenticates. That is the design (§2), not a defect, and the module docs say so as
accepted-and-bounded — together with the **downstream obligation that must not live nowhere**: the
receiver validates `stream_id` against its own declared slots (§6) and rejects unknown ones,
otherwise MH gets 16 free bits per frame. That obligation lands in the SDK (story tasks 15/19), not
here; I will ask @team-lead to carry it into those task prompts, and the module rustdoc names it so
the constraint travels with the format.

### Tests

- One test per reject reason asserting the **specific** `err.reason()`, never `is_err()`
  (@test #1); exhaustiveness driven off the enum, not a copied list.
- One multi-condition test per adjacent pair in the normative order (@observability O-1).
- **Byte-coverage proof** (@security #8, ADR-0036 "A control's coverage must be demonstrated, not
  asserted"), as `tests/byte_coverage.rs`: take a canonical valid v2 frame, mutate **every byte
  position** in turn and assert each position either changes a decoded field value or produces a
  rejection. Same shape bit-by-bit for the flags byte. That is the executable proof no skipped byte
  exists; "we deleted the reserved field" is only the assertion half.
- Zero-copy asserted **by aliasing**, not equality (@test #2): every returned slice's pointer must
  fall inside the input buffer's address range.
- Derived constants exercised **behaviourally** (@test #4): a key-bearing frame's minimum length is
  observably `WRAPPED_TRANSMIT_KEY_SIZE` longer than the same frame without the flag. Exactly one
  literal-pin test per derived constant, commented as an intentional value pin.
- Relay-region rewrite (@test #5, @media-handler #1): rewriting leaves the publisher region and the
  64-byte signature byte-identical, and the frame still decodes with the new relay values.
- Overflow: `payload_length = u32::MAX` with a short buffer must reject, not wrap.
- `peek_frame_len` on the reader path, including `MAX_PAYLOAD_BYTES + 1`.

### Fuzz

Both targets rewritten for v2. `codec_decode` drives `decode_datagram`, `decode_stream_frame` and
`peek_frame_len` (the last is the only one that can catch the pre-allocation defect §2 names).
`codec_roundtrip` asserts byte-identical re-encode plus `PartialEq`, with no header sizes or
`1048576` hardcoded in either target (@dry-reviewer #5).

**Corpus.** `crates/media-protocol/fuzz/corpus/` is gitignored (`.gitignore:8`) and empty today, so
"regenerate" cannot mean checking in blobs. The checked-in artifact is a **self-asserting
deterministic generator** living in `tests/corpus_seeds.rs` — **see R-15 for why it is a test and
not an `examples/` binary**: an example's `main()` is compiled by `clippy --all-targets` but never
executed by `cargo test`, so its assertions would never have run. It builds each seed through the v2
encoder (so a seed can never drift from the codec — the DRY-correct SSoT), asserts the expected
decode outcome for each, and fails loudly on any mismatch. Regeneration is one documented command,
added to `docs/FUZZING.md`. Seeds cover
@test #3's list: key-bearing on/off; `ext_length` zero, valid non-zero, and malformed TLV;
`payload_length` at `MAX_PAYLOAD_BYTES` and at max+1; each of flag bits 3–7 individually; and
truncation at **every** field boundary from mid-version to mid-signature. I will **run** both
targets (@operations Ask 2) and report the result honestly — including "the nightly/cargo-fuzz
toolchain is unavailable in this container" if that turns out to be the case, rather than claiming
a run that did not happen.

Note for @team-lead: `crates/media-protocol/fuzz` is `exclude`d from the root workspace
(`Cargo.toml:23`), so `scripts/layer-all.sh` never compiles the fuzz targets. Updating them is
therefore not covered by the pipeline gate — I will compile them explicitly and say so.

### Answers to questions asked before the plan

- **@test Q1** — decode does **not** verify the signature; it returns the ranges. The
  "sig covers publisher ∪ payload, excludes relay" test therefore belongs here as a *range* test
  (assert `signed_ranges()` and `publisher_region()` are exactly the right spans of the input), and
  the cryptographic half belongs to task 8's vectors.
- **@test Q2** — extensions are validated and typed at decode; `extensions_malformed` is detected
  inside decode.
- **@media-handler #2** — values: `PUBLISHER_FIXED_PREFIX_SIZE` 10, `WRAPPED_TRANSMIT_KEY_SIZE` 50,
  `EXT_LENGTH_FIELD_SIZE` 2, `RELAY_REGION_SIZE` 6, `SIGNATURE_SIZE` 64, `MAX_PAYLOAD_BYTES`
  1 048 576, `SFRAME_OBJECT_OVERHEAD_BYTES` **24** = `KEY_ID_BYTES (8) + AEAD_TAG_BYTES (16)`, both
  defined exactly once here and not duplicated from the SFrame spec. No fixed relay-region offset
  constant (R-2). `MAX_EXT_BYTES` is derived from the registry, **3** today (R-22). Added at R-5:
  `MAX_HEADER_BYTES` **71** and `MAX_FRAME_BYTES`, both derived — 71, not the 126 this section
  originally carried, because the parser validates through the relay region before it will report a
  length, so the relay region is part of what a reader must buffer (see §Issues Encountered).
- **@media-handler #3** — nothing in the API requires a key, a decrypt, or any SFrame
  interpretation. The wrapped transmit key and the payload are opaque slices; the only structured
  thing inside the 50-byte field is `kek_generation`, which the *receiver* inspects for §4 rotation
  and MH ignores. No media-type field exists.
- **@media-handler #9** — `proto/test-vectors/` is **story task 8**, protocol-owned (me), which
  depends on this task; it is not in this diff. Nothing is spun out and no ownership gap exists.
- **@dry-reviewer** — agreed and on the record: Rust↔TypeScript duplication of this layout is
  designed-in and task 8's drift guard is the mechanism. Not a finding on either side.

### Order of work

1. `frame.rs` — constants, `FrameFlags`, `WrappedTransmitKey`, `MediaFrameView`, redacting `Debug`.
2. `extensions.rs` — TLV registry, validating parser, canonical encoder.
3. `codec.rs` — `RejectReason` / `DecodeError` / `EncodeError`, the four decode entry points,
   `rewrite_relay_region`, `encode_frame`, the three invariant comments.
4. Delete `stream.rs`; rewrite `lib.rs` docs; trim `Cargo.toml`.
5. Unit tests + `tests/byte_coverage.rs`.
6. `tests/common/mod.rs` + `tests/corpus_seeds.rs` (R-15, R-21); rewrite both fuzz targets; run them.
7. Doc pointer updates (protocol INDEX, client INDEX, WEBTRANSPORT_FLOW, FUZZING,
   FUZZ_THIS_CHECKLIST).

### Open decisions for Gate 1

| # | Decision | My position | Ruler |
|---|---|---|---|
| 1 | Reject reasons: 6 → 8 (`trailing_bytes`, `extensions_too_large`) | Add both — see the vocabulary table in Revision 1. @security, @operations, @observability, @test, @media-handler, @dry-reviewer all back `trailing_bytes`; @observability asks for `extensions_too_large` | @team-lead |
| 2 | `payload_length_exceeds_*` naming vs tree convention | Keep the task-mandated spellings; record the exception in the taxonomy | @team-lead, @observability |
| 3 | TLV: reject unknown types (vs skip) | **RULED: reject** — @security, @media-handler, @observability all agree §2 fail-closed governs | @security, @media-handler |
| 4 | `MAX_EXT_BYTES` value | **CLOSED: derived from the registry** (= 3 today) per R-22 — confirmed by @security, @media-handler and @observability | @media-handler, @security, @observability |
| 5 | v2 extension registry = `0x01` salience, 1 byte | **RULED: confirmed** by @media-handler and @security (with the per-type exact-length rule, R-6) | @media-handler |
| 6 | Delete `stream.rs` vs replace with `HopSequence` | **RULED by @media-handler: delete, and do NOT add `HopSequence` here** — `hop_sequence` semantics are MH-owned runtime concerns to be designed in task 16 against the real forward path, not pinned prematurely in a stateless wire crate | @media-handler |
| 7 | Drop unused `tokio`/`serde`/`tracing`/`common`/`criterion` deps | **RULED: drop** by @observability — removing `tracing` converts §11's no-log-macro rule from a request into a compile error | @code-reviewer, @observability |

### Plan Revision 1 — reviewer rulings folded in (pre-approval)

Everything below supersedes the plan text above where they conflict. Nothing here is a redesign;
the wire layout is unchanged.

**R-1 (@security P1, load-bearing) — one layout parser, four entry points.** A single private
`parse_layout(&[u8]) -> Result<Layout, DecodeError>` returns every offset, and
`decode_datagram`, `decode_stream_frame`, `peek_frame_len` and `rewrite_relay_region` are all built
on it. The plan's "rewrite_relay_region parses the header prefix" was a *second, weaker* parser over
the same bytes — the classic parser-differential shape, where two parsers agree on every well-formed
input and disagree on exactly the malformed ones an attacker constructs. Concretely: a frame with a
well-formed `ext_length` but a malformed TLV body would have its relay region written by MH at one
offset and read by the receiver at another. `rewrite_relay_region` therefore performs the **same full
structural validation** as decode — it is zero-copy and cheap, and MH should not write into a frame
it has not validated. The differential becomes unrepresentable rather than merely tested against.
Also closes @dry-reviewer E: `peek_frame_len` cannot re-derive header length separately, because
there is only one derivation.

**R-2 (@security P2, load-bearing; supersedes @media-handler's fold-in and @dry-reviewer C) —
`FIXED_AUDIO_RELAY_REGION_OFFSET` is deleted, not renamed.** It is correct only when
`key_bearing == true` AND `ext_length == 0`, and a **publisher controls both**. Exported as a bare
constant labelled "audio fast path" it invites the unchecked use that breaks both premises: a
participant sending one extension moves the real relay region to 65, so MH writing at 62 corrupts
three bytes of the signed publisher region and the payload start — which makes every frame of that
stream fail signature verification, i.e. **any participant can silently kill any other
participant's audio at the relay**; and MH *reading* `stream_id` at 62 reads publisher-controlled
extension bytes as a routing decision, letting the publisher pick its own subscriber slot. The
premise is not even stable, since §7 has MH reading the extension region for selection input.
**Resolution, after @security and @media-handler converged: delete it and add nothing in its
place.** I had offered a checked `relay_region_offset(buf) -> Result<usize, DecodeError>` as the
fallback; @media-handler ruled there is no consumer for it — their write path is covered entirely by
`rewrite_relay_region` re-deriving internally, and their read/selector path goes through validated
`MediaFrameView` accessors, never a raw index. Shipping a checked offset function anyway would be
"a future trap with better manners": the next person with a hot-path idea finds a function whose
name promises an offset and uses it where the checks are wrong for a different reason. **Smallest
correct surface is no surface.** With R-1 the full walk is a handful of bounds checks and no
allocation, so the "skip parsing entirely" fast path was buying very little.

**R-3 (@security P3 = @observability OB-A) — the incomplete/terminal rule is restated so it
decides every step.** New rule: **any shortfall of available bytes against a required region length
— fixed or declared — is *incomplete* on the stream path; only a violation of a limit or of the
grammar is *terminal*.** The old wording ("shortfall against a *declared* length") was silent on
every shortfall against a **fixed** size — an empty buffer, a buffer short of the publisher prefix,
a key-bearing frame short of the wrapped-key field, a buffer short of the extension-length field,
short of the relay region, or short of the signature — so a TypeScript implementer in task 15 could
read it literally and return a reject where Rust returns `Ok(None)`. That is exactly the divergence
O-1 exists to prevent. (Cited by condition, not by row number: the table has been renumbered once
already, and a positional reference into it is the trap deleting the numbered list was meant to
close.) Note the consequence, which @observability needs for task 22 and @operations for task 21:
**`ProducibleBy::reachable_on_stream_carried_frames()` is the accessor an alert selector or runbook
must consult** — not the variant name, and not a list copied from here. Only `trailing_bytes` is
absent from stream-carried frames; `truncated` and `payload_length_exceeds_available` are reachable
there through `rewrite_relay_region`, which is the claim OPS-1 corrected.

**R-4 (@security P4 + @observability OB-B) — the `Ok(None)` caller obligation is named.** A peer that
opens a stream and stalls mid-frame produces `Ok(None)` forever: no token, no counter, no timeout —
silent by construction, which CLAUDE.md's fail-loudly rule forbids. The codec sees one buffer and
has no notion of time or stream count, so this cannot be enforced here. It goes in the same module
rustdoc paragraph as the `stream_id` obligation: *an `Ok(None)` outcome must be paired with a
caller-side bound on both buffered bytes per stream and time-to-completion, and the give-up must be
counted.* Recorded in `docs/TODO.md` and carried to task 16 via @team-lead.

**R-5 (@security P5 + @observability OB-B) — two more derived constants.**
`MAX_HEADER_BYTES = PUBLISHER_FIXED_PREFIX_SIZE + WRAPPED_TRANSMIT_KEY_SIZE + EXT_LENGTH_FIELD_SIZE
+ MAX_EXT_BYTES + RELAY_REGION_SIZE` (shipped value **71**) is the number of bytes a reader must
buffer before `peek_frame_len` can possibly answer — which is what makes "bounded before the length
is knowable" checkable rather than asserted. **It includes the relay region**, because `parse_layout`
validates through the relay region before it will report a length; an earlier draft of this
paragraph omitted that term and gave 65, which under-allocates by six bytes. Consequently
`MAX_FRAME_BYTES = MAX_HEADER_BYTES + MAX_PAYLOAD_BYTES + SIGNATURE_SIZE` — it must **not** add
`RELAY_REGION_SIZE` again, which the same earlier draft did, double-counting it. Both derived, no
new literals; otherwise MH and the SDK each recompute them, and tasks 15 and 16 both size buffers
from these two constants.

**R-6 (@security ruling 2a) — the extension registry carries an exact expected value length per
type, and a mismatch is a reject.** Exact-region-consumption does *not* close this: type `0x01` with
`length = 5` and `ext_length = 7` consumes the region exactly, passes ascending-order and
no-duplicates, and hands the receiver a 5-byte value for a type defined as 1 byte — an unvalidated
publisher-controlled span, the reserved-byte channel rebuilt inside a TLV. With the per-type length
check the extension region is **fully determined** by which subset of known types is present, i.e.
zero free bits; that property is stated in the rustdoc.

**R-7 (@security ruling 2b) — WITHDRAWN AND DELETED. See R-26.** @security re-examined this ruling
after @observability's value-set clause and reversed it: each registry entry declares an accepted
value set, enforced at decode. The withdrawn text has been **deleted rather than struck**, because
it read as a live prohibition on exactly the check R-26 makes mandatory, and a reader who found it
would build a decoder that accepts values R-26 rejects. R-26 is the whole of the current position;
there is nothing to reconstruct here.

**R-8 (@security P6) — `encode_frame` enforces the whole extension grammar**, not just ascending
order: unknown types, duplicates, wrong value length and an oversize region are all encode errors.
Otherwise the encoder can emit bytes the decoder rejects, `encode(decode(x)) == x` stops being total
over the encoder's output, and the roundtrip fuzz target reports it as a bug in the wrong place.
The roundtrip target's comment records **why** the byte-identity assertion matters and must not be
weakened to field-equality: canonical encoding plus byte-identical roundtrip means decode is
**injective** over the accepted language — no two distinct byte strings decode to the same view —
which is the no-covert-channel property proved independently of the byte-coverage test. Two
independent proofs of the one control this task exists to establish.

**R-9 (@security P7/P8, restated after `HopSequence` moved to task 16) — the reset asymmetry lands
as rustdoc at the two accessors in THIS diff.** @media-handler ruled `HopSequence` out of this crate
(no consumer-less speculative state; `hop_sequence` semantics are MH-owned runtime concerns for task
16), which is right — but it would have moved the *asymmetry* out with it, and the asymmetry is the
durable half. Carried only in task 16's prompt it exists nowhere until task 16 lands and nowhere at
all if task 16 is re-scoped: the same control-that-has-to-notice problem as the `stream_id`
obligation. So two rustdoc lines, at the accessors, where they cannot drift from the format:
- `hop_sequence()` — resetting is **harmless**: unauthenticated, per (connection, media stream), and
  §2 notes a transmitter lying about its own send count conceals only drops it could already perform.
- `stream_sequence()` — this is the **AEAD nonce input**; it must never be reset, and no counter
  wrapper for it may be built with a reset API, because a reset at the wrong moment relative to a key
  swap is authentication-key recovery under GCM, not merely a confidentiality loss (§2, §4).
The *contrast* is the point: a future reader who meets a resettable hop counter in task 16 must not
build the symmetric type for the stream counter. Task 16 then inherits the constraint from the format
docs and @media-handler's prompt-carry becomes belt-and-braces rather than the only copy.
(P8) `PartialEq` on `MediaFrameView` is not constant-time and exists for tests and the roundtrip
property; one rustdoc line says it must never back an authentication or authorization decision.

**R-10 (@observability OB-C) — `extensions_malformed` splits in two.** I accepted the collapse on
the basis that sub-causes stay available in `Display`; my own O-8 answer then closed that hatch —
§11 bans per-frame dimensions on the media path, so in production the sub-cause is unreachable and
the collapse is undiagnosable. Split into `extensions_too_large` (`ext_length > MAX_EXT_BYTES`) and
`extensions_malformed` (TLV walk fails, duplicate type, unknown type, wrong value length,
non-ascending order, zero-length entry). Opposite operator responses: the first says a bound *we*
chose was outgrown, the second says hostile or corrupt input. Especially clean today — the v2
registry is one 3-byte entry against a 64-byte bound, so any `extensions_too_large` in this story is
unambiguously hostile. Cardinality is not the constraint (budget is 1000 combinations per metric).

**R-11 (@security ruling 4 + @operations; Lead ruling 4) — four `docs/TODO.md` entries**, because a
prompt-carry is a control that has to *notice*: if a downstream task is re-scoped or re-prompted the
obligation evaporates and nothing fails. The Lead carries all four into the task prompts as well;
the TODO entry is the copy that survives.
1. **@security's `stream_id`-validation obligation** — the receiver validates `stream_id` against
   its own declared slots (§6) and rejects unknown ones, else a compromised MH gets 16 free
   unauthenticated bits per frame. Target: tasks 15/19; deleted when task 19 lands the check.
2. **The `Ok(None)` slow-subscriber bound** (@observability, @security) — a caller must pair the
   incomplete outcome with a bound on both buffered bytes per stream and time-to-completion, and
   **count the give-up**, else a peer that opens a stream and stalls mid-frame is silent by
   construction. Target: task 16; @media-handler has accepted ownership.
3. A dt-guard follow-up adding `transmit_key` / `wrapped_key` to `pii_vocabulary.rs` CATEGORY_A
   (@semantic-guard). Not this diff's surface and not needed here — this crate has no `tracing`
   dependency, so the name cannot be logged from it — but MH (task 16) and the SDK (task 19) do log,
   and that is where an unredacted key name would slip past. With @security's warning attached: the
   addition must **not** use the `\b…\b` word-boundary idiom from `rust_pii.rs` / `metric_labels.rs`,
   which cannot see inside compound names (`\btoken\b` matches neither `userToken` nor `user_token`);
   the working idiom is segment equality,
   `crates/dt-guard/src/ts_retained_credentials.rs:198::segments()`.
4. The fuzz-workspace exclusion, naming **both** excluded paths verbatim — `crates/ac-service/fuzz`
   *and* `crates/media-protocol/fuzz` (`Cargo.toml:22-25`) — with the consequence stated: 
   `layer-all.sh` compiles neither, and a target that does not build cannot fail. @operations is
   right that this is a pair, not an instance, and right that fixing it is toolchain work owned by
   test or infrastructure, not by a wire-format change. Naming only my own would leave the
   ac-service one to be rediscovered.

**R-12 (@dry-reviewer A) — `ALL_REJECT_REASONS` is `&[RejectReason]`, not `&[&str]`.** A string slice
would write the wire tokens twice, three lines apart, and a typo in the second copy would propagate
into task 8's vectors, task 18's fixtures and task 22's catalog — which would all agree with each
other and disagree with the codec, while the cross-language drift guard reported clean because both
sides were generated from the same wrong list. Consumers call `.as_str()`. Residual: the variant
list is still a hand-maintained *enumeration*, so a test contains an exhaustive `match` over
`RejectReason` with **no wildcard arm**, which fails to *compile* when a variant is added without
being added to the const.

**R-13 (@dry-reviewer B) — three more constants get the full cross-language SSoT treatment**, and
this is a commitment recorded here so task 8 cannot quietly ship without them: `MAX_EXT_BYTES`,
`LEGAL_FLAG_MASK` and the **extension registry** (type → exact value length) each get one Rust home,
an `ANCHOR (DRY):` comment naming `proto/test-vectors/frame-v2.vectors.json`, a corresponding entry
in that file, and drift-guard coverage — exactly as `MAX_PAYLOAD_BYTES` does. Without it, task 15's
TypeScript hardcodes `64` and `0x07` independently and nothing catches the drift; that is the same
failure the task's single-source mandate targets, for constants the task text merely did not happen
to enumerate. @security ruling 2c asks for the same thing about the TLV grammar and is satisfied by
the registry entry.

**R-14 (@dry-reviewer D) — negative fuzz seeds and byte-coverage mutations are built by mutating an
encoder-produced valid frame at named-constant offsets**, never by writing literal byte arrays.
Illegal flag bits, malformed TLV, `payload_length` at max+1 and every truncation boundary are all
unencodable by construction, so the hand-crafted alternative would bake the layout into the test
files as a second home. Field offsets (`VERSION_OFFSET`, `FLAGS_OFFSET`, `PAYLOAD_LENGTH_OFFSET`,
`STREAM_SEQUENCE_OFFSET`) are exported from the same `const` block.

**R-15 (@test A) — the `examples/` generator is dropped; everything moves into
`tests/corpus_seeds.rs`.** @test caught a real coverage-theatre trap: examples are compiled by
`clippy --all-targets` but their `main()` is never *executed* by `cargo test`, which the pipeline
runs — so the generator's per-seed assertions would have been compile-only, lint-clean and never
run. Instead, one file holds the seed table and its expected-outcome assertions (executed on every
`cargo test --workspace`), plus a single `#[test] fn regenerate_fuzz_corpus()` that writes the blobs
to the gitignored `fuzz/corpus/**` deterministically on every run. One table, assertions actually
execute, and "regenerate the corpus" becomes "run the tests".

**R-16 (@test B) — the codec does not branch on `SFRAME_OBJECT_OVERHEAD_BYTES`**, and the rustdoc
says so. The payload is fully opaque here (the task: "a documented size-only constant with no
SFrame parsing"); no minimum-payload check is derived from it, because that would couple the header
codec to payload semantics. So a literal value-pin is the strongest available test, and the
not-possible determination is stated rather than silent. Task 8's `full_frame` row is the
behavioural cross-check.

**R-17 (@test C) — confirmed**: the view exposes `payload()` and `signature()` as observable
slices/ranges, so `tests/byte_coverage.rs` genuinely covers those regions. Without that the
signature would be a 64-byte hole in the no-skipped-byte proof, since decode does not validate it.

**R-18 (@operations) — a two-concatenated-frames test locks the datagram/stream asymmetry.** In
`decode_stream_frame` the bytes after the signature *are the next frame* (§2), so if the exactness
check is ever refactored into the stream path every video stream rejects everything after its first
frame — which presents as "video works for exactly one frame then dies" and reads like an encoder or
keyframe bug. Test: two concatenated valid frames yield the first from `decode_stream_frame` with
`encoded_len()` pointing at the second, and never a `trailing_bytes` error.

**R-19 (@observability OB-D) — a redaction test, because a hand-rolled `Debug` is a convention and
conventions get "cleaned up" into `#[derive(Debug)]`.** Build a frame with recognisable sentinel key,
tag, signature and payload bytes; format `MediaFrameView`, `WrappedTransmitKey`, `DecodeError` and
`EncodeError` through both `{:?}` and `{}`; assert none of those byte sequences appear. Precedent:
`crates/common/src/secret.rs::test_debug_is_redacted`. This is the one control in the diff that was
still vigilance-based.

**R-22 (@security, offered as non-blocking; taken) — `MAX_EXT_BYTES` is derived from the registry,
not fixed at 64.** `MAX_EXT_BYTES = EXT_REGISTRY_TOTAL_BYTES`, the sum over the registry of
`(EXT_TYPE_FIELD_BYTES + EXT_LENGTH_FIELD_BYTES + value_len)` — **3 today**, one salience entry.
It cannot reject a legitimate frame: unknown types and duplicates are already rejected, so a valid
region is a subset of the registry with each type at most once, whose maximum size is exactly that
sum — meaning the fixed 64 was a bound that **could never fire on anything the grammar would have
accepted**, i.e. a dead check. Deriving it makes it live, tightens the pre-slice bound from 64 to 3,
and grows automatically with the registry. It also makes `extensions_too_large`'s operator meaning
("no legitimate cause — this is hostile") true **by construction and permanently**, rather than an
accident of the gap between 3 and 64 that would silently degrade as the registry grew. CLAUDE.md's
derive-one-from-the-other rule, and for task 8 it means the vectors pin one fact (the registry)
rather than two that can disagree. **SETTLED: adopted.** @media-handler re-confirmed the derived
form from the consumer side, withdrawing their own headroom argument for 64 (new extension types are
wire changes that grow the registry anyway), and @observability — whose token's meaning this changes
— endorsed it, noting their "unambiguously hostile" claim was true only because of the accidental
3-vs-64 gap and would have decayed silently as the registry grew. `MAX_HEADER_BYTES` shrinks by
the same 61 bytes, landing at its shipped **71** — see R-5 for the derivation, which is the one
place that formula should be read from.

Two consequences that ride with it:
- **R-10's operator meaning is re-documented.** @media-handler caught that a derived bound is no
  longer a *chosen capacity limit*, so `extensions_too_large` is not a capacity signal: it means
  *the declared extension region is larger than the registry could ever produce — structurally
  impossible, therefore hostile — detected before the TLV walk, so no per-byte work is done on it.*
  The chosen-bound language from R-10 is dropped so the two do not contradict. @security's point
  applies in **either** design and goes in the rustdoc regardless: a token documented as a capacity
  limit teaches operators to raise the limit, and for a fail-closed parser bound that reflex is
  exactly wrong — raising it widens the publisher-controlled header span this task exists to close.
  The remediation is never "raise the bound"; it is "investigate the sender".
- **R-13 grows, and task 8 owns it.** Once the bound is derived, **the registry *is* the wire
  bound**, so task 8's vectors must pin the registry itself — each entry's type byte and exact value
  length — not merely a `max_ext_bytes` scalar. If the TypeScript codec derives its bound from its
  own copy of the registry and the two drift by one entry, the implementations compute different
  bounds and reject different frames while a guard comparing only scalars reports clean: the O-1
  failure one level down. The drift guard asserts the registry; both sides derive the bound from it.

**R-21 (@dry-reviewer, Gate-2 watch item taken now) — one shared canonical-frame builder.**
`tests/corpus_seeds.rs` and `tests/byte_coverage.rs` both need the same two capabilities — build a
canonical valid v2 frame, and mutate it at a named-constant offset — which is a two-consumer pattern
at the second use, this repo's extraction point. Two private `fn canonical_frame()` helpers would be
two builders that must agree on the v2 layout and would diverge on the first header change, i.e.
R-14 one level up. Both live in `crates/media-protocol/tests/common/mod.rs`, matching the existing
crate-local convention (`crates/mc-service/tests/common/mod.rs`, `crates/mh-service/tests/common/`).

**R-20 — `crates/media-protocol/fuzz/Cargo.toml` needs no edit** (target names and paths unchanged).
Recorded so the absence is deliberate rather than missed (@security's ask).

### Vocabulary: eight tokens, two beyond the task's six — @team-lead rules

The task enumerates six. Two additions are proposed, each backed by two reviewers, and both are free
now and a coordinated cross-language change after task 8 (which I own):

| Token | Why it must exist | Backed by |
|---|---|---|
| `trailing_bytes` | The signature covers publisher ‖ payload, so a tail is covered by **nobody** — a compromised MH can append bytes and the frame still verifies. That is §2's six-unvalidated-reserved-bytes covert channel restated one field to the right. Rejecting the tail closes it; without a distinct token the control is alive and unobservable, and mapping it onto `truncated` is a semantic inversion (too few bytes vs too many) that sends an operator down the transport-loss ladder for a security event. `decode_datagram` only. | @security, @operations, @observability, @test, @media-handler, @dry-reviewer |
| `extensions_too_large` | The sub-cause is unreachable in production once §11 bans per-frame `Display` on the media path, so the collapsed token is undiagnosable. Opposite operator responses: *structurally impossible for a well-formed sender* vs a grammar violation in a present region. | @observability |

The two task-mandated spellings `payload_length_exceeds_max` / `payload_length_exceeds_available`
are **kept as written** despite having no verb-phrase precedent in the tree (@observability O-4);
@observability, @dry-reviewer and @media-handler all support keeping them rather than paying a
cross-language rename after task 8. The convention exception gets recorded in the taxonomy row.

### Plan Revision 2 — final Gate-1 items

**R-23 (@observability ruling on R-22/R-10; @operations' three asks) — the extension registry and
its two tokens are documented once, verbatim, in three places.**

`MAX_EXT_BYTES` is derived (Package A). R-10's chosen-bound language is **struck, not softened** —
under a derived bound it is simply false, and @observability's and @security's decisive reason is
recorded as decisive: *a token documented as a capacity signal teaches an operator the reflex "raise
the limit", and for a fail-closed parser bound that reflex widens the publisher-controlled span in
the header — the exact covert-channel surface this task exists to close.* Under the derived form the
constant is not tunable at all, so the wrong response is unavailable rather than merely
undocumented.

The split is retained over @security's fairly-stated counter (two hostile-input tokens sharing one
response verb), on @observability's three grounds: (1) same verb, different investigation — under
§11's ban on per-frame `Display` the token is the *entire* production diagnostic channel, and
`extensions_too_large` asks *what registry is that sender built against* while
`extensions_malformed` asks *what is wrong with that sender's encoder*; (2) different cost shape —
one scalar comparison before the walk versus a grammar verdict reached by per-byte work, which is
the distinction a saturation incident turns on; (3) **cost asymmetry** — after task 8 the vocabulary
is cross-language frozen, and merging two tokens later is a documentation change while splitting one
later is a wire-contract change across two codecs, the vectors, task 18's fixtures, the catalog,
dashboards and runbooks. When one direction is cheap to reverse and the other is not, take the
reversible one.

Token text, used verbatim in the rustdoc so the taxonomy row and task 21's ladder can quote rather
than paraphrase:

> `extensions_too_large` — the declared extension region is larger than the registry could ever
> produce. Structurally impossible for a well-formed sender, therefore hostile or corrupt. Detected
> before the TLV walk, so no per-byte work is done on it. Producible by: datagram, stream. Operator
> response: investigate the sender's build and registry version; **never raise the bound — it is
> derived, not chosen**.

> `extensions_malformed` — the extension region is **entirely present** and within the size the
> registry permits, but violates the grammar: TLV walk overruns, unknown type, duplicate type,
> wrong value length for the type, **a value outside the type's declared accepted set**,
> non-ascending type order, or a zero-length entry. A region *shorter* than the declared
> `ext_length` is a shortfall, not a grammar violation, and is `Ok(None)` on the stream path.
> Usually a sender encoder defect or a sender built against a different registry revision; also
> reachable by corrupt or hostile input. Producible by: datagram, stream. Operator response:
> investigate that sender's encoder and the registry revision it was built against.

Per @operations, the **"producible by" line stays adjacent to the operator meaning in the same
rustdoc block**, never in a separate table — the two get read together at 3am or not at all.

**Durability (@observability, strengthened by @security).** The derivation
`sum over registry of (type + length + value_len)` is well-defined only because every entry today
has a *fixed* value length. @observability asked that every entry declare a fixed or maximum value
length with the derivation using the maximum. @security showed that "maximum" alone **reopens the
hole ruling 2(a) closed**: the 2(a) property is that the extension region is fully determined by
which subset of known types is present — zero free bits — which a fixed length gives for free but a
max length does not. If a value can legitimately be encoded at more than one length with the same
meaning (leading zeroes, padding, any non-minimal encoding), the length byte becomes a
publisher-chosen free variable carrying information the receiver does not act on: the reserved-byte
channel one level down, passing known-type, no-duplicate, ascending-order, exact-consumption and
length ≤ maximum. It also breaks P6's argument — byte-identical roundtrip still holds because decode
preserves received bytes, but *semantic* injectivity fails, and the roundtrip fuzz target cannot see
that.

So the registry rustdoc rule is: **every entry declares either a fixed value length, or a canonical
variable-length encoding in which the length is a function of the value — no two distinct encodings
may carry the same meaning.** The derivation uses the fixed length or the declared maximum. When the
first variable-length type is proposed, canonicality is a **decode-time check in that type's
parser**, not a comment. For v2 this changes nothing (`0x01` is fixed at 1 byte and the property
holds trivially); it is written now because the natural repair under pressure is a literal, the
second natural repair is a max-length type with a sloppy parser, and both roads end at a
publisher-controlled span inside a signed header.

**@operations' three asks, answered in the rustdoc:**
1. **An unknown TLV type is rejected, not skipped** — stated at the registry, so nobody assumes TLV
   forward-compatibility.
2. **Adding an extension type is therefore a version bump**, and that is what keeps
   `extensions_too_large` free of a legitimate cause. The rustdoc names §2's meeting-wide,
   MC-directed version as the reason the bump is *safe* rather than a fleet-skew hazard: MC never
   composes a mixed-version meeting, so a newer publisher cannot meet an older receiver. @operations
   reconstructed this chain correctly from §2; the point of writing it down is that a future author
   will not.
3. **Answered honestly: no guard catches a registry addition made without a version bump.** The
   vectors drift guard catches Rust-vs-TypeScript disagreement and, because both codecs derive
   `MAX_EXT_BYTES` from the pinned registry, it catches a registry that drifts between languages.
   It does **not** catch "both languages agree, and both grew the registry without bumping the
   header version" — which is the single way `extensions_too_large`'s operator text silently becomes
   false later. Recorded as a `docs/TODO.md` line and flagged to task 8 as a candidate guard (pin
   the registry per header version, fail when the registry changes without the version). Named
   rather than assumed, per @operations.

**R-24 (@operations) — the parser-differential test is round-trip, not two agreeing call sites.**
Their operational point is the half I had not stated: a parser differential here is **invisible to
MH's own telemetry** — MH computes an offset, writes six bytes, every counter it owns reports
success, and the frame is malformed only from the receiver's point of view, which MH structurally
cannot observe. That is §8's partial-blackhole-reporting-healthy shape arriving by a different
route. Since `rewrite_relay_region` re-derives through the *same* `parse_layout` the decoder uses
(R-1), the covering test is: encode → rewrite → decode, asserting the rewritten `stream_id` and
`hop_sequence` read back and that the publisher region and the 64-byte signature are byte-identical.
@test adds the adversarial shape that matters most and that I would have under-covered: the rewrite
test must use a **key-bearing frame with a non-empty extension region**, because the zero-extension
case is precisely the one that hid the deleted-constant bug.

**R-25 (@code-reviewer) — classification table reconciled with the changed file set.** The
`examples/gen_fuzz_corpus.rs` row is gone; `tests/corpus_seeds.rs` and `tests/common/mod.rs` are
added, both Domain-judgment (GSA co-sign, owner media-handler). **No `src/parse.rs` row is needed:
`parse_layout` is a private function inside `codec.rs`, not a new module** — it is the shared
implementation the four public entry points call, and giving it a module would make it look like
public surface. The Layer-B guard reports clean on the updated table.

### Plan Revision 3 — @security's amendment to ruling 2(b)

**R-26 — each registry entry declares its accepted value set, and out-of-set values are rejected at
decode.** This supersedes R-7, which recorded @security's original ruling 2(b) (no range constraint
on the salience byte). @observability proposed the clause; @security re-examined 2(b) and adopted
it, and their sharpened reasoning is what gets carried, because the generic label would misapply it:

> **The extension region is cleartext; the payload is not.** Publisher→receiver is already an
> unbounded channel, so constraining salience buys nothing *there* — which is why my first
> re-examination nearly re-confirmed 2(b). But publisher→*anyone who cannot decrypt* is bounded, and
> cleartext header fields are exactly that channel: a compromised client signalling to MH, to an
> on-path observer, or to a party holding captured ciphertext, using bits no key protects. At 8 bits
> per frame and 50 fps that is ~400 bits/s per stream — the same order as, and the same *kind* as,
> the ~180 B/s reserved-byte channel §2 calls out, since v1's reserved bytes were also
> cleartext-and-unvalidated.

So the rustdoc records the *cleartext, receiver-inspected span* criterion rather than a generic
covert-channel note, because the criterion is also what says where the rule does **not** apply: the
relay region gets accepted-and-bounded treatment instead (cleartext, but necessarily MH-writable),
and nothing analogous is owed for the payload (not cleartext). @security notes this is also the
honest rationale for ruling 2(a) — the surplus bytes in a length-confused TLV are cleartext and
unvalidated — so 2(a) stands unchanged with its reasoning sharpened rather than restated.

**Structural form, per @security: declaration in the type, not in prose.** The registry entry type
makes the accepted set a required field, so an entry cannot be added without declaring one:

```rust
struct ExtensionSpec {
    ext_type:  u8,
    value_len: usize,          // fixed; or a canonical variable-length encoding (R-23)
    accepted:  AcceptedValues, // no default, no Option — the entry does not compile without it
}

enum AcceptedValues {
    /// Single-byte value constrained to this inclusive range.
    ByteRange { min: u8, max: u8 },
}
```

The enum is deliberate rather than a bare `RangeInclusive<u8>`: a future multi-byte extension type
must **add a variant**, which is a decision point a reviewer sees, instead of quietly reaching past
a `u8`-shaped field. Same structural-impossibility posture as `parse_layout` (R-1) and the deleted
offset constant (R-2) — the wrong thing is unrepresentable, not merely discouraged.

**The two clauses compose, and the rustdoc says so in one sentence**, because each looks sufficient
in isolation: **canonical encoding (R-23) closes the channel in the *length*; the accepted-value set
(R-26) closes it in the *value*. A registry entry needs both to have zero cleartext free bits.**
Together with known-types-only, no-duplicates, ascending order and exact consumption, that is the
full statement of ruling 2(a)'s property — the extension region is completely determined by which
subset of known types is present.

**Open, but not blocking (@security is explicit that I must not pick this myself).** The accepted set
for type `0x01` must be declared by the owner of salience semantics. §7 defines no scale and leaves
ranking, debouncing and signal combination as selector design surface — story task 5, @media-handler's
domain. @security has asked them directly and I have confirmed I will take their answer. **Default if
no answer arrives before implementation: declare `AcceptedValues::ByteRange { min: 0, max: u8::MAX }`
explicitly** — the clause is "declare and enforce", not "narrow for its own sake" — and record the
residual 8 bits/frame in the *same rustdoc paragraph* as the relay region's 48 bits, since @security
is right that it is the same species and deserves the same accepted-and-bounded treatment rather
than silence. Narrowing later is a one-line change to a declaration that already exists, which is
the whole point of building the mechanism first.

**Task-8 carry (@observability's formulation, which is the one to use in the vectors' rationale):**
*positive vectors prove the **encoders** agree; only negative vectors prove the **decoders** agree —
and it is the decoders that face hostile input.* One negative case per rejection rule: unknown type,
duplicate type, non-ascending order, wrong value length, and now out-of-range value.


### Gate-2 review round — ten findings from six reviewers, all fixed

No deferrals. Every finding was in-changeset with no design ambiguity, which is the
suspicious-deferral shape the protocol says to fix rather than argue.

| # | Reviewer | Finding | Fix |
|---|---|---|---|
| OB-E | @observability | R-3 cited evaluation steps **by ordinal** after the table was renumbered 12→14 rows, so it named the terminal grammar case as a stream-path incomplete; and its consequence sentence listed 2 of 3 datagram-only tokens | Cite by **condition**, never position; consequence now points at `reachable_on_stream_carried_frames()` and names the one true absence (`trailing_bytes`) rather than copying a set |
| OB-F | @observability | `MAX_HEADER_BYTES` still **65** in R-5 — the section that *defines* it — omitting `RELAY_REGION_SIZE`, and `MAX_FRAME_BYTES` double-counted it | Formula corrected to include the relay region (**71**); `MAX_FRAME_BYTES` no longer re-adds it |
| OB-G | @observability | The struck "a bound we chose was outgrown" framing survived in the justification table | Replaced with the structurally-impossible framing |
| T-1 | @test | Stream-path "terminal even on a short buffer" was **untested** — every stream `Err` assertion fed a *complete* frame, and the prefix test only used prefixes of *valid* frames | New `a_violation_visible_in_a_short_prefix_is_terminal_on_the_stream_path` |
| SG-1 | @semantic-guard | `MediaFrameParts` **derived** `Debug` while holding raw `payload` and `signature` — contradicting the Gate-1 guarantee, and R-19 did not cover it | Hand-rolled redacting `Debug`; `MediaFrameParts` added to the R-19 sentinel test |
| OPS-1 | @operations | `producible_by()` omitted `rewrite_relay_region`, a **fourth** fallible entry point that MH calls on stream-carried video — so `DatagramOnly` was a *false* claim in the mis-triage direction | `ProducibleBy` reworked to an entry-point model; `operator_meaning` for both dual-cause tokens forks on entry point; cross-check extended to all four |
| DRY-F1 | @dry-reviewer | `all_reject_reasons_is_complete` tested only for **duplicates** — nothing forced a variant into `ALL_REJECT_REASONS` | Vocabulary now **macro-generated**: enum, tokens and list from one source, so omission is unrepresentable |
| DRY-F2 | @dry-reviewer | `ext_length_offset()` was a **parallel derivation of a conditional layout rule** — third instance of the mechanism | New `MediaFrameView::ext_length_field_range()` / `extensions_range()`; helpers read from the decoded frame |
| DRY-F3 | @dry-reviewer | Sentinel arrays restated their own length constants as literals | `[0xA1; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES]` etc. |
| CR-1 | @code-reviewer | Four `#[allow(...)]` where ADR-0002 requires `#[expect(..., reason)]` — clippy cannot catch this, it is a manual-review item | All four converted with reasons |
| CR-2 | @code-reviewer | `spec_for()` hand-rolled a `while` loop with an unreachable arm | `EXT_REGISTRY.iter().find(...)`, 7 LoC → 1 |
| MH-1 | @media-handler | Two idioms for one operation — `checked_add` in some offset advances, `saturating_add` in others, including **both inside one loop** in `validate()` | Six sites unified on `checked_add`; `saturating_add` now appears only in the capacity hint, so the invariant is greppable |

**@operations' generalisation of their own finding, worth more than the finding.** OPS-1 and the
`extensions_too_large` question at Gate 1 were the same shape: *a classification that was true for
the paths someone had in mind and false for a path they had not enumerated* — and both failed toward
**"the operator is told this is impossible"** rather than toward a dead alert. The mis-triage
direction is worse than the dead-alert direction, because a dead alert is silent while a false
impossibility actively redirects an incident. What caught both was asking **what an operator would
conclude from the annotation**, not whether the annotation was internally consistent. Carry this into
task 22's catalog, where these same `producible_by` values are consumed mechanically and the
enumeration question arises again.

**Three of these are the same mechanism this loop keeps producing**, and that is the story-level
signal: OPS-1, DRY-F2 and SG-1 are all *a second authority for something that already has one* —
a producibility claim contradicted by a fourth entry point, a layout rule recomputed beside the
decoder, and a `Debug` impl for bytes that already had a redacting one. The two offset defects fixed
earlier were the same shape. The generalisation, stated once here rather than five times:
**when a fact has an owner, every other expression of it is a claim that can go stale — and the ones
that go stale silently are those that agree today.**

**One correction to a reviewer's suggested fix, made rather than accepted verbatim.** @test proposed
asserting that an undefined flag bit truncated to **two bytes** rejects. It does not, by design: the
normative order checks the publisher-prefix *length* before inspecting any field inside it, so a
two-byte buffer is `Ok(None)` whatever it contains. Implementing the suggestion as written would have
required reordering the parse to contradict the published table and the TypeScript port. The test
instead truncates to the prefix boundary — still far short of a complete frame, which is the property
@test was after — **and** asserts the two-byte case is `Ok(None)`, so the boundary is pinned rather
than left looking like a gap.

---

## Rollback Procedure

Revert-only, and clean **today**: no schema, no migration, no config key, no deployed consumer, no
rollout coordination, no data at rest (@operations verified zero `use media_protocol::` sites at
`6b4056e`). Nothing is stranded by a revert of this commit alone.

**One ordering caveat an operator would otherwise get wrong**: once story task **#8** lands
(`proto/test-vectors/frame-v2.vectors.json` plus the cross-language drift guard), that guard asserts
this crate's `MAX_PAYLOAD_BYTES` equals the vectors file's `max_payload_bytes`. From that point the
rollback unit is **#2 and #8 together** — reverting #2 alone leaves #8's guard pointing at a deleted
constant and fails the pipeline.

---

## Implementation Summary

The v1 42-byte header is gone. `crates/media-protocol` now implements the ADR-0036 §2 v2 format:
a publisher region (signed, and used verbatim as the AEAD associated data), a relay region the
media handler rewrites per subscriber and nobody authenticates, an opaque `SFrame`-object payload,
and a trailing 64-byte Ed25519 signature. Decode borrows; nothing is copied.

### What the crate exposes

**`frame.rs`** — the wire layout and the decoded view. Size constants derive from two atomics
defined exactly once in the workspace (`AEAD_TAG_BYTES = 16`, `KEY_ID_BYTES = 8`); `50` and `24`
appear as literals only in one intentional value-pin test each. `MediaFrameView<'a>` borrows the
caller's buffer, so a copy is unrepresentable rather than merely absent, and offers both slices and
`Range<usize>` accessors so a caller holding `Bytes` can slice with refcount sharing.

**`extensions.rs`** — the TLV grammar ADR-0036 mandates but does not specify. A registry entry is a
type byte, an exact value length and an accepted value set, all required fields, so an entry cannot
be added without declaring them. v2 has one entry: `0x01` declared salience, one byte, `0..=100`
(declared by @media-handler as the owner of salience semantics). Unknown types are rejected rather
than skipped, types must be strictly ascending, and the region is consumed exactly — so it is fully
determined by which subset of known types is present.

**`codec.rs`** — one private `parse_layout()` behind all four public entry
points, the eight-token reject vocabulary, and a canonical encoder.

| Entry point | Shape |
|---|---|
| `decode_datagram` | exact; a trailing byte is `trailing_bytes` |
| `decode_stream_frame` | prefix; `Ok(None)` is *incomplete*, not a rejection |
| `peek_frame_len` | reader-side pre-allocation bound |
| `rewrite_relay_region` | in-place 6-byte relay write, offset derived per frame |

### Deleted

`MediaFrame`, `HEADER_SIZE = 42`, `FrameType` (§2: the independently-decodable flag replaces a
media-type field; §7: MH stays type-blind), `FrameFlags::to_u16`/`from_u16` (the mask-away-unknown-
bits idiom under replacement), `CodecError::InvalidFormat(String)`, the six unvalidated reserved
bytes, and `src/stream.rs` entirely. No v1 decode arm and no version negotiation.
`Cargo.toml` drops `tokio`, `serde`, `tracing`, `common` and `criterion` — all unused. Removing
`tracing` is the point rather than hygiene: a log or metric macro in this crate is now a compile
error, which is §11's no-telemetry-in-the-forward-path rule made structural.

### What is enforced structurally rather than documented

| Property | Mechanism |
|---|---|
| No masking narrowing cast (invariant 3) | crate-level `deny(clippy::cast_possible_truncation, …)` under `-D warnings` |
| No telemetry from the forward path | no `tracing` dependency |
| No parser differential | one `parse_layout()` and no helpers — a single cursor, never recomputed, so there is exactly one answer to "where does the next field start"; `rewrite_relay_region` validates to the decoder's standard before writing |
| No hot-path relay offset misuse | no offset constant exists to misuse |
| No unsafe | `#![forbid(unsafe_code)]` |
| Reject-reason list cannot desynchronise | `ALL_REJECT_REASONS` is `&[RejectReason]`, plus an exhaustive match with no wildcard arm |
| Key material unrenderable | hand-rolled `Debug` on every type holding wire bytes, plus a sentinel-byte redaction test |
| Registry entry cannot omit its accepted set | required struct field, plus a `const` block asserting non-zero lengths and ascending types |

### Tests: 48, and what each family actually proves

- `reject_reasons.rs` (21) — one test per reason asserting the *specific* `RejectReason`; one per
  adjacent pair in the evaluation order; the producibility cross-check; the prefix invariant.
- `byte_coverage.rs` (3) — every byte position of all four shapes mutated: each must change an
  observable decoded value or reject. Every flag bit likewise. Plus a tiling assertion that the four
  regions cover the frame with no gap and no overlap.
- `frame_properties.rs` (20) — zero-copy asserted by **pointer containment**, not equality;
  byte-identical canonical roundtrip; derived constants exercised behaviourally *and* pinned;
  the relay rewrite over all four shapes; redaction with sentinel bytes.
- `corpus_seeds.rs` (4) — 558 seeds built through the encoder, each asserted, written to the
  gitignored corpus on every `cargo test`.

**The honest claim about this suite.** Every fixture is built by this crate's own encoder, so the
suite proves the codec **self-consistent and fail-closed** — *not* that the format is correct. An
encoder and decoder sharing one layout misreading pass all 47. The specific silent case
(@security): if both computed the publisher region one byte short, signatures would **verify
successfully while covering the wrong bytes**, leaving an unauthenticated byte in a header whose
premise is that a relay cannot alter it — roundtrip stays byte-identical, byte coverage still finds
every position load-bearing, every prefix still returns `Ok(None)`. ADR-0036 §2 describes only the
loud half of this, where the two implementations disagree and verification visibly fails. Only story
task 8's **frozen, externally anchored** vectors can catch the self-consistent case, which is
recorded in the rustdoc at `signed_ranges` so the four AAD-span rows are not later dropped as
redundant with the range tests here.

### Two invariants asserted rather than stated

1. **No strict prefix of a valid frame may ever produce `Err` on the stream path**
   (`no_prefix_of_a_valid_frame_ever_errs`). Truncating at every byte offset of all four shapes at
   two payload sizes. If this fails, a frame spanning a read boundary is being reported as a
   permanent structural fault, which under this crate's own consumer guidance means reset-the-stream
   with no resync — a remotely-triggerable stream kill.
2. **The relay write offset equals the decoder's read offset**
   (`rewrite_offset_equals_the_decoder_read_offset`), found by locating the changed byte rather than
   by comparing two computations. A differential here is invisible to MH's own telemetry: it writes
   six bytes, every counter reports success, and the frame is malformed only from the receiver's
   point of view.

---

## Files Modified

| File | Change |
|---|---|
| `crates/media-protocol/src/frame.rs` | Rewritten: v2 layout, derived size constants, field offsets, `FrameFlags`, `WrappedTransmitKey`, `MediaFrameView`, redacting `Debug`, the three invariants, the accepted-cleartext-channel note |
| `crates/media-protocol/src/extensions.rs` | **New**: TLV grammar, registry, `ExtensionError`, validating parser, canonical encoder |
| `crates/media-protocol/src/codec.rs` | Rewritten: `RejectReason` (8), `DecodeError`/`EncodeError`, `parse_layout()`, four entry points, canonical `encode_frame` |
| `crates/media-protocol/src/lib.rs` | Crate docs; `deny` for narrowing casts; `forbid(unsafe_code)`; module wiring |
| `crates/media-protocol/src/stream.rs` | **Deleted** — unreferenced v1-shaped state contradicting every v2 field width |
| `crates/media-protocol/Cargo.toml` | Dropped `tokio`, `serde`, `tracing`, `common`, `criterion` |
| `crates/media-protocol/tests/common/mod.rs` | **New**: encoder-built 2×2 fixture matrix and named-offset mutation helpers |
| `crates/media-protocol/tests/reject_reasons.rs` | **New**: 20 tests |
| `crates/media-protocol/tests/byte_coverage.rs` | **New**: 3 tests |
| `crates/media-protocol/tests/frame_properties.rs` | **New**: 20 tests |
| `crates/media-protocol/tests/corpus_seeds.rs` | **New**: 4 tests, 558 seeds |
| `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs` | Rewritten for v2; drives all four entry points |
| `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs` | Rewritten for v2; byte-identity assertion |
| `docs/specialist-knowledge/protocol/INDEX.md` | v2 navigation entries |
| `docs/specialist-knowledge/client/INDEX.md` | Line 61 byte-count removed (@client's wording); line 15 mislabel fixed |
| `docs/WEBTRANSPORT_FLOW.md` | v1 JavaScript example replaced with v2 fields |
| `docs/FUZZING.md` | Corpus regeneration command and the two properties worth copying |
| `docs/FUZZ_THIS_CHECKLIST.md` | v2 targets, generated seed corpus, the known pipeline gap |
| `docs/TODO.md` | New §Media Path Obligations — four entries |

`crates/media-protocol/fuzz/Cargo.toml` deliberately unchanged: target names and paths are the same.

---

## Devloop Verification Steps

| Check | Command | Result |
|---|---|---|
| Compile | `cargo check -p media-protocol` | pass |
| Lint | `cargo clippy -p media-protocol --all-targets -- -D warnings` | pass |
| Tests | `cargo test -p media-protocol` | 48 passed, 0 failed |
| Fuzz targets compile | `cargo +nightly check --all-targets` in `crates/media-protocol/fuzz` | pass (exit 0) |
| Fuzz corpus | written by `cargo test -p media-protocol --test corpus_seeds` | 558 seeds per target |

### Fuzzing was NOT executed — stated precisely

The fuzz targets are **compile-checked, not run**. `cargo-fuzz` is not installed in this container
(`rustc 1.95.0`, nightly toolchain present, `cargo-fuzz` absent), so no fuzzing was performed
anywhere in this task. "Fuzz targets updated" must not be read as "fuzzed".

What carries the coverage instead is `tests/corpus_seeds.rs`, which executes under `cargo test`:
558 seeds, each asserted for its expected decode outcome, and each driven through all four entry
points asserting no panic. That is the property a fuzzer searches for, asserted over a
deliberately-chosen input set rather than a random one.

Two structural gaps behind this, recorded in `docs/TODO.md` and **not fixable in a wire-format
change**: `Cargo.toml:22-25` excludes both `crates/ac-service/fuzz` and
`crates/media-protocol/fuzz` from the workspace, so `layer-all.sh` compiles neither; and the
container lacks `cargo-fuzz`. @test owns the structural fix as story task 23.

---

## Issues Encountered

**`MAX_HEADER_BYTES` was wrong, and a test caught it.** I defined it as prefix + wrapped key +
ext-length + max extensions (65), omitting the relay region — but `parse_layout()` validates *through*
the relay region before it will report a length, so a reader buffering 65 bytes could still get
`Ok(None)`. The constant contradicted its own documented meaning ("bytes a reader must buffer before
`peek_frame_len` can possibly answer"). Fixed the constant to 71, not the test. Worth noting because
it is exactly the class @security's P5 asked for the constant to make checkable: a derived bound is
only useful if it is derived from what the code actually requires.

**A clippy lint nearly undid a security finding, and the recovery is the useful part.**
After `cargo fmt` reflow, `clippy::too_many_lines` tripped at 111/100 on `parse_layout()` — the one
function @security's P1 exists to constrain. I split the leading fixed prefix into a private helper,
recorded it in this section as a lint fix, and **did not re-check it against P1**. The agreed name
drifted to `parse_header` in the same edit. Nothing mechanical was watching: clippy was *satisfied*
by the split, and every gate passed. @security caught it from an inconsistency in a status message.

**The trade, stated because it was a real decision and not a formality** (@security asked for it on
the record and declined to rule, holding that @code-reviewer and I were better placed): a private
helper with one call site plus a "do not add a second caller" rustdoc is a **documented rule** where
P1 bought a **structural guarantee** — the same substitution this review rejected in four other
places, and there is no reason it should pass here because the argument happens to cut toward more
code rather than less. Against that, a 111-line parser is genuinely hard to read, and readability in
a fail-closed parser has its own security value.

I merged it back. Two things decided it. First, @security's sharper differential test: disjoint byte
ranges only rule out the two functions disagreeing about a byte's *contents*, and the other half is
whether they disagree about an *offset*. They did — the split **recomputed** the resume offset in the
caller instead of returning it from the helper, so two places independently asserted where the
publisher region continues. Merging removes that surface; documenting it would not have.

Second, the readability objection evaporated on contact: merging also deleted the wrapper's plumbing
(a second `Shortfall` construction, the outer `match`, the tuple return), so the merged function is
**below** the threshold and needs no suppression at all. The structural guarantee and the lint agree;
nothing was traded. The rustdoc records that if a future edit pushes it over, the right move is
`#[expect(clippy::too_many_lines)]` naming the invariant, not another split.

**The generalisable lesson**: a lint-driven refactor is exactly where a review finding gets quietly
undone, because the change presents as formatting and the tool that forced it is satisfied by the
result. **When a lint forces a structural change to code a review finding constrains, re-check the
finding, not just the lint.** @security sharpened this further: a lint-driven refactor is the one
class of change that arrives **pre-justified** — the tool demanded it, the tool is satisfied by the
result, the diff reads as formatting, and every gate goes green. The pipeline could not have caught
it because the pipeline was the thing asking for it.

**And the second lesson, which is the one that actually cost the most.** Merging `parse_layout()`
felt like finishing the finding. It was not. The finding was about **offset authority** — who gets
to decide where a field starts — and `build_view` held a second claim to it (`let base =
PUBLISHER_FIXED_PREFIX_SIZE`) that the merge never touched. I only found it by applying
@security's stated review property to my own code before they did, one message after declaring the
finding closed. **A fix scoped to the function where a finding was reported is not the same as a fix
scoped to the property the finding was about.** The test for "is this closed?" is not "did I change
the code that was named?" but "can I still name a second place that decides the same question?"

Note the shape: the surviving instance was *latent*, not active. `build_view`'s recomputation agrees
with the cursor today and would have diverged only when a conditional field was added ahead of the
wrapped key. That is why it survived 47 passing tests, a byte-coverage proof and a fuzz roundtrip —
none of which can see a duplicate computation that currently agrees.

**Two other clippy lints shaped the API, correctly.** `copy_iterator` on `ExtensionIter` — a `Copy`
iterator duplicates rather than moves, so an accidental copy silently restarts iteration; removing
`Copy` is right. And `iter_without_into_iter` prompted the `IntoIterator for &Extensions` impl.

**No consumer migration was needed.** `mh-service` and `mc-service` both declare the
`media-protocol` path dependency but contain zero `use media_protocol::` sites, independently
verified by @dry-reviewer and @operations at plan stage and confirmed at implementation. Retiring v1
broke no caller.

---

## Lessons Learned

**A constant with publisher-controlled preconditions is an attack primitive, not an optimisation.**
The plan exported `FIXED_AUDIO_RELAY_REGION_OFFSET = 62` for MH's audio fast path. @media-handler
and @security independently found that *both* of its preconditions — key-bearing set, zero
extensions — are chosen by the publisher, so one participant sending a single extension byte would
have MH write six bytes over another participant's **signed** publisher region, failing verification
at every receiver: a one-line client change that silently kills any other participant's audio at the
relay. The first fix was a checked `relay_region_offset()` function; the better fix, after
@media-handler confirmed no consumer needs a raw offset, was to ship **nothing**. A checked function
is still a name promising an offset, which the next person with a hot-path idea will find.
Generalised: *when an affordance's safety depends on conditions an attacker controls, the fix is
usually deletion rather than validation.*

**Two parsers over one byte range agree on exactly the inputs that do not matter.** The plan had
`rewrite_relay_region` doing a lighter prefix parse than decode. Two parsers agree on every
well-formed frame and diverge on precisely the malformed ones an attacker constructs — and here the
divergence is invisible to the party doing the writing, since MH's own counters all report success
and the frame is malformed only from the receiver's point of view. One shared parser makes the
differential unrepresentable rather than tested-against.

**"Compiled" is not "executed", and the gap is easy to ship.** The plan put the corpus generator in
`examples/`, which `clippy --all-targets` compiles and `cargo test` never runs — the per-seed
assertions would have been lint-clean and never executed a single decode, while the plan described
them as coverage. @test caught it. The general form: when adding a check, name the command that runs
it and confirm the pipeline runs *that* command.

**A test can fire and not apply.** The prefix-invariant test built on this story's production frame
(key-bearing audio, zero extensions) never places a prefix boundary inside a TLV region — so it
would have passed vacuously over the exact ambiguity it exists to catch. @observability caught it;
the fix was to iterate the full 2×2 shape matrix. This is ADR-0036's own does-it-fire/does-it-apply
distinction turned on a new test rather than on an existing control.

**A summary with no edge back to its source rots, and it rots silently.** This document amended by
addition through two revisions, leaving superseded blocks reading as live. That directly caused a
reviewer to transcribe a stale "seven tokens" line, costing three reviewers a round — and would have
caused a Gate-2 reviewer to flag correct code as defective against a stale token mapping. The
structural fix was to delete the numbered evaluation-order list rather than renumber it, make the
reason × entry-point table the single statement, and reference by **token** rather than by ordinal.
The narrower lesson, which recurred four times in this loop: **a numeral in prose is a second source
of truth for a fact some enum or registry already owns.** `ALL_REJECT_REASONS` is `&[RejectReason]`
and not `&[&str]` for the same reason.

**Declare-and-enforce removes a ledger entry rather than sizing one.** @security's first instinct
was to record the salience byte's residual capacity as an accepted covert channel. The correction is
sharper: the channel is the surplus of *representable* over *declared*, and enforcing a declared set
drives that to zero either way — so the clause removes the need for an entry. The accepted-channel
paragraph therefore has exactly one entry, the relay region, with its **direction** stated
(MH → receiver) and §11's adversary set **cited rather than restated**, after an overstated
"on-path observer" claim was caught and corrected against the ADR.

---

## Pre-Work

None.

---

## Accepted Deferrals

- `docs/TODO.md` §Media Path Obligations — both fuzz workspaces excluded from pipeline; `cargo-fuzz` absent
- `docs/TODO.md` §Cross-Service Duplication (DRY) — MC/MH duplicate `MAX_MESSAGE_SIZE` + read-prefix routine (extraction opportunity, not a deferral — see §Gate 3 ruling)
