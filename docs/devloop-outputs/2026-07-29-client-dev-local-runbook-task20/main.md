# Devloop Output: client-dev-local.md Runbook (R-49, story task #20)

**Date**: 2026-07-29
**Task**: Author `docs/runbooks/client-dev-local.md` — the prose/diagnosis companion to `scripts/dev-web.sh`, covering prerequisites, local-stack bring-up, first-run failure-mode scenarios, `/etc/hosts` fallback, teardown, and an explicit production non-goal banner.
**Specialist**: operations
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-20`
**Duration**: ~2h active (2026-07-29 19:30 → 2026-07-30 15:05, incl. an implementer outage)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `dc8c7ebc97f9cafafa968ac807bbfa6e8473287c` |
| Branch | `feature/browser-client-join-task-20` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Gate 2 | Layers 1,2,3,4,5,7 **OK**; Layer 6 **FAIL** (`pnpm-audit-failed`) — pre-existing, out-of-domain, spun out by user decision (see §Gate 2 below) |
| Implementer | `implementer` (operations) |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | (implementer — reviewed by the rest of the panel) |
| Semantic Guard | `semantic-guard` |
| Client (conditional domain reviewer) | `client` |

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (conditional — 3 content constraints must land in the text; verified at review) |
| Test | confirmed; hunk-ACK granted for `SKILL.md`. **NB**: @test's re-ruling proposed re-pointing row 7 to "Layer 8" — that edit was **withdrawn** by @team-lead, since ADR-0033 renumbered env-tests Layer 8 → Layer 7 and no Layer 8 exists (`adr-0033:349`, `:152`, `:384`; no `layer8.sh` on disk). Implemented instead: clause kept, marked pending #18/#19, no "Layer 8" introduced. |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed (ADR-0019 extraction opportunity filed at verdict time) |
| Semantic Guard | confirmed (native: SAFE at plan level) |
| Client | confirmed (hunk-ACK granted for `packages/web-app/README.md`) |

---

## Task Overview

### Objective

Story task #20 / requirement R-49. Produce `docs/runbooks/client-dev-local.md`: the human-readable
bring-up + diagnosis guide for running the browser client against the local Kind stack. It is the
prose companion to `scripts/dev-web.sh` (landed 2026-07-16, commit `7b69288`; extended by `015403e`),
whose header already declares the split: *"the runbook stays the source of prose/diagnosis; this
script is the fast path."*

### Scope
- **Service(s)**: none (documentation only); references AC/GC/MC/MH local topology
- **Schema**: No
- **Cross-cutting**: Yes — documents client + infra + cluster-helper surfaces, but edits only `docs/`

### Debate Decision
NOT NEEDED — documentation of already-landed behavior; no new design decision.

---

## Cross-Boundary Classification

**Revision 3** — expanded after Gate 1 input from all seven reviewers and @team-lead rulings. **Eight files.**

<!-- FORMAT CONTRACT — two traps, both hit on 2026-07-29. Do not delete.
     This table is machine-parsed by
     scripts/guards/common.sh::parse_cross_boundary_table.

     TRAP 1 — column count. The parser requires EXACTLY three data cells per row:
     | Path | Classification | Owner |. Add a column (a "#" index, a "Change"
     description) and it reads the wrong cell as Path: Layer A then reports every
     REAL path as scope_drift_inbound and every index value as
     scope_drift_planned_untouched. Per-file change descriptions belong in
     §Per-file scope below, NEVER in this table.

     TRAP 2 — section scope. The parser consumes ANY pipe table between this
     "## Cross-Boundary Classification" heading and the next "## " heading. An
     unrelated table in this section (we had one for the artifact-kind rule) gets
     parsed as classification rows and its cells reported as bogus paths. Use
     bullet lists for other tabular content inside this section.

     WHY THIS COMMENT EXISTS: the guard fires loudly but names the wrong thing.
     The failure reads as "my scope is wrong" when the fault is "my table shape
     is wrong", so the error message leads you away from the fix. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `docs/runbooks/client-dev-local.md` | Mine | — (operations) |
| `packages/web-app/README.md` | **Minor-judgment** | `client` — `Approved-Cross-Boundary: client` hunk-ACK |
| `scripts/dev-web.sh` | Mine | — (operations) |
| `scripts/generate-dev-certs.sh` | Mine | — (operations) |
| `docs/LOCAL_DEVELOPMENT.md` | Mine | — (operations) |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine | — (operations) |
| `.claude/skills/devloop/SKILL.md` | **Minor-judgment** | `test` — `Approved-Cross-Boundary: test` hunk-ACK |
| `docs/TODO.md` | Mine | — (operations) |
| `docs/decisions/adr-0013-local-development-environment.md` | **Minor-judgment** | `infrastructure` — **NOT on this panel; @team-lead to secure the trailer or convert to spin-out** |

### Per-file scope

1. **`docs/runbooks/client-dev-local.md`** (new) — author the runbook.
2. **`packages/web-app/README.md`** — *scope expanded (@dry-reviewer F2, @client, @team-lead).* Delete the 6 numbered bring-up steps and the 3-item "Things that go wrong" in favour of a pointer (the README already declares the runbook their owner); fix the bring-up command; name the machine on the `/etc/hosts` advice; de-stale the "lands as task #20 / not yet on this branch" forward refs. **Keeps** the `dev-web.sh` fast path, app URL, "Transport & scheme", the `VITE_*` table, the E2E contract, and Scripts — client-domain content the runbook must not absorb.
3. **`scripts/dev-web.sh`** — four hunks. (a) the hard-fail port hint pointed at `infra/devloop/dev-cluster setup`, the wrong topology → `./infra/kind/scripts/setup.sh`. (b) the `demo.localhost` WARN said *"the browser can't reach the app"* then printed a WSL2-side fix — wrong machine → reframed to name WSL2-side tooling; the `Fix:` line was already correct and stays. (c) @dry-reviewer F7: shortened the MC/MH reachability block comment to its operational core plus a pointer to runbook §5 F1, so the script stops being a second canonical copy of the mirrored-mode explanation. (d) **disclosed scope addition** — `--help` used `sed -n '2,33p'`, four lines past the end of the header, so it printed `set -euo pipefail` and the `REPO_ROOT=` line as if they were usage text. Pre-existing (verified byte-identical at HEAD; my earliest hunk is far below it), one character, and **the runbook I just wrote tells readers to run `--help` for the current check list** — so it is load-bearing for this deliverable rather than drive-by polish. Fixed to `2,29p`.
4. **`scripts/generate-dev-certs.sh`** — two comment-honesty edits, neither a deletion. (a) Three sites asserted a present-tense Playwright consumer — a **forward reference**, not a false claim (R-36 specifies it; task #18 builds it). Marked pending-with-task-number; the real Vite consumer named concretely. (b) @dry-reviewer F8: the expiry hint printed *"(R-49 will automate this)"* — R-49 **is** this runbook and it automates nothing. Clause replaced with pointers to the runbook and the TODO entry. The printed recovery sequence **stays**; it fires exactly when someone is stuck.
5. **`docs/LOCAL_DEVELOPMENT.md`** — "The AC service is NOT deployed by setup.sh" is false (`main()` deploys AC/GC/MC/MH). Corrected, along with the "infrastructure only" framing above it. The downstream "Run AC Service" section stays — Telepresence/cargo iteration is still real, just an alternative to the in-cluster pod rather than a required step.
6. **`docs/user-stories/2026-05-02-browser-client-join.md`** — four categories, all landed:
   (i) the bring-up-command and scheme defects in the R-49 requirement prose, plus the task-#20 description row;
   (ii) **Layer-8 category A — live use → corrected to Layer 7**: **8 occurrences across 7 lines** (`:177`, `:351`, `:357`, `:430`, `:470`, `:510`, `:513`). Note `:510` carries **two** occurrences — which is why the site count is 7 lines but the occurrence count is 8. Both numbers are correct; they count different things.
   (iii) **Layer-8 category B — literal `/devloop "…"` invocation strings → verbatim + bracketed pointer appended *outside* the quoted command**: `:723` (task #19), `:726` (task #22). Plus the two earlier invocation-pointer sites from the bring-up defect, `:511` and `:724`.
   (iv) **Layer-8 category C — mention, not use → NEVER TOUCHED**: `:529`, `:682`, `:695`, `:742`, `:764`, `:827`, `:832`.
   **Post-sweep verification**: `grep -c "Layer 8"` returns **9** = 2 (category B) + 7 (category C), and `grep "Layer 7→7"` returns nothing, confirming category C is intact and there was no over-sweep. See §Row 6 below for the authoritative classification and the use-vs-mention rationale.
7. **`.claude/skills/devloop/SKILL.md`** — same case as row 4: a valid forward reference, marked pending, not deleted and not re-pointed. ADR-0033 puts Playwright *inside* Layer 7 in the target design, and this row is a near-verbatim mirror of the ADR's table, so editing one side alone would leave a silently-diverged mirror. ADR untouched.
9. **`docs/decisions/adr-0013-local-development-environment.md`** — @code-reviewer Gate 2 Finding 1. The ADR's §Setup Script carries the *same two false claims* row 5 just corrected in `docs/LOCAL_DEVELOPMENT.md` ("deploys **infrastructure only**"; "The AC service is NOT deployed by setup.sh"). Before this diff both files agreed and both were wrong; after row 5, `LOCAL_DEVELOPMENT.md` is right and the ADR — the more authoritative-looking of the two — is wrong. That is my own row-7 framing-lock argument applied to my own diff. **Treatment: dated erratum, original text preserved verbatim.** An ADR is a decision record; the decision ("single-tier kind + podman") is unaffected, and only implementation prose drifted underneath it — so this gets the same record-vs-prescriptive-prose treatment @team-lead ruled for the story doc's invocation strings, not a silent in-place rewrite. **Gating**: `infrastructure` leads ADR-0013's deciders and is not on this panel, so there is no available signer. @team-lead must either secure the trailer or tell me to revert this hunk and convert it to a spin-out with a `docs/TODO.md` entry — @code-reviewer explicitly offered both routes.
8. **`docs/TODO.md`** — @dry-reviewer F8: the cert-rotation entry was misassigned to task #20, but its substance is a `setup.sh` **automation** a prose runbook cannot deliver. Retitled to the remaining automation, re-owned to operations/infrastructure, manual sequence replaced by a pointer. **Not ticked.**

### Classification rationale

- **Row 3 scope has a stopping rule** (@team-lead, binding). The sweep covers *diagnostic clauses that name a consequence on the wrong machine, or point at the wrong topology* — and nothing else. By @client's full read of the script that is exactly `:142` and `:243`: the fingerprints WARN correctly scopes itself to the join step, and the MC/MH reachability warnings correctly name the media step. A third hit meeting the rule would be in scope; rewording WARNs that are merely terse is polish and is out.
- **Row 3 (`dev-web.sh`) is `Mine`, not Mechanical.** I accept @code-reviewer's argument and @team-lead's ruling. The sed-test fails on both prongs: `infra/devloop/dev-cluster setup` → `./infra/kind/scripts/setup.sh` is a *concept substitution* between two bring-up paths with different port regimes (the worked example draws the line at "key-rename, NOT concept substitution"), and no guard covers hint strings, so a partial state is invisible to the pipeline. But the Mechanical question is moot: `scripts/dev-web.sh` is not in `cross-boundary-ownership.yaml` (GSA paths only), so there is no manifest owner and ownership falls to domain judgment — operations owns dev bring-up tooling. No trailer needed.
- **Row 2 stays Minor-judgment.** Confirmed unchallenged by @security, @test, and @code-reviewer. The sed-test genuinely fails (it changes which command executes, on an operations-domain fact); no GSA path; so owner-trailer rather than owner-implements is right per §6.3/§6.4. The expanded deletion scope does not change the tier — it is the same file, same owner, same reason clause. @test's note is honoured: the §6.5 env-tests cross-link into that README lands **inside the same hunk-ACK**, not as an untabled second edit.
- **Row 7 is the one I am least sure of** and I am deliberately not self-assigning it. It is a one-clause deletion, but `.claude/skills/devloop/SKILL.md` is the devloop workflow definition, not a bring-up doc. I am taking it rather than deferring because it is the third of three files asserting a runner that does not exist (rows 4 and 7 plus the runbook itself) — fixing two of three and calling the third "out of scope" is exactly the framing-lock bug the review protocol names. @test: classify it and I will comply.
- **No GSA paths in the diff** (confirmed independently by @security). No intersection rule, no `security` co-sign.
- `Approved-Cross-Boundary:` trailers will carry a reason clause ≥10 chars naming the authority, per §6.7.

### Row 6 — authoritative "Layer 8" site classification (16 hits, NOT 9)

<!-- Verified by @dry-reviewer against static repo state; ruled by @team-lead.
     Recorded here rather than in the message log so it survives an implementer
     restart. A fresh implementer given only "correct the stale Layer 8 refs"
     will reach for `grep -n 'Layer 8'` and over-sweep. DO NOT DO THAT. -->

`docs/user-stories/2026-05-02-browser-client-join.md` contains **16** "Layer 8" hits in three categories.

**A. Correct to Layer 7 — 8 sites**: `:177`, `:351`, `:357`, `:430`, `:470`, `:510`, `:513`
(plus `:385`'s separate bring-up-command and scheme defects). Live references to where browser E2E runs.

**B. Keep verbatim + bracketed pointer outside the quoted command — 2 sites**: `:723` (task #19), `:726` (task #22).
Both verified as *inside* the literal `/devloop "…"` string, not adjacent prose.

**C. DO NOT TOUCH — 7 sites**: `:529`, `:682`, `:695`, `:742`, `:764`, `:827`, `:832`.
Each **mentions** "Layer 8" as the old number being renamed (`Layer 8→7 renumber`) or records history.
Sweeping these yields `"Layer 7→7 renumber"` — nonsense — and erases the record of how the renumber
happened, which is the record that made this whole class of drift diagnosable.

**Discriminator is use-vs-mention, not audience.** This is a *different* test from the
`dev-cluster setup` case (where the discriminator was audience: WSL2 host vs devloop container).
Applying that rule here by analogy gets category C wrong.

**Refined artifact-kind rule** (supersedes the earlier two-category form). Deliberately a list, not
a pipe table: any markdown table inside `## Cross-Boundary Classification` is parsed as the
classification table by `scripts/guards/common.sh::parse_cross_boundary_table`.

- **Live use** — prose or a decision-log entry referring to a *current* layer → **correct in place**.
- **Mention** — the name being renamed, or historical narrative → **never touch**.
- **Literal `/devloop "…"` invocation string** → **verbatim**, with a bracketed pointer appended
  *outside* the quoted command.

`:470` ruling (@team-lead): it is a decision-log row, so neither prescriptive prose nor an invocation —
outside the rule as originally stated. **Correct it.** The decision recorded (local/devloop only, in the
validation suite) is unchanged; only the layer number beneath it moved. Leaving it would make a live
policy statement point at a layer that does not exist. A Q&A log records *what was decided*, not *how it
was then spelled*.

**Files explicitly NOT touched** (superseding the Revision-1 list — rows 3, 5 and 6 above moved *into* the diff on reviewer findings and @team-lead rulings):
- `infra/kind/kind-config.yaml` and `crates/devloop-helper/src/ports.rs` — cited as the port SSoT, never copied (@dry-reviewer F1).
- `packages/sdk-core/**` — §4.2 documents `MediaTransport`'s ≥1-of-N semantics as deliberate R-21 design, not a defect. No code finding.
- `crates/gc-service/**` and the Kind CORS overlay — F10 documents the origin mismatch as conditional-on-configuration; no config change is warranted for the default proxy path (see Revision 2 §A1).
- `docs/DEVELOPMENT.md`, `docs/BUILD_REQUIREMENTS.md` — cross-linked, not duplicated (SSoT).

**Revision-1 claims now withdrawn**: "no bug in `scripts/dev-web.sh`" is narrowed to "no bug in its *checks*" — line 142's hint is a genuine diagnosis bug (row 3), and my original claim was inconsistent with my own §1 analysis. "`docs/LOCAL_DEVELOPMENT.md` — cross-linked, not duplicated" still holds for its prerequisites content, but the file also carries a false claim that this diff corrects (row 5).

---

## Planning

### Mechanism restatement (framing check, per the pre-share instruction)

**Instance-language** (how the task names it): "document seven first-run traps hit during the 2026-07 manual bring-up."

**Mechanism-language**: the local browser-demo topology spans **two machines and four network/namespace boundaries** — Windows Chrome → WSL2 loopback → podman rootlessport → Kind node NodePort → pod. Every one of the seven traps is an instance of the *same* mechanism: **a value that is correct inside one boundary is wrong on the other side of it, and the failing layer has no error channel back to the operator**, so the symptom surfaces somewhere far from the cause.

- `localhost` correct in WSL2, resolves to `::1` from Chrome (F1)
- `nvm use` correct in this shell, absent in the next (F2, F3)
- corepack's signing keys correct for the Node that bundled them, stale for the pinned pnpm (F4)
- `pnpm --filter` correct as a package selector, wrong as a task-graph entry point (F5)
- `/etc/hosts` correct until WSL regenerates it (F6)
- `fingerprints.json` correct on disk, stale inside the already-started Vite process (F7)

**Wider class the task does not name** — same owner (operations), same mechanism, and I am surfacing them rather than silently scoping them out:

1. **There are TWO Kind topologies in this repo, and the task's stated bring-up command selects the wrong one.** `infra/devloop/dev-cluster setup` (ADR-0030 helper) generates its config from `infra/kind/kind-config.yaml.tmpl` with ports allocated in **20000–29999** (`crates/devloop-helper/src/ports.rs:20` `PORT_RANGE_START`) bound to the podman host-gateway IP, and patches MC/MH `*_WEBTRANSPORT_ADVERTISE_ADDRESS` to `https://<gateway-ip>:<dynamic-port>` (`infra/kind/scripts/setup.sh:694-698`). The browser demo needs the **static** topology: `infra/kind/kind-config.yaml`, host ports AC 8443 / GC 8444 / MC 4433,4435 / MH 4434,4436, all `listenAddress: 127.0.0.1`, brought up by `./infra/kind/scripts/setup.sh`. Those are exactly `dev-web.sh`'s `AC_PORT`/`GC_PORT` defaults and exactly the on-disk configmap values `dev-web.sh` greps. **This is the same "correct in one boundary, wrong in another" mechanism applied to the cluster itself**, so it earns a first-class section in the runbook (§1 "Which cluster am I running?"), not a footnote.
2. **`dev-web.sh`'s MC/MH check reads the configmaps from disk, not from the live cluster.** Under a devloop-helper cluster the live ConfigMap is patched and the on-disk file is stale, so the check would validate the wrong address. This is a *documented limitation of the script's stated target* (its comment says the default "serves the host-browser/host-env-test path"), not a bug — but the runbook must say it out loud so a reader on the wrong topology isn't misled by a green preflight.
3. **`crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`** is the third consumer of the same port-map ambiguity. Out of scope to change; named in the runbook's "Related" section so a reader connects the three.

### Findings against the task brief (verified against the repo — must be settled before I write)

Each of these contradicts something in my task brief or an adjacent doc. Flagging rather than quietly writing around them.

1. **Bring-up step 1 is `./infra/kind/scripts/setup.sh`, not `infra/devloop/dev-cluster setup`.** Evidence above. Additionally `dev-cluster` requires the helper socket at `/tmp/devloop/helper.sock` (`infra/devloop/dev-cluster:24,109`) which only exists while `devloop.sh` is running on the host — it is not a command a plain WSL2 shell can run. I will document the static path as *the* demo bring-up, with an explicit note on the devloop-container path and why it is different.
2. **"demo-org seed" is not a separate bring-up step.** `setup.sh`'s `main()` calls `seed_demo_org` (`infra/kind/scripts/setup.sh:996`); it is idempotent via `ON CONFLICT DO NOTHING` (`:576-595`). Writing it as a step the operator performs would be false. It becomes a "what setup.sh did for you (and how to verify it)" bullet.
3. **Dev certs are likewise generated by `setup.sh`**, which invokes `scripts/generate-dev-certs.sh` from `create_mc_tls_secret` and `create_mh_tls_secret` (`:663`, `:728`). The manual `scripts/generate-dev-certs.sh` invocation is a **re-generation / rotation** step (14-day MC/MH leaf window, `DAYS_WT_CERT=14`, `scripts/generate-dev-certs.sh:50`), not a first-run step. The runbook will frame it that way.
4. **`docs/user-stories/2026-05-02-browser-client-join.md:511` says "opening Chrome at `http://demo.localhost:8443`".** The real URL is `http://demo.localhost:5173` (the Vite dev server; 8443 is the AC NodePort). I propose **not** editing it: that line is the historical task-decomposition record of what was planned, and rewriting planning history to match implementation reality is worse than leaving it. The authoritative URL will be in the runbook, which the story row points to. **Reviewer input wanted** — if the panel reads the story as a live tracking doc rather than a record, I will fix it.
5. **No bug found in `scripts/dev-web.sh`.** Read in full. Its SSoT discipline is sound (versions from `.nvmrc` / `packageManager`; advertise addresses from the configmaps), its WARN/HARD-FAIL split is documented inline, and the on-disk-configmap read is correct for its stated target topology. No change proposed.
6. **There is no Playwright E2E suite on this branch.** `packages/web-app/package.json` has Vitest 4 browser-mode component tests (`test:component`) and a Node-tier prod-bundle test (`test:unit`); no `playwright.config.*` exists anywhere outside `node_modules`. The story's O-1 line asks for a "running E2E suite from devloop or host" section — I will write a short, honest **"not on this branch yet (tasks #18/#19)"** subsection naming what *does* exist, rather than documenting a command that would fail.

### Divergence from `docs/runbooks/TEMPLATE.md` (deliberate, per brief item 6)

`TEMPLATE.md` is alert-shaped: it assumes a Prometheus rule fired, an SLO is burning, an oncall is paged, and a retrospective follows. A dev-environment bring-up guide has none of those referents. Force-fitting it would produce empty ceremony (`**Severity**: N/A`, `**Blast Radius**: one laptop`, `**Escalate to**: yourself`), which is exactly the "runbook that lies" failure mode.

| Template element | Decision | Rationale |
|---|---|---|
| `**Alert**` / `**Severity**` header fields | **Drop** | No alert rule exists or could exist; nothing fires on a dev laptop. |
| `**Service**` / `**Owner**` / `**Last Updated**` | **Keep** | Real metadata; keeps the runbook index scannable. |
| `## Symptom` → `## Diagnosis` → `## Mitigation` per-scenario shape | **Keep — this is the transferable core** | Applied *per failure mode* in §5 as **Symptom → Cause → Diagnosis (with the exact command) → Fix → Why it recurs**. The "Why it recurs" field is an addition: five of the seven traps are recurring-by-construction (session-scoped shell state, 14-day certs, WSL-regenerated `/etc/hosts`), which an incident template has no slot for. |
| `## Impact` / `**Blast Radius**` | **Drop** | User/business impact and blast radius are undefined for a single-developer local stack. |
| `## Escalation` (PagerDuty / oncall / #incidents) | **Replace** with "If you are still stuck" — pointing at the tcpdump ground-truth technique, `dev-cluster`/`setup.sh` logs, and the `@operations` / `@client` specialists. |
| `## Post-Incident` / retrospective / `docs/incidents/` | **Drop** | No incident occurred. |
| `## Example Queries` (PromQL / kubectl / SQL) | **Adapt** | Replaced by "Ground-truth commands" (§4): `kubectl logs` greps, `ss -uln`, `tcpdump`, `getent ahosts`. Local Prometheus exists but is not the diagnostic surface for these traps. |
| `## Related Runbooks` | **Keep** | Cross-links to `gc-deployment.md` §Smoke Tests, `devloop-validation.md`, `LOCAL_DEVELOPMENT.md`. |
| `## Changelog` | **Keep** | Verbatim table shape. |

The runbook will carry a one-line note stating it deliberately diverges from `TEMPLATE.md` and why, so the next author does not "correct" it back.

### Document outline

```
# Runbook: Local Browser-Client Dev Environment
   metadata block (Service / Owner / Last Updated / companion script)
   >>> NON-GOAL BANNER — production <<<          (blockquote, above the fold)
§0  Which machine am I on?                       two-box topology + ASCII diagram + command-prefix convention
§1  Which cluster am I running?                  static (demo) vs devloop-helper (agents) — port tables, how to tell
§2  Prerequisites
    2.1 Windows side (Chrome; .wslconfig networkingMode=mirrored)   [marked UNVERIFIABLE-FROM-WSL2]
    2.2 WSL2 side (kind, kubectl, podman; → LOCAL_DEVELOPMENT.md for inotify/keyring limits)
    2.3 WSL2 persistence (/etc/wsl.conf generateHosts=false)
    2.4 Node / pnpm — pins live in .nvmrc + package.json packageManager (pointer, no restated numbers)
§3  Bring-up
    Step 0  scripts/dev-web.sh --check           the fast path; WARN vs HARD-FAIL semantics
    Step 1  ./infra/kind/scripts/setup.sh        + what it does for you (demo org, certs, migrations)
    Step 2  /etc/hosts: 127.0.0.1 demo.localhost
    Step 3  scripts/dev-web.sh                   (install + Nx codegen + vite)
    Step 4  Chrome → http://demo.localhost:5173  sign-up → create → copy code → join
    Manual fallback for each step (what dev-web.sh automates, done by hand)
§4  Verifying the join is REAL (not optimistic)
    4.1 MC server-side ground truth: "Join succeeded"
    4.2 Why the client state is optimistic (connectAll resolves on ≥1-of-N MH; connect-envelope only)
    4.3 tcpdump ground truth for the UDP path
    4.4 Second-tab roster check
§5  Failure modes  (F1–F7, Symptom → Cause → Diagnosis → Fix → Why it recurs)
§6  Teardown
§7  Out of scope on this branch (E2E suite #18/#19; devloop-container cluster; production)
    Related runbooks & docs
    Changelog + "this deliberately diverges from TEMPLATE.md" note
```

### Verification log — every claim traced to a repo artifact

| Claim to be written | Verified against |
|---|---|
| Static host ports AC 8443 / GC 8444 / MC 4433,4435 / MH 4434,4436, all `127.0.0.1` | `infra/kind/kind-config.yaml` `extraPortMappings` (R-37 loopback comment) |
| Devloop topology uses 20000–29999 + gateway IP | `crates/devloop-helper/src/ports.rs:16-20,25-47`; `infra/kind/kind-config.yaml.tmpl`; `crates/devloop-helper/src/commands.rs:630,639,703-711` |
| MC/MH advertise the IPv4 literal; the WSL2 `::1` rationale | `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-configmap.yaml` (inline comment) |
| `dev-web.sh` reads versions from `.nvmrc` + `packageManager`; WARN/FAIL split; ports env-overridable | `scripts/dev-web.sh:10-30,60-64,66-70,134-147` |
| Node pin `22.11.0`, pnpm pin `10.33.2` — **cited by file, not restated** | `.nvmrc`; `package.json:6` |
| `setup.sh` seeds the demo org idempotently | `infra/kind/scripts/setup.sh:576-595,996` |
| `setup.sh` generates dev certs | `infra/kind/scripts/setup.sh:663,728` |
| MC/MH leaves are 14-day (Chrome `serverCertificateHashes` cap); fingerprints at `infra/docker/certs/fingerprints.json` | `scripts/generate-dev-certs.sh:7-27,46-50` |
| Fingerprints read at Vite **config** time → restart required | `packages/web-app/vite.config.ts:24-27,38` |
| `dev` target `dependsOn` `proto-gen:codegen`; `pnpm --filter` bypasses it | `packages/web-app/project.json` (`dev` target); `packages/proto-gen/project.json` (`codegen` outputs `sdk-core/src/proto/**/*_pb.ts`) |
| Vite proxy: `/api/v1/auth` → AC with `changeOrigin:false` (Host preserved, ADR-0020 org extraction) | `packages/web-app/vite.config.ts:51-61` |
| App URL is `http://demo.localhost:5173` | `packages/web-app/vite.config.ts` (default vite port) + `scripts/dev-web.sh:265` |
| MC ground-truth log `"Join succeeded"`, target `mc.webtransport.connection` | `crates/mc-service/src/webtransport/connection.rs:400-407` |
| Client join states `idle→fetching-token→connecting-mc→joining→joined` | `packages/sdk-core/src/session/events.ts:22-29`; `MeetingSession.ts:6,196-256` |
| `"Signaling transport closed"` is the `SignalingErrorCode.Transport` text | `packages/sdk-core/src/signaling/errorCodeMap.ts:44` |
| **Client state is optimistic**: `connectAll` resolves on ≥1-of-N MH and only ever performs the connect envelope | `packages/sdk-core/src/media/MediaTransport.ts:4-14,106-123` |
| Teardown is `./infra/kind/scripts/teardown.sh` (deletes cluster, kills port-forwards) | `infra/kind/scripts/teardown.sh:7-8,40-60` |
| `dev-cluster` needs the helper socket | `infra/devloop/dev-cluster:24,109-115` |
| No Playwright E2E suite on this branch | `packages/web-app/package.json`; `find packages -name 'playwright*' -not -path '*/node_modules/*'` → empty |
| GC smoke tests anchor | `docs/runbooks/gc-deployment.md:269` (§6) and `:754` (`## Smoke Tests`) |
| inotify / keyring limits are a real WSL2 prerequisite | `docs/LOCAL_DEVELOPMENT.md:98` "System Limits (Linux/WSL2)" |

### What I explicitly cannot verify from WSL2 (will be marked as such in the text, per "fail loudly")

- The Windows-side `%UserProfile%\.wslconfig` `[wsl2] networkingMode=mirrored` setting, and `wsl --shutdown` to apply it. I can observe the *consequence* inside WSL2 (`getent ahosts localhost` ordering, which is what `dev-web.sh:181` keys off) but not read or set the Windows file. The runbook will say "this step is on the Windows box and was not executed or verified from this environment" rather than asserting it works.
- Chrome's actual `serverCertificateHashes` acceptance behaviour and its 14-day cap. Sourced from `scripts/generate-dev-certs.sh`'s inline rationale, attributed as such.
- The claim that WSL2 NAT mode forwards TCP only (so mirrored mode is *required* for the QUIC path). No repo artifact supports this; it comes from the 2026-07 bring-up session. It will be written as an observation from that session with that attribution, not as a documented platform guarantee.

### Non-goal banner (brief item 7) — placement and wording

A `> [!WARNING]`-style blockquote immediately under the metadata block, above the first section, listing the specific dev-only patterns by name so it cannot be read as generic boilerplate: self-signed CA + `serverCertificateHashes` pinning, `demo.localhost` + `/etc/hosts`, host-published loopback NodePorts, single-node Kind with `disableDefaultCNI`, a seeded `demo` org, and pre-seeded OAuth client secrets printed by `setup.sh`. Plus the story's required sentence: production deployment, CSP/COEP/COOP headers, CDN serving, and synthetic probes are tracked separately under **ADR-0028 §10** (verified: `docs/decisions/adr-0028-client-architecture.md:342` is "### 10. Deployment Architecture").

### Risks

- **Drift with `scripts/dev-web.sh`.** Mitigated by never restating the script's logic or version numbers: the runbook cites `.nvmrc` / `package.json` / the configmaps as the SSoT and describes *why* each check exists, which is the half the script cannot carry.
- **Length.** Seven scenarios plus two topology sections is a long document. Mitigated with a table of contents and a one-screen "fast path" at the top so the common case is not buried.

---

# Plan Revision 2 (post-Gate-1 input)

All seven reviewers responded before Gate 1 closed. This revision absorbs their input, records two places where I checked a reviewer's claim and it did not hold, and restates the outline.

## A. Two reviewer claims I verified and am correcting

I am not taking these at face value, and I am not quietly writing around them either.

### A1. @security — the GC CORS 403 does **not** happen on the default demo path

@security wrote: *"that overlay allows `http://localhost:5173`, but `dev-web.sh:265` tells the user to open `http://demo.localhost:5173` — different origin, so GC preflight will 403 on the demo host."*

The overlay fact is exactly right — `infra/kubernetes/overlays/kind/services/gc-service/configmap-cors-patch.yaml` sets `CORS_ALLOWED_ORIGINS: "http://localhost:5173"`, and `demo.localhost:5173` is indeed a different origin. But the conclusion doesn't follow, because **the browser never makes a cross-origin request in the default configuration**:

- `packages/web-app/src/lib/config.ts::loadConfig` defaults `gcBaseUrl` to `''` (same-origin) and `acOriginTemplate` to `http://{subdomain}.localhost:5173` — which for the `demo` org resolves to the page's *own* origin.
- All API calls therefore go to relative paths on `http://demo.localhost:5173` and are proxied server-side by Vite (`packages/web-app/vite.config.ts`, `server.proxy`). Same-origin fetches trigger no preflight.
- `cors_preflight_observer`'s own gate requires `OPTIONS` **+** `Origin` **+** `Access-Control-Request-Method`, and its module doc is explicit that a non-preflight request "passes through completely untouched — never rewritten, never recorded."

So on the documented path the CORS allowlist is **inert** and its `localhost` vs `demo.localhost` mismatch is invisible. Writing "GC will 403" as a bring-up expectation would send readers chasing a failure that cannot occur, which is the failure mode this runbook exists to prevent.

**What I am taking instead**: the mismatch becomes a real failure the moment someone sets `VITE_GC_BASE_URL` to a direct GC origin (e.g. `http://127.0.0.1:8444`) to bypass the proxy — a plausible debugging move. That gets **F10**, framed as conditional-on-configuration, with @security's prescribed remediation verbatim: the Kind overlay patch is the single edit point, prod base stays `""`, add the demo origin, never wildcard. @security — flag if you disagree with my reading.

### A2. @code-reviewer — the ConfigMap resource name is `mc-0-config`, not `mc-service-0-config`

You flagged this yourself and asked me not to inherit it — good call, it was wrong. Verified `metadata.name` in `infra/services/mc-service/mc-0-configmap.yaml` is **`mc-0-config`** (and `mh-0-config`, `mc-1-config`, `mh-1-config`), namespace `dark-tower`. `setup.sh`'s own patch commands confirm it: `kubectl patch configmap mc-0-config -n dark-tower`. The F8 diagnosis command will use the verified name. Your `setup.sh` patch-time log-line pointer is solid and is being used as the second confirming signal.

## B. Rulings and findings absorbed

| Source | Item | Disposition |
|---|---|---|
| @team-lead | Bring-up is `./infra/kind/scripts/setup.sh`, not `dev-cluster` | Taken — was already my finding #1; now settled |
| @team-lead | `DT_HOST_GATEWAY_IP` must be unset; symptom-identical to F1 | Taken → **F8**, cross-referenced from F1 |
| @team-lead | Take the `dev-web.sh:142` fix; classify `Mine` | Taken — row 3. My earlier "not touched" claim was inconsistent with my own §1 analysis; narrowed to "no bug in its *checks*" |
| @team-lead | Story doc: fix line 385 in place, bracket line 511 | Taken — row 6 |
| @team-lead + @client | `/etc/hosts` is per-machine; trap #6 was mis-scoped | Taken — **the highest-leverage correction in the document**; see C below |
| @security | Credentials by pointer, not value; "non-secret fixtures" framing | Taken |
| @security | R-37 rationale yes, but don't present the observability wildcard bind as a defect; note the Grafana `admin/admin` + wildcard compound | Taken |
| @security | tcpdump: add `-c 20`, last rung, shared-host caveat, UDP-only | Taken |
| @security | **C1** — 8443/8444 are plaintext HTTP despite TLS-looking numbers; no TCP capture; throwaway password at sign-up | Taken — banner + sign-up step |
| @security | **C2** — no `kubectl get secret -o yaml`; use `describe` / fingerprint comparison | Taken |
| @security | **C3** — document `/etc/hosts` *removal* in teardown | Taken |
| @security | MediaTransport is not silent — `MediaConnectionUpdate` is the error channel | Taken; corrects my §4.2 framing (see C) |
| @test | Don't document a Playwright suite (3 stale refs) | Taken — rows 4 and 7 fix two of them |
| @test | **env-tests are runnable today** — promote out of §7 | Taken — new **§6.5**; see C |
| @test | Bare `cargo test` runs ZERO env-tests | Taken — the section's load-bearing sentence |
| @test | MC has **two** instances; tail both or use `-l app=mc-service` | Taken — false-negative avoidance in §4.1 |
| @test | MC logs are JSON; `RUST_LOG` narrowing to `mc_service=` hides the line | Taken — §4.1 caveat |
| @test | Empty `media_servers` ⇒ `connectAll` resolves with `[]` | Taken — see C |
| @test | Vite binds localhost only, no `host` in config ⇒ mirrored mode required | Verified (`vite.config.ts` sets `proxy` but no `host`/`port`) → **F9** |
| @observability | No browser-side join log exists (`logJoinEvent` never called); client metrics off by default | Taken — prevents me citing a signal that cannot appear |
| @observability | Triage ladder, tcpdump last | Taken — **§4.0**, referenced by every scenario |
| @observability | `mc_webtransport_connections_total{status="accepted"}` — its **absence** is the F1 oracle | Taken; verified at `server.rs::run` (recorded before auth) |
| @observability | Don't hardcode observability ports; two path schemes | Taken — derivation, not values |
| @dry-reviewer | **F1** — drop the standalone port table | Taken; see C |
| @dry-reviewer | **F2** — README duplicate deletion | Taken — row 2 scope expanded |
| @dry-reviewer | **F3** — `LOCAL_DEVELOPMENT.md` AC claim is false | Taken — row 5 |
| @dry-reviewer | Follow `devloop-validation.md` shape, not `TEMPLATE.md` | Taken |
| @dry-reviewer | **Cite guard**: `docs/runbooks` is in `IN_SCOPE_DIRS` | Taken — see D. Highest-value warning received |
| @client | Error strings, resting state, dual timeout | Taken — verified verbatim; see C |

## C. Substantive corrections to the runbook's content

**C-i. `/etc/hosts` — two machines, two files, two reasons.** My brief's trap #6 conflated them. Corrected:
- **Windows Chrome** likely needs *no* entry — Chromium implements RFC 6761 §6.3, resolving `.localhost` names to loopback without a hosts file. **I cannot drive Windows Chrome from WSL2, so this is written as the mechanism plus an explicit "not empirically confirmed from this environment"**, with the fallback named: if it does not resolve, the entry goes in `C:\Windows\System32\drivers\etc\hosts` — *not* WSL2's.
- **WSL2 tooling** genuinely needs it: glibc has no `.localhost` special case, so `getent ahosts`, `curl`, and env-tests all require it. That is what `dev-web.sh` checks, and it is a correct check for that audience.
- So F6 is real but re-scoped: WSL regenerating `/etc/hosts` breaks **WSL2-side tests**, not the browser. `generateHosts=false` stays. Every `/etc/hosts` mention in the runbook names its machine; §0's convention distinguishes the two hosts files, not just the two shells.

**C-ii. §4.2 reframed** (@security's correction + @test's stronger case). Not "success reported while the media path is dead" — there *is* an error channel, it just doesn't run to the client's state. Three facts, in order:
1. `joined` is a **≥1-of-N** signal by design (R-21).
2. **Empty `media_servers` resolves successfully with `[]`** — the throw is guarded on `#order.length > 0`, so zero MH connections is reported as success. Purest false join, and plausible here since MH assignment data comes from Redis.
3. The authoritative per-MH picture is the `MediaConnectionUpdate` the client always sends once `connectAll` settles. **MC's log proves signaling, not media** — so §4.1 alone does not catch case 2; the roster check and tcpdump do.

**C-iii. Verbatim client strings** (@client, all verified): rendered text is `SIGNALING: Signaling transport closed` — `errorText.ts::errorText` returns `` `${err.code}: ${err.message}` `` and the view renders `code: message`; `code` is the coarse `SIGNALING`, not `TRANSPORT`. F1 carries **both** possible strings, since either the 15s SDK deadline (`DEFAULT_JOIN_TIMEOUT_MS`) or Chrome's QUIC timeout can fire first: the other is `SIGNALING: Join timed out before a JoinResponse was received`. And the state readout **settles at `disconnecting`**, not `connecting-mc`, because `join()`'s catch calls `disconnect()` — so the doc names `connecting-mc` as the *stage* and `disconnecting` as what is *on screen*.

**C-iv. §1 loses its port tables** (@dry-reviewer F1 — accepted, no counter-proposal). §1 becomes: static topology = "the `hostPort` values in `infra/kind/kind-config.yaml` `extraPortMappings`, all `listenAddress: 127.0.0.1` per R-37"; devloop topology = "allocated per-clone into 20000–29999 by `ports.rs`, published to `/tmp/devloop/ports.json`"; plus **the distinguishing test** — the one command that tells a reader which topology they are on, which is the actual value-add and what neither file can answer. Concrete numbers survive only inline where they make a symptom recognizable (the `tcpdump … udp and port 4433` line), marked at point of use. No standalone reference table, so no new consumer for `docs/TODO.md` §Port Constant Scattering.

**C-v. New §6.5 "Automated checks that exist today"** (@test's gap — the one plan finding I think was most worth raising). `crates/env-tests` is runnable immediately after Step 1: `cargo test -p env-tests --features smoke` / `smoke,flows` / `all`, with `24_join_flow.rs` covering the real AC→GC→MC join. Load-bearing sentence: **bare `cargo test` runs zero env-tests** (no default features), so a reader who runs it and sees green has validated nothing. Section also carries the **two-access-path split** (env-tests reach AC 8082 / GC 8080 via port-forward; the browser reaches AC 8443 / GC 8444 via `extraPortMappings`) and the **org split** (`devtest` vs `demo`, both seeded by `setup.sh`) — both are §1's mechanism applied to the test layer. §7 then narrows honestly to: no Playwright/browser-driven E2E (#18/#19).

## D. Citation style — mandated by a guard I did not know about

@dry-reviewer's catch is the most immediately consequential thing in this round. `crates/dt-guard/src/cite_extract.rs` sets `IN_SCOPE_DIRS = ["docs/runbooks", ".claude/skills"]`, so **the runbook is walked by `cite-no-line-numbers` and `cite-symbol-resolves` at Layer 3**. Every `file.ext:NN` form in my verification log above is a `bare_line_cite` violation if carried into the runbook.

Rule for the prose: **cite by path alone, or `file.ext::symbol`** — and because `cite-symbol-resolves` actually resolves the symbol, every `::symbol` must exist. Row 7 (`SKILL.md`) is in the same guarded scope. The verification log in *this* plan doc keeps its line numbers: `docs/devloop-outputs/` is not in `IN_SCOPE_DIRS`, and precise line cites are the point of a verification record.

## E. Revised outline

```
§0  Which machine am I on?      two-box topology; command-prefix convention covering BOTH shells AND both hosts files
§1  Which cluster am I running? static vs devloop — derivation + the distinguishing test (no port table)
§2  Prerequisites               2.1 Windows (Chrome, .wslconfig mirrored) [marked unverifiable-from-WSL2]
                                2.2 WSL2 tools (→ LOCAL_DEVELOPMENT.md for inotify/keyring)
                                2.3 WSL2 persistence (/etc/wsl.conf generateHosts=false)
                                2.4 Node/pnpm — pins cited, never restated
§3  Bring-up                    Step 0 dev-web.sh --check │ Step 1 setup.sh │ Step 2 /etc/hosts (per machine) │ Step 3 dev-web.sh │ Step 4 Chrome
§4  Is the join REAL?           4.0 triage ladder (state → logs → counters → tcpdump)
                                4.1 MC ground truth (both instances; JSON logs; RUST_LOG caveat)
                                4.2 why client state is optimistic (≥1-of-N; empty-array; MediaConnectionUpdate)
                                4.3 tcpdump (bounded, UDP-only, last rung)  4.4 roster check
§5  Failure modes F1–F10        Symptom → Discriminator → Cause → Fix → Why it recurs
                                F1 IPv4/IPv6 loopback │ F2 nvm not sourced │ F3 nvm use session-only
                                F4 corepack keyid │ F5 proto codegen bypass │ F6 /etc/hosts regenerated (WSL2-side)
                                F7 cert fingerprints stale │ F8 stale DT_HOST_GATEWAY_IP │ F9 Vite unreachable from Windows
                                F10 GC CORS (conditional — only off the default proxy path)
§6  Teardown                    cluster + /etc/hosts removal (C3)
§6.5 Automated checks today     env-tests tiers; bare-cargo-test warning; two-path + org splits
§7  Not on this branch          Playwright E2E (#18/#19); devloop-container cluster; production
    Related runbooks · Changelog · deliberate-TEMPLATE-divergence note
```

@test's bar is adopted throughout §5: every scenario needs a **discriminator**, not just a symptom. F1 and F8 are symptom-identical (dead join at `connecting-mc`, AC/GC fine) and F1/F9 both present as "it doesn't work from the browser" — each pair gets an explicit runnable discriminator, since two scenarios sharing a symptom with no way to tell them apart is a runbook that has not done its job.

---

# Plan Revision 3 (final pre-implementation)

Eight files. Adds `docs/TODO.md` as row 8 (@dry-reviewer F8).

## A. Row 8 — `docs/TODO.md`, and a handoff that was misassigned

`docs/TODO.md` §Port Constant Scattering carries a bullet titled **"Auto rollout-restart on cert-secret recreate (R-49 / task #20 handoff)"** with `Owner: R-49 dev runbook (task #20)`. @client surfaced it; @dry-reviewer diagnosed the category error, and they are right: **the entry's substance is an automation change to `infra/kind/scripts/setup.sh`**, which is a behaviour change with the regression surface the entry itself names. A prose runbook cannot deliver it. Ticking it off on the strength of this devloop would evaporate the automation — no owner, no tracking, and the only surviving trace a script comment promising something that already "shipped" as documentation.

Three surfaces, three jobs, no restatement:

1. **Runbook §5-F7** owns the *why* and the sequence. This discharges the documentation half — the only half task #20 can do.
2. **`docs/TODO.md` (row 8)** — retitle to name only the remaining `setup.sh` automation, move the owner off task #20 onto operations/infrastructure, and replace its restatement of the manual sequence with a pointer to the runbook. **Not ticked.** Classification **Mine**.
3. **`scripts/generate-dev-certs.sh` (row 4b)** — keeps its printed recovery sequence; loses the false "(R-49 will automate this)".

**F7's sequence follows the script's own text, not a paraphrase** (@team-lead's binding diagnosis-drift check). Verified, the script prints three steps: `./infra/kind/scripts/setup.sh` (which regenerates expiring leaves *and* recreates the `mc/mh-service-tls` Secrets) → `kubectl rollout restart deployment/mc-0 deployment/mc-1 deployment/mh-0 deployment/mh-1 -n dark-tower` → restart the dev server. @client's four-step version splits step 1; I am using the script's three-step form so the two texts match verbatim.

`DAYS_WT_CERT=14`: stating "14-day" in prose is defensible where a bare version number would not be, because it is an **external Chrome constraint on `serverCertificateHashes`**, not a value we chose. The reason attaches to Chrome's cap — that is the part that stops someone helpfully lengthening it.

## B. Row 7 — HOLDING. I verified the disputed premise independently; @team-lead is right.

@test and @team-lead reached opposite conclusions, so I checked ADR-0033 myself rather than picking a side. **@test's Layer-8 premise fails**, on four independent points:

- `adr-0033` § "Decision" (semantic-guard move): *"Layer 8 (env-tests) renumbers to **Layer 7**. Layer-script set is now `layer1.sh` through `layer7.sh`."*
- The SLO table row reads *"7 (Env-tests, **including Playwright**)"* — Playwright is explicitly **inside** Layer 7 in the target design, not a separate layer.
- `lang/ts/e2e.sh` for Playwright is sited in the Layer 7 wrapper set, "added when task #15 lands".
- The § "Negative" renumbering-churn list enumerates exactly which files the Layer 8 → 7 rename had to touch — SKILL.md, ADR-0030, the ADR, the debate doc, ≤4 specialist INDEXes. **The user story is not on that list**, which is precisely why task #19 still says "Layer 8": it is stale pre-renumbering text that the rename missed.

Confirmed there is no `Layer 8` in `layer-all.sh`, `verify-completion.sh`, or SKILL.md.

So row 7 is **the same case as row 4**: a valid forward reference to a specified-but-unbuilt consumer. Keep the clause, mark it pending with its task number, do not delete, and do not re-point it to a layer that does not exist. My Revision-2 instinct that all three Playwright sites should be treated alike was right; only the *treatment* changed — mark, not delete.

**Holding for @test's corrected edit** per @team-lead. Rows 1-6 and 8 are unaffected and proceed.

**A consequence neither reviewer has dispositioned — flagging, not fixing.** If ADR-0033 is authoritative, then story task #19's "Layer 8" language (four sites) is itself stale, and it is *the* root defect here: it is what led @test to a wrong conclusion about their own task's scope. That is in `docs/user-stories/2026-05-02-browser-client-join.md` — row 6, a file I am already editing. I am **not** taking it, for two reasons: it falls outside my artifact-kind rule (which @team-lead scoped to wrong bring-up command, scheme, or port), and correcting it would change what task #19 is scoped to build, which is a real decision in @test's domain rather than a typo. @team-lead / @test to disposition.

## B2. Ownership note on row 7 — the story says the file is operations-owned

@test's re-ruling supersedes both my "delete the false clause" framing and @dry-reviewer's mirror analysis, because it establishes something neither of us had: **browser E2E is specified as Layer 8, not Layer 7.** Story task #19 reads *"Edit `.claude/skills/devloop/SKILL.md` **Layer 8** description so `pnpm --filter @darktower/web-app test:e2e` runs sequentially after `cargo test -p env-tests --features all`"*. So marking the clause pending *inside the Layer 7 row* would preserve a misdescription. Two edits, per @test:
- `:399` table row → `dev-cluster + Rust env-tests (`cargo test -p env-tests --features all`)`.
- One sentence appended to the Layer 7 prose block: browser E2E is **not** Layer 7; it lands as **Layer 8** per task #19 (R-48), pending #18. That creates the marker, not the layer — there is no Layer 8 in SKILL.md, `layer-all.sh`, or `verify-completion.sh` today.

This also resolves @dry-reviewer's mirror concern without touching ADR-0033: the ADR keeps the Playwright design target, SKILL.md stops claiming it runs today, and the forward reference survives *with its task number* at its specified layer.

**Ownership flag I am raising rather than resolving unilaterally.** Story task #19 says of this same file: *"**Cross-boundary**: edits SKILL.md (operations-owned) — pair `--paired-with=operations`"*. So the story's own decomposition calls `.claude/skills/devloop/SKILL.md` **operations-owned** — which would make row 7 `Mine`, with @test as the domain authority on the Layer-7 *content* rather than as the file owner. I am **keeping the row at Minor-judgment / owner `test` and landing @test's trailer regardless**, because the tier that requires more scrutiny is the safe direction and the trailer is already granted. Flagging it so @code-reviewer and @team-lead can correct the record if they read the ownership differently — I would rather over-attribute than quietly claim a file the story assigns elsewhere.

## C. Citation style — pinned, after verifying two traps @client found

Both verified in `crates/dt-guard/src/cite_extract.rs`:

1. **`.ts` / `.svelte` cites are never checked.** `SUPPORTED_RESOLUTION_EXTENSIONS = ["rs","sh","toml","yaml","yml","md","proto"]`; `symbol_resolves_in_file` returns `true` early for anything else. So the guard gives **zero protection on exactly the client-side cites this runbook leans on hardest**. Consequence for style: prefer exported, greppable names a reader can jump to (`connectAll`, `MeetingSessionState`, `errorText`, `loadCertFingerprints`) over private members (`#terminate`, `#settleJoinFailure`) — the reader cannot navigate to a private member and the guard will not catch a wrong one.
2. **YAML cites resolve only at column 0.** `YAML_RESOLVER = (?m)^([A-Za-z_][\w\-]*)\s*:` — no leading whitespace. So in `infra/kind/kind-config.yaml` only `kind`, `apiVersion`, `name`, `nodes`, `containerdConfigPatches`, `networking` resolve; **`extraPortMappings` is indented and will NOT**, nor will `MC_WEBTRANSPORT_ADVERTISE_ADDRESS` (nested under `data:`). Those are the two most load-bearing infra references in the document, so both use **path-only cites with the key named in prose**.

Shell (`^name()`) and Rust (`fn|struct|enum|trait|impl|const|static|type`) resolve fine, so `dev-web.sh::check_wt_endpoint`, `setup.sh::seed_demo_org`, and the `connection.rs` cites are safe.

**Convention is inherited, not restated** (@dry-reviewer): `devloop-validation.md` §10 already declares its scope as "other long-lived docs under `docs/runbooks/**`", so this file is covered by its terms. One line in Related — "cite convention: `devloop-validation.md` §10" — and nothing more. Writing my own copy of the three cite shapes would be the exact failure this review is about.

## D. §4.2 framing — @semantic-guard's, adopted verbatim in substance

They rejected my "second-order" characterisation and they are right: it is the *same* error committed against the replacement signal. `joined` measures *signaling completed + ≥1 MH accepted an envelope*, read as *media works*. `"Join succeeded"` measures *signaling completed*, read as *media works*. Same error, second victim — and it landed in my plan the way it lands in a debugging session: the first signal disappointed, so I reached for a more trustworthy one and inherited the identical defect, because I replaced the **signal** without restating the **claim**.

So §4.2 does not rank signals by trustworthiness. It states:

> A signal is never trustworthy or untrustworthy. It is trustworthy **for a specific claim**, and useless for every other one. When a signal disappoints you, the move is not "find a more reliable signal" — it is: state the claim you actually need, then find the signal that measures it.

Plus their operational test, verbatim, because it is the part a reader can run on a signal this runbook never mentions: *finish the sentence "this proves ___"; if you cannot fill the blank without the words **working**, **fine**, or **healthy**, you have a vibe, not a signal.*

**Ordering: the empty-`media_servers` case leads**, not ≥1-of-N. Their reasoning is decisive — in the empty case *nothing lied and nothing malfunctioned*: the client says `joined` (true), MC logs `"Join succeeded"` (true), and there are zero media connections. Every signal green, every signal correct. That is the case that proves the lesson cannot be "some signals are unreliable." And nobody designed it: it is emergent from the `#order.length > 0` guard on the all-failed throw, which is why reading the R-21 design docs would never have warned you.

**Discriminator** (@semantic-guard verified `sendMediaConnectionUpdate` has no empty-reports early return, so the message is always sent): the `mc.media_connection_update` span's **`statuses_count`** field — `0` = empty fan-out, `< N` = partial, `= N` with all `state="connected"` = full, *and still only proves each MH accepted a connect envelope*. That third row makes the table demonstrate the rule instead of asserting it. Cheaper than the roster check and it fires on every join.

## E. Two corrections I am taking against things I had already written

**E1. @observability withdrew their own hedge, and the corrected version is sharper.** They had told me flat `accepted` does not prove F1 because a TLS failure also leaves it flat; they then traced `wtransport`'s `Endpoint::accept()` and found it yields on **QUIC Initial packet arrival**, before the TLS handshake. So a cert failure *does* increment `accepted` and then produces `warn!` `"Failed to receive session request"`. That gives F1 and F7 cleanly separable server-side signatures, and I am writing the 4-row table they supplied.

But @semantic-guard's counter also applies, and I am taking both: absence of that counter proves **"no QUIC session reached the MC accept loop"** — not "packets never arrived." It bundles at least three causes (never left the Windows/WSL2 boundary; dropped at podman/NodePort; handshake failed before completing). §4.0 words it as the boundary it establishes and lets `ss -uln` / tcpdump split what is below it. Also noting `status="rejected"` (capacity) exists so a reader who sees it does not read it as a cert problem.

**E2. My F6 env-tests claim was wrong** (@test). I wrote that WSL regenerating `/etc/hosts` breaks env-tests. It does not. Verified: nothing in `crates/env-tests` references `demo.localhost` at all; `build_org_host_header` is pure string formatting that produces `devtest.localhost:8082` as an **HTTP `Host` header only** — nothing resolves it. env-tests need `localhost` to resolve, which a regenerated `/etc/hosts` always provides. F6's real blast radius is thin and will be stated plainly: `dev-web.sh`'s own preflight WARN, and WSL2-side `curl`/browser hits on `demo.localhost:5173`. Not the browser (RFC 6761), not env-tests. A trap oversold is a runbook trusted less.

@test's bonus framing is going in: a `Host` **header** and a DNS **name** look like the same value and are correct in different boundaries — which is why "add it to `/etc/hosts`" *feels* like it should fix an AC org-routing problem and does not. That is §1's mechanism again.

## F. Sweep boundaries — both directions

@dry-reviewer confirmed the `dev-cluster setup` sweep is complete at two wrong sites (`README.md`, `dev-web.sh`) and warned against extending it: every other occurrence — `layer7.sh`, `devloop.sh`, ADR-0030, the debate docs, several TODO entries — is **correct in context**. The rule is not the string, it is the audience: **a human on the WSL2 host gets `./infra/kind/scripts/setup.sh`; the devloop container gets `dev-cluster setup`.** That sentence goes in §1, since a reader who has seen `dev-cluster setup` in `layer7.sh` will otherwise wonder which is right.

Row 3(c) makes the runbook the canonical home for the mirrored-mode explanation and shortens `dev-web.sh`'s block comment to a pointer — otherwise the script violates its own stated contract ("the runbook stays the source of prose/diagnosis") the moment this lands. The **four MC/MH configmap copies are explicitly not in this diff**: cross-boundary infra files whose comment is load-bearing where it sits, for someone editing that literal. @dry-reviewer will file them as an ADR-0019 extraction opportunity at verdict time.

---

## Pre-Work

None.

---

## Implementation Summary

New `docs/runbooks/client-dev-local.md` (**1010 lines**): §0 two-machine topology · §1 which-cluster (three behavioural discriminators, no port table) · §2 prerequisites · §3 bring-up · §4 is-the-join-real (triage ladder → MC ground truth → claim-scoping → tcpdump → roster) · §5 **F1–F10**, each Symptom → Discriminator → Cause → Fix → Why it recurs · §6 teardown · §6.5 env-tests · §7 out-of-scope.

Scope grew 2 files → 10 paths. Every addition was a **sibling of a defect the task already required fixing** — the same false claim or stale path living in another file. Fixing a subset and calling the rest out-of-scope is the framing-lock anti-pattern the review protocol names, and it was invoked (and honoured) five separate times.

---

## Files Modified

10 paths: `docs/runbooks/client-dev-local.md` (new) · `packages/web-app/README.md` · `scripts/dev-web.sh` · `scripts/generate-dev-certs.sh` · `docs/LOCAL_DEVELOPMENT.md` · `docs/user-stories/2026-05-02-browser-client-join.md` · `.claude/skills/devloop/SKILL.md` · `docs/decisions/adr-0013-local-development-environment.md` · `docs/TODO.md` · this devloop output.

---

## Devloop Verification Steps

`./scripts/layer-all.sh`: **L1 OK/35s · L2 OK/1s · L3 OK/6s · L4 OK/229s · L5 OK/29s · L6 FAIL/5s · L7 OK/846s**

**Layer 6 FAIL is pre-existing and out-of-domain** — `pnpm audit` reports 11 advisories across 6 packages against an unmodified lockfile. No `package.json`, `pnpm-lock.yaml` or `.npmrc` is in this diff, so the result is byte-identical with or without this work. The always-run case ADR-0033 names: external state moved under an unchanged footprint. Spun out by user decision (option c) rather than suppressed — @security argued suppression would invert the manifest's own admission criterion, since every existing entry earns its place by demonstrating *no in-range fix exists*.

@security's framing, recorded verbatim: **"Layer 6 red is a true positive about the repository and a false positive about our diff."**

Layer 3 re-run clean (33/33) after every edit round, including twice when files changed *after* a pipeline run had already passed them.

---

## Code Review Results

**7/7 verdicts, zero deferrals across the entire panel.**

| Reviewer | Verdict | Findings | Scope |
|---|---|---|---|
| Security | RESOLVED-FIXED | 4 found, 4 fixed | re-scanned to 10 paths |
| Test | RESOLVED-FIXED | 2 + 1 suggestion, all fixed | 9 files |
| Observability | RESOLVED-FIXED | 4 found, 4 fixed | extended to 9 files |
| Code Quality | RESOLVED-FIXED | 3 found, 3 fixed | 9 files, reissued |
| DRY | CLEAR | 0 against diff (8 resolved at Gate 1) | re-reviewed to 9 |
| Semantic Guard | CLEAR / native SAFE | 0 | all 9 paths |
| Client | RESOLVED-FIXED | 3 found, 3 fixed | — |

**Trailers (4)**: `client` (README) · `test` (SKILL.md) · `security` (generate-dev-certs.sh, GSA co-sign) · `infrastructure` (ADR-0013, late owner review).

**Best finding — @observability O1**: §4.2 cited a `jq` command that returns empty on a *healthy* join, in the section that trains readers to treat absence as evidence. The span is real but the handler emits no event on the success path, and `fmt::layer().json()` writes per-event with no span-lifecycle events configured — so its only log is a `debug!` on the error branch, filtered by the deployed `info` global **for exactly the reason the runbook documents 400 lines earlier**. The document contained the explanation for why its own command could not work.

---

## Accepted Deferrals

**(none)** — zero deferrals, zero spin-outs of any reviewer finding.

Four items were *surfaced while verifying* and were never findings against this diff (Step 9: §Accepted Deferrals is reserved for findings that remain in the diff):
- `docs/TODO.md` §Cross-Service Duplication — four MC/MH configmap copies of the mirrored-mode rationale (DRY, infrastructure-owned).
- `docs/TODO.md` §Observability Debt — MC emits no log event on the `MediaConnectionUpdate` success path.
- `docs/TODO.md` §Developer Experience — stale `scripts/dev/iterate.sh` path, **next-up**, infrastructure-owned.
- `docs/TODO.md` §Dependency Vulnerabilities (pnpm audit) — the 11-advisory Layer 6 spin-out.

Surfaced, not filed: **ADR-0030 §Port Map File** states `~/.cache/devloop/devloop-{slug}/ports.json`; the implementation uses `/tmp/devloop-{slug}/`, conflating it with `port-registry.json`. Pre-existing ADR-vs-code drift; the runbook correctly follows the code.

Security-owned process items: **ADR-0033 §12 source-of-record gap** and the **ID-scoped suppression hazard**.

---

## Rollback Procedure

1. Start commit: `dc8c7ebc97f9cafafa968ac807bbfa6e8473287c`
2. Review: `git diff dc8c7eb..HEAD`
3. Soft reset: `git reset --soft dc8c7eb`
4. Hard reset: `git reset --hard dc8c7eb`

Documentation-only change — no migrations or manifests to unwind.

---

## Issues Encountered & Resolutions

**Implementer outage.** Hit a transient API 500 and went dark ~2.5h with nothing on disk. Recovered via a status ping. The plan document was the recovery point — which is why every ruling and reviewer condition was written into `main.md` rather than left in the message log. Five of nine agents hit transient 500s; reviewers recovering mattered less, since their findings were already recorded.

**Guard that fired loudly and named the wrong thing.** `parse_cross_boundary_table` requires exactly three cells; the plan's table had a `#` index and a `Change` column, so Layer A read the index as the Path and reported every real file as `scope_drift_inbound`. The failure reads as "my scope is wrong" when the fault is "my table shape is wrong". Fixed, with a `FORMAT CONTRACT` comment recording both that trap and a second (the parser consumes *any* pipe table in the section).

**Pre-commit Gate-2 gate bypassed — explicitly authorized by the user.** The `.githooks/pre-commit`
Gate-2 authority verdict (ADR-0033 §8.5) refused the commit with `Gate-2: verdict is FAIL`, because
`layer-all.sh` recorded Layer 6 red. That hook exists to close skip-vector C — *"a Lead (or agent)
asserting a pipeline pass in lieu of actually running it"* — so the Lead did **not** self-authorize
the bypass; it was put to the user as a blocking decision and they chose it.

What was bypassed, precisely: **Layer 6 only**, red on **11 pre-existing npm advisories across 6
packages** against a lockfile this diff does not touch. Layers 1, 2, 3, 4, 5 and 7 all passed, and
Layer 3 was re-run clean (33/33) against the final frozen tree immediately before the commit. The
advisories are tracked in `docs/TODO.md` §Dependency Vulnerabilities (pnpm audit) with a four-step
remediation order and an owner. Nothing about this diff was skipped.

The honest cost: the recorded Gate-2 verdict for this commit says FAIL, and `--no-verify` leaves no
trace in git itself. That is why it is written here — the bypass is discoverable from the devloop
record, which is the only place it exists.

**Masked exit code.** `layer-all.sh` was run through `| tail -60`, so the harness reported exit 0 while `TOTAL_RESULT=FAIL`. The summary block caught it; the pipe status did not.

---

## Lessons Learned

**1. The loop's signature failure — seven instances across five agents including the Lead.** @code-reviewer's statement is the precise one: *a plausible characterisation accepted from someone else without checking, then propagated with the original speaker's confidence attached.* That is why the count kept climbing — each instance arrived looking like new information rather than a relayed claim.

@implementer's general form: **when a fact doesn't fit, we reach for an explanation before we re-examine the fact.** Four instances were inferences about a tool or source invented to explain a gap in our own reading; the ADR date was different — a plausible value supplied for a slot that wanted filling.

Enforceable version: **any date, version, or count asserted in prose needs a source named in the same sentence** — an uncited wrong value and an uncited right value are indistinguishable on the page. And @implementer's corollary: **a figure supplied by a teammate is an input, not a finding.**

**2. What actually worked was not more caution.** It was **writing down which claims were verified and which were inherited** — that is what let later readers spot the unverified ones. Every catch in this loop came from that: @security flagging their own scope-of-claim error, @semantic-guard marking two prior "verified" lines superseded, @infrastructure's 8/8 traceability table, @code-reviewer volunteering that "14 months" was asserted rather than derived, @implementer refusing a Lead instruction their evidence contradicted.

**3. A distinct second failure — verifying the wrong link in the chain** (@semantic-guard). A check that *passes* without proving what it was cited for: "the field exists in the span declaration" vs "the line appears in `kubectl logs`". Different detection ("does this check reach the thing it's cited for?") and different fix from the invented-mechanism class.

**4. Naming a failure mode does not inoculate against it.** The `docs/TODO.md` entry contained a well-written tombstone warning against unverified claims **one section above a live instance of one** — same document, same author, same week. The check has to be mechanical, not remembered.

**5. A validation run is only valid for the tree that existed when it started.** Layer 3 passed, then two guard-relevant files changed — one of them the Lead's own edit to `main.md`, which no one would think to check. Nothing in the workflow surfaces this. @implementer's fix: **record the run's start timestamp alongside `STATUS=`**; ADR-0033 §4 already emits `START=` on stderr, it just isn't carried anywhere a later reader can compare against.

**6. "Frozen" means "no side writes", not "static"** (@dry-reviewer). The freeze permits review fixes by design, so a diff under verdict still grows — this one went 8 → 9 → 10 paths *after* verdicts started landing. Fix adopted mid-loop: **verdicts name the file count they cover.** @security's stronger version: a verdict rendered early doesn't merely miss the new file, it **implicitly certifies** it.

**7. Guard silence is not guard clearance.** The Layer B cross-boundary guard structurally *cannot* see `scripts/generate-dev-certs.sh`, because ADR-0024 §6.4's ADR-0027 bullet is path-independent and the manifest is enumerable-only. Three specialists hand-applied the criterion and converged on reachable-but-not-engaged — visible only because each wrote down that they had done it.

**8. Two protocol defects found, both of the same family** — a rule whose mechanical application produces a confident answer the governing document never authorised:
- **`review-protocol.md`**: §Accepted Deferrals collects DRY extraction opportunities *and* forces RESOLVED-DEFERRED, contradicting ADR-0019. Three reviewers hit it independently. Fix (@dry-reviewer): split by the predicate *"was this raised as a finding during this review?"* — checkable against the message record, not reviewer characterisation, so it cannot be satisfied by rewording.
- **ADR-0033 §12**: MTTR "measured from advisory publication date" names no source of record. GitHub and OSV differ by three weeks on both advisories here — the whole distance between a fired tripwire and a comfortable margin. **A rule that returns either answer depending on who runs it is indistinguishable from having no rule.**

**8b. Absence claims have a shelf life; positive assertions mostly don't.** (@infrastructure, refined with @implementer.) *"`setup.sh:998` calls `deploy_ac_service`"* stays true unless someone edits that line, and a stale copy fails loudly when re-checked. *"`Erratum` appears nowhere in `docs/decisions/`"* is invalidated by **any** concurrent write in scope — including by an agent who doesn't know the claim exists — and is silently false a second later. So @infrastructure's traceability table's eight positives are durable while its three negatives are only as good as their timestamp, which is why they re-ran the greps immediately before each confirm and treated the **md5 as the load-bearing part** of the sign-off.

**This is the Layer-3 staleness finding generalised, and one amendment covers both**: a passing guard run *is* an absence claim ("no violations exist") and decays exactly as `Erratum: 0` does.

Criterion, final form — three clauses:
> Every factual assertion traceable to a named line in-repo; **every absence claim stamped with when it was taken**; and **an empty result treated as a fact about the pattern until the pattern is widened.**

The third clause is @infrastructure's, from nearly publishing an absence claim off a too-narrow glob (`scripts/*.sh` + `scripts/guards/`, when the implementation lives in `scripts/lang/`) and catching it by widening rather than publishing: **"an empty result is evidence about your pattern before it is evidence about the tree."** It would also have caught the truncated `pnpm audit` read and @implementer's mistaken report that @infrastructure's sweep had missed a hit.

**The mechanical fix has a named two-line site**, not just an ADR reference — and the original claim that "ADR-0033 §4 already emits `START=`" was true per-layer and false for the line people actually quote:
- Specified: `adr-0033:195` — `LAYER=N START=<ts> END=<ts> RESULT=<enum>`
- Implemented per-layer: `scripts/lang/_common.sh:463`, with `__LAYER_START` captured at `:371`
- **Dropped by the aggregator**: `scripts/layer-all.sh:165` prints only `LAYER=%d RESULT=%s DURATION=%s` — no `START=`. Same at the budget-breach line `:135`.

That roll-up is exactly the line quoted as "Layer 3 33/33" — the most authoritative-looking output and the only one with no timestamp. Propagating `START=` into `:165` also recovers `END` by derivation, since `DURATION` is already there: one field, both endpoints, aligning the aggregate with the per-layer contract.

**9. Records get annotated, not rewritten** — applied consistently to three mirror pairs (`SKILL.md`↔`adr-0033`, `LOCAL_DEVELOPMENT.md`↔`adr-0013`, runbook↔`dev-web.sh`). @code-reviewer's refinement: **"erratum" implies the original was mistaken when written; it wasn't** — the prose was true in 2025-12 and the code moved. "Amendment" says that, and matches an existing in-repo *shape* (`adr-0011:69`), not just an existing word.

**10. Deferral math changes once a diff is under verdict.** Fix-don't-defer prices the edit; it doesn't price re-notifying seven reviewers whose verdicts named a file count. The discriminator that decided it: **does anything we ship reference it?** `dev-web.sh --help` did (ruled in — the runbook would have been wrong by reference); `iterate.sh` did not (ruled out).
