# Devloop Output: Release-Build Premise Guard + dev-web.sh MH Preflight Hard-Fail

**Date**: 2026-09-01
**Task**: Assert the deployed-artifact premises this story's controls rest on: a new `dt-guard` subcommand + Layer-3 wrapper that fails on release-build drift (service Dockerfile `cargo build` losing `--release`, or `[profile.release]` enabling `debug-assertions`), plus escalating `scripts/dev-web.sh`'s two MH WebTransport preflight checks from WARN to HARD FAIL.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ebebea89bb92427618cab5140f5af2bea36d8b81` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `a2d3ac6f` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `abbb62ea` |
| Test | `aea18d93` |
| Observability | `a4b7aa6a` |
| Code Quality | `a9a94bb4` |
| DRY | `a7e0421a` |
| Operations | `adebc7d7` |
| Semantic Guard | `a39d74c9` |
| Client (conditional domain reviewer) | `a4ffce7a` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
-->

---

## Task Overview

### Objective
Assert the deployed-artifact premises ADR-0036 §11's compile-time controls rest on, and stop
`scripts/dev-web.sh` from greenlighting a demo that joins and returns no audio.

Both halves are the same defect in different clothing: **a control that is inert, while looking
like coverage.** §11's `compile_error!` is inert the moment `debug_assertions` is on in a shipped
artifact; a WARN on MH reachability is inert the moment MH became the demonstrated path. Neither
had anything watching it.

### Scope
- **Service(s)**: none directly — this is build-configuration and dev-tooling policy. It asserts a
  premise for `mh-service` (ADR-0036 §11) and covers all four service images.
- **Schema**: No.
- **Cross-cutting**: Yes — guard pipeline (Layer 3), all four service Dockerfiles, the workspace
  manifest, the client dev-launch path, and two runbooks.

### Debate Decision
NOT NEEDED — implements decisions already taken in ADR-0036 §11 and in the story manifest. The one
design question that arose (whether to mechanise the standing insecure-flag prohibition in-loop)
was resolved by @team-lead at Gate 1 as a consistency call, not a new decision.

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/dt-guard/src/release_build_profile.rs` (new) | Mine | — |
| `crates/dt-guard/src/no_insecure_browser_flags.rs` (new) | Mine | — |
| `crates/dt-guard/src/lib.rs` (two module declarations) | Mine | — |
| `crates/dt-guard/src/main.rs` (two clap variants and dispatch arms) | Mine | — |
| `crates/dt-guard/Cargo.toml` (adds the toml dependency; §H3/§J5) | Mine | — |
| `crates/dt-guard/src/common/services.rs` (doc comment only; dt-guard-internal, NOT the shared `crates/common` crate) | Mine | — |
| `scripts/guards/simple/validate-release-build-profile.sh` (new) | Mine | — |
| `scripts/guards/simple/validate-no-insecure-browser-flags.sh` (new) | Mine | — |
| `scripts/dev-web.sh` | Not mine, Domain-judgment | `client` (paired, ADR-0024 §6.5; confirmed both asks) |
| `scripts/dev-web.test.sh` (new) | Mine | — |
| `scripts/layer3.sh` (one run_and_emit line for the self-test) | Mine | — |
| `crates/mh-test-utils/src/lib.rs` (one ANCHOR-DRY comment line; §I7/P-DRY-5) | Not mine, Mechanical | `media-handler` (owner, **absent from this devloop** — see note) + `test` |
| `docs/runbooks/client-dev-local.md` (§3 Step 0 severity-contract hunk only) | Not mine, Domain-judgment | `operations` (authored the prose; §I9/§I12) |
| `docs/runbooks/devloop-validation.md` (operations-authored hunks) | Not mine, Domain-judgment | `operations` |
| `crates/dt-guard/tests/release_build_profile_e2e.rs` (new) | Mine | — |
| `crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs` (new) | Mine | — |
| `docs/TODO.md` (OPS-3 rollback entry, two named deferrals, line-cite rot) | Mine | — |
| `audit-suppressions.toml` | Not mine, Domain-judgment | `security` (policy) + `operations` (machinery) |
| `.cargo/audit.toml` (generated) | Not mine, Domain-judgment | `security` (policy) + `operations` (machinery) |
| `docs/devloop-outputs/2026-09-01-release-premise-and-preflight-guards/main.md` | Mine | — |

**Notes on the cross-boundary rows.**

- `scripts/dev-web.sh` — the edit changes *behavior* for client developers (a preflight that warned
  now blocks the demo launching), which ADR-0024 §6.2 puts squarely in Domain-judgment. Deliberately
  **over**-classified: the WARN→FAIL call rests on infra topology facts, but the blast radius lands
  on the client dev loop. @client has confirmed all four sub-asks and will co-sign at Gate 3.
- `crates/mh-test-utils/src/lib.rs` — classified **Mechanical**, and the classification carries a
  condition rather than being a formality. The edit is a single `ANCHOR (DRY):` comment line in the
  tree's existing convention (5 precedents), adding **no** claim: it names
  `dt-guard release-build-profile` as the enforcing home for a premise that doc comment *already*
  asserts. Value-neutral and structure-preserving; the `sed`-test applies.
  **@code-reviewer's caution is right and I am honouring it explicitly: media-handler is NOT on this
  devloop.** So the commitment is — if the line turns out to need *any* wording that changes what
  the doc comment claims, it stops being Mechanical and I **drop it** rather than write it
  unilaterally, raising it as a follow-up for an owner. A comment anchor is worth having; it is not
  worth an unreviewed edit in an absent owner's crate. (I first classified it Minor-judgment;
  @code-reviewer's framing that a pure value-neutral comment anchor is plausibly Mechanical is
  better, *provided* the no-new-claim property actually holds — which is the condition above.)
- `docs/runbooks/client-dev-local.md` §3 Step 0 — **@operations authored the replacement prose**
  (§I12). Conditional per §I9: if `dev-web.test.sh` cannot be made hermetic and so is not wired into
  `layer3.sh`, I return to @operations **before** writing this hunk, because the DRY collapse would
  then remove redundancy without adding a forcing function. @team-lead is holding this as a Gate 3
  blocker.
- **The two audit-suppression rows are not this task's work** — they are a pre-existing
  `RUSTSEC-2023-0071` suppression that expired on the calendar (2026-09-01) mid-loop, unrelated to
  this diff and untouched by it. @security granted acceptance and @operations authored the renewal;
  I deliberately did **not** touch either file, because bumping an expiry to green a pipeline is the
  masked-failure pattern this devloop exists to close. The rows exist so Layer A scope-drift reflects
  the real changeset — both owner-involvement tiers were already satisfied on the record. Verified
  the landed change against @security's terms: expiry `2026-12-01`, rationale corrected from
  "build-time via sqlx-macros" to **lockfile-only**, and the verify command of record updated to
  `cargo tree -p rsa --invert --edges normal` (must print nothing).
- **The secure-context SECTION of `docs/runbooks/client-dev-local.md` is deliberately absent from
  this table** — story task 20's deliverable at a frozen anchor I consume verbatim (§D). Two
  different hunks in one file; only §3 Step 0 is in scope here.

Not a Guarded Shared Area: none of these paths match ADR-0024 §6.4's criterion (no wire
format, no auth-routing policy, no forensics contract, no schema evolution) or the
enumerated list.

### Approved-Cross-Boundary trailers (for the commit — @team-lead ruling 2026-09-01)

ADR-0024 §6.7 makes these **optional**: owner confirmation is already satisfied by Gate 1 review
plus the Gate 3 Ownership Lens verdict, and here more strongly than usual — @operations *authored*
the `client-dev-local.md` §3 Step 0 and `devloop-validation.md` prose, and @client co-signed the
pairing. @team-lead ruled to add them anyway for **audit durability**: story task 20 makes
`client-dev-local.md` client/operations co-owned, so the next person to touch it arrives after the
ownership has changed and will want to know who signed a severity-contract *deletion*. Exact strings:

```
Approved-Cross-Boundary: operations authored the §3 Step 0 severity-contract hunk and ruled deletion over synchronization
Approved-Cross-Boundary: client co-signed the dev-web.sh header contract and the runbook pairing
```

- The first (operations) covers **both** ops-owned files — `docs/runbooks/client-dev-local.md` §3
  Step 0 and `docs/runbooks/devloop-validation.md` §8/§6.3.1 — since one reason scopes both
  (@code-reviewer Ownership Lens).
- The second (client) covers `scripts/dev-web.sh` (Domain-judgment, owner client): the WARN→HARD-FAIL
  escalation and the secure-context pointer for the client dev loop.
- **No trailer for `crates/mh-test-utils/src/lib.rs`** — it is Mechanical (review-only, §6.3), and
  @code-reviewer accepted it conditionally on the line adding no new claim. A trailer there would
  imply an approval tier that does not apply. @test eyeballs it at their gate as co-owner ACK.

Reason clauses are ≥10 chars and name the *authority*, per §6.7. Standard trailer per CLAUDE.md:
`Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

---

## Planning

### A. Restating the problem in mechanism-language (before the file list)

The task is phrased in instance-language: "guard the Dockerfiles' `--release`, and the
workspace `[profile.release] debug-assertions`."

Stated as a mechanism, it is:

> **A compile-time control is only as strong as the build configuration of the artifact that
> ships.** ADR-0036 §11 chose `compile_error!` over a CI check precisely because "a control that
> has to notice fails silently for anyone building outside the pipeline; a compile error has
> nothing to notice." That reasoning holds *conditionally on* `debug_assertions` being off in the
> deployed binary. Nothing in the repo asserts that condition. So the guard's subject is not
> "two files" — it is **every input that can flip `debug_assertions` on in a shipped artifact.**

Enumerating that class from Cargo's own semantics gives seven inputs, of which the task names two.
(Rows 1–5 are mine from the ADR reasoning; **row 6 is @security's A4** and **row 7 is
@observability's (d)** — I missed both, and each leaves every file the task names byte-identical.)

| # | Input that can flip `debug_assertions` on in a shipped binary | Named by task? | In tree today |
|---|---|---|---|
| 1 | Service Dockerfile `cargo build` losing `--release` / switching `--profile <non-release>` | yes | 4 Dockerfiles, all `--release` |
| 2 | `[profile.release] debug-assertions = true` in the workspace root `Cargo.toml` | yes | absent (premise holds) |
| 3 | `[profile.release.package.<crate>] debug-assertions = true` — a per-package override Cargo **does** honour, and which can target `mh-service` alone | **no** | absent |
| 4 | A custom `[profile.X] inherits = "release"` with `debug-assertions = true`, selected by a Dockerfile `--profile X` | **no** | absent |
| 5 | `RUSTFLAGS = "-C debug-assertions=on"` injected via `.cargo/config.toml`, a Dockerfile `ENV`, or a CI workflow `env:` | **no** | absent (`.cargo/config.toml` does not exist) |
| 6 | `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true` in a Dockerfile `ENV`/`ARG` or CI `env:` — flips it with `Cargo.toml` untouched (**@security A4**) | **no** | absent |
| 7 | A `[profile.release]` table **inside `.cargo/config.toml`** — cargo honours config profiles, and this falls between rows 2 (scoped to root `Cargo.toml`) and 5 (scoped to `rustflags` in that file) (**@observability (d)**) | **no** | absent (no `.cargo/config.toml`) |

**This is the widening the workflow asks me to surface.** The mechanism forbids using the task's
own nouns ("the Dockerfile's `--release` flag", "`[profile.release]`") because rows 3–5 defeat the
premise without touching either noun. Row 3 is the sharpest: `[profile.release.package.mh-service]
debug-assertions = true` re-arms §11's `compile_error!` in exactly the one crate the story cares
about, while the workspace `[profile.release]` block the task names stays byte-identical and every
Dockerfile keeps its `--release`. A guard scoped to the task's two nouns would report clean.

All seven rows are infrastructure-owned files (Dockerfiles, root `Cargo.toml`, `.cargo/`,
`.github/workflows/`), so widening adds **no** new cross-boundary surface — it adds same-owner
siblings only. **Proposal: guard all seven.** Reviewers, push back if you want it narrowed to the
two named rows; my argument for the full seven is that rows 3–7 are cheaper to cover now (same
parser, same file walk) than to discover later, and a guard that reports clean while the offending
line ships "reads as coverage" — ADR-0036 §11's own words about a *different* guard, same failure
shape.

**One scope boundary I am NOT crossing.** The premise is also "the shipped artifact is the one the
pipeline built" (image tag / digest pinning). That is a real deployed-artifact premise, it is
mine, and it is genuinely task-sized — different inputs, different mechanism, and it needs an
`operations` conversation about tag policy. Named here, not silently dropped; not in this devloop.

**Fact reviewers should know before approving.** I grepped for the §11 `compile_error!` guard the
task's rationale rests on: `grep -rn compile_error crates/mh-service/src/` returns **nothing**. The
per-frame-tracing feature and its compile-time guard are not in tree yet — presumably a later task
in this story. This does **not** change the plan: the premise is worth holding on its own, and a
premise guard that lands *before* its consumer is strictly better than one that lands after. But
it does mean the new guard must **not** be written as a forcing function on `compile_error!`
existing (that would fail Layer 3 today). It checks premises unconditionally. Flagging so nobody
reviews it expecting a coupling that isn't there.

### B. Half 1 — `dt-guard release-build-profile` + Layer-3 wrapper

**New module** `crates/dt-guard/src/release_build_profile.rs`, following the `kustomize.rs` /
`ts_dev_trust.rs` shape (rule-id consts, `Hit` struct, `run(repo_root, explain)`, `emit_ok` on
clean, `anyhow::bail!("<class>-<count>")` on hits so `main.rs` renders `STATUS=FAIL REASON=`).

Rule ids (one per failure class, per @observability #1 / @test #1 — never one token for the
whole guard):

| rule_id / REASON token | Row(s) | What it means |
|---|---|---|
| `dockerfile-build-not-release` | 1 | A service Dockerfile's `cargo build` does not select the release profile. **Premise break.** |
| `dockerfile-cook-profile-mismatch` | 1 | `cargo chef cook` disagrees with `cargo build` on profile. Cache-layer drift, **not** a premise break (see §H1). |
| `release-profile-debug-assertions-enabled` | 2, 3 | `[profile.release]` or `[profile.release.package.*]` enables debug-assertions. |
| `release-inheriting-profile-debug-assertions` | 4 | A `inherits = "release"` profile enables it *and* a Dockerfile selects it. |
| `rustflags-debug-assertions` | 5 | `-C debug-assertions=<truthy>` in `.cargo/config.toml`, a service Dockerfile, or a CI workflow. |
| `cargo-profile-env-debug-assertions` | 6 | `CARGO_PROFILE_<PROFILE>_DEBUG_ASSERTIONS` set via Dockerfile `ENV`/`ARG` or CI `env:`. |
| `no-service-dockerfiles-discovered` | vacuity | The walk matched zero Dockerfiles. |
| `dockerfile-no-cargo-build-line` | vacuity | A service Dockerfile exists but has no `cargo build` at all. |
| `workspace-release-profile-unreadable` | vacuity | Root `Cargo.toml` absent, unparseable, or has no `[profile.release]`. |
| `service-crate-without-dockerfile` | H2 | A `crates/*-service` workspace member has no `infra/docker/<name>/Dockerfile` under the gate. |

Behaviour notes:

- **Discovery is bidirectional** (§H2): glob `infra/docker/*/Dockerfile` for what exists, and
  `[workspace] members` for what *should* exist. Non-Rust dirs (`certs/`, `grafana/`, `postgres/`,
  `prometheus/`) self-exclude by having no cargo line — **no hardcoded exclusion list to rot.**
- **`--release` and `--profile release` are both accepted** (@code-reviewer #4, @security A4) —
  semantically identical; requiring the literal `--release` would false-fail a legitimate build.
  `--profile <anything-else>` fails, and cross-checks row 4.
- Line continuations (`\`) are joined before matching, so a flag on the next physical line is not
  a false failure. Its own unit test — a guard that false-fails gets bypassed.
- **Vacuity is FAIL, never OK** (§H4): every "found nothing to check" branch has its own token.

**Wrapper** `scripts/guards/simple/validate-release-build-profile.sh` — the canonical 4-line
shape, `source .../_dt_guard_wrapper.sh release-build-profile`. **No `layer3.sh` edit is needed
or wanted**: `run-guards.sh` auto-discovers `simple/*.sh`, and `layer3.sh` only names guards that
are deliberately *outside* that directory (the `*.test.sh` self-tests). @test — this answers your
"enumerate the layer3.sh wiring line": for the guard itself there is none by design, and I'll
verify auto-discovery picks it up rather than assert it. There *is* a wiring line for the bash
self-test in §C.

`lib.rs` gets `pub mod release_build_profile;`; `main.rs` gets the `ReleaseBuildProfile { root,
explain }` variant + dispatch arm, matching every existing subcommand.

**Unit tests** (`#[cfg(test)] mod tests` in the module, plus `tempfile` trees — `tempfile` is
already a dev-dependency). Per @test's front-loaded ask, the negative direction is the point:

| Test | Asserts |
|---|---|
| `dockerfile_missing_release_fails` | crafted Dockerfile with bare `cargo build --package mh-service` → hit |
| `dockerfile_release_passes` | `--release` → no hit |
| `dockerfile_profile_release_long_form_passes` | `--profile release` → no hit |
| `dockerfile_non_release_profile_fails` | `--profile quick` → hit |
| `dockerfile_line_continuation_release_passes` | `--release` after a `\` continuation → no hit (guards against a false-positive guard) |
| `profile_release_debug_assertions_fails` | `[profile.release] debug-assertions = true` → hit |
| `profile_release_package_override_fails` | `[profile.release.package.mh-service] debug-assertions = true` → hit (**row 3 — the case the task's framing misses**) |
| `profile_release_clean_passes` | today's actual `[profile.release]` (opt-level/lto/codegen-units/strip) → no hit |
| `inheriting_profile_selected_by_dockerfile_fails` / `..._unselected_passes` | row 4, both directions |
| `rustflags_debug_assertions_fails` | `.cargo/config.toml` with `-C debug-assertions=on` → hit |
| `current_tree_premise_holds` | runs `run()` against the **real repo root** and expects OK — this is the "premise currently holds" assertion the task states, and it is the test that will fire the day someone drops `--release` |

### C. Half 2 — `scripts/dev-web.sh` WARN → HARD FAIL

**Mechanism restatement here too.** The WARN/HARD-FAIL split in the header encodes one question:
*is this check on the demo's critical path?* Every WARN in the script is annotated "join only" or
"browser only" — i.e. written when the script's success criterion was "sign-up and create work."
This story makes **audio through MH the demonstrated path**. The predicate didn't change; the
demo's critical path did, and the WARN annotations are now stale facts, not stale opinions.

That restatement makes the scope slightly wider than the task's two nouns, and I want reviewers to
rule on it explicitly:

- The cert-fingerprint check is **one** check covering MC *and* MH (a single
  `infra/docker/certs/fingerprints.json`). Escalating it is unambiguous.
- The listener check runs over **four** configmaps: `mc-0`, `mc-1`, `mh-0`, `mh-1`. The task says
  "the two MH checks." **If I escalate only the MH rows and leave MC at WARN, the identical
  silent-demo failure survives via MC** — the join dials MC for signaling before it dials MH, so
  an MC-side listener gap produces the same "joined, no audio" (in fact "never joined") with a
  yellow `!` the operator scrolls past. Same mechanism, same symptom, same story. **Proposal:
  escalate all four**, and rewrite the header's per-line annotations from "join only" to a single
  honest statement of what the script now considers critical path.
- Two sub-branches are **indeterminate**, not broken: `ss`/iproute2 absent, and no
  `WEBTRANSPORT_ADVERTISE_ADDRESS` found in the configmap. CLAUDE.md's "fail loudly; never mask"
  says an indeterminate result on a critical-path check is a HARD FAIL with a named remedy
  (`install iproute2`), not a pass. **Proposal: HARD FAIL, no bypass env-var.** A
  `DEV_WEB_SKIP_WT=1` escape hatch is exactly the "silently skip a failure to make progress"
  pattern the house rules forbid, so I am not adding one.

**The one real objection to all of the above, stated against my own proposal.** The script already
documents (lines ~245-248) that on a devloop cluster `setup.sh` patches the *live* ConfigMap while
the on-disk file goes stale — so the on-disk advertise address can point at a port nothing is
listening on even though the cluster is healthy. As a WARN that is a harmless yellow line; as a
HARD FAIL it **blocks a working setup**. My read: `dev-web.sh` is the host-side demo launcher for
the static topology (its header says so), devloop work does not go through it, and the honest fix
if that ever bites is to read the live ConfigMap — not to soften the check. But this is a real
false-positive risk that I am not entitled to wave away alone. **@client + @operations: this is
the question I most want a ruling on at Gate 1.**

**Also fixing, in the same file (one-line, in-scope, "fix don't defer"):** `--help` runs
`sed -n '2,29p' "$0"` — a hardcoded line range duplicating the fact "where the header ends." Any
header edit (including mine) silently truncates or over-prints `--help`. That is a
CLAUDE.md single-source-of-truth violation sitting in the exact lines I'm touching. Replacing it
with a derivation (print from line 2 until the first non-comment line) removes the drift class
rather than re-tuning the constant. I'd rather do this than ship an edit that leaves a fresh
trap behind.

**Bash self-test** `scripts/dev-web.test.sh`, wired into `scripts/layer3.sh` with a
`run_and_emit "dev-web-preflight-selftest" "${__here}/dev-web.test.sh"` line, sourcing the shared
assertion SSoT `scripts/lang/_test_helpers.sh` — @test, this is your item 2, and I agree a
Rust-only suite would leave the escalation unproven. Placed at `scripts/` (not
`scripts/guards/simple/`) for the documented reason the other self-tests are: `run-guards.sh`'s
`find simple -name '*.sh'` would otherwise auto-run it as a guard. Tests:
`--check` exits nonzero with the fingerprints file absent; exits nonzero when a configmap
advertises a port with no listener; exits nonzero when `ss` is unavailable; the header's
HARD-FAIL/WARN table matches the checks the body actually performs (a drift assertion — the
header contract and the code are two encodings of one fact); and `--help` output still ends at the
header's last comment line after my range fix.

### D. The secure-context pointer — I was wrong, and the anchor is already frozen upstream

**Revised after @dry-reviewer's P-DRY-1. My original §D was wrong and I've dropped it entirely.**

My first read matched the Lead's setup note: the section does not exist, so *someone* has to write
it, and since the prose is client-owned that someone is @client, in-loop. I sent @client a request
to author it. **That was a mistake, and I've told them to disregard it.**

What I missed: the section is **already assigned, with a frozen anchor**, in the story manifest.
Verified directly in `docs/user-stories/2026-08-27-hear-yourself-through-handler.md`:

- **Task 20** (client, deps 19) owns it: "add a bounded dev-launch and secure-context subsection at
  the frozen heading `## Secure Context and Media Setup` (anchor `#secure-context-and-media-setup`,
  no punctuation — operations' triage ladder and **infrastructure's dev-web.sh header pointer**
  both reference it)". My pointer *is* that named consumer.
- Task 20 is also told to "land your subsection first as the anchor that both dev-web.sh's header
  pointer and operations' ladder reference" — authoring is task 20's deliverable, not this task's.
- The story summary (line 98) repeats it: "Runbook subsection `## Secure Context and Media Setup`
  lands first as the shared anchor."
- My task is #6; task 20 depends on #19. So the anchor lands **after** me, by design.

**Consequences, and they are the right outcome rather than a compromise:**

1. **The pointer uses `docs/runbooks/client-dev-local.md#secure-context-and-media-setup`
   verbatim.** The anchor is settled upstream and is not mine to negotiate. Proposing `§2.5 Secure
   context` and delegating the spelling to @client in-loop, as I originally did, would have
   produced a **third** spelling and desynchronised operations' triage ladder, which is already
   written against the frozen one. A pointer whose target text I invent is exactly the drift this
   repo's SSoT rule exists to stop — and I nearly introduced it while writing a plan about
   premises drifting silently.
2. **`docs/runbooks/client-dev-local.md` comes off my Cross-Boundary Classification table.** I am
   not editing it. Neither @client authoring it here nor my stub fallback is correct: both create a
   second encoding that task 20 would then have to reconcile.
3. **The interim dangle is accepted, not tolerated-and-hoped.** Story open question #7 ("Runbook
   `#anchor` resolution in `dt-guard`?", raised independently by operations and observability) is
   recorded **Answered — TODO, not this story**, so no guard resolves runbook anchors and nothing
   fails on the gap between task 6 and task 20. Verified at line 178 of the story file, not assumed.
4. **The story already has @security's D1 correction baked in**, which is a good independent check
   on it: task 20's prompt requires the prose state "that an origin of the form
   `http://<org-subdomain>.localhost:5173` is potentially trustworthy in Chrome so it works over
   plain HTTP, that a non-localhost HTTP origin silently disables the pipeline and presents as
   joined-with-no-audio". So the precise claim lands from the story, not from my hand-off.
5. **One thing I must flag to @client rather than silently break** (P-DRY-1, third bullet): task 20
   is told to "keep the cert section consistent with dev-web.sh's stale-but-present fingerprint
   failure text" — and **I am rewriting that text** (§C). The wording I land becomes the source
   task 20 mirrors. @client needs to mirror the new HARD-FAIL wording, not the WARN-era wording
   they'd see if they read the file before this task merges. Sent explicitly.

### E. The no-insecure-flag invariant — promoted to a second guard

The story forbids any browser/test-harness setting that disables certificate validation or forces
an insecure origin, in any script, config, runbook, or test invocation.

**Note on this document:** the flag literals are deliberately **not spelled out anywhere in this
plan**. The guard's vocabulary table is their single home; every other mention points at it. That
is not squeamishness — it is the allowlist-surface argument in §E3 applied to my own output. A plan
doc that enumerates the tokens becomes a file the guard must then exempt, and each exemption is a
hole.

**E1. Verified state today.** A `git grep` over tracked files for the full family (Chrome/Chromium
cert-bypass and insecure-origin flags, the Playwright and WebDriver equivalents, and the Node TLS
opt-outs) returns exactly **one** hit: `docs/devloop-outputs/2026-08-03-playwright-e2e-harness/main.md`
line 114, which asserts the harness deliberately does *not* use them. The live config
(`packages/web-app/playwright.config.ts`, `launchOptions.args`) carries only the two ADR-0028
fake-media flags and a comment saying "NO cert-bypass flags" — it names no literal, so it will not
even trip a literal matcher. The invariant holds, and its rationale is already written down.

**E2. Decision — build it now, as a separate subcommand.** @security's B ("prose-only enforcement
of a standing prohibition is not enforcement") and @test's item 3 arrived independently and agree.
My original instinct was to defer this to a follow-up task; **I'm reversing that.** The reversal is
not deference to two reviewers agreeing — it's that their argument exposes a real asymmetry I had
mispriced: this half is *cheap* (one file walk, one vocabulary table, ~150 LOC) and its failure
mode is *expensive and silent* (cert trust here flows exclusively through `serverCertificateHashes`
pinning, so any one of these settings turns the pinning assertion into decoration while every test
stays green). Cheap-to-build, expensive-to-miss is exactly the wrong thing to defer.

It lands as **`dt-guard no-insecure-browser-flags`** with its own wrapper — *not* folded into
`release-build-profile`. One subcommand must mean one thing; a browser-flag policy filed under an
infra build-profile name is the kind of miscategorisation nobody finds later.

@team-lead: this is a **third deliverable beyond the task's two halves**, so it is your call, not
mine. It is cleanly separable — if you want the devloop kept to the stated scope, cut §E and I'll
raise it as a follow-up with @security as owner; nothing else in the plan depends on it.

**E3. Scope decisions, stated explicitly because @security asked for them.**

- **Vocabulary covers the property, not one spelling** (B1, adopted in full). Three families:
  Chrome/Chromium launch flags (cert-bypass, insecure-origin-as-secure, web-security-off,
  insecure-content, and the security-warning-suppression flag that gates several of the others);
  the Playwright context option; the WebDriver/Selenium capability. **@security's B1 point is the
  load-bearing one**: matching only the Chrome flags is bypassed by one boolean in
  `playwright.config.ts` with zero flags present and the guard silent — and this repo *uses*
  Playwright, so that is the cheap path, not the theoretical one.
  **Node TLS opt-outs (the reject-unauthorized env var and its agent-option twin): I am
  including them.**
  @security offered them as the droppable tier. I'd rather not drop them — the env-test and E2E
  harnesses are Node processes that talk to MC/MH, so this is the same threat with a different
  runtime, and the marginal cost is two more rows in a table. Flagging it as an *addition* to
  @security's minimum so it is a visible choice, not scope drift. If the false-positive rate on
  the agent option turns out to be real (it is a legitimate word in unrelated TLS code), I'll
  narrow to the env-var form and say so at Gate 2 rather than adding a broad exemption.
- **False positives are handled by a 2-entry literal-path allowlist, not a `docs/**` exclusion**
  (B2, adopted). @security is right that excluding docs wholesale deletes the highest-value
  coverage — a runbook telling a developer to paste the flag is precisely the leak. The allowlist
  is: (1) the guard module that *is* the vocabulary's canonical home, and (2) the single
  2026-08-03 devloop-output line above. Each entry carries an inline justification. **The
  allowlist is literal-path, never a prefix or glob** — so it cannot silently widen to cover a
  future real violation in a sibling file, which is the failure @security warned about.
  Test fixtures live in tempdirs, so **no test file needs an exemption at all.**
- **Git-tracked files only** (B3, adopted) — mirrors `28defd8`'s lesson on
  `validate-subdomain-regex-sync`. Untracked Playwright traces and `node_modules` otherwise
  produce noise or a bail that reads as a pass.
- **Non-zero-match assertion applies here too** (A2 generalised): if the walk matches zero
  candidate files, that is a FAIL, not a pass.

**E4. Unit tests** (B4): one positive case per family (Chrome flag / Playwright option / WebDriver
capability / Node TLS), one allowlisted-path case proving the exemption mechanism *works*, and one
proving it is **narrow** — the same literal in a sibling path under the same directory still fails.
That last test is the one that keeps the allowlist honest as the tree grows.

### F. Order of work

1. `release_build_profile.rs` + `lib.rs`/`main.rs` wiring + wrapper + unit tests (self-contained;
   no cross-boundary dependency; can start the moment the plan is approved).
2. Send @client the secure-context content + section-home request (in parallel with 1, so their
   prose is landing while I build the guard).
3. `dev-web.sh` escalation + `--help` range derivation + `scripts/dev-web.test.sh` + `layer3.sh`
   wiring — after the @client/@operations ruling on the devloop-stale false-positive risk (§C).
4. Pointer line into `dev-web.sh`'s header, last, against @client's actual anchor.

### H. Gate-1 reviewer input — what changed, and two adjudications

Five reviewers sent pre-plan input (@test ×2, @security, @observability, @code-reviewer). Most of
it is adopted as-is and already folded into §A–§E above. This section records only the deltas that
changed my design, and the **two places where reviewers contradicted each other** and I had to
rule rather than accept both.

**H1. ADJUDICATION — `cargo chef cook`: in scope, but as a *different* failure class.**
@security A3 wants the `cook` line covered ("a cook line that drops `--release` silently changes
what gets compiled in the cached dependency layer"). @observability #3 wants the matcher scoped to
`cargo build` only, because "the shipped binary comes from `cargo build`; chef cook dropping
`--release` is a cache-efficiency bug, not a premise break" — and warns that widening the match
produces a confusing token when cook alone drifts.

**@observability is right on the mechanism and @security is right on the coverage.** cargo-chef's
cook stage only pre-builds dependencies; if cook goes dev-profile while build keeps `--release`,
cargo rebuilds those deps in release anyway. The artifact is still correct — just slower to build.
So it is *not* a premise break, and filing it under the same token would mean a red build that
says "your release premise is broken" when it isn't.

Ruling: **check both lines, with two rule_ids and two REASON tokens.**
`dockerfile-build-not-release` = premise break. `dockerfile-cook-profile-mismatch` = cache-layer
drift, still a FAIL (it is real drift and CLAUDE.md says fail loudly) but a token whose name tells
the reader immediately that the shipped binary is fine. @observability's requested "say so in a
comment" becomes a module-doc paragraph explaining exactly this distinction, so the next reader
doesn't collapse the two.

**H2. ADJUDICATION — service-Dockerfile enumeration: glob *and* workspace members, bidirectionally.**
@code-reviewer #4 and @security A1 want a glob (`infra/docker/*/Dockerfile`) so a new service is
covered automatically. @observability #2 wants the set *derived from the workspace members* as
SSoT, noting `infra/docker/` also holds `certs/`, `grafana/`, `postgres/`, `prometheus/` with no
Rust build, and that "a new `xx-service/Dockerfile` must not silently escape a premise gate."

A glob alone misses the opposite failure: a new `crates/xx-service` that never gets a Dockerfile
premise-checked. Derivation alone misses a Dockerfile for something not a workspace member.

Ruling: **both directions, which is strictly stronger than either.**
1. Discover by glob; a Dockerfile containing any `cargo build` is in scope regardless of directory
   name (so non-Rust dirs like `postgres/` and `grafana/` self-exclude by having no cargo line —
   no hardcoded exclusion list to rot).
2. Derive the *expected* set from the `[workspace] members` in the root `Cargo.toml` — every
   member matching `crates/*-service` must have a matching `infra/docker/<name>/Dockerfile` under
   the gate. Missing one is its own FAIL token.
   `crates/dt-guard/src/common/services.rs::CANONICAL_SERVICES` exists and is the declared SSoT for
   the `{ac,gc,mc,mh}` enumeration — but it is a *metric-prefix* list, not a deployable-artifact
   list, and hardcodes four services. **I am deriving from `[workspace] members` instead**, and
   will say why in the module doc, so @dry-reviewer doesn't read this as a fork of the services
   SSoT: they answer different questions ("which services emit metrics" vs "which crates ship as
   an image"), and binding the premise gate to the metrics list would make adding a service to one
   list silently change the other's meaning.

**H3. Cargo.toml parsing — typed `toml` deser, and it adds a dependency.**
@code-reviewer #3: prefer `toml::from_str` into a typed struct over a line regex, and any regex
must sit in a `Lazy<Regex>` canonical-home static per `clippy.toml`.

Adopted — with a cost I want visible rather than discovered at Layer 6: **`toml` is not currently
in `Cargo.lock`**, so this is a genuinely new direct dependency and it will trip the ADR-0033
§3/§11 audit dep-change gate (`scripts/lang/_audit_gate.sh:audit_dep_changed_rust`).

I considered hand-rolling a line scanner to avoid that, and **rejected it**: TOML permits
`[profile] release.debug-assertions = true` as a dotted key and inline-table forms, so a
line-oriented scanner has a real miss window — and a premise guard with a miss window "reads as
coverage," which is the failure mode this whole task exists to prevent. Paying one audit-gate run
to make the parse total is the right trade. **@operations / @security: flag it now if a new
direct dep is a problem and I'll re-open the choice.** No regex is needed anywhere in the Cargo.toml
half; the Dockerfile half is line-oriented plain string scanning, also regex-free.

**H4. Vacuity is a first-class failure class** (@security A2, @observability #2, @test #2 — three
reviewers independently). Adopted in full, with a distinct token each: zero Dockerfiles
discovered; a discovered Dockerfile with no `cargo build` line at all; root `Cargo.toml` absent,
unparseable, or carrying no `[profile.release]` table. All FAIL, none OK.

This is the single most important correction to my draft. My §A framing was "assert the premises,"
and a guard that walks nothing and prints OK asserts nothing while *looking* like it does — the
same shape as the inert `compile_error!` this task was written to protect. **This deserves its own
unit test per case, not a shared one.**

**H5. REASON-token vocabulary and `--explain` are the operator-facing contract, and are tested as
such** (@observability #1/#5, @test #1/#3). One `rule_id` per failure class, not one for the guard;
`--explain` emits via `common::explain::print_finding` with `policy = "<subcommand>::<rule_id>"`
and `src_file: file!(), src_line: line!()` at the emit site; plain `Finding`, not the redacted
variant (nothing here is a credential). **Tests assert the token *string*, not just the exit code**
— @observability's argument is exactly right and I would not have written the tests this way:
pinning only exit codes lets the vocabulary collapse to one undifferentiated token with every test
still green. At least one `EXPLAIN:` assertion per `rule_id`, mirroring
`crates/dt-guard/tests/binary_status_surface.rs` for the STATUS-surface tier.

**H6. dev-web.sh: hard-fail branches get remedies, and the contract sentence gets rewritten, not
retagged** (@observability #6/#9). The three `nothing listening on …` branches in
`check_wt_endpoint` carry no `Fix:` line today — and those are precisely the branches I am
escalating, so I'd have shipped "turn red without saying what to do." Each hard fail now states:
what broke, why it matters **in the user's terms ("the demo joins and you hear nothing", not "join's
media step will fail")**, and the next command.

The header's contract sentence ("HARD FAIL = nothing works without it; WARN = only part of the demo
breaks") stops being true under this change — MH reachability is "only part of the demo" by that
wording, yet now hard-fails. Retagging the table while leaving that sentence would leave a contract
that contradicts its own rows. It is rewritten around the real predicate: **hard fail = the
demonstrated path silently produces no audio.** "Silently" is the operative word and the reason
this task exists.

**H7. `--help` is fragile, and THIS diff is what breaks it.** ⚠ **Superseded in part by §I17** —
I originally wrote here that it "already truncates today," repeating @code-reviewer's report without
measuring. @operations measured; it is false. Line 29 is the genuine last header line and
`sed -n '2,29p'` prints the header complete. The fix still lands, for the honest reason: my own
header edits push past line 29, and a derived range removes the drift class rather than re-tuning
the constant. Form is the bounded `awk` contiguous-comment-run variant, plus a sentinel assertion in
`dev-web.test.sh` (§I17).

**H8. The secure-context prose — @security's D1 corrects a claim I would have shipped.**
I had written that "a non-localhost HTTP origin disables the pipeline," which is directionally
right but imprecise in a way that misdirects a reader. The actual rule is *potentially trustworthy
origin* per the Secure Contexts spec: `https://`, plus `localhost`, `*.localhost`, `127.0.0.1`,
`[::1]`. **`http://demo.localhost:5173` IS a secure context** — which is exactly why the demo works
over plain HTTP today, and a reader told otherwise would go chase the wrong problem. The real
failure is a **non-loopback HTTP origin**: reaching Vite at `http://192.168.x.x:5173` or a LAN
hostname from another machine — which is precisely what this runbook's WSL2/Windows split tempts
people into (cf. F9). This correction goes to @client as part of the §D content hand-off.

**H9. And @security's D2 is why §D and §E are one control, not two.** The moment a developer hits
this on a LAN IP, the "obvious fix" they reach for is the prohibited insecure-origin flag. So the
prose must name the **approved** remedy in the same breath — use a `.localhost` name or a loopback
literal, or terminate real TLS. A section that describes the symptom without an approved remedy
measurably raises the odds the prohibited flag gets added later by someone who never read this
task. Corollary I'm enforcing on the hand-off: **the runbook section must not name the prohibited
flag at all**, even to reject it, because naming it both plants the idea and forces an allowlist
entry (§E3) that then has to be maintained forever.

**H10. Naming** (@code-reviewer #1). `release-build-profile` and `no-insecure-browser-flags` —
concrete, single-concern, no catch-all. Noted and appreciated that dt-guard's 32 existing
subcommands mean the ADR-0034 ≥10 re-debate threshold is a long-standing pre-existing condition
this task does not newly trip.

---

### I. Second reviewer wave — what changed again (and the two findings that broke my design)

@dry-reviewer, @operations and @security's addendum arrived after §H. Three of these are not
refinements — they are corrections to things I had *wrong*, and I'd rather say so plainly than
fold them in silently.

**I1 — @dry-reviewer P-DRY-1 killed my §D outright.** Covered above; §D is rewritten. The short
version: the anchor was frozen upstream in the story manifest and I was about to invent a third
spelling of it. I verified every claim in P-DRY-1 against
`docs/user-stories/2026-08-27-hear-yourself-through-handler.md` (task 20 text, the line-98 summary,
and open question #7 at line 178) rather than taking it on trust, and it holds in full.

**I2 — @operations OPS-8: my bash test suite was vacuous, and would have shipped green.**
This is the sharpest finding of the whole Gate 1 and it invalidated my §C test design.

Trace: `check_port` calls `fail()`, which sets `HARD_FAIL=1` **without exiting**; the WT checks run
regardless; the single `exit 1` is at the very end. So in CI, or on any machine with no cluster,
`dev-web.sh --check` **always exits nonzero** — because AC/GC on 8443/8444 don't answer. Every
assertion I planned ("exits nonzero when fingerprints are absent", "…when a configmap advertises a
dead port", "…when `ss` is unavailable") would have passed **with the escalation reverted**. A
suite that stays green when the feature it tests is removed is not coverage; it is decoration that
costs review attention. I would not have caught this from reading my own plan.

Redesign, adopting OPS-8's two asks:
- **Assert on the specific `✗` marker and message text of the branch under test**, never on the
  process exit code — `assert_marker` / `assert_no_marker` from `scripts/lang/_test_helpers.sh`,
  which I'd already named as the SSoT. The negative direction (`assert_no_marker` for the `!` WARN
  glyph on those branches) is what actually pins the escalation. @test stated the acceptance
  property precisely and I'm adopting it verbatim as the bar: **reverting WARN→HARD-FAIL must
  re-emit a WARN marker for those branches, which flips the `assert_marker '✗'` red.** A test that
  cannot distinguish the pre- and post-escalation states does not test the escalation.
- **Hermetic before wiring**: PATH-stub `curl` / `ss` / `getent`, temp repo root, `AC_PORT` /
  `GC_PORT` overrides — the technique `scripts/guards/run-guards.test.sh` already uses to PATH-stub
  `timeout`. **If I cannot make it hermetic, it does not get wired into `layer3.sh`.** A
  permanently-red Layer-3 step is worse for everyone on this branch than no self-test, and
  quarantining is not an available escape (ADR-0028).
- P-DRY-7's trap applies here: `report_results` requires every `FAILURES+=` entry to carry a
  `[label]` prefix, because `tee_collect_statuses` matches `^STATUS=` at line start — a raw failure
  diagnostic beginning `STATUS=` would vote into Layer 3's verdict. My header-contract drift
  assertion quotes header text, so it is exactly the shape that trips this. Routed through the
  helpers, `[label]` preserved.

**I3 — @operations OPS-4 corrected the Lead's setup note, and adds a file row.** I'd have looked at
§6.5. OPS-4 checked: §6.5 documents env-tests/Vitest/Playwright and does **not** describe the
preflight, so it doesn't go stale. What *does* go stale is **§3 Step 0** — the "Read its two
severities correctly" block (which after this change is actively wrong: there are no WebTransport
WARNs left, and the script no longer starts the server) and the "One limitation to know" block
(whose hazard **inverts** — post-escalation, a *red* preflight on a devloop cluster no longer means
the join is broken, and it now blocks launch). New table row, operations-owned, and it **must land
in the same commit as the escalation**: a merge where the runbook contradicts the script is the
drift the SSoT rule exists to prevent. I'll also check `packages/web-app/README.md:36-104` doesn't
restate the severity contract.

**I4 — @operations OPS-5(b): the operator lane I was implicitly designing for does not exist.**
`scripts/guards/run-guards.sh:classify_guard_exit` routes **only** exit 124 and 137 to
`PRECONDITION_FAILURE`; the default arm classifies every other non-zero exit as `FAILED`
(implementer lane). So a guard exiting 2 for "I can't find my inputs" gets reclassified as a diff
defect and lands on an implementer with nothing to fix. Achievable design instead: **carry the lane
in the reason token** (`release-build-profile-no-dockerfiles-discovered`,
`release-build-profile-workspace-manifest-unreadable` — distinct from the violation tokens) and say
in the message body that it is a discovery/precondition failure, not a diff defect, so
`docs/runbooks/devloop-validation.md` §6.3.1 triage reaches the right owner. This also satisfies
@observability's sharpened bar for constraint 1: **the test is not "are the tokens different
strings" but "could someone write the devloop-validation triage entry from the token alone."** I'm
adopting that as the acceptance bar for the vocabulary.

**I5 — @security A5/A6 and @operations' remedy findings, adopted.**
- **A6 `.cargo/config.toml`** is the strongest of the late additions and I'd under-weighted it: it
  defeats the premise **without touching root `Cargo.toml` and without touching any Dockerfile** —
  invisible to *both* halves as I'd specified them. Covered: if the file exists, it must not set
  `[profile.release] debug-assertions` nor `[build]/[target.*] rustflags` carrying it.
- **A5**: match `debug-assertions=yes|true|on|1` **and** the no-space `-Cdebug-assertions` spelling,
  and cover `CARGO_ENCODED_RUSTFLAGS` alongside `RUSTFLAGS`. One form matched is a bypass, not a guard.
- **The two rules have deliberately opposite scopes, and that gets a comment in the source.**
  The RUSTFLAGS rule is scoped to build/deploy **execution paths** (Dockerfiles, CI `env:` blocks,
  `.cargo/config.toml`) — `RUSTFLAGS` appears legitimately in prose in at least six tracked docs
  (mold linker, sanitizers) and `ci.yml:170` sets `RUSTFLAGS: --cfg coverage` benignly, all of which
  must not fire. The cert-flag rule is scoped to **include** docs and runbooks, because a runbook
  telling someone to paste a flag *is* the leak. @security is right that someone will later
  "harmonize" these two scopes and silently break one; the module doc says why they differ.
- **OPS-1 is the load-bearing operations ask and I'd have shipped it wrong.** Three of the branches
  I'm escalating (`nothing listening on …` — IPv4, IPv6, hostname) print **no `Fix:` line at all**.
  Tolerable as a WARN; as a HARD FAIL it is a stopped developer with nowhere to go. Every escalated
  branch now prints what broke, why it matters *in user terms*, and the next command
  (`./infra/kind/scripts/setup.sh` + `kubectl get pods -n dark-tower`; `sudo apt-get install -y
  iproute2` for the `ss` branch).
- **OPS-2**: the "no advertise address in `$file`" branch is a **different owner lane** — repo/config
  drift, not the developer's environment — and must say so, or a 3am reader spends an hour
  restarting a healthy cluster. Distinct message shape naming the lane and saying there is nothing
  to fix locally.
- **OPS-3** is the condition on operations accepting the devloop-stale false positive, and it is a
  fair price: the listener failure text names the known false-positive case and its ground-truth
  command, plus a `docs/TODO.md` entry
  recording that the on-disk read is now a **blocker** rather than a stale-green, with the
  live-ConfigMap read named as the fix if it bites. That entry is the *rollback plan* for this
  decision — knowing what "undo" looks like before shipping, not after.
- **OPS-6**: WARN-era wording ("sign-up/create work, but WebTransport JOIN will fail") understates a
  stop. Every escalated message re-worded to state the stop.
- **OPS-5(a)**: the zero-discovery assertion is stated explicitly for the release half, not left
  implicit in §E's flags-guard scope. Already in §B's rule table; called out here so it isn't read
  as flags-only.

**I6 — @operations verified the blast radius, which is worth recording so nobody re-derives it.**
Zero automated callers of `dev-web.sh` across `.github/`, `scripts/layer*.sh`, `scripts/workflow/`,
`.githooks/`, `infra/`, `packages/`. No CI gate, no Layer-7 step, no git hook goes red. The
counterweight: `dev-web.sh` is the *only* documented launch path (runbook §3 Step 3,
`packages/web-app/README.md:47`), so under WARN a developer with a broken WT setup still got a Vite
server for AC/GC work, and under HARD FAIL they get nothing. **A false positive now costs the whole
dev session, not just the join.** That is precisely why OPS-1's remedy lines are non-negotiable
rather than polish.

**I7 — @dry-reviewer's remaining placements, adopted.**
- **P-DRY-2**: a doc-comment sentence in `release_build_profile.rs` recording the boundary against
  story **task 23**, which lands "a real build, not a CI string check" of the `compile_error!`.
  Mine asserts the *premise* (shipped artifacts are built release); task 23 asserts the *control
  fires* given the premise. Complements, not duplicates — and without the note, whoever lands task
  23 can reasonably read this guard as superseded and delete it.
- **P-DRY-3**: no third TOML walker. `cite_extract.rs`'s `TOML_SECTION_RESOLVER`/`TOML_KEY_RESOLVER`
  answer a different question (does symbol X appear as a section-or-key) than mine (does
  `debug-assertions` hold a truthy value *within* `[profile.release]`). Real `toml` crate, per §H3;
  the doc comment says why those resolvers weren't reused so nobody collapses them later.
- **P-DRY-4**: the **floor vs. scan set** split — scan set stays the walk (a new non-service
  Dockerfile still gets checked), floor is a derived roster. See §I16 for where the floor's roster
  comes from: @code-reviewer's Blocker 2 showed `CANONICAL_SERVICES` alone would not satisfy
  "nothing to hand-maintain.
- **P-DRY-5**: `crates/mh-test-utils/src/lib.rs`'s doc comment already makes a load-bearing security
  argument resting on exactly this premise — it cites `infra/docker/mh-service/Dockerfile` lines 53,
  59, 78 and 135 by number, with nothing enforcing any of it. Verified in tree. My guard is its
  missing forcing function; one `ANCHOR (DRY):` line names it. (Those four line-number cites will
  rot on any Dockerfile edit and `cite-no-line-numbers` doesn't reach Rust doc comments — not my
  fix, but I'll add a `docs/TODO.md` line while I'm there.)
- **P-DRY-6**: the out-of-scope boundary is hoisted to a **module-level** scope statement covering
  all rules, not buried in the rustflags rule. Named non-encodings: `ci.yml:91` / `ci-client.yml:89`
  build `dt-guard`/`dt-story` **host** binaries (different artifact class), and
  `docs/BUILD_REQUIREMENTS.md` carries an illustrative Dockerfile. None are encodings of this
  premise; the statement exists so the next contributor doesn't "complete" the guard by adding them.
- **P-DRY-8**: the `--help` derivation is a first instance, not a duplicate — no extraction wanted.

**I8 — @observability's hardened constraints 4 and 8** are treated as settled, not open: the
bypass class is covered (rows 5/6 + `.cargo/config.toml`) with distinct tokens, and the `--help`
range is derived in-diff. Their asymmetry argument is the one I'd want on the record: the
`[profile.release]` table is the *documented* way to set debug-assertions, but the env-var and
`RUSTFLAGS` forms are what appear **when someone is debugging a production issue under time
pressure** — exactly when MH's `compile_error!` most needs to still be armed. A premise assertion
covering only the tidy path is inert in the scenario it exists for.

---

### I9. P-DRY-9 — the severity contract has THREE encodings, and the runbook already forbids the third

@dry-reviewer's late finding, verified in tree and adopted subject to @operations' ruling (their
hunk, their call).

`docs/runbooks/client-dev-local.md:257-268` does not merely *reference* the script's severities — it
**restates them**, including the per-check consequence: *"Sign-up and create-meeting run over TCP
through the Vite proxy and are unaffected by every WebTransport-related warning; only the join step
breaks."* That is the same fact as `scripts/dev-web.sh:11-20`'s bracketed `[WARN — join only]`
annotations, written twice in two files, going stale together for the same reason — which is why
OPS-4 exists at all.

**The DRY problem is what happens after OPS-4 is fixed.** My `scripts/dev-web.test.sh` drift
assertion covers header-vs-body. Nothing covers runbook-vs-header. Writing a *corrected duplicate*
leaves three encodings with a forcing function across only two of them, so the next severity change
re-opens this exact gap one file to the left — in the devloop whose entire subject is premises going
stale unnoticed.

**What makes this cheap rather than a redesign: the collapse is already declared, and the runbook
is already violating itself.** Two lines above the offending block, the same file says: *"Read
`scripts/dev-web.sh --help` for the current check list — it is maintained in the script and
deliberately not re-enumerated here."* And the script header says the complementary half at
`scripts/dev-web.sh:6-8`: *"The runbook stays the source of prose/diagnosis; this script is the fast
path."* So the per-check-consequence sentence contradicts a contract its own file states two lines
earlier. This is not a new policy — it is restoring one that is already written down and already
broken.

Shape: the runbook states the two-severity **concept** (what `✗` and `!` mean, and that the script
tells you which part broke) and points at `--help` as the enumerated source; the per-check
consequence sentence is **deleted, not corrected**. Prose in the runbook, enumeration in the header,
one home for "which check is which severity." Strictly less work than writing the corrected
duplicate, and the next escalation touches one file.

**Ordering interaction, which sharpens §H7 from nice-to-have to prerequisite.** This makes `--help`
load-bearing for a runbook pointer — so a truncated `--help` would now silently truncate the
runbook's answer too. **The range derivation lands first, in the same commit**, before the runbook
points harder at it.

Two things @dry-reviewer checked and explicitly did *not* raise, recorded so nobody re-opens them:
the "One limitation to know" paragraph is genuine diagnosis (the on-disk-vs-live ConfigMap hazard)
and belongs in the runbook — keep it, invert it per OPS-4; and its
`scripts/dev-web.sh::check_wt_endpoint` cite is symbol-resolved by `cite-symbol-resolves`, so a
rename during the escalation is already caught.

**RULED (@operations): adopt the DRY shape — delete the per-check-consequence sentence, don't
correct it.** @client had asked in parallel for the opposite (keep both copies, Gate-3 co-sign that
they match); @operations ruled against it and their reasoning is the sharpest argument in the loop:

> "A Gate 3 human co-sign confirming two files match **is exactly the 'someone has to notice'
> mechanism this devloop exists to replace.**"

ADR-0036 §11 chose `compile_error!` over a CI check on that reasoning and §A of this plan restates
it — accepting a notice-based control for the runbook in the same commit that removes one for the
build profile would be incoherent. Plus the asymmetry: a co-sign works exactly once, for the people
currently holding the context; the next severity change is made by someone who has no idea the
runbook restates the header. **Deletion protects that person; a co-sign protects only us.**

**⚠ CONDITION — the one coupling in this plan that can bite silently, so it is recorded loudly.**
Deleting the runbook's copy trades *redundancy* for a *forcing function*. The chain becomes:

`runbook §3 Step 0` → points at → `--help` → derives from → `script header` → drift-asserted against
→ `script body`

**Exactly one automated link in that chain: my `dev-web.test.sh` header-vs-body drift assertion.**
If OPS-8's hermeticity escape hatch fires — the self-test can't be made hermetic, so per §I2 it does
not get wired into `layer3.sh` — then that link does not run, and we would have deleted the
redundancy while gaining nothing. **Strictly worse than today.**

So: **if the self-test does not land wired, I go back to @operations BEFORE writing the runbook
hunk.** Their fallback in that case is the corrected-duplicate shape plus a `docs/TODO.md`
§Cross-Service Duplication (DRY) entry, accepting the deferral honestly rather than shipping a chain
with no forcing function in it. This is an implementation-time decision point, not a plan-time one,
and it is the single thing most likely to be forgotten mid-implementation — hence its own callout.

### I10. Where the flags vocabulary lives, if §E is approved

@dry-reviewer answered my §E siting question: **its own module, not `secret_patterns.rs`.**
`HYGIENE_PATTERNS` is a table of credential-shaped literals with two live consumers
(`alert-rules-policy`, `secret-scan`); a browser trust-bypass flag is a CLI *capability*, not a
secret. Folding it in would silently widen both existing consumers' scan surface —
`alert-rules-policy` would start reporting browser flags in alert YAML, which is meaningless there —
and every future false positive in the new vocabulary would become a false positive in two unrelated
guards. Concept-collapse wearing DRY's clothes.

Right precedent is `ts_dev_trust.rs::FORBIDDEN_LITERAL`: a policy-scoped const next to the single
policy that consumes it; extract to a shared table only when a second consumer actually appears. The
doc comment says why it is **not** in `secret_patterns.rs`, for the same reason P-DRY-3's note
explains why `cite_extract`'s TOML resolvers weren't reused — otherwise the next reader collapses
them on sight.

---

### I11. OPS-3's remedy command was wrong, and the fix generalises

@operations self-corrected the `kubectl` command they gave me in OPS-3, which I had already adopted
**verbatim** into the plan. Verified in tree: the ConfigMap keys are **service-prefixed** —
`infra/services/mh-service/mh-0-configmap.yaml:19` is `MH_WEBTRANSPORT_ADVERTISE_ADDRESS`,
`infra/services/mc-service/mc-0-configmap.yaml:19` is `MC_WEBTRANSPORT_ADVERTISE_ADDRESS`.

The unprefixed jsonpath doesn't error — it returns **empty output**, which a developer reasonably
reads as "the live ConfigMap has no advertise address either." That is precisely the false
conclusion the remedy line exists to prevent. **A remedy command that fails silently is worse than
no remedy**, and it is the same failure shape as everything else in this task: not wrong-and-loud,
but wrong-and-quiet.

Adopted:
- Emit the command matching the **failing label** (`mc-0`/`mc-1`/`mh-0`/`mh-1`) rather than a
  generic line. The label is already in scope in `check_wt_endpoint`, so deriving the key prefix
  from it costs nothing and keeps the operator from hand-editing a command at 3am.
- **A self-test asserting the emitted remedy command contains the service-prefixed key.**
  @operations' framing is the right one and worth recording: this class of defect — a remedy string
  that looks right and resolves to nothing — is invisible to every other check in the pipeline. It
  is also, as they noted, the defect they had just made themselves. That is the argument for
  testing remedy text as an output, not treating it as prose.
- Do-not-"fix" note: `scripts/dev-web.sh:277`'s `grep -E 'WEBTRANSPORT_ADVERTISE_ADDRESS'` is
  **unanchored**, which is why it matches the prefixed keys correctly today. Recorded in a comment
  so nobody tightens it into an anchored match and breaks the parse.

**This is the strongest argument in the loop for the OPS-1 remedy-line requirement**, and it cuts
against my own instinct to treat remedy text as documentation. Every escalated branch now prints a
command; if those commands are not exercised, the escalation converts silent-no-audio into
silent-wrong-remedy. The `dev-web.test.sh` suite therefore asserts remedy content, not just the `✗`
marker.

### I12. OPS-4 prose received, and it resolves P-DRY-9 by construction

@operations authored the §3 Step 0 replacement and it **deliberately does not enumerate which
checks are WARN** — their reasoning: Step 0 already establishes that `scripts/dev-web.sh --help` is
the SSoT for the check list and is "deliberately not re-enumerated here," so enumerating severities
in prose would recreate the drift class §H7 removes from `--help`.

That is P-DRY-9's collapse, arrived at independently. The three encodings become two with a stated
division of labour, and no `docs/TODO.md` deferral entry is needed. Three things @operations marked
load-bearing that I will not smooth away in voice-matching: (a) HARD FAIL states the script does not
start the server, (b) the hazard-**inverts** framing, (c) the live-ConfigMap command with the
prefixed key.

@client had proposed a Gate-3 co-sign that the §3 Step 0 prose and the `dev-web.sh` header block
say the same thing. **@operations ruled that shape out** (see §I9): under the adopted shape they are
no longer two copies of one contract, so there is nothing to co-sign. The runbook states the
*concept* and points at `--help`; the header holds the *enumeration*. @client's underlying goal —
no drift in the severity contract — is better served by having one copy than by two synchronized
ones, which is the same argument this whole task makes about premises.

@operations' revised hunk also folds in a good @client catch I'd have missed: the runbook should
name the **positive** validation path on a devloop cluster ("the validation path there is the Rust
env-tests, §6.5") rather than only saying "don't trust this." A limitation paragraph that leaves the
reader with no way forward is the documentation form of a hard fail with no `Fix:` line.

---

### I13. The dangle window IS the risk window — the header pointer carries substance, not just a cite

@security's follow-up, and it is the best argument made about §D. I had settled the pointer question
as "cite the frozen anchor verbatim, accept the interim dangle" — procedurally correct, and I
stopped thinking there.

What I missed is **who follows that pointer.** By construction it is someone who just hit my new
HARD FAIL, or is staring at "joined, no audio." They follow it, find no such section (task 20
depends on task 19, so the window is several devloops wide), and **that is precisely the moment the
prohibited insecure-origin flag gets pasted in.** The dangle window and the risk window are the same
window. Worse: my own §E guard would then catch it at *their* commit — after they have already run
it locally and formed the belief that it is the fix. Catching it there is catching it at the most
expensive possible point.

@security also confirmed independently what §D relies on: no guard resolves markdown anchors
(`validate-doc-citations-symbol-resolves.sh` and `-no-line-numbers.sh` cover symbol resolution and
line-number cites, not headings), so nothing catches the dangle and **nothing will tell me when it
lands** either.

**Adopted: the header line carries the substance, then cites the anchor for detail.** Media is
secure-context gated; use a `.localhost` origin or a loopback literal; a LAN-IP HTTP origin silently
disables getUserMedia / WebCodecs / WebCrypto / WebTransport and presents as "joined, no audio";
details at `docs/runbooks/client-dev-local.md#secure-context-and-media-setup`. Two lines instead of
one — fine, since §H7 derives the `--help` range anyway.

Three properties that make this strictly better than the cite-only form and cost nothing:
it is **my own file**, so no cross-boundary edit and no dependency on task 20; it is **accurate
before and after** task 20 lands; and it puts the **approved remedy** in front of the reader at the
moment of failure rather than one hop away, which is @security's D2 requirement applied at the point
it actually bites. The D2 corollary still holds: the prohibited flag is **not named**, here or in
the runbook.

### I14. Forward-compat: the vocabulary must not fire on task 20's sanctioned flags

@security's second item, and it is a real trap in the §E design if @team-lead approves it.

Task 20's prompt **requires** the runbook to tell a developer with no microphone to launch Chrome
with `--use-fake-device-for-media-stream` and `--use-fake-ui-for-media-stream`. Those are sanctioned
— they inject synthesized capture and auto-grant the mic permission; they do **not** disable
certificate validation and do **not** force an insecure origin. They already live at
`packages/web-app/playwright.config.ts:60`.

So a vocabulary built on **shapes** (`--use-fake-*`, a broad `--disable-*` / `--allow-*` sweep, or a
"chrome flag in a runbook" heuristic) would fire on task 20's required runbook text *and* on the
existing Playwright config — and the fix under time pressure would be a broad allowlist entry that
also covers real violations. That is the allowlist-widening failure §E3 is built to avoid, arriving
through the front door.

**Design consequence: exact literals per prohibited flag, never shapes.** Plus an explicit
**negative** unit test asserting `--use-fake-device-for-media-stream` and
`--use-fake-ui-for-media-stream` do **not** match — pinning the sanctioned/prohibited boundary where
a future maintainer will actually read it.

Worth noting the tension with §J3, since it is real rather than resolved: @security's S2 argues
(correctly, via §11) that a literal vocabulary is inherently incomplete and must be *claimed* as
"covers these N channels." Here the same literalism is what keeps the guard from false-firing on
sanctioned flags. Both are true: **literal-exact is right for this rule, and the coverage claim must
stay honest about being enumerative.** The two constraints point the same way — narrow matching,
modest claim — rather than against each other.

Also: task 20 states the prohibition slightly **wider** than my task does ("disables certificate
validation, **disables web security**, or forces an insecure origin"). The web-security-disabling
row already covers the middle clause. **Do not narrow below task 20's statement** — recorded so a
later scope trim doesn't silently create a gap between two tasks in the same story.

---

### I15. @observability's row-5 audit — one coverage gap, four matcher escapes, and a mechanism correction

The most technically detailed review of the loop, and it found a real gap. All seven items adopted.

**(d) — CONFIRMED GAP, promoted to row 7.** You did not misread the scoping: row 2 was scoped to
the workspace root `Cargo.toml`, row 5 to `rustflags` *within* `.cargo/config.toml`. Cargo also
honours a full `[profile.release] debug-assertions = true` **table** in `.cargo/config.toml`, and
that spelling fell between the two rules. Same property that made me promote row 3 — it flips
`debug_assertions` with every file the task names byte-identical — and it is the same TOML parse I
am already doing for rows 2–4, pointed at a second path. Now row 7, and it satisfies @security's
condition J5.3 (one parser, both files) by construction rather than by discipline.

**(a) — the split-array form is the idiomatic spelling and would have escaped.** In
`.cargo/config.toml`, `rustflags = ["-C", "debug-assertions=on"]` puts `-C` and its value in
**separate array elements**. Any matcher looking for `-C` adjacent to `debug-assertions` in one
string misses it — and it is the form the cargo book shows, so it is what people actually write.
Parse the array and join elements before matching; dedicated unit test in split form. This is
exactly the "guard reports clean while the offending spelling ships" failure the plan quotes ADR-0036
§11 about, and I had it in my own rule.

**(b) — four more valid spellings, all covered:** `-Cdebug-assertions=on` (no space);
`--codegen debug-assertions=on` and `--codegen=debug-assertions=on` (long forms); and
**`-C debug-assertions` with no value at all**, which rustc treats as *enabled* — the shortest
possible bypass, and one a matcher keyed on `=<truthy>` misses entirely. Truthy set is rustc's:
`y`, `yes`, `on`, `true` (falsey: `n`, `no`, `off`, `false`), not my narrower `on|true|1`.

**(c) — `CARGO_ENCODED_RUSTFLAGS` is `\x1f`-delimited**, not space-delimited, so a whitespace scan
cannot see the argument boundary. Its own split and its own test. As @observability notes, it is the
variable someone reaches for precisely when they are scripting a build.

**The mechanism correction, which is the most useful item here.** I scoped row 5 to execution paths
and treated the docs exclusion as load-bearing. @observability surveyed all 12 tracked `RUSTFLAGS`
occurrences — `--cfg coverage` (`ci.yml:170`), mold linker (3), sanitizers (8) — and **none contains
`debug-assertions`.** So a **value-keyed** matcher fires on exactly zero of them *with no exclusion
list at all*.

**Confirmed: value-keyed is primary; the execution-path scope is secondary belt-and-suspenders.**
That inverts what my plan implied and it matters, because an exclusion list is a thing that rots
while a value key does not. The two-rules-opposite-scopes comment stays regardless (someone will try
to harmonize RUSTFLAGS-excludes-docs with cert-flags-includes-docs), but it is no longer the thing
preventing false positives. My own line — "a guard that false-fails gets bypassed" — applied to me.

**Cook message body, adopted.** `dockerfile-cook-profile-mismatch`'s body states affirmatively that
**the shipped binary is unaffected — this is build-cache layer drift**. The token name alone reads
alarming to someone who does not know cargo-chef's caching model, and the person triaging a red
Layer 3 at 3am is exactly that person: "without it, it passes the bar for you and me and fails for
the reader." Also confirmed it is a `cook != build` **consistency** check, not a `cook must be
release` check — the invariant is that the two lines agree.

**(e) — `CANONICAL_SERVICES` is being repurposed across a semantic boundary, and the invariant is
written down nowhere.** `common/services.rs:18` documents it as `(metric_prefix, directory_name)`
for services **that emit metrics**; my floor uses it as "services that ship a container." Those sets
coincide today, but the invariant "every canonical service both emits metrics and ships a container"
is now load-bearing and unrecorded — so someone editing that array for a metrics reason would
silently move a release-premise gate's floor. Fix: extend that doc comment to name both consumers
and state the invariant, plus a word that the Rust array is authoritative for this gate (its Bash
mirror at `scripts/guards/common.sh:343` is a known un-guarded duplicate, tracked in `docs/TODO.md`
§DRY). Note this makes `crates/dt-guard/src/common/services.rs` a new touched file — in-domain
(Mine), one doc-comment edit.

**(f) — name and pin the OK token; print counts.** Ten FAIL tokens and no OK reason was an
oversight: `current_tree_premise_holds` asserts OK, so an undeclared OK string could drift with
nothing noticing. Named, pinned by test. And a normal stdout line before the STATUS line carrying
**counts** — "4 Dockerfiles, 7 inputs, 0 hits" — never in the REASON token, which stays stable and
space-free. That makes the anti-vacuity property **visible to a reader without reading the source**,
which is the entire point of the vacuity work and something no token can convey.

**(g) — the test that guards the guard needs its own anti-vacuity assertion.**
`current_tree_premise_holds` resolves the repo root by walking for `.git`; if that walk fails and the
test silently passes, it is the vacuous pass one level up. It asserts the root was found **and** that
at least the expected number of Dockerfiles were scanned. Same reasoning as the guard, applied to the
test — and I would not have thought to apply it there.

---

### I16. Blocker 2 — the anti-vacuity floor cannot rest on a hand-maintained list

@code-reviewer asked the question I should have asked myself: **where does the floor's roster come
from?** I had written "nothing to update by hand," and that was not literally true.

`crates/dt-guard/src/common/services.rs::CANONICAL_SERVICES` is a hand-maintained literal
`[("ac","ac-service"), ("gc","gc-service"), ("mc","mc-service"), ("mh","mh-service")]`. Using it as
the floor means a service added later never enters the floor — so **the anti-vacuity check itself
becomes vacuous for exactly the new service.** That is the fail-loudly violation one level down,
inside the mechanism built to prevent it. @observability's (e) reached the same place from the
semantic side: that array is documented as "services that emit metrics," and I was silently
repurposing it as "services that ship a container."

**Resolution — derive the floor, and guard the array against the derivation:**

1. **Floor = `[workspace] members` matching `crates/*-service`.** A true SSoT: cargo itself fails
   to build if it is wrong, so it cannot rot unnoticed. This restores the derivation from my
   original §H2 that P-DRY-4 superseded — the two reconcile because P-DRY-4's real contribution was
   the *floor vs. scan set* distinction, not the specific roster source.
2. **New rule `canonical-services-roster-drift`:** FAIL when `CANONICAL_SERVICES`'s directory names
   disagree with that derived set.

Rule 2 is a better answer to @observability's (e) than the doc comment I had promised them. Their
concern was that "every canonical service both emits metrics and ships a container" is load-bearing
and written down nowhere, so a metrics-motivated edit could silently move a release-premise gate's
floor. **A doc comment asks a human to notice; this makes the invariant machine-checked** — the
thesis of this whole task. The doc comment still lands, as navigation rather than as the control.
And `CANONICAL_SERVICES` goes from an *unguarded* mirror of the service roster to a *guarded* one:
a net reduction in drift surface, not the new duplication @code-reviewer was right to flag.

**Clarification confirmed:** every `common/services.rs` reference in this plan means
`crates/dt-guard/src/common/services.rs` — dt-guard-internal, Mine. **Not** the shared
`crates/common` crate, which would be a different ownership story under ADR-0024 §6.4. The table row
now spells the full path.

(Known, not mine: the Bash mirror at `scripts/guards/common.sh:343` is an un-guarded duplicate
tracked in `docs/TODO.md` §DRY. Rule 2 makes the *Rust* array authoritative for this gate; the
cross-stack collapse stays that TODO's business.)

### I17. `--help` is fragile, NOT broken — correcting a claim I propagated

@operations measured the header rather than taking the claim forward, and they are right. I verified
independently: `scripts/dev-web.sh` line 29 is `# AC_PORT / GC_PORT env if your overlay differs.` —
the genuine last header line — and line 30 is the first truly-empty line. **`sed -n '2,29p'` prints
the complete header today. Nothing is lost.**

@code-reviewer reported it "already truncates at line 30," I adopted that without measuring, and I
repeated it to three reviewers and to @team-lead as a "live defect." That was wrong and it is
corrected here and in every message I sent. The mechanism @code-reviewer identified is real — the
apparently-blank header lines (3, 9, 18, 21, 26) are `#`-prefixed, which is what makes the block
contiguous — but the consequence was not.

**This matters beyond bookkeeping.** A commit message claiming to repair a defect that does not
exist misleads whoever reads it later, and precision about what is and is not broken is the entire
subject of this devloop. Taking @operations' framing, which is a complete justification without the
embellishment:

> **This diff is the change that breaks it.** Adding the secure-context pointer lines and rewriting
> the HARD-FAIL/WARN contract block grows the header past line 29, at which point `2,29p` silently
> starts dropping the tail — including part of the severity contract the runbook now points at as
> SSoT. Deriving the range removes the drift class instead of re-tuning the constant to a new magic
> number the next header edit breaks again.

**The derivation's own failure mode gets a mechanical check, not a Gate-3 eyeball.** @client spotted
that any derivation keyed on "first blank line" or "contiguous comment run" truncates silently if
someone inserts a **bare** empty line mid-header, and proposed eyeballing it at Gate 3. @operations
declined, on grounds I find compelling and self-consistent: they had just ruled against @client's
synchronize-and-co-sign proposal because a control requiring someone to notice is the control class
this devloop replaces — so accepting a Gate-3 eyeball as the guard on the mechanism that ruling
depends on would be incoherent. Since the runbook now points at `--help` as the severity SSoT, a
silently-truncated `--help` silently truncates the runbook's answer.

Adopted: `scripts/dev-web.test.sh` **asserts `--help` output contains a sentinel string from the
LAST line of the header block.** One `assert_marker`, and it is the only thing standing between a
whitespace edit and a truncated contract.

**Form: taking the `awk` variant**, `awk 'NR==1{next} /^#/{print; next} {exit}'`. It defines the
block by what it *is* (the contiguous comment run) rather than by what follows it — bounded by
construction, no trailing blank, and no EOF-runaway. @client's `sed -n '2,/^\$/p'` works today but
has an unbounded failure mode @operations flagged: with no empty line after the header, sed prints
to EOF and `--help` dumps the whole script. Not reachable today (line 31 is `set -euo pipefail`),
but "not reachable today" is exactly the kind of premise this task exists to stop relying on.

### I18. Q4 approved — and the Lead's reason is better than the one I argued

@team-lead approved `no-insecure-browser-flags` for this devloop. Worth recording that the decisive
argument was neither mine nor @security's cost/benefit framing:

> The prohibition is *already in the task text as a hard requirement*. It is not a third deliverable
> someone bolted on — the only question ever open was whether it lands as prose or as a mechanism.
> Landing it as prose would make it a control that requires someone to notice, in the same commit
> where we remove exactly that control class for the build profile.

That reframes it from a scope question to a consistency one, and it is the same argument
@operations used to rule against the runbook duplicate. I had been treating it as an optional
extra; it was never optional, only ambiguously encoded.

Four binding conditions, all already in the plan: exact literals never shapes (§I14); a negative
test pinning `--use-fake-device-for-media-stream` / `--use-fake-ui-for-media-stream` as
non-matching (§I14); its own module, never folded into `secret_patterns.rs` (§I10); and **do not
narrow below task 20's wider statement** — task 20 prohibits flags that disable certificate
validation, **disable web security**, *or* force an insecure origin. Three clauses; my task text
states two. Covering all three, because a gap between two tasks in the same story is precisely the
drift this devloop is about.

Also noted from @team-lead's read of task 20, constraining my inline header substance (§I13): the
runbook must state that `http://<org-subdomain>.localhost:5173` **is** potentially trustworthy in
Chrome and works over plain HTTP. My header lines must not contradict that — so the inline substance
says *non-loopback* HTTP origin, never "HTTP origin." Same correction @security made in D1, now
binding from two directions.

---

### I19. Q4 hardening — the flags guard inherits the release half's discipline

@team-lead made two @observability items binding on the flags guard, and @security added a
self-consistency point I had not considered. Plus @semantic-guard's two pins, which close the one
gap my §E4 test design left open.

**I19a — distinct tokens per bypass class, not one fused token.** Three classes, three remedies, in
three different *places*: a cert-validation-disabling launch flag, an insecure-origin-forcing launch
flag, and a **config property**. `packages/web-app/playwright.config.ts:60` carries the sanctioned
fake-media args inside `launchOptions.args`, whereas the Playwright ignore-HTTPS-errors option would sit in the
`use:` block as a property with zero flags present. **A fused token cannot tell a reader whether to
look in an args array or a config property** — which fails @observability's "write the triage entry
from the token alone" bar exactly as it would have on the release half.

**I19b — vacuity applies here too, and it is the easiest one in the plan to miss.** This guard's
green *today* literally is "found nothing": the invariant holds by convention, and the only tracked
hit is a note explaining the deliberate absence (§E1). So if the walk finds zero browser-config or
test-invocation files, that must **FAIL**. A guard whose passing state and whose broken state look
identical is the pathology this devloop is named after.

**I19c — `scripts/dev-web.sh` is inside this guard's own scan surface (@security).** It is a script,
and the rule's scope is scripts/configs/runbooks/tests. So my expanded header **must not name a
prohibited flag**, or it trips the guard landing in the same commit — and the pressure-fix would be
an allowlist entry for `dev-web.sh`, the single last file that should ever have one. This is a
mechanical reason the §D2 corollary (never name the flag, even to reject it) is non-negotiable
rather than stylistic, and it applies to my own diff first.

Two escalation triggers now bracket the vocabulary from opposite sides, which is a good place to be:
**@client escalates a false positive that would block task 20; @security escalates a broad allowlist
added to silence one.** Exact literals plus the negative tests is the only outcome that satisfies
both — narrowing to stop a false positive and widening to catch a real flag both get caught.

**I19d — @semantic-guard's two pins, adopted, and they close a real gap.** My §E4 tests prove the
*allowlist* is narrow (same literal in a sibling path still fails). They do **not** pin the
sanctioned/prohibited boundary, and @semantic-guard is right that this is the inertness risk running
the other way:

> If the vocabulary is ever "harmonized" to treat any `--use-fake-*` shape as suspicious, the guard
> fires on `playwright.config.ts` and on task 20's required runbook text, and the tempting fix is a
> broad allowlist entry for those files — which then masks a real cert-bypass flag added to the same
> file later. §E4 as written stays green through that entire regression.

1. **Negative unit test on vocabulary NON-MEMBERSHIP**: a config/invocation line containing each
   sanctioned fake-media flag yields zero hits **with no allowlist entry covering it**. Passing on
   non-membership rather than on exemption is the whole point — it proves the vocabulary is
   property-scoped, not `--use-fake-*`-shaped.
2. **A real-tree "flags guard clean today" test**, parallel to the release half's
   `current_tree_premise_holds`, running against the actual repo root — where
   `playwright.config.ts` already carries both sanctioned flags *and* the "NO cert-bypass flags"
   comment — and expecting OK. **That is the differential that goes red the day someone widens the
   vocabulary and reaches for the broad allowlist entry.**

**I19e — the opposite-scoping comment must give the reason, not just the rule** (@observability,
adopted as standard by @team-lead). The comment says *why* the RUSTFLAGS rule excludes docs while
the cert-flag rule includes them — because a runbook telling someone to paste a flag *is* the leak,
whereas `RUSTFLAGS` in a mold-linker recipe is not. **A comment recording the rule without the
reason gets deleted by the same person who would tidy the asymmetry away.**

**I19f — row 7 verified, as @team-lead asked.** The scoping half is checkable in-tree and I checked
it: my own row 2 is scoped to the workspace root `Cargo.toml` and row 5 to `rustflags` *within*
`.cargo/config.toml`, so a `[profile.release]` **table** in `.cargo/config.toml` was covered by
neither. The cargo-semantics half rests on Cargo's config-profile support (`[profile.<name>]` tables
are honoured from `config.toml`, stable since Cargo 1.43), which is what makes the spelling
effective rather than inert. @observability did not misread it. Row 7 stands: one rule_id, one
token, one test, against a TOML parse the guard already performs — and it has the property that got
row 3 promoted, defeating the premise with every file the task names byte-identical.

---

### I20. The OK token is itself a coverage claim — §J's modest claim has to live where readers look

@observability's last item, and it is the sharpest version of §J. Adopted.

§J commits the *module doc comment* to saying "covers these N channels," never "the release premise
holds." But **the doc comment is the thing a reader doesn't see.** What they see is the STATUS line.

`STATUS=OK REASON=no-insecure-browser-flags` reads as a universal claim — *there are no insecure
browser flags*. The true statement is *none of the N enumerated spellings appear in the scanned
set*. Those differ exactly where it matters: at the N+1th spelling. A reader citing a green Layer 3
as evidence that the prohibition holds would be making precisely the overclaim §J warns about — and
**the token would have told them to.**

It bites harder on the release half, because §J1 already commits me to stating that the guard does
nothing for anyone building outside the pipeline. An OK reason reading as "the release premise
holds" would contradict that commitment in the one place a reader actually looks.

**Adopted: the OK token names the enumeration, not the conclusion.** Shape is "these enumerated
inputs were checked and were clean," never "the property is true." The property that matters, in
@observability's words: *a reader must not be able to derive a universal claim from a green.*

The §I15(f) counts line is the natural companion and already does most of the work — "4 Dockerfiles,
7 inputs, 0 hits" makes the enumeration **visible rather than asserted**, because a number is
inherently a scope statement. The token just must not undercut it. Together they are the honest
green: a scope, a count, and a claim bounded by both.

This is the same defect class as the overclaiming doc comment, one layer out. §J fixed the prose
claim; §I20 fixes the claim that actually gets read.

---

### J. What this guard does NOT claim (@security S1/S2 — folded in as text, because the wording *is* the deliverable)

@security's closing input changes what the guard **claims**, not what it does. It is the sharpest
framing in the whole Gate 1 and it goes into the module doc comment verbatim in substance, because
in a guard the coverage claim is a load-bearing artifact — an overclaiming doc comment is the same
defect class as a vacuous pass.

**J1 — this guard is exactly the category ADR-0036 §11 declined.** §11: *"A development-only
per-frame tracing feature is guarded by a compile error in release builds, **not by a CI check**. A
control that has to notice fails silently for anyone building outside the pipeline; a compile error
has nothing to notice."* Mine **is** a CI check. That's not a contradiction — the compile error
cannot assert its own premise, so something has to, and a check is the only thing that can — but
the doc comment must say plainly: **this guard does nothing for anyone building outside the
pipeline.** A local `cargo build`, a dev container, a fork's CI. §11's own closing line is why:
*"A guard reporting clean while the offending line ships reads as coverage."* The guard protecting
§11 must not exhibit §11's named pathology.

**J2 — S1: the control being protected does not exist yet, and the prose must say so.** I noted in
§A that `grep -rn compile_error crates/mh-service/src/` is empty; @security independently confirmed
no `compile_error!` and no `debug_assertions` usage anywhere in `crates/`. So the justification text
says this guard protects a premise for a control **specified in ADR-0036 §11 and not yet
implemented** — not that it protects an existing one. Otherwise the guard's own prose asserts a
false state, which is the exact failure mode the guard exists to prevent. (And the ordering is good,
not awkward: premise first, control second.)

**J3 — S2: six rows is a bypass *vocabulary*, and vocabularies are incomplete by construction.**
§11's last paragraph makes precisely this argument in a different context: *"Vocabulary additions
cannot be cited as the protection... the directory-scoped deny catches by shape."* So the failure
text and doc comment say **"covers these N channels"**, never *"the release premise holds."* The
difference is not pedantry — the second sentence is the one that lets a future reader stop looking.

**J4 — the shape-based version, named as a follow-up rather than allowed to evaporate.**
@security's suggestion observes the *effective cfg* instead of its causes, and is total by
construction — it catches all six channels and ones nobody has enumerated:

```rust
#[cfg(all(feature = "release-artifact", debug_assertions))]
compile_error!("release artifact built with debug_assertions enabled");
```

with the service Dockerfiles passing `--features release-artifact`. This is §11's own technique
applied to the premise rather than to the tracing feature — "a compile error has nothing to notice."

**I am deferring it, and @security explicitly allowed a deferral if it is task-sized. It is**: it
touches four service crates (`ac`/`gc`/`mc`/`mh` — three of them other specialists' domains), their
`[features]` tables, and four Dockerfiles, and the feature-plumbing shape wants media-handler's
sign-off since §11's control is theirs. Naming the mechanism concretely, per the deferral
condition, so it is a task and not a vanished idea. **The two are complements, not alternatives**:
mine catches the source change early with a readable message and covers all four services at review
time; the cfg assertion catches the *artifact* totally but only says "compile error." Landing mine
does not reduce the value of landing that.

**J5 — three conditions on the approved `toml` dependency**, all accepted:
1. **If the new subtree needs any audit suppression to go green, I stop and escalate to @security
   and @operations.** Fix-not-suppress; a dep that can only pass by suppression is a different
   decision than the one approved.
2. The parse is **total** — unparseable `Cargo.toml` FAILs, never skips. (Already a token; restated
   because it is the entire justification for the dep.)
3. **The same parser handles `.cargo/config.toml`** (A6). Two TOML parsers in one guard would
   reintroduce internally the divergence the dep exists to eliminate.

@security's decisive argument is worth recording because it is better than my own: the guard must
model the same semantics as the consumer whose behaviour it asserts about. The artifact is
determined by how *cargo* reads `Cargo.toml`; the `toml`/`toml_edit`/`winnow` family is what cargo
itself parses with. A hand-rolled scanner is a second, divergent implementation of that grammar, and
every divergence is a false-negative window by construction. **Correctness of the parse is the
security property here** — so shipping our own parser would be the worse supply-chain position, not
the safer one. Precedent in-tree: `serde_norway` was chosen over regex for typed YAML deser, same
reasoning, same direction. Q6 is therefore **closed — approved**.

---

### G. Open questions — status after two reviewer waves

Most of my original questions were answered in-thread. Recorded here with their rulings so Gate 1
has one place to look; only Q3 and Q6 are still open, and only Q6 blocks anything.

| # | Question | Status |
|---|---|---|
| Q1 | Widen the release guard from the task's 2 inputs to all 6 (§A)? | **Answered — yes.** @operations, @dry-reviewer, @security, @observability all yes. @operations' framing: rows 3–6 are "exactly the shapes a change takes under time pressure." |
| Q2 | Escalate all four MC+MH listener rows, or MH only (§C)? | **Answered — all four.** @operations: "splitting MH-critical from MC-warn would encode a distinction the failure mode does not have." |
| Q2b | Accept the devloop-stale on-disk-ConfigMap false positive as a HARD FAIL? | **Answered — accept, conditional on OPS-3** (named diagnosis + ground-truth `kubectl` command at the point of failure, plus a `docs/TODO.md` rollback entry). Condition adopted. |
| Q3 | Will @client author the secure-context section, and where does it live? | **Withdrawn — the question was wrong.** @dry-reviewer P-DRY-1: the anchor is frozen upstream and the section is story task 20's deliverable. I consume `#secure-context-and-media-setup` verbatim. §D rewritten. One thing still needs @client's acknowledgement, not a decision: that I am rewriting the fingerprint failure text task 20 is told to mirror. |
| Q4 | `no-insecure-browser-flags` as a third deliverable beyond the task's two halves? | **Open — @team-lead's call, and only theirs.** @security, @test, @observability want it; @operations supportive ("zero runtime cost, zero rollback risk, and R-33 requires the invariant regardless"); @dry-reviewer prefers a separate task. Cleanly separable either way — nothing else in the plan depends on it. |
| Q5 | Indeterminate branches HARD FAIL, no bypass env-var? | **Answered — yes, no `DEV_WEB_SKIP_WT`.** Plus OPS-2: the two indeterminate branches are different owner lanes and must not share a message shape. |
| Q6 | `toml` as a new direct dependency (§H3), tripping the Layer-6 audit dep-change gate? | **Answered — approved by @security** (§J5), with three conditions. @dry-reviewer reached the same conclusion independently (P-DRY-3). Paying one audit-gate run beats shipping a parse with a miss window. |

Two structural gaps surfaced during Gate 1 that are **real, infrastructure-owned, and out of scope
here** — named so they are not silently dropped:

1. **No way for a guard binary to self-declare an operator-lane precondition failure**
   (OPS-5(b)): `classify_guard_exit` routes only 124/137 to `PRECONDITION_FAILURE`. Worked around
   in this guard via the reason-token vocabulary; the mechanism gap is a future task.
2. **Deployed-artifact identity** (image tag / digest pinning) is the other half of "the shipped
   artifact is the one the pipeline built." Mine, genuinely task-sized, needs an @operations
   conversation about tag policy. Named in §A, not in this devloop.

### H. Gate-1 reviewer input — what changed, and two adjudications

Five reviewers sent pre-plan input (@test ×2, @security, @observability, @code-reviewer). Most of
it is adopted as-is and already folded into §A–§E above. This section records only the deltas that
changed my design, and the **two places where reviewers contradicted each other** and I had to
rule rather than accept both.

**H1. ADJUDICATION — `cargo chef cook`: in scope, but as a *different* failure class.**
@security A3 wants the `cook` line covered ("a cook line that drops `--release` silently changes
what gets compiled in the cached dependency layer"). @observability #3 wants the matcher scoped to
`cargo build` only, because "the shipped binary comes from `cargo build`; chef cook dropping
`--release` is a cache-efficiency bug, not a premise break" — and warns that widening the match
produces a confusing token when cook alone drifts.

**@observability is right on the mechanism and @security is right on the coverage.** cargo-chef's
cook stage only pre-builds dependencies; if cook goes dev-profile while build keeps `--release`,
cargo rebuilds those deps in release anyway. The artifact is still correct — just slower to build.
So it is *not* a premise break, and filing it under the same token would mean a red build that
says "your release premise is broken" when it isn't.

Ruling: **check both lines, with two rule_ids and two REASON tokens.**
`dockerfile-build-not-release` = premise break. `dockerfile-cook-profile-mismatch` = cache-layer
drift, still a FAIL (it is real drift and CLAUDE.md says fail loudly) but a token whose name tells
the reader immediately that the shipped binary is fine. @observability's requested "say so in a
comment" becomes a module-doc paragraph explaining exactly this distinction, so the next reader
doesn't collapse the two.

**H2. ADJUDICATION — service-Dockerfile enumeration: glob *and* workspace members, bidirectionally.**
@code-reviewer #4 and @security A1 want a glob (`infra/docker/*/Dockerfile`) so a new service is
covered automatically. @observability #2 wants the set *derived from the workspace members* as
SSoT, noting `infra/docker/` also holds `certs/`, `grafana/`, `postgres/`, `prometheus/` with no
Rust build, and that "a new `xx-service/Dockerfile` must not silently escape a premise gate."

A glob alone misses the opposite failure: a new `crates/xx-service` that never gets a Dockerfile
premise-checked. Derivation alone misses a Dockerfile for something not a workspace member.

Ruling: **both directions, which is strictly stronger than either.**
1. Discover by glob; a Dockerfile containing any `cargo build` is in scope regardless of directory
   name (so non-Rust dirs like `postgres/` and `grafana/` self-exclude by having no cargo line —
   no hardcoded exclusion list to rot).
2. Derive the *expected* set from the `[workspace] members` in the root `Cargo.toml` — every
   member matching `crates/*-service` must have a matching `infra/docker/<name>/Dockerfile` under
   the gate. Missing one is its own FAIL token.
   `crates/dt-guard/src/common/services.rs::CANONICAL_SERVICES` exists and is the declared SSoT for
   the `{ac,gc,mc,mh}` enumeration — but it is a *metric-prefix* list, not a deployable-artifact
   list, and hardcodes four services. **I am deriving from `[workspace] members` instead**, and
   will say why in the module doc, so @dry-reviewer doesn't read this as a fork of the services
   SSoT: they answer different questions ("which services emit metrics" vs "which crates ship as
   an image"), and binding the premise gate to the metrics list would make adding a service to one
   list silently change the other's meaning.

**H3. Cargo.toml parsing — typed `toml` deser, and it adds a dependency.**
@code-reviewer #3: prefer `toml::from_str` into a typed struct over a line regex, and any regex
must sit in a `Lazy<Regex>` canonical-home static per `clippy.toml`.

Adopted — with a cost I want visible rather than discovered at Layer 6: **`toml` is not currently
in `Cargo.lock`**, so this is a genuinely new direct dependency and it will trip the ADR-0033
§3/§11 audit dep-change gate (`scripts/lang/_audit_gate.sh:audit_dep_changed_rust`).

I considered hand-rolling a line scanner to avoid that, and **rejected it**: TOML permits
`[profile] release.debug-assertions = true` as a dotted key and inline-table forms, so a
line-oriented scanner has a real miss window — and a premise guard with a miss window "reads as
coverage," which is the failure mode this whole task exists to prevent. Paying one audit-gate run
to make the parse total is the right trade. **@operations / @security: flag it now if a new
direct dep is a problem and I'll re-open the choice.** No regex is needed anywhere in the Cargo.toml
half; the Dockerfile half is line-oriented plain string scanning, also regex-free.

**H4. Vacuity is a first-class failure class** (@security A2, @observability #2, @test #2 — three
reviewers independently). Adopted in full, with a distinct token each: zero Dockerfiles
discovered; a discovered Dockerfile with no `cargo build` line at all; root `Cargo.toml` absent,
unparseable, or carrying no `[profile.release]` table. All FAIL, none OK.

This is the single most important correction to my draft. My §A framing was "assert the premises,"
and a guard that walks nothing and prints OK asserts nothing while *looking* like it does — the
same shape as the inert `compile_error!` this task was written to protect. **This deserves its own
unit test per case, not a shared one.**

**H5. REASON-token vocabulary and `--explain` are the operator-facing contract, and are tested as
such** (@observability #1/#5, @test #1/#3). One `rule_id` per failure class, not one for the guard;
`--explain` emits via `common::explain::print_finding` with `policy = "<subcommand>::<rule_id>"`
and `src_file: file!(), src_line: line!()` at the emit site; plain `Finding`, not the redacted
variant (nothing here is a credential). **Tests assert the token *string*, not just the exit code**
— @observability's argument is exactly right and I would not have written the tests this way:
pinning only exit codes lets the vocabulary collapse to one undifferentiated token with every test
still green. At least one `EXPLAIN:` assertion per `rule_id`, mirroring
`crates/dt-guard/tests/binary_status_surface.rs` for the STATUS-surface tier.

**H6. dev-web.sh: hard-fail branches get remedies, and the contract sentence gets rewritten, not
retagged** (@observability #6/#9). The three `nothing listening on …` branches in
`check_wt_endpoint` carry no `Fix:` line today — and those are precisely the branches I am
escalating, so I'd have shipped "turn red without saying what to do." Each hard fail now states:
what broke, why it matters **in the user's terms ("the demo joins and you hear nothing", not "join's
media step will fail")**, and the next command.

The header's contract sentence ("HARD FAIL = nothing works without it; WARN = only part of the demo
breaks") stops being true under this change — MH reachability is "only part of the demo" by that
wording, yet now hard-fails. Retagging the table while leaving that sentence would leave a contract
that contradicts its own rows. It is rewritten around the real predicate: **hard fail = the
demonstrated path silently produces no audio.** "Silently" is the operative word and the reason
this task exists.

**H7. `--help` is fragile, and THIS diff is what breaks it.** ⚠ **Superseded in part by §I17** —
I originally wrote here that it "already truncates today," repeating @code-reviewer's report without
measuring. @operations measured; it is false. Line 29 is the genuine last header line and
`sed -n '2,29p'` prints the header complete. The fix still lands, for the honest reason: my own
header edits push past line 29, and a derived range removes the drift class rather than re-tuning
the constant. Form is the bounded `awk` contiguous-comment-run variant, plus a sentinel assertion in
`dev-web.test.sh` (§I17).

**H8. The secure-context prose — @security's D1 corrects a claim I would have shipped.**
I had written that "a non-localhost HTTP origin disables the pipeline," which is directionally
right but imprecise in a way that misdirects a reader. The actual rule is *potentially trustworthy
origin* per the Secure Contexts spec: `https://`, plus `localhost`, `*.localhost`, `127.0.0.1`,
`[::1]`. **`http://demo.localhost:5173` IS a secure context** — which is exactly why the demo works
over plain HTTP today, and a reader told otherwise would go chase the wrong problem. The real
failure is a **non-loopback HTTP origin**: reaching Vite at `http://192.168.x.x:5173` or a LAN
hostname from another machine — which is precisely what this runbook's WSL2/Windows split tempts
people into (cf. F9). This correction goes to @client as part of the §D content hand-off.

**H9. And @security's D2 is why §D and §E are one control, not two.** The moment a developer hits
this on a LAN IP, the "obvious fix" they reach for is the prohibited insecure-origin flag. So the
prose must name the **approved** remedy in the same breath — use a `.localhost` name or a loopback
literal, or terminate real TLS. A section that describes the symptom without an approved remedy
measurably raises the odds the prohibited flag gets added later by someone who never read this
task. Corollary I'm enforcing on the hand-off: **the runbook section must not name the prohibited
flag at all**, even to reject it, because naming it both plants the idea and forces an allowlist
entry (§E3) that then has to be maintained forever.

**H10. Naming** (@code-reviewer #1). `release-build-profile` and `no-insecure-browser-flags` —
concrete, single-concern, no catch-all. Noted and appreciated that dt-guard's 32 existing
subcommands mean the ADR-0034 ≥10 re-debate threshold is a long-standing pre-existing condition
this task does not newly trip.

---

### I. Second reviewer wave — what changed again (and the two findings that broke my design)

@dry-reviewer, @operations and @security's addendum arrived after §H. Three of these are not
refinements — they are corrections to things I had *wrong*, and I'd rather say so plainly than
fold them in silently.

**I1 — @dry-reviewer P-DRY-1 killed my §D outright.** Covered above; §D is rewritten. The short
version: the anchor was frozen upstream in the story manifest and I was about to invent a third
spelling of it. I verified every claim in P-DRY-1 against
`docs/user-stories/2026-08-27-hear-yourself-through-handler.md` (task 20 text, the line-98 summary,
and open question #7 at line 178) rather than taking it on trust, and it holds in full.

**I2 — @operations OPS-8: my bash test suite was vacuous, and would have shipped green.**
This is the sharpest finding of the whole Gate 1 and it invalidated my §C test design.

Trace: `check_port` calls `fail()`, which sets `HARD_FAIL=1` **without exiting**; the WT checks run
regardless; the single `exit 1` is at the very end. So in CI, or on any machine with no cluster,
`dev-web.sh --check` **always exits nonzero** — because AC/GC on 8443/8444 don't answer. Every
assertion I planned ("exits nonzero when fingerprints are absent", "…when a configmap advertises a
dead port", "…when `ss` is unavailable") would have passed **with the escalation reverted**. A
suite that stays green when the feature it tests is removed is not coverage; it is decoration that
costs review attention. I would not have caught this from reading my own plan.

Redesign, adopting OPS-8's two asks:
- **Assert on the specific `✗` marker and message text of the branch under test**, never on the
  process exit code — `assert_marker` / `assert_no_marker` from `scripts/lang/_test_helpers.sh`,
  which I'd already named as the SSoT. The negative direction (`assert_no_marker` for the `!` WARN
  glyph on those branches) is what actually pins the escalation. @test stated the acceptance
  property precisely and I'm adopting it verbatim as the bar: **reverting WARN→HARD-FAIL must
  re-emit a WARN marker for those branches, which flips the `assert_marker '✗'` red.** A test that
  cannot distinguish the pre- and post-escalation states does not test the escalation.
- **Hermetic before wiring**: PATH-stub `curl` / `ss` / `getent`, temp repo root, `AC_PORT` /
  `GC_PORT` overrides — the technique `scripts/guards/run-guards.test.sh` already uses to PATH-stub
  `timeout`. **If I cannot make it hermetic, it does not get wired into `layer3.sh`.** A
  permanently-red Layer-3 step is worse for everyone on this branch than no self-test, and
  quarantining is not an available escape (ADR-0028).
- P-DRY-7's trap applies here: `report_results` requires every `FAILURES+=` entry to carry a
  `[label]` prefix, because `tee_collect_statuses` matches `^STATUS=` at line start — a raw failure
  diagnostic beginning `STATUS=` would vote into Layer 3's verdict. My header-contract drift
  assertion quotes header text, so it is exactly the shape that trips this. Routed through the
  helpers, `[label]` preserved.

**I3 — @operations OPS-4 corrected the Lead's setup note, and adds a file row.** I'd have looked at
§6.5. OPS-4 checked: §6.5 documents env-tests/Vitest/Playwright and does **not** describe the
preflight, so it doesn't go stale. What *does* go stale is **§3 Step 0** — the "Read its two
severities correctly" block (which after this change is actively wrong: there are no WebTransport
WARNs left, and the script no longer starts the server) and the "One limitation to know" block
(whose hazard **inverts** — post-escalation, a *red* preflight on a devloop cluster no longer means
the join is broken, and it now blocks launch). New table row, operations-owned, and it **must land
in the same commit as the escalation**: a merge where the runbook contradicts the script is the
drift the SSoT rule exists to prevent. I'll also check `packages/web-app/README.md:36-104` doesn't
restate the severity contract.

**I4 — @operations OPS-5(b): the operator lane I was implicitly designing for does not exist.**
`scripts/guards/run-guards.sh:classify_guard_exit` routes **only** exit 124 and 137 to
`PRECONDITION_FAILURE`; the default arm classifies every other non-zero exit as `FAILED`
(implementer lane). So a guard exiting 2 for "I can't find my inputs" gets reclassified as a diff
defect and lands on an implementer with nothing to fix. Achievable design instead: **carry the lane
in the reason token** (`release-build-profile-no-dockerfiles-discovered`,
`release-build-profile-workspace-manifest-unreadable` — distinct from the violation tokens) and say
in the message body that it is a discovery/precondition failure, not a diff defect, so
`docs/runbooks/devloop-validation.md` §6.3.1 triage reaches the right owner. This also satisfies
@observability's sharpened bar for constraint 1: **the test is not "are the tokens different
strings" but "could someone write the devloop-validation triage entry from the token alone."** I'm
adopting that as the acceptance bar for the vocabulary.

**I5 — @security A5/A6 and @operations' remedy findings, adopted.**
- **A6 `.cargo/config.toml`** is the strongest of the late additions and I'd under-weighted it: it
  defeats the premise **without touching root `Cargo.toml` and without touching any Dockerfile** —
  invisible to *both* halves as I'd specified them. Covered: if the file exists, it must not set
  `[profile.release] debug-assertions` nor `[build]/[target.*] rustflags` carrying it.
- **A5**: match `debug-assertions=yes|true|on|1` **and** the no-space `-Cdebug-assertions` spelling,
  and cover `CARGO_ENCODED_RUSTFLAGS` alongside `RUSTFLAGS`. One form matched is a bypass, not a guard.
- **The two rules have deliberately opposite scopes, and that gets a comment in the source.**
  The RUSTFLAGS rule is scoped to build/deploy **execution paths** (Dockerfiles, CI `env:` blocks,
  `.cargo/config.toml`) — `RUSTFLAGS` appears legitimately in prose in at least six tracked docs
  (mold linker, sanitizers) and `ci.yml:170` sets `RUSTFLAGS: --cfg coverage` benignly, all of which
  must not fire. The cert-flag rule is scoped to **include** docs and runbooks, because a runbook
  telling someone to paste a flag *is* the leak. @security is right that someone will later
  "harmonize" these two scopes and silently break one; the module doc says why they differ.
- **OPS-1 is the load-bearing operations ask and I'd have shipped it wrong.** Three of the branches
  I'm escalating (`nothing listening on …` — IPv4, IPv6, hostname) print **no `Fix:` line at all**.
  Tolerable as a WARN; as a HARD FAIL it is a stopped developer with nowhere to go. Every escalated
  branch now prints what broke, why it matters *in user terms*, and the next command
  (`./infra/kind/scripts/setup.sh` + `kubectl get pods -n dark-tower`; `sudo apt-get install -y
  iproute2` for the `ss` branch).
- **OPS-2**: the "no advertise address in `$file`" branch is a **different owner lane** — repo/config
  drift, not the developer's environment — and must say so, or a 3am reader spends an hour
  restarting a healthy cluster. Distinct message shape naming the lane and saying there is nothing
  to fix locally.
- **OPS-3** is the condition on operations accepting the devloop-stale false positive, and it is a
  fair price: the listener failure text names the known false-positive case and its ground-truth
  command, plus a `docs/TODO.md` entry
  recording that the on-disk read is now a **blocker** rather than a stale-green, with the
  live-ConfigMap read named as the fix if it bites. That entry is the *rollback plan* for this
  decision — knowing what "undo" looks like before shipping, not after.
- **OPS-6**: WARN-era wording ("sign-up/create work, but WebTransport JOIN will fail") understates a
  stop. Every escalated message re-worded to state the stop.
- **OPS-5(a)**: the zero-discovery assertion is stated explicitly for the release half, not left
  implicit in §E's flags-guard scope. Already in §B's rule table; called out here so it isn't read
  as flags-only.

**I6 — @operations verified the blast radius, which is worth recording so nobody re-derives it.**
Zero automated callers of `dev-web.sh` across `.github/`, `scripts/layer*.sh`, `scripts/workflow/`,
`.githooks/`, `infra/`, `packages/`. No CI gate, no Layer-7 step, no git hook goes red. The
counterweight: `dev-web.sh` is the *only* documented launch path (runbook §3 Step 3,
`packages/web-app/README.md:47`), so under WARN a developer with a broken WT setup still got a Vite
server for AC/GC work, and under HARD FAIL they get nothing. **A false positive now costs the whole
dev session, not just the join.** That is precisely why OPS-1's remedy lines are non-negotiable
rather than polish.

**I7 — @dry-reviewer's remaining placements, adopted.**
- **P-DRY-2**: a doc-comment sentence in `release_build_profile.rs` recording the boundary against
  story **task 23**, which lands "a real build, not a CI string check" of the `compile_error!`.
  Mine asserts the *premise* (shipped artifacts are built release); task 23 asserts the *control
  fires* given the premise. Complements, not duplicates — and without the note, whoever lands task
  23 can reasonably read this guard as superseded and delete it.
- **P-DRY-3**: no third TOML walker. `cite_extract.rs`'s `TOML_SECTION_RESOLVER`/`TOML_KEY_RESOLVER`
  answer a different question (does symbol X appear as a section-or-key) than mine (does
  `debug-assertions` hold a truthy value *within* `[profile.release]`). Real `toml` crate, per §H3;
  the doc comment says why those resolvers weren't reused so nobody collapses them later.
- **P-DRY-4**: the **floor vs. scan set** split — scan set stays the walk (a new non-service
  Dockerfile still gets checked), floor is a derived roster. See §I16 for where the floor's roster
  comes from: @code-reviewer's Blocker 2 showed `CANONICAL_SERVICES` alone would not satisfy
  "nothing to hand-maintain.
- **P-DRY-5**: `crates/mh-test-utils/src/lib.rs`'s doc comment already makes a load-bearing security
  argument resting on exactly this premise — it cites `infra/docker/mh-service/Dockerfile` lines 53,
  59, 78 and 135 by number, with nothing enforcing any of it. Verified in tree. My guard is its
  missing forcing function; one `ANCHOR (DRY):` line names it. (Those four line-number cites will
  rot on any Dockerfile edit and `cite-no-line-numbers` doesn't reach Rust doc comments — not my
  fix, but I'll add a `docs/TODO.md` line while I'm there.)
- **P-DRY-6**: the out-of-scope boundary is hoisted to a **module-level** scope statement covering
  all rules, not buried in the rustflags rule. Named non-encodings: `ci.yml:91` / `ci-client.yml:89`
  build `dt-guard`/`dt-story` **host** binaries (different artifact class), and
  `docs/BUILD_REQUIREMENTS.md` carries an illustrative Dockerfile. None are encodings of this
  premise; the statement exists so the next contributor doesn't "complete" the guard by adding them.
- **P-DRY-8**: the `--help` derivation is a first instance, not a duplicate — no extraction wanted.

**I8 — @observability's hardened constraints 4 and 8** are treated as settled, not open: the
bypass class is covered (rows 5/6 + `.cargo/config.toml`) with distinct tokens, and the `--help`
range is derived in-diff. Their asymmetry argument is the one I'd want on the record: the
`[profile.release]` table is the *documented* way to set debug-assertions, but the env-var and
`RUSTFLAGS` forms are what appear **when someone is debugging a production issue under time
pressure** — exactly when MH's `compile_error!` most needs to still be armed. A premise assertion
covering only the tidy path is inert in the scenario it exists for.

---

### G. Open questions for Gate 1

1. **@team-lead / all** — widen the guard from the task's 2 inputs to all 7 (§A)? My recommendation:
   yes; rows 3–7 defeat the premise without touching either noun the task names.
2. **@client / @operations** — escalate all four MC/MH listener rows, or MH only (§C)? And the
   false-positive risk: HARD FAIL on a devloop cluster whose on-disk configmap is stale — accept,
   or does that need the live-ConfigMap read first?
3. **@client** — will you author the secure-context section, and where does it live (§D)?
4. **@team-lead** — the `no-insecure-browser-flags` guard is a third deliverable beyond the task's
   two halves (§E2). @security and @test both want it in-loop and I now agree; it is cleanly
   separable if you want scope held. Your call.
5. **@operations** — indeterminate sub-branches (`ss` missing, no advertise address) as HARD FAIL
   with a named remedy and no bypass env-var (§C). @security (C1) and @observability (#7) both
   independently said these precondition branches must not stay WARN, so I now treat this as
   settled unless you object: **two outcomes only on critical-path checks — PASS and HARD FAIL.**
   "Cannot verify" collapses into HARD FAIL, with wording that says CANNOT VERIFY explicitly so
   the remedy is distinguishable (`install iproute2` vs `bring the cluster up`). A check that did
   not run must never read as a check that passed.
6. **@operations / @security** — H3: the `toml` crate is a new direct dependency and will trip the
   Layer-6 audit dep-change gate. Acceptable, or do you want the hand-rolled parse despite its
   miss window?

---

## Pre-Work

None. Started clean at `ebebea8`.

---

## Implementation Summary

### Half 1 — `dt-guard release-build-profile` (new subcommand + Layer-3 wrapper)

Asserts that shipped artifacts are built with `debug_assertions` off. The task named **two**
inputs; the guard covers **nine** — seven premise channels plus the vacuity and roster classes —
because the other seven defeat the premise without touching either file the task names.

| Channel | rule_id | Found by |
|---|---|---|
| `cargo build` drops `--release` / picks another profile | `dockerfile_build_not_release` | task |
| `cargo chef cook` disagrees with `cargo build` on profile | `dockerfile_cook_profile_mismatch` | @security A3 / @observability #3 |
| `[profile.release]` enables debug-assertions | `release_profile_debug_assertions` | task |
| `[profile.release.package.<crate>]` enables it | same rule, recursive walk | @security row 8 |
| `inherits = "release"` profile enables it **and** a Dockerfile selects it | `release_inheriting_profile` | me |
| `RUSTFLAGS` / `CARGO_ENCODED_RUSTFLAGS` carry `-C debug-assertions` | `rustflags_debug_assertions` | @security A5, @observability |
| `CARGO_PROFILE_<P>_DEBUG_ASSERTIONS` via Dockerfile `ENV`/`ARG` or CI `env:` | `cargo_profile_env_debug_assertions` | @security A4 |
| `[profile.release]` table **inside** `.cargo/config{,.toml}` | `config_toml_release_profile` | @observability (d), @security A6/row 9 |

Plus five classes that FAIL rather than reporting clean: `no_dockerfiles_discovered`,
`dockerfile_no_cargo_build_line`, `workspace_manifest_unreadable`,
`service_crate_without_dockerfile`, `canonical_services_roster_drift`.

Design points worth carrying forward:

- **Typed `toml` parse, not a line scanner.** New direct dependency (~5 crates), approved by
  @security on the argument that the guard must model the same grammar as the consumer it asserts
  about — `toml`/`winnow` is what cargo itself parses with, so a hand-rolled scanner would be a
  second, *divergent* implementation and every divergence a false-negative window. Parse is total:
  unparseable FAILs, never skips. One parser serves both `Cargo.toml` and `.cargo/config*`.
- **Bidirectional discovery.** Scan set is a walk (`infra/docker/*/Dockerfile`; non-Rust contexts
  self-exclude by having no cargo line, so there is no exclusion list to rot). The anti-vacuity
  **floor** derives from `[workspace] members` matching `crates/*-service` — a true SSoT, since
  cargo fails to build if it is wrong.
- **`CANONICAL_SERVICES` is now a guarded mirror, not a trusted one.** It is documented as the
  metrics-emitting service list and is read here as the container-shipping list; those sets
  coincide today and that coincidence had become load-bearing. `canonical_services_roster_drift`
  machine-checks it against the workspace-derived roster.

### Half 2 — `scripts/dev-web.sh` WARN → HARD FAIL

- **All four MC/MH listener rows escalated**, not just the two MH ones. The join dials MC before
  MH, so an MC gap produces the same silent outcome; warning on one and failing on the other would
  encode a distinction the failure mode does not have.
- **"Cannot verify" is a hard fail**, in two distinct owner lanes with distinct message shapes:
  `ss` absent (the operator's machine, remedy `apt-get install -y iproute2`) versus no advertise
  address in the configmap (repo/config drift — *nothing to fix locally*). No bypass env-var.
- **Every escalated branch prints a remedy.** Three of them printed none before; as a WARN that was
  tolerable, as a hard fail it is a stopped developer with nowhere to go.
- **The header contract was rewritten, not retagged.** The old rule ("HARD FAIL = nothing works
  without it; WARN = only part of the demo breaks") stops explaining its own rows once MH
  reachability — "only part of the demo" by that wording — hard-fails. New predicate: *hard fail =
  the demonstrated path silently produces no audio.*
- **`--help` is derived, bounded, and sentinel-tested.** `awk 'NR==1{next} /^#/{print; next}
  {exit}'` defines the block by what it is; its worst case is truncation, where the `sed`
  alternative's is an unbounded dump to EOF.

### Half 3 — `dt-guard no-insecure-browser-flags` (approved in-loop by @team-lead)

Mechanises R-33's standing prohibition, covering all **three** clauses of story task 20's wider
phrasing (disable cert validation / disable web security / force an insecure origin). Thirteen
exact literals across three classes with three distinct tokens, a two-entry literal-path allowlist,
tracked-files-only, docs and runbooks deliberately **in** scope.

### Two bugs the hermetic self-test flushed out of `dev-web.sh`

Both pre-existing, both the same class as everything else here — a failure that exits silently
rather than loudly:

1. `LOCALHOST_FIRST="$(getent … | awk …)"` — a failing or absent `getent` made the pipeline
   nonzero, `pipefail` propagated, and `set -e` **aborted the entire preflight at that line**. The
   remaining checks never ran and the report looked finished.
2. `url="$(grep -E 'WEBTRANSPORT_ADVERTISE_ADDRESS' … | head -1)"` — same mechanism, and it made
   **the "no advertise address" branch unreachable**. That is precisely the branch @operations'
   OPS-2 asked me to give its own owner lane; it could never have executed.

I would have shipped both without the suite.

### Two claims corrected mid-flight

- **`--help` did not truncate.** I propagated @code-reviewer's report to three reviewers and to
  @team-lead before anyone measured. Line 29 is the genuine last header line. The fix still lands,
  for the honest reason: *this diff* grows the header past 29. "Live defect" stays out of the
  commit message.
- **The secure-context section is not mine to write.** My first plan had @client authoring it
  in-loop; @dry-reviewer found the anchor is frozen upstream as story task 20's deliverable and
  already referenced by operations' triage ladder. I consume
  `#secure-context-and-media-setup` verbatim and author nothing there — and per @security, the
  header carries the *substance* inline rather than only a cite, because the reader who follows a
  not-yet-existing anchor is by construction someone who just hit a hard fail.

---

## Files Modified

```
 crates/dt-guard/Cargo.toml                              |    8 +
 crates/dt-guard/src/common/services.rs                  |   23 +
 crates/dt-guard/src/lib.rs                              |    2 +
 crates/dt-guard/src/main.rs                             |   28 +
 crates/dt-guard/src/no_insecure_browser_flags.rs        |  563 ++++++
 crates/dt-guard/src/release_build_profile.rs            | 1781 +++++++++++++
 crates/dt-guard/tests/no_insecure_browser_flags_e2e.rs  |  310 ++++
 crates/dt-guard/tests/release_build_profile_e2e.rs      |  419 +++++
 crates/mh-test-utils/src/lib.rs                         |    5 +
 docs/TODO.md                                            |  140 ++
 docs/devloop-outputs/2026-09-01-.../main.md             | 1865 +++++++++++++
 docs/runbooks/client-dev-local.md                       |   47 +-
 scripts/dev-web.sh                                      |  175 +-
 scripts/dev-web.test.sh                                 |  285 +++
 scripts/guards/simple/validate-no-insecure-browser-flags.sh |    7 +
 scripts/guards/simple/validate-release-build-profile.sh |    5 +
 scripts/layer3.sh                                       |   17 +
```

(`docs/runbooks/devloop-validation.md` is operations-owned and lands separately in this commit.)

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/dt-guard/src/release_build_profile.rs` | New policy: 7 premise channels + 5 vacuity/drift classes, typed TOML parse, bidirectional discovery, 30 unit tests |
| `crates/dt-guard/src/no_insecure_browser_flags.rs` | New policy: 13 exact literals, 3 classes, 2-entry literal-path allowlist, 9 unit tests |
| `crates/dt-guard/tests/*_e2e.rs` | Operator-surface tier: token-inside-`VIOLATION:`-line, precondition-line-first, real-tree non-vacuity |
| `crates/dt-guard/src/common/services.rs` | Doc comment naming both consumers and the now machine-checked invariant |
| `crates/mh-test-utils/src/lib.rs` | One `ANCHOR (DRY):` line naming the enforcing home; no new claim |
| `scripts/dev-web.sh` | Escalation, contract rewrite, secure-context pointer, derived `--help`, two silent-abort fixes |
| `scripts/dev-web.test.sh` | 47 hermetic assertions; marker-based, never exit-code-based |
| `scripts/layer3.sh` | One `run_and_emit` line wiring the self-test |
| `docs/runbooks/client-dev-local.md` | §3 Step 0 severity contract (operations-authored); duplicate enumeration deleted, points at `--help` |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS — `cargo check --workspace` clean.

### Layer 2: cargo fmt
**Status**: PASS — `cargo fmt --all --check` clean.

### Layer 3: Simple Guards
**Status**: 38/39 PASS, 1 expected FAIL pending an owner's hunk.

| Guard | Status |
|-------|--------|
| validate-release-build-profile (new) | PASS — `SCOPE: 4 cargo Dockerfiles, 9 enumerated inputs, 0 hits` |
| validate-no-insecure-browser-flags (new) | PASS — `SCOPE: 1407 candidate files, 13 enumerated settings, 15 allowlisted mentions, 0 hits` |
| validate-cross-boundary-scope | FAIL — `scope_drift_planned_untouched "docs/runbooks/devloop-validation.md"` |
| all others (36) | PASS |

The one failure is a **true positive and expected**: the classification table carries the
operations-owned `devloop-validation.md` row (§8 caveat repair, token catalogue rows, §6.3.1 fourth
failure shape) per @team-lead's instruction, and @operations has not yet landed those hunks. It
closes when they do; nothing on my side is outstanding for it.

Worth recording, because it cost a round: the Path cell must be `` `path` `` followed by at most a
*single* parenthetical. A second backticked span inside the cell, or prose before the parenthetical,
gets swallowed into the extracted path and produces a spurious inbound + untouched pair.

Also recorded because it is the point: **`no-insecure-browser-flags` failed on my own devloop
record** the first time I staged it, catching two prohibited literals I had quoted while relaying
reviewer messages. Fixed at the source (de-literalised the prose), **not** by adding an allowlist
entry — which is exactly @team-lead's Gate-1 condition 5 and @security's self-consistency point,
demonstrated rather than asserted.

### Layer 4: Unit Tests
**Status**: PASS — `cargo test -p dt-guard`: 414 lib + 43 integration across 8 suites, 0 failed.
- `release_build_profile` unit: 30
- `release_build_profile_e2e`: 16
- `no_insecure_browser_flags` unit: 9 (counted in the lib total)
- `no_insecure_browser_flags_e2e`: 9
- `scripts/dev-web.test.sh`: 47 assertions, 0 failed

### Layer 5: All Tests (Integration)
**Status**: PASS — no new integration surface beyond the two e2e suites above; workspace check clean.

### Layer 6: Clippy
**Status**: PASS — `cargo clippy -p dt-guard --lib --bins --tests -- -D warnings` clean.
**Note for the audit gate**: this diff adds a new direct dependency (`toml`, pulling
`toml_edit`/`winnow`/`serde_spanned`/`toml_datetime`/`toml_write`), so the ADR-0033 §3/§11 audit
dep-change gate will fire. Per @security's condition and @operations' agreement: **if that subtree
needs any suppression to go green, I stop and escalate rather than suppress.**

### Layer 7: Env-tests
**Status**: N/A for this diff — no service, manifest, or cluster surface is touched. `dev-web.sh` has
zero automated callers (verified by @operations across `.github/`, `scripts/`, `.githooks/`,
`infra/`, `packages/`), so nothing in Layer 7 exercises it.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

---

## Accepted Deferrals

- `docs/TODO.md` §Infrastructure Validation in Devloops — deployed-artifact identity (digest pinning)
- `docs/TODO.md` §Infrastructure Validation in Devloops — shape-based `cfg` complement to the premise guard
- `docs/TODO.md` §Infrastructure Validation in Devloops — dev-web.sh on-disk ConfigMap read is now a blocker (rollback plan)
- `docs/TODO.md` §Infrastructure Validation in Devloops — mh-test-utils Dockerfile line-number cites are unguarded
- `docs/TODO.md` §Documentation Hygiene — reconcile duplicated secure-context prose when task 20 lands
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — make "which dialects?" standing at class-sweep reviews

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied

---

## Issues Encountered & Resolutions

### Issue 1: The bash self-test I planned was vacuous and would have shipped green
**Problem**: @operations (OPS-8) traced the exit path: `check_port` sets `HARD_FAIL=1` **without**
exiting, and the sole `exit 1` is at the end of preflight — so `dev-web.sh --check` exits nonzero in
any cluster-less environment because AC/GC don't answer. All three of my planned assertions were
green *before* the escalation and green again after a revert. A suite that cannot distinguish those
two states does not test the escalation.
**Resolution**: Redesigned around per-branch `✗`/`!` markers via `_test_helpers.sh`, never the exit
code, with @test's property as the stated bar: *reverting the escalation must re-emit a WARN marker,
flipping the `✗` assertion red.* Made hermetic (PATH-stubbed `ss`/`curl`/`getent`, temp root, port
overrides) before wiring into `layer3.sh`.

### Issue 2: Two `set -e` silent aborts in `dev-web.sh`, one making a branch unreachable
**Problem**: Building the hermetic fixture immediately killed the preflight mid-report. Cause:
`LOCALHOST_FIRST="$(getent … | awk …)"` — a failing `getent` makes the pipeline nonzero, `pipefail`
propagates, `set -e` aborts. Then the same shape in `check_wt_endpoint`'s `grep … | head -1`, which
meant **the "no advertise address" branch had never been reachable** — the exact branch OPS-2 asked
me to give its own owner lane.
**Resolution**: `|| true` on both, each with a comment explaining that the guard is load-bearing
rather than defensive clutter, plus a note that the grep is deliberately unanchored (the real keys
are service-prefixed) so nobody tightens it. Neither would have been found without the suite.

### Issue 3: I propagated an unmeasured claim to four recipients
**Problem**: @code-reviewer reported `--help` "already truncates at line 30". I adopted it without
measuring and repeated it as a "live defect" to @observability, @operations, @security and
@team-lead. @operations measured: line 29 is real content and the header prints complete.
**Resolution**: Corrected in the plan (§I17) and in every message; "live defect" kept out of the
commit message. The fix still lands on the honest reason — *this diff* grows the header past 29,
and once the runbook cites `--help` as the severity SSoT a truncation would silently truncate the
runbook's answer too. Three reviewers corrected their own findings during this loop; this was mine.

### Issue 4: My own plan document violated the guard I was writing
**Problem**: On first staging, `no-insecure-browser-flags` failed on
`docs/devloop-outputs/2026-09-01-.../main.md` — I had quoted two prohibited literals verbatim while
relaying reviewer messages.
**Resolution**: De-literalised the prose. **Not** an allowlist entry — `dev-web.sh` and this record
both sit inside the guard's own scan surface, and an exemption there is the one thing that would
hollow it out. @team-lead's Gate-1 condition 5, demonstrated rather than asserted.

---

### Issue 5: My Gate-2 pre-signal check was itself a vacuous assertion
**Problem**: Gate 2 attempt 1 failed Layer 5 on three `clippy::indexing-slicing` denials in my new
code. I had reported clippy clean. The cause was not a stale cache and not a missing flag —
`indexing_slicing = "deny"` is a workspace lint in `Cargo.toml:44` and applied all along. It was my
**filter**: I verified with `cargo clippy … | grep -cE "^error"`, and clippy's output lines begin
with ANSI escapes (`^[[1m^[[91merror`), so `^error` **can never match**. The check returned 0 for a
run that had failed, and I signalled Ready on it.

A verification step whose passing condition cannot be true is the exact defect class this devloop
exists to close — `SCOPE:` counts lines, vacuity tokens, `assert_no_marker`, `#![expect]`
self-cleaning — committed in the verification of that same devloop.
**Resolution**: Use `bash scripts/layer5.sh` and read its `STATUS=`/`RESULT=` line, which is what
Gate 2 actually applies, rather than a hand-built clippy invocation plus a hand-built grep. (The
Lead's diagnosis — that `-D warnings` doesn't enable a `restriction`-group lint — is true in
general but wasn't the mechanism here; the workspace lint was already active.)

### Issue 6: The indexing lint was covering two real bugs, one a live bypass
**Problem**: @team-lead flagged `no_insecure_browser_flags.rs:203` as the site to look at hardest.
The suspected `idx - 1` underflow was not reachable (guarded by `if idx > 0`), but two genuine
behavioural defects were sitting behind the lint, both making the guard *quietly* weaker:
1. **First-match-only was a bypass.** `line.find(name)` returns only the first occurrence, so a
   non-boundary decoy earlier on the line (a `x<prop>` identifier assigned a number, followed on
   the SAME line by the genuine `<prop>` assignment) caused the boundary check to reject the decoy
   and `return false`, **missing the real violation**. Verified against the pre-fix code: it
   returned `false` on that input.
2. **Byte-wise boundary was UTF-8-wrong.** `as_bytes()[idx - 1] as char` reads a continuation byte
   when the preceding character is multi-byte; those map to non-alphanumeric chars, so an adjacent
   word character read as a boundary. Verified: the property name prefixed by `mу` (Cyrillic `у`,
   two bytes) returned `true` — a false positive — before the fix.
**Resolution**: `match_indices` over every occurrence, character-wise boundary via a total
`get(..idx)`, and total accessors throughout (no indexing, no slicing, no arithmetic that can
underflow). Three regression tests added, plus a totality test over hostile inputs. The two library
token loops became `enumerate` + `get(i + 1)`. **No `#[expect]` or scoped allow in library code**,
per @team-lead: production guard code that panics on malformed input reads as a crashed pipeline
rather than a finding.

The two integration-test files *did* take a crate-level `#![expect(clippy::unwrap_used)]` — the
documented ADR-0002 test carve-out with existing precedent at
`tests/ts_retained_credentials_fixtures.rs`, because `allow-unwrap-in-tests` does not reach
non-`#[test]` helper fns, and a fixture helper that swallowed an IO error would yield a harness that
passes while proving nothing. Notably `#![expect]` then reported `clippy::expect_used` **unfulfilled**,
so that lint was dropped from the attribute — the escape narrowed to exactly what fires.

### Issue 7: My own UTF-8 regression test was vacuous on first write
**Problem**: The test I added for bug (2) capitalised the first letter of the property name. The
vocabulary entry is lower-camel, so `find` never matched and the test passed without ever reaching
the boundary check it existed to pin.
**Resolution**: Caught by extracting the pre-fix implementation into a scratch binary and running
both inputs through it — the old code returned `false` on the "false positive" case, which was the
tell that the input was wrong rather than the fix. Corrected to lowercase and re-verified as a true
differential (old: `true`, new: `false`); the comment records why, so nobody re-capitalises it.

---

### Issue 8: OPS-9 — the fingerprint drift assertion passed vacuously
**Problem**: @operations found that `scripts/dev-web.test.sh`'s header-vs-body drift block extracted
the fingerprint section with `sed -n '/Cert fingerprints (HARD FAIL/,/^fi$/p'` — **anchored on the
severity word inside the section comment, i.e. the very text a revert rewrites.** Reproduced
independently before fixing: flipping the comment back to `(WARN — …)` and `fail`→`warn` yields a
**zero-length** block, so `assert_absent` passes on nothing, and `header-tags-fingerprints-hard-fail`
also passes because it reads the top-of-file table row that revert doesn't touch. Both green, body
warning.

This is not a small hole. It is the **one automated link** in the chain
`client-dev-local.md §3 Step 0 → --help → header → body` that @operations' deletion of the runbook's
duplicate severity enumeration rests on — so the forcing function I told them was intact was, for
the fingerprint half, not. And it is the same failure class the subject of this devloop enforces
against: an extraction that finds nothing and reads as clean, next door to a guard that fails closed
on exactly that with five vacuity tokens.

**Resolution**: two independent controls, neither sufficient alone:
- **(a) Anchor on stable code**, `/if \[\[ -s "\$FINGERPRINTS_JSON" \]\]/,/^fi$/p` — a test
  expression a revert leaves alone — rather than on prose that encodes the answer.
- **(b) Assert the block is non-vacuous** (`assert_block_nonvacuous`) before asserting what it must
  not contain, so a future anchor that stops matching **reds here instead of silently disarming the
  check**. Applied to the `check_wt_endpoint` half too: @operations correctly noted it is not
  vulnerable today (it anchors on a function definition), but the assertion is one line and makes
  both halves fail closed *by construction* rather than by luck of anchor choice.

Both controls demonstrated separately rather than argued:

| Simulated change | Before OPS-9 fix | After |
|---|---|---|
| Full revert (comment → WARN, `fail`→`warn`) | fingerprint half green | **4 assertions red** |
| Harmless refactor breaking only the anchor (`"$X"` → `"${X}"`), severity untouched | green | **`fingerprints-extraction-nonvacuous` red** |

Suite is 50 assertions (was 47). @operations' review also confirmed the remedy text survived the
`set -e` fixes intact, the WebTransport half asserts drift bidirectionally, and their §8 token rows
match `RULE_ORDER` exactly after the `token_for` refactor.

---

### Issue 9: the roster floor could be voided by a routine refactor — three reviewers, one line
**Problem**: @security (F1), @dry-reviewer (F-DRY-B) and @code-reviewer (F1) independently landed on
`check_service_roster`. Cargo supports **glob** workspace members, and `members = ["crates/*"]` is
the tidy-up someone does when the list gets long. `"crates/*"` strips to `"*"`, which fails the
`ends_with("-service")` filter, so the derived roster came back empty — and an empty roster made
**both** halves of the floor check nothing: the coverage loop iterated zero times and the drift
comparison was short-circuited by a `!derived.is_empty()` guard. Reproduced on trees differing only
in how members are spelled: enumerated → `STATUS=FAIL` with two precondition hits; glob →
`STATUS=OK`. @code-reviewer found a second entrance to the same silence — a second `toml::from_str`
of the same string whose failure was swallowed on a comment ("already recorded by the caller") that
was false for a wrong-typed `[workspace]`.

That `!derived.is_empty()` conjunct read as defensive but converted *"I could not derive anything"*
into silence, in the one mechanism whose entire job is to not be silently disable-able — and in the
one spot in the module where every other derivation failure fails loudly. My own module doc names
the failure: *"a guard that globs, matches zero files, compares nothing and reports OK … the same
inertness this guard exists to prevent, turned on itself."*

**Resolution**: parse the manifest **once** into a combined `WorkspaceManifest` (removes the second
parse and its false comment), and make an untrustworthy roster a loud `service_roster_underivable`
precondition. Trigger is **glob presence, not emptiness** — a mixed
`["crates/mh-service", "crates/*"]` derives a non-empty roster that is still untrustworthy, so
emptiness was the wrong discriminator. Enumerated members with no service crate stays clean: that
roster is *accurately* empty, not underivable, and failing there would be a false positive.

### Issue 10: the guard was false-failing on a premise that holds
**Problem**: @observability's four-sub-case analysis of `workspace-manifest-unreadable` found the
token fused four causes across two files, and asked whether a missing `[profile.release]` is a
failure at all. @test coupled their `manifest_without_release_profile_fails` verdict to the answer.

**Settled by measurement, twice, independently.** @observability built a throwaway crate printing
`cfg!(debug_assertions)`; I built one carrying ADR-0036 §11's *exact* control
(`#[cfg(debug_assertions)] compile_error!`). Both agree: with **no `[profile.release]` table**, a
`--release` build has debug-assertions **off** — cargo's built-in defaults — so the premise
**holds**. The guard was reporting a premise break where there was none. My own line-continuation
test exists to defend the same direction; I had the defect in the neighbouring arm.

**Resolution**: that path now passes and falls through clean; both test tiers flipped in lockstep to
`manifest_without_release_profile_passes_via_cargo_defaults`. The token split reduced to two rather
than three: `workspace_manifest_unreadable` (absent/unparseable) and a new
`cargo_config_unparseable` — @observability is right that a token saying `workspace-manifest` sends
a triager to a file that is fine, and that this class **is** causable by an ordinary edit, so it
must not inherit the manifest classes' "you cannot cause these by editing a file" clause in
`devloop-validation.md` §8. @observability's control run also **empirically confirmed row 6**
(`CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS` flips it with no profile table present at all), which had
only ever been reasoned from documented semantics; recorded in the module doc so a future reader
tempted to drop it as theoretical has a result to weigh.

**Precision, per @operations at Gate 3 — no token was removed from the vocabulary.** An earlier
message of mine described this as "one token is GONE", which reads as a removal and would send a
future reader hunting for a catalogue row that was never wrong. All seven violation tokens
(including `release-profile-debug-assertions-enabled`) still exist. What changed is a **code path**:
the missing-`[profile.release]` case no longer *routes* to a failure, because the premise holds
without the section. It is a false-positive removal, not a vocabulary change, and needs no §8 edit —
`workspace-manifest-unreadable` still accurately means "root `Cargo.toml` absent or unparseable".
@operations updated §8 to split the precondition tokens into **environmental** (five — start at the
discovery code) and **causable-by-an-ordinary-edit** (`cargo-config-unparseable`,
`service-roster-underivable` — go fix the named file), and fixed the same over-broad "do not look in
the input tree" clause in the §6.3.1 hunk, which had been correct only while all five were
environmental.

### Issue 11: folding the double read silently dropped a channel
**Problem**: fixing @code-reviewer's MINOR 2 (Dockerfiles read twice, strict then tolerant) by
folding the execution-path scan into the discovery loop, my first edit's anchor did not match
post-`fmt` text — so the call was removed with the second loop and never re-added. Dockerfiles
stopped being scanned for `RUSTFLAGS` / `CARGO_PROFILE_*` entirely.
**Resolution**: caught immediately by `cargo_profile_env_in_dockerfile_fails`. Re-inserted, and
placed deliberately *before* the no-cargo-line `continue` so an env override in a build context is
flagged regardless of which stage runs cargo. A cleanup that silently removed coverage is the exact
thing per-channel tests exist to catch, and they did.

### Issue 12: the magic channel count had already drifted before shipping
**Problem**: @dry-reviewer (F-DRY-D) and @code-reviewer (MINOR 3) both flagged
`let inputs_checked = 9;` — a hand-typed literal in the operator-facing SCOPE line, disagreeing with
the module doc's 7-row table and with `RULE_ORDER`'s 7 non-precondition entries. Three encodings,
two wrong, in the line whose purpose is making the scanned scope legible.
**Resolution**: derived from `RULE_ORDER` minus the precondition classes, and the **numeric claim
removed from the module doc entirely** rather than synced — a number in prose is a second encoding
of what the table already carries. Pinned by `enumerated_channel_count_is_derived_from_rule_order`.

---

### Issue 13: F-DRY-F — the same silence pattern, one branch below the one we fixed
**Problem**: @dry-reviewer reopened at Gate 3 after applying **Lesson 12's own generalisation** to the
diff — sweeping for siblings of "a conjunct that silences a check on unmatched input" instead of
stopping at the instance that produced it. Two sites, both bypassing
`common/scan.rs::warn_skip`, which exists for exactly this pattern and has **8 consumer modules**:

- `no_insecure_browser_flags.rs` — `let Ok(content) = … else { continue; }`, commented "binary or
  unreadable".
- `release_build_profile.rs` — `if let Ok(content) = …` with no `else`, over the CI-workflow sweep.

ADR-0019 blocking class (shared home exists, reimplemented) rather than a tracked extraction.

**The substantive stake, verified rather than accepted.** `SCOPE:` prints `files.len()` — the
*candidate* count — while a file that fails to read is `continue`d and never scanned. So the count
and the coverage claim disagree, silently, in the guard whose green is precisely *"none of the N
enumerated spellings appear in the scanned set"* and whose whole value is coverage of a security
prohibition. A strong vacuity bail existed for **zero** files; **some** files could drop out with no
signal at all.

**Resolution, and the asymmetry is principled rather than arbitrary.** Both sites now use
`warn_skip`. Only `no_insecure_browser_flags` additionally **fails closed**, on a new
`unreadable_candidate_file` precondition token:

- **It fails** because every file in its set carries equal weight in one coverage claim, so losing
  one weakens the claim itself — the same rule already accepted for `dev-web.sh`'s missing `ss`
  ("a check that CANNOT RUN is not a check that passed").
- **`release_build_profile` warns** because its *primary* premise surface (Dockerfiles, workspace
  manifest, cargo configs) is already strict — those reads propagate with `?` or record an explicit
  precondition hit — and the CI-workflow sweep is the *secondary* belt-and-suspenders half of the
  value-keyed `RUSTFLAGS` rule (@observability's finding). Losing one workflow degrades a backstop,
  not the premise.

Both directions are pinned by tests, and the asymmetry itself is pinned
(`unreadable_workflow_warns_but_does_not_fail`) so it reads as a decision, not an oversight.

**Measured before choosing to fail closed**: 0 of 1407 tracked candidate files are unreadable today,
so the strict path has no false-positive cost on the real tree. **Differential verified**: reverting
only the fail-closed half (keeping `warn_skip`) makes the new test red.

Also added, in the shape `ts_retained_credentials` documents: a **WARN-free-run contract** on both
real-tree tests — a clean run must emit no `warn_skip`, because a skip means `SCOPE` over-counts the
scanned set. Asserting the *absence* is what keeps the count honest.

### Issue 14: an expired audit suppression tripped Layer 3, from outside this diff
**Problem**: the final Layer 3 sweep went red on `audit-suppressions` —
`RUSTSEC-2023-0071 expired 1 day(s) ago (expires 2026-09-01)`. The session's date rolled to
2026-09-02 mid-loop.
**Not this devloop's**: the suppression is pre-existing (last touched `a37b667`, 2026-06-07, three
months prior), untouched by this diff, and unrelated to the `toml` dependency added here — it covers
`rsa 0.9.10` reached via `sqlx-mysql`. It is a **fail-closed expiry control working as designed**.
**Resolution**: escalated to @security and @operations rather than renewed. Bumping the date would be
the masked-failure pattern this devloop is about, and the renewal is a security judgement, not an
implementer's. I ran the re-verification the suppression's own reason line prescribes and handed over
the evidence: `cargo tree -p rsa --invert --edges normal` and `--target all` both print *nothing to
print* (in `Cargo.lock`, **not in the build graph**); `sqlx` features are `postgres`-only, no
`mysql`; no `rsa::` anywhere in `crates/`; `cargo audit` exits 0. So the stated rationale holds and
the fail-closed trigger ("if a runtime consumer ever appears") is **not** met — which makes
*dropping* the suppression a live option alongside renewal. That call is @security's.

---

### Issue 15: the same class again, in the dialect nobody swept — and one was a real silent pass
**Problem**: @dry-reviewer closed F-DRY-F after re-running their sweep across both new Rust modules
and confirming no siblings. I re-checked their Rust result (clean — every remaining site is a
converter, a documented flag, or the crate-wide walkdir idiom) and then extended the same method to
the **bash** half of the diff, which their sweep structurally could not reach. Two more instances,
both mine:

- **`LOCALHOST_FIRST` — a genuine silent pass.** `getent` absent or failing leaves it empty. The
  IPv6-mismatch test is `[[ "$LOCALHOST_FIRST" == ::* && … ]]`, which is merely **false** when the
  value is empty — so the check could not fire, control fell through to the `elif has4 || has6`
  arm, and a hostname resolving IPv6-first against an IPv4-only listener **reported PASS**. That
  configuration is a guaranteed QUIC timeout and silent no-audio: the exact outcome this whole
  escalation exists to prevent, waved through by the escalated check itself.
- **`ss` present but failing** — restricted container, missing `/proc/net`. The snapshot came back
  empty, so every port read as unlistened and all four instances reported *"nothing listening — is
  the cluster up?"*. It fails closed, so not a silent pass — but it sends the reader to restart a
  healthy cluster. Same misdirecting-remedy class as the unprefixed jsonpath @operations caught in
  OPS-3: not wrong-and-quiet, but wrong-and-confident.

**Both were unreachable on today's tree** — every configmap advertises the IPv4 literal `127.0.0.1`,
so the hostname branch is dead code — which is precisely why they would have sat there until someone
switched to a hostname, the moment they matter most.

**Resolution**: hostname-with-no-resolver and `ss`-failed both become CANNOT VERIFY hard fails with
their own remedies, consistent with the taxonomy already agreed (`a check that CANNOT RUN is not a
check that passed`). `WT_SS_OK` now distinguishes *"ss ran and found nothing"* from *"ss failed to
run"*, and the second explicitly tells the reader **not** to restart the cluster on that signal.

**Differential proven, not asserted**: removing only the `LOCALHOST_FIRST` guard makes four
assertions red, including `hostname-no-resolver-does-not-pass`, which reports
`WebTransport listener on demo.localhost:4434` **WAS present** — the pre-fix clean pass, captured.
Suite is 61 assertions (was 51).

The lesson generalises past the instance: F-DRY-F's class had **four** members across two languages,
and a per-language sweep closes only what that language can see. When a class is named, sweep every
dialect in the diff.

### Issue 16: I was wrong that dropping the audit suppression was an option
**Problem**: escalating the expired `RUSTSEC-2023-0071` suppression, I offered @security two paths —
renew, or **drop** it, reasoning that `cargo tree` showed `rsa` absent from the build graph.
**@security corrected it, and the correction is the useful part**: `cargo audit` reads
**`Cargo.lock`, not the build graph**. They ran it against an isolated copy of our lockfile with no
suppression config at all — `RUSTSEC-2023-0071`, severity 5.9, *"No fixed upgrade is available!"*,
`error: 1 vulnerability found!`. So dropping the entry makes Layer 6 permanently red with no fix
available, converting a governed, expiring, re-justified suppression into standing pressure to do
something worse. **Renewal is the only viable path and is the designed one.**

My evidence-gathering was right and reproduced exactly; my *inference* from it overreached, because
I reasoned about the graph while the tool reasons about the lockfile. Recorded because the shape is
worth keeping: verifying a rationale is not the same as verifying what a tool will do with it.

Renewal is @operations' to land, on @security's terms: expiry **2026-12-01**; rationale corrected
from "pulled transitively by sqlx-macros-core" to **lockfile-only** (a stronger position than the
comment claims — both invert commands print nothing); fail-closed trigger unchanged; derived files
regenerated via `scripts/audit-suppressions-check.sh --fix`; and landed as **its own labelled
commit** so the precedent is greppable rather than buried in a guard change.

---

### Issue 17: OPS-10 — the header under-listed the cannot-run causes, and nothing pinned it
**Problem**: @operations found that after Issue 15 added two branches, the body had **four**
`CANNOT VERIFY` causes (no advertise address, `ss` failed, `ss` missing, no resolver) while the
header enumerated **two** — and the suite stayed 61/61 green through the change. Verified by
counting both before touching either.

**Weighted honestly, as @operations did: the semantics were never wrong.** The contract paragraph
below states the general rule — *"A check that CANNOT RUN is a hard fail too, not a pass"* — which
covers all four correctly. The defect is narrower: the parenthetical reads as **enumerative**, and
after the §3 Step 0 deletion the runbook cites `--help` as the SSoT for which check carries which
severity. A reader asking *"is getent-unavailable a hard fail?"* scans the list, fails to find it,
and needs to notice the general rule underneath to get unstuck.

**The durable half is the assertion gap, and it is OPS-9 one axis over.** The drift block pinned the
severity *tags* and the absence of `warn`; nothing pinned the header's *cause list* against the
body's branch set. Same blind spot shape: an assertion covering one property of a block while the
neighbouring property drifts silently underneath it.

**Resolution**: header completed to all four causes and marked explicitly enumerative (rather than
softened to "e.g." — @operations' point that "e.g." in an SSoT invites the next reader to guess is
right). Plus the self-maintaining assertion, built so it **cannot rot in halves**: one
`CANNOT_RUN_CAUSES` array drives *both* checks — every phrase must appear in the header, **and** the
body's branch count must equal the array's length. A fifth branch reds until its phrase is added to
the header *and* the array.

**Both halves proven to fire, separately**: reverting the header to the two-cause form reds all four
phrase assertions; adding a fifth body branch with the header untouched reds the count assertion
with the remedy in its message. Suite 61 → 66.

---

## Lessons Learned

1. **A guard's coverage claim is a load-bearing artifact, not documentation.** The strongest single
   correction in this loop (@security S1/S2, sharpened by @observability §I20) was not about what
   the guard *does* but what it *says*: the module doc is what a reader doesn't see, the STATUS line
   is what they do, and `STATUS=OK REASON=no-insecure-browser-flags` reads as a universal claim when
   the truth is "none of N enumerated spellings in the scanned set." Both OK tokens now name the
   enumeration, never the conclusion.
2. **"Nothing to update by hand" has to be literally true.** I wrote it about the anti-vacuity floor
   and it wasn't: `CANONICAL_SERVICES` is a hand-maintained literal, so a service added later would
   never enter the floor — the anti-vacuity check going vacuous for exactly the new service, this
   task's pathology one file to the left. Deriving from `[workspace] members` plus a drift rule
   turned an unguarded mirror into a guarded one.
3. **Replace controls that require noticing, including in the fix.** @operations refused a Gate-3
   co-sign for the runbook duplicate on the grounds that a human check is the control class this
   devloop replaces, then applied the same standard to their own OPS-7 endorsement (sentinel test,
   not eyeball). Consistency under one's own rule is harder than stating it.
4. **A remedy string is an output and needs testing.** @operations' `kubectl` command used an
   unprefixed key that returns *empty* rather than erroring — manufacturing the exact false
   conclusion the remedy existed to prevent. Wrong-and-quiet, in a line written to prevent
   wrong-and-quiet. The suite now asserts remedy content two-sided.
5. **Check whether the work is already assigned before designing how to do it.** My §D would have
   minted a third spelling of an anchor that was frozen upstream and already referenced by two other
   consumers — in a plan whose subject is things drifting silently.
6. **Verify with the wrapper the gate runs, never a hand-built equivalent.** `cargo clippy … | grep
   -cE "^error"` returned 0 for a failing run because clippy colorizes and `^error` cannot match an
   ANSI-prefixed line. I built a check whose passing condition was unreachable and then signalled on
   it. `scripts/layer5.sh` carries the flags and reads the STATUS line; the hand-built version
   carried neither guarantee. Generalisable: **any verification that reformats a tool's output is a
   place a vacuous pass can hide.**
7. **The self-consistency condition fired twice, unprompted, and both times at me.** The
   flags guard failed on this very record at Gate 1 (two prohibited literals quoted while relaying
   reviewer messages) and again at Gate 2 (the bug descriptions in §Issues spelled out the property
   assignment verbatim). Both fixed at the source by de-literalising; neither by an allowlist entry.
   A guard whose own devloop record keeps tripping it is a guard that works — and `scripts/dev-web.sh`
   plus this file both sitting inside its scan surface is what makes the never-name-the-flag rule
   mechanical rather than stylistic.
8. **An extraction is part of an assertion's soundness, not plumbing.** OPS-9's hole was one `sed`
   range anchored on the severity word it was meant to police — self-referential in the way that
   guarantees a revert erases the evidence. The general rule: **anchor on what a change leaves
   alone, and assert non-vacuity before asserting content.** I had written five vacuity tokens into
   the guard fifty lines away and did not apply the same rule to the test asserting the guard.
9. **"Defensive" guards that convert failure into silence are the most dangerous line in a
   validator.** `!derived.is_empty()` looked like care and was the one place in the module where a
   derivation failure did not fail loudly. Three reviewers found it independently, which says the
   shape is recognisable once you look for it: **a conjunct that suppresses a check when its input
   is missing is almost always inverted** — missing input is the finding.
10. **Measure before defending a fail-closed choice.** I would have justified the missing
   `[profile.release]` failure as "the guard wants an explicit table". Two independent experiments
   showed cargo's release defaults make the premise hold, so the honest answer was that my guard was
   false-failing. Deciding it by argument would have cemented a false positive that already had a
   test pinning it as intended.
11. **A lint denial is a question, not a chore.** Three `indexing_slicing` sites looked like
   mechanical `.get()` swaps. One was concealing a live bypass in a security guard — a decoy
   occurrence could hide a real violation on the same line — and a UTF-8 false positive. @team-lead
   was right to say which site to look at hardest and to forbid a scoped allow; had I reached for
   `#[expect]`, both bugs would have shipped with the lint silenced on top of them.
12. **A generalisation is worth more applied to the diff than written in the record.**
   @dry-reviewer took Lesson 12 below and *swept for siblings* rather than filing it, which found
   F-DRY-F one branch below the fix that produced it. The lesson I'd written was the search key;
   writing it down was not the same as running it. Worth doing at every gate: when a pattern is
   named, grep the diff for it before closing.
13. **A class sweep is only as wide as the dialect it runs in.** F-DRY-F was closed after a
   thorough Rust sweep; the same class had two more members in the bash half of the same diff, one
   of them a real silent pass. Per-language sweeps close per-language subsets. When a pattern is
   named, sweep every language in the changeset.
14. **An assertion pins the property you wrote, not the block you pointed it at.** OPS-9 and OPS-10
   are the same blind spot on different axes of one header: the drift block checked severity tags and
   `warn`-absence while the extraction range (OPS-9) and the cause enumeration (OPS-10) drifted
   underneath it. When a test extracts a block, ask which properties of that block are *not* being
   asserted — those are where the next drift lands.
15. **The durable lesson of this devloop: three of the four behavioural findings were a
   defensive-looking conjunct converting failure into silence** — `!derived.is_empty()`, the
   swallowed second parse, and the comment-anchored `sed` range — **and none was caught by my own
   tests.** That is the same shape as the task's subject (a control that is inert while looking like
   coverage), found three times inside the tooling built to catch it. The recognisable tell:
   whenever a predicate reads as "be careful when the input is missing/empty/unmatched", check
   whether it has actually converted *"I could not evaluate this"* into a pass. In a validator that
   is almost always inverted — the un-evaluable input is the finding, not the exemption.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
