# `credential_leak_key_custody` fixtures

Fixtures for the **credential-leak semantic check, items 11-13** (MC key custody)
in `scripts/guards/semantic/checks.md`. ADR-0036 §11, story task 23.

## What is different about these, and why they live under `lens/`

**There is no mechanical guard for items 11-13.** They are judged by the
`semantic-guard` agent, by value rather than by name. So unlike
`../media_telemetry_deny/`, no harness can make these fixtures fire — the
executor is an agent, and the demonstration is a Lead-directed
**fixture-verification run**.

That is why they sit under `lens/`, following
`../ts_retained_credentials/lens/`. The path segment carries the fact that the
executor is an agent and that the mechanical guards are *expected* to be partly
silent here — encoded in the path rather than in an exemption list, because an
exemption list is the thing these guards refuse to ship for themselves.

## Where each fixture's expected verdict lives — one home

**Each fixture's own `// Invariant:` block is its expected verdict.** It names
both the outcome (FIRE / CLEAR) and the check item (11 / 12 / 13 /
value-not-name), so a future agent reaching the block can reproduce a verdict
without the conversation that produced it.

This README deliberately carries **no per-fixture verdict table**. A third copy
of the expectation — spec in the fixture, catalog in the harness, table here —
is the one that drifts, and it drifts silently because nothing reads it. Same
ruling as `../media_telemetry_deny/README.md` makes about its `catalog()`.

If a run disagrees with an `// Invariant:` block, that is a question for the
check's owners (`security` + `semantic-guard`), **never** a licence to relax the
block.

## Safety of `.rs` under `fixtures/` — read `../media_telemetry_deny/README.md` first

The general argument for why `.rs` files here are inert (no cargo target, so
Layer 2 `fmt` and Layer 6 `clippy` never see them; scanner exclusion by path
segment) is stated once, there. It is not repeated here.

**What is genuinely different here, and it is load-bearing.** The
`media_telemetry_deny` fixtures plant telemetry macros and no credential
vocabulary at all. **These fixtures plant live `PII_TOKENS_CATEGORY_A` terms** —
`meeting_kek` and `transmit_key` are catalogued entries
(`crates/dt-guard/src/common/pii_vocabulary.rs`). So the double exclusion by
`common::test_code_filter::is_scan_exempt` — the `/fixtures/` path segment *and*
`crates/dt-guard/**` — is doing real work here in a way it was not doing there.
If either exclusion is ever narrowed, this directory reds the Rust secret
scanners on every devloop for the whole team.

Two consequences:

1. **Never move these fixtures outside an exempt path**, and never "fix" a
   resulting VIOLATION with a suppression comment. Move the fixture back.
2. **Never plant into `crates/mc-service/**`.** That is the check's real scope;
   a plant there is a live finding against the team on every run.

## What the harness can and cannot assert

`../../credential_leak_key_custody_fixtures.rs` asserts only what a machine can:
catalog integrity, the check's scope being non-empty against the real tree, the
mechanical floor's per-spelling extent, and poles-to-fixture drift.

**It cannot assert that the semantic check fires.** That half is the directed
run. Anyone reading a green `cargo test` here should read it as *"the fixtures
are intact and the floor behaves as documented"*, never as *"items 11-13 were
demonstrated"*.

## Repeating the verification run

The check's own §Fixture-verification runs clause makes named fixture files
in-scope when the Lead directs a run — test files are otherwise out of scope, so
without that clause a dutiful "no findings" here would be procedurally correct
and completely vacuous.

To repeat it:

1. Lead directs a fixture-verification run over
   `crates/dt-guard/tests/fixtures/credential_leak_key_custody/lens/`.
2. `semantic-guard` reports a verdict **per fixture**, checked against that
   fixture's `// Invariant:` block. A `pos_` fixture reported CLEAR, or a `neg_`
   fixture reported FIRE, is a discrepancy — not a pass.
3. The same run confirms the check is **clean on the real artifacts**, which is
   the *applies* half. A fixture set that only fires proves the control is alive,
   not that it is aimed at anything. The real artifacts are:
   - the MC→MH contract construction at
     `crates/mc-service/src/grpc/mh_client.rs` (`RegisterMeetingRequest`, which
     carries no key material by design), plus the other `internal::v1`
     construction sites in `crates/mc-service/src/grpc/` and `src/main.rs`;
   - MC's real key-custody code: `crates/mc-service/src/media_admission/`
     (`kek.rs`, `identity_key.rs`), `src/actors/meeting.rs`, `src/actors/messages.rs`,
     `src/media_routing/generation.rs`, `src/webtransport/connection.rs`;
   - MC's real key-adjacent logging sites. A sweep of MC tracing macros on
     2026-09-08 found exactly one, `crates/mc-service/src/main.rs` — an `error!` reporting
     that a binding-token secret failed to base64-decode, which logs decode-error
     metadata and not the secret.
4. Record every verdict in the devloop output. **The recorded run is the
   demonstration**; a claim that it was performed is not.

## Adding a fixture

Give it a banner, an `// Invariant:` block naming verdict and item, and
obviously-synthetic values (`[0xC3u8; 32]` — the convention from
`crates/proto-gen/tests/signaling_roundtrip.rs`). Never anything with the texture
of captured key material, and never a value from a real KDF.

Do **not** enumerate credential tokens here or in a fixture. The vocabulary has
one home (`crates/dt-guard/src/common/pii_vocabulary.rs`) and it is a *floor*
these items sit above; `checks.md` says twice that a second vocabulary drifts
from the first silently. Plant one realistic spelling in a realistic shape and
let the `// Invariant:` block say what must happen.
