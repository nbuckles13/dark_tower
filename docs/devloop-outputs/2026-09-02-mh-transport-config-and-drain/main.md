# Devloop Output: MH Explicit QUIC Transport Config, Required Env Vars, Derived Drain, Real Transport Trait Impl

**Date**: 2026-09-02
**Task**: Replace mh-service's default QUIC transport configuration with explicit, startup-validated ADR-0036 §1 transport parameters; make the new env vars required; derive the shutdown drain window from `MH_TERMINATION_GRACE_SECONDS`; provide the real implementation of the transport trait seam.
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full, HEADLESS RUN (run-story task #12)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~2h20m (setup 20:00Z → commit 22:20Z), 3 Gate-2 attempts

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7c740e71d302388df8fa65f9e23e709ab6c39f19` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Story | `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` task 12 |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `a6a504b834ad6acd5` |
| Implementing Specialist | `media-handler` |
| Iteration | `1` |
| Security | `ac54ca477a1ebc5af` |
| Test | `a060d6fd6f8490f5c` |
| Observability | `a229fa99597a20f76` |
| Code Quality | `a909d41b37bc1eb1b` |
| DRY | `a8c83243a767362d2` |
| Operations | `abbcac313c4262f39` |
| Semantic Guard | `a22efdc41cb75b45b` |

---

## Task Overview

### Objective

Per ADR-0036 §1, `crates/mh-service` must stop running on quinn's default QUIC transport
configuration (1 MiB datagram buffer, no keepalive) and instead build an explicit
`quinn::TransportConfig` from required, startup-validated environment configuration:

- `MH_MAX_CONCURRENT_UNI_STREAMS` — QUIC transport bound (distinct from any capacity figure)
- `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` — datagram send buffer in FRAMES OF AUDIO, converted once at
  config load to bytes via `NOMINAL_AUDIO_FRAME_BYTES`
- explicit non-`None` datagram receive buffer so `max_datagram_frame_size` stays negotiated
- `MH_KEEPALIVE_INTERVAL_MS` — keeps a muted participant's NAT binding alive (§1, §5)
- `MH_MAX_CONNECTIONS` becomes required (drop `unwrap_or(DEFAULT_MAX_CONNECTIONS)`), relabelled as a
  resource-exhaustion guard at accept, not capacity
- `MH_TERMINATION_GRACE_SECONDS` — drives the shutdown drain window
  `min(SETTLE_TARGET, grace − MARGIN)`, replacing the hardcoded 2s sleep in `main.rs`

Plus: startup validation (egress queue bound must trip before the transport datagram-buffer ceiling;
termination grace must exceed the shutdown margin), one structured startup line logging every
effective transport value and the derived drain window, and the REAL implementation of the
transport trait seam (from devloop `2026-09-01-mh-transport-seam`) wrapping wtransport
per-connection I/O in the webtransport module.

### Out of Scope (explicit)

- `MH_MAX_STREAMS` and its consumers — the egress-budget chain is story 2. Leave untouched.
- Forward loops. This is foundation plumbing; the forward-path task runs against the trait.
- A second transport abstraction. Consume the existing seam.

### Scope
- **Service(s)**: mh-service
- **Schema**: No
- **Cross-cutting**: Manifests already landed (task 11, commit `612bc379`); this is the code half.

### Debate Decision
NOT NEEDED — ADR-0036 §1 already decided the transport-parameter policy; this is implementation.

---

## Cross-Boundary Classification

Every planned file change. `crates/media-protocol/**` (GSA) and `crates/common/src/webtransport/**`
(GSA) are **NOT touched** — every size component this task needs is already `pub` in
`media-protocol::frame`, so nothing needs to be added there. `crates/mh-service/src/transport/mod.rs`
(the seam) is **NOT touched** — this task supplies an implementation of it, not a change to it.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/webtransport/server.rs` | Mine | — |
| `crates/mh-service/src/webtransport/mod.rs` | Mine | — |
| `crates/mh-service/src/webtransport/media_transport.rs` (**new**) | Mine | — |
| `crates/mh-service/src/lib.rs` | Mine | — (crate docstring: the seam is no longer unimplemented) |
| `crates/mh-service/tests/common/accept_loop_rig.rs` | Mine | — (`WebTransportServer::new` signature) |
| `crates/mh-service/tests/gc_integration.rs` | Mine | — (`Config` struct literal gains fields) |
| `crates/mh-service/src/transport/mod.rs` | **ADDED AT IMPLEMENTATION — Minor-judgment** | **@test** (seam owner). **Doc-only, one clause.** The seam's `DatagramSendError::WouldBlock` docs assert as clause 4 that `wtransport`'s `quic_connection()` "does not compile here today" because the `quinn` feature is absent from this workspace's dependency. **This commit makes that false**: `with_custom_transport` — the only way to hand `wtransport` an explicit `quinn::TransportConfig`, which ADR-0036 §1 requires — is gated behind that same feature, so `crates/mh-service/Cargo.toml` now enables it. The five-clause conclusion is unchanged (clause 5 was always the stronger leg); only clause 4's reason is. Corrected in place with a dated note rather than deleted, because a reader who checked that clause, found it false, and discarded the argument would conclude `WouldBlock` is reachable in production — which it is not. **No contract, trait, or signature is touched.** |
| `crates/mh-service/Cargo.toml` | Mine | — (enables `wtransport/quinn`; see the row above) |
| `crates/mh-service/tests/transport_real_impl.rs` (**new**) | Mine | — (@test's required items 1/2/3) |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-09-02-mh-transport-config-and-drain/main.md` | Mine | — |
| `docs/runbooks/mh-deployment.md` | **Minor-judgment** | **@operations** — their pre-plan item 5. Two edits: (a) `kubectl rollout undo deployment/mh-service` at `:278` targets a Deployment that does not exist (workloads are `mh-0`/`mh-1`) — it fails `NotFound` mid-rollback today; (b) a rollback-ordering constraint that **this commit creates**: manifests must not roll back alone, because reverting the ConfigMap under the new image removes five now-required vars and CrashLoops both pods. Requesting explicit ACK. |

**Not touched, deliberately** (a reviewer finding if they appear in the diff): `MH_MAX_STREAMS`,
`Config::max_streams`, `DEFAULT_MAX_STREAMS`, `grpc/gc_client.rs` (story 2); any forward loop; any
second transport abstraction; `crates/media-protocol/**`; `infra/services/mh-service/mh-{0,1}-deployment.yaml`
and `kustomization.yaml` (@operations verified them; do not touch); and
`infra/services/mh-service/configmap.yaml` — **@main ruled it out of the diff at Gate 1** (its stated
"~263 KB" per-connection ceiling survives my ~264 KB under the tilde, and a 7% move is not worth an
@infrastructure co-sign). The two arithmetic defects I found in that comment are recorded in the Rust
docstring beside the constants, which explicitly states that it **supersedes** `configmap.yaml:174`.

**Deferred, with @operations' minimum bar met in-diff**: a numbered `mh-incident-response.md` scenario
for the seven new boot-refusal paths. Story task **21** is the operations runbook task and
`configmap.yaml:142` already reserves Scenario 15 for the keepalive symptom — claiming a number here
races it. Instead, **every one of the seven error messages names the offending variable AND its
remediation location** (which ConfigMap key, or which pod-spec field), so the refusal line is itself
the runbook. @operations to accept or reject that trade.

> **SUPERSEDED at Gate 1 — see amendment A4.** @operations rejected the deferral by dismantling its
> *premise*: a CrashLoop after `kubectl apply` is a **deployment** symptom, so its home is
> `mh-deployment.md` — already in this table, no numbered anchors, no race with task 21. The
> `mh-incident-response.md` scenario is genuinely not owed by this task and that file stays untouched;
> what was deferred is now landed, in a different file, at no collision cost.

---

## Planning

**Gate 1 confirmations**

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

**Navigation injection note**: the Lead directed each teammate to read its own
`docs/specialist-knowledge/{name}/INDEX.md` as the first action of its turn rather than
pasting 83 KB of INDEX text across eight spawn prompts. Same navigation map, same
first-action guarantee, materially cheaper.

**Gate 1 closed** — all seven confirmed; `validate-cross-boundary-classification.sh` returned
`STATUS=OK REASON=cross-boundary-classification-clean-1-files`. "Plan approved" issued.

### Lead rulings at Gate 1

| # | Question | Ruling |
|---|----------|--------|
| 1 | Three scope additions (receive-window trio; `max_idle_timeout` + keepalive:idle ratio validation; `EGRESS_QUEUE_FRAMES`) | **All approved.** The trio is @security-blocking and `docs/TODO.md:216` names this task as owner; the task text already requires an explicit non-None datagram receive buffer. `EGRESS_QUEUE_FRAMES` is required to make the task's own first validation expressible — constrained to a compile-time constant and a comparison, no queue, no `MH_MAX_STREAMS` drift. |
| 2 | `docs/runbooks/mh-deployment.md` | **Approved, Minor-judgment, owner @operations** (ACK granted Gate 1, re-confirms at Gate 3). Written by @implementer, not by the owner. |
| 3 | `infra/services/mh-service/configmap.yaml` | **Not edited.** Its "~263 KB" survives the ~264/~281 KB delta. Corrected arithmetic goes in the Rust docstring, which must state it **supersedes** `configmap.yaml:174` and name both defects separately (double-counted `stream_receive_window`; omitted 16 KiB `crypto_buffer_size`), plus a `docs/TODO.md` entry naming @infrastructure. |
| 4 | `docs/TODO.md:683` (four ADR-0036 §8 policy-bound ConfigMap keys) | **Re-filed to @infrastructure.** Verified NOT stale — all four keys have 0 hits under `infra/`, 1 each under `crates/`. Re-filed on an ADR-0024 §6.3 routing constraint, not scope preference: four @infrastructure-owned files, owner not on this panel, so absorbing would mean landing owner-less manifest edits. Entry must carry its detection-gap paragraph intact and gain "allocation owed before story close". |
| 5 | @test's real-impl contract tests 1/2/3 | **Required, not optional.** Nothing in the tree sends a datagram or opens a uni-stream over the real transport today (`wt_client` is bi-stream only), so without them the seam's real half ships unexercised while task 16 trusts the shim. Full conformance harness explicitly NOT built here. No end-to-end datagram *delivery* assertion (ADR-0028 zero-retry flake). |
| 6 | ADR-0036 §11 amendment | **Not needed** — drain resolves to `min(2, 35−5) = 2 s`, so "sleeps two seconds" stays true; only provenance changes. `SETTLE_TARGET` must bind by construction and test, not by coincidence of today's deployed grace. |

### A1 — Opus sizing: ruled 48 kbps, verified, REVERSED to 32 kbps

Recorded as a reversal rather than as a destination, because the wrong turn is the useful part.

The task text defines the constant at **32 kbps**/20 ms but also says "sized for the client's
configured VBR ceiling". @dry-reviewer's F2 argued the ceiling (48 kbps, story task 19's range),
and the Lead ruled for it — reasoning that a buffer denominated in *frames* is an operator-facing
latency budget, so 32 frames must be a floor across the range rather than a point estimate at its
bottom.

**That was wrong, and all three of us made the same error.** `datagram_send_buffer_size` is a
**latency ceiling**, not a capacity guarantee: `adr-0036-media-flow.md:64` objects to quinn's 1 MiB
default precisely because it is "≈ 93 seconds of queued audio … before the oldest is silently
discarded. A realtime path must prefer loss to unbounded latency." Held frames =
`buffer_bytes ÷ actual_frame_bytes`, so queued latency is maximised by the *smallest* frame, and the
bound holds across 32–48 kbps only when sized at the **lowest** bitrate:

| Sizing | at 32 kbps traffic | at 48 kbps traffic |
|---|---|---|
| `NOMINAL = 236` (32 kbps floor) | 32 frames / 640 ms ✓ | 27 frames / 540 ms ✓ |
| `NOMINAL = 276` (48 kbps ceiling) | **37 frames / 740 ms ✗** | 32 frames / 640 ms |

The Lead's ruling named "dropping sooner than the operator asked for" as the harm — which is the
behaviour §1 chooses in terms. @dry-reviewer retracted F2's value half, @implementer re-derived it
independently, and the Lead verified `adr-0036-media-flow.md:64` directly before reversing.

**Landed: 32 kbps, `NOMINAL_OPUS_PAYLOAD_BYTES = 80`, `NOMINAL_AUDIO_FRAME_BYTES = 236`,
32-frame buffer = 7,552 B.** The task text's arithmetic and principle clauses never conflicted —
covering a range for a *latency ceiling* means sizing at its floor. No ADR divergence note is owed:
§1's "~225 B", `:311`'s 80 B and 236 are one derivation chain.

**What survives from F2 regardless** (the instrument half, unaffected by the value retraction): no
`ANCHOR (DRY):` naming the SDK encoder config. MH and the SDK stand in an *inequality*, so an anchor
would claim an uncheckable lock — and one asserting a shared 32 would now look accidentally correct
while encoding the wrong relation. Replaced by the stated relation **with its direction written out**
(MH sizes against the SDK's *floor*; a rise in the SDK's ceiling does not affect this constant, a drop
in its floor does), both endpoint frame counts on the constant, and the same note carried into the
task-19 follow-up entry.

### Plan amendments during Gate 1

The plan above is preserved verbatim per the devloop contract. Everything below changed during Gate 1
review and supersedes it where they conflict.

**A1 — `NOMINAL_AUDIO_FRAME_BYTES` stays at 32 kbps / 80 B / 236 B, as originally planned. A ruling
to move it to the 48 kbps ceiling was made, verified, and REVERSED inside Gate 1.** The wrong turn is
recorded rather than erased, because the sign error that produced it is the single most likely mistake
a future reader will repeat.

*What was ruled, and why it was wrong.* @dry-reviewer's F2 argued that MH and the SDK stand in an
inequality (MH must COVER the SDK's 32-48 kbps range), so the constant should be sized at the
**ceiling** — 120 B payload, 276 B frame. @main ruled for it, reasoning that a budget labelled 32 frames
which holds 27 at the top of the range is broken. **I carried the argument to @main and did not check
it either.** @dry-reviewer then retracted the value half themselves; I verified the arithmetic
independently before accepting the retraction:

```
buffer = 32 * NOMINAL;   held = buffer / actual_frame_bytes
NOMINAL=236 (size@32):  traffic@32 -> 32 frames = 640 ms OK    traffic@48 -> 27 frames = 540 ms OK
NOMINAL=276 (size@48):  traffic@32 -> 37 frames = 740 ms BAD   traffic@48 -> 32 frames = 640 ms
```

*The sign error.* `datagram_send_buffer_size` is a **latency ceiling**, not a capacity guarantee.
ADR-0036 §1's row is explicit: quinn's 1 MiB default is "~93 seconds of queued audio ... before the
oldest is silently discarded. **A realtime path must prefer loss to unbounded latency.**" The defect it
exists to fix is the buffer holding *too much*. So the bounded quantity is **maximum frames held**,
which is maximised by the **smallest** real frame, not the largest. Sizing at the ceiling *loosens* the
latency bound by ~17% on exactly the traffic §1's own worked example describes (`:311`, "an 80-byte
Opus frame (32 kbps, 20 ms)"). @main's ruling said sizing at 32 "breaks silently in the direction of
dropping sooner than the operator asked for" — the first clause is right and **the second has the sign
inverted**: dropping sooner is the direction §1 chooses. Three people made the same inversion in one
afternoon, which is the strongest available argument for stating the direction loudly in code.

*Consequences.* The task text's arithmetic clause and its principle clause **never actually disagreed** —
"sized for the client's configured VBR ceiling" means *covering the range*, and covering a range for a
latency ceiling means sizing at its **bottom**. So there was no rule-versus-example conflict to resolve.
And the ADR divergence note is **moot**: 236 puts code and ADR on one arithmetic chain (§1's "~225 B",
`:311`'s 80 B, and 236 are one derivation with a rounding in the prose, not three figures).

*What survives from F2, unchanged, and it is the half that was genuinely @dry-reviewer's to rule on:*
**no `ANCHOR (DRY):` naming the SDK encoder config as co-holder.** MH and the SDK stand in an inequality
either way; only its *direction* changed. An anchor would claim a lock that does not exist and cannot be
checked. The retraction **strengthens** this: an anchor asserting a shared `32` would now look
accidentally correct while still encoding the wrong relation, which is the most durable way for a false
SSoT to survive review. Instead, the direction is stated explicitly at the constant:

> MH sizes this against the **lowest** bitrate the SDK may emit, not the highest. The buffer is a
> latency ceiling (ADR-0036 §1: prefer loss to unbounded latency), so the binding case is the *smallest*
> frame — a smaller frame means more frames queued in the same bytes, i.e. more latency. If the SDK's
> floor ever drops below 32 kbps this constant must drop with it; a rise in the SDK's ceiling does not
> affect it.

The same *why the ceiling is the wrong end* note goes into the story-task-19 `docs/TODO.md` follow-up,
because whoever lands the SDK encoder config is the single most likely person to "fix" this constant
upward, and they will arrive holding the word "ceiling".

**A2 — the header-region enumeration is pinned at compile time.** F1, @dry-reviewer, ruled accept.
`const _: () = assert!(NOMINAL_HEADER_BYTES == MAX_HEADER_BYTES - MAX_EXT_BYTES, ...)`. The duplicated
knowledge is **the set of header regions**, not the widths (those are imported and track correctly): a
sixth region added to frame v2 updates `MAX_HEADER_BYTES` and silently misses a hand-listed sum,
under-sizing the buffer *and* feeding a stale premise into the egress-queue validation. Idiom already
live at `routing/mod.rs:72,78,84`; both operands `pub`; no GSA edit.

**A3 — `DATAGRAM_RECEIVE_BUFFER_BYTES = 2048` is justified by an ASSERTED FLOOR, not a ratio.**
@security, two passes. The plan's "~8.7x a nominal audio frame" is a *ratio* argument and the binding
constraint is a *floor*: the largest legitimate audio datagram is ~1.55 KB (max-bitrate Opus 1275 B +
`MAX_HEADER_BYTES` + `SIGNATURE_SIZE` + `SFRAME_OBJECT_OVERHEAD_BYTES`), not 276 B. A reader following
the ratio concludes "1024 is still 4x nominal, therefore safe" — and 1024 is *under* the floor, where
the failure is a client `send_datagram` returning `TooLarge` while **MH observes nothing at all**: one
publisher silently drops off with no MH-side signal. So: `const _: () = assert!` on the floor, derived
from the same `media-protocol` constants, so a future shrink fails the build.
Second pass: the floor gains a named `DATAGRAM_ENCAPSULATION_OVERHEAD_BYTES = 8`, because
`max_datagram_frame_size` bounds the **QUIC DATAGRAM frame**, not our payload — the QUIC frame header
plus the HTTP/3 WebTransport session-id varint wtransport prepends (`wtransport datagram.rs:34-48`).
An off-by-encapsulation floor would *pass* for a value that fails on the wire: the same defect class as
the ratio, one order smaller.
Note the **opposite max/nominal conventions**: the floor uses `MAX_HEADER_BYTES` (maximum case, includes
`MAX_EXT_BYTES`), `NOMINAL_AUDIO_FRAME_BYTES` uses nominal-case components. Two constants from
overlapping provenance under opposite conventions is a swap hazard sharper than the `MAX_PAYLOAD_BYTES`
one, so the *reason* is stated at both sites: the floor must clear the largest frame the codec can
legitimately emit (a permissive error breaks a real publisher), the nominal must reflect the typical
frame (a conservative error wastes buffer). Opposite directions of harm, hence opposite conventions.

**A4 — the runbook deferral is WITHDRAWN; `mh-deployment.md` widens from 2 edits to 4.** @operations
rejected the deferral by **dismantling its premise rather than its conclusion**: my task-21 collision
argument assumed the home was `mh-incident-response.md`. It is not — a CrashLoop in the ninety seconds
after `kubectl apply` is a *deployment* symptom, and `mh-deployment.md` is already in the diff, has no
numbered anchors, and cannot race task 21. The collision cost I priced the deferral against was zero
the whole time.
(a) `:278`'s `kubectl rollout undo deployment/mh-service` -> the two real per-instance workloads (both,
    since rolling one leaves a split-version pair); keep the existing severed-sessions note.
(b) Rollback ordering + the `apply -f` sentinel trap. Forward: manifests then image. Backward: image
    may roll back alone; **manifests must not** — reverting the ConfigMap under the new image strips
    five now-required vars and CrashLoops both pods, a **join-path** outage, so a page not a ticket.
(c) A required-environment-key table — @observability found `mh-deployment.md` has **no configuration
    section at all**, unlike GC/AC/MC. Five required rows plus a sixth non-required `MH_MAX_STREAMS`
    row for contrast, with a description column naming **the enforcing layer**, which is what keeps
    the three `MAX`-named keys distinguishable and keeps the table in step with the log field names.
(d) A "both pods CrashLoop immediately after apply" block: `kubectl logs --previous`, three causes in
    likelihood order, page-not-ticket framing — plus the three-way partition @observability verified
    against `main.rs:62 / 70-81 / 93-100`. **The absence of the startup line is itself a diagnostic**,
    and it has a collision that will bite someone: `Config::from_env()` and `init_otel`'s eager
    collector probe *both* run before subscriber init, so **a rejected config and an unreachable OTLP
    collector produce an identical signature** (zero structured lines + a `Debug` dump). The only
    discriminator is the error *type*, so the triage step says read the dumped error's type, not its
    message. Pre-existing and explicitly not filed as a finding against this PR; it lands here only
    because five newly-required keys make that case materially more likely.
Wording for (b) goes to @operations for word-level ACK before Gate 2; the rest at intent.

**A5 — real-impl seam tests are REQUIRED, not optional.** @test's decisive finding: **nothing in the
tree sends a datagram or opens a uni-stream over the real transport today** — `wt_client` is bi-stream
only, and every finish/datagram assertion runs against `LossDelayTransport`. So the reachability suite
proves the *double* honours the finish-then-write contract and nothing proves wtransport does — the
exact silent-divergence window `transport/mod.rs:320-324` elevates to a contract, with task 16 about to
trust the shim there. This is the acceptance test for "provide the REAL implementation", which is task
text. Three parts, all landing now: (1) `finish()`-then-`write_all()` -> `StreamClosed` against the real
`WtSendStream`; (2) `recv_datagram()` -> `ConnectionClosed` on close against the real
`WtMediaTransport`; (3) #1's assertion body written **generic over `MediaSendStream`** and applied to
both the real stream and the shim's — parity, not just coverage.
**Pulled back on @test's instruction: NO datagram-delivery assertion on the real path.** QUIC datagrams
are unreliable; "sent N, received N" over loopback is a latent ADR-0028 zero-retry flake even at
999/1000 green. A happy-path round-trip, if wanted, goes over the reliable uni-stream. The extracted
free-function error mappings cover the datagram variant logic deterministically.
Plumbing cost on the record: `wt_client` has no datagram/uni helpers and nothing constructs
`WtMediaTransport` server-side, so (1)/(2) need a test path yielding an accepted server-side
`wtransport::Connection` plus client read helpers. Bounded, not free.
The **full** conformance harness (impl registry, growing parity set, `mh-test-utils` surface) is a
legitimate scope addition and is spun out to @test.

**A6 — `Error Context Preservation` engaged; I had not pre-flagged it.** @semantic-guard. "Variants
carry nothing but integers" is fine for the credential-leak lens but is the wrong shape here: a
`MH_KEEPALIVE_INTERVAL_MS=abc` failure must carry the offending value and the underlying parse error,
not discard `e` into a bare fielded variant.

**A7 — `config.rs` stays whole; `parse_quic_transport` extracted for the parses only.**
@code-reviewer, answering both plan questions, with the guard mechanics verified:
`dt-guard/src/env_config.rs:590` opens `crates/<svc>/src/config.rs` **by exact path** and greps its body
with the `:186` regex. So any required-var parse moved to a sibling `transport.rs` goes invisible while
`STATUS` stays `OK`. Constants stay in `config.rs` too — splitting only them would fragment the
frames->bytes derivation from its consumer for marginal line savings. The extracted
`parse_quic_transport` stays in `config.rs` and **inlines the literal `MissingEnvVar("MH_...")` sites**.
The repetition comment names the guard file and line so a future cleanup cannot plead ignorance.

**A8 — all 17 startup-line fields stay.** @observability: field count is not a readability cost in JSON
structured logging (nobody reads the line left-to-right; they grep one field in Loki), the cardinality
cost is zero (emitted once per process start), and **omitting a field returns its value to being an
invisible library default** — the condition the line exists to end. The two I offered to cut are the two
they most want kept: `nominal_audio_frame_bytes` is the conversion factor that makes the units chain
self-checking at the point of use, and `transport_max_datagram_frame_size_bytes` is a silent-drop knob
an operator needs without a redeploy. Also: group the constants in `config.rs` so env-driven vs
compile-time is obvious at a glance and say so in the docstring — the line merges a *third* provenance
state (not settable at all) into the two the original docstring contemplates.

**A9 — `configmap.yaml` NOT edited; two DISTINCT arithmetic defects recorded, not netted.**
@main, @security and @operations independently concurred: the load-bearing figure in
`configmap.yaml:167-176` is not the per-connection ceiling but the conclusion
(`500 x per-conn ~= 13% of 1 Gi`), which survives 264 KB and 281 KB alike, so a tilde-qualified 7% move
is not worth an @infrastructure co-sign. The corrected arithmetic goes in the Rust docstring beside the
constants, plus a `docs/TODO.md` entry naming @infrastructure and citing `configmap.yaml:174`.
**Two defects, opposite directions, stated separately rather than netted** (@operations), or story 2
inherits a number that looks right by coincidence: `stream_receive_window` was **double-counted** (it is
bounded by the connection window, not additive) and `crypto_buffer_size` (16 KiB, quinn default,
undeclared) was **omitted**.

**A10 — `docs/TODO.md:683` verified NOT stale; escalated to @main for routing.** The four ADR-0036 §8
policy-bound keys have **zero** hits across all of `infra/`. Task 11 landed the *code* half (parsing,
ceilings, startup logging) — which is exactly the "two of three" that entry already credits it with,
leaving "no manifest", which is the entry. Recommended re-file with owner @infrastructure (four
infra-owned files; optional-with-default with hard ceilings and already logged, so the failure mode is
"a default nobody chose", not a CrashLoop; orthogonal to every mechanism here) — with the
counter-argument stated rather than buried: the entry's own title is *"NOTHING FAILS IF IT FORGETS"*,
so a re-file that loses its owner is how it becomes permanent. Awaiting @main.

**A11 — `docs/TODO.md:216` and `:45` updated in place.** :216 records the values landed and whether
"~263 KB" held; :45 marks the three fail-closed preconditions met, pointing at the specific tests.
**:216 is discharged only in part** — @security explicitly did not demand the downward-API
`resourceFieldRef: limits.memory` startup validation, calling it task-sized. That half stays **visibly
open with its owner intact**; a partial discharge that deletes the undone part is worse than not
touching the entry at all.

**Also unchanged after review, recorded so the confirmations are not mistaken for silence:** no
ADR-0036 amendment (drain resolves to `min(2, 35-5) = 2 s`, so §11's "sleeps two seconds" stays true;
a forcing-function comment on `SHUTDOWN_SETTLE_TARGET_SECONDS` obliges amending §11 in the same change
if the constant ever rises). The validation bound stays `grace > MARGIN`, **not** the stricter
`grace >= MARGIN + SETTLE` — @operations checked the edge: the invariant is `drain + MARGIN <= grace`,
which holds for any `grace > MARGIN`, and the stricter form would wrongly refuse `grace = 6` (`1 + 5 = 6`,
safe). The `grace - MARGIN` arm binds *only* at `grace = 6`, which is why `drain_window_source` earns
its place and why that observation goes in the enum's docstring — a two-variant enum with one
near-unreachable variant reads as dead weight to the next person.

---

## Pre-Work

None. Working tree clean at `7c740e71`.

---

## Implementation Summary

MH no longer runs on quinn's default QUIC transport configuration. It declares its transport
parameters explicitly, validates them at startup, derives its shutdown drain from the pod's own
termination grace, and ships the real implementation of the ADR-0036 §10 transport seam.

**1. Five required environment variables, no silent defaults.**
`MH_MAX_CONCURRENT_UNI_STREAMS`, `MH_DATAGRAM_BUFFER_AUDIO_FRAMES`, `MH_KEEPALIVE_INTERVAL_MS`,
`MH_TERMINATION_GRACE_SECONDS`, and — newly required — `MH_MAX_CONNECTIONS`, whose
`unwrap_or(DEFAULT_MAX_CONNECTIONS)` and the `DEFAULT_MAX_CONNECTIONS` constant itself are both
deleted. Every value parses strictly: a malformed value is a loud refusal, never a silent revert.
The presence checks stay written out per-variable rather than behind a helper, because
`dt-guard env-config` discovers required variables by scanning `config.rs` for that literal
construction — a helper would blind the guard to all five **while it kept printing `STATUS=OK`**.

**2. Explicit `quinn::TransportConfig`** via `with_custom_transport`, built by
`webtransport/server.rs::build_transport_config` (extracted so every field is assertable without
binding a socket). Env-driven: uni-stream bound, datagram **send** buffer, keepalive. Compile-time:
`max_idle_timeout`, both receive windows, and a non-`None` datagram **receive** buffer that keeps
`max_datagram_frame_size` negotiated. The accept loop is byte-for-byte unchanged — capacity check,
`fetch_add`, spawn, decrement on every exit path — and `MH_MAX_CONNECTIONS` is relabelled an
accept-time resource-exhaustion guard, never a capacity figure.

**3. Receive-side flow control, which is what makes `MH_MAX_CONNECTIONS=500` mean anything.**
Undeclared, quinn leaves `receive_window` at `VarInt::MAX`; with no `accept_uni` loop the windows
fill and stay filled, so the ~206 MB per-connection adversarial ceiling against a 1 Gi limit is the
*ordinary* case, not a tail — roughly five hostile connections, not 500. And since OOMKill is
`SIGKILL`, that voided the very drain window this task derives. Declaring the three constants drops
the ceiling to **~264 KB** (~281 KB counting quinn's inherited 16 KiB crypto buffer), a ~780x
reduction that makes 500 genuinely memory-derived at ~13% of the limit.

**4. `NOMINAL_AUDIO_FRAME_BYTES = 236`, derived rather than typed.** One new number — the 32 kbps /
20 ms Opus payload, itself const arithmetic — plus six size components imported from
`media-protocol`. A compile-time pin asserts the local region enumeration equals
`MAX_HEADER_BYTES − MAX_EXT_BYTES`, so a frame-v2 layout change breaks the build rather than
silently resizing a live buffer. Frames convert to bytes **once**, at config load.

**5. Two startup validations became three, each with its own fielded `ConfigError`.** The app egress
queue must trip before the transport ceiling; the termination grace must exceed the shutdown margin;
and the keepalive must leave room for a lost packet before the idle timeout. Fielded rather than
`InvalidValue(String)` because configuration loads *before* the tracing subscriber exists — in a
CrashLoop the variant's `Display` is the entire operator surface, so each names the invariant, the
offending value, the bound, **and** the remediation location.

**6. The drain is derived, and `main.rs` holds no seconds literal for it.**
`min(SETTLE_TARGET, grace − MARGIN)` computed once in `config.rs`, beside the validation that guards
it, exposed as `drain_window` plus a two-variant `DrainWindowSource`. At the deployed grace of 35 it
resolves to **2 s** — identical to the hardcoded sleep it replaces, which is why ADR-0036 §11's
"sleeps two seconds" stays true and needs no amendment. Only the provenance changed.

**7. One structured startup line, extended not duplicated.** 17 new fields on the existing
`"Configuration loaded successfully"` event, layer-qualified (`transport_*`) so three unrelated
`max_`-named quantities stop looking related, unit-suffixed in the unit actually logged, carrying
**both** links of the frames→bytes conversion plus the conversion factor itself so the units chain is
checkable from the line rather than from source.

**8. The real transport-seam implementation**, `webtransport/media_transport.rs` — two newtypes over
the existing seam, no second abstraction, no forward loop, no `tracing`/`metrics` macro reachable.
Constructed by nobody; task 16 wires it, after the JWT gate.

### The finding: a contract that did not hold, caught on the first run

`tests/transport_real_impl.rs` failed immediately with `ConnectionClosed` where the seam's contract
requires `StreamClosed`. The cause is structural rather than a mapping mistake:

```
wtransport-0.7.2/src/driver/streams/mod.rs:564-568
    quinn::WriteError::ConnectionLost(_) | quinn::WriteError::ClosedStream
        => StreamWriteError::NotConnected
```

The seam's contract cites quinn returning the **stream-scoped** `ClosedStream` after `finish`. quinn
does — but `wtransport` folds it together with connection loss into one value whose own docstring
reads *"Connection has been dropped"*. By the time an error reaches MH the two cases are
indistinguishable, and **no mapping choice recovers information already discarded**.

Fixed by recovering the distinction rather than weakening the contract or the test: `WtSendStream`
carries a `finished` flag, latched only on a successful `finish` and checked before touching the
transport. MH is entitled to it because MH is the party that caused the transition. The tempting
alternative — mapping `NotConnected → StreamClosed` — was rejected as inverting the damage: a
genuinely lost connection would report as a dead stream and the forward path would hold it open
indefinitely, and the stream case is recoverable where the connection case is not.

**Why this matters more than the fix.** The distinction is load-bearing for task 16 — a dead stream
is a per-subscriber problem, a dead connection a per-participant one — and it is available *only*
where MH holds local state. A peer-initiated reset arriving as `NotConnected` is unrecoverable.
Filed in `docs/TODO.md` as a design constraint on task 16, not as a doc comment.

Before this suite existed, **nothing in the tree sent a datagram or opened a unidirectional stream
over the real transport**: `wt_client` is bidi-only and every `finish` assertion ran against the
double. So the reachability suite proved the *double* honoured the contract while nothing proved
`wtransport` did — and task 16's tests would have trusted the double at exactly that point. The
suite was made a condition of plan approval by @test and @main; it paid off on run one.

### Testing

31 new tests. Unit: five per-variable required-var refusals asserting the exact variable name; a
malformed-value sweep asserting the offending text survives into the message; the component-sum
derivation of `NOMINAL_AUDIO_FRAME_BYTES` (never a literal); frames→bytes; **a held-frames assertion
whose failure mode reads "the constant got sized at the bitrate ceiling"**, which is the mistake this
gate actually made; boundary pairs on all three validations; the drain at both arms plus a
range-wide invariant check; and transport-error mapping. Integration: the four real-impl contract
tests, one of which shares its assertion body with the double so parity is mechanically checked.

`cargo test -p mh-service`: 17 binaries, all green. `cargo clippy -p mh-service --all-targets`: clean.
`cargo fmt --all`: clean. `validate-env-config.sh`, `validate-kustomize.sh`,
`validate-cross-boundary-{classification,scope}.sh`, `validate-knowledge-index.sh`,
`validate-todo-tracking.sh`, `validate-doc-citations-*.sh`, `no-{secrets,pii}-in-logs.sh`,
`test-registration.sh`: all `STATUS=OK`.

### Two guard catches worth recording, because both would have red Gate 2

- **`dt-guard env-config` scans comments.** An illustrative `MissingEnvVar` construction written in
  *prose* with an upper-case string literal is indistinguishable from a real one, and was reported as
  two required variables no manifest declares. Fixed by describing the construction without
  instantiating it — and the docstring that explains this now obeys its own rule. The guard is
  behaving correctly; it is a reminder that a detector keyed on a literal cannot know it is reading
  documentation.
- **The knowledge INDEX was already at exactly its 75-line cap**, so five new bullets had to fold
  into five existing ones rather than be appended.

---

## Files Modified

| File | Change |
|---|---|
| `crates/mh-service/src/config.rs` | Five required vars (strict parse, literal presence checks); `DEFAULT_MAX_CONNECTIONS` + its `unwrap_or` + its test deleted; ~14 new constants with two compile-time pins; `QuicTransportParams`; `DrainWindowSource`; three fielded `ConfigError` variants; drain derivation; 22 new tests |
| `crates/mh-service/src/webtransport/server.rs` | `with_custom_transport` + extracted `build_transport_config`; `QuicTransportParams` constructor arg; 4 new tests. **Accept loop unchanged.** |
| `crates/mh-service/src/webtransport/media_transport.rs` | **New.** Real `MediaTransport` / `MediaSendStream` impl over wtransport; error mapping as free functions; `finished` disambiguation; 4 unit tests |
| `crates/mh-service/tests/transport_real_impl.rs` | **New.** @test's required 1/2/3 — finish-then-write against both impls via one shared generic assertion, recv-after-close, reliable-stream round-trip |
| `crates/mh-service/src/main.rs` | 17 fields on the existing startup line; drain reads `config.drain_window`. **No seconds literal for the drain survives.** |
| `crates/mh-service/src/webtransport/mod.rs` | Register + re-export `media_transport` |
| `crates/mh-service/src/lib.rs` | Crate docs: the seam is no longer unimplemented; transport params; the wtransport error-collapse constraint |
| `crates/mh-service/src/transport/mod.rs` | **Cross-boundary, @test ACKed.** Doc-only: `WouldBlock` clause 4 corrected (this commit enables `wtransport/quinn`), plus @test's strengthening that clauses 2/3 hold independent of the feature gate |
| `crates/mh-service/Cargo.toml` | Enable `wtransport/quinn` (required by `with_custom_transport`) |
| `crates/mh-service/tests/common/accept_loop_rig.rs` | Pass deployed `QuicTransportParams` through the real `bind()` |
| `crates/mh-service/tests/gc_integration.rs` | New `Config` fields in the struct literal |
| `docs/runbooks/mh-deployment.md` | **Cross-boundary, @operations ACKed (rollback-ordering paragraph verbatim).** Broken `rollout undo` fixed for MH **and** MC; rollback ordering + `apply -f` sentinel trap; required-env-key table (enforcing-layer column); "both pods CrashLoop" triage with the config-vs-OTel signature collision |
| `docs/TODO.md` | Two entries part-discharged with the undone halves left visibly open; §8 policy-keys entry re-filed to @infrastructure carrying its detection gap; three new entries (task-16 constraint, task-19 bitrate re-confirmation, ConfigMap arithmetic sync) |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Navigation for all of the above, folded into existing bullets to stay at the 75-line cap |


---

## Devloop Verification Steps

**Gate 2 — `DEVLOOP_FAIL_FAST=0 ./scripts/layer-all.sh` (run-all mode, ADR-0033 §4 unattended-caller
contract). Verdict: PASS, first attempt, zero iterations.**

```
LAYER=1 RESULT=OK   DURATION=9      LAYER=5 RESULT=OK   DURATION=9
LAYER=2 RESULT=OK   DURATION=1      LAYER=6 RESULT=N/A  DURATION=5
LAYER=3 RESULT=OK   DURATION=49     LAYER=7 RESULT=OK   DURATION=646
LAYER=4 RESULT=N/A  DURATION=205
TOTAL_DURATION=924 TOTAL_RESULT=N/A
```

### Why `TOTAL_RESULT=N/A` is a pass, verified rather than assumed

`N/A` is not "clean by default", so the Lead re-ran layers 4 and 6 individually to read their
per-wrapper `REASON=` tokens rather than rationalise the aggregate:

| Layer | Wrapper | Status |
|---|---|---|
| 4 | rust | `STATUS=OK REASON=cargo-test-passed` |
| 4 | ts | `STATUS=OK REASON=nx-test-passed` |
| 4 | proto | `STATUS=N/A REASON=not-applicable-to-this-lang` |
| 6 | rust | `STATUS=OK REASON=cargo-audit-passed` |
| 6 | ts | `STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes` |
| 6 | proto (buf) | `STATUS=OK REASON=buf-breaking-passed` |
| 6 | proto | `STATUS=N/A REASON=not-applicable-to-this-lang` |

Both `N/A` aggregates come **solely** from proto's registered intentional-gap placeholders
(`proto/test.sh`, `proto/audit.sh`), which ADR-0033 §6 and the devloop skill name explicitly as
self-justifying — the wrapper's own `REASON=` is the justification, and the implementer owes Gate 2
no separate explanation. The Layer-6 TS `SKIPPED-NO-DIFF no-dep-changes` is the one remaining
documented skip producer and is correct here: `crates/mh-service/Cargo.toml` changed (so `cargo audit`
ran and passed) while no TS dependency manifest did.

Nothing in the run is a `FAIL`, a `FAIL-MISSING-VERB`, or a `NOT-RUN`. The distinction matters and
was checked: `NOT-RUN` would mean *unmeasured*, and none appears — the run was invoked with
`DEVLOOP_FAIL_FAST=0` per the unattended-caller contract, so all seven layers were evaluated.

**Layer 7** ran its full suite against a live cluster: Rust env-tests green (including the
PII-filtering and NetworkPolicy suites), then the browser E2E — 8/8 Playwright specs passing,
`STATUS=OK REASON=browser-e2e-passed`.

**Layer 3** ran all guards green, including the ones this diff most stressed:
`validate-env-config` (`4-services-6-workloads` — `mh-0` and `mh-1` checked separately, not by
union), `validate-cross-boundary-classification` (`19-files`),
`validate-cross-boundary-scope` (`no-drift` — the plan's file list matches the diff exactly),
`validate-knowledge-index`, `validate-todo-tracking`, and both doc-citation guards.

---

## Code Review Results

### @operations — 1 finding, FIXED

**F1 — the runbook promised remediation text on every refusal; the most likely refusal did not have
it.** §Both pods CrashLoop closed with *"Every startup refusal names the offending variable **and** the
remediation location ... If a message you are looking at does not, that is a defect worth filing."*
True of the three new fielded validation variants; **false of `MissingEnvVar(String)`**, which names
the variable and nothing else — and that variant is **cause #1 in the section's own likelihood
order**, since all five newly-required keys land on it. An operator hitting the most probable failure
would read a message without remediation and be told by the runbook that this constitutes a fileable
defect: a bug filed against correct code, mid-outage, on first encounter.

**Fixed by narrowing the claim, not by restructuring the variant.** The sentence now splits the two
refusal kinds: the three *validations* embed remediation inline; a *missing-variable* refusal names
the variable, and the §Required environment keys table is its remediation. A closing paragraph
records why the variant is deliberately left alone — `dt-guard env-config` matches
`MissingEnvVar\("([A-Z_][A-Z0-9_]*)"`, i.e. a string literal **immediately** after the paren, so
converting it to a struct variant carrying a remediation field would blind the guard on **all
thirteen** of MH's required variables while it kept reporting `STATUS=OK`. That is the Gate-1 failure
mode arrived at from the opposite direction, and the note exists so the next reader does not
"improve" it.

Verified independently before fixing: `config.rs` `MissingEnvVar(String)` is bare; the guard regex is
as quoted; and there are exactly **13** distinct required-variable literal sites (a 14th grep hit is
a comment using `"..."`, which the regex correctly ignores).

### Two @operations observations, neither a finding, both actioned

- **The deployed keepalive sits exactly on its validation boundary.**
  `MH_KEEPALIVE_INTERVAL_MS = 10000`, bound = `30000 / 3` = **exactly 10000**, check is `>`, so it
  passes with **zero headroom**. Correct — a chosen 1:3 ratio is what boundary-exact looks like — but
  it means `MIN_KEEPALIVE_TO_IDLE_RATIO` and `MAX_IDLE_TIMEOUT_SECONDS` are a **live wall, not a
  margin**: raising the ratio or lowering the idle timeout sends both pods into `CrashLoopBackOff`
  immediately, with no warning band. Added to `MIN_KEEPALIVE_TO_IDLE_RATIO`'s docstring, with the
  reason it needs saying at all — the arithmetic is invisible from either constant alone, because the
  third value resting exactly on their quotient lives in a manifest.
- **89 s (runbook) vs ~93 s (`configmap.yaml`) for quinn's 1 MiB default.** Same arithmetic,
  differing only in assumed frame size (ADR-0036 §1's rounded ~225 B vs the code's summed 236 B). The
  reconciliation already existed in `NOMINAL_AUDIO_FRAME_BYTES`'s docstring, but both *operator-facing*
  surfaces carried different numbers with the explanation in a Rust file an operator will not open.
  @operations would have accepted it as-is; a one-clause parenthetical in the runbook now states the
  relationship and says explicitly **not** to reconcile them by editing one to match, since that would
  make one of the two surfaces silently wrong.

### @dry-reviewer — F3, FIXED

**The operator-facing message for a malformed number had two homes, and `parse_bounded`'s own
docstring predicted it in writing** before this diff: *"what would actually drift is the pair of
operator-facing messages: reword one copy and the four bounds start explaining a rejection two
different ways."* Adding `parse_required_number` made that live — MH explained "that is not a number"
one way for the five new required vars (with the parse error appended) and another for the four
ADR-0036 §8 bounds (without), a distinction invisible and irrelevant to the operator reading it.

Fixed by **delegation**, not by re-syncing two strings: `parse_bounded` now calls
`parse_required_number` for its parse arm, with `<T as FromStr>::Err: fmt::Display` added to its
bound. The range check stays where it is — that one is genuinely `parse_bounded`-only.

This is a strict improvement rather than tidying: `parse_bounded` previously did
`map_err(|_| ...)`, **discarding the underlying parse error**, so an operator debugging a §8 bound
got strictly less information than one debugging a transport parameter, for no reason anyone chose.
All nine numeric variables now explain a rejection identically, with the same detail. Pinned by
`all_nine_numeric_vars_explain_a_malformed_value_the_same_way`, which asserts the underlying parse
error survives on the §8 path.

### @security — SEC-1 and SEC-2, both FIXED

**SEC-1 — three new required vars had no upper bound, in a file that documents why `> 0` is
insufficient.** `config.rs`'s own §8 preamble already states the rule (*"a `> 0` check alone lets a
fat-fingered value silently re-open the surface the bound exists to close, and it would read as
configured-on-purpose forever"*), and all four §8 bounds carry a `..._CEILING`. The new §1 bounds
carried `reject_zero` and nothing above.

`MH_DATAGRAM_BUFFER_AUDIO_FRAMES` was the one that mattered, for three compounding reasons — all
verified at source, not accepted:
1. It multiplies into memory **twice** (`frames x 236 B` per connection, then x `max_connections`).
   `3200` is ~377 MB against a 1 Gi limit and **boots clean, reporting healthy**; `32000` is ~3.8 GB
   and OOMKills — and OOMKill is `SIGKILL`, which voids the drain window derived 40 lines below.
2. The multiply **wraps silently in release**: confirmed `[profile.release]` in `/work/Cargo.toml`
   sets no `overflow-checks`, so a huge value yields an arbitrary *small* buffer with no error.
3. Sharpest, and independent of typo size: the parameter is a **latency ceiling**, and quinn's
   unchosen 1 MiB default is ~4,443 frames ≈ 89 s. **Any configured value above that is strictly
   worse than the default this task exists to replace** — and V1 alone accepted it. A validation
   permitting a state worse than the thing it was written to eliminate is not a bound.

Both ceilings are **derived, not typed**, per @security's condition:
- `MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING = MAX_DATAGRAM_BUFFER_LATENCY_MS / AUDIO_FRAME_DURATION_MS`
  = 3000 / 20 = **150 frames**. Expressed as a latency because latency is what §1 bounds, so the
  refusal says *what it protects* (`"would queue up to Xms of audio against a 3000ms maximum"`)
  rather than quoting a frame count nobody can evaluate.
- `MAX_CONCURRENT_UNI_STREAMS_CEILING = QUINN_DEFAULT_MAX_CONCURRENT_UNI_STREAMS` = **100**, verified
  against `quinn-proto-0.11.17/src/config/transport.rs` rather than the ConfigMap comment. Derived
  from the declaration's own stated purpose: above quinn's default, MH would be *loosening* a limit
  it declared in order to be tighter than the default, and `configmap.yaml`'s justification for the
  deployed 64 would be false. Equal to the default is still a choice and is allowed.

New `reject_above` mirrors `reject_zero`, keeping the name-the-bound-and-the-remediation shape. The
frames ceiling is checked **before** the frames→bytes multiply, so the wrapping case in (2) is
unreachable. Five new tests, including `the_ceiling_forbids_configuring_something_worse_than_the_quinn_default`
and `the_deployed_values_sit_inside_every_ceiling` — the latter guarding against a hard bound that
would CrashLoop production the moment it landed, which is the failure mode a ceiling can introduce
by itself.

**No ceiling on `MH_MAX_CONNECTIONS`**, at @security's explicit instruction: a hardcoded one would be
a second encoding of a memory budget whose real source is `resources.limits.memory`, which
`docs/TODO.md`'s receive-flow-control entry rules out and which the `resourceFieldRef` startup
validation is for. Its absence is deliberate.

**SEC-2 — the corrected per-connection ledger omitted the datagram SEND buffer.** The
`CONNECTION_RECEIVE_WINDOW_BYTES` docstring bills itself as superseding `configmap.yaml` and is
written to be inherited by story 2, yet summed only `receive_window + datagram_receive_buffer
[+ crypto_buffer]`. At the deployed 32 frames the missing term is 7,552 B — ~3% of ~264 KB, so the
~13% conclusion is unchanged — **but it is the only operator-tunable term in the ledger**. A ledger
that omits the one value an operator can move cannot show whoever inherits it the effect of moving
it. Added, listed last with that reasoning explicit, and bounded by SEC-1's ceiling (worst case
~35 KB/conn, ~17 MB across 500). Omitting it while correcting the same defect class in the ConfigMap
would have been the narrower version of the same mistake.

### @observability — 2 findings FIXED, 1 nit PARTLY declined with reason

**OBS-1 — a comment claimed both conversion factors were on the startup line; only one was.**
`nominal_audio_frame_bytes=236` made `32 x 236 = 7552` checkable from the log, but the `20` in
`frames x 20 = ms` was not a field — so the ms link required opening `config.rs`, exactly the
asymmetry the comment denied. Added `audio_frame_duration_ms` (18th field). Taking the field rather
than correcting the comment, on @observability's own argument for vetoing an earlier trim: the two
halves of one conversion should not be held to different standards, and
`transport_datagram_send_buffer_ms` is the field an operator actually reasons with.

**OBS-2 — `media_transport.rs` sits outside the deny meant to keep it emission-free.** The file is
clean (verified: no `tracing` / `metrics` / `log` / `println!` / `#[instrument]`), but ADR-0036 §11's
durable enforcement is a **directory-scoped** deny over `crates/mh-service/src/media/`, which arrives
at task 16 — and this file is media-path code *by function* at a path that deny will not match.
Whoever adds it will scope it to `media/`, see it pass, and reasonably believe they are done. Filed
in `docs/TODO.md` §Observability Debt naming the file explicitly, with the caveat that relocating it
must be checked against §11's sibling-not-child layout rule rather than assumed. Guard machinery is
infrastructure's and the deny does not exist yet, so this is a filing, not a fix.

**The nit — naming the ConfigMap path in `InvalidValue` messages — taken where true, declined where
it would introduce a falsehood.** Added to `reject_zero` and `reject_above`: every caller supplies a
key that really is in `mh-service-config`. **Not** added to `parse_required_number`, because F3 now
routes the four ADR-0036 §8 policy bounds through it, and those are **in no manifest at all**
(`docs/TODO.md`, the §8-policy-keys entry) — naming a ConfigMap path there would send an operator to
look for a key that is not in the file. The reason is recorded at both sites so the asymmetry does
not read as an oversight.

### @test — 1 finding, FIXED

The one bare `assert!` in the diff, in
`the_deployed_keepalive_satisfies_the_ratio_the_configmap_asserts`, guarding the zero-headroom
relationship @operations flagged (deployed `10000ms` x ratio `3` == idle `30000ms`, exactly). It is
the thing that goes red in CI instead of a silent production CrashLoop if either constant moves the
boundary — and it said only "assertion failed". Now prints the full arithmetic, names which constant
must have moved, states that both pods would refuse to start, and says **do not relax the ratio to
make this pass** — because making a boundary test green by moving the boundary is the obvious wrong
fix and the message is where it gets pre-empted.

### Post-fix verification

After @operations' F1 only: as below. **After all six Gate-3 findings** (@dry F3, @security SEC-1/2,
@observability 1/2, @test 1): `cargo test -p mh-service` 17 binaries green, **213 lib tests**
(+6 this round) · `cargo clippy -p mh-service --all-targets` **0 warnings** ·
`cargo fmt --all --check` clean · `cargo check --workspace --all-targets` 0 errors ·
`validate-{todo-tracking, cross-boundary-classification (19 files), cross-boundary-scope (no-drift),
knowledge-index, env-config (4-services-6-workloads), kustomize, doc-citations-no-line-numbers,
doc-citations-symbol-resolves}`, `no-{secrets,pii}-in-logs` and `test-registration` all `STATUS=OK`.
Changeset unchanged at 14 files — every fix landed inside files already in the diff, no new file.

---

## Gate 2 Attempt History

| Attempt | Trigger | Result |
|---|---|---|
| 1 | Implementer signalled Ready | **PASS**. `TOTAL_RESULT=N/A` from proto intentional-gap placeholders only. |
| 2 | Six Gate-3 findings fixed → real code changed, so the docs-only Layer-3 shortcut did not apply | **FAIL at Layer 5.** `STATUS=FAIL REASON=cargo-clippy-failed`: five `clippy::doc-markdown` errors in `config.rs` doc comments added that round. Consumed one of three attempts. Layers 1-4, 6, 7 all green in the same run (invoked `DEVLOOP_FAIL_FAST=0`, so nothing downstream was left `NOT-RUN`). |
| 3 | Lint fixed with backticks (no `#[allow]`) | **PASS.** `LAYER=5 RESULT=OK`; zero `FAIL`/`PRECONDITION_FAILURE`/`UNKNOWN` statuses anywhere in the run. |

The Layer-5 miss is worth recording because the implementer's own sweep had reported clean:
they ran `cargo clippy -p mh-service --all-targets`; the gate runs
`cargo clippy --workspace --all-targets -- -D warnings` (`scripts/lang/rust/lint.sh`). The
`-D warnings` promotes `doc_markdown` from a pedantic warning to a hard error, so the pre-signal
sweep was measuring a weaker predicate. The implementer's own diagnosis went further and is the
better one: `lint.sh` is a one-line file that was available the whole time, and they *reconstructed*
what they assumed it ran rather than reading it — the same shape as the corpus-count error, verifying
a derived thing instead of the authority next to it.

Two things did **not** happen and are recorded because each was the cheap option: the fix did not
reach for `#[allow(clippy::doc_markdown)]` (which would have been the third appearance of the
guard-defeating pattern this devloop catalogued, and the least defensible, since the only motive
would have been clearing a gate); and a first pass that shortened three doc lines to fit the
backticks silently truncated two sentences — clippy was green on that version — which was caught only
by re-reading the whole doc block rather than patching at the reported column offset.

---

## Code Review Results — Lead Summary

All seven reviewers reported. **Zero ESCALATED. Zero accepted deferrals.**

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 |
| Test | RESOLVED-FIXED | 1 | 1 | 0 |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 |
| Code Quality | **CLEAR** | 0 | — | 0 |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 |
| Semantic Guard | **CLEAR** (native SAFE) | 0 | — | 0 |

**Security** — SEC-1: three §1 vars had `reject_zero` and no upper bound, in a file whose own
preamble states why that is insufficient. `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` multiplies into memory
twice, `[profile.release]` sets no `overflow-checks` so the multiply wraps silently in release, and —
the argument independent of typo size — quinn's 1 MiB default is ~4,443 frames, so any value above
that was *strictly worse than the default this task exists to replace* and the existing validation
accepted it. Fixed with **derived** ceilings (frames from a latency budget; uni-streams from quinn's
own default read out of `quinn-proto` source), checked before the multiply. SEC-2: the corrected
per-connection ledger omitted the only operator-tunable term.

**Test** — the conformance suite they made a condition of plan approval caught a structural
divergence on its first run (below). Their finding was the diff's one bare `assert!`; the fix also
pins the correct remediation, pre-empting the make-CI-green-by-moving-the-boundary wrong fix.

**Observability** — the startup line's comment claimed both conversion factors were checkable from
the log; only one was a field. Fixed by adding the 18th field rather than weakening the comment.

**DRY** — F3: the new `parse_required_number` became a second home for the malformed-number operator
message and had already diverged from `parse_bounded`'s copy. Fixed by delegation, which also
repaired the pre-existing asymmetry.

**Operations** — the runbook promised every startup refusal names a remediation location; true of the
three new validations, false of `MissingEnvVar`, which is cause #1 in the runbook's own likelihood
order. Fixed by narrowing the claim, **not** by enriching the variant — which would have broken
`dt-guard`'s `MissingEnvVar\("` regex and blinded the guard on all thirteen required vars while it
kept printing `STATUS=OK`.

### The headline finding: a contract that did not hold

`tests/transport_real_impl.rs` failed on its first run — `ConnectionClosed` where the seam requires
`StreamClosed`. Structural, not a mapping slip: `wtransport-0.7.2` folds quinn's stream-scoped
`ClosedStream` together with `ConnectionLost` into one `NotConnected` value, so no mapping choice
recovers information already discarded. Fixed by recovering the distinction (`WtSendStream.finished`,
latched only on a successful `finish`) rather than weakening test or contract; the inverted mapping
was rejected because it would report a lost connection as a dead stream, leaking an
`active_connections` slot permanently. The unrecoverable case — a peer-initiated reset — is filed as
a task-16 design constraint with an explicit "do not read the `finished` flag as having closed this."

Before this suite, nothing in the tree sent a datagram or opened a unidirectional stream over the
real transport; every `finish` assertion ran against the double, which task 16 would then have
trusted.

### On what is filed in `docs/TODO.md` versus what was deferred

Every finding raised by every reviewer was fixed in this diff — all seven verdicts are CLEAR or
RESOLVED-FIXED, none is RESOLVED-DEFERRED, and nothing was spun out. §Accepted Deferrals is
correspondingly empty.

Seven `docs/TODO.md` entries were nonetheless filed during this devloop, and two pre-existing entries
were part-discharged. None of them was a finding against this changeset, so none is a deferral: they
are adjacent defects found while reviewing (the 163 broken runbook commands; the `dt-guard`
workload-floor gap), forward constraints discovered during implementation (the task-16
dead-stream/dead-connection limit; the task-19 bitrate re-confirmation), an ownership re-file (the
four §8 policy-bound ConfigMap keys → @infrastructure), and DRY extraction opportunities routed under
the ADR-0019 exception. Each has a named owner. The two part-discharged entries (`:216`, `:45`) were
restructured so their undone halves — notably @security's `resourceFieldRef: limits.memory` startup
validation — stay visibly open rather than buried in a struck-through paragraph.

### One composition that had no clean fix

@dry-reviewer's F3 and @observability's remediation-location nit are each individually correct and
compose into a **false statement**: F3 routes the four ADR-0036 §8 policy bounds through the shared
parse helper, and those bounds are in no manifest, so a ConfigMap path there would name a location
that does not exist. The implementer took the nit where it is truthful and declined it in the shared
helper, then asked @observability to rule rather than choosing. @observability upheld the decline and
rejected the Lead's proposed threaded-parameter alternative on stronger grounds than either: five of
the nine variables have no ConfigMap path (four in no manifest, one written by the kustomize
`replacements:` block), and the only useful placeholder text goes false the moment @infrastructure
lands those keys — manifest state hand-copied into Rust literals with no forcing function.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `7c740e71d302388df8fa65f9e23e709ab6c39f19`
2. Review: `git diff 7c740e71..HEAD`
3. Soft reset: `git reset --soft 7c740e71`
4. Hard reset: `git reset --hard 7c740e71`

---

## Issues Encountered & Resolutions

### Issue 1: The task text's Opus sizing clause contradicted its own sizing rule
**Problem**: The task defined the constant at 32 kbps but also said "sized for the client's
configured VBR ceiling"; story task 19 sets a 32–48 kbps range, so the two clauses disagreed.
**Resolution**: The Lead first ruled for the ceiling (48 kbps) on a frames-are-a-floor reading.
@dry-reviewer then retracted their own argument, the implementer re-derived it independently, and the
Lead verified `adr-0036-media-flow.md` directly and **reversed**. The send buffer is a latency
ceiling, so queued latency is maximised by the smallest frame and the bound holds only when sized at
the floor. Landed at 32 kbps / 236 B. Three agents inverted the same sign before any of them caught
it; the durable fix is a compile-time assertion whose failure message names the trap.

### Issue 2: A seam contract that the real implementation could not satisfy
**Problem**: The required conformance test failed on its first run — `ConnectionClosed` where the
seam requires `StreamClosed`. `wtransport` folds quinn's stream-scoped `ClosedStream` together with
`ConnectionLost` into one value, so no error mapping could recover the distinction.
**Resolution**: Recovered the distinction with locally-held state (`WtSendStream.finished`) rather
than weakening the test or the contract. The inverted mapping was rejected as leaking an
`active_connections` slot permanently. The genuinely unrecoverable case is filed for task 16.

### Issue 3: Gate 2 red at Layer 5 on the second attempt
**Problem**: Five `clippy::doc-markdown` errors, after the implementer's own sweep reported clean.
**Resolution**: The sweep ran `cargo clippy -p mh-service --all-targets`; the gate runs
`cargo clippy --workspace --all-targets -- -D warnings`, which promotes `doc_markdown` to a hard
error. Fixed with backticks, explicitly not `#[allow]`. A first pass at the fix silently truncated
two sentences to make room for the backticks and was clippy-green; caught by re-reading the whole doc
block rather than patching at the reported column offset.

### Issue 4: A reviewer's corpus count taken while the corpus was being edited
**Problem**: @operations filed a 162-site defect class; the figure joined a post-fix count for one
service to a pre-fix count for another, while their own review was mutating the corpus.
**Resolution**: The implementer re-derived rather than transcribing (163 pre-fix, 161 remaining).
@operations corrected their own filing and left a methodology note so the next reader does not revert
it. The implementer declined to edit another specialist's entry silently and surfaced it instead.

### Issue 5: Two individually-correct findings that composed into a falsehood
**Problem**: @dry-reviewer's F3 routed the ADR-0036 §8 policy bounds through a shared parse helper;
@observability's nit asked that helper's message to name a ConfigMap path. Those bounds are in no
manifest, so applying both would have printed a remediation location that does not exist.
**Resolution**: The implementer took the nit where it is truthful, declined it in the shared helper,
and asked @observability to rule rather than choosing. @observability upheld the decline and rejected
the Lead's threaded-parameter alternative on stronger grounds than the Lead had.

---

## Lessons Learned

### 1. Escalating a two-word fix found a 163-site defect that the single site could not reveal

**The setup made escalation look like a waste of a message.** `mh-deployment.md:335` carried
`kubectl rollout undo deployment/mc-service`. There is no `mc-service` Deployment. The fix was two
words, in a file already open in this diff, **three lines below a change already being made**, and
the same defect three lines *above* had already been approved for repair. Every incentive pointed at
just making it.

It was escalated to @operations instead, on the reasoning that MC's workload names are
meeting-controller's to confirm and that this commit is not what made that line wrong. @operations
overruled the reasoning — *"a workload's name is not a domain judgement, it is a manifest lookup"* —
and told me to take it. **The escalation was still the right call, and the reason has nothing to do
with who was right about ownership**: it triggered a corpus sweep.

**What the sweep found** (figures re-derived against the tree rather than accepted; see the
correction note below):

| Target | Sites | Correct? |
|---|---|---|
| `deployment/mh-service` | 37 | **Broken** — MH is a per-instance pair, `mh-0` / `mh-1` |
| `deployment/mc-service` | 80 | **Broken** — MC is a per-instance pair, `mc-0` / `mc-1` |
| `deployment/ac-service` | 46 | **Broken by KIND, not name** — AC is a `statefulset/` |
| `deployment/gc-service` | 115 | **CORRECT** — GC genuinely is one Deployment of that name |

**163 broken targets across 7 runbook files — interleaved with 115 correct ones that are textually
indistinguishable from them.**

**The part that makes this a lesson rather than an anecdote.** The obvious efficient fix — a
pattern-wide `sed` over `docs/runbooks/**` — would have **corrupted all 115 correct `gc-service`
references** while fixing the rest. And even a per-service rewrite would have missed AC, which is
wrong by *kind* rather than by name. Three different right answers hid under one wrong-looking
pattern, and **nothing observable at the single site disclosed any of it**. Only the sweep did, and
the sweep happened only because the fix was escalated instead of made.

**The generalisation, which holds independently of the outcome:** the cost of escalating a small
change is one message. The cost of not escalating is bounded only by how far the pattern extends —
and *that is precisely what cannot be seen from inside the single instance*. The asymmetry is the
argument. This would have been the right call even if the sweep had found nothing, and it should be
read that way rather than as vindication-by-result.

Two fixes landed here (`mh-service` and `mc-service`, both in this diff's own file). The other 161
are filed with the durable fix: a guard resolving every runbook `kubectl <verb> <kind>/<name>`
against `infra/services/**`. That entry also records — at my request, and @operations agreed — that
such a guard is **necessary but not sufficient**, since it would green a command that resolves and
still does the wrong thing (`rollout restart` is right against both pods; `exec` targets one and the
runbook must say which).

> **Figures corrected on re-derivation, and the discrepancy is itself instructive.** @operations
> supplied "162 across 7 files"; counting against the tree gives **163 across 7**. The gap is one
> `mc-service` site: their total combined a **post-fix** `mh-service` count (36) with a **pre-fix**
> `mc-service` count (80), i.e. two snapshots of a tree that this very diff was mutating underneath
> the measurement. Post-fix the remaining figures are **161 across 6 files** (`mh-deployment.md` now
> has zero). Nothing in the argument changes. It is recorded because a corpus count taken while the
> corpus is being edited is exactly the kind of number that gets quoted forward as settled, and the
> filed TODO entry should carry a figure someone can reproduce.

### 2. Tidiness and helpfulness are the two motives that produce guard blindness

The same defect was reached twice from opposite directions, and both times it would have left
`dt-guard env-config` reporting `STATUS=OK` over **all thirteen** of MH's required variables:

- **Gate 1 — tidiness.** Extract a `require_var(vars, "MH_X")` helper. Thirteen near-identical
  `.ok_or_else(|| ConfigError::MissingEnvVar("..."))` sites is exactly the duplication a reviewer is
  trained to collapse. @dry-reviewer and @operations both flagged it *pre-emptively*, which is the
  only reason it was never written.
- **Gate 3 — helpfulness.** Restructure `MissingEnvVar(String)` into a struct variant carrying a
  remediation field, in response to a legitimate finding that its message lacks one. @operations
  named the trap in the same message as the finding.

The guard matches `MissingEnvVar\("([A-Z_][A-Z0-9_]*)"` — a string literal **immediately** after the
paren. Either change breaks the match, silently, across every variable.

**The second is the more dangerous of the two, because it arrives dressed as fixing a reviewer
finding** — it carries a reviewer's authority and the momentum of being mid-remediation, which is
when scrutiny is lowest. The mitigation now lives in prose at both sites, phrased as *why this stays
ugly*, because a comment saying "do not extract this" is what a future tidier will read first.

**A third instance of the same shape appeared in this diff, self-inflicted:** the guard scans
comments, so an *illustrative* `MissingEnvVar` construction written in **prose** with an upper-case
string literal was reported as two required variables no manifest declares. The guard was right — a
detector keyed on a literal cannot know it is reading documentation. The docstring explaining this
now deliberately obeys its own rule.

### 3. A contract asserted only against its test double is not asserted

`transport/mod.rs` elevates finish-then-write to a **contract** rather than an implementation
detail, explicitly because a double and the real transport diverging there would be invisible. It
then had that contract verified against the double alone — `wt_client` is bidi-only, and **nothing
in the tree opened a unidirectional stream or sent a datagram over the real transport**.

@test made a real-implementation test a condition of plan approval. It failed on run one:
`wtransport` folds quinn's stream-scoped `ClosedStream` together with connection loss into a single
`NotConnected`, so the distinction the contract depends on is destroyed one layer below where the
contract is written. Task 16 would have inherited a behaviour the double invented.

Generalisable form: **a seam's contract must be asserted against every implementation that ships,
and "the vendor documents this behaviour" is a claim about the vendor, not about the layer you
actually call.** The contract cited quinn correctly; quinn was not the layer being called.

### 4. Reviewers self-correcting caught more than reviewers confirming

Recorded because the confirmations are not the evidence the gate worked:

- **@dry-reviewer retracted the value half of their own F2** after it had already been ruled on,
  which reversed the constant from 276 B back to 236 B. Three people — implementer, reviewer, Lead —
  had independently reasoned about a **latency ceiling** as though it were a capacity guarantee, each
  having read the governing ADR row. The sign error survived a reviewer finding, an escalation, and a
  Lead ruling; it did not survive the reviewer re-reading their own argument.
- **@security sharpened their own accepted fold twice**, adding `DATAGRAM_ENCAPSULATION_OVERHEAD_BYTES`
  after establishing that `max_datagram_frame_size` bounds the QUIC DATAGRAM frame rather than our
  payload — an off-by-encapsulation floor would have *passed* for a value that fails on the wire.
- **@operations dismantled the premise of my runbook deferral rather than its conclusion**, showing
  the task-21 collision I had priced it against was zero all along because the correct home was
  `mh-deployment.md`, not `mh-incident-response.md`.

The through-line with lesson 1: in every case the useful information was **one level up from where
the question was being asked** — the corpus rather than the site, the range rather than the value,
the file's identity rather than its content.

**The sharpest instance is not the escalation but the corpus count itself**, a framing @operations
supplied against their own error after re-deriving it. Every individual figure in the 162 was
correct *at the moment it was measured*; what was wrong was that two of them were measured at
different moments, while this diff was editing the corpus underneath the measurement. So the level
up was not a bigger sample — it was the **snapshot**. That is a harder instance than the others,
because nothing about the number looked provisional: a count is the archetype of a settled fact, and
it had no visible seam where two readings had been joined. It survived only because it was
re-derived rather than transcribed, which cost one command.

Recorded with attribution because it is also an instance of lesson 4: the correction came from the
author of the number, unprompted and against their own filing, and it produced a better articulation
of the lesson than the original claim had.

### 5. A guard reported clean about how many things it checked

`dt-guard env-config`'s hard-fail is `discovered.workloads.is_empty()` — **per service,
all-or-nothing**. It catches "this service has zero workloads". It does **not** catch a service
dropping from two workloads to one: the list is non-empty, no hit fires, the status token quietly
reads `5-workloads` instead of `6`, and **`STATUS=OK`** — with the dropped instance entirely
unchecked for the required-env coverage this task just made load-bearing.

The count *is* the intended detector; `Report::checked_workloads`'s docstring says so. But
**visibility is not enforcement** — it needs a human to read the number *and remember what it should
be*. Nothing asserts it against the tree.

Listed separately from the corpus-count lesson despite sharing its through-line, because the two
differ where it matters operationally: that was a **measurement artifact a human produced**, and this
is a **control whose coverage shrinks with nobody in the loop at all**. It is also the only one of the
five about a mechanism this project relies on to catch the other four.

Found by checking a colleague's closing courtesy at source instead of acknowledging it — the same
move that produced lesson 1. Filed to @infrastructure (`dt-guard` machinery); analysis lives in the
`docs/TODO.md` entry, whose load-bearing half is that the fix must **derive** the expected count from
the manifest tree rather than enumerate it, or the guard acquires the very defect it is being taught
to detect.
