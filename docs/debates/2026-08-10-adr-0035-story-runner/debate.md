# Debate: Ratify ADR-0035 — Deterministic Story Runner (run-story)

**Date**: 2026-08-10
**Status**: Complete — consensus reached, ADR-0035 Accepted
**Participants**: infrastructure, security, test, observability, operations, code-reviewer

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

Stress-test and ratify draft ADR-0035 (`docs/decisions/adr-0035-story-runner.md`), the Deterministic Story Runner (`run-story`). The draft is written author-first from the Run #1 build session as a **debate seed**:

- Sections marked **[SETTLED]** document decisions already made and validated in Run #1 — **ratify or amend, do not re-litigate from scratch**.
- Sections marked **[OPEN]** (A–G, N) are the genuine debate agenda.

The ADR instructs: **anchor on B, C, D, E** — these are where independent lenses change the answer. Treat the consequential skill batch as ratify-and-sequence.

### Open Questions (the agenda)

- **A. Substrate durability** — float the CLI (auto-update + probe) vs. pin. Agent SDK verified off the table (no agent teams). *(infra, security)*
- **B. Does the full 8-reviewer panel earn its cost?** ~$30–50/Opus task; the one known escaped defect (credential-lifetime) got through the *full* panel. *(test, operations)*
- **C. Gate-2 binding artifact consumption** — (i) trust the whole tree-bound verdict, (ii) re-run the whole pipeline (status quo), or (iii) extend `emit_gate2_verdict` to per-layer status so "consume 1–6, re-run 7" becomes possible. *(test, security, operations)*
- **D. Is serial-single-branch permanent or a v1 simplification?** *(infra, operations)*
- **E. The runner is untested** — `run-story.sh` has zero tests. Hermetic suite now? *(test)*
- **F. Model tiering beyond "Opus default."** *(operations/cost)*
- **G. Harder suppression gate in auto-remediation** — force a human stop on a suppression-*adding* remediation task? *(security)*
- **N. Should major process machinery require an ADR up front?** *(operations)*

## Context

Background is the full draft ADR at `docs/decisions/adr-0035-story-runner.md` (read it in full — Context, Decision/[SETTLED], Open Questions, Consequences, Alternatives Considered, Implementation Status, and the Run #1 incident taxonomy).

Implementation under debate:
- `scripts/workflow/run-story.sh` (~22KB, the serial loop — the untested subject of E)
- `scripts/workflow/preflight-story.sh`, `scripts/workflow/devloop-stop-hook.sh`
- `crates/dt-story/src/{engine,manifest,markdown,lib,main}.rs`
- `scripts/lang/_gate2_binding.sh` + `scripts/guards/simple/selftest-gate2-verdict.sh` (subject of C)
- `scripts/lang/{rust,ts}/audit-remediation.md` (subject of G)
- Test-suite convention for E: `scripts/layer-all.test.sh`, `scripts/layer7.test.sh`, `scripts/setup.test.sh`

Related ADRs: 0024 (Agent Teams devloop), 0025 (containerized devloop), 0033 (polyglot validation pipeline), 0034 (guard pipeline as Rust binary).

**Cost note (new since the draft was written)**: Fable is now billed separately from the Max subscription and carries incremental cost. Any "use a stronger model for judgment work" reasoning in **F** must account for this.

## Positions

### Initial Positions

| Specialist | Position | Satisfaction |
|------------|----------|--------------|
| infrastructure | A: float+probe (+A1/A2). D: serial permanent, DAG option preserved. C: (iii) → folding to (ii). Ratifies [SETTLED] infra sections. | 85 |
| security | C: NO to (i), (ii) floor + C(iv)/C(v). G: hard gate. A: float+probe, no objection. 2 merge conditions. | 72 |
| test | C: (ii). E: blocking and larger than stated (needs seams first). B: correlated misses; acceptance-tests-first as a guard. | 72 |
| observability | Ratifies all [SETTLED]. Blocking: evidence trail in ephemeral `/tmp`, six ledger fields missing. | 65 |
| operations | C: (ii) + cross-check. D: serial, quota-based trigger. B: free retrospective + `review_mode`. F: downward-only. N: blast-radius test. | 74 |
| code-reviewer | B: wrong axis — mechanization ratchet; COI declared. E: extraction-then-tests. N: promotion-to-load-bearing trigger. | 72 |

**Round 1 complete — no consensus (all six below 90).** Critically, every "WOULD_ACCEPT_CURRENT: no" was justified by **amendable factual/scope defects, not by disagreement on the decisions.** Convergence on the open questions is high; see Synthesis below.

## Lead-verified corrections to the draft ADR

These are established facts for the remainder of the debate, verified by the Lead against source (not accepted on a participant's assertion). **The ADR carries these amendments regardless of how the open questions resolve.**

### 1. ADR line 46 is factually wrong — the Gate-2 artifact IS per-layer

The draft states, marked "**Verified 2026-08-07**", that the artifact is *whole-pipeline, not per-layer*, and therefore that the surgical "trust 1–6, re-run only 7" option is unsupported and option (iii) is "new machinery, not consume-what-exists."

Raised by @infrastructure; confirmed by the Lead reading `scripts/lang/_gate2_binding.sh`:

- **Lines 522–535** emit, inside every verdict, per-layer detail under the verbatim comment *"Per-layer recorded detail (RECORDED, not enforced)"*:
  `for n in 1 2 3 4 5 6 7; do printf 'LAYER %s %s %s\n' "$n" "${st:-UNKNOWN}" "${du:-0}"` — fed from `layer_status[]`/`layer_dur[]`, which `scripts/layer-all.sh:33,127` fills per layer and passes by nameref (`layer-all.sh:48`).
- The verdict is whole-pipeline **enforced**, but per-layer **recorded**. The draft conflated the two.

**Consequence 1 — (iii) is re-priced.** From "build new machinery" to "parse `LAYER n STATUS` lines that already exist."

**Consequence 2 — (i) and (iii) have identical trust surfaces.** `_gate2_binding.sh:510` documents the signature as *"over the canonical blob-first NUL record stream (NOT the FILE lines)"* — `sig="$(gate2_records_worktree | gate2_signature)"`. `SIGNATURE=` binds **which tree the verdict describes**, not **what the verdict says**; neither `GATE2=PASS` nor the `LAYER` lines fall inside it. Anyone who can forge the PASS bit can forge a LAYER line. **The draft's "larger trust surface" argument against (i) therefore does not distinguish (i) from (iii).** The real axis is *what gets independently re-executed*, not signature coverage.

### 2. Open sub-issue — the verdict binding is worktree-bound, not commit-bound

Raised by @infrastructure. The producer binds to the worktree changeset (`git diff HEAD` + untracked, `_gate2_binding.sh:503-506`). After the devloop commits, that changeset is empty, so the runner cannot re-verify a signature against the committed tree. Post-commit the only available inference is *"the pre-commit hook validated a PASS verdict for exactly this staged tree, or the commit wouldn't exist"* — caveats: the hook is slug-conjunct-gated, and `--no-verify` bypasses it.

Lead-surfaced fact (offered to @security/@test as data, not as a Lead position): the verdict also emits `HEAD=`, the pre-commit HEAD, which post-commit equals the new commit's parent — so `git rev-parse HEAD~1` may serve as a cheap partial re-binding. Adjudication belongs to @security and @test.

## Discussion

### Round 1

#### @infrastructure — initial position (A, D, C correction, [SETTLED] ratification, E co-sign)

Inspected: `run-story.sh`, `preflight-story.sh`, `devloop-stop-hook.sh`, `_gate2_binding.sh`, `layer-all.sh`, `dt-story` engine, and the devloop container (`entrypoint.sh` / `devloop.sh` / `Dockerfile`).

**A — float the CLI + probe; oppose pinning. Ratify as written, with two amendments.**

- Pinning is a three-site SSoT task, not a flag: `infra/devloop/Dockerfile:101` (`npm install -g`, unpinned), `infra/devloop/entrypoint.sh:63` (`npm update -g`, backgrounded every container start), `infra/devloop/devloop.sh:680` (`npm install -g`, every attach). A pinned image would still drift while `entrypoint.sh:63` races container readiness.
- Pinning defers an *undocumented experimental* breakage to a moment we don't choose (auth change, model retirement, CLI CVE), and costs one-version attribution. Float+probe fails at preflight with a named version before burning a devloop (`preflight-story.sh:63-85` caches the probe per `claude --version`).
- **A1 (documentation amendment)**: state that the probe-cache marker lives under `DEVLOOP_TMP` (`preflight-story.sh:64-66`), i.e. it is **per-container-lifetime, not per-version-forever**, so no one later "optimizes" it onto a persisted volume and reintroduces stale-probe risk. Explicitly *not* asking to change the behavior.
- **A2 (policy amendment)**: record float as a deliberate, revisitable posture with a named exit — **two substrate-probe failures within one quarter flips to pin**, and pinning then means closing all three install sites against one SSoT.
- **Stated condition to flip to pin**: evidence of a plausible **silent-degradation** mode — "teams still spawn but SendMessage delivery got lossy." The probe catches hard breakage, not silent degradation. Routed to @security and @test.

**D — serial permanent-by-default; keep the DAG option alive. Harder on serial than the draft.**

- 9 tasks in `browser-client-join` are `env_tests`-tagged; each runs layer 7, bringing up a live Kind cluster (postgres, redis, four services, prometheus, grafana). Parallel tasks each need their own cluster.
- **Ports are not the constraint** — `crates/devloop-helper/src/ports.rs` (20000–29999, stride 200) has ample headroom. WSL2 memory/CPU is, realistically 2–3 concurrent clusters. So parallelism only buys throughput on the *non*-env_tests (cheap) tasks; the expensive tail is serialized by physics regardless of scheduler.
- The draft's stated cost of serial ("one stuck task blocks the story") is already mostly bought off by escalated-retry (`run-story.sh:35` / `dt-story next` reopens escalated tasks) — a stuck task blocks until a human looks, and a human was going to look anyway.
- **The option is cheap to preserve**: `crates/dt-story/src/engine.rs:57` is `.find(|t| t.deps.iter().all(|d| completed.contains(d)))` — already a real DAG readiness predicate. Parallel is a scheduler change (`find` → `filter().take(K)`), not a data-model change. `deps` already exist (`manifest.rs:48`); validation already rejects dangling deps and cycles (`engine.rs:231,291`); `/user-story` already plans in waves.
- **Named risk**: under a serial runner nobody is punished for lazily writing `deps: [previous-task]` on everything; after 60 tasks the DAG has silently degenerated to a chain and the option expired unnoticed. Proposed detector: `dt-story validate` reports **critical-path depth vs. task count** (depth == count ⇒ chain). Explicitly *a metric, not a gate*.
- **Proposed revisit trigger**: more than one story where wall-clock is dominated by non-env_tests tasks **AND** no new failure class in two consecutive stories. Until both hold, serial.
- **Stated condition to flip to parallel-in-v1**: a measurement showing non-env_tests tasks dominate story wall-clock — *wanted before, not after*. Routed to @observability.

**C — see Lead-verified corrections above.** Recommendation: **(iii), scoped as "consume LAYER 1–6, independently re-run layer 7"**, landed **after E**, and made **commit-bound** (producer stamps the target HEAD, or the runner captures the verdict immediately post-commit and checks `HEAD=`/`SLUG=`). Rationale: the honest-mistake case (what Run #1 actually hit) is fully covered; the saving is real and recurring (layers 1–6 are the per-task minutes, ~15 tasks deep); layer 7 — the expensive, flakiest layer — keeps a genuinely independent verdict. Defers the forged-verdict threat call to @security, and **would drop to (ii) status quo without much fight** if @security judges that threat unacceptable for a runner operating unattended overnight (a materially different context from the interactive pre-commit hook the original threat model was written for).

**E — co-signed, strongly; offers to take the infra half of the suite.** `run-story.sh` is **465 lines with zero tests**, holding: a **`git reset --hard` + `git clean -fdq` (lines 349-350) reachable from a *classification* decision** — if `canary_classify` misfires, uncommitted work is destroyed; `sleep 93600` (line 340); resume-pointer persistence; an in-loop `git commit` (lines 229-230). Proposed shape: hermetic suite per the `scripts/layer-all.test.sh` convention, with fake `claude`/`dt-story` on PATH and a scratch git repo.

**[SETTLED] ratifications (infra sections):**
- *Serial / single-branch / single-container* — RATIFY, with the deps-truthfulness rider above.
- *Preflight-owned substrate registration* — RATIFY strongly; called "the ADR's best mechanism." `preflight-story.sh:38-60` patches `~/.claude/settings.json` in-container every run with a `jq -e` read-back verification (lines 57-59): removes the image-bake coupling that caused the task #64 idle-death, idempotent, fails loudly.
- *Execution substrate (headless `claude -p` + experimental teams)* — RATIFY the mechanism; durability handled under A.
- *Pass/fail authority = the runner's own pipeline run* — RATIFY the **principle**, with a requested clarification: the ADR should state that C is about *which artifact establishes* the runner's independent verdict, **not** about moving authority back to the devloop's self-report — so a later reader cannot misread C-adoption as a retreat from the false-CLEAR lesson.

**No standing claimed on B** (panel-size cost). Agrees with the draft that B is the long-term hedge for A.

**In-tree fix made during the debate** (fix-don't-defer): `scripts/workflow/devloop-stop-hook.sh:8` said the hook was "Registered container-side by infra/devloop/entrypoint.sh" — the exact coupling the task #64 fix removed, which would have sent an incident responder to the wrong file. Now names `preflight-story.sh` and cites the incident. Verified by Lead.

## Final scores — CONSENSUS REACHED

| Specialist | R1 | Final | Accepts |
|---|---|---|---|
| infrastructure | 85 | **96** | yes |
| code-reviewer | 72 | **96** | yes |
| operations | 74 | **96** | yes |
| security | 72 | **95** | yes |
| observability | 65 | **92** | yes |
| test | 72 | **91** | yes |

All six at 90+. **ADR-0024 §5.7 risk acceptance not triggered** — no cross-cutting specialist finished below 70, and none finished with an unresolved objection.

**Link**: `docs/decisions/adr-0035-story-runner.md` (Accepted, 2026-08-10).

### What the debate actually produced

Beyond the eight resolutions, the debate's output was **evidence and defects**, not opinion:

- **Six false claims in the draft corrected** — three of them ("Verified 2026-08-07" per-layer, "classified by field access", "the panel had passed" #63) are claims about *mechanism that the implementation does not support*. The pattern is recorded in §N as the argument for the inspect-the-artifact condition.
- **Defects found in shipped code: nine fixed in-tree** across five files during the debate, plus D2–D5 scheduled as E's opening test cases. **Six of the nine were found by reading recovered artifacts rather than source** — the escalation overwrite, the non-JSONL ledger, the recordless auth/infra lanes, the canary overwrite, and two of the false ADR claims. The artifacts found more defects than the source reading did, and they nearly did not survive to be read.
- **Four measurements that changed conclusions**: 1,184 SendMessage calls at 47% sibling (settled A, killed the SDK permanently, decoupled A from B); per-seat finding rates 31–63% (refuted the oversized-panel hypothesis); 17.1h work vs 106.4h gap (settled D on measurement — the story was 86% waiting); and the recovered cost ledger (replaced two unsourced figures with a distribution).

### A sixth false claim — introduced during the rewrite, not inherited

The draft carried five claims contradicted by the artifacts. A **sixth was introduced during the accepted rewrite**: a timing measurement restated after its author had retracted it as sleep-contaminated, alongside a double-count of the cost ledger's cumulative retry rows. Both were caught in the review pass, by the same artifact-inspection discipline.

That is the strongest evidence in the debate for the N condition. The failure mode is **not carelessness in a draft** — it is a standing hazard of writing decision records at all, recurring with six specialists actively hunting for exactly it. A related pattern appeared three times in three unrelated places (`SIGNATURE` naming itself an authenticity primitive while being an unkeyed digest; the verdict's `LAYER` lines carrying a record's shape without provenance; the probe marker asserting "substrate validated" from a filename encoding only a version). General form: **an artifact's name and shape routinely outrun what it actually establishes.**

### Method note — position changes against self-interest

Every participant except one changed a position against their own prior argument or interest, on evidence:

- **code-reviewer** opened arguing the panel was oversized and its own seat redundant; measured, found its seat highest-yield and no seat idle, and reversed. Then corrected its own residue-map proposal after noticing it had twice predicted yield from role *self-descriptions* — adding the citation requirement that fixes it.
- **infrastructure** withdrew "(iii) is cheap" and "layer 7 dominates the tail", both after its own premises were checked.
- **observability** was wrong three times (durable record, evidence recoverability, and the scope of its own ask), each caught by a teammate; also accepted a ruling against its own cache proposal on signal-integrity grounds from its own domain.
- **operations** corrected its own 4%→7% figure, withdrew retry-once-on-red, withdrew its baked-marker preference, and withdrew its own light-set composition as an over-read.
- **security** withdrew C(iv) — its own proposal — twice, and corrected its own "contains live tokens" framing after scanning and finding zero credential material.
- **test** recorded that its own stated threshold for supporting (iii) was checked and not met, and volunteered the caveat that cuts against its own side (no measurement of in-diff yield exists).

## Round 2 — scores and movement

| Specialist | R1 | R2 | Movement |
|---|---|---|---|
| infrastructure | 85 | **93** | Folded to C(ii) on its own pre-committed condition; ruled the container sentinel and implemented it; settled A empirically |
| code-reviewer | 72 | **91** | **Reversed its own opening argument on B with data**; measured the panel directly |
| test | 72 | **91** | Hardened C from "prefer (ii)" to "decline all three"; delivered the acceptance-guard shape; found D4/D5 |
| operations | 74 | **88→** | Costed C definitively; ruled F, N, sequencing; blocking item now closed |
| security | 72 | **86→** | Ruled C declined on the digest argument; ruled G scope; withdrew its own C(iv) on new evidence |
| observability | 65 | **72→** | Corrected itself twice; measured D from git; conditions now met |

`→` = conditions satisfied after the score was given; re-scores requested.

**No participant's remaining concern was a disagreement about a decision.** Every one was an amendable factual or scope defect. All eight open questions converged.

## Consensus

Reached on substance in round 2. Every open question (A–G, N) resolved with no dissent; the four mandatory cross-cutting specialists are at or above 86 with all stated conditions met, so **ADR-0024 §5.7 risk acceptance is not triggered.**

Notable: three participants changed position against their own prior arguments or interests — **code-reviewer** reversed its panel-is-oversized thesis after measuring finding rates (its own seat proved highest-yield); **infrastructure** withdrew its "(iii) is cheap" inference and folded to (ii); **observability** corrected its own central claim twice, each time weakening its own position. **security** withdrew its own C(iv) proposal on test's evidence.

## Decision

**ADR-0035 accepted with amendments** — `docs/decisions/adr-0035-story-runner.md`, rewritten 2026-08-10 and out for the review pass.

Resolutions: **A** float+probe (SDK off the table permanently — 47% sibling messaging measured), +A1–A4. **B** panel earns its cost, do not shrink; per-role mechanization boundaries + `review_mode` routing; both escapes were diff-locality failures. **C** declined in all three forms; (ii) + cross-check + C(v) + freshness check. **D** serial permanent, led by the quota argument; `--keep-going`. **E** the keystone and a strict predecessor; seams-then-suite; journal absorbed. **F** downward-only, per-seat, instrument first; re-filed to Group 3. **G** hard stop on any suppression-manifest change. **N** build-then-document, with the merged promotion-to-load-bearing rule.

Six factual claims in the draft were corrected. Sixteen defects were found in shipped code; two fixed in-tree during the debate (container-boundary enforcement; the CLI-update race), four scheduled as E's opening test cases.

---

## Defect register — found by inspecting the implementation during the debate

These are **new defects in shipped code**, not ADR-text disagreements. Several were found independently by multiple specialists (noted). This register is the debate's highest-value output and is largely independent of how the open questions resolve.

| # | Defect | Found by | Lead-verified |
|---|---|---|---|
| 1 | **No container guard.** `run-story.sh:11-12` documents "only safe behind the container boundary"; nothing enforces it. `preflight-story.sh:46-56` rewrites the operator's **global** `~/.claude/settings.json` (registers a Stop hook; sets `CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=0`, ungated and never reverted) and runs `claude --dangerously-skip-permissions` in a loop against the real tree — which also reaches `git reset --hard` + `git clean -fdq`. | security (S1), infrastructure (#3), operations (F2) — **3× independent** | ✅ `grep -rn 'dockerenv\|containerenv\|DEVLOOP_CONTAINER' scripts/ infra/devloop/` → **no matches**. Host `~/.claude/settings.json` is **clean** — preflight has never run on the host. Exposure is **prospective**; no cleanup owed. |
| 2 | **ADR line 46 states a false "Verified" claim** — the Gate-2 artifact IS per-layer. | infrastructure (proved by executing `emit_gate2_verdict`), test, operations — **3× independent** | ✅ `_gate2_binding.sh:522-535` emits `LAYER n STATUS DURATION`, commented "RECORDED, not enforced". |
| 3 | **No post-commit Gate-2 validator exists**, and the existing one **returns 0 vacuously** post-commit: `if [[ ${#staged_paths[@]} -eq 0 ]]; then return 0`. run-story consumes post-commit, when the index is clean. | test | ✅ Read `gate2_validate_commit` in full. |
| 4 | **`--stop-after` can silently no-op.** `:433` compares ids only for tasks *executed in this invocation*; if the target is already complete or renumbered, the runner runs the rest of the story **and the full story-close gate**. Load-bearing: `--stop-after` is the seam that halts the run before the runner self-modifies. | test | — |
| 5 | **`canary_probe` merges stderr into the JSON file** (`:114` `>"$out" 2>&1`). Any CLI warning invalidates the JSON → `jq` fails → classified `infra` → story halts. A **healthy but noisy** canary is misread as broken infrastructure — the exact class the canary was built to eliminate. | test | — |
| 6 | **`entrypoint.sh:63` runs `npm update -g … >/dev/null 2>&1 &`** — backgrounded and silenced. If it lands after preflight samples `claude --version`, the story runs a **never-probed** version under a marker naming the previous one, and every later story reads the stale marker and skips the probe. Defeats the version-cached probe that A's float posture rests on; the silencing violates "fail loudly, never mask." | security | — |
| 7 | **Probe cache + entire evidence trail live in ephemeral `/tmp`.** `RUN_DIR`/`RUN_BASE` = `${DEVLOOP_TMP:-/tmp/devloop}/story-runner`. Run #1's cost ledger, gate logs, canary outputs and escalation records are **unrecoverable**. Probe cache doesn't survive a container (re-pays a `timeout 420` two-agent probe per container); a *negative* probe's log evaporates too. | observability | — |
| 8 | **Stop hook allows the stop unconditionally once HEAD moves** (`devloop-stop-hook.sh:26-29`). A devloop that commits early and stops satisfies the mechanism **while skipping review entirely**, and the runner cannot tell. | security | — |
| 9 | **ADR line 34 overstates the canary**: "classified by field access" is only half true — `:124` and `:128` still grep human prose. Real control-flow consequence: if the API rewords quota messages, a quota event misclassifies as `infra` and the story **stops instead of sleeping through the window**, defeating the headline "come back to it done" promise. `:326-327` likewise screen-scrapes `resets 3:45pm` from prose. | code-reviewer (E-1), security — **2× independent** | — |
| 10 | **Cost rollup only runs after a green close gate** (`:459-462`); `escalate()` exits 1 and infra lanes exit 2 — so **the runs you most want cost data for produce none**. Fix: `trap ... EXIT`. | observability | — |
| 11 | **Stale hardcoded `STORY_MODEL` default `claude-opus-4-8`** (`:71`) — same drift shape as the stale `Co-Authored-By` trailers already batched. Deliberately not fixed inline: the correct value is what F decides. | operations | — |
| 12 | **`engine.rs` (355 lines, densest logic in the crate) has no `#[cfg(test)]` module** — exercised only black-box via CLI. The ADR's "dt-story has good coverage" needs a one-line honesty fix. | code-reviewer | — |
| 13 | **Ledger omits `model`, `specialist`, `outcome`, `wall_clock_s`, `gate_rc`, `gate_elapsed_s`** — all in scope at the call site. `report_task_cost` is called from **both** `escalate()` and the success path with identical row shapes, so cost-of-success and cost-of-failure are inseparable. Blocks F's evidence gate and B's cost side. | observability | — |
| 14 | **Canary classification verdicts are computed and discarded**; the `healthy` branch (`:358-361`) logs **nothing**. Session-limit sleeps get a console line but no structured record, so "hours spent asleep" is not computable — the number that distinguishes a throughput problem from a quota problem. | observability | — |
| 15 | **Escalation history is destroyed on retry** (`engine.rs:87-90` sets `escalation = None`); `$RUN_DIR/escalation.json` is one fixed path, overwritten each time. A task that escalated 3× and passed on the 4th is byte-identical to one that passed first try. | observability | — |
| 16 | **`run-story.sh` has no test seam of any kind** — `REPO_ROOT` from `BASH_SOURCE` (always the real repo), unconditional `preflight` call, hardcoded `target/release/dt-story`. A naive suite written today would be **destructive to the developer's machine**. | test | — |

Also fixed in-tree during the debate (fix-don't-defer): `devloop-stop-hook.sh:8` claimed registration by `infra/devloop/entrypoint.sh` — the exact coupling the task #64 fix removed. Now names `preflight-story.sh` and cites the incident. (infrastructure; Lead-verified)

## Empirical evidence gathered during the debate

### Panel retrospective (commissioned by code-reviewer; @operations' "free experiment")
166 of 214 devloops parsed; the 13 August loops hand-verified; ~370 reviewer-observations.

| Reviewer | CLEAR rate (Aug / corpus) | Character |
|---|---|---|
| Security | 69% / 79% | Low volume, high value |
| Test | 58% / 63% | High yield, mostly non-automatable |
| Observability | 38% / 74% | Highest-value non-automatable class |
| Code Quality | 38% / 63% | **Highest raw yield, most automatable** |
| DRY | 77% / 80% | Output dominated by TODO breadcrumbs, not defects |
| Operations | 50% / 71% | Non-automatable |
| Semantic Guard | 77% / 93% | **~3 unique findings in 55 loops (~95% no-unique-yield)** |

- Semantic Guard: 2 of 3 August findings were **explicit duplicates of Security at the same site**; 2 of 7 lifetime findings were against guard tooling it was itself co-developing.
- Code Quality exhibit: `2026-05-23-…-task24/main.md:213` — *"8 clippy/fmt findings under `-D warnings`"* = clippy output routed through a human-shaped reviewer.
- **Highest-value class corpus-wide: findings *about the automation silently passing*** — operations catching `validate-env-config` silently skipping MC+MH (`2026-07-05-mc-otel-wiring-task6/main.md:331`); security catching the Layer-6 audit gate failing open (`2026-06-08-…-task50/main.md:567`). **A guard structurally cannot catch a claim about the guard.**
- Methodological: **`RESOLVED-DEFERRED` frequently means zero findings** — verdict-label counting overstates yield. 45 distinct reviewer-table header shapes across the corpus; only 47 files canonical; 48 unparseable. Schema drift in a human-authored record — the argument for machine-emitted structured events with an asserted schema.

### Escaped-defect traces — **neither is a headcount failure**

**Credential-lifetime**: the panel missed it. Security's own note reads *"pw in-memory-only note (sdk-core API, OOS)"* — it **saw** the retained password and scoped it out as someone else's API. Found by **the user, manually**, chasing an unrelated AC 409 during browser-join debugging (`7b69288`). Root cause: the semantic credential-leak check was Rust-only and scoped to *exfiltration*, not *lifetime*. Fixed by a **new lens + a full-tree mechanical guard**, not another reviewer. → **a lens-coverage failure.**

**#63 display-name**: the panel **saw it and correctly scoped it out** (reversing MINOR-003 in MC is a meeting-controller Domain-judgment change, owner-implements per ADR-0024 §6.3). The #63 Lead flagged the hole in bold twice; operations' verdict reads **"carrier inert."** The escape was in **story bookkeeping** — the spin-out became an unchecked TODO, #63 was marked `completed`, and its acceptance criterion was *"manual verification note in main.md."* → **a bookkeeping failure.**

**Consequence: ADR line 80 is misleading.** It claims *"#60's tests caught a cross-service defect that the seven-reviewer panel had passed."* The panel did not pass a defect; the completion bookkeeping accepted prose as acceptance for machine-assertable behavior. This makes the trace far stronger evidence for **acceptance-tests-first** than for any panel-size conclusion.

## Synthesis — convergence state after round 1

| Q | Converged position | Status |
|---|---|---|
| **A** | **Float the CLI + probe.** Nobody argued for pinning; security found no security basis for it (pinning doesn't reduce vendor trust, only changes when we ingest; ADR-0025 isolation is the real blast-radius control). Nobody produced the silent-degradation evidence infrastructure named as its own flip condition. **+A1** (probe cache is per-container-lifetime — document it) **+A2** (named flip-trigger: two probe failures in a quarter → pin, which means closing all three install sites against one SSoT). **Condition**: fix defect #6 (async/silenced npm update) — otherwise the probe does not bind the version that runs. | **Settled** |
| **B** | **Not a headcount question and not the Fable/Opus A/B.** The ADR's deferral fails twice: it conflates B with F, and Fable's separate billing removes the cheapness. The free retrospective has now been *run*. Neither escaped defect is headcount-attributable, and an escaped-defect-rate A/B is likely **underpowered by construction** (2 escapes in 214 devloops). Resolution: **route by surface** — a `review_mode: light\|full` manifest field set at `/user-story` planning time, defaulting to full, using the **existing** `--light` mode (3 teammates) and its existing any-reviewer escalation rule. Data indicates Semantic Guard and DRY are the empirically weakest seats. **Caveat on record (code-reviewer)**: the deterrent effect is invisible to finding-count data and cannot be settled by it. | Converging — light-set membership open |
| **C** | **(ii) re-run as the floor**, plus three cheap additions: **operations' cross-check** (read `GATE2` + `LAYER` before re-running; emit `STORY_RUN: VERDICT-MISMATCH` on disagreement — free flake/forgery detector and the only dataset that could justify revisiting (i)/(iii)); **security's C(iv)** asymmetric trust (believe the artifact when it *accuses* — FAIL/missing/mismatched escalates immediately without running anything); **security's C(v)** (assert the task commit contains a main.md at `Phase=complete`). Decided by defect #3 (no post-commit validator) + security's threat model (the adversary is an agent optimizing for turn-completion under a Stop hook **we built** to forbid ending without committing) + the C↔D composition (serial-single-branch makes per-task gate integrity *more* load-bearing; a false PASS at task 7 of 15 isn't caught until story close). Priced at **~4% of task wall-clock, $0 tokens** (layers 1–6 ≈ 305s, layer 4 = 229s of it, vs a ~2h task). **The lever for pipeline minutes is layer 4, not the trust surface** — that helps CI and every interactive devloop. | **Settled** pending infrastructure's fold |
| **D** | **Serial permanently for execution.** Decisive argument is operations', not the physics one: **parallelism buys nothing while quota is the binding constraint** — N parallel tasks drain the window N× faster then all sleep; steady wall-clock becomes bursty wall-clock plus longer sleeps. Layer 7 is structurally hostile (the 2026-06-29 Prometheus `PRECONDITION_FAILURE` root-caused to kube-proxy NodePort iptables churn on a resource-starved single node; concurrent cluster rebuilds would make layer 7 flaky **by construction**, and a flaky gate unattended is worse than a slow one). **Revisit trigger**: quota no longer binding (API-billed execution or a materially larger window) **AND** a per-task layer-7 cluster isolation story — both, not either. **+ deps-truthfulness rider** (`dt-story validate` reports critical-path depth vs task count; a metric, not a gate) so the DAG option doesn't silently expire — `engine.rs:57` is already a real readiness predicate, so parallel is `find` → `filter().take(K)`. **+ `--keep-going`** (opt-in, default off) to fix head-of-line blocking without parallelism: on escalation, record, skip that task and its dependents, continue with dep-satisfied tasks, exit 1 at the end with the full queue. **Decided without the measurement** — observability confirmed no wall-clock or sleep-duration data exists. Recorded as such. | **Settled** |
| **E** | **The keystone; hard-ordered before everything, including C.** Larger than the ADR states — it is **seams first, then suite**: `run-story.sh` has no test seam at all, so a naive suite would `git reset --hard` the real tree and rewrite the real `~/.claude/settings.json`. Scope: E1 seams (repo-root override, `DT_STORY`, layer-script dir, preflight bypass, audit-glob root — `DEVLOOP_TEST`-gated and rejected under `GITHUB_ACTIONS` per the `layer-all.test.sh` precedent) + **extraction of four functions** (`run_task_devloop`, `handle_devloop_failure`, `run_gate`, `audit_remediation_gate` — you cannot hermetically test the body of a 235-line `while :;` loop) + the suite (PATH-injection of `claude`/`sleep`/`date` under `env -i` needs zero source changes) + **journal schema assertions** + the container guard (defect #1, a seam E needs anyway). Wire into `layer3.sh` alongside the other `*.test.sh`. | **Settled** |
| **F** | **Downward-only, evidence-gated.** The Fable billing fact **inverts the asymmetry**: upward tiering now costs real incremental money; downward tiering saves *subscription quota*, and quota is what binds story wall-clock (D). Mechanism: a `model:` manifest field with `STORY_MODEL` as story-level default. **Gate the downgrade on evidence, never a guess**: a task class with ≥N observations at one-iteration-clean and zero escaped defects becomes downgrade-eligible. **Blocked on defect #13** — the trigger is unsatisfiable without a `model` field in the ledger. Fold defect #11 (stale `claude-opus-4-8` pin) into F's task. | **Settled**, blocked on #13 |
| **G** | **Yes — hard stop, scoped to the audit-remediation gate.** Promote `run-story.sh:426-431` from `slog NOTE` to `exit 1`; **widen the detector** to any addition to `audit-suppressions.toml` (not just added `id =` lines — "extend" is equally a policy decision and is invisible to the current regex). Watching the SSoT rather than generated files is correct and stays. The "breaks come-back-to-done" objection is void: the remediation task is appended at story close **after every tracked task is complete**, so stopping costs **zero** downstream work. The human's "I reviewed it" gesture is **rerunning the runner**, reusing [SETTLED] escalated-retry semantics rather than inventing an ack file or a flag. | **Settled** |
| **N** | **Build-then-document was correct here; ratify with a real test rather than a platitude.** No up-front debate produces the Run #1 incident taxonomy (unregistered `subagent_type`, print-mode idle-death, unbaked entrypoint, credential rotation) — an ADR written first would have specified handlers for *anticipated* failures, which §Failure-handling's twice-then-mechanize rule explicitly rejects. **Rule (operations' blast-radius test)**: debate-first is required when the work (a) changes a gate other people's commits pass through, (b) touches production/deploy surface, or (c) cannot be abandoned cheaply. Otherwise build first **with a written kill criterion** (which this had — the part that made it an experiment rather than a hunch) and an ADR **before it becomes load-bearing for anyone else**. **Compatible refinement (code-reviewer)**: promotion to load-bearing owes a **test suite at the same moment**. By that test run-story was mostly inside the envelope; two things pushed outside — the `/devloop` SKILL.md edits (criterion a) and defect #1's global-settings mutation (criterion c). **Meta-evidence (infrastructure)**: defects #1 and #2 were found by *inspecting the implementation during the debate*. An ADR-first debate would have ratified line 46's false claim, because there'd have been no code to check it against. | **Settled** |

### Cross-cutting outcome not attached to any lettered question

**Acceptance-tests-first is the best-evidenced item in the debate** and is promoted to run immediately after E. Its "ban *manual verification note* as acceptance for machine-assertable behavior" must be **a guard, not prose in a skill file** — a prose ban is an instruction followed by a model, which is exactly what the ADR-0016→0021→0022→0024→0035 arc exists to replace with deterministic enforcement. The #63 trace is the worked example of the failure it prevents.

### Mandatory ADR amendments (independent of any open question)

1. Correct line 46's false "Verified 2026-08-07" claim (defect #2), and add C's unstated **worktree-bound** precondition (`_gate2_binding.sh:503-506`).
2. Correct line 34's "classified by field access" (defect #9).
3. Correct line 80's panel-miss framing of #63 (it was a bookkeeping escape, not a panel miss).
4. Correct Implementation Status line 101 — it undersells the existing structured per-task JSONL cost ledger (`run-story.sh:78-92`), which matters because every observability ask is a ~30-line increment to an existing emitter, not new machinery.
5. Reconcile `:45` "$30–50/Opus task" vs `:86` "$40/Opus task" — two unsourced figures for one quantity, in one document, and the entire quantitative basis for B and F. Source them or drop both.
6. Correct "dt-story has good coverage" — `engine.rs` has no unit tests (defect #12).
7. State that C is about *which artifact establishes the runner's independent verdict*, **not** about moving authority back to the devloop's self-report, so C-adoption is never misread as retreating from the false-CLEAR lesson.
8. State the `--dangerously-skip-permissions` ↔ ADR-0025 container-boundary dependency in Consequences, referencing defect #1's enforcement.
9. Record run logs as **do-not-promote** (`task-N.devloop.log` is full stream-json of a credentialed session; `task-N.canary.json` is a raw API response).
10. Add to Consequences/Positive: **because no model sits in the control flow, every state transition keys on an exit code — so a structured event log is a faithful record rather than a model's narration of itself.** An underrated advantage over an LLM-orchestrated runner, and why instrumenting it is cheap.

---

## Method findings (moved here from the ADR — process record, not decisions)

The ADR carries the decisions; this section carries what the debate learned about *how it reached them*. Nothing here is normative.

### The recurring failure class: a faithful test of the wrong consumer

The same defect appeared **ten times, from six people, in one debate** — including inside the fix written to prevent it. Each instance had the identical shape: **the reading was real, and what it established was narrower than what it was taken to establish.**

| # | Instance | What it established | What it was taken to establish |
|---|---|---|---|
| 1 | `SIGNATURE=` in the Gate-2 verdict | which tree the verdict describes | that the pipeline ran |
| 2 | `gate2_signature` post-commit | SHA-256 of the empty string, constant for any clean tree | a binding to this task's changeset |
| 3 | The verdict's `LAYER n` lines | a record's *shape* | a record with provenance |
| 4 | The substrate-probe marker | a CLI version matched | "this substrate is validated" (version + settings + agent registration) |
| 5 | `cost-ledger.jsonl` | a filename promising JSONL | a file whose lines parse (99 lines, 11 objects, 0 parseable) |
| 6 | Sourced replacement cost figures | a citation existed | a correct number (retry rows are cumulative supersets; double-counted) |
| 7 | A bash `[ -e "$p" ]` path check | paths exist *under brace expansion* | paths the guard (which does not expand) can resolve |
| 8 | Two hand-rolled filters skipping `{`/`*` | "0 unresolved" among tokens checked | 0 unresolved |
| 9 | The reflection fix's own `grep` | no per-file lines on a clean tree → exit 1 | a failure |
| 10 | A `git diff \| grep -E '^[+-][^+-]'` review sweep | no non-bullet lines changed | no changes to review |

Plus: a line count sampled mid-write and reported as settled; and a `service/deployment.yaml` regex sweep that matched a *legitimate* GC pointer and inflated a count from three to four.

**The practical tell** (better than the principle, because it says when to get suspicious): the failure is rarely that a check returns something *wrong* — it is that a check returns **nothing** where something was expected. **Empty output is this failure mode wearing the costume of a pass.** When a check comes back clean, confirm it *can* come back dirty.

**Why it is structural rather than carelessness**: it recurred through the draft, the accepted rewrite, three specialists' independent verifications, and the remediation itself, with six people actively hunting for it. It is what happens by default whenever a check is written *adjacent to* rather than *through* the thing it checks.

### A second pattern: the mechanism is correct, the process walks around it

Four instances: `layer-all.test.sh` pinned the operator/implementer exit-code taxonomy and the runner rebuilt the same collapse one level up; the Gate-2 producer emits per-layer lines nothing reads; the pre-commit hook validates a verdict no post-commit path re-checks; and the reflection step turned a green guard red because nothing ran it between authoring and commit.

### Corrections to the draft ADR

Six factual claims in the draft were wrong, plus one introduced during the accepted rewrite and caught in review. All were found by **inspecting the implementation or its artifacts**, never by reading the prose:

- **line 34** — "classified by field access": both real canary captures carry `api_error_status: null` and classify via the prose grep; the `429` field check has never fired in production.
- **line 44** — "the real long-term hedge is B": panel size does not move SDK migration cost; peer-messaging dependency does (47% of 1,184 calls).
- **line 45/80** — the credential-lifetime and #63 escapes framed as panel misses: both were diff-locality failures, and #63's was a story-bookkeeping escape the panel correctly scoped out.
- **line 46** — "Verified 2026-08-07: whole-pipeline, not per-layer": the verdict already emits per-layer `LAYER n STATUS DURATION`.
- **line 49** — "journal shows one-iteration-clean classes": that evidence does not exist.
- **line 101** — "timestamped logging": undersells an existing structured per-task ledger.
- **introduced during the rewrite** — a retracted, sleep-contaminated timing median restated as current, plus a cost total double-counting cumulative retry rows.

### Options considered and rejected within C

- **C(iv) — trust the verdict when it reports FAIL.** Raised, withdrawn, revived once a freshness discriminator was found, dropped by its author. Decisive reason: its trigger is anomalous by construction — the runner reaches the gate only after a commit, and a commit staging `Phase=complete` already required a hook-validated PASS. So "verdict says FAIL *and* a commit exists" means the hook was bypassed or the devloop committed without marking complete — the state where the artifact is *least* trustworthy and independent evidence is worth *most*.
- **`HEAD=` as a general re-binding.** Considered and insufficient: it discriminates cross-task staleness only. It does not cover intra-task drift (a devloop can run the pipeline early, keep writing, then commit, with HEAD never moving), does not separate attempts within a task, and is unsigned free text.

### Position changes against self-interest

Every participant changed a position on evidence: code-reviewer reversed its oversized-panel thesis after measuring (its own seat proved highest-yield), then withdrew the mechanization ranking it had built, then disclosed a stale pointer it had introduced into the region it had just measured; infrastructure withdrew "(iii) is cheap" and "layer 7 dominates the tail"; observability was corrected four times, twice by itself; operations corrected its own gate figure twice, withdrew retry-once-on-red, its baked-marker sentinel, its critical-path metric and its light-set composition; security withdrew C(iv) and corrected its own "contains live tokens" framing after scanning and finding zero credential material; test recorded that its own threshold for supporting (iii) was tested and not met.

**The clearest statement of panel value produced by the debate came from this, not from the yield table**: a specialist certified its own file clean twice, with the measurements in hand, having explicitly named the blind spot the defect sat in — and still missed it. Three other reviewers, including the Lead, also passed that changeset. One person, reading a TODO entry written by someone else, caught it. That value is invisible to every finding count.
