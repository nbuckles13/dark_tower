# Devloop Output: Story 4 task I-3 — deploy guards for the MH slot model

**Date**: 2026-11-13
**Task**: ADR-0039 §7 template invariants, Kind-only shareProcessNamespace, no MH workload in root apply set, slot-table drift guard, §9 force-delete ban (dt-guard + Layer-3 wrappers)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full panel; Gate-1 SKIPPED — tier=light (manifest: guard machinery only, no runtime behaviour change; security reviews policy content) <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~2h40m (incl. ~37m Layer 7)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e2a688889e7af3c6b306f13e27b65b27053f4a4c` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `light — guard machinery only, no runtime behaviour change; security reviews policy content (run-story manifest)` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned (no check surface: guard machinery, no credential/Debug/client-lifetime surface)` |
| Paired GC | `paired-global-controller` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

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
| `crates/dt-guard/src/kustomize_mh_template.rs` (new) | Mine (machinery); rule-2 membership is security policy content | security (rule-2 content review) |
| `crates/dt-guard/src/kustomize.rs` | Mine | — |
| `crates/dt-guard/src/mh_slot_model.rs` (new) | Mine | — |
| `crates/dt-guard/src/force_delete_ban.rs` (new) | Mine (machinery); the ban's vocabulary is security policy content | security (rule-5 content review) |
| `crates/dt-guard/src/common/mod.rs` | Mine | — |
| `crates/dt-guard/src/common/shell_consts.rs` (new) | Mine | — |
| `crates/dt-guard/src/common/rust_consts.rs` | Mine | — |
| `crates/dt-guard/src/common/mh_slot_sources.rs` (new; review: one home for script paths/const names) | Mine | — |
| `crates/dt-guard/src/common/services.rs` (review: `KIND_ENV`) | Mine | — |
| `crates/dt-guard/src/common/test_code_filter.rs` (review: `strip_comments_file`) | Mine (machinery) | — |
| `crates/dt-guard/src/client_metrics_export.rs` (review: uses `strip_comments_file`, mechanical) | Mine (machinery) | — |
| `crates/dt-guard/src/tool_bin_paths.rs` (review: reads via `shell_const`, mechanical) | Mine (machinery) | — |
| `crates/dt-guard/src/lib.rs` | Mine | — |
| `crates/dt-guard/src/main.rs` | Mine | — |
| `crates/dt-guard/tests/mh_slot_guards_e2e.rs` (new) | Mine | — |
| `scripts/guards/simple/validate-mh-slot-model.sh` (new) | Mine | — |
| `scripts/guards/simple/validate-force-delete-ban.sh` (new) | Mine | — |
| `infra/kind/scripts/deploy.sh` (refactor: `env_root_apply_list`) | Mine | — |
| `infra/kind/scripts/iterate.sh` (remove a live force-delete) | Mine | — |
| `scripts/setup.test.sh` | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/devloop-outputs/2026-11-13-mh-slot-deploy-guards/main.md` | Mine | — |

------|----------------|---------------------|
| `{path}` | Mine \| Not mine, Mechanical \| Not mine, Minor-judgment \| Not mine, Domain-judgment | {specialist or —} |

---

### Gate 1 — SKIPPED (tier=light; reason: guard machinery only, no runtime behaviour change; security reviews policy content)

## Planning

**Problem, in mechanism language.** The MH slot model rests on structural facts no runtime check sees until a cluster misbehaves: (1) the ONE template (`Deployment mh-template`) must carry what makes every MH instance scraped (Prometheus pod-SD keeps on `app` label + port 8083), reachable by MC (gRPC 50053, MH NetworkPolicy podSelector + MC 50052 egress), browser-trusted (`mh-service-tls` Secret mounted) and slot-addressable (4434/UDP is the port mh.sh puts the per-slot hostPort on); (2) `shareProcessNamespace` is Kind-only test instrumentation; (3) the template must never be applied as a workload — only stored as ConfigMap `mh-template`; (4) K and the slot ports have one home (`ports.rs`) and four downstream copies (static kind-config, tmpl placeholder, per-slot ConfigMaps, GC allowlists); (5) a zero-grace force delete (`--force`, which kubectl turns into grace 0 by itself) is the one way to defeat the hostPort slot lock.

**Rules 1+2 — `kustomize` subcommand, new module `kustomize_mh_template.rs`.** Run over every render the kustomize policy already builds. Template identity = `Deployment/<MH_TEMPLATE_DEPLOYMENT>` read from deploy.sh (never retyped); required in the MH base render and the env-root render (exactly one). Rule 1 per template: required labels (ADR-0039 §7 content) present on the POD template; MH NetworkPolicy in the same render has non-empty podSelector ⊆ pod labels, Ingress+Egress, and an egress rule to `app=mc-service` TCP 50052; in the env root, the MC NetworkPolicy's egress rule to MH TCP 50053 selects pods the template labels match, and Prometheus job `mh-service`'s `keep` relabels (read from `infra/kubernetes/observability/prometheus.yml`) match the template's labels and a declared containerPort; containerPorts 8083/TCP, 50053/TCP, 4434/UDP; the 4434/UDP port is the ONE port named `MH_WT_PORT_NAME` (read from mh.sh) — the port mh.sh's instantiation sets the slot's hostPort on — and no port in the template carries a hostPort itself, no hostNetwork (I-2 design: hostPort is per slot; `scripts/setup.test.sh` pins the template's absence and mh.test.sh pins the instantiation); a `secret.secretName: mh-service-tls` volume mounted by the mh-service container. Rule 2: in a Kind render (dir under `overlays/kind`), only the MH template may set `shareProcessNamespace: true` and the env-root template MUST; in every non-Kind render (every base) no pod template may set it. A non-kind environment directory under `infra/kubernetes/overlays/` fails closed (not rendered → unchecked).

**Rules 3+4 — new subcommand `mh-slot-model`.** One bash probe sources deploy.sh (its BASH_SOURCE guard suppresses main) with DT_* unset (static kind-config, hermetic), runs `cache_renders`, sets placeholder content refs, `render_env_overlay` into a temp dir and prints (a) the wrapper render as JSON docs (`render_env_root_json`) and (b) the root APPLY LIST via a new deploy.sh function `env_root_apply_list` (extracted from `apply_env_root`, which now calls it — the ONE derivation the guard reuses). Rule 3: the apply list contains no MH workload (any pod-template-bearing object whose pod labels carry the template's `app` label or whose containers run the template's image repo — both read off the rendered template), and contains no `mh-template-input` doc; the render does contain the template (non-vacuous). Rule 4 (set comparisons, empty-set check first with its own token): K from `ports.rs` (`MH_SLOT_COUNT`) == static kind-config marked UDP slot count; slot ids == 0..K-1; tmpl carries exactly one `${<var>}` line for the var ports.rs binds to `mh_slot_port_mappings` and no literal slot marker (`MH_SLOT_MARKER` read from ports.rs); slot-ConfigMap `MH_HOST_PORT` set == kind-config slot ports == ports of each `MH_WEBTRANSPORT_ADVERTISE_ADDRESS`; rendered gc-service-config `GC_MH_WT_ALLOWED_PORTS` == slot ports, `GC_MH_WT_ALLOWED_HOSTS` == slot hosts, `GC_MH_GRPC_ALLOWED_CIDRS` == `networking.podSubnet` of the static kind-config AND of the tmpl, `GC_MH_GRPC_ALLOWED_PORTS` == the template's `grpc` containerPort. `GC_MH_GRPC_ALLOWED_SCHEMES` is policy and is out (per paired GC). Each disagreement names both sides.

**Rule 5 — new subcommand `force-delete-ban`.** Files: git tracked + untracked-not-ignored under `infra/`, `scripts/`, `crates/env-tests/`, `packages/*/e2e/`. Comments are not commands: Rust via `test_code_filter::strip_comments`, TS/JS via `ts_lex::lex`, `#`-languages via a quote-aware `#` stripper, Markdown via `markdown_code` (code spans/fences only). Each file is cut into command units (shell: backslash continuations joined, split on newline/`;`/`&&`/`||`/`|`; code: split on `;`), tokenised on whitespace/quotes/brackets/commas. A unit fails when it has a `delete` token, a pod target (`pod|pods|po`, `pod/x`, comma list containing one, or `all`) and a force flag (`--force`, `--force=true`), in any order. `--grace-period=0` alone is NOT a force delete (kubectl converts it to 1); `--force` alone IS (kubectl sets grace 0) — the stricter-than-spec arm is flagged for @security. No allowlist, no suppression; vacuity (zero files) fails. Live violation found: `infra/kind/scripts/iterate.sh:218` — replaced by the graceful delete the scale-to-0 already performs, with a loud wait.

**Tests.** Pure `check_*` functions take parsed inputs; unit fixtures prove every negative named in the task and its positive arm (template missing each property; sPN in base / missing in Kind; MH Deployment in root apply set; each drift pair; each force-delete ordering) plus e2e over a temp git repo for rule 5 and the binary STATUS surface. The shipped tree passes via the Layer-3 wrappers.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Guards
| Rule | Where | Reads (never retypes) |
|------|-------|-----------------------|
| 1 template invariants | `dt-guard kustomize` → `kustomize_mh_template.rs` (base, Kind overlays, env root) | template name `deploy.sh:MH_TEMPLATE_DEPLOYMENT`, hostPort port name `mh.sh:MH_WT_PORT_NAME`, Prometheus `mh-service` keep relabels |
| 2 Kind-only `shareProcessNamespace` | same module; `KIND_PID_NAMESPACE_SHARERS` (security content) | none on any non-Kind pod template; Kind: only the MH template, required in env root, single-container (security addition); non-kind env dir fails closed |
| 3 no MH workload in root apply set | `dt-guard mh-slot-model` → `mh_slot_model.rs` | runs deploy.sh's `render_env_overlay` + new `env_root_apply_list` (now THE derivation `apply_env_root` applies) hermetically (DT_* unset, Secrets dropped in the probe) |
| 4 slot-table drift | same | K/marker/placeholder from `ports.rs`; static + tmpl kind-config; `mh_slot_table`; rendered slot ConfigMaps; the rendered ConfigMap carrying GC's allowlists (set comparison, empty-side = vacuous token) |
| 5 force-delete ban | `dt-guard force-delete-ban` → `force_delete_ban.rs` | `--force` (Go ParseBool) + `delete` + pod target any order; unresolvable target fails closed; API zero-grace; split `.arg` builder per file; comments/prose not commands; no allowlist/suppression |

### Additional Changes
- `infra/kind/scripts/iterate.sh`: removed a live `kubectl delete pod ... --grace-period=0 --force` (the scale-to-0 deletes gracefully; now a loud bounded wait).
- `infra/kind/scripts/deploy.sh`: `env_root_apply_list()` extracted from `apply_env_root` (pure refactor; `scripts/setup.test.sh` now uses it too).
- Interpretation recorded: I-2's template carries NO hostPort by design (mh.sh sets it per slot); "4434 declares hostPort" is enforced as "4434/UDP is the one port named `MH_WT_PORT_NAME`, no other port declares a hostPort, no hostNetwork".
- Recorded residuals (security-accepted, at the matcher site): builders whose args are not `.arg`/`.args` string literals; raw API calls whose zero grace is not spelled in the file.

---

## Files Modified

See `git diff --stat HEAD` (lead commits). New: `crates/dt-guard/src/{kustomize_mh_template,mh_slot_model,force_delete_ban}.rs`, `crates/dt-guard/src/common/shell_consts.rs`, `crates/dt-guard/tests/mh_slot_guards_e2e.rs`, `scripts/guards/simple/validate-{mh-slot-model,force-delete-ban}.sh`.

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` → `TOTAL_RESULT=OK` (2499s).

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 7 |
| 2 Format | OK | 5 |
| 3 Guards (incl. new validate-mh-slot-model, validate-force-delete-ban) | OK | 125 |
| 4 Test | OK | 138 |
| 5 Lint | OK | 2 |
| 6 Audit | OK | 6 |
| 7 Env-tests (Rust + browser E2E) | OK | 2216 |

Fast check before review: `layer-fast.sh` L1–L6 OK.

---

## Code Review Results

Gate 3 verdict table:

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 5 | 5 | 0 | Rules 2+5 policy content |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | 9 scratch-tree mutations all failed loudly |
| Observability | CLEAR | 0 | 0 | 0 | Scrape keeps read from prometheus.yml |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | |
| DRY | RESOLVED-FIXED | 9 | 9 | 0 | No cross-service extraction |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | iterate.sh fail-loud + derived wait |
| Paired GC | RESOLVED-FIXED | 1 | 1 | 0 | GC env configMapKeyRef wiring check |

Semantic Guard not spawned (no check surface in the diff).

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 5 found, 5 fixed, 0 deferred

F1 case-insensitive pod type; F2 stdin/xargs targets fail closed; F3 `crates/devloop-helper/` added to ban scope; F4 hostPID/hostIPC rejected on MH template; F5 sharer single-container count includes initContainers. Security-accepted residuals (documented in module doc): non-literal split builders, runtime-built zero-grace API calls (need AST/runtime pass).

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

F1 probe strips every inherited `DT_*` var; F2 fixture for `unrendered_environments` fail-closed arm.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No findings. No metric/log/trace surface added.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 9 found, 9 fixed, 0 deferred

Shared `common/mh_slot_sources.rs`; scope list single home; dead `RULE_IDS` wired; long functions split; port consts reused; stale docs fixed; iterate.sh masked failure.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
F1–F9 (git_changes reuse, shared `strip_comments_file`, `logical_lines`, `shell_const`, `mh_slot_sources`, `KIND_ENV`, `meta_str`, port consts, shared e2e harness) — all fixed.

**Extraction opportunities** (appended to `docs/TODO.md`):
None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

iterate.sh: failed pod listing now exits loudly; wait derived from `terminationGracePeriodSeconds` + named margin.

### Paired Global Controller
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

Guard now proves GC consumes the checked allowlist ConfigMap (`gc_env_ref_drift`).

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `e2a688889e7af3c6b306f13e27b65b27053f4a4c`
2. Review all changes: `git diff e2a688889e7af3c6b306f13e27b65b27053f4a4c..HEAD`
3. Soft reset (preserves changes): `git reset --soft e2a688889e7af3c6b306f13e27b65b27053f4a4c`
4. Hard reset (clean revert): `git reset --hard e2a688889e7af3c6b306f13e27b65b27053f4a4c`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit (guards + iterate.sh fix + deploy.sh `env_root_apply_list` extract move together; reverting the extract alone breaks the guard probe) (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Live force-delete violation
**Problem**: `infra/kind/scripts/iterate.sh` force-deleted AC pods (rule 5 found it on first run).
**Resolution**: Replaced with graceful scale-to-0 + bounded, fail-loud wait derived from `terminationGracePeriodSeconds`.

### Issue 2: "4434 declares hostPort" vs I-2 template design
**Problem**: I-2's template intentionally carries no hostPort (mh.sh sets it per slot).
**Resolution**: Guard pins 4434/UDP as the sole port named `MH_WT_PORT_NAME` and forbids template-level hostPort/hostNetwork; mh.test.sh pins mh.sh setting the slot hostPort on it. Test reviewer confirmed this satisfies the test-gates item.

---

## Lessons Learned

1. Drift guards that read a value from the same file they validate against are circular; compare against the independent SSoT (ports.rs) and prove the consumer actually reads the checked source (GC configMapKeyRef).
2. A new ban guard found a live violation on first run — run new guards against the shipped tree early.

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
