# Devloop Output: MH Transport Trait Seam + Deterministic Loss/Delay/Backpressure Shim

**Date**: 2026-09-01
**Task**: Introduce a per-connection transport trait seam in `mh-service` mirroring the client SDK's `IWebTransport` shape, plus a deterministic test-double shim (datagram loss, added delay, send-capacity backpressure) that drives the forward path with zero syscalls.
**Specialist**: test (paired with media-handler)
**Mode**: Agent Teams (v2) — full, HEADLESS
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~2h15m (19:44Z start → 21:55Z Gate-3 close; includes one Gate-2 failure, a mid-review file truncation and recovery, and three voided pipeline runs)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `4419df26ce523a7013d449859c79f0f65044635a` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |
| Story | `docs/user-stories/2026-08-27-hear-yourself-through-handler.md` task 5 |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (test) spawned |
| Implementing Specialist | `test` |
| Iteration | `1` |
| Paired media-handler | `paired-media-handler` spawned |
| Security | spawned |
| Test | spawned |
| Observability | spawned |
| Code Quality | spawned |
| DRY | spawned |
| Operations | spawned |
| Semantic Guard | spawned |

---

## Task Overview

### Objective
Deliver the ADR-0036 §10 transport seam in MH: hot-path per-connection I/O (send datagram, receive datagram, open/finish unidirectional stream, write to stream) behind a trait; endpoint and accept loop stay concrete. Ship a deterministic test-double implementing the trait with `set_datagram_loss` (in-flight loss, for hop-sequence gap detection) and `set_send_capacity(n)` (`WouldBlock(Bytes)` handing the payload back, to trip the bounded egress queue), `Bytes` payloads for the refcount zero-copy gate, generics not `dyn`, `tokio::time` deterministic delay under `start_paused` (no Clock trait). Plus a reachability test proving each drop path is deterministically reachable.

### Scope
- **Service(s)**: mh-service (new transport seam module + test shim)
- **Schema**: No
- **Cross-cutting**: No (single service; consumed later by MH forward-path and transport-config tasks)

### Debate Decision
NOT NEEDED — ADR-0036 §10 already decided the seam and its shape; this is implementation of a ratified ordering constraint.

---

## Cross-Boundary Classification

No Guarded Shared Area is touched. `crates/media-protocol/**`, `proto/**`, `proto-gen/**` and
`crates/common/src/webtransport/**` (a manifest glob that currently matches nothing) are all
**untouched** — the seam types are `bytes::Bytes` and `std::time::Duration` only, so no
`media-protocol` type appears in any signature (@security 2/5, @observability 8, @dry-reviewer D1,
@semantic-guard). `crates/mh-service/**` is media-handler's domain, not a GSA; this loop is
`--paired-with=media-handler`, so the Domain-judgment row below is **owner-implemented** by
@paired-media-handler rather than trailer-approved (ADR-0024 §6.3).

| Path | Classification | Owner | GSA? | Plan row | Change |
|------|----------------|-------|------|----------|--------|
| `crates/mh-service/src/transport/mod.rs` | Not mine, **Domain-judgment** | media-handler | No | P1 | NEW — the trait seam: `MediaTransport` + `MediaSendStream` + `DatagramSendError` + `TransportError`, plus the contract doc comment. **Authored by @paired-media-handler** (owner-implements: the consumer of the seam shapes the seam), to the shape agreed in §Planning. Zero `wtransport`, zero `tracing`/`metrics`/`log`, zero `media-protocol` imports. Includes the inline `#[cfg(test)]` unit module specified in P1 |
| `crates/mh-service/src/lib.rs` | Not mine, **Minor-judgment** | media-handler | No | P2 | `pub mod transport;` added to the module list; one line added to the crate-status doc paragraph naming the seam as landed-and-unconsumed. No other change |
| `crates/mh-service/Cargo.toml` | Not mine, **Minor-judgment** | media-handler | No | P3 | One line under `[dev-dependencies]`: `mh-test-utils = { path = "../mh-test-utils" }`. No production dependency added (@operations 2) |
| `crates/mh-service/tests/transport_seam_reachability.rs` | **Mine** | test | No | P7 | NEW — the reachability suite. **Basis for `Mine`, stated so nobody re-litigates it from the table alone**: test targets are the test specialist's domain per the CLAUDE.md specialist table, the path is in no Guarded Shared Area, and @paired-media-handler is co-implementer on this loop and reviews it at Gate 3 as crate owner regardless of the label. Both @paired-media-handler and @observability reviewed this row and neither challenges it |
| `crates/mh-test-utils/Cargo.toml` | **Mine** | test | No | P4 | NEW crate manifest. Deps: `mh-service`, `bytes`, `tokio`. Mirrors `ac-`/`gc-`/`mc-test-utils` |
| `crates/mh-test-utils/src/lib.rs` | **Mine** | test | No | P5 | NEW — crate doc + one `pub mod transport_shim;` |
| `crates/mh-test-utils/src/transport_shim.rs` | **Mine** | test | No | P6 | NEW — `LossDelayTransport`, the deterministic loss/delay/backpressure test double |
| `Cargo.toml` | **Mine** | test | No | P4 | One line: `crates/mh-test-utils` added to `[workspace] members`, alphabetically beside the three sibling test-utils crates |
| `docs/TODO.md` | **Mine** | test | No | P8 | **SHARED FILE this loop** — @observability appends under §Media Path Obligations, @dry-reviewer under §Cross-Service Duplication. Test lands three: two under §Test Debt (the `MockWebTransport` adverse-condition-knob gap with its copy-on-capture at `:234`; the four-crate test-utils dev-dep-edge guard spun out from @security S2) and one under §Documentation Hygiene **authored by @code-reviewer** (ADR-0036 §10 Tier 1a injected-clock wording; ownership stays with them, test lands it because test is already editing the file). Different sections, no textual conflict; re-read before write, never from a stale copy |
| `docs/devloop-outputs/2026-09-01-mh-transport-seam/main.md` | **Mine** | test | No | — | This file |

**Not touched, stated so the absence is deliberate**: `crates/mh-service/src/webtransport/server.rs`
and `connection.rs` — the endpoint, the accept loop and the connection lifecycle stay concrete and
byte-identical, so rollback is a clean revert with zero runtime-behaviour surface (@operations 3).
No production code is type-parameterized in this task; nothing in `mh-service`'s release artifact
changes at all.

---

## Gate 1 — Plan Confirmation

`./scripts/guards/simple/validate-cross-boundary-classification.sh` on this file:
`STATUS=OK REASON=cross-boundary-classification-clean-1-files` (exit 0).

| Reviewer | Plan Status |
|----------|-------------|
| Paired media-handler (co-implementer) | confirmed |
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

### Lead adjudications at Gate 1

1. **`Bytes` on datagrams / `&[u8]` on stream write (P1) — APPROVED as a deviation from the literal
   task text.** The task text's stated purpose for `Bytes` is "so the refcount gate can assert
   zero-copy", and that gate lives on the datagram path, which keeps `Bytes` by value on
   `send_datagram` and as the `recv_datagram` return. The stream path carries no such gate,
   `wtransport::SendStream::write_all` takes `&[u8]` upstream, and forcing `Bytes` there would make
   callers materialize the length prefix as an owned buffer per write — pressure to pull framing
   down into the seam, which is the duplication @dry-reviewer's D2 exists to prevent. The deviation
   serves the task's stated intent rather than trading it away. Raised jointly by
   @paired-media-handler and @dry-reviewer for open adjudication rather than settled in a side
   channel, which is the right instinct.
2. **P9's ADR-0036 §10 "injected clock" tension — endorsed as handled.** The task instruction and the
   ADR sentence conflict in surface wording. The plan follows the task instruction and argues why
   (the ADR names a property; tokio's paused timer satisfies it at the runtime boundary; no `Clock`
   trait exists in the workspace and pause/advance is the established idiom at four existing sites).
   Flagged to @code-reviewer as decided rather than missed. @code-reviewer owns the ADR-drift
   follow-up entry per their agreement with @dry-reviewer.

---

## Planning

### P0. Problem restated in mechanism-language

**Instance language** (the task): MH needs a per-connection transport trait plus a loss/delay/
backpressure test double.

**Mechanism language**: *every drop counter in ADR-0036 claims to observe a failure that only real
network conditions produce; this task installs the boundary at which those conditions can be
manufactured, so "we believe the counter can fire" becomes "here is the line that fires it."* §10
says exactly this — the seam's second of four justifications is "deterministic reachability of drop
paths for metric coverage".

The wider class that restatement produces, with same-owner siblings the task does not name:

1. **The SDK half of the same mechanism already exists and lacks the knobs.**
   `packages/test-utils/src/MockWebTransport.ts` has injection (`simulateIncomingDatagram`) and
   inspection (`getOutboundDatagrams`) but no loss, capacity or delay knob. §11 assigns the SDK the
   sender-side drop counter — "the SDK keeps the transport queue shallow, owns a bounded queue above
   it, makes the drop decision there, and counts it" — and that counter will have the identical
   reachability gap this task closes for MH. It also copies on capture (`out.set(chunk)` at
   `MockWebTransport.ts:234`), so a TS-side zero-copy claim is not assertable either. Client-owned,
   out of scope, recorded in `docs/TODO.md` (P8) rather than silently left for whoever writes that
   counter.
2. **ADR-0036 Assumption 1 wants both halves in one harness** ("handler server half and browser
   client half in one harness through the transport seam"). Two doubles, two languages, no compiler
   between them. @dry-reviewer is filing the drift entry; noted here so the shape is chosen with
   that future in mind rather than against it.

Neither widening changes this task's scope. Both are surfaced because they are the same mechanism
with a different owner, which is precisely what the restatement is for.

### P1. The seam — `crates/mh-service/src/transport/mod.rs`

**Placement.** A NEW sibling module. Not `media/` — §11 requires `media/` hold only the hot path so
infrastructure's directory-scoped macro-deny equals the hot-path boundary, and a narrowed deny is
the failure mode §11 names. Not `webtransport/` — that is the accept loop and lifecycle, and task
12 puts the *real impl* there importing the trait from elsewhere. Not `crates/common/src/
webtransport/` — that would materialize a GSA glob that currently matches nothing, converting a
one-consumer trait into an owner-co-sign surface for zero benefit (@dry-reviewer D1) — and the
manifest lists that glob as `[meeting-controller, protocol]`, so it would have required co-sign from
two specialists who do not own the code while its actual owner was not on the list at all.

**Shape** (agreed with @paired-media-handler, who compiled both variants):

- `trait MediaTransport: Send + Sync + 'static` with `type SendStream: MediaSendStream`.
- `fn send_datagram(&self, payload: Bytes) -> Result<(), DatagramSendError>` — **sync**, because
  `wtransport::Connection::send_datagram` is `pub fn`, not `async fn` (wtransport-0.7.2
  `src/connection.rs:297`). An async signature would force task 12 to write a future that never
  yields and put a misleading await point on the per-frame path.
- `fn recv_datagram(&self) -> impl Future<Output = Result<Bytes, TransportError>> + Send`
- `fn open_uni(&self) -> impl Future<Output = Result<Self::SendStream, TransportError>> + Send`
- `trait MediaSendStream: Send` with `write_all(&mut self, buf: &[u8]) -> impl Future<...> + Send`
  and `finish(&mut self) -> impl Future<...> + Send`.

**Deviation from the literal task text, adjudicated here in the open rather than in a side channel.**
The task says "Payloads are `Bytes` so the refcount gate can assert zero-copy." We split the payload
type: `Bytes` by value on `send_datagram` and as the `recv_datagram` return, `&[u8]` on stream
write. Ownership transfer is needed on the datagram path — for the `WouldBlock` hand-back and for
the fan-out refcount gate — and is not needed on the stream path; the asymmetry is in the upstream
API (`wtransport::SendStream::write_all` takes `&[u8]`), not a style choice. Forcing `Bytes` on the
stream path would make callers allocate for the length prefix, which is exactly the framing
@dry-reviewer D2 requires stay above the seam. The refcount gate the task text is protecting lives
on the datagram path and is unaffected. Raised for Gate-1 adjudication at
@paired-media-handler's and @dry-reviewer's joint request; neither intends to raise it as a finding.

Four operations, no lifecycle: no `ready`, no `closed`, no `close(info)`. Loop termination comes
from `recv_datagram` returning the closed variant plus the existing `CancellationToken`.

**RPITIT with an explicit `+ Send`, not bare `async fn`.** @paired-media-handler compiled the
counter-example: bare AFIT compiles fine here and fails only at task 16's `tokio::spawn` of a
generic consumer ("future cannot be sent between threads safely"). That is a defect this task would
ship green and the forward-path task would inherit after two tasks are built against the signature —
the ADR's own "cheap against nothing, expensive against concrete types", applied to the seam's own
signature. Scratch files at `/tmp/seamcheck/{a,b}.rs`; we re-run both at implementation.

**"Generics, not dyn" is compiler-enforced, not conventional.** RPITIT makes the trait
dyn-incompatible: `&dyn MediaTransport` is E0038. So @security's "no `Box<dyn Transport>`, no
config-selected transport" needs no guard and no review vigilance. One doc line will say this is
deliberate, because a future author who hits E0038 and "fixes" it by boxing the futures reintroduces
the per-frame allocation the seam exists to avoid.

**Error types — two enums, and the split is the point.**

- `DatagramSendError`: `WouldBlock(Bytes)`, `TooLarge`, `ConnectionClosed`, `DatagramsUnsupported`.
- `TransportError`: `ConnectionClosed`, `StreamClosed`. Fatal; tear down; count nothing.

**Flat, not nested** (@paired-media-handler, accepted). An earlier draft had
`DatagramSendError::Fatal(TransportError)`, which admits impossible states: `Fatal(StreamClosed)`
from a datagram send is unrepresentable in reality but representable in the type, and task 16 would
have to write a match arm that cannot be exercised — the dead-arm problem P7 test 9 exists to
prevent. Flat gives a 1:1 map onto wtransport's three real send variants plus the one shim-only
variant, so task 12's mapping is total and obvious. The cost is named rather than absorbed: test 9
now needs `DatagramsUnsupported` reachable, so the shim carries `set_datagrams_unsupported()`
alongside `set_closed()`. Uniform `?` propagation in task 16 is the thing given up; the consumer who
would write those arms asked for flat, which is the right tie-breaker.

`WouldBlock` carries the payload back **uncopied** so the caller's bounded queue owns the drop
decision (§11 "makes the drop observable by construction"). It is a *different variant* from
connection-gone, so the future egress-exhaustion alert cannot count connection deaths as
backpressure (@observability 2, @paired-media-handler 6). No variant formats a `String` — the hot
path allocates nothing on error (@code-reviewer 5). `TooLarge` does not carry the payload back: it
is not retryable at that size, and handing bytes back would invite a retry loop.

**`WouldBlock` has no production producer, and the doc records three facts rather than one
sentence** — one sentence invites the next author to delete the variant as dead code. Verified
against the resolved dependency (`Cargo.lock:5176-5177` pins a single `wtransport` at **0.7.2**;
both 0.7.1 and 0.7.2 are unpacked in the registry cache, which is a known source of mis-citation):

- (a) **wtransport** exposes exactly three variants — `NotConnected`, `UnsupportedByPeer`,
  `TooLarge` (`wtransport-0.7.2/src/error.rs:210-221`), mapped at `src/driver/mod.rs:214-227`. None
  is a refusal.
- (b) **quinn's** wrapper enum has no blocked variant either — but `quinn_proto`'s does:
  `SendDatagramError::Blocked(Bytes)` at `quinn-proto-0.11.17/src/connection/datagrams.rs:243`, and
  the payload it hands back is the exact shape `WouldBlock(Bytes)` has. It is unreachable **by
  construction**, not absent: `quinn-0.11.11/src/connection.rs:442` calls
  `conn.inner.datagrams().send(data, true)` with `drop` hardcoded true, and `:448` matches the
  blocked arm as `unreachable!()`. With `drop == true`, `quinn-proto` **silently evicts the oldest
  queued datagram** instead of refusing (`datagrams.rs:28` onward). That silent eviction *is* the
  §1/§11 gap MH's own bounded egress queue exists to close. quinn does offer
  `send_datagram_wait` (`quinn-0.11.11/src/connection.rs:464`), which wtransport does not use — so
  the question is foreclosed at the wtransport layer, not at the QUIC layer. But `send_datagram_wait`
  **awaits** rather than returning a blocked error, so there is no `WouldBlock`-shaped *return*
  anywhere in the stack on any path — our variant is not a signal quinn hides, it is a shape that
  exists nowhere below us.

  The escape hatch, named because someone will look for it (@paired-media-handler): wtransport
  exposes the raw connection at `src/connection.rs:415`, `quic_connection() -> &quinn::Connection`.
  Two things stop it, and the second is the one that matters. It sits behind
  `#[cfg(feature = "quinn")]`, and we do not enable that feature — `cargo tree -e features` shows
  mh-service pulling only `default` (`self-signed`, `ring`) plus dev-only `dangerous-configuration`
  — so the hatch does not compile today and reaching it needs a manifest edit. And even then it
  sends a **QUIC** datagram *below* WebTransport, skipping the H3 session-id varint that
  `Datagram::write` prepends (`src/datagram.rs:35-48`); the result is unattributable to a session
  and dropped by a conforming peer. Taking the hatch means reimplementing H3 datagram framing inside
  MH — wire-format logic that is protocol's, not MH's. "The wrapper does not expose it" would invite
  someone to go find `quic_connection()`, which is right there with a `_mut` sibling; the true
  statement is that the only congestion-aware API sits below WebTransport's framing layer, so
  reaching it means leaving the protocol. The hatch is therefore not merely gated but
  **self-defeating**, which is a better control than a gate because it does not depend on anyone
  maintaining the gate (@security, who verified the feature set independently:
  `Cargo.toml:63` declares `wtransport = "0.7"` with no `features` key, and the only feature either
  service names anywhere is `dangerous-configuration`, dev-scoped in both).
  @paired-media-handler carries this into the trait doc, which is where someone trying to make
  `WouldBlock` production-reachable will be reading.
- (c) The variant is therefore the deterministic trip-wire for the future egress-overflow counter,
  and its only producer is the shim, **by design**. And — the clause that closes the coverage
  illusion (@security S5) — **in production the refusal is produced by MH's own bounded egress queue
  above the seam (task 16); the transport below the seam never refuses, so nothing may describe the
  egress-overflow counter, or any alert on it, as observing transport back-pressure.** Without that
  sentence a reader comes away believing the counter observes a transport condition, which is the
  exact shape ADR-0036's "A control's coverage must be demonstrated, not asserted" warns about: a
  control demonstrated against the double whose production firing path lives somewhere else. Alert
  wording is @observability's and @operations' lane and both are notified.

Stated as one layered account on purpose. "No blocked variant anywhere below us" would tell a future
author that back-pressure is fundamentally unavailable in this stack and foreclose the question;
the truth leaves open a `send_datagram_wait` path or an upstream ask, while explaining why neither
is in scope here. Versions are read from `Cargo.lock` (`:3042` quinn 0.11.11, `:3062` quinn-proto
0.11.17, `:5177` wtransport 0.7.2), not from the registry cache, which holds several unpacked
copies of each — the trap that has now mis-cited a version twice in this loop
(@observability Finding 1).

We do **not** add `writable()` / `wait_send_capacity()` to "fix" it — task 12 would have nothing to
implement them with, and dead surface in the real impl is worse than a documented one-sided variant
(@paired-media-handler 7/8).

**Retention of this one-sided variant is signed off by @code-reviewer (disposition 1), explicitly
rather than by silence** — @semantic-guard routed it there as a dead-surface / one-sided-type
question outside their five checks. The ruling, recorded so a later tidy-up sees it was decided: the
variant is not dead (the shim constructs it, task 16's match arms consume it — live across the seam's
producer/consumer pair); a trait modelling a superset of the states its concrete impls exercise is a
legitimate abstraction, and the alternative (a non-trait `refuse_next()` inspector) forces task 16's
egress-overflow gate into shim-coupled code, defeating the §10 drive-against-the-trait justification;
the rot risk is compiler-mitigated because P7 test 9 fails to compile if any variant loses its firing
path; and as a `pub` member of a `pub` enum it draws no `dead_code` lint, so **no suppression
attribute appears on it** — if one is ever needed it is `#[expect(…, reason = "…")]`, never bare
`#[allow]`.

**Keyless-MH constraint.** Payloads are opaque `Bytes` end-to-end. No associated type, method or
parameter names key material, a KEK or generation, or a parsed publisher region; no
`media-protocol` type appears in any signature. The publisher region, payload and signature stay one
uninspected blob below the seam (@security 2). Stream writes take `&[u8]` and the seam does **no**
framing — the 4-byte length prefix already has two Rust homes (`mc-service` and `mh-service`
`connection.rs`) and is double-tracked in `docs/TODO.md`; a third home in the seam would be true
duplication (@dry-reviewer D2). Framing stays above the seam.

**Ingress size bound.** `recv_datagram` returns `Bytes` — for the real impl that is
`conn.receive_datagram().await?.payload()`, itself a zero-copy slice of the ingress buffer
(`wtransport-0.7.2/src/datagram.rs:19`). The caller reads `.len()` and rejects against
`media_protocol::frame::MAX_PAYLOAD_BYTES` before any per-frame work, with no copy having occurred
(@security 4). The seam adds no allocation on either direction.

The stream side stays **send-only** — no `accept_uni`. Adding it with no implementor and no consumer
would be dead surface with its own failure mode, and task 16 is audio-datagram only. But one doc
line on the trait carries the constraint forward to whoever adds the receive half (@security S4):
*any future receive-stream method must expose the frame length before per-frame allocation, because
the ingress cap is enforced by the caller above the seam — a `read_frame() -> Vec<u8>`-shaped method
would foreclose it.* `media_protocol::codec::peek_frame_len` already exists for that case. A line
now versus a signature change two more tasks are built against later: the ADR's own ordering
argument, applied to the half of the seam we are deliberately not building.

**Zero emission.** No `tracing` / `metrics` / `log` / `println!` / span / `#[instrument]` is
reachable from the trait, its impls or the shim, and the seam takes **no metric handles as
parameters** — task 16's zero-registry-lookup decision belongs in task 16, not here. The trait's
module doc carries the same pin the client's canonical interface carries ("No metric / trace / log
emissions originate from this declaration"), so the constraint travels with the file
(@observability 3, @paired-media-handler).

**The fidelity boundary — where the shim is deliberately more forgiving than production, stated so
nobody writes a gate against a property production lacks.** This is the sharpest hazard in the task
(@dry-reviewer, Gate 1) and it gets its own doc block in both the trait and the shim.

Production **copies the datagram payload at the transport boundary, structurally**, and the
signatures say so rather than the bodies:

- `wtransport-0.7.2/src/connection.rs:297-302` — `send_datagram<D: AsRef<[u8]>>(&self, payload: D)`
  **borrows**. Handing it a `Bytes` by value transfers ownership to nothing.
- `src/driver/mod.rs:206-213` — `send_datagram(&self, session_id, payload: &[u8])`, body
  `Datagram::write(session_id, payload).into_quic_bytes()`.
- `src/datagram.rs:35-44` — `Datagram::write` allocates `vec![0; h3dgram.write_size()]` and
  `wtransport-proto-0.7.2/src/datagram.rs:50-66` then does `put_varint(qstream_id)` followed by
  `put_bytes(payload)`. **The payload is copied to prepend the WebTransport session-id varint.**
  H3 datagram framing requires that prefix, so this is structural and permanent, not incidental.

Two consequences, both written into the docs:

1. **The justification for `Bytes`-by-value on `send_datagram` is stated correctly.** The type is
   right, but *not* because the write to the wire is zero-copy — it is not. The two real
   justifications are both *above* the transport boundary: the `WouldBlock(Bytes)` hand-back needs
   ownership for requeue-without-copy, and MH's fan-out shares one ingress buffer across N egress
   payloads before any transport call. No doc in this diff will say "Bytes so the write to the wire
   is zero-copy"; that sentence is false against 0.7.2 and someone would eventually act on it.
2. **The refcount gate is scoped to the fan-out, upstream of the trait call.** A gate written as
   "hand a `Bytes` to `send_datagram`, assert sharing afterwards" passes against the shim and
   asserts a property production does not have — a test double certifying fidelity it does not
   possess, which reads as coverage and is the worst outcome available here. §10 already states the
   principle in the other direction ("scoped to the fan-out, not across the stream-read boundary
   where a copy is inherent"); the send boundary is the same case. So: assert one ingress `Bytes`
   and N egress payloads share an allocation **before** the trait call, and assert nothing about
   refcounts surviving it. This constrains task 16's gate, so it goes in the shim's doc where that
   author will read it, not only here — and that shim-side block carries a pointer to
   `docs/TODO.md` §Media Path Obligations, so the quinn-eviction / hop-gap cross-reference is
   reachable **from the code** rather than only from a file someone must know to search
   (@observability Note B).

Note P7 test 3 is not an instance of this: it asserts the payload handed back **by** `WouldBlock`
is the same allocation that went in — a property of the **shim's** contract, which task 16's
requeue tests are built on. Production never takes this path, so the assertion constrains the double
rather than describing the real impl. The earlier draft said it "is required of the real impl too",
which is unfalsifiable by (a)-(c) above and is exactly the hook a future author needs to go
"verify the real impl honours the hand-back", find it never produces `WouldBlock`, and add the
`writable()` surface this section rejects (@dry-reviewer).

Two smaller items in the same block. §10 justifies this seam partly on **benchmark
representativeness**; since Tier-2 deltas are measured through an allocation-free shim while
production pays an allocation and a copy per datagram, the doc records that the benchmark measures
the forward path *above* the seam and deliberately excludes a known per-datagram allocation below
it, so a green delta is not read as a wider claim than it is (@observability). The
layered quinn account in the `WouldBlock` block above belongs to this same concern: the shape exists
below us and is unreachable by construction, which is exactly what a future reader will try to
"wire up".

**Inline `#[cfg(test)]` module** (so the new production file is not a `test-coverage` finding):
asserts that every variant's `Display` text is **non-empty and distinct from every other variant's**,
and that `DatagramSendError::WouldBlock` yields back the same allocation through its accessor. The
`Display` strings are deliberately **incidental, not a contract** — asserting them verbatim would
red the suite for the next author who improves a message, with no way to tell whether they broke
something. Distinctness is the property worth pinning, because two variants sharing a message is
what makes a log line ambiguous. A doc line on the enums says so (@team-lead Gate-1 item 1). No
dependency on `mh-test-utils`, so no dev-cycle is needed to run unit tests.

### P2–P3. Wiring in `mh-service`

`lib.rs` gains `pub mod transport;` and one status-doc sentence. `Cargo.toml` gains exactly one
`[dev-dependencies]` line. **Zero new production dependencies** (@operations 2). No composition-root
change: nothing in `main.rs`, `server.rs` or `connection.rs` is touched, so there is no production
implementor of the trait in this diff — task 12 adds the wtransport impl and binds it at a
monomorphised call site, task 16 adds the consumers. Stating that plainly because it is the "does it
apply" answer for this task: the evidence that the seam is implementable is the API check above
against the real wtransport 0.7.2 signatures, recorded so a reviewer can verify it rather than take
it on trust.

### P4–P6. The shim — new crate `crates/mh-test-utils/`

**Location, and why.** A new crate whose only edge into `mh-service` is a `[dev-dependencies]` one.
This is @security's and @operations' first-ranked option and it satisfies @observability 5: a
criterion bench target (§10 Tier 2) can depend on it, which `crates/mh-service/tests/common/` cannot.
It follows three existing precedents — `ac-test-utils`, `gc-test-utils`, `mc-test-utils` all depend
on their service crate and are dev-depended on by it. Chosen **over** the `common`-style `test-utils`
cargo feature that @paired-media-handler suggested, because cargo features are additive: `--all-
features`, or any future non-dev edge, re-enables a lossy simulator inside a release build, which
@operations names as the top operational risk here.

**The control claim, stated at its true strength and no higher** (@security S1 — this plan refuses
"Bytes so the write to the wire is zero-copy" for the same reason and must hold itself to it): a
separate crate does **not** make substitution impossible. What is true is that while the edge stays
in `[dev-dependencies]`, the compiler forbids production code in `mh-service` from importing
`mh_test_utils` at all, so substituting the shim requires a visible one-line manifest move out of
`[dev-dependencies]` — a reviewable source edit, not a build-flag flip. That is strictly stronger
than the feature option, where `--all-features` needs no source edit at all, which is why option (a)
still wins. But it is one line away, not impossible, and both this plan and the `mh-test-utils`
crate doc say so in those words. Nothing `pub`-exports the shim from `mh-service`, and no production
generic defaults to it (@semantic-guard 1, @operations 1).

**The release-artifact path has three independent structural barriers**, traced by @operations and
re-verified here against `infra/docker/mh-service/Dockerfile`: the builder stage runs
`cargo chef cook --release … --package mh-service` (`:53`) and `cargo build --release --package
mh-service` (`:59`) — both **package-scoped, not `--workspace`** — cargo does not build
dev-dependencies for a non-test profile at all, and the distroless stage copies only the stripped
`/build/target/release/mh-service` binary (`:78`, `:135`). So adding `crates/mh-test-utils` to
`[workspace] members` does not put it in the release image even incidentally. Substituting the shim
into production therefore needs the manifest move **plus** a production `use mh_test_utils::…` —
**two** visible source edits in the same diff, not one. That sentence goes in the crate doc beside
the caveat, because the caveat alone understates the control in the other direction. Non-blocking
side effect, recorded so a slow build is not misread as a regression: the planner stage does
`COPY . .` then `cargo chef prepare` (`:42`), so a new workspace member changes `recipe.json` and
busts the MH image's cached dependency layer once on the next build.

**`mh-test-utils` is different in kind from its three siblings**, which is why the edge deserves a
mechanical check rather than review vigilance. `ac-`/`gc-`/`mc-test-utils` are assertion helpers and
mocks of external services; none implements a production trait that production code is generic over.
This one is the first test-utils crate that is a drop-in for a production hot-path type — a lossy,
delaying, capacity-refusing implementation of `MediaTransport`. That elevation is recorded in the
crate doc so the next reader knows why the edge matters here more than it does next door.

**The closer precedent is not a test-utils crate at all** (@security): wtransport's
`dangerous-configuration` — its certificate-validation-bypass feature — is already
dev-dependency-scoped in both `crates/mh-service/Cargo.toml:78` and `crates/mc-service/Cargo.toml:84`.
That is the repo's existing posture for *a capability that must never reach a release build*, and it
is a nearer analogue than the three test-utils crates, because like the shim it is a capability
rather than a helper. Recorded in the spun-out guard's `docs/TODO.md` entry as the **second member
of the class**, so whoever writes that guard designs for the class (a dev-only feature migrating
into `[dependencies]` is the same defect as a `*-test-utils` path dep doing so) rather than for one
instance and having to widen it later.

`crates/mh-test-utils/src/transport_shim.rs` opens with a **file-scoped**
`#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]`
(@code-reviewer). The crate does not take `[lints] workspace = true` — that would break the sibling
precedent and force `#[expect]` noise on legitimate future assertion helpers elsewhere in the crate
— but the shim itself is the code that drives the zero-syscall benchmark path, so its no-panic
posture gets the same structural enforcement the rest of this plan prefers over convention
(dyn-incompatibility via RPITIT, zero syscalls via the no-I/O-driver runtime, drop-path coverage via
exhaustive match). A genuine future `unwrap` there surfaces as `#[expect(..., reason = "...")]`
rather than passing silently.

`crates/mh-test-utils/src/lib.rs` opens with **`#![forbid(unsafe_code)]`** (@security S3). The
siblings deliberately do not inherit workspace lints so they can `unwrap`/`panic` in assertions, and
`unsafe_code` is not in `[workspace.lints.rust]` either — so opting out would otherwise leave this
crate with no `unsafe` bar at all. A test double whose entire job is to certify a *memory* property
(pointer identity, no-copy hand-back) is the likeliest place in the tree for someone to reach for
`unsafe` to make a zero-copy assertion pass; `forbid` makes the shim structurally unable to fake the
property it exists to demonstrate. P7 test 3 uses safe `as_ptr()`, so this costs nothing today.

**`LossDelayTransport`** — cloneable handle over an `Arc` of shared state; knobs are settable through
`&self` after the shim has been handed to a forwarder (@paired-media-handler 10). Counters and knobs
are **atomics**, so the send path is lock-free and total; the two capture sinks use a `Mutex`
recovered through `PoisonError::into_inner` rather than `unwrap`, so nothing panics on knob misuse
even though this crate follows the sibling precedent of not inheriting the workspace deny lints.

Knobs (all deterministic — no RNG, seeded or otherwise; @test 1):

- `set_egress_datagram_loss(DatagramLoss)` and `set_ingress_datagram_loss(DatagramLoss)`, where
  `DatagramLoss` is `None | Next(u32) | EveryNth(NonZeroU32) | All`. **Two separately-named methods,
  not one method taking a direction enum** — a direction enum in this crate would be a second home
  for the direction label vocabulary whose SSoT is `docs/observability/metrics/mh-service.md`, and
  task 16 maps to labels at the emission site. Direction matters because §2 makes the hop sequence
  **bidirectional**: "it applies to the client's uplink as well as MH's downlink, so each side can
  detect loss on the hop it receives" (`docs/decisions/adr-0036-media-flow.md:183-189`). So:
  **egress** loss is observable only by the client (MH writes its own downlink hop sequence and
  cannot see a gap in its own output) and fires no MH counter — it is asserted on the shim's
  delivered sink; **ingress** loss is upstream of MH, is what MH's own uplink-gap detector reads,
  and is the *only* firing path for that future counter. The two are not interchangeable with burst
  injection, which trips MH's bounded inbound queue (its own refusal) rather than a gap. Corrected
  from an earlier `stream_sequence` framing: that field is the AEAD nonce input and selection is
  intentional gapping, so a gap there would measure forwarding policy rather than loss
  (§2, line 188).
- `set_send_capacity(n)` / `clear_send_capacity()`.
- `set_added_delay(Duration)` — applied to datagram *delivery* (a datagram injected at virtual `t`
  becomes readable at `t + d`) and to `write_all` / `finish` completion, which is the slow-subscriber
  mechanism §10's forward-path gate needs.
- `set_max_datagram_size(Option<usize>)`, `set_closed()` and `set_datagrams_unsupported()` — so
  **every variant of both error enums
  is reachable through the shim**, asserted by an exhaustive-match test. This is the seam's stated
  §10 justification applied to itself.

Knobs are per-shim-instance, never keyed by stream id or participant: a knob that targets loss at a
named stream is the shape that invites a per-stream metric label later, and §11 is explicit that the
time-ordered per-stream size sequence is the voice-activity trace (@observability 7).

`send_datagram` order of operations, which is what keeps the two knobs structurally distinct
(@observability 1):

1. `set_closed` armed → `Err(ConnectionClosed)`.
2. payload longer than `max_datagram_size` → `Err(TooLarge)`.
3. remaining capacity zero → `refused_for_capacity += 1`, `Err(WouldBlock(payload))` — the **same**
   `Bytes`, moved, never cloned into a new buffer.
4. capacity decremented, `accepted += 1`; loss schedule consulted on the accepted index → if lost,
   `egress_dropped_in_flight += 1` and return **`Ok(())`** with the payload never reaching the sink.
5. otherwise `delivered += 1` and push the `Bytes` (refcount bump, no copy) into the sink.

Three separate counters, never one shared `drops`: `egress_dropped_in_flight()`,
`ingress_dropped()` and `refused_for_capacity()`, plus `delivered_datagrams() -> Vec<Bytes>` (not `Vec<u8>`, so task 16's
refcount fan-out gate is writable), `delivered_count()`, `finished_uni_streams()`,
`open_uni_streams()`, `max_concurrent_open_uni()`.

Ingress: `push_inbound(Bytes) -> Result<(), Bytes>` over a **bounded** `tokio::sync::mpsc` channel
sized at construction — pre-allocated, so burst injection costs no per-frame allocation, and a full
queue hands the payload back and fails loudly instead of silently dropping. Burst injection to
overrun task 16's bounded inbound queue is therefore just repeated `push_inbound` while the consumer
is delayed (@paired-media-handler 10).

**Bounded capture** (@security 3): the delivered sink is a `Vec::with_capacity(limit)` fixed at
construction, so it neither grows mid-measurement nor retains unboundedly. Past the limit the shim
stops capturing but keeps counting and sets a `capture_truncated()` flag, so a test cannot silently
assert against a truncated capture. Capture holds only payload bytes — no per-frame provenance, no
connection or participant identity, nothing on the production trait.

**Explicitly NOT built here** (@test 7, @paired-media-handler 9): no bounded egress queue, no
drop-oldest policy, no ingress or egress loop. Those are task 16's, in `crates/mh-service/src/
media/`; building a queue here would be the second abstraction task 12's prompt forbids. The
reachability assertions below need no queue.

### P7. The reachability suite — `crates/mh-service/tests/transport_seam_reachability.rs`

Assertions are on **shim-observable outcomes, not counter deltas** — neither MH counter
(hop-sequence gap, egress overflow) exists yet, and nothing in this diff names them as if they do
(@test 3, @observability 6, @operations 4). The tests drive a generic
`async fn drive<T: MediaTransport>(…)`, which is what exercises monomorphisation.

1. `datagram_send_drop_is_deterministically_reachable` — `EveryNth(2)`, four sends: all return
   `Ok(())`; the delivered sink holds exactly payloads 1 and 3; `egress_dropped_in_flight() == 2`;
   `refused_for_capacity() == 0`.
2. `ingress_datagram_loss_is_deterministically_reachable` — `set_ingress_datagram_loss(Next(1))`,
   three `push_inbound` calls, three `recv_datagram().await`s would block after two: the second and
   third injected payloads surface, the first never does, `ingress_dropped()` is 1, and no send-side
   counter moves. This is the only firing path for MH's future uplink hop-sequence gap counter, so
   it gets its own test rather than riding on the egress one.
3. `backpressure_refusal_hands_back_the_same_allocation` — `set_send_capacity(2)`, three sends: two
   `Ok`, the third `Err(WouldBlock(b))` with `b.as_ptr() == original.as_ptr()` and `b.len() ==
   original.len()`. Pointer identity, not byte equality — byte equality passes against an impl that
   copied (@test 5, @paired-media-handler 9).
4. `the_two_drop_paths_are_distinguishable` — the cross-zero assertion: under loss,
   `refused_for_capacity() == 0`; under capacity, `egress_dropped_in_flight() == 0`. This is the test that
   makes the two knobs usable as two independent counter triggers later (@observability 1).
5. `added_delay_is_released_by_virtual_time` — `#[tokio::test(start_paused = true)]`; assert the
   elapsed `tokio::time::Instant` across the `recv_datagram().await` equals the configured delay
   **exactly**. Exact equality is only possible under virtual time, so the assertion also proves the
   clock is virtual.
6. `added_delay_requires_explicit_advance` — the receive is spawned, the main task yields, the handle
   is asserted **not** finished, then `tokio::time::advance(delay).await` releases it. Answering
   @test 2 directly: test 5 is auto-advance-tolerant and test 6 drives `advance` explicitly; we ship
   both so neither mechanism is mistaken for the other.
7. `forward_path_needs_no_io_driver` — the zero-syscall gate, structural rather than prose (@test 4).
   The runtime is built by hand as `Builder::new_current_thread().enable_time().start_paused(true)`
   — **no I/O driver at all**. Inject, receive, rewrite, send, inspect. Any attempt at real network
   I/O on that runtime panics with "there is no reactor running", so the test cannot pass unless the
   path is syscall-free. This is a compile-and-run proof, not an assertion of intent.
8. `uni_stream_open_write_finish_round_trips` — open, two `write_all`s, `finish`; the captured chunks
   match and `max_concurrent_open_uni() == 1`, which is the observation §10's "concurrent unfinished
   groups stay at or below the application cap" gate will read.
9. `every_transport_error_variant_is_shim_reachable` — exhaustive match over both enums, each arm
   reached by a knob. A new variant added later without a firing path fails to compile.

Guard visibility (@test 8): the file sits at `crates/mh-service/tests/*.rs`, so it is a
first-class cargo test target, not a subdirectory needing `#[path]` registration; `test-registration`
therefore has nothing to flag. `test-rigidity` is scoped to `crates/env-tests/tests/` and does not
apply, but the suite has no early returns, no warning-as-assertion and no assertion-free arms
regardless.

### P8. Recorded, not fixed

One `docs/TODO.md` entry under §Test Debt, naming both siblings specifically rather than gesturing
at "the SDK test double needs work" (@team-lead Gate-1 item 2): (i) `packages/test-utils/src/
MockWebTransport.ts` has `simulateIncomingDatagram` and `getOutboundDatagrams` but **no
adverse-condition knob** — no loss, no send capacity, no delay — so §11's SDK sender-side drop
counter will ship with the identical reachability gap this task closes for MH; (ii)
`MockWebTransport.ts:234` does `out.set(chunk)`, copying on capture, so the TS side cannot assert
zero-copy either and the cross-language claim is asymmetric in a way neither side documents.
Client-owned; naming it is the deliverable, not fixing it. The entry cites @dry-reviewer's
§Cross-Service Duplication entry by section heading and theirs cites this one — two limbs of one
fact (the Rust and TS halves of one mechanism diverging with no compiler between them), and two
entries that do not cite each other is how a reader fixes one and never learns the other exists.

**Spun out, not deferred (@security S2)** — a second `docs/TODO.md` entry under §Test Debt for the
dependency-edge guard: fail if any `*-test-utils` path dependency appears outside a
`[dev-dependencies]` section in any workspace manifest, naming all four crates (`ac-`, `gc-`, `mc-`,
`mh-test-utils`) and the rule.

**Scoped correctly, because a wrong stated cost is what makes a TODO permanent.** An earlier draft
justified the spin-out with "ADR-0034 means this is a dt-guard module plus registration plus wrapper
plus self-test plus layer wiring". That premise is false and @security checked it: ADR-0034 was
accepted 2026-05-18, and five pure-bash guards in `scripts/guards/simple/` invoke no dt-guard
subcommand and were all created *after* that date — `audit-suppressions.sh` (24 LoC),
`selftest-gate2-verdict.sh` (499), `validate-story-manifest.sh` (22),
`validate-subdomain-regex-sync.sh` (270), `validate-slug-class-sync.sh` (104). The last two are the
same species (cross-file drift/sync checks) and the subdomain one was modified two commits ago on
this branch (`28defd8`). ADR-0034 collapsed guards that *had* dt-guard subcommands into thin
wrappers; it did not close the door on a small bash drift check. So the real shape is **one bash
guard in the established post-ADR-0034 idiom plus one layer registration, ~100 LoC, copying
`validate-slug-class-sync.sh`** — and it must carry a pinned expected count of test-utils dev-dep
edges so it cannot pass vacuously when a crate is renamed, which is the anti-vacuity bar
`validate-subdomain-regex-sync.sh`'s header sets and the reason it is ~100 LoC rather than ~20.

The invariant **holds today with zero violations** — @operations scanned every workspace manifest
for a `*-test-utils` path dependency outside `[dev-dependencies]` and found none (`ac`, `gc`, `mc`
all dev-scoped; the only other hit is `common`'s `[features] test-utils = [...]`, a feature
definition rather than a dependency edge). So this is a pure **regression** guard that lands green
on day one with no cleanup attached, and the `docs/TODO.md` entry records that baseline explicitly —
a guard whose implementer expects it to land red is a guard that gets an exemption on day one. The
entry states the rule in matcher-unambiguous form: *a `*-test-utils` path dependency in any
workspace manifest must appear only under `[dev-dependencies]`.*

The spin-out stands on the reasons that survive that correction: it is a **four-crate class fix**
whose other three owning specialists are not in this loop and whose manifests it constrains equally;
and the compiler already closes the accidental path, so this is defence-in-depth against a
deliberate, visible, one-line manifest edit — real and worth having, not load-bearing today. The
`docs/TODO.md` entry carries this scoping, not the ADR-0034 one, for the same reason fact (c) in the
`WouldBlock` block was corrected: a stated reason that misdirects scrutiny is worse than no reason.

**Not duplicated**: the quinn silent-eviction thread is already recorded by @observability under
§Media Path Obligations, more completely than this loop would have written it (eviction blindness,
ADR-0036 §1's `datagram_send_buffer_size` sizing remedy in frames of audio, the hop-sequence-gap
counter as the compensating far-end control, @security's S5 naming constraint, and operations named
as owner of task 21's runbook caveat). Nothing further is owed from this loop on it.

**Third entry, authored by @code-reviewer, landed by test** (§Documentation Hygiene): ADR-0036 §10
Tier 1a's "All deadline logic takes an injected clock" reads as mandating a bespoke `Clock` trait
that the workspace deliberately avoids, with the four pause/advance sites named and a proposed
reword. Ownership stays with @code-reviewer — they author the wording and verify it at review — and
test lands it because test is already editing this file; that arrangement is what stops it falling
between the two of us. Editing a shipped ADR is its own governance judgment and does not belong in
this changeset, which is why it is an entry rather than an edit.

**`docs/TODO.md` is a shared file this loop.** @observability has already appended an entry under
§Media Path Obligations (the quinn silent-eviction blind spot on MH's future egress-overflow
counter, with the far-end hop-sequence gap counter as the compensating control) and @dry-reviewer
under §Cross-Service Duplication. Different sections,
no textual conflict — but the file is re-read immediately before editing rather than written from a
stale copy, and the classification row records it as shared.

### P9. The one ADR tension, surfaced rather than stepped around

ADR-0036 §10 Tier 1a says "All deadline logic takes an injected clock." The task instruction says
the opposite in mechanism terms: no Clock trait, delay via `tokio::time::sleep` released by
`tokio::time::advance` under `start_paused`. We follow the task instruction, for three reasons.
First, the ADR sentence names a *property* — deadlines must be drivable from a test — and tokio's
paused timer is an injected clock: the test runtime substitutes virtual time for the real one at the
runtime boundary rather than at a trait boundary. Second, there is no `Clock` trait anywhere in the
workspace; the established idiom is pause/advance, at `crates/mc-service/src/actors/meeting.rs`,
`crates/mh-service/src/webtransport/connection.rs` and two mc-service integration suites. A Clock
trait would be a parallel mechanism to a working one. Third, task 16's own prompt already resolves
the tension the same way and in the same words. Flagged here so @code-reviewer sees it was decided,
not missed (@dry-reviewer D3).

### P10. Mirroring claim, stated precisely

The task text cites `packages/sdk-core/src/transport/types.ts`; that file holds only
`WebTransportConnectOptions` and the dev cert-pinning types. The canonical contract is
`packages/sdk-core/src/transport/IWebTransport.ts`, and that is what we mirror. The mirror is
**conceptual, not member-level**: `IWebTransport` is `ready` / `closed` / `datagrams {readable,
writable}` / `createBidirectionalStream()` / `close()` — Web Streams and *bidirectional* streams;
ours is send-datagram / recv-datagram / open-*uni* / write / finish. Same idea — per-connection I/O
behind an interface with a deterministic double — in two idioms. Saying so keeps anyone from later
building a cross-language parity test against a parity that was never claimed (@dry-reviewer D4,
@code-reviewer 2).

### P11. File ownership split with @paired-media-handler

@paired-media-handler authors `crates/mh-service/src/transport/mod.rs` (P1) and makes the three-line
`lib.rs` / `Cargo.toml` edits (P2, P3) — their domain, and their acceptance of the consumer-shapes-
the-seam offer. test authors the `mh-test-utils` crate (P4–P6), the reachability suite (P7), the
`docs/TODO.md` entry (P8) and this document. Neither touches the other's files.

---

## Pre-Work

None.

---

## Implementation Summary

### What landed

The ADR-0036 §10 transport seam, a deterministic test double for it, and a reachability suite
proving every drop path fires on demand. No ingress or egress loop, no bounded egress queue, no
production implementor of the trait — those are tasks 12 and 16.

**The seam** (`crates/mh-service/src/transport/mod.rs`, authored by @paired-media-handler): four
per-connection operations — `send_datagram` (sync, because `wtransport::Connection::send_datagram`
is `pub fn` not `async fn`), `recv_datagram`, `open_uni`, and `MediaSendStream::{write_all, finish}`.
RPITIT with an explicit `+ Send` rather than bare async-fn-in-trait: @paired-media-handler compiled
both, and the bare form compiles fine in *this* task and fails only at the forward-path task's
`tokio::spawn` of a generic consumer. Two flat error enums,
`DatagramSendError::{WouldBlock(Bytes), TooLarge, ConnectionClosed, DatagramsUnsupported}` and
`TransportError::{ConnectionClosed, StreamClosed}`, neither `#[non_exhaustive]`.

**The shim** (`crates/mh-test-utils/`, a new dev-dependency-only crate): `LossDelayTransport`, a
cloneable handle over atomics with six knobs — egress loss, ingress loss, send capacity, added
delay, max datagram size, closed, datagrams-unsupported — bounded ingress injection that hands the
payload back when full, and bounded capture that keeps counting past its limit while flagging
truncation.

**The suite** (`crates/mh-service/tests/transport_seam_reachability.rs`): 16 tests, all green, all
in **0.00s** of wall time because every delay is virtual.

### The adjudicated deviation from the literal task text

@team-lead adjudicated at Gate 1 and the reasoning is recorded here rather than only in the thread.
The task text says "Payloads are `Bytes` so the refcount gate can assert zero-copy." The seam uses
`Bytes` by value on the datagram path and `&[u8]` on the stream write path. **Approved**: the
asymmetry is in the upstream API, not a style choice — `wtransport::SendStream::write_all` takes
`&[u8]` — and ownership transfer is needed only where the datagram path needs it, for the
`WouldBlock` hand-back and the fan-out refcount gate. Forcing `Bytes` on the stream path would make
callers allocate for the length prefix, which is the framing that must stay above the seam. The
refcount gate the task text protects lives on the datagram path and is unaffected.

### Both drop paths, and a third the review round added

The task named two knobs. The review round established there are three distinct outcomes, because
ADR-0036 §2 makes the hop sequence **bidirectional** — "it applies to the client's uplink as well as
MH's downlink, so each side can detect loss on the hop it receives" (§2, lines 183-189). MH writes
its own downlink hop sequence and structurally cannot see a gap in its own output, so **egress loss
fires no MH counter at all** and is asserted on the shim's delivered sink; **ingress loss** is the
only firing path for MH's own uplink gap detector; and **capacity refusal** is neither. Three
counters, three accessors, never one shared `drops`, and
`the_three_drop_paths_are_distinguishable` asserts each scenario moves its own counter and leaves
the other two at zero. Had the knob shipped in one direction only — the inversion originally
proposed — MH's own gap counter would have had no firing path, which is the exact condition this
seam exists to prevent.

### The two assertions that carry the deliverable, both verified as traps

Neither is a tautology; both were confirmed by deliberate mutation before being trusted.

**Pointer identity on the `WouldBlock` hand-back.** Byte equality would pass against an
implementation that copied, which is the whole property under test. Mutating the shim to return
`Bytes::copy_from_slice(&payload)` fails the assertion with "WouldBlock must hand back the same
allocation, not a copy". Restored and re-verified green.

**Zero syscalls, proved structurally rather than asserted.** `forward_path_needs_no_io_driver`
builds its runtime by hand as `Builder::new_current_thread().enable_time().start_paused(true)` —
**no I/O driver at all** — and drives inject → receive → relay-rewrite → send → inspect on it.
Inserting a `tokio::net::UdpSocket::bind` into that runtime panics at `tokio/src/net/udp.rs:171`, so
the test cannot pass unless the path is genuinely syscall-free. Restored and re-verified green.

### The fidelity boundary

The highest-value artifact in this diff, and it lives in the shim's module doc and the trait's doc,
not only here, because the person who needs it is arriving cold at the forward-path task.
**Production copies the datagram payload structurally.** `wtransport-0.7.2/src/connection.rs:297-302`
borrows via `AsRef<[u8]>`; `src/driver/mod.rs:206-213` takes `&[u8]`; `src/datagram.rs:35-44`
allocates `vec![0; write_size]` and `wtransport-proto-0.7.2/src/datagram.rs:50-66` copies the
payload in after the session-id varint. H3 framing requires that prefix, so the copy is permanent.
Two consequences are written where they will be read: no doc claims `Bytes` makes the wire write
zero-copy, and the refcount gate is scoped to the fan-out **upstream** of the trait call, because a
gate asserting refcount survival *through* `send_datagram` passes against the shim while asserting a
property production does not have.

### `WouldBlock` has no production producer, recorded as five facts

Verified against the locked versions (`Cargo.lock`: wtransport 0.7.2, quinn 0.11.11, quinn-proto
0.11.17). wtransport surfaces three non-refusal variants. `quinn_proto::SendDatagramError::Blocked(Bytes)`
exists at `quinn-proto-0.11.17/src/connection/datagrams.rs:243` and is structurally this exact shape,
but is unreachable by construction: `quinn-0.11.11/src/connection.rs:442` hardcodes `drop: true` and
`:448` matches the blocked arm as `unreachable!()`, so quinn silently evicts the oldest queued
datagram instead of refusing. `send_datagram_wait` (`:464`) is congestion-aware but *awaits* rather
than returning a blocked error, so no `WouldBlock`-shaped return exists anywhere in the stack. The
escape hatch someone will look for — `quic_connection()` at `src/connection.rs:415` — is behind a
`quinn` feature this workspace does not enable and would skip the H3 varint anyway, making it
self-defeating rather than merely gated. And per @security S5: **in production the refusal is
produced by MH's own bounded egress queue above the seam; the transport below never refuses, so
nothing may describe the egress-overflow counter or its alert as observing transport back-pressure.**
@code-reviewer signed off on retaining the variant (disposition 1) explicitly rather than by silence.

### Controls stated at their true strength

Two claims were weakened during review and the weakened versions are what shipped, in the code and
not only in this document. `crates/mh-test-utils/src/lib.rs` says a separate crate does **not** make
substitution impossible — what is true is that the compiler forbids production `mh-service` code
from importing `mh_test_utils` while the edge stays in `[dev-dependencies]`, so substitution is a
visible one-line manifest move rather than a build-flag flip. The same doc then records the three
Dockerfile barriers @operations traced (`--package`-scoped builds at lines 53 and 59, cargo not
building dev-dependencies for a non-test profile, and only the stripped binary copied at lines 78
and 135), so the caveat does not understate the control either: substitution needs the manifest move
**plus** a production `use` — two visible source edits in one diff.

### Scope — what is deliberately not in this diff

Recorded here because it is task-definition scope, not tracked debt. The `docs/TODO.md` pointer
for every finding routed out of this loop lives in §Accepted Deferrals, one line each; this section
is only the things deliberately not built.

**Out of scope by the task definition, not deferred.** The datagram ingress and egress loops, MH's
bounded egress queue and its drop-oldest policy, and the real wtransport implementation of the seam
all belong to tasks 12 and 16. Building any of them here would have been the second abstraction task
12's prompt forbids, and none of the reachability assertions needs one. Listed affirmatively because
a reviewer will look for them and should find them decided rather than absent.

**`accept_uni` is a deliberate non-goal.** The stream side of the seam is send-only —
open / write / finish — because this story is audio-datagram only. Adding a receive half with no
implementor and no consumer would be dead surface. The constraint is carried forward instead: a doc
line on the trait records that any future receive-stream method must expose the frame length before
per-frame allocation, since the ingress cap is enforced by the caller above the seam and a
`read_frame() -> Vec<u8>`-shaped signature would foreclose it (@security S4).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/mh-service/src/transport/mod.rs` | NEW (@paired-media-handler) — the seam: two traits, two flat error enums, `into_payload`, the contract doc including the fidelity boundary and the five-clause `WouldBlock` block, inline `#[cfg(test)]` unit module |
| `crates/mh-service/src/lib.rs` | (@paired-media-handler) `pub mod transport;`; status paragraph; stale `applied_generation` task pointer corrected from task 5 to task 11 |
| `crates/mh-service/Cargo.toml` | (@paired-media-handler) one `[dev-dependencies]` line: `mh-test-utils` |
| `crates/mh-service/tests/transport_seam_reachability.rs` | NEW — 16-test reachability suite; both key assertions mutation-verified as traps |
| `crates/mh-test-utils/Cargo.toml` | NEW — crate manifest; deps `tokio`, `bytes`, `mh-service`; no workspace lints, per sibling precedent |
| `crates/mh-test-utils/src/lib.rs` | NEW — crate doc (different-in-kind rationale, true-strength control statement, three Dockerfile barriers), `#![forbid(unsafe_code)]` |
| `crates/mh-test-utils/src/transport_shim.rs` | NEW — `LossDelayTransport`, `ShimSendStream`, `DatagramLoss`, `ShimConfig`, `FinishedUniStream`; file-scoped `#![deny(unwrap_used, expect_used, panic, indexing_slicing)]`; module doc carries the fidelity boundary and the ingress/egress direction rationale |
| `Cargo.toml` | One `members` line: `crates/mh-test-utils` |
| `docs/TODO.md` | SHARED. Test's three: two under §Test Debt (`MockWebTransport` adverse-condition-knob gap; the four-crate test-utils dev-dep-edge guard), one under §Documentation Hygiene authored by @code-reviewer (ADR-0036 §10 injected-clock wording). @observability's two under §Observability Debt and §Media Path Obligations preserved |
| `docs/devloop-outputs/2026-09-01-mh-transport-seam/main.md` | This file |

---

## Devloop Verification Steps

| Step | Result |
|------|--------|
| `cargo check -p mh-test-utils` | clean |
| `cargo clippy -p mh-test-utils -p mh-service --all-targets` | 0 warnings, 0 errors |
| `cargo fmt -p mh-test-utils -p mh-service -- --check` | clean |
| `cargo test -p mh-service --test transport_seam_reachability` | **16 passed, 0 failed, 0.00s** |
| Trap check 1 — shim mutated to copy on the `WouldBlock` hand-back | assertion FAILS as required; restored, re-verified green |
| Trap check 2 — `UdpSocket::bind` inserted into the no-I/O-driver runtime | panics at `tokio/src/net/udp.rs:171` as required; restored, re-verified green |
| `cargo clippy --workspace --all-targets -- -D warnings` (as Layer 5 runs it) | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `scripts/layer-all.sh` | **PASSED** — @team-lead's authoritative Gate-2 run; figures below |

### Gate 2 — authoritative run (@team-lead)

`PIPELINE_MODE=run-all SOURCE=headless`, run against the frozen tree. **`LAYER_ALL_EXIT=0`.**

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 | OK | 6 |
| 2 | OK | 1 |
| 3 | OK | 20 |
| 4 | N/A | 180 |
| 5 | OK | 2 |
| 6 | N/A | 1 |
| 7 | OK | 363 |
| **TOTAL** | **N/A** | **573** |

**`TOTAL_RESULT=N/A` is a pass**, not a gap. N/A outranks OK in the aggregation, so a single N/A
layer carries the total. Both N/A layers are the documented self-justifying case: Layer 4 is proto's
intentional-gap placeholder (`not-applicable-to-this-lang` then `test-aggregate-na`) with
`cargo-test-passed` and `nx-test-passed` OK beneath it; Layer 6 is the TypeScript lane's
`SKIPPED-NO-DIFF no-dep-changes` with `cargo-audit-passed` OK. **No layer FAILed and no layer was
NOT-RUN** — every one was evaluated, which is the distinction that matters, since NOT-RUN would mean
unmeasured rather than clean. Layer 7 ran in full: `env-tests-passed` and `browser-e2e-passed`. 36
`STATUS=OK` lines total. One `WARN BUDGET_BREACH LAYER=4 DURATION=180 BUDGET=20` — the TypeScript
lane spending three minutes determining it has nothing to do; not a failure and not diff-caused.

**The figure that matters most is not in that table.** The run was executed under a checksum guard
over all ten changeset files, which reported **`ALL FILES UNCHANGED DURING RUN`**. This green is
therefore bound to a tree that provably did not move while it was being measured. That is the
property the two preceding greens lacked, and it is why both were void — the run before this one
also exited 0, and its checksum guard is the only reason we know that result was meaningless. **The
guard is the record, not merely the result.**

**Attempt accounting**, because the count looks worse than it was: **one** gate failure across the
whole loop (attempt 1, Layer 3 `validate-todo-tracking`). Every subsequent run was forced by the tree
moving under a run, not by a gate rejecting the work. The three-attempt budget bounds fix-fail-fix
cycles and exactly one such cycle occurred, so this is not an exhausted-attempts escalation.

### Implementer verification (test), by exit code

Every check below reads an **unpiped** exit status. This matters: the loop's first Layer-5 failure
was caused by a check of mine that could not fail (see §Issues Encountered), so status-by-`$?`
without an intervening pipe is the standard every figure here is held to.

| Check | Result |
|-------|--------|
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo check --tests -p mh-service` | exit 0 |
| `cargo test -p mh-service --test transport_seam_reachability` | 17 passed, 0 failed, 0.00s |
| Trap 1 — shim mutated to `copy_from_slice` on the `WouldBlock` path | pointer assertion FAILS as required; reverted, byte-identical |
| Trap 2 — `UdpSocket::bind` in the no-I/O-driver runtime | panics at `tokio/src/net/udp.rs:171`; reverted, byte-identical |
| Trap 3 — retention bound reverted to unbounded `history.push` | bound test FAILS as required; reverted, byte-identical |

### Implementer verification (media-handler), pipe-free

Reported deliberately pipe-free — each command redirected with `> f 2>&1` and its status read
directly, so no `$?` passes through `grep`, `sed`, `tail` or a colouriser.

| Check | Result |
|-------|--------|
| `crates/mh-service/src/transport/mod.rs` | 471 lines |
| `crates/mh-service/src/lib.rs` | 41 lines |
| `crates/mh-service/Cargo.toml` | 3861 bytes |
| clippy / lib tests / reachability / fmt | exit 0 / 0 / 0 / 0 |
| test counts | 7 passed (lib), 17 passed (reachability) |

**One planned check was deliberately NOT run.** I had committed to re-running the two
cross-boundary guards and `todo-tracking` pipe-free via `${PIPESTATUS[0]}` so their figures would be
status-verified rather than content-verified. @team-lead's instruction accompanying the Gate-2 pass
was to make this edit and run nothing, because a main.md-only edit demonstrably reds Layer 3
(attempt 1) and they re-run the full pipeline afterwards. Recorded as skipped-by-instruction rather
than silently dropped: the guard figures in this document are **content-verified** (the guards'
own `STATUS=` output contract) and not status-verified, and @team-lead's post-edit pipeline run is
the authority over both.

---

**`Send` is enforced, not merely declared.** Converting the shim's three RPITIT bodies to `async fn`
(Layer-5 fix below) could in principle have dropped the `+ Send` guarantee silently. It does not, and
the suite proves it rather than asserting it: `added_delay_requires_explicit_advance` and
`closing_wakes_a_parked_receiver` both `tokio::spawn` a future returned by `recv_datagram`, and
`tokio::spawn` requires `Send`. If the impl's future stopped being `Send`, those two tests would fail
to compile.

---

## Code Review Results

Gate 3: all eight verdicts in, none ESCALATED.

| Reviewer | Verdict | Detail |
|----------|---------|--------|
| Security | RESOLVED-DEFERRED | 1 finding (overstated control claim, `crates/mh-service/Cargo.toml:60-68`), fixed. 1 accepted spin-out — S2 test-utils dev-dep-edge guard (§Accepted Deferrals). |
| Test | CLEAR | No findings. Eight Gate-1 criteria verified against code; independently mutation-verified the pointer-identity trap. |
| Observability | RESOLVED-FIXED | 2 findings, both fixed — the fidelity block's missing §Media Path Obligations pointer, and `stream_id` → `open_ordinal`. |
| Code Quality | RESOLVED-FIXED | 1 finding (redundant bare `#[allow]`, `transport/mod.rs:407`), fixed by deletion, not conversion. |
| DRY | RESOLVED-DEFERRED | All findings fixed; deferral is the §Cross-Service Duplication extraction. Ruling below. |
| Operations | RESOLVED-FIXED | 1 finding (`finished_uni_streams` unbounded against a doc claiming it was bounded), fixed with a loud truncation flag and a trap-verified test. |
| Semantic Guard | CLEAR | Native SAFE. Five checks run; out-of-lens scope stated explicitly rather than padded. |
| Paired media-handler | RESOLVED-DEFERRED | 5 findings raised, 5 fixed, 0 escalated, 1 suspicion explicitly withdrawn. Ownership Lens labelled per row. |

**DRY Gate-3 ruling (@team-lead).** Left RESOLVED-DEFERRED rather than overridden down to CLEAR: the
move available at the last gate — reading a rosier verdict off a taxonomy technicality — is exactly
what the verdict split exists to prevent. @dry-reviewer surfaced the conflict rather than resolving it
in their own favour; it proved to be five statements of the verdict rule across two severity models,
with `.claude/agents/dry-reviewer.md` instructing the reviewer in a superseded taxonomy while
`review-protocol.md` grades them in the current one. Carried in `docs/TODO.md` §Documentation Hygiene.

**Paired media-handler — independent-review follow-ups.** As co-implementer, they named four items on
their own authored rows that a genuinely independent reviewer still needs to check, since a
co-implementer cannot fully self-certify them; the remaining three are the independent-check flags on
their Ownership-Lens rows. The one closable here was their own untested `[dev-dependencies]` compiler
claim — that production `mh-service` code cannot import `mh_test_utils` while the edge stays dev-only.
@security **proved it by negative control**: a production `use mh_test_utils::…` fails with
`error[E0433]`. That closes it — asserted-then-proven, the loop's own standard.

---

## Accepted Deferrals

- **`*-test-utils` dev-dependency-edge guard** (@security S2) → `docs/TODO.md` §Test Debt.
- **Per-connection transport contract: three declarations, two languages, Rust↔TS edge unguarded** (@dry-reviewer) → `docs/TODO.md` §Cross-Service Duplication.
- **DRY verdict rule stated five times across two incompatible severity models** (@dry-reviewer) → `docs/TODO.md` §Documentation Hygiene.
- **MH egress-drop counter blind to quinn's silent eviction; hop-gap is the compensating control** (@observability) → `docs/TODO.md` §Media Path Obligations.
- **`metric-coverage` discards an unparseable metric name instead of failing** (@observability) → `docs/TODO.md` §Observability Debt.
- **`MockWebTransport` has no adverse-condition knob; §11 SDK drop counter inherits the gap** (test) → `docs/TODO.md` §Test Debt.
- **ADR-0036 §10 Tier-1a injected-clock wording** (@code-reviewer) → `docs/TODO.md` §Documentation Hygiene.

---

## Rollback Procedure

1. Start commit: `4419df26ce523a7013d449859c79f0f65044635a`
2. Review: `git diff 4419df2..HEAD`
3. Soft reset: `git reset --soft 4419df2`
4. Hard reset: `git reset --hard 4419df2`

---

## Issues Encountered & Resolutions

### My own clippy verification was vacuous, and Layer 5 is what caught it

**This is the finding worth recording, and it is about the check, not the lints.** Before running the
pipeline I reported clippy clean on both crates. The command was
`cargo clippy … --all-targets 2>&1 | grep -cE "^(warning|error)"`, which returned `0`. It returned
zero because cargo emits ANSI colour codes, so every diagnostic line begins with an escape sequence
and **`^error` can never match**. The check was structurally incapable of reporting a failure. It was
not a weak check; it was a check that had no failure mode at all — exactly the "alive, never applied"
shape ADR-0036's coverage section names, committed by the specialist who spent the planning round
arguing against it.

**@paired-media-handler independently committed the same error in the same session**, running the
identical idiom over the identical tool and reporting this crate clean on that basis. Recorded
because two independent instances in one hour say something a single slip does not: the idiom is
plausible enough to defeat careful reviewers, and neither of us applied to our own tooling the
negative-control discipline we were simultaneously applying to the tests.

Layer 5 then found four genuine lints in `transport_shim.rs`: one `manual_is_multiple_of` and three
`manual_async_fn`. A fifth surfaced on the fix pass — `clippy::panic` in the reachability suite,
which has no test allowance in `clippy.toml` (only `expect`, `unwrap` and indexing do).

**Resolutions.** `offer_index % divisor == 0` → `offer_index.is_multiple_of(divisor)`. The three
`fn … -> impl Future + Send { async move { … } }` bodies → `async fn`, which is permitted in an impl
against an RPITIT declaration and keeps the trait's `+ Send` bound checked at the impl. The `panic!`
arm → an `assert!(matches!(…))` plus `refused.err().and_then(DatagramSendError::into_payload)`, which
is strictly better than what it replaced because it now exercises @paired-media-handler's
`into_payload` accessor across the crate boundary the way a real bounded-queue requeue would.

**Method changed, not just the code.** Every subsequent verification in this task is by **exit code**,
never by grepping output: `cargo clippy --workspace --all-targets -- -D warnings; echo $?`. The
generalisable rule — a verification whose failure path has never been observed is not evidence — is
the same rule this task applies to the seam's own drop paths, which is why the two load-bearing
assertions in the suite were mutation-verified rather than trusted.

### A source file was silently emptied mid-review, and the pipeline stayed green

`crates/mh-test-utils/src/transport_shim.rs` was found at **0 bytes** during Gate 3, having been 817
lines. The loudest fact is not the loss but that nothing detected it: a green pipeline result had
already been reported against a tree that no longer contained the file.

**Root cause, and it is @team-lead's, recorded at their instruction.** Before "Start Review" they ran
`git add -A -N .` so `git diff 4419df2` would show the new files to reviewers — a helpful and
correct-seeming action whose cost was invisible until someone reverted. Their framing is the one to
keep: **making a diff visible to reviewers should not change the recoverability of the tree.** Six
paths in this changeset were left intent-to-add, holding git's empty blob `e69de29b…` in the index.

**Trigger**: @test, independently mutation-verifying the pointer-identity trap, reverted with
`git checkout -- <path>`. Against an intent-to-add path that faithfully restores the **empty index
blob**. `git add -N` was the loaded gun; `git checkout` pulled the trigger. Four agents then spent
an hour characterising downstream verbs before anyone asked why the files were intent-to-add at all.

**Recovery, and its honest limit.** Not a rewrite from the plan. A `cp` snapshot at `/tmp/shim.bak`
survived from trap-testing, under a name no search for `transport_shim.rs*` would find — which is why
every reviewer correctly concluded nothing was recoverable. Three subsequent edits were replayed from
transcript, each an exact-string replacement guarded by an assert that would have refused to apply to
non-matching text. The replay landed at **exactly 817 lines**, the count four reviewers had
independently reported from `git diff --stat` before the loss — four observations of a number not
available while rebuilding. The result is **verified-equivalent, not byte-verified**, and is
described that way everywhere. Two things made the restore checkable rather than a matter of trust:
a backup taken *before* a mutation, and a suite that pins the shim's entire public surface.

**Settled git-verb behaviour**, tested by at least two agents each in throwaway `/tmp` repos, never
against `/work`. Reasoning produced four wrong claims in a row here; only running it settled anything.

| Verb | Behaviour against an intent-to-add path |
|------|------------------------------------------|
| `git checkout -- <p>` | **Silently truncates to 0 bytes**, exit 0. The incident verb |
| `git restore <p>` | **Silently truncates to 0 bytes**, exit 0. Same hazard; modern git steers people here |
| `git stash` | Refuses loudly (`Entry '…' not uptodate`), file intact. **Not** a hazard |
| `git commit -am` | Includes the intent-to-add file with real content. Safe |
| bare `git commit` | **Conditional.** Fails loudly (exit 1, zero commits) when nothing is staged. But once **any** tracked change is staged, it succeeds at exit 0 and **silently omits** the intent-to-add path — commit created, skip unreported |
| narrow `git add <path>` then commit | Omits intent-to-add and plain-untracked **identically** — ordinary selective staging, not an intent-to-add hazard |

**Remedy, in its corrected form.** `git add -A`, then — per @security's refinement — a **positive
per-file assertion**: for each changeset file expected to have content, assert its staged blob is not
`e69de29bb2d1d6434b8b29ae775ad8c2e48c5391`. The blanket `git ls-files -s | grep e69de29b` that was
first circulated is **wrong**: it fires forever on `packages/.gitkeep`, a pre-existing file that is
legitimately empty, and a gate that reds on a benign permanent condition is waived on day one and
stays waived. This changeset's ten files were each checked in the positive form and all carry real
staged blobs, with index equal to working tree, so the hazard class is closed here. The runbook entry
belongs wherever that gate lands, not in this document.

### Nothing else

The trait compiled against the shim on the first attempt with no signature negotiation at build
time, because the surface was exchanged in writing beforehand. Two derive omissions
(`Clone` on `TransportError`, `Clone + PartialEq + Eq` on `DatagramSendError`) were caught by
@paired-media-handler from that written surface before either of us spent a build cycle on them.

---

## Lessons Learned

**Sequencing input for story-close, not a defect in this task.** This diff ships no production
implementor of the seam, so the only evidence the trait is implementable by real wtransport is the
API check recorded in §Planning P2-P3 with file:line against wtransport 0.7.2. That is stated rather
than papered over with a fake production impl. The gap closes the moment task 12 lands its real
wtransport implementation, and task 12 depends only on tasks 5 and 9 — so **scheduling task 12
immediately after this one, rather than behind tasks 6-11, is the shortest path to converting a
recorded assumption into a compiled fact.** ADR-0036's own ordering argument applies to itself here:
cheap now, expensive once two more tasks are built against an unvalidated signature.

**Restating the task in mechanism language found a sibling the task did not name.** "A transport
trait plus a loss/delay shim" restated as "install the boundary at which an adverse condition can be
manufactured, so a counter's firing path is a line of code rather than a belief" immediately
surfaced that the client SDK's `MockWebTransport` is the other half of the same mechanism and has no
adverse-condition knob at all — meaning §11's SDK sender-side drop counter, the one §11 says
"matters most", will ship with the gap this task just closed on the Rust side. That is now tracked.
The restatement cost one paragraph and found a whole-class omission; it is worth doing routinely.

**Two of the plan's own claims were overstated in the author's favour, and reviewers caught both.**
"A separate crate makes substitution structurally impossible" was false (it is one manifest line),
and the `WouldBlock` documentation as first drafted let a reader conclude the future egress-overflow
counter observes a transport condition, which it cannot. A third — the ADR-0034 cost premise used to
justify a spin-out — was also wrong, and was caught only because @security checked the tree instead
of accepting the citation. The pattern is consistent: **every one of the three was a claim that made
the author's own choice look better, and none was caught by reasoning, only by someone opening the
file.** The plan's own fidelity-boundary section forbids exactly this failure mode for zero-copy
claims; applying that standard reflexively to process claims is the transferable lesson.

### The through-line: a check is evidence only about the artifact it actually read, reported through the stages it actually passed through

Both clauses failed, repeatedly and independently, across four agents. This is the loop's headline
finding and everything below is a worked example of it.

**First clause — the artifact actually read.** A clippy check read a *colourised stream*, not
diagnostics. A Gate-2 green described a *tree that no longer existed*. A retraction tested *shapes A
and B* and concluded about shape C. A verification tested the *intent-to-add-only* case and concluded
about the tracked-modification case.

**Second clause — the stages actually passed through.** `$?` after a pipe is the status of the pipe's
last stage, not of the command. A `grep -c` through ANSI escapes counts nothing and reports zero. A
`git stash` status read after `sed` reported success for a command that had just failed.

**The strongest evidence for it is that the same error recurred inside three separate attempts to
pin the git hazard down**: @test's over-broad retraction of the bare-`git commit` row (deleted on
shapes A/B while it held on shape C), this author's `git stash | sed` exit code read as git's, and
@security's near-miss on the staged-column check — the last two committed *while writing up or acting
on the lesson about exactly this*. (An earlier draft of this sentence listed @test's one retraction
twice and dropped @security's instance — itself a miscount, corrected here.) A failure mode that
survives being actively documented is a property of the tooling, not of anyone's care. **That is the
argument for structural fixes over vigilance rules, made by the loop itself rather than asserted.**

**Tally, derived rather than asserted** — "at least six" was itself a count stated without a list,
which is this section's own error in miniature. The enumerable through-line instances: the vacuous
ANSI clippy check (this author + @paired-media-handler); two void Gate-2 greens, both caught by the
checksum guard; @test's bare-`git commit` over-claim and its over-broad retraction, two distinct
moves; this author's `git stash | sed` exit code; @security's staged-column near-miss; and the
misattribution of @test's conduct in this very document. **Eight instances across four agents and the
pipeline** — not one person's carelessness, which is the whole point.

**Standing fixes, both cheap.** Verify by unpiped exit code, never by pattern-matching output.
And @operations' rule for any claim about a destructive operation: **test where the blast radius is
zero** — a throwaway repo costs fifteen seconds. In this loop, every claim that was tested survived
and every claim that was reasoned about was wrong (@paired-media-handler's summary), across four
consecutive wrong git claims, two of them corrected by retracting a retraction.

### Worked example: the verification commands that could not fail

The implementer — this author, in the test-**specialist** role — piped clippy through
`grep -cE "^(warning|error)"`, got `0`, and reported `mh-test-utils` clean; @paired-media-handler ran
the identical idiom over the same crate and reported the same (@team-lead's Gate-3 determination).
Both counts were structurally incapable of being non-zero: cargo emits ANSI colour codes, so every
diagnostic line begins with an escape sequence and `^error` matches nothing, ever. Layer 5 found the
four real lints both checks had been blind to.

**This paragraph first named @test (the reviewer) as the author of that vacuous check, and that was
wrong.** @test caught it by reading their own transcript rather than accepting this document: their
clippy was exit-code-based (`-D warnings`, status by `$?`) from the first run, they never ran the
idiom, and `mh-test-utils` is the test specialist's crate, not the reviewer's. The record had
conflated two different agents under one name — the cross-cutting `test` reviewer and this author's
own `test` specialist role. **Getting a named person's conduct wrong in the durable record is the
most consequential form of the exact error this section documents, and it happened inside the section
documenting it**: a claim about @test's conduct that had not been read against the artifact of @test's
conduct. It is the strongest single piece of evidence the loop produced — worth more than a tidy
count, and the reason the count below is derived from a list rather than asserted.

The framing that generalises is @paired-media-handler's. This is not a lesson about grep. Both of us
had just spent a full planning round arguing that a control must be *demonstrated* rather than
asserted — the fidelity boundary, the "does it fire / does it apply" table, three overstated claims
corrected — and then both of us trusted an undemonstrated control, because it was *our own tooling*
rather than the artifact under review. The same discipline applied one level up would have caught it
in seconds: this task mutation-verified its two load-bearing assertions (make the shim copy; insert a
real `UdpSocket::bind`) and neither of us thought to do the equivalent for the commands doing the
verifying. **Ask of a verification command what this task asks of a counter: has its failure path
ever been observed?**

Cheap standing fix, now used for every check in this loop: verify by **exit code**
(`cargo clippy --workspace --all-targets -- -D warnings; echo $?`), never by pattern-matching output.
Recorded as a shared finding rather than one implementer's slip, because a single-author framing
would read as carelessness when the actual signal is that a plausible idiom defeated both
co-implementers — this author and @paired-media-handler — over the same tool on the same day.

**Four instances of one pattern in one diff, which makes it a pattern rather than four slips.**
Each is a name or a claim quietly promising more than the code delivers:

- The **fidelity boundary** — `Bytes` on `send_datagram` reads as "zero-copy to the wire", and
  production copies structurally to prepend the H3 session-id varint.
- The **zero-syscall claim**, narrowed during implementation from "proves the path is syscall-free"
  to "traps *tokio-driven* I/O". True and still the control that matters, because production reaches
  the network through wtransport and quinn and both are tokio-based — but it would not trap a raw
  `std::net` call, and the wider phrasing would have misled whoever extends the driven path.
- **`egress_offered` renamed `egress_accepted`**, because it never counted refusals (a refusal
  consumes no capacity and is not an acceptance). "Offered" would have read as a rate denominator it
  cannot serve — a defect that surfaces months later as a subtly wrong dashboard, never as a red
  test.
- **`ShimConfig`'s doc claimed "both collections… neither grows without bound"** while there were
  three retention sites and the third was unbounded (@operations). A doc asserting a *resource*
  property the code lacked, two screens below this document's own block warning against exactly that
  — and the consequence was not tidiness: a leaking benchmark harness reports regressions it caused
  itself.

The common shape is a claim whose *scope* is wider than its *mechanism*, and in every case the
correction cost one sentence while the uncorrected version would have been acted on by someone with
less context. Worth naming as a class: ADR-0036 §11 already forbids it for metrics, and this diff
shows it arising just as readily in a type choice, a test-harness claim, an accessor name and a
config doc.

**A correction is not complete until you have searched for the claim everywhere it may already have
landed.** The overstated control claim was corrected in the crate doc and in this document, and
survived in a `crates/mh-service/Cargo.toml` comment three lines above the manifest line whose
relocation it wrongly declared impossible — the one place a reader acting on it would be misled, and
it undercut the spun-out guard's own rationale. Caught by @security at Gate 3, not by the author who
wrote the correction. Sweeping for the original phrasing after weakening it costs one grep.

**A verdict is a statement about a specific tree.** Two Gate-2 greens were void because files moved
during the run; the checksum guard over all ten changeset files is what converted "the pipeline
exited 0" into "the pipeline exited 0 *on this tree*". The same applies one level down to reviewer
verdicts, which rebind whenever content changes. This is the through-line's first clause applied to
process rather than to a command.

**Mutation-verify an assertion before trusting it.** Both load-bearing assertions here were
confirmed as traps by deliberately breaking what they check — the shim was made to copy, and a real
`UdpSocket::bind` was inserted into the no-I/O-driver runtime — and both failed as required before
being restored. A pointer-identity assertion and a "no reactor running" panic are exactly the kind of
check that can silently become a tautology; ADR-0036's "does it fire" half is cheap to answer
directly and expensive to assume.
