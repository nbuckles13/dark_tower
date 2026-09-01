# Protocol Conventions

Canonical rules for `.proto` files in this repository. This doc is the **spec**
for tasks #30 (file-layout cleanup) and #31 (STANDARD-lint rename sweep) of the
browser-client-join user story (R-61). New `.proto` work must conform to these
rules from the start; pre-existing deviations are draining via the R-61 chain.

## Enforcement

`proto/buf.yaml` is the enforcement mechanism. It pins:

- `lint.use: [STANDARD]` — the full buf STANDARD ruleset, no carve-outs.
- `breaking.use: [FILE]` — symbol-set breakage detection, the strictest
  category. Chosen over `WIRE_JSON` on 2026-05-20 (`docs/TODO.md`): `WIRE_JSON`
  covers binary-wire compatibility only, so a package rename via file move —
  same tags, same types, no wire-encoding change — passed it silently. `FILE`
  catches `PACKAGE_NO_DELETE`, `FILE_NO_DELETE` and message removal.

There is no `lint.ignore` block, and as of the ADR-0036 signalling reshape no
`// buf:lint:ignore` annotations either — `signaling.proto` carried five, all
now drained. If a **lint** finding surfaces, the fix lives in the `.proto`
source, not in a buf config carve-out and not in an inline annotation
(consistent with ADR-0033 §13 and ADR-0034's "fix the parser, don't relax the
check" principle). This paragraph is about `lint` only — see the breaking-change
note below, which no longer shares its absolutes.

**Intentional wire breaks.** A devloop that intends a wire break declares the
expected Layer-6 findings up front — by rule class, in its `main.md` under
`## Expected Layer-State` and in the commit message. The declaration is the
artifact: the real log is diffed against it class by class, and any
*unpredicted* finding is a regression on its merits. In a **headless** run the
Lead may **not** accept the red itself (that is a self-approved risk acceptance);
it escalates and a human decides.

> **Correction (2026-08-31, ADR-0036 signalling reshape).** This section
> previously read "`buf breaking` has no suppression mechanism by design
> (`scripts/lang/proto/breaking.sh`: no `--exclude-path`, no `--against`
> override, no env bypass)." **That was false and is corrected here rather than
> quietly deleted, because a reader who trusted it would misread a green gate.**
> `breaking.sh`'s lockdown covers **CLI flags and env bypass** — it does not
> forward `"$@"` and has no skip variable, and that much remains true. It says
> nothing about **config keys**, and `proto/buf.yaml` can silence the gate
> entirely via `breaking.ignore`, which takes **paths** (files or directories
> relative to the module root), not package names.
>
> **A carve-out is live right now.** `proto/buf.yaml` carries
> `breaking.ignore: [dark_tower/signaling/v1/signaling.proto]`, added at the
> user's direction to carry the ADR-0036 intentional break (54 predicted and
> measured findings) through Layer 6 and CI. While it is present, **no break of
> any kind is caught in that file — including an unintended one — and a green
> Layer 6 is not evidence that file is break-free.** `internal.proto` and every
> other proto stay fully enforced; do **not** widen the entry to `dark_tower`.
> `scripts/lang/proto/breaking.sh` prints `SUPPRESSED=<paths>` on every run
> while the key exists. Restore by deleting the key once the ADR-0036 story
> merges to main — tracked in `docs/TODO.md` ("Restore buf breaking enforcement
> after ADR-0036 story 1").
>
> **If you are the task-4 (`internal.proto`) implementer**: your break is *not*
> covered by the entry above and Layer 6 will fire for you. Declare it the same
> way and expect the same escalation.

## Rules

### 1. File layout mirrors package path

A `.proto` file lives at `proto/<package_path>/<file>.proto`, where
`<package_path>` is the protobuf `package` declaration with dots replaced by
directory separators.

Concrete: package `dark_tower.internal.v1` lives at
`proto/dark_tower/internal/v1/internal.proto`. Task #30 lands this layout.

### 2. Package version-suffix

Every package ends in `vN`, where `N` is a positive integer. `v1` is the
current major version for all packages.

New major versions live in sibling `vN+1` directories. Once a package has
external on-the-wire clients, its `vN` files are frozen — additive,
wire-compatible changes only. Wire-breaking changes get a new `vN+1` package.

### 3. Bare RPC request/response names

For each RPC `Foo`, the request type is `FooRequest` and the response type is
`FooResponse` — **bare**, not service-prefixed.

Concrete: the `RegisterParticipant` RPC on `MediaHandlerService` uses
`RegisterParticipantRequest` / `RegisterParticipantResponse`, not
`MediaHandlerServiceRegisterParticipantRequest`.

Each service's RPCs live under their own service block; cross-service name
collisions don't happen in practice and bare names read cleaner at call sites.

Rationale: Clarification Q14 in
`docs/user-stories/2026-05-02-browser-client-join.md`.

### 4. Distinct response type per RPC

STANDARD's `RPC_REQUEST_RESPONSE_UNIQUE` requires every RPC to have its own
response message type. No sharing across RPCs, even when the shapes are
currently identical.

Concrete: the legacy `HeartbeatResponse` (shared by Fast + Comprehensive
heartbeats on `GlobalControllerService`) splits into `FastHeartbeatResponse`
and `ComprehensiveHeartbeatResponse`. Task #31 lands this rename.

Rationale: Clarification Q15 in
`docs/user-stories/2026-05-02-browser-client-join.md`. The split is cheap now
and keeps future divergence (extra fields on one side) wire-clean.

### 5. Reserve the vacated tag and name; never repurpose in place

When a field's meaning changes, the old tag number **and** the old field name go
into `reserved`, and the new field takes a **fresh** tag. A tag is never
re-pointed at a different concept, even when the wire types differ.

```proto
// Pre-ADR-0036: `uint64 user_id = 2`.
//
// `sender_id` deliberately does NOT re-point tag 2: `uint64` and `uint32` are
// both varint, so an old peer would decode it SUCCESSFULLY with the wrong
// semantics.
reserved 2;
reserved "user_id";

optional uint32 sender_id = 8;
```

Rationale: a repurposed tag whose old and new encodings are both decodable by an
old peer is the one failure mode that does not fail loud — the peer reads a
plausible wrong value instead of erroring. Reserving converts it into a field
deletion, which every decoder and `buf breaking` both report. Reserve the
wire-incompatible repurposes too: a rule applied to some of the repurposes in a
message is worse than no rule, because a reader cannot tell the unreserved tags
from the ones nobody looked at.

Applies to **enum values** as well as field tags: a vacated enum number is
reserved, never re-pointed.

**Two exceptions, and only these two.** Both must be annotated at the site so
the omission reads as a decision rather than an oversight.

1. *A name reused by the new shape.* If the new field keeps the old field's name
   on a fresh tag, the name cannot also be reserved — protoc rejects reserving a
   name that is then used. Reserve the number only, and say why in a comment.
2. *Enum value 0.* proto3 forces value 0 to exist, so it cannot be reserved.
   Renaming it to `<ENUM>_UNSPECIFIED` re-points it by necessity. This is safe
   only in the direction where a real value collapses to the fail-closed
   reading; check that before relying on it.

## Why STANDARD, not a custom ruleset

We adopted the full buf STANDARD ruleset rather than carving exceptions because
it is the lingua franca for protobuf hygiene — future tooling, `buf breaking`
semantics, and ecosystem interop all assume STANDARD shapes. The one-time
wire-break cost to bring the repo into compliance is acceptable: there are no
on-the-wire clients outside this codebase yet (same precedent as R-60's
`MediaConnectionUpdate` redesign earlier in this story).

Carve-outs would be a permanent tax. Each `// buf:lint:ignore` annotation or
`lint.ignore` entry is a maintenance hazard ("remember to remove this") that
silently rots into the codebase. We pay the rename cost once.

## Sequencing note (R-61, task #29 → #30 → #31)

This doc lands first as the spec. Once it lands, `buf lint` fails repo-wide on
the 21 pre-existing STANDARD findings on `proto/internal.proto` and
`proto/signaling.proto` until task #31 closes. Track 2 therefore runs as a
contiguous, exclusive 29 → 30 → 31 sequence — no other devloops in flight
during the window, because every devloop's Layer 5 will fail until the rename
sweep completes.

This trade-off is accepted to eliminate the `buf.yaml` `lint.ignore` carve-out
anti-pattern, per Revision 8 of the user story.
