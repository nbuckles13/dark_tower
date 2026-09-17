# Devloop Output: dt-guard Proof-of-Trap Fixtures + Bash-Harness Coverage

**Date**: 2026-09-16
**Task**: Close the dt-guard proof-of-trap gap (fixtures for every rule ID of four unfixtured guards + four unasserted counter-zero-init rules) and make coverage tooling see the bash guard harnesses.
**Specialist**: infrastructure (paired-with=test)
**Mode**: Agent Teams (v2), full
**Branch**: `feature/dt-guard-code-coverage`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a4d1b9281a094f0f48e9946e6b3c9b708bf332cb` |
| Branch | `feature/dt-guard-code-coverage` |
| Lead Model | `claude-opus-4-8[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `confirmed` |
| Test | `paired-test` — confirmed (paired, replaces test reviewer slot) |
| Observability | `confirmed` |
| Code Quality | `confirmed` |
| DRY | `confirmed` |
| Operations | `confirmed` |
| Semantic Guard | `confirmed` |

**RECOVERY 2026-09-17:** loop interrupted by a WSL crash mid-`implementation` (iter 1). Roster respawned; main.md Loop State treated as authoritative (per SKILL §Recovery). Verified on resume: 5 new `*_e2e.rs` + `tests/common/` compile clean and all pass; `ci.yml` coverage rework + `permissions: contents: read`, `list-coverage-suites.sh`, and 4 `# coverage-exempt:` markers all in tree. Deliverable 8 (§8.6 coverage lane in `docs/runbooks/devloop-validation.md`) completed with @operations.

**Gate 2 (2026-09-17):** attempt 1 red at Layer 2 (rustfmt on new e2e files) → fixed via `cargo fmt`. attempt 2 red at Layer 3 (`validate-cross-boundary-scope` — 6 files missing from the Classification table: `.gitignore`, `tests/common/mod.rs`, 4 marker `.test.sh`) → table reconciled. attempt 3: **Layers 1–6 GREEN** (L4 `cargo-test-passed`+`nx-test-passed`; L6 audit N/A no-dep-changes). **Layer 7 = PRECONDITION_FAILURE `cluster-setup-failed`** — operator lane, NOT a code regression: the WSL crash killed the Kind control-plane container, leaving a dead cluster the helper reused (apiserver `10.255.255.254:23303` refused). Recovered via `dev-cluster teardown` + `setup` (host-side helper, ADR-0030). **Final full run GREEN (exit 0):** L1/L2/L3/L5 OK, **L7 OK (536s — env-tests + browser E2E passed)**, L4 & L6 the documented self-justifying N/A aggregates (L4 children `cargo-test-passed`+`nx-test-passed`; L6 audit no-dep-changes). Review findings resolved in-diff: O-A (fail-open `cfg=coverage` assert → three tight fail-closed instrumentation asserts), O-B (gitignore coverage-step artifacts). Required-`Code Coverage`-check = advisory (tracked in `docs/TODO.md`, repo-owner decision).

**Gate 3 — ALL 7 VERDICTS CLEAN, ZERO DEFERRALS (2026-09-17):**

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | CLEAR | S1–S7 + F3 verified against source; O-A/O-B fixes confirmed; CODECOV_TOKEN scoped to upload step |
| Test (paired) | CLEAR | Every rule ID proof-of-trapped w/ live-demonstrated reddening; coverage measured: counter_zero_init.rs 95.02%, media_telemetry_deny.rs 95.75% (>85%), profraw delta 94→139 |
| Observability | CLEAR | All rule IDs isolate (assert-absent siblings); clean alert fixture ships inventory pair; fixture-root hygiene holds; no policy regressions |
| Code Quality | RESOLVED-FIXED | 1 finding (spawn/capture dedup in common/mod.rs) fixed in-review; ADR-0002/0033/0034/0024 compliant |
| DRY | RESOLVED-FIXED | 1 finding (bash `is_lazy_reason` vocab fork) fixed via length-floor-only (SSoT `ignore.rs::MIN_REASON_LEN`); single derivation point confirmed |
| Operations | RESOLVED-FIXED | O-A + O-B fixed in-diff; §8.6 owned deliverable complete + code-consistent; marker-discipline narrowing owner-approved |
| Semantic Guard | CLEAR | No anti-pattern in non-test production code; fixture bad-content confined to TempDir roots w/ reserved placeholders |

Acceptance MET: pipeline green; coverage >85% for both target files; every previously-unfixtured rule ID executed ≥1; paired-test's Gate-3 record enumerates each rule ID's proof-of-trap mutation.

**Gate 1: PLAN APPROVED (all 7 reviewers confirmed).** Build-in contract (verified at Gate 2/3):
- CI: profraw **delta** assert (snapshot before bash-suite loop, fail if not grown — per-suite; not a bare ≥1-exists); DT_GUARD precondition (fail if empty/non-exec/==repo target/release/dt-guard); assert CARGO_ENCODED_RUSTFLAGS contains `--cfg`+`coverage` after env setup; identical profile flags build/test --no-report/report; `llvm-cov clean` first; SSoT via `find scripts -name '*.test.sh'`+grep DT_GUARD (recursive, drop `guards/` scope), floor pinned at **2**, two-way marker (`# coverage-exempt:<reason>` on non-drivers → unmarked-unselected fails), accumulate exit across loop; `set -euo pipefail`; `permissions: contents: read`.
- Fixtures: **source-derived rule-ID SSoT** (extract `*_RULE_ID` consts from each guard .rs, set-equality with fixtured set, bidirectional red); subprocess via `CARGO_BIN_EXE_dt-guard`/`dt_guard_bin()` (not in-process run() nor cargo_bin) so `--explain policy=` tokens are captured and llvm-cov sees the child; exact-set `policy=<guard>::<rule_id>` assertions + assert-absent co-firing siblings + non-vacuous positive control on clean fixtures; `runbook_url` exercises BOTH containment branches asserting the specific message; hermetic git for cross_boundary_scope e2e; TempDir roots carry no `/tests/` or `/fixtures/` segment (is_scan_exempt avoidance) + positive control that a real label was scanned; settle 203.0.113.5-vs-192.0.2.1 doc drift.

---

## Task Overview

### Objective
Two coverage gaps on `dt-guard`, measured on main after PR #72 via `cargo llvm-cov -p dt-guard`:

1. **Proof-of-trap gap.** `dashboard_panels.rs` (65%), `metric_labels.rs` (52%), `alert_rules.rs` (73%),
   `cross_boundary_scope.rs` (76%) have NO fixture test of any kind — only their Layer-3 wrappers run them on the
   real (clean) tree, which proves they pass but nothing about whether they *fire*. And `counter-zero-init`'s bash
   test asserts only 6 of 10 rule IDs; `slot_witness_cfg_test`, `catalog_but_no_entrypoint`,
   `non_literal_metric_name_in_entrypoint`, `empty_scan_no_counters` have zero executions.
2. **Bash-harness invisibility.** `counter_zero_init.rs` (48%) and `media_telemetry_deny.rs` (77%) are mostly
   exercised by bash harnesses (`counter-zero-init.test.sh`, 19 cases; `media-telemetry-deny.test.sh`, 92 cases)
   that drive `target/release/dt-guard`, which `llvm-cov` cannot see.

### Deliverables
1. Proof-of-trap fixtures for `dashboard_panels`, `metric_labels`, `alert_rules`, `cross_boundary_scope`: for EVERY
   rule ID each guard can emit, one negative fixture that must produce that rule's finding, plus one clean fixture per
   guard that must pass. Cargo-side, using the `tests/fixtures/` + integration-test layout (shape:
   `tests/fixtures/media_telemetry_deny/`, `tests/release_build_profile_e2e.rs`) so they run under `cargo test` and
   are visible to `llvm-cov`. No weakening/broadening of any rule; if a rule cannot fire from a fixture, record why
   in the test and file it.
2. Close the four unasserted counter-zero-init rule IDs the same way (cargo-side fixtures, not more bash cases).
3. CI coverage job (`.github/workflows/ci.yml` 'Run tests with coverage'): build dt-guard instrumented under
   `cargo llvm-cov --no-report`, export `DT_GUARD` to that binary, run the two bash suites, then produce the single
   lcov report from all profiles. Fail loudly if either suite fails or the instrumented binary is missing — never
   degrade to 'skipped'. Keep `RUSTFLAGS=--cfg coverage` semantics intact.
4. Single source of truth: coverage job must NOT hand-maintain a list of bash suites — derive it (glob
   `scripts/guards/*.test.sh` that reference `DT_GUARD`) or add a guard that fails on drift.

### Scope
- **Service(s)**: `crates/dt-guard` (tests only) + `.github/workflows/ci.yml` + possibly a drift guard/wrapper.
- **Schema**: No
- **Cross-cutting**: No (test + CI infra)

### Out of scope
- Refactoring `run()` bodies (separate devloop).
- Any change to guard policy content (YAML manifests, vocabularies, thresholds).

### Acceptance
- `./scripts/layer-all.sh` green.
- `cargo llvm-cov -p dt-guard` shows `counter_zero_init.rs` and `media_telemetry_deny.rs` above 85%, and every rule
  ID in the four previously-unfixtured guards executed at least once.
- Test reviewer's Gate-3 record lists each rule ID with its proof-of-trap mutation.

### Debate Decision
NOT NEEDED — no cross-service design decision; tests + CI wiring within existing guard-crate conventions.

---

## Cross-Boundary Classification

<!-- Implementer fills this at planning per ADR-0024 §6.2. Guard-crate ownership note (CLAUDE.md):
     crates/dt-guard/** is NOT a GSA today; splits by what the edit changes.
     TEST fixtures (crates/dt-guard/tests/**) are test domain — implementer is infrastructure, paired-with test.
     CI workflow machinery (.github/workflows/ci.yml) is infrastructure (Mine). -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/dt-guard/tests/dashboard_panels_e2e.rs` | Mine — test domain (infra impl, paired-test collab). Minor-judgment (fixture-trap design is domain-judgment, shared w/ test). | — |
| `crates/dt-guard/tests/metric_labels_e2e.rs` | Mine — test domain. Minor-judgment. | — |
| `crates/dt-guard/tests/alert_rules_e2e.rs` | Mine — test domain. Minor-judgment (loading-scaffold + inventory design). | — |
| `crates/dt-guard/tests/cross_boundary_scope_e2e.rs` | Mine — test domain. Minor-judgment (git-repo fixture harness). | — |
| `crates/dt-guard/tests/counter_zero_init_e2e.rs` | Mine — test domain. Minor-judgment. | — |
| `crates/dt-guard/tests/common/mod.rs` (new) | Mine — test domain (shared e2e helper reused by the five sibling suites: TempDir roots, `CARGO_BIN_EXE_dt-guard` subprocess plumbing, source-derived rule-ID SSoT scrape). Minor-judgment. | — |
| `.github/workflows/ci.yml` | Mine — infrastructure CI machinery. Minor-judgment. | — |
| `scripts/guards/list-coverage-suites.sh` (new) | Mine — infrastructure CI/SSoT machinery (suite-discovery extracted out of YAML per @operations O5). Minor-judgment. | — |
| `.gitignore` | Mine — infrastructure/CI machinery (O-B, added during review: ignore the coverage step's repo-root artifacts `llvm-cov-env.sh` / `coverage-suites.txt`). Mechanical (two ignore-pattern lines). | — |
| `scripts/guards/run-guards.test.sh` | Mine — infrastructure/CI machinery (deliverable 9: one `# coverage-exempt:` marker line on a non-driver guard self-test). Mechanical. | — |
| `scripts/guards/validate-frame-vectors.test.sh` | Mine — infrastructure/CI machinery (deliverable 9: `# coverage-exempt:` marker). Mechanical. | — |
| `scripts/guards/validate-slug-class-sync.test.sh` | Mine — infrastructure/CI machinery (deliverable 9: `# coverage-exempt:` marker). Mechanical. | — |
| `scripts/guards/validate-subdomain-regex-sync.test.sh` | Mine — infrastructure/CI machinery (deliverable 9: `# coverage-exempt:` marker). Mechanical. | — |
| `docs/runbooks/devloop-validation.md` | Not-mine — operations domain (new coverage-lane triage per @operations O6). Domain-judgment. | operations |
| `docs/devloop-outputs/2026-09-16-dt-guard-proof-of-trap-coverage/main.md` | Symmetric-exclusion (not drift scope) — planning record. | — |
| `docs/TODO.md` | Symmetric-exclusion — append-only, used ONLY if a rule genuinely cannot fire from a fixture. | — |

No `crates/dt-guard/src/**` edits — this is tests + CI only. No guard policy content (YAML manifests, vocabularies, thresholds) changes. No GSA path touched.

---

## Planning

### Authoritative rule-ID enumeration (read from each `run()` + emission sites, not any doc)

**`dashboard_panels` (13):** `panel_unit`, `hardcoded_datasource`, `rate_window`, `counter_misuse`, `gauge_misuse`, `histogram_misuse`, `metric_not_in_code`, `metric_not_in_catalog`, `lazy_ignore_reason`, `counter_window`, `expected_empty_description`, `expected_empty_set_empty`, `mixed_panel_attribution`.

**`metric_labels` (9):** `label_secret`, `label_pii`, `label_naming`, `literal_value_length`, `unbounded_value`, `lazy_pii_safe_reason`, `metric_name_length`, `metric_name_naming`, `parse_error`.

**`alert_rules` (7):** `runbook_url`, `severity`, `for_duration`, `annotation_hygiene`, `lazy_ignore_reason`, `rule_file_loading`, `inventory_expr_drift`.

**`cross_boundary_scope` (2):** `scope_drift_inbound`, `scope_drift_planned_untouched`.

**`counter_zero_init` four unasserted (of 10):** `slot_witness_cfg_test`, `catalog_but_no_entrypoint`, `non_literal_metric_name_in_entrypoint`, `empty_scan_no_counters`. (Already bash-asserted: `missing_zero_init`, `exempt_and_zero_init`, `malformed_exempt_reason`, `slot_witness_wildcard`, `entrypoint_not_called_from_main`, `scan_root_absent`.)

### Approach

All fixtures are **cargo integration tests** driving the real binary via `assert_cmd::Command::cargo_bin("dt-guard")` against a `TempDir` root (the `release_build_profile_e2e.rs` shape — inline `write()` into a temp tree, not on-disk fixture files, because these roots are multi-file: `metrics.rs` + catalog + dashboards + prometheus configs). Each test asserts on the STATUS/REASON token and the `VIOLATION:`/`ERROR:` line carrying the target `rule_id`. This exercises `run()` (the coverage gap) and is llvm-cov-visible under `cargo test`.

### Load-bearing per-guard design notes (the traps reviewers should scrutinize)

- **dashboard_panels — `expected_empty_set_empty` fires on ANY dashboard root whose catalogs carry zero `Expected-empty: yes` annotations.** So (a) its own negative fixture is trivially any dashboard + plain catalog; (b) EVERY other dashboard fixture that must isolate a different rule, and the clean fixture, MUST seed ≥1 `Expected-empty` annotation to keep this rule silent. Negatives only assert their target token, so extra fires are harmless there; the clean fixture must be fully green, so it seeds the annotation AND carries the marker.
- **dashboard_panels isolation tricks:** `rate_window`/`metric_not_in_catalog` are demoed on a **non-governed `table` panel** (not in `COUNTER_PANEL_TYPES`) so `counter_window` never owns the verdict; `gauge_misuse` uses `[$__rate_interval]` so `rate_window` stays silent; `histogram_misuse` uses a `_bucket` ref whose stripped base is the declared `histogram`.
- **alert_rules — `rule_file_loading` fires unless prometheus configs + kustomization all agree.** Per @observability, EVERY per-alert negative ships the full valid loading apparatus so `rule_file_loading` stays silent and the exact set is `{target}`. The **clean fixture** scaffolds the same apparatus AND a `docs/observability/alerts.md` inventory pair (so `inventory_pairs≥1`): `mc-alerts.yaml` (loadable name), both `prometheus.yml` configs with `rule_files: ["rules/*-alerts.yaml"]`, `infra/docker/prometheus/kustomization.yaml` `configMapGenerator` listing `rules/mc-alerts.yaml`, a real `docs/runbooks/*.md` target. `annotation_hygiene` negative uses non-allowlisted IPv4 `192.0.2.1` (RFC-5737). `inventory_expr_drift` negative adds an `alerts.md` whose `#### Name` restates a mismatched `expr`. **`rule_file_loading` negative** is aimed at a real policy cause (a `.yml`-suffixed loadable-name miss / glob non-match), NOT "config not found". **`runbook_url` gets TWO negatives (@security F3)** exercising both containment branches of `alert_rules.rs:274-288`: (i) repo-root escape `docs/runbooks/../../etc/passwd` (string only, no target created) → "cannot be resolved or escapes"; (ii) subdir escape `docs/runbooks/../decisions/<f>.md` WITH that file created in the temp tree → "escapes docs/runbooks/ via traversal or symlink". Both assert the SPECIFIC message substring, not just the `runbook_url` token (a missing-file typo also yields `runbook_url`).
- **cross_boundary_scope — `run()` reads git.** Its e2e helper `git init`s an isolated TempDir repo (never `/work`), commits a baseline, then writes an untracked `docs/devloop-outputs/x/main.md` with a `## Cross-Boundary Classification` table plus dirty/untracked files so the active-edit diff contains the main.md and the drift files. Inbound: an untracked file absent from the plan. Planned-untouched: a plan path with no diff entry. Clean: diff == plan.
- **counter_zero_init** e2e reuses the `counter-zero-init.test.sh` synthetic-root shape (one `mc-service`), targeting the four unasserted IDs: `slot_witness_cfg_test` (a `slot_*` witness inside a `#[cfg(test)]` block), `catalog_but_no_entrypoint` (catalog lists a `_total`, emitted, no marked entrypoint), `non_literal_metric_name_in_entrypoint` (a `counter!(name_var, …)` in a marked body), `empty_scan_no_counters` (a canonical service present but catalog/emit yield zero counters → `EMPTY_SCAN`).

### CI coverage job (deliverable 3 + 4)

Rework the `Run tests with coverage` step to the cargo-llvm-cov **external-binary** flow: `source <(cargo llvm-cov show-env --export-prefix)` → `cargo llvm-cov clean` → build the **instrumented** `dt-guard` → run the cargo tests (`cargo llvm-cov --no-report`) → export `DT_GUARD` to the instrumented binary and run the derived bash suites → `cargo llvm-cov report --lcov`. `RUSTFLAGS=--cfg coverage` stays in job env (cargo-llvm-cov merges its instrumentation flags onto it). **Fail loudly:** the step `set -euo pipefail`, asserts the instrumented binary is executable (else exit 1), and asserts ≥1 suite ran (positive control against a vacuous no-op).

**SSoT (deliverable 4):** the step does NOT hand-list suites — it globs `scripts/guards/*.test.sh` and runs each that greps `DT_GUARD` (today exactly `counter-zero-init.test.sh` + `media-telemetry-deny.test.sh`; the other four `*.test.sh` don't drive the binary and are correctly skipped). No second list exists to drift against.

### Files
1. `crates/dt-guard/tests/dashboard_panels_e2e.rs` (new) — 13 negatives + 1 clean.
2. `crates/dt-guard/tests/metric_labels_e2e.rs` (new) — 9 negatives + 1 clean.
3. `crates/dt-guard/tests/alert_rules_e2e.rs` (new) — 7 negatives + 1 clean.
4. `crates/dt-guard/tests/cross_boundary_scope_e2e.rs` (new) — 2 negatives + 1 clean.
5. `crates/dt-guard/tests/counter_zero_init_e2e.rs` (new) — 4 unasserted negatives (+ a clean control).
6. `.github/workflows/ci.yml` — rework the coverage step.

No `Cargo.toml` change: `assert_cmd` + `tempfile` are already dev-deps (used by existing `*_e2e.rs`). New top-level `tests/*.rs` need no registration (`test-registration` guard only governs `tests/<subdir>/*.rs`).

### Gate-1 reviewer constraints folded in

Independent enumerations from @paired-test (35), @observability (29 for its 3 guards + counter-zero-init 4) and @security all MATCH mine exactly. Adjustments adopted:

**Fixture shape decision — inline `TempDir` roots, NOT committed `tests/fixtures/` data.** These guards' `run()` take `repo_root`, so each fixture is a whole synthetic repo tree (`crates/*/src/observability/metrics.rs` + `docs/observability/metrics/*.md` + `infra/...`), built via inline `write()` into a `TempDir` (the `release_build_profile_e2e.rs` precedent named in my spawn prompt). This **resolves** rather than relies on @security S6 / @observability cross-cut #1: nothing is committed under `crates/**`/`infra/**`/`docs/observability/**` that the real-tree Layer-3 guards could discover — the only committed artifacts are the test `.rs` files (exempt via the `/tests/` segment in `is_scan_exempt`) and `ci.yml`. Deliberately-bad alert YAML / dashboard JSON exist only in `/tmp` at test runtime.

**Assertion granularity (@observability, @paired-test, @security S3-adjacent):** run each fixture with `--explain` and parse the machine-stable `policy=<guard>::<rule_id>` tokens (`common::explain::print_finding`) into a multiset; assert the **exact set**, not "contains X". Each guard gets a shared clean-base helper; each negative perturbs exactly ONE thing so the exact set is `{target}` where structurally possible. Documented structural co-fires (assert the set, which still reddens on the target's mutation): `catalog_but_no_entrypoint` co-fires with `missing_zero_init` (a catalogued+emitted+non-exempt counter with no entrypoint is also R\Z). Positive control on every clean fixture: assert the OK token names a non-vacuous scope — `dashboard-panels-clean-<N≥1>-files`, `metric-labels-clean-<N≥1>-files`, `alert-rules-clean-<F≥1>-files-<L≥1>-loadable-covered-<P≥1>-inventory-pairs` (P≥1 ⇒ clean alert fixture SHIPS an `alerts.md` inventory pair) — never a `no-dir`/`no-files` token.

**bail!() vs findings (@paired-test #3):** `empty_scan_no_counters` and `scan_root_absent` surface via `anyhow::bail!` → `STATUS=FAIL REASON=<token>`, not `VIOLATION:` lines. Their fixtures assert on the STATUS/REASON token, not on a findings line. (My e2e reads combined stdout+stderr, so this is handled uniformly.)

**Per-guard scaffolding (@observability §3):** dashboard misuse rules require a synthetic `metrics.rs` declaring the metric (else `metric_not_in_code` fires first and `continue`s); each of the 12 non-`expected_empty_set_empty` dashboard fixtures seeds ≥1 `Expected-empty: yes` catalog annotation to keep that vacuity control silent; `metric_not_in_code`/`metric_not_in_catalog` fixtures each trip exactly one direction; every per-alert negative ships the full valid loading apparatus so `rule_file_loading` stays silent, and the `rule_file_loading` negative is aimed at a real policy cause (a `.yml` loadable-name miss / glob non-match), NOT "config not found"; `annotation_hygiene` fixture reaches the `else` branch (valid runbook/severity/for); no fixture relies on the YAML-parse-error bail path.

**@security CI constraints (S1/S2/S3/S7):**
- S1: coverage job adds an explicit precondition that fails non-zero unless `DT_GUARD` is non-empty, `-x`, AND not equal to `$PWD/target/release/dt-guard` (the uninstrumented fallback). Not weakening the wrapper's `${VAR:-default}` — asserting at the call site.
- S2: after the suites run, assert profraw actually landed (fail if none) so a non-propagated `LLVM_PROFILE_FILE` (0% → green) is a loud failure.
- S3: no `continue-on-error` / trailing `|| true` on the step; negative-exit is expected per-invocation and handled inside each bash suite (which already asserts its own pass/fail and exits accordingly), the step runs `set -euo pipefail`.
- S4/S6: NOT adding any root/scope override to `_dt_guard_wrapper.sh` and NOT adding any fixture-path carve-out to a security guard or `test_code_filter.rs`. Fixtures use `--root` (production flag) against throwaway roots.
- S5: fixture string content uses reserved/published placeholders only — `192.0.2.1` (RFC-5737, verified absent from `IPV4_ALLOWLIST`) for `annotation_hygiene`; `https://example.invalid/...` and `docs/runbooks/../../etc/passwd` (string only, no symlink) for `runbook_url`; `AKIAIOSFODNN7EXAMPLE` / obviously-filler `Bearer` token if a non-IPv4 hygiene case is added. Each such fixture carries a one-line header comment marking the value a published documentation placeholder.
- S7: add workflow-level `permissions:\n  contents: read` to `ci.yml` (Lead-approved adjacent one-liner). The false-parity comment in `ci-client.yml:38-40` becomes TRUE once ci.yml gains the block, so the minimal move is to add the block and leave the comment — no second-file edit needed.

**CI job placement + SSoT direction (@security S7-half, @dry-reviewer lens):** deliverable 3 mandates ONE lcov merged from cargo + bash-suite profiles, which requires ONE job sharing a profile dir — so the glob-discovered suite step stays in the `coverage` job. Justification: (a) the globbed scripts are all reviewed in-tree `scripts/guards/*.test.sh`, same trust surface as the rest of the pipeline CI already runs; (b) `CODECOV_TOKEN` is scoped to the separate `Upload coverage to Codecov` step (`with: token:`), NOT present in the environment while the suites execute. SSoT: a single derivation point (`grep -lE 'DT_GUARD' scripts/guards/*.test.sh`) with a ≥1 positive control — there is no second list to drift against, so the two-way-drift requirement is satisfied by construction (a suite renamed within the `*.test.sh`+DT_GUARD pattern is still found; one renamed out of it is no longer a guard self-test and correctly drops).

### GSA / ownership note (cross_boundary_scope)

The `cross_boundary_scope` fixture builds its `## Cross-Boundary Classification` table and any ownership YAML INSIDE the throwaway git root — it does NOT add a row to the real GSA mirror `scripts/guards/simple/cross-boundary-ownership.yaml` (`gsa-sync` rejects stray keys). `crates/dt-guard/**` is not a GSA today and gets no key. No crypto/`jwt.rs`/`proto/**`/`migrations/**` path is touched.

### Gate-2 build-in conditions (final — supersede the sharper points above)

Consolidated binding conditions from @operations (O1-O9), @security (F1-F4), @dry-reviewer, @observability, and @team-lead (1-6). These are Gate-2/3 verdict conditions, built in now.

**Rule-ID SSoT — decision on Lead #6 = option (b), SOURCE-DERIVED (no `src/**` edit).** Per guard, a test reads the guard's own `src/<guard>.rs` (via `CARGO_MANIFEST_DIR`), regex-extracts every `pub const \w+_RULE_ID: &str = "([^"]+)"`, and asserts that source set EQUALS the set of rule IDs the fixture catalog targets — **red in BOTH directions** (a new `*_RULE_ID` const with no fixture → red; a fixtured ID no longer in source → red). This is strictly stronger than a `pub const ALL_RULE_IDS` in `src/**` (which itself could omit a rule and needs the same scrape to verify), so I do NOT edit `src/**`. For `counter_zero_init` the scrape covers all 10 consts, partitioned into `{4 cargo-fixtured} ∪ {6 named-bash-covered}` (the 6 listed in-test with a comment); an 11th unclassified const → red. @paired-test to sanity-check the regex against the const spellings (all confirmed `pub const NAME_RULE_ID: &str = "..."`).

**Binary resolution — `CARGO_BIN_EXE_dt-guard` subprocess, NOT `assert_cmd::cargo_bin` (@dry #3, @observability, Lead #4).** Under `cargo llvm-cov` the cargo tests run in a separate target dir where `assert_cmd::cargo_bin` mis-resolves; `CARGO_BIN_EXE_dt-guard` (set by cargo at test-compile to the instrumented binary, with a `DT_GUARD` override fallback) is the correct route — mirror the `dt_guard_bin()` helper at `tests/credential_leak_key_custody_fixtures.rs:336-345` (added in `c5ca799`). Must be a **subprocess** (not in-process `run()`): `println!`-written `policy=` tokens are libtest-captured and unreadable in-process, and in-process `run()` returns only a violation *count* (@observability). The child inherits `LLVM_PROFILE_FILE` (`%p` pattern) so coverage is still collected.

**Fixture-root hygiene (@observability, Lead #5):** assert each `TempDir` root path contains NO `/tests/` or `/fixtures/` segment (else `metric_labels` `is_scan_exempt` silently greens the suite); positive control on the clean `metric_labels` fixture that a real label was actually scanned (`metric-labels-clean-<N≥1>-files`). `// Invariant:` blocks sit ADJACENT to each fixture's tree-construction code, not collected in one table (media_telemetry_deny precedent). Base the `TempDir` under the session scratchpad if a failed fixture needs post-mortem artifacts.

**Assert-absent the must-not-fire rules (@paired-test (c)) — targeted, not full exact-set-equality on whole-catalog rules.** Minimum assert-absent set: `rate_window`↔`counter_window`, `gauge_misuse`→`rate_window` absent, `histogram_misuse`→its siblings absent, `metric_not_in_code`↔`metric_not_in_catalog`, `expected_empty_set_empty` absent in every non-target dashboard fixture, and the single-fire isolation pairs/triples @paired-test named (`metric_name_length`↔`metric_name_naming`, the `label_*` triple, the `*_misuse` triple, `literal_value_length`↔`unbounded_value`, `expected_empty_description`↔`mixed_panel_attribution`, both `lazy_ignore_reason` with an otherwise-VALID ignore directive).

**cross_boundary_scope git hermeticity (@paired-test (a)):** the isolated repo is created with `git -c user.name=… -c user.email=…`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null`, `commit.gpgsign=false`, and a pinned default branch, so it inherits no ambient config and does not flake in CI. Per-guard shared invocation plumbing (root construction + cwd + binary invocation) is one helper reused by clean + negatives, so the negatives serve as the positive control for the clean fixture's OK assertion.

**runbook_url two-branch coverage (@security F3):** two negatives, both asserting the SPECIFIC containment message (not just the `runbook_url` token) — repo-root escape and runbooks-subdir escape (the latter with the target file created inside the temp tree).

**CI coverage step — final shape (@operations O1-O4, @security F1-F2, Lead 1-2):**
- Write `cargo llvm-cov show-env --export-prefix` to a FILE, assert it non-empty, then `source` it (a `source <(…)` process-substitution swallows failure under `set -e`/`pipefail` → uninstrumented build, silently). After sourcing, assert `CARGO_ENCODED_RUSTFLAGS` carries `--cfg`+`coverage` (cargo silently prefers it over `RUSTFLAGS`; dropping `--cfg coverage` un-ignores three `#[cfg_attr(coverage, ignore)]` tests in `ac-service`).
- Derive the instrumented `DT_GUARD` path from the `CARGO_TARGET_DIR` `show-env` exports — do NOT hardcode `target/llvm-cov-target/...`.
- Precondition (fail non-zero): `DT_GUARD` non-empty, `-x`, and `!= $PWD/target/release/dt-guard`.
- **Profraw DELTA assert (the load-bearing one — O1/F1):** snapshot the profraw set/count in the profile dir IMMEDIATELY BEFORE the bash-suite loop and require STRICT growth after each suite (per-suite, so one silently-uninstrumented suite can't hide behind the other). A post-run "≥1 profraw exists" is vacuous — the cargo test run already deposited many.
- Order: `cargo llvm-cov clean --workspace` → write/source show-env → build instrumented → `cargo llvm-cov --no-report` (tests) → bash suites → `cargo llvm-cov report --lcov`. Identical **dev** profile across build/test/report (measure the instrumented-dt-guard delta over 92+19 cases; escalate ALL THREE to `--release` only if timing is close to the 30-min cap, and raise `timeout-minutes` deliberately if needed).
- Loop accumulates exit status (`rc=0; … || rc=1; … exit $rc`) so every suite's failure reaches the job (last-suite exit status would otherwise mask an earlier failure — @dry).

**SSoT discovery — extracted script + bounded two-way drift check (@operations O5/O6, @dry, @security F2, Lead #3). PENDING reviewer confirm on the marker-universe scope.**
- Extract discovery into a committed `scripts/guards/list-coverage-suites.sh` (linted/testable/locally-runnable, not inline YAML) that prints the selected suites.
- **SELECTION is recursive over all of `scripts/`** (`find scripts -name '*.test.sh' | xargs grep -lE 'DT_GUARD'`), per @dry — a dt-guard-driving suite landing ANYWHERE is selected and run, so none is silently missed by a directory assumption. (Verified: only the 2 guard suites reference `DT_GUARD` with a real invocation today; three other `scripts/**` mentions are comments/preconditions the grep correctly excludes.)
- **DRIFT MARKER universe is bounded to `scripts/guards/*.test.sh`**, NOT all of `scripts/**`. Every `scripts/guards/*.test.sh` not selected must carry a `# coverage-exempt: <reason>` marker (reason floored via `is_lazy_reason`); one neither selected nor exempt FAILS. That is FOUR marker lines (`run-guards.test.sh`, `validate-frame-vectors.test.sh`, `validate-slug-class-sync.test.sh`, `validate-subdomain-regex-sync.test.sh`), all in the guard-self-test directory — NOT ~20 markers across unrelated cross-domain `scripts/**/*.test.sh`, which would be disproportionate scope creep into other specialists' files. **This is why I split selection (recursive, catches drivers anywhere) from the marker discipline (bounded to where guard self-tests belong).** Residual gap named explicitly: a suite that both moves OUT of `scripts/guards/` AND drops the `DT_GUARD` var would evade the marker check — but it is still not silently missed, because recursive selection only misses it if it ALSO stops driving via `DT_GUARD`, and a binary invocation by any other means inside a `*.test.sh` is itself the anti-pattern; if reviewers want that closed too I'll add a second grep in the discovery script for non-`DT_GUARD` `dt-guard` invocations. **@security / @dry / @team-lead: confirm the bounded-to-`scripts/guards/` marker universe, or direct me to mark all `scripts/**/*.test.sh`.**
- Pin the selected-count floor at **2** with a message naming both suites (a silent 2→1 drop is the half-vacuous run the floor exists to catch; raising a pinned int when a 3rd lands is a deliberate one-line edit).
- Comment that the discovered set must stay hermetic: the coverage job's checkout is deliberately shallow (`ci.yml`, no `fetch-depth: 0`), so a future suite needing a git base ref would be pulled in and fail confusingly.

**S7 / permissions (Lead-approved):** add workflow-level `permissions:\n  contents: read` to `ci.yml`; the now-true `ci-client.yml:38-40` parity comment needs no edit.

**Operations runbook (O6):** add the coverage lane to `docs/runbooks/devloop-validation.md` — local repro + triage for the three new failure shapes (suite red under instrumentation but green in Layer 3; the profraw-delta "no coverage produced" assert; the discovery exempt-marker drift), and whether `coverage` is a required status check. Drafted by me (I have the failure-mode knowledge), **owned/reviewed by @operations** (runbooks are ops domain) — coordinating placement with ops.

### Files (updated)
1-6 as above, plus:
7. `scripts/guards/list-coverage-suites.sh` (new) — SSoT suite discovery + two-way exempt-marker check.
8. `docs/runbooks/devloop-validation.md` — new coverage lane (ops-owned).
9. Four existing non-driver `scripts/guards/*.test.sh` — add one `# coverage-exempt: <reason>` line each (`run-guards.test.sh`, `validate-frame-vectors.test.sh`, `validate-slug-class-sync.test.sh`, `validate-subdomain-regex-sync.test.sh`). Marker universe bounded to `scripts/guards/` (pending reviewer confirm — see SSoT bullet).

---

## Accepted Deferrals

- (none yet)

## Review Adjustments (Gate 2/3)

- **O-A (ci.yml cfg assert), RESOLVED.** The plan's build-in "assert CARGO_ENCODED_RUSTFLAGS contains `--cfg`+`coverage`" was based on a wrong mental model: cargo-llvm-cov 0.8.7 instruments via a `RUSTC_WRAPPER` and never exports `CARGO_ENCODED_RUSTFLAGS`; it injects `--cfg=coverage` itself. Empirically verified (`show-env` byte-identical with/without `RUSTFLAGS`). Replaced the single (fail-open for the uninstrumented-build mode) grep with three TIGHT fail-closed asserts — `RUSTC_WRAPPER` set / `-Cinstrument-coverage` / `cfg=coverage` — each guarding a distinct silent-0%-coverage mode. Redundant job-env `RUSTFLAGS: --cfg coverage` kept, annotated (observability: zero blast radius). Signed off by @security, @operations, @observability.
- **O-B (gitignore), RESOLVED.** Coverage-step repo-root artifacts `llvm-cov-env.sh` + `coverage-suites.txt` added to `.gitignore`.
- **Coverage-exempt reason bar NARROWED (owner-approved), from plan wording "floored via `is_lazy_reason`" → LENGTH FLOOR ONLY** (`REASON_MIN_LEN=10`, SSoT `crates/dt-guard/src/ignore.rs::MIN_REASON_LEN`). Reason: the bash `is_lazy_reason` re-encoding was an unanchored cross-language fork of ignore.rs's `\b`-boundaried lazy-vocabulary (portable ERE can't express `\b`; it had already drifted). @dry-reviewer flagged (FIXED), @operations approved as marker-discipline owner. Short lazy tokens still rejected by the ≥10 floor; SELECTION still runs any real DT_GUARD driver regardless of marker, so no coverage-hole risk.
- **common/mod.rs dedup** (@code-reviewer): extracted a private `spawn()` helper shared by `run_guard`/`run_guard_plain`. FIXED.
