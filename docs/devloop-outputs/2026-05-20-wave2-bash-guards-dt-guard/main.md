# Devloop Output: Wave 2 — Bash Guards Migrate to dt-guard Subcommands

**Date**: 2026-05-20
**Task**: Migrate ~25 pure-bash guards to `dt-guard` subcommands per ADR-0034 §Decision (R-63, task #45)
**Specialist**: infrastructure
**Mode**: Full (Agent Teams)
**Branch**: `feature/browser-client-join-task45`
**Duration**: ~3 calendar days (2026-05-20 setup → 2026-05-23 Gate 3 close); active driving spread across multiple wall-clock sessions per LLM rate-limit incidents

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `abf844e1037b93fb5361d2e8f341f3b16fed0f86` |
| Branch | `feature/browser-client-join-task45` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@wave2-bash-guards-dt-guard` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@wave2-bash-guards-dt-guard` (paired) |
| Test | `test@wave2-bash-guards-dt-guard` (paired) |
| Observability | `observability@wave2-bash-guards-dt-guard` |
| Code Quality | `code-reviewer@wave2-bash-guards-dt-guard` |
| DRY | `dry-reviewer@wave2-bash-guards-dt-guard` |
| Operations | `operations@wave2-bash-guards-dt-guard` (paired) |
| Semantic Guard | `semantic-guard@wave2-bash-guards-dt-guard` |
| Paired Client | `paired-client@wave2-bash-guards-dt-guard` (paired-only, non-mandatory slot) |

### Plan Confirmation Tracker (Gate 1)

| Reviewer | Status | Notes |
|----------|--------|-------|
| Security | confirmed | Conditional lifted after F1/F2/F3 folded. PII vocab promoted to `common/pii_vocabulary.rs` with CATEGORY_A widening (`pwd`/`cred`/`bearer`/`auth_code`) + CATEGORY_B widening (5 PII identifiers). `ExportsValue` untagged enum + recursive leaf walk; Check A first-level only, Check B all leaf `./` strings. `instrument_skip_all` consumes CATEGORY_A. Q1 full HYGIENE_PATTERNS confirmed. Q2 syn-based range filter → Wave 3 Accepted Deferral. |
| Test | confirmed | All 6 asks resolved by revision (3-bucket predicate insertion/deletion/edit-existing for tightening #1, 9-case userstory suite incl. case-4 deletion mirror, 8-case classification, 8-case gsa_sync, check-5a/5b/5c split, tempfile+real-git commitment). One non-blocking clarification: confirm Mine-row skip is explicitly fixtured in `cross_boundary_classification.rs` (currently named cases 7=template-placeholder, 8=header/separator parser — Mine-row may need a 9th fixture row). Accept at code-review time either way. |
| Observability | confirmed | All 6 items folded: Q1 PII vocab consolidation (canonical home + strict tightening, ~23 new tokens caught); Q2 `instrument-skip-all` Check 2 → FAIL with distinct REASON token `rust-instrument-sensitive-param-no-skip-all`; S1 6 fixture pairs for `metric-coverage`; S2 pure `starts_with` for histogram-buckets; S3 `^[a-z][a-z0-9_]+$` regex preserved; S4 ts-name-guard length-cap HARD FAIL. ADR compliance verified (ADR-0011 / ADR-0029 / ADR-0032 / ADR-0034 §6 / §7). |
| Code Quality | confirmed | All 6 items folded: `--explain` MANDATORY + single-SoT `common::explain::print_finding`; PII vocab Pattern-B option (a) with 4 consumers; `resolve_cited_path` reuse in both cross-boundary modules; STRICT_EXPORTS_MAP reframed as in-contract (3-line wrapper + `_dt_guard_wrapper.sh` `"$@"` forwarding amendment); sprawl-heuristic TODO entry at close-out; `metric_macros::*` reuse plan-time confirmed. No upgrade challenges. ADR-0002/0019/0024/0034 compliance verified. |
| DRY | confirmed | Non-blocking nudge: `kustomize_glob` is single-consumer (only `kustomize.rs`) — prefer inline `mod tools` or sibling `kustomize_tools.rs` over `common/` siting unless second consumer planned. HYGIENE cross-stack collapse modeled correctly; Wave 1 kernel reuse intact; `rust_log_secrets::SECRET_PATTERNS` non-consolidation rationale (identifier vocabulary axis) should land as a module-header comment. TEST_PATH_EXCLUDES correctly defers per docs/TODO.md Wave-2 #6 rule-of-three-with-asymmetry. |
| Operations | confirmed | Two non-blocking polish items sent to implementer: `gsa_sync.rs` mirror-comment carryover + REASON-token catalog grouped by failure-shape class. |
| Semantic Guard | confirmed | Both BLOCKING items folded: Q1 [credential-leak] resolved via new `common::explain::print_secret_finding(&SecretFinding)` helper emitting `pattern=<name>` instead of `matched=<span>`; Wave-1 `alert_rules::annotation_hygiene` amended; fixture-level redaction assertion shape committed. Q2 [error-context-preservation] resolved via `.with_context()` + stable `emit_fail("<token>")` before each `return Err()`; "context names files/ops only, never contents" constraint pinned. Non-blocking [metrics-path-completeness]: `metric_coverage`/`histogram_buckets` macro scan set pinned to bash-parity (`describe_*` excluded). |
| Paired Client | confirmed | All 4 asks resolved by revision: F1 `--search-path` join in `ts_dev_trust.rs`; F2 `--diff-filter=D` exclusive pin in `ts-no-test-removal`; F3 `ExportsValue` untagged enum + recursive leaf-string walk (folded with security F2); F4 wrapper shim exact-string `"1"` semantics. No upgrade challenges on classification table. |

---

## Task Overview

### Objective

Land subcommands for ~25 currently-pure-bash guards in `dt-guard`, in four thematic groups:

- (a) **TS guards** (6): `no-secrets-in-ts`, `no-pii-in-logs-ts`, `no-test-removal-ts`, `name-guard-dt-client`, `no-dev-trust-path-in-prod-bundle`, `exports-map-closed`
- (b) **secret/PII shell guards** (4): `no-hardcoded-secrets`, `no-pii-in-logs`, `no-secrets-in-logs`, `instrument-skip-all`
- (c) **test-discipline guards** (6): `test-coverage`, `test-registration`, `test-rigidity`, `validate-cross-boundary-classification`, `validate-cross-boundary-scope`, `validate-gsa-sync`
- (d) **structure/metadata guards** (7): `api-version-check`, `validate-env-config`, `validate-kustomize`, `validate-knowledge-index`, `validate-metric-coverage`, `validate-histogram-buckets`, `validate-todo-tracking`

Each becomes a `dt-guard <subcommand>` invocation in a ≤5-line shell wrapper using the existing `_dt_guard_wrapper.sh` prelude.

Reuse `dt_guard::common::secret_patterns::HYGIENE_PATTERNS` (Wave 1) for cross-stack secret-pattern consolidation in the (b) group.

**Required tightenings during the port**:
1. ~~`validate-cross-boundary-scope`'s `docs/user-stories/*.md` row-level tightening~~ — **ROLLED BACK** per @team-lead 2026-05-21 redirect. Wave-2 preserves bash whole-file exemption verbatim; row-level tracked under `docs/TODO.md` §Polyglot Pipeline Follow-ups for future focused work. See §Tightenings during port for context.
2. `scripts/guards/common.sh::get_*_files` extension filter is regex-interpreted (`.` matches any char in `.test.ts` → false positives); Rust port uses literal-suffix matching per the existing `docs/TODO.md` entry on the same helper.

### Scope
- **Service(s)**: `crates/dt-guard/` (new subcommands) + `scripts/guards/simple/**` (wrappers)
- **Schema**: None
- **Cross-cutting**: Yes — Lead workflow (GSA-classification gate), client (TS guards), security (PII/secrets), test (test-discipline), operations (GSA gates Lead's workflow)

### Debate Decision

NOT NEEDED — ADR-0034 (Accepted 2026-05-18) is the governing decision. This devloop executes §Decision "every shell guard becomes a wrapper" Wave 2 strictly. No design questions remain.

---

## Cross-Boundary Classification

All paths below are NOT in Guarded Shared Areas (GSA) per ADR-0024 §6.4 — `crates/dt-guard/**`, `scripts/guards/simple/**`, `clippy.toml`, `docs/TODO.md`, and `docs/devloop-outputs/**` are not in the §6.4 enumerated list. `scripts/guards/common.sh` is operations-owned per the SKILL ownership model (it gates Lead's GSA workflow); we are NOT touching it in this devloop (the regex-interpreted-extension-filter fix lives behind the helper boundary — see Planning §Tightenings).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/dt-guard/src/lib.rs` | Mine | — |
| `crates/dt-guard/src/main.rs` | Mine | — |
| `crates/dt-guard/src/ts_secrets.rs` | Mine | — |
| `crates/dt-guard/src/ts_pii.rs` | Mine | — |
| `crates/dt-guard/src/ts_test_removal.rs` | Mine | — |
| `crates/dt-guard/src/ts_metric_naming.rs` | Mine | — |
| `crates/dt-guard/src/ts_dev_trust.rs` | Mine | — |
| `crates/dt-guard/src/ts_exports_map.rs` | Mine | — |
| `crates/dt-guard/src/rust_secrets.rs` | Mine | — |
| `crates/dt-guard/src/rust_pii.rs` | Mine | — |
| `crates/dt-guard/src/rust_log_secrets.rs` | Mine | — |
| `crates/dt-guard/src/instrument_skip_all.rs` | Mine | — |
| `crates/dt-guard/src/test_coverage.rs` | Mine | — |
| `crates/dt-guard/src/test_registration.rs` | Mine | — |
| `crates/dt-guard/src/test_rigidity.rs` | Mine | — |
| `crates/dt-guard/src/cross_boundary_classification.rs` | Mine | — |
| `crates/dt-guard/src/cross_boundary_scope.rs` | Mine | — |
| `crates/dt-guard/src/gsa_sync.rs` | Mine | — |
| `crates/dt-guard/src/api_version.rs` | Mine | — |
| `crates/dt-guard/src/env_config.rs` | Mine | — |
| `crates/dt-guard/src/kustomize.rs` | Mine | — |
| `crates/dt-guard/src/kustomize_tools.rs` | Mine | — |
| `crates/dt-guard/src/secret_patterns.rs` | Mine | — |
| `crates/dt-guard/src/knowledge_index.rs` | Mine | — |
| `crates/dt-guard/src/metric_coverage.rs` | Mine | — |
| `crates/dt-guard/src/histogram_buckets.rs` | Mine | — |
| `crates/dt-guard/src/todo_tracking.rs` | Mine | — |
| `crates/dt-guard/src/common/git_changes.rs` | Mine | — |
| `crates/dt-guard/src/common/pii_vocabulary.rs` | Mine | — |
| `crates/dt-guard/src/common/test_code_filter.rs` | Mine | — |
| `crates/dt-guard/src/common/explain.rs` | Mine | — |
| `crates/dt-guard/src/metric_labels.rs` | Mine | — |
| `crates/dt-guard/src/alert_rules.rs` | Mine | — |
| `crates/dt-guard/src/common/markdown_table.rs` | Mine | — |
| `crates/dt-guard/src/common/manifest_match.rs` | Mine | — |
| `crates/dt-guard/src/common/mod.rs` | Mine | — |
| `scripts/guards/simple/ts/no-secrets-in-ts.sh` | Mine | — |
| `scripts/guards/simple/ts/no-pii-in-logs-ts.sh` | Mine | — |
| `scripts/guards/simple/ts/no-test-removal-ts.sh` | Mine | — |
| `scripts/guards/simple/ts/name-guard-dt-client.sh` | Mine | — |
| `scripts/guards/simple/ts/no-dev-trust-path-in-prod-bundle.sh` | Mine | — |
| `scripts/guards/simple/ts/exports-map-closed.sh` | Mine | — |
| `scripts/guards/simple/no-hardcoded-secrets.sh` | Mine | — |
| `scripts/guards/simple/no-pii-in-logs.sh` | Mine | — |
| `scripts/guards/simple/no-secrets-in-logs.sh` | Mine | — |
| `scripts/guards/simple/instrument-skip-all.sh` | Mine | — |
| `scripts/guards/simple/test-coverage.sh` | Mine | — |
| `scripts/guards/simple/test-registration.sh` | Mine | — |
| `scripts/guards/simple/test-rigidity.sh` | Mine | — |
| `scripts/guards/simple/validate-cross-boundary-classification.sh` | Mine | — |
| `scripts/guards/simple/validate-cross-boundary-scope.sh` | Mine | — |
| `scripts/guards/simple/validate-gsa-sync.sh` | Mine | — |
| `scripts/guards/simple/api-version-check.sh` | Mine | — |
| `scripts/guards/simple/validate-env-config.sh` | Mine | — |
| `scripts/guards/simple/validate-kustomize.sh` | Mine | — |
| `scripts/guards/simple/validate-knowledge-index.sh` | Mine | — |
| `scripts/guards/simple/validate-metric-coverage.sh` | Mine | — |
| `scripts/guards/simple/validate-histogram-buckets.sh` | Mine | — |
| `scripts/guards/simple/validate-todo-tracking.sh` | Mine | — |
| `scripts/guards/simple/_dt_guard_wrapper.sh` | Mine | — |
| `scripts/guards/simple/validate-alert-rules.sh` | Mine | — |
| `scripts/guards/simple/validate-application-metrics.sh` | Mine | — |
| `scripts/guards/simple/validate-infrastructure-metrics.sh` | Mine | — |
| `scripts/guards/simple/validate-metric-labels.sh` | Mine | — |
| `scripts/guards/simple/validate-dashboard-panels.sh` | Mine | — |
| `scripts/guards/simple/grafana-datasources.sh` | Mine | — |
| `scripts/guards/simple/validate-doc-citations-no-line-numbers.sh` | Mine | — |
| `scripts/guards/simple/validate-doc-citations-symbol-resolves.sh` | Mine | — |
| `crates/mh-service/tests/errors_grpc_metrics_integration.rs` | Mine | — |
| `crates/mh-service/tests/mc_notifications_metric_integration.rs` | Mine | — |
| `crates/mh-service/tests/token_refresh_integration.rs` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-05-20-wave2-bash-guards-dt-guard/main.md` | Mine | — |

Notes:
- `docs/TODO.md` is on the symmetric exempt list in `validate-cross-boundary-scope` but is still listed explicitly for surface visibility (close-out updates the §Polyglot Pipeline Follow-ups row-level + literal-suffix entries).
- `clippy.toml` is NOT planned to be modified — Wave 1 already installed the canonical `disallowed_methods` entry. If a new canonical-home module is needed, it stays inside `crates/dt-guard/src/`.
- No changes to `scripts/guards/common.sh`. The literal-suffix tightening (TODO §Polyglot Pipeline Follow-ups, owner=infrastructure on the canonical helper) is fulfilled inside the Rust port for the guards we are flipping — bash callers of `get_*_files` are not changed. Common-helper-level cleanup remains its own future devloop.
- No new bash wrappers — only the 23 existing shell files are rewritten to ≤5-line wrappers.

---

## Planning

### Module decomposition

Per ADR-0034 §Neutral discipline (single-concern names, no `util`/`common`/`validate` catch-alls) and §When-to-Revisit (≥10-subcommand sprawl heuristic — Wave 2 lands ~23 subcommands, total ~31 post-Wave-2; the heuristic will fire as a re-debate trigger, surfaced here for @code-reviewer's lens — NOT a tripwire, not a blocker on this PR), modules:

**Group (a) TS guards** (6 modules — each a single concern; no `ts_guards.rs` aggregator):
- `src/ts_secrets.rs` — Subcommand `ts-no-secrets`. 5 checks: secret-assignment literal, API-key prefix, conn-string credentials, Authorization header, JWT shape. Drops bare `token` from check 1 (TS tightening retained). Per-language file extensions: `.ts`, `.tsx`, `.svelte` (production-only — TEST_PATH_EXCLUDES inline filter port).
- `src/ts_pii.rs` — Subcommand `ts-no-pii-in-logs`. 2 blocking + 1 warning checks. **Consumes `common::pii_vocabulary::PII_TOKENS_CATEGORY_B`** for the PII identifier set (per @code-reviewer item 2 Pattern-B trigger — three consumers in `dt-guard` of overlapping PII catalogs warrant a canonical home). Same SoT as `rust_pii.rs`. The CLOSED-LIST framing from bash today (any expansion required observability sign-off) is now structurally enforced by the `pub(crate)` consumption — additions to CATEGORY_B happen exactly once at the canonical home with the same observability sign-off gate. `.ts` + `.svelte` only. `// pii-safe:` opt-out, hashed/REDACTED/masked allowances preserved.
- `src/ts_test_removal.rs` — Subcommand `ts-no-test-removal`. v1: file-deletion-only check (block-count heuristic stays deferred per main.md §Tech Debt #2 of task #37). Uses the local literal-suffix collector pattern (this is the canonical Rust port; literal-suffix matching is the *default* in the Rust common helper). **Paired-client F2 commitment**: the underlying git-diff call MUST pin `--diff-filter=D` exclusively — NO `-M`/`-C` rename detection. A rename appears as delete+add by design, and the same-basename/same-package match policy from bash today catches the rename case naturally. Adding `-M`/`-C` would silently change classification semantics. The `common::git_changes::get_deleted_files` helper documents the explicit `--diff-filter=D`-only contract; if a future caller needs rename detection, it adds a separate helper.
- `src/ts_metric_naming.rs` — Subcommand `ts-name-guard-dt-client`. R-26 `^dt_client_[a-z][a-z0-9_]{0,53}$` regex against `.createCounter/.createHistogram/.createUpDownCounter/.createGauge/.createObservableCounter/.createObservableGauge/.createObservableUpDownCounter` calls. POSITIVE include-list scope (sdk-core + web-app `src/`). Sdk-svelte deferred — same as bash today. **Per @observability S4**: `{0,53}` upper bound is a HARD FAIL, not advisory. Total cap is 64 chars (`dt_client_` prefix is 10 + 53 body + 1 final = 64). Consistent with `metric_labels.rs:MAX_LITERAL_VALUE_LENGTH = 64` (Wave 1 observability-catalog constant). REASON token on violation: `ts-metric-name-violation` (rule-class includes both regex non-match AND length-cap exceedance — both surface as the same violation since the regex itself encodes the cap).
- `src/ts_dev_trust.rs` — Subcommand `ts-no-dev-trust-path-in-prod-bundle`. R-14. Four-state machine: (1) sdk-core absent → ok; (2) sdk-core present + canonical test missing → fail (forcing function); (3) both present → ok + state-4 belt-and-suspenders; (4) `dist/` exists → grep `serverCertificateHashes`. **Paired-client F1 commitment** (carries past paired-security F2 from task #37): the subcommand takes `--search-path` (defaults to `--root` if absent) and resolves `SDK_CORE_DIR` / `CANONICAL_TEST` / `SDK_CORE_DIST` by JOINING under the search path — NOT bare relative. The bash port previously regressed to bare relative and silently fired state 1 when invoked outside the repo root; the Rust port pins the join explicitly. Path-safety boundary: the joined paths stay descriptive (no canonicalization) — we're testing existence + grepping content, not crossing a trust boundary.
- `src/ts_exports_map.rs` — Subcommand `ts-exports-map-closed`. 3 checks: forbidden keys, forbidden values, missing-exports (SOFT, promotable via `--strict`). Owns the `jq`-equivalent walk via `serde_json` typed Deserialize. **Folded paired-client F3 + security F2** (security F2 expanded with subpath-key semantic):
  - `ExportsValue` modelled as untagged enum — `enum ExportsValue { String(String), Object(BTreeMap<String, ExportsValue>) }`. Walker recurses through every conditional sub-object (`import` / `require` / `types` / `default` / `node` / `browser` / `worker` / etc.) collecting all leaf strings — parity with bash's `[.. | strings]`.
  - `#[serde(deny_unknown_fields)]` NOT applied at the conditional-keys layer (open vocabulary per npm spec). Top level: use `serde_json::Value` and reach in for `.exports` per @security F2 simplification — avoids needing to enumerate every package.json field.
  - **Check A (forbidden-keys regex) applies ONLY to first-level subpath keys** (e.g. `"./test/foo"`, `"./internal"`) — NOT to nested conditional sub-keys (e.g. `"import"`, `"require"`, `"types"`). Bash `walk_exports` emits `key|value` pairs where `key` is only the first-level subpath key; the Rust walker pins the same semantic. Concretely: the recursion descends from each first-level subpath key, but the `forbidden_key_regex` is checked ONLY at the top-of-recursion (the first-level subpath key); descended conditional keys are walked-through for value collection but their KEYS are not regex-checked. Per @security F2 explicit pin.
  - **Check B (forbidden-value regex) applies to every collected leaf `./`-string** — at every recursion depth. Bash `[.. | strings]` walks all leaves; Rust walker mirrors it.

**Group (b) Secret/PII Rust guards** (4 modules — split by emission shape, NOT collapsed to `rust_security.rs`):
- `src/rust_secrets.rs` — Subcommand `rust-no-hardcoded-secrets`. 5 checks ported from `no-hardcoded-secrets.sh`. **MUST consume `secret_patterns::HYGIENE_PATTERNS`** for Check 2 (API-key prefixes) — closes the cross-stack dupe per ADR-0034 §6. Check 5 (long-base64) stays WARN-only.
- `src/rust_pii.rs` — Subcommand `rust-no-pii-in-logs`. 4 checks against `info!/debug!/warn!/error!/trace!` macros, `#[instrument]` fields, named tracing fields, anyhow/bail/Err contexts. **Consumes `common::pii_vocabulary::PII_TOKENS_CATEGORY_B`** for the PII identifier set (per @security F1 + @observability Q1 — both reviewers concur on canonical-home consolidation; option (a) from @observability Q1 is adopted) — NO module-local PII vocab. **The Wave-2 union is strictly broader than bash today's `PII_PATTERNS`** — adding `ip` / `ipv4` / `ipv6` / `device_id` / `fingerprint` / `ssn` / `dob` / `passport` / `driver_license` / `credit_card` / `card_number` / `cvv` / `latitude` / `longitude` / `geolocation` / `geoip` / `display_name` / `nickname` / `handle` / `address` / `postal_code` / `zip` / `zipcode` from CATEGORY_B that bash didn't catch. **This is a strict tightening for `rust-no-pii-in-logs`, not a loosening** (per @observability Q1 framing). @security paired-confirmed via F1 sign-off on CATEGORY_B promotion + additions. Module-header comment carries the surface-rationale note for the avoidance of doubt: PII identifier vocabulary is shared across observability surfaces (metric labels = Prometheus series dimensions; log fields = span attribute strings) because the *vocabulary* is the same even when cardinality cost shape differs.
- `src/rust_log_secrets.rs` — Subcommand `rust-no-secrets-in-logs`. 6 checks: `#[instrument]` without skip for secret params, secret in log macro, `expose_secret()` in log macro, named tracing fields, secret in error context, debug-fmt heuristic. **Per @security F1 (supersedes earlier rust_log_secrets-local SECRET_PATTERNS plan)**: consumes `common::pii_vocabulary::PII_TOKENS_CATEGORY_A` for the secret-identifier vocabulary — NO module-local copy. Bash's `SECRET_PATTERNS` superset (password|passwd|pwd|secret|token|key|credential|cred|bearer|api_key|master_key|private_key|client_secret|access_token|refresh_token) maps onto CATEGORY_A with @security F1's additions (`pwd`, `cred`, `bearer`). **Module-header comment** clarifies the axis distinction (identifier vocab vs value-shape catalog):
  ```
  // This module matches Rust identifier vocabulary (variable names like
  // `password`, `token`, `credential`) — consumed from the canonical
  // crate::common::pii_vocabulary::PII_TOKENS_CATEGORY_A SoT.
  //
  // DO NOT consolidate with crate::secret_patterns::HYGIENE_PATTERNS —
  // HYGIENE matches value shapes (JWTs, AWS keys, bearer tokens) and
  // answers a different question. Two separate catalogs, two separate
  // maintenance axes.
  ```
- `src/instrument_skip_all.rs` — Subcommand `rust-instrument-skip-all`. 2 checks: `#[instrument(skip(...))]` not `skip_all`; `#[instrument]` on functions with sensitive params lacking `skip_all`. **Per @security F3**: sensitive-param list (bash `instrument-skip-all.sh:108` = `password|secret|token|credential|private_key|client_secret|auth_code`) consumes `common::pii_vocabulary::PII_TOKENS_CATEGORY_A` directly. `auth_code` (OAuth authorization-code) is added to CATEGORY_A per F3's one-line proposal. **Per @observability Q2**: Check 2 fires as FAIL (preserves bash counting behavior — bash today increments violations despite the "POTENTIAL" wording). REASON tokens: `rust-instrument-skip-not-all` for Check 1, `rust-instrument-sensitive-param-no-skip-all` for Check 2 (two distinct tokens so operators can tell which heuristic tripped). The "POTENTIAL" wording from bash output is dropped as misleading-on-its-face; the runbook entry will document Check 2 as a heuristic with known FP shapes (multi-line `#[instrument]` blocks formatted by cargo fmt where `skip_all` appears beyond the 3-line lookahead — same residual FP class as bash today).

**Group (c) Test-discipline + ownership guards** (6 modules — already single-concern):
- `src/test_coverage.rs` — Subcommand `test-coverage`. Quick-mode only (warning-only, exit 0). `--full` mode is deferred — it runs `cargo llvm-cov` which is out of scope for a guard subcommand and is bash-only-today. Surfaced as Accepted Deferral.
- `src/test_registration.rs` — Subcommand `test-registration`. `#[path]` discovery in `crates/*/tests/*_tests.rs` against subdir `.rs` files. Whole-tree scan (not diff-based — same as bash today).
- `src/test_rigidity.rs` — Subcommand `test-rigidity`. 6 checks under `crates/env-tests/tests/`. Brace-depth tracking for match arms.
- `src/cross_boundary_classification.rs` — Subcommand `cross-boundary-classification`. Layer B; takes both explicit `main.md` arg and default mode (scan diff for `docs/devloop-outputs/**/main.md`). Manifest parser + GSA-not-Mechanical + Owner-in-manifest rules. Reuses `markdown_table` + `manifest_match` shared helpers (see below).
- `src/cross_boundary_scope.rs` — Subcommand `cross-boundary-scope`. Layer A; resolves scope (pending vs commit:SHA), finds active main.md, diff-vs-plan set arithmetic with symmetric exclusions. **Whole-file `docs/user-stories/*.md` exemption** (verbatim bash; row-level tightening rolled back per @team-lead 2026-05-21 — see §Tightenings during port + §Accepted Deferrals). Pure policy function `check_scope(changed_files, plan_paths, exempt_extras) -> ScopeReport`; thin orchestrator delegates verdict to the pure function.
- `src/gsa_sync.rs` — Subcommand `gsa-sync`. Static CANON array in code; checks 4 external mirror files. Markdown-slice extractor + YAML-key reader + intersection-subpath allowlist. **Per @operations flag (carries forward from bash `validate-gsa-sync.sh:3-17,38`)**: module header comment MUST explicitly call out the "5th mirror = CANON array in this module" so a future contributor extending GSA via micro-debate is pointed at all 5 places to update. The comment carries the same enumeration:
  ```
  // GSA enumeration mirrors (must update together):
  //   1. docs/decisions/adr-0024-agent-teams-workflow.md §6.4
  //   2. .claude/skills/devloop/SKILL.md
  //   3. .claude/skills/devloop/review-protocol.md
  //   4. scripts/guards/simple/cross-boundary-ownership.yaml
  //   5. This module's CANON const below (the canonical fully-expanded list)
  ```
  The `CANON` const itself carries "Update all five mirrors together." inline.

**Group (d) Structure/metadata guards** (7 modules):
- `src/api_version.rs` — Subcommand `api-version-check`. `.route("..")` literal extraction + version-prefix policy. Diff-scoped Rust files only.
- `src/env_config.rs` — Subcommand `env-config`. 3 checks against `config.rs::MissingEnvVar("..")` vs deployment manifest env vars + configMapKeyRef ↔ configmap data keys. Service set from canonical mapping (lifted into `common::services.rs` — Wave 1 already has it; verify and reuse).
- `src/kustomize.rs` — Subcommand `kustomize`. R-15…R-20. Shells out to `kustomize`/`kubectl kustomize`/`kubeconform` (these are external tools the bash today already shells out to — no new external dep). Owns multi-doc YAML splitting + security-context grep + empty-secret detection + dashboard cross-check.
- `src/knowledge_index.rs` — Subcommand `knowledge-index`. Walks `docs/specialist-knowledge/*/INDEX.md` — stale-pointer + ADR-existence + 75-line cap.
- `src/metric_coverage.rs` — Subcommand `metric-coverage`. Per-service `metrics.rs` macro extraction + `tests/**/*.rs` fixed-string search. Consumes `crate::metric_macros::MACRO_INVOCATION_WITH_FIRST_ARG_RE` (Wave 1 SoT) for opener detection — captures both bare `counter!(...)` and qualified `metrics::counter!(...)` forms. **Per @semantic-guard `[metrics-path-completeness]`**: post-extract filter pins the scanned macro-form set to **emission macros ONLY**: `MacroKind::{Counter, Gauge, Histogram}`. The `MacroKind::{DescribeCounter, DescribeGauge, DescribeHistogram}` variants are documentation macros (not emission sites) and are EXCLUDED from coverage scanning — matches bash today (`validate-metric-coverage.sh:82` extracts only `counter|histogram|gauge`, NOT `describe_*`). Pin uses `matches!(invocation.kind, MacroKind::Counter | MacroKind::Gauge | MacroKind::Histogram)` filter after `MACRO_INVOCATION_WITH_FIRST_ARG_RE` capture. Coverage is NOT loosened vs. bash; the macro-form set is the same `{counter, gauge, histogram}` triple. **Per @observability S3**: post-filter extracted names against `^[a-z][a-z0-9_]+$` (bash `validate-metric-coverage.sh:85`) to discard stray captures from comments/string literals — preserved verbatim. **Per @observability S1 fixture-suite contract** under `tests/fixtures/metric_coverage/`:
    - Positive+negative: emitted counter without test reference.
    - Positive+negative: emitted histogram without test reference.
    - Positive+negative: emitted gauge without test reference.
    - Parity fixture: `counter!(...)` (bare opener) — coverage-check applies.
    - Parity fixture: `metrics::counter!(...)` (qualified opener) — same coverage check.
    - Service-without-`metrics.rs` → STATUS=OK + WARN line via `common::scan::warn_skip` (bash today emits `WARNING`, NOT a FAIL — parity preserved).
    - `[metrics-path-completeness]` regression fixture: `describe_counter!("foo_total", "desc")` in `metrics.rs` does NOT count toward coverage requirement (describe macros are documentation, not emission). If only a describe macro is present and no test references `"foo_total"`, the policy does NOT FAIL — verifies bash parity is preserved against the wider Wave-1 macro set.
- `src/histogram_buckets.rs` — Subcommand `histogram-buckets`. `histogram!()` ↔ `Matcher::Prefix()` co-location check (same `metrics.rs`). Consumes `MACRO_INVOCATION_WITH_FIRST_ARG_RE` filtered to `MacroKind::Histogram` (only — `describe_histogram` excluded; documentation macros don't define buckets). Macro-form set: `{histogram}`, matching bash today. **Per @observability S2**: pure prefix-match semantics preserved (`metric_name.starts_with(prefix)`) — bash `[[ "$metric_name" == "$bp"* ]]` parity. NO equality requirement, NO longest-prefix-wins, NO `Matcher::Suffix`/`Matcher::Full` variants. Fixture: `Matcher::Prefix("mc_grpc_")` covers `mc_grpc_register_meeting_duration_seconds` (documented edge case).
- `src/todo_tracking.rs` — Subcommand `todo-tracking`. 2 rules: only `docs/TODO.md` exists tree-wide; main.md §Accepted Deferrals / §Tech Debt Pointers sections are pointer-only (line-shape check).

**New shared helpers** under `crates/dt-guard/src/common/`:
- `src/common/git_changes.rs` — `get_modified_files`, `get_added_files`, `get_deleted_files`, `get_all_changed_files`, `get_untracked_files`. **Literal-suffix matching by default** (tightening #2): callers pass `&[&str]` of extensions (e.g. `[".ts", ".tsx", ".svelte"]`); we suffix-match each path with `ends_with` after the path-prefix filter. The implementation calls `git` via `std::process::Command` (no libgit2 dep) — matches the existing bash forwarder posture (`get_diff_base` → `_get_base_ref.sh`).
- `src/common/manifest_match.rs` — Glob-match (`literal | prefix/** | extglob-equivalent`) ported from `path_matches_glob`. Used by `cross_boundary_classification` + `cross_boundary_scope`.
- `src/common/markdown_table.rs` — Parser for §Cross-Boundary Classification table rows + §Devloop Tracking table rows (per tightening #1). Returns `Vec<TableRow>` with cell trim, backtick strip, trailing-slash→`/**` canonicalization, and template-row filtering.
- `src/common/pii_vocabulary.rs` — **NEW per @security F1** — canonical home for PII / secret-identifier vocabulary. Wave 1 landed `PII_TOKENS_CATEGORY_A` (secret-identifier vocab: password, passwd, api_key, apikey, secret, token, bearer_token, access_token, refresh_token, session_token, id_token, private_key, privkey, signing_key, jwt, auth_header, authorization) at `crates/dt-guard/src/metric_labels.rs:33-52` and `PII_TOKENS_CATEGORY_B` (PII vocab: email, phone, ip_addr, name, etc.) at `:58-90` as private consts. Wave 2 promotes both to `pub(crate)` in `common/pii_vocabulary.rs`; `metric_labels.rs` is amended to `use` from there; Wave-2 modules consume the same SoT. Wave 2 also ADDS per @security F1+F3 recommendation: `pwd`, `cred`, `bearer`, `auth_code` to CATEGORY_A (legitimate identifier vocabulary that bash today's `SECRET_PATTERNS` + `instrument-skip-all.sh:108` catch but Wave-1 CATEGORY_A omitted); `full_name`, `first_name`, `last_name`, `real_name`, `user_name` to CATEGORY_B (PII identifier additions from bash today's `PII_PATTERNS`). Per @security per-token sign-off recorded inline in module header. **NOTE**: this promotion edits Wave-1's `metric_labels.rs` (re-export migration); since Wave-1 is `Mine` and `metric_labels.rs` itself isn't in GSA, this stays a `Mine` edit; row added below.
- `src/kustomize_tools.rs` (sibling to `kustomize.rs`, NOT under `common/`) — Small shell-out wrappers for `kustomize build` / `kubectl kustomize` / `kubeconform`. Detection probe + invocation + stderr capture. Keeps `kustomize.rs` policy module focused on policy. **Per @dry-reviewer nudge**: single-consumer extraction does not warrant `common/` siting (which by convention houses cross-subcommand kernels); a sibling module keeps the policy/tooling split visible without falsely signaling reuse. If a second consumer of kustomize-tooling ever lands, promotion to `common/` becomes the obvious move.

### Subcommand catalog

23 new subcommands, named to preserve wrapper-mapping clarity. Names use kebab-case with bash-guard-name stems where possible:

| Bash guard | New subcommand | Module |
|---|---|---|
| `ts/no-secrets-in-ts.sh` | `ts-no-secrets` | `ts_secrets.rs` |
| `ts/no-pii-in-logs-ts.sh` | `ts-no-pii-in-logs` | `ts_pii.rs` |
| `ts/no-test-removal-ts.sh` | `ts-no-test-removal` | `ts_test_removal.rs` |
| `ts/name-guard-dt-client.sh` | `ts-name-guard-dt-client` | `ts_metric_naming.rs` |
| `ts/no-dev-trust-path-in-prod-bundle.sh` | `ts-no-dev-trust-path-in-prod-bundle` | `ts_dev_trust.rs` |
| `ts/exports-map-closed.sh` | `ts-exports-map-closed` | `ts_exports_map.rs` |
| `no-hardcoded-secrets.sh` | `rust-no-hardcoded-secrets` | `rust_secrets.rs` |
| `no-pii-in-logs.sh` | `rust-no-pii-in-logs` | `rust_pii.rs` |
| `no-secrets-in-logs.sh` | `rust-no-secrets-in-logs` | `rust_log_secrets.rs` |
| `instrument-skip-all.sh` | `rust-instrument-skip-all` | `instrument_skip_all.rs` |
| `test-coverage.sh` | `test-coverage` | `test_coverage.rs` |
| `test-registration.sh` | `test-registration` | `test_registration.rs` |
| `test-rigidity.sh` | `test-rigidity` | `test_rigidity.rs` |
| `validate-cross-boundary-classification.sh` | `cross-boundary-classification` | `cross_boundary_classification.rs` |
| `validate-cross-boundary-scope.sh` | `cross-boundary-scope` | `cross_boundary_scope.rs` |
| `validate-gsa-sync.sh` | `gsa-sync` | `gsa_sync.rs` |
| `api-version-check.sh` | `api-version-check` | `api_version.rs` |
| `validate-env-config.sh` | `env-config` | `env_config.rs` |
| `validate-kustomize.sh` | `kustomize` | `kustomize.rs` |
| `validate-knowledge-index.sh` | `knowledge-index` | `knowledge_index.rs` |
| `validate-metric-coverage.sh` | `metric-coverage` | `metric_coverage.rs` |
| `validate-histogram-buckets.sh` | `histogram-buckets` | `histogram_buckets.rs` |
| `validate-todo-tracking.sh` | `todo-tracking` | `todo_tracking.rs` |

### Tightenings during port (per Lead's spec)

**1. `cross-boundary-scope` user-story exemption — ROLLED BACK to verbatim bash behavior** (per @team-lead 2026-05-21 redirect).

Earlier Wave-2 plan called for a row-level tightening that detected hunk shape against the `## Devloop Tracking` table block (3-bucket predicate — pure-insertion / pure-deletion / edit-existing-row). After implementation + the 8-case veto-blocking suite landed, the user redirected: the row-level tightening + diff-hunk machinery were not the right complexity for this devloop. Replaced with verbatim bash behavior: `docs/user-stories/*.md` files are exempt **whole-file** from drift detection.

Net plan-text deviation:
- `cross_boundary_scope.rs` is a thin orchestrator that delegates verdicts to a pure `check_scope(changed_files, plan_paths, exempt_extras) -> ScopeReport` function. The orchestrator fetches diff paths via git shellout, parses the main.md `## Cross-Boundary Classification` table via `common::markdown_table`, filters both sides through `is_symmetric_exclusion` (which treats user-story files as exempt), then calls `check_scope`.
- Unit tests target `check_scope` directly with hand-built string inputs — no `tempfile`, no `git init`, no subprocess (13 unit tests). The `cross_boundary_scope_userstory.rs` integration test was deleted; the 9-case veto-blocking suite is no longer applicable.
- The 2026-05-19 absorption finding (substantive user-story revisions bypass classification under the whole-file exemption) **stays open** under `docs/TODO.md` §Polyglot Pipeline Follow-ups for future focused work. PR review catches substantive edits in the interim. See §Accepted Deferrals.

**2. `git_changes::get_*_files` literal-suffix matching** (closes the same TODO entry).

Rust contract:
```rust
pub fn get_all_changed_files(
    repo_root: &Path,
    search_path: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> { /* ... */ }
```
- Each ext is matched with `path.to_str().map_or(false, |p| p.ends_with(ext))` — literal suffix, no regex.
- Callers can pass `&[]` to disable suffix filtering (matches bash's empty-`ext` behavior).
- The bash callers of `common.sh::get_*_files` are NOT modified; their false-positives remain a separate-devloop concern per the TODO entry (its scope is "Cross-guard behavior change — needs its own devloop with infrastructure + operations + all guard owners present").

### Test strategy per group

**Universal unit tests** — every new subcommand gets a `#[cfg(test)] mod tests` at the bottom of its `src/<module>.rs`, covering the core matcher functions (regex compile spot-checks, edge cases for line-parsing, etc.). Per ADR-0034 §2.

**Integration tests** at `crates/dt-guard/tests/`:
- `ts_secrets_e2e.rs` — assert_cmd against a fixture tree under `tests/fixtures/ts_secrets/{positive,negative}`. Positive fixture has one violation per check; negative fixture passes clean. Same shape as Wave 1's `cite_extract_e2e.rs`.
- `rust_secrets_e2e.rs` — same shape; positive fixture exercises every HYGIENE_PATTERNS class consumed by check 2.
- `test_rigidity_e2e.rs` — fixture tree under `tests/fixtures/test_rigidity/` — each of the 6 checks gets one positive fixture line + one negative (with the documented escape — `#[ignore]`, manual assert, etc.). **Check 5 is split into 5a/5b/5c per @test ASK**: 5a = HTTP-status numeric match arm (`NNN => {`), 5b = `Ok(...) => {`, 5c = `Err(...status: NNN...) => {`. Each sub-arm gets both a positive (unenforced) and negative (enforced via `assert!`/`panic!`) fixture — 8 total cases on Check 5 (3×2 + 2 edge cases for brace-depth-30 cap + nested-arm). Final case count = 5 + 8 + … per check; total ~14 cases across the 6 checks.
- ~~`cross_boundary_scope_userstory.rs`~~ — **DELETED** per @team-lead 2026-05-21 redirect when the row-level tightening was rolled back. The 8 unit-test cases in `cross_boundary_scope::tests` (covering `check_scope` directly with hand-built inputs) substitute coverage. See §Tightenings during port.
- `cross_boundary_classification.rs` — covers Layer B rule shapes with 9 cases (expanded per @test ASK + clarification on Mine-row fixture):
  1. FAIL — GSA path classified Mechanical.
  2. FAIL — GSA path with non-Mine classification missing Owner field.
  3. FAIL — GSA path with Owner not in manifest's specialist list for the path.
  4. PASS — GSA path classified Mine (no cross-boundary concern; skipped from rule checks). **Explicit Mine-row fixture** per @test confirmed-with-clarification.
  5. PASS — non-GSA path classified Mine.
  6. PASS — non-GSA path classified Mechanical (rule (a) only fires on GSA paths).
  7. PASS — explicit-mode invocation with single main.md arg.
  8. PASS — template-placeholder row (`{path}` / `TBD` / "during planning") skipped without check.
  9. PASS — header row + separator row skipped (table parser correctness regression test).
- `gsa_sync.rs` — fixture mirror tree (4 small markdown stubs + a YAML stub) — 8 cases (expanded per @test ASK):
  1. FAIL — canon path missing entirely from a markdown mirror.
  2. FAIL — backticked-token count mismatch (markdown has 13 paths but canon has 14).
  3. FAIL — YAML stray key (typo'd path not in CANON or INTERSECTION_SUBPATHS).
  4. PASS — INTERSECTION_SUBPATHS-allowed entry only in YAML (not in markdown mirrors).
  5. PASS — markdown shorthand: basename form (`meeting_token.rs` after canon `crates/common/src/meeting_token.rs`).
  6. PASS — markdown shorthand: last-dir `/**` form (`src/token/**` after canon `crates/ac-service/src/token/**`).
  7. PASS — markdown shorthand: two-segment `dir/last/**` form.
  8. PASS — slicer boundary case: prose paragraph below the bullet list does not extend the enumeration slice.
- `todo_tracking_e2e.rs` — covers stray TODO.md detection + inline-debt-body detection.
- `binary_status_surface.rs` (existing) — extended with two more cases pinning STATUS=FAIL emission for newly-added failure paths (kustomize tool absent; gsa-sync mirror missing).

**`assert_cmd` per @observability per-policy-fixture-suite convention**: each subcommand gets at minimum ONE positive + ONE negative fixture under `tests/fixtures/`. Heavier integration suites listed above are layered on top.

### Sub-phase sequence (4 phases)

Run the four groups SEQUENTIALLY to preserve a working tree at every step (`cargo check -p dt-guard` + `cargo test -p dt-guard` between phases). The final phase intentionally lands `validate-cross-boundary-scope` last — its tightening is the most subtle and any defect there could mis-gate Lead's Gate-2 workflow.

| Phase | Group | Subcommands |
|---|---|---|
| 1 | (a) TS guards | 6 — wholesale, no spec changes |
| 2 | (b) Secret/PII Rust guards | 4 — closes ADR §6 cross-stack dupe via HYGIENE_PATTERNS consumption |
| 3 | (d) Structure/metadata guards | 7 — externally-shaped, lower surface risk |
| 4 | (c) Test-discipline + cross-boundary guards | 6 — includes the two tightenings; runs **last** so Lead's workflow gate stays bash-stable until everything else is green |

Each phase ends with: build + test + spot-run a couple of wrappers against the live tree (`./scripts/guards/simple/<guard>.sh` invocation; expect either OK or a tractable diagnostic). Full `./scripts/layer-all.sh` runs ONLY at Gate 2 (per Lead's "Do not push to CI" instruction).

### Decisions worth flagging

- **No `rust_security.rs` aggregator** — split into the four modules above. Each owns a single concern; merging would defeat ADR §Neutral discipline.
- **Subcommand sprawl** — Wave 2 alone adds 23 subcommands. Combined with Wave 1's 8, total = 31. Per ADR-0034 §When-to-Revisit ≥10 trigger, this would normally warrant a re-debate on splitting `dt-guard` into focused binaries. Proceeding because (a) ADR-0034 §When-to-Revisit reads as "consider", not "must", (b) splitting is itself a re-debate and not in this devloop's scope, (c) the binary-build + dispatcher cost is bounded. **Per @code-reviewer item 5**: a `docs/TODO.md` §Polyglot Pipeline Follow-ups entry will be added at close-out time naming the trigger condition: *"Re-evaluate splitting `dt-guard` into 2-3 focused binaries (e.g., `dt-cite`, `dt-yaml-policy`) when subcommand count reaches 40 OR `cargo build --release -p dt-guard` cold-cache time exceeds Layer-1 budget OR `dt-guard --help` becomes unscannable for a contributor."* Owner: code-reviewer + infrastructure. Listed in §Files Modified at close-out.
- **`--explain` debug surface — MANDATORY on every new subcommand** (ADR-0034 §7 + @code-reviewer item 1). All 23 new subcommands take `--explain` (clap flag) and route findings through `common::explain` helpers. Per-rule `<rule_id>` constants live at the module top (e.g. `pub const TS_SECRETS_API_KEY_RULE_ID: &str = "api_key_prefix";`) for stable observability tokens. **No subcommand is exempt** — including the small ones (`test-registration`, `histogram-buckets`) where the `--explain` output is just the violating filename + matched-line context.
- **`--explain` secret-redaction — BLOCKING credential-leak fix** (per @semantic-guard Q1 — Option B adopted). Wave 1's `common::explain::print_finding(&Finding)` emits `matched="<span>"` where `<span>` is caller-supplied raw text. For Wave 2 group (b) callers, the matched span IS the secret (AKIA key, JWT, `password="hunter2"`, Slack token). Echoing that to stdout — which wrappers parse and humans pipe to logs/PR comments — would violate the credential-leak axis. Mitigation: a new helper at `crates/dt-guard/src/common/explain.rs`:
  ```rust
  pub struct SecretFinding<'a> {
      pub file: &'a str,
      pub row: usize,
      pub col: usize,
      pub policy: &'a str,
      pub pattern_name: &'a str, // e.g. "AWS access key", "JWT", "bearer token"
      pub extras: &'a [(&'a str, &'a str)],
      pub src_file: &'a str,
      pub src_line: u32,
  }
  pub fn print_secret_finding(f: &SecretFinding<'_>) {
      // Wire: EXPLAIN: <file>:<row>:<col> policy=<policy> pattern=<pattern_name> [<extras>] src=<src>:<line>
      // NOTE: no `matched=` field — pattern_name is the redacted descriptor.
  }
  ```
  - Distinct call site = explicit choice. Reviewers reading group-(b) code see `print_secret_finding` and understand the redaction guarantee at a glance.
  - `pattern_name` comes from `HYGIENE_PATTERNS`' tuple key (`"bearer token"`, `"AWS access key"`, `"JWT"`, etc.) — already structured. For non-HYGIENE callers (TS API-key prefix, password-literal scan, JWT shape) the caller passes a static `&str` describing the rule, e.g. `"api_key_prefix"`, `"jwt_literal"`.
  - For `rust-no-pii-in-logs` + `ts-no-pii-in-logs`: the matched span CAN be PII bytes from the source code (e.g. a hardcoded email literal in a log statement). Same treatment — these callers also use `print_secret_finding` with `pattern_name="<pii_token>"` (the CATEGORY_B token that hit).
  - For `instrument_skip_all` Check 1 (denylist `skip(...)` not `skip_all`): matched span is the `#[instrument(skip(password))]` attribute text — NOT secret-shaped, but the *parameter name* could carry a secret-vocabulary hit. Conservative call: use `print_secret_finding` for both Checks 1 and 2 in this module (consistent for the whole module).
  - **Group-(b) callers** using `print_secret_finding` (full list, pinned for @semantic-guard Gate-2 audit):
    - `rust_secrets.rs` — every rule.
    - `rust_pii.rs` — every rule.
    - `rust_log_secrets.rs` — every rule.
    - `instrument_skip_all.rs` — every rule.
    - `ts_secrets.rs` — every rule.
    - `ts_pii.rs` — every rule.
    - Plus `validate-alert-rules-policy` annotation_hygiene rule (Wave 1) — Wave 2 amends to `print_secret_finding` (the HYGIENE_PATTERNS matches there ARE secret bytes; Wave 1 currently emits via `print_finding`). This is a one-site Wave-1 amend; added as a §Cross-Boundary row for `crates/dt-guard/src/alert_rules.rs` (Mine).
  - **Non-secret callers** (group-a TS naming/dev-trust/exports/test-removal, group-c test-discipline + cross-boundary + gsa-sync, group-d structure/metadata) continue to use the existing `print_finding` — their matched spans are file paths / metric names / table cells / no secrets present.
  - **Fixture suite** (per @semantic-guard's Gate-2 audit request): each group-(b) subcommand's positive fixture in `tests/fixtures/<policy>/` includes one assertion of the form `assert!(!stdout.contains(<the actual secret bytes>))` — verifies redaction is observable, not just declared. The fixture file contains a known secret (e.g. `AKIA1234567890ABCDEF`); the test asserts `stdout.contains("pattern=AWS access key")` AND `!stdout.contains("AKIA1234567890ABCDEF")`. One assertion per pattern class.
  - Wave 1 reviewer trail: `print_finding` callers stay unchanged; the new helper is additive.
- **`metric_macros` canonical reuse — plan-time confirmation** (per @code-reviewer item 6): `metric_coverage` consumes `crate::metric_macros::MACRO_INVOCATION_WITH_FIRST_ARG_RE` (single regex covering both bare `counter!(...)` and qualified `metrics::counter!(...)` forms; capture group 3 = first-arg snake_case metric name; ordering puts `describe_*` before bare `counter!` so the alternation prefers the longer match per `metric_macros.rs:130-150`). No extraction needed — the Wave-1 pub surface already exposes the exact pattern. NO new `Lazy<Regex>` in `metric_coverage` for macro-opener detection. `histogram_buckets` similarly consumes `MACRO_INVOCATION_WITH_FIRST_ARG_RE` filtered to `MacroKind::Histogram` (kind classifier already at `metric_macros.rs:33-90`); the `Matcher::Prefix(...)` regex is bespoke (no Wave-1 prior art) and lands as a module-local `Lazy<Regex>`.
- **`path_safety::resolve_cited_path` reuse — confirm intent** (per @code-reviewer item 3): both `cross_boundary_scope` AND `cross_boundary_classification` consume `crate::common::path_safety::resolve_cited_path` for any path-containment check. Neither module re-implements the component-wise-`starts_with` + `canonicalize().ok()?` pattern. Specifically: when `cross_boundary_scope` processes diff paths and plan-table paths it joins them onto `repo_root` and validates containment via `resolve_cited_path` before adding to the set-arithmetic buckets; when `cross_boundary_classification` reads main.md and checks a path-cell against the manifest, it does NOT need containment (the manifest globs are inherently trusted strings, not user-supplied paths) — but path-cells from the table that flow into diff comparison DO route through `resolve_cited_path`. Single SoT preserved.
- **No new bash wrappers, no new flags** — each shell file is rewritten to ≤5 lines that source `_dt_guard_wrapper.sh`. Existing wrappers Wave 1 already added (the 8 cite/alert/dashboard/metric/grafana wrappers) are NOT modified — wave 2 only flips the 23 new ones.
- **`STRICT_EXPORTS_MAP=1` env var — one additional flag arg passed through** (NOT a wrapper-shape exception, per @code-reviewer item 4 reframe). The `ts-exports-map-closed` wrapper computes one extra clap arg from the env var:
  ```bash
  # ts/exports-map-closed.sh
  #!/usr/bin/env bash
  extra_args=()
  [[ "${STRICT_EXPORTS_MAP:-0}" == "1" ]] && extra_args+=(--strict)
  # shellcheck source=../_dt_guard_wrapper.sh
  source "$(dirname "${BASH_SOURCE[0]}")/../_dt_guard_wrapper.sh" ts-exports-map-closed "${extra_args[@]}"
  ```
  This is **3 substantive lines** (env-var check + array build + source) plus shebang + comment — within the ≤5-line wrapper contract. Earlier framing of "5-line shape exception" was incorrect; corrected here. Rationale: bash today does `STRICT="${STRICT_EXPORTS_MAP:-0}"; [[ "$STRICT" == "1" ]]` — literal-string check. Preserving the exact semantics avoids a behavior change on edge values like `STRICT_EXPORTS_MAP=true` (current bash: NOT strict; generic truthy would flip that).
  **Wave-1 wrapper-prelude touch needed**: `_dt_guard_wrapper.sh` currently `exec`s with a fixed `--root "$REPO_ROOT"` and discards `"$@"`; to support per-wrapper extra args, the prelude is amended to forward `"$@"` after `--root "$REPO_ROOT"`. This is a 1-line change to the prelude — behavior-preserving for the 8 Wave-1 consumers (none pass extra args today). `scripts/guards/simple/_dt_guard_wrapper.sh` is in the Cross-Boundary table as a Mine row.
- **`kustomize` subcommand needs external tools** — `kustomize`/`kubectl`/`kubeconform` are detected at subcommand-start, missing tools degrade gracefully to WARN (same as bash today). No new dep added.
- **Status emission + error-context plumbing** (per @semantic-guard Q2 — blocking).
  - **(a) External-I/O `.with_context`**: Every external-I/O call in the 23 new subcommands carries `.with_context(|| format!("..."))` that names file/path/operation. Sites that are mandatorily wrapped:
    - `std::fs::read_to_string(path)` and `read_dir(path)` — context = `format!("reading {}", path.display())`.
    - `std::process::Command::new("git").args(...).output()` and equivalents for `kustomize`/`kubectl`/`kubeconform` — context = `format!("running {tool} {args:?}")` (the `{args:?}` `Debug` impl on `OsString` does not echo file contents).
    - `serde_json::from_str` / `serde_norway::from_str` — context = `format!("parsing {} as {}", path.display(), schema_name)`.
    - `walkdir::WalkDir::new(path).into_iter()` skip points — context = `format!("walking {}", root.display())`.
    - **CRITICAL credential-leak constraint**: context strings MUST name files/operations only — NEVER echo file *contents*. Forbidden shape: `.with_context(|| format!("parsing {}: {}", path.display(), contents))`. Acceptable shape: `.with_context(|| format!("parsing {}", path.display()))`. Enforced via code-review at Gate 2 + the same fixture-assertion pattern as Q1 (positive fixture contains a secret; assert stdout/stderr does NOT contain the secret bytes).
  - **(b) Policy-violation paths**: every "guard found a violation" branch calls `emit_fail("<stable-token>")` BEFORE `return Err(...)` so wrappers parse the stable token rather than the slugified-from-`anyhow` token (which would vary as error messages get edited). Same shape as Wave 1's `alert_rules::run`. **Per @semantic-guard request**: §Implementation Summary will list every stable REASON token used per subcommand (not just the policy-violation classes already flagged for @operations) so audit confirms no path lacks a stable token. Stable-token names per the catalog already drafted in §Decisions "REASON-token catalog grouped by failure-shape class". Catch-all path in `main.rs` `match run(cli) { Err(e) => emit_fail(reason_token(&e)) }` remains the safety net for `Result` shapes that bypass explicit emission.
  - Per Wave 1 + Layer 3 contract. `common::status::{emit_fail, reason_token}` is the single SoT for wire emission.
- **REASON-token catalog grouped by failure-shape class** (per @operations flag for Wave 3 runbook surface §6.3.1): at close-out time, §Implementation Summary will list every Wave-2 REASON token grouped by class:
  1. **Stale-binary class** (Wave 1, no new tokens): `dt-guard-binary-missing` (wrapper-emitted on missing binary).
  2. **Subcommand-not-found class** (Wave 1, no new tokens): clap-emitted, exit 2.
  3. **Policy-violation class** (most Wave-2 tokens — code-level invariant violated): `ts-secrets-violation-found`, `rust-pii-violation-found`, `cross-boundary-scope-drift-inbound`, etc.
  4. **External-tool-absent class** (NEW with Wave 2 — `kustomize` subcommand when `kustomize`/`kubectl`/`kubeconform` are absent): degrades to WARN per bash today; STATUS=OK REASON=`kustomize-tool-absent-skipped` rather than FAIL. Not a failure but worth its own §6.3.1 bullet because the WARN line carries operator-actionable context.
  5. **Mirror-state-error class** (NEW with Wave 2 — `gsa-sync` reports coordination-state errors across repo locations, not code-level invariants): `gsa-sync-canon-missing`, `gsa-sync-count-mismatch`, `gsa-sync-stray-yaml-key`, `gsa-sync-intersection-subpath-not-allowed`. Different runbook resolution path from policy-violation class — fix is "update mirror M to match canon" or "update canon to reflect new mirror state" rather than "fix the failing code."
  
  @operations to mirror this grouping into `docs/TODO.md` §Polyglot Pipeline Follow-ups for Wave 3 runbook surface (`docs/runbooks/devloop-validation.md` §6.3.1).

---

## Decisions (cumulative Wave-2 deviations)

The six deviations below were taken in-loop, after Gate 1 plan-confirmation. Each names the deviation, the deviation's reason, and where the next reader picks up the trail.

1. **Test-block filter — `#[cfg(test)]` brace-counter promoted from Wave 3 to Wave 2.**
   Originally `filter_test_code` was in §Accepted Deferrals as a Wave-3 follow-up (per @security Q2): bash today uses `strip-test-code.sh` (nightly-rustc-backed) for surgical line ranges; Wave 2 would ship a path-based `is_test_file` heuristic only. **Empirically the path-based heuristic produced a 92% false-positive rate** on the Wave-2 corpus (production `crates/*/src/*.rs` files that contain inline `#[cfg(test)] mod tests` were misclassified, but only the mod-tests block was test code — every other line in the file was production). Resolution: built `crates/dt-guard/src/common/test_code_filter.rs` with a brace-counter detector for `#[cfg(test)]` / `#[cfg(any(test, ...))]` / `#[cfg(all(test, ...))]` blocks, in addition to `is_test_path`. Consumed by `rust_secrets.rs`, `rust_pii.rs`, `rust_log_secrets.rs`, `instrument_skip_all.rs`, `ts_secrets.rs`, `ts_pii.rs`. The (α) "exact rustc spans" vs (β) "approximate brace counter, in-tree, no nightly dep" tradeoff resolved in favor of (β) — the FP cost of waiting for Wave 3 outweighed the precision win of the rustc approach. Caveat documented at the module head: `#[cfg_attr(test, ...)]` is NOT detected (strict `cfg(test)` only). Wave 3 `syn`-based port stays open as a precision upgrade.

2. **PII vocab `address` swap — bare `address` dropped from CATEGORY_B; added `ip_address` + `email_address` + `mac_address`.**
   The plan promoted `address` (along with `postal_code` / `zip` / `zipcode`) into CATEGORY_B as a strict tightening for `rust-no-pii-in-logs`. After Phase 2 wired CATEGORY_B into the four group-(b) consumers, the bare `address` token blast-radiused into `listen_address` / `metrics_address` / `grpc_address` / `webtransport_advertise_address` config plumbing — every service's `main.rs` and `config.rs` logs these at startup, and they are network endpoints, not user PII. Net effect: ~30 false positives across `ac-service` / `gc-service` / `mc-service` / `mh-service` config-bootstrap logging. Resolution: dropped bare `address` from CATEGORY_B; added the three compound forms `ip_address`, `email_address`, `mac_address` so the original tightening intent is preserved on the actual PII-shaped identifiers without the config-plumbing blast. `postal_code`/`zip`/`zipcode` stay (no false-positive blast). Documented inline at `crates/dt-guard/src/common/pii_vocabulary.rs:93-100`.

3. **`rust-no-secrets-in-logs` Check 2 regex tightened to bash's three interpolation shapes.**
   Original plan: Check 2 = "secret-vocab identifier appears in a log macro call." During Phase 2 spot-validation the rule false-positived on production `info!("authorize", token = ?req.token.id())` shapes (the bare `token` in the call-site triggered the match even though the value being logged is `req.token.id()` — an int, not a secret). Resolution: tightened to bash parity — Check 2 fires only when a secret-vocab identifier appears in **one of bash's three interpolation shapes**: (a) `{X}` brace-format interpolation; (b) `%X` / `?X` tracing display/debug marker; (c) `, X,` / `, X)` positional after format string. Bare identifier mention without one of these shapes is no longer a hit. Pinned at `crates/dt-guard/src/rust_log_secrets.rs:66-76` with a module-comment naming the three shapes.

4. **Row-level user-story tightening rolled back to bash whole-file exemption.**
   Already covered in `## Tightenings during port` item 1 (per @team-lead 2026-05-21 redirect). Reason summary: complexity of the 3-bucket hunk-shape predicate + 8-case veto-blocking suite exceeded the value vs. PR review catching substantive user-story edits in the interim. The 2026-05-19 absorption finding stays OPEN as a `docs/TODO.md` §Polyglot Pipeline Follow-up.

5. **SoT-only git rule — policy modules have zero `Command::new("git")`; all git shell-outs go through `common::git_changes`.**
   The plan named `common::git_changes` as a Wave-2 shared helper with five public functions. During Phase-4 implementation of `cross_boundary_scope.rs`, an inline `Command::new("git").args(["diff", "--name-only", base, "HEAD"])` shell-out leaked into the orchestrator for the dirty-vs-clean scope resolution (the plan's helper set didn't cover that exact shape). Resolution: extended `common::git_changes` with the two missing helpers (`get_active_edit_paths` for the pending-mode union and `get_tracked_files` for tracked-set membership), added a third (`is_gitignored`) once a second consumer for it materialised, and pushed all six remaining inline git shell-outs through the helper. **Verified independently by team-lead** via `grep -rn 'Command::new("git")' crates/dt-guard/src` — zero matches outside `common/git_changes.rs` (the lone match in `cross_boundary_scope.rs:212` is a docstring comment naming what the module DOESN'T do). Reason: testability — policy code is pure-functions-of-data plus a thin orchestrator; the IO boundary stays in one module so policy-shape changes don't require subprocess mocking.

6. **Guard-internal-path structural exclusion — `crates/dt-guard/**` and `scripts/guards/**` exempt from every policy module's scan.**
   The Wave-2 implementation accumulated four production-code self-match sites where guard source legitimately contained the patterns it detects: `rust_log_secrets.rs` near Check 4 (the `line.contains("tracing::")` predicate names a catalog token); `cross_boundary_classification.rs:289` and `gsa_sync.rs:288` (each `bail!("...{token}-{}", ...)` interpolates a local `token` string variable that the guard sees as a secret-vocab identifier in a log/error context); `histogram_buckets.rs:159-162` + `metric_coverage.rs:196-214` (inline `r#"histogram!("foo", labels); counter!(...)"#` test fixtures that the metric-labels parser tries to parse as real macro calls). The original mid-loop plan was a one-off `// guard:ignore(string-literal-pattern)` annotation sweep across the known sites — that approach was rejected per @team-lead 2026-05-22 redirect: the self-match family is structurally inevitable for guard source code (regex catalogs, vocabulary identifiers, detection-logic literals, macro-shaped fixtures are all load-bearing), so the principled boundary is **"guards don't scan themselves."** Resolution: new `crate::common::test_code_filter::is_guard_internal_path` predicate returns true for paths under `crates/dt-guard/**` or `scripts/guards/**`; new composing predicate `is_scan_exempt` unifies it with `is_test_path` as the single maintenance point for scan-exclusion categories. All ~7 policy modules with a `fn is_excluded_path` wrapper now delegate to `is_scan_exempt`; Wave-1's `metric_labels.rs` is amended to consume the same predicate in its secondary-scan loop. The earlier narrow `CANONICAL_PATTERN_HOMES` constant (enumerated just `secret_patterns.rs`) is retired — subsumed by the structural rule. Net effect: L3 self-match class collapses to zero with one principled rule instead of a sweep of per-site annotations.

7. **Scope-creep: mh-service metric-coverage backfill (4 metrics, 11 tests, 3 test files).**
   The Wave-2 `validate-metric-coverage` port surfaced 4 mh-service metrics as previously-uncovered: `mh_grpc_requests_total`, `mh_errors_total`, `mh_mc_notifications_total`, `mh_token_refresh_failures_total`. Initial Gate-2 framing was "pre-existing — would need a separate mh-service test-coverage devloop." User authorized in-loop closure 2026-05-22 once the root cause was understood (see §Decisions item 8 for the regex inheritance audit that explains why these were uncovered before). Tests landed: extended `crates/mh-service/tests/token_refresh_integration.rs` with a `mh_token_refresh_failures_total` assertion on the existing `failed_refresh_emits_token_refresh_metrics_end_to_end` test plus a new matrix test (`failed_refresh_emits_failure_counter_for_every_error_category`) over the 6 bounded `error_category` values from `common::token_manager::error_category`; created new `crates/mh-service/tests/errors_grpc_metrics_integration.rs` (5 tests covering `record_error` + `record_grpc_request` direct-wrapper invocation with label-swap-bug adjacency assertions); created new `crates/mh-service/tests/mc_notifications_metric_integration.rs` (3 tests including the 2×2 matrix over `event_type` × `status`). All 11 tests pass; `validate-metric-coverage` now reports `STATUS=OK REASON=metric-coverage-all-covered`. Pattern: direct `record_*` wrapper invocation (no surrounding policy/retry seam adds value for these label-stable wrappers) — mirrors the existing `token_refresh_integration.rs` shape rather than the heavier MC `mc_client_integration.rs` gRPC-mock shape. The 3 new test files are added to the §Cross-Boundary Classification table as `Mine` rows (test code is universally `Mine` for whichever specialist writes them per ADR-0024 §6.4 — these paths are not in GSA).

8. **Single-line-regex inheritance audit — no LIMITATIONS INHERITED beyond `metric_coverage`'s now-fixed case.**
   Trigger: bash predecessor of `validate-metric-coverage` had a single-line regex blind spot for multi-line `counter!(...)` invocations, silently missing the 4 mh-service metrics named in item 7. The Rust port via `metric_macros::MACRO_INVOCATION_WITH_FIRST_ARG_RE` uses `(?s)` (DOTALL) and matched correctly — surfacing the gap. Audit goal: confirm no other Wave-1/Wave-2 Rust guard inherited the same blind spot when real source code has multi-line shapes. **Audit method**: enumerated all 35+ `Lazy<Regex>` / `Regex::new` sites under `crates/dt-guard/src/**` and classified each by call-site shape (line-iter vs whole-content) + production-source likelihood of multi-line forms.
   
   **Classification:**
   - **OK — Rust correctly multi-line aware (`(?s)` or balanced-paren walker)**: `metric_macros::MACRO_INVOCATION_WITH_FIRST_ARG_RE` (explicit `(?s)`); `metric_labels.rs::find_macro_invocations` (opener regex + byte-walking balanced-paren — handles arbitrary newlines inside the call); `metric_coverage.rs::extract_emission_metric_names` + `histogram_buckets.rs::extract_histogram_names` (both consume `MACRO_INVOCATION_WITH_FIRST_ARG_RE`).
   - **OK — inherently single-line by design (line-iter pattern is semantically correct)**: `rust_secrets.rs`, `rust_pii.rs`, `rust_log_secrets.rs`, `instrument_skip_all.rs`, `ts_secrets.rs`, `ts_pii.rs` (all use `for (idx, line) in content.lines().enumerate()` and check per-line predicates — a `password = "..."` assignment, an `info!(...)` call's macro-name token, an `#[instrument]` attribute opener: all are intrinsically single-line in real source); `env_config.rs::extract_workload_configmap_key_refs` (line-iter + windowed-lines lookups — k8s YAML keys are one-per-line); `ts_metric_naming.rs::scan` (line-iter for `meter.createCounter(...)` calls — see caveat below); `test_rigidity.rs` (line-iter with anchored `^\s*...$` regexes for match arms — match-arm headers are one-per-line); `alert_rules.rs`, `dashboard_panels.rs`, `infrastructure_metrics.rs`, `grafana_datasources.rs` (all operate on extracted PromQL or JSON string values, not raw file content — multi-line semantics don't apply at this layer); `cite_extract.rs::resolvers` (per-extension regex resolvers use `(?m)` for line anchoring over whole-content scan — symbol declarations are one-per-line by convention); `knowledge_index.rs::BACKTICK_PATH_RE` (backtick spans are one-line in markdown by Markdown spec); `ts_exports_map.rs` (operates on `serde_json::Value` walk, not file regex); `ignore.rs::IGNORE_MARKER_*_RE` (comment lines, one-per-line).
   - **LIMITATION INHERITED — potential blind spot**: NONE. Per @team-lead 2026-05-23 directive (silent-missing-validation > overly-strict-early), the `ts_metric_naming.rs` multi-line awareness was implemented in-loop rather than deferred. The opener regex (`METER_OPENER_RE`) matches up to `.createX(`; a byte-walker (`find_meter_calls`) then walks across newlines for the first string-literal arg + the matching `)`, with full support for line/block comments inside the call body, parens-inside-literals, chained calls, conditionally-nested calls, and template literals (interpolated names yield `name: None` → HARD FAIL via R-26). Reference shape: `metric_labels::find_macro_invocations`. 13 new unit tests under `ts_metric_naming::tests` exercise: single-line baseline regression, multi-line 2-line, multi-line 5+ lines with blanks + leading prose, block-comment-inside-call, line-comment-inside-call, chained `.bind({})`, nested in `if`, parens-inside-literal-do-not-close-outer-call, whitespace-before-`.factory`, bad name multi-line HARD-FAIL, interpolated template literal HARD-FAIL, string-concatenation documented residual limitation (bash today's same shape), opener-line vs literal-line line-no tracking, multiple-calls-in-one-source.
   
   **Net audit result**: 0 inherited blind spots, 0 theoretical caveats. The single-line-regex pattern from bash was contained to (a) the `metric_coverage`/`histogram_buckets` family (Rust port fixed via canonical `MACRO_INVOCATION_WITH_FIRST_ARG_RE` with `(?s)`) and (b) `ts_metric_naming` (fixed in-loop 2026-05-23 via the balanced-paren walker described above). Comprehensive unit-test coverage proves correctness for multi-line shapes even though no real TS source exists in-tree yet.

9. **HYGIENE catalog applicability slices — `HYGIENE_SOURCE_SCAN_SUBSET` introduced.**
   Original Gate-1 framing (per @security Q1): full `HYGIENE_PATTERNS` consumption from both source-code consumers (`rust_secrets`, `ts_secrets`) to close the cross-stack dupe per ADR-0034 §6. Empirically (Gate 2 2026-05-22 via @paired-client F-CLIENT-2 + @security in-tree FP survey): 7 of the 13 HYGIENE patterns were authored for alert-annotation-text hygiene and FP on source code that legitimately contains internal hostnames, Bearer-shaped doc placeholders, and PEM-format references in comments. In-tree Rust FPs surveyed across `crates/gc-service/src/config.rs:314,335` (internal-DNS hostnames in config defaults), `crates/ac-service/src/crypto/mod.rs:876` ("auth-prod-2025-01" key_id label), `crates/devloop-helper/src/ports.rs:744,750,858` ("host.containers.internal" podman dev-host alias). Resolution: split the catalog into one full set (alert-rule consumers) and one source-scan subset (`HYGIENE_SOURCE_SCAN_SUBSET` — 5 unambiguous Class-V value-shape patterns: `AWS access key`, `OpenAI/Stripe-style key`, `GitHub PAT`, `Slack token`, `JWT`). Single catalog, two applicability slices; cross-stack dupe closure preserved. `rust_secrets.rs` + `ts_secrets.rs` Check 2 call sites consume `secret_patterns::source_scan_patterns()` instead of `HYGIENE_PATTERNS.iter()`. `alert_rules.rs` consumer unchanged. Module rustdoc at `secret_patterns.rs:17-39` carries the Class-V/Class-C framing per @paired-client's nudge so the applicability question is discoverable next to the patterns themselves for future catalog additions. Original Q1 "full HYGIENE everywhere" was the wrong scope — corrected here.

10. **`kustomize` subcommand R-15/R-17/R-18/R-19 implemented in-loop — sibling `kustomize_tools.rs` module.**
    Initial Wave-2 implementation (mid-loop) shipped only R-16 (orphan manifests) + R-20 (dashboard coverage); R-15 (kustomize build), R-17 (kubeconform schema), R-18 (security-context invariants), R-19 (empty-secret-value detection) were silently absent — the tool-detection probe (`detect_kustomize_tool`) was called but its result only fed the STATUS emission token, no build was actually invoked. Surfaced by @operations Gate-2 F1 (LoC delta bash 507 vs. Rust 302; load-bearing R-18 security-context regression detection); @team-lead authorized fix-in-loop 2026-05-22 (user redirect). Resolution: new sibling module `crates/dt-guard/src/kustomize_tools.rs` (per @dry-reviewer Gate-1 nudge — sibling over `common/` siting until a second consumer materializes) carrying tool-detection (`KustomizeTool` enum, `detect_kustomize_tool` standalone-or-kubectl probe, `detect_kubeconform`), build invocation (`run_kustomize_build`, `run_kubeconform`), and two pure-policy functions (`check_security_context` walks `Deployment`/`StatefulSet` for `runAsNonRoot`/`allowPrivilegeEscalation`/`capabilities.drop ALL`/`readOnlyRootFilesystem` with bash-parity name-substring exemptions for `postgres`/`prometheus`/`loki`/`grafana`; `check_empty_secret_data` walks Secret `data:`/`stringData:` for empty values, reports key names only — NEVER echoes values). `kustomize.rs::run` extended with a thin orchestrator: enumerates build targets (per-service bases + observability base + per-service overlays + observability overlay), invokes kustomize build per target, runs R-17/R-18/R-19 on rendered stdout for bases (overlays inherit container specs — bash today's same shape). Tool-absent fallback: per bash today + @team-lead directive, missing `kustomize`/`kubectl kustomize`/`kubeconform` degrades to per-check WARN, never FAIL on missing tools (devloop containers may lack kubeconform). Pure-function policies are tested with hand-built rendered-YAML string fixtures (15 new unit tests in `kustomize_tools::tests`) — no Kind cluster needed. Live-tree spot-check: `STATUS=OK REASON=kustomize-clean-kubeconform-skipped` (kubectl-kustomize ran R-15 for all targets; R-17 WARN-skipped via `has_kubeconform = false`; R-18/R-19 ran clean against rendered output). REASON-token grouping per failure-shape class: `kustomize-build-failed-N`, `kustomize-kubeconform-failed-N`, `kustomize-security-context-N`, `kustomize-empty-secret-value-N`, plus the existing `kustomize-orphan-manifest-N` + `kustomize-dashboard-orphan-N` — fires the largest class for runbook-action clarity. Per @operations §6.3.1 runbook surface: empty-secret findings ONLY report Secret name + key name; value bytes are never echoed (port-fidelity with bash). Pointer back: §Cross-Boundary Classification table now lists `crates/dt-guard/src/kustomize_tools.rs` as Mine.

---

## Pre-Work

None. Wave 1 (task #44, completed) established the scaffold.

---

## Implementation Summary

23 bash guards flipped to ≤5-line wrappers invoking `dt-guard <subcommand>` via the shared `_dt_guard_wrapper.sh` prelude. Net delta: +11,330 / -4,754 lines across 73 files. Per-thematic-group counts and module homes are enumerated in §Cross-Boundary Classification.

Beyond the 23 planned subcommand ports, this devloop absorbed 5 in-loop scope additions (each documented in §Decisions):
1. Brace-counter `#[cfg(test)]` test-block filter pulled in from Wave-3 deferral after empirical 92% FP rate on Phase-2 spot-tests.
2. SoT-only-git rule: `Command::new("git")` consolidated to `common::git_changes` with three new helpers (`get_tracked_files`, `is_gitignored`, `get_active_edit_paths`).
3. Structural guard-internal-path exclusion (`is_guard_internal_path` + composing `is_scan_exempt`) subsuming an earlier `CANONICAL_PATTERN_HOMES` hand-curated list.
4. mh-service metric-coverage gap closure: 11 component tests covering 4 metrics bash silently missed via single-line-regex blind spot.
5. Kustomize R-15/R-17/R-18/R-19 fix-in-loop: new `kustomize_tools.rs` (472 LoC) implements `kustomize build` + `kubeconform` + security-context regression detection + empty-secret-data detection (key-only redaction). Restores bash port-fidelity; the original Rust port only implemented R-16/R-20.

Also landed: regex inheritance audit closing all real-world blind spots and the one initially-theoretical caveat (`ts_metric_naming` multi-line balanced-paren walker mirroring `metric_labels::find_macro_invocations`).

---

## Files Modified

```
73 files changed, 11330 insertions(+), 4754 deletions(-)
```

41 modified files (Wave-1 shell wrappers flipped + Wave-1 Rust amendments) + 32 new files (23 Wave-2 policy modules + 5 new common helpers + kustomize_tools sibling + 2 new mh-service test files + the devloop output dir). All paths listed in §Cross-Boundary Classification above, classified `Mine` for infrastructure (the implementing specialist), none in §6.4 GSA.

### Plan-to-impl deviation: N-case suites landed inline rather than `tests/<file>.rs`

Per @test F6 2026-05-23 (default-accept): the plan's §Test strategy named several N-case suites with target locations under `crates/dt-guard/tests/<file>.rs` (notably `cross_boundary_classification.rs` 9 cases, `gsa_sync.rs` 8 cases, `test_rigidity_e2e.rs` 14 cases). Implementation landed those cases as `#[cfg(test)] mod tests { ... }` inline within the policy module instead. Rationale: every named case is a whitebox test of a private helper (`run_check(&md, &manifest)`, `check_markdown_mirror(label, content)`, `arm_has_enforcement(lines, start_line)`, etc.) — inline access avoids a `pub(crate)` widening for tests-only consumers, matches Wave-1 precedent (`metric_macros::tests`, `path_safety::tests`), and keeps the case-to-policy distance short. The plan's `tests/<file>.rs` shape would have required either re-exposing the private helpers or re-implementing the orchestration shim outside the module — both worse than inline whitebox. No coverage delta; same case count + same assertions.

---

## Devloop Verification Steps

Final `./scripts/layer-all.sh` run at Gate 3 close (post all-fix-ups, including the multi-line ts-name-guard fix-in-loop):

| Layer | Result | Duration | Notes |
|-------|--------|----------|-------|
| 1 (build) | OK | 23s | cargo check + buf compile + nx typecheck all clean |
| 2 (fmt) | OK | 1s | cargo fmt + buf format + nx format all clean |
| 3 (guards) | OK | 2s | all 31 simple guards pass including extended `validate-kustomize` |
| 4 (test) | OK | 169s | 299 dt-guard lib tests + 11 mh-service integration tests + workspace pass |
| 5 (lint) | OK | 2s | cargo clippy `-D warnings` + nx lint + proto lint all clean |
| 6 (audit) | FAIL | 2s | **pre-existing only**: RUSTSEC-2023-0071 (rsa Marvin via sqlx-mysql; Wave-3 follow-up); `buf breaking` on already-deleted `internal.proto`/`signaling.proto` from task #30 (override per `eb8b827`) |
| 7 (env-tests) | N/A | 0s | wave2-pending by design (no service-touching changes) |

**Wave-2-introduced surface: end-to-end clean.** Every failure that fires does so only on items that pre-date this devloop and are already tracked or on the accepted-override path.

---

## Code Review Results

### Verdict Tracker (Gate 3)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | F-Wave2-S1 LOW closed: implementer added the sub-bullet under existing `filter_test_code` rustc-span Wave-3 entry in docs/TODO.md (the resolution path security specified). No latent finding chase. F1/F2/F3 Gate-1 fold-throughs all verified. |
| Test | RESOLVED-FIXED | 6 | 4 | 0 | All 6 findings closed: F1 dead helper deleted; F2 Check 1 collapsed (single accumulator, last-write-wins); F3 dominant-class fold-down comment at 3 sites (operator-affordance accepted); F4 `let _ = md;` deleted; F5 4×2 matrix + rename to `distinguishes_all_combinations`; F6 plan-to-impl-inline deviation noted in §Files Modified. 285 lib tests + 11 mh integration tests green. |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | F-OBS-W2-1 — dead `PhantomData<PathBuf>` workaround in metric_coverage.rs fixed in-loop. ADR-0034/0032/0029/0011 all PASS. Item 7 mh-coverage backfill confirmed using right patterns. |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | F1 (5 `#[allow(clippy::indexing_slicing)]` → `#[expect(..., reason = ...)]` per ADR-0002), F2 (stale wrapper comment at `validate-cross-boundary-scope.sh:4` advertising rolled-back tightening). Both fixed in-PR. ADR-0034/0002/0019/0024 all PASS. Ownership Lens: 67 rows all `Mine`, GSA-clear. |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | F1 (`CANONICAL_SERVICES` re-inlined in 3 modules — F-DRY-3 regression; fixed by `use crate::common::services::CANONICAL_SERVICES`), F2 (stale `MacroKind` E-DRY-1 TODO entry — marked resolved with Wave-2 pointer). Both fixed in-PR. Surfaced 9 cumulative Wave-2 DRY wins. |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | F1 (kustomize R-15/R-17/R-18/R-19 implemented in-loop via sibling `kustomize_tools.rs`; R-18 security-context regression detection restored; tool-absent fallback per-check WARN matching bash; 15 unit tests; value-redaction in secret-data check preserved). F2 (stale wrapper comment scrubbed). F3 (dead-import shim removed). Gate-1 flag-1 5-mirror CANON comment landed; flag-2 REASON-token classification preserved through §Decisions item 10. |
| Semantic Guard | CLEAR | 0 | 0 | 0 | Native SAFE → CLEAR. All 4 anti-pattern checks pass: [credential-leak] `print_secret_finding` redaction observable + no raw-byte echoes; [actor-blocking] n/a sync CLI; [error-context-preservation] `.with_context()` files/ops-only discipline preserved; [metrics-path-completeness] macro-form pinning preserved, `(?s)` DOTALL strictly better than bash. Gate-1 Q1+Q2 commitments verified at code level. |
| Paired Client | RESOLVED-FIXED | 3 | 2 | 0 | F-CLIENT-1 (`ts_pii` Check 2 widened bash's object-field regex shape — fixed via `PII_IN_OBJECT_FIELD_RE` static gating Check 2), F-CLIENT-2 (`ts_secrets`+`rust_secrets` Check 2 over-broad HYGIENE consumption — fixed via `HYGIENE_SOURCE_SCAN_SUBSET` Class-V/Class-C split, security co-signed + extended to rust_secrets). F-CLIENT-3 (`is_allowed_line` substring vs word-boundary) accepted-as-is — bash-inheritance carryover, not a port regression. All 4 Gate-1 fidelity asks (F1-F4) verified in source. |

---

## Accepted Deferrals

Pointer-only per SKILL convention — full bodies live in `docs/TODO.md` §Polyglot Pipeline Follow-ups.

- **`filter_test_code` rustc-span precision upgrade (Wave 3)** — body at `docs/TODO.md` §Polyglot Pipeline Follow-ups (`filter_test_code` rustc-span precision upgrade). See §Decisions item 1 for why the brace-counter landed in Wave 2 anyway.
- **`test-coverage --full` mode NOT ported to Wave 2** — body at `docs/TODO.md` §Polyglot Pipeline Follow-ups (`test-coverage --full` mode entry).
- **`ts-no-test-removal` block-count heuristic NOT ported** — body at `docs/TODO.md` §Polyglot Pipeline Follow-ups (`ts-no-test-removal` block-count entry). Carries forward from task #37 main.md §Tech Debt #2.
- **dt-guard subcommand-sprawl re-debate trigger** — body at `docs/TODO.md` §Polyglot Pipeline Follow-ups (dt-guard subcommand-sprawl re-debate trigger entry). Wave 2 lands subcommand 23 (total 31); ADR-0034 §When-to-Revisit ≥10 trigger fired; deferred to a focused `/debate` when one of three escalation triggers fires.
- **`cross-boundary-scope` row-level user-story tightening — rolled back to bash whole-file exemption** — body at `docs/TODO.md` §Polyglot Pipeline Follow-ups (`validate-cross-boundary-scope` user-story exemption entry; that row was extended 2026-05-21 to note the Wave-2 deferral). See §Decisions item 4 + §Tightenings during port item 1 for in-loop context.

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `abf844e1037b93fb5361d2e8f341f3b16fed0f86`
2. Review all changes: `git diff abf844e..HEAD`
3. Soft reset (preserves changes): `git reset --soft abf844e`
4. Hard reset (clean revert): `git reset --hard abf844e`

---

## Issues Encountered & Resolutions

### Issue 1: 92% false-positive rate on Phase-2 secret/PII guards
**Problem**: First spot-test of the 4 secret/PII Rust subcommands surfaced 25 / 10 / 20 / 8 violations vs. bash's 2 / 0 / 0 / 5 — 92% FP rate on `rust-no-hardcoded-secrets`. Root cause: `filter_test_code` was deferred to Wave-3 per @security Q2 fold-in, leaving inline `#[cfg(test)] mod tests` blocks (containing literal pattern definitions) exposed to the scan.
**Resolution**: Pulled the brace-counter `#[cfg(test)]` test-block filter forward into Wave 2 (§Decisions item 1). Subsequent runs: 0 / 0 / 0 / 1 — the single residual is a coincident bash+Rust self-match later resolved structurally via §Decisions item 6 (`is_guard_internal_path` exclusion).

### Issue 2: Bare `address` PII token blast radius
**Problem**: @observability Q1 + @security F1 widened CATEGORY_B PII vocab to include bare `address`. Empirical scan during Phase-2 fix found 3 production FP sites (`gc-service::config:314`, `ac-service::crypto:876`, `devloop-helper::ports:744`) — `listen_address` / `metrics_address` / `host.containers.internal` config plumbing.
**Resolution**: Swapped bare `address` for `ip_address`/`email_address`/`mac_address` (§Decisions item 2). @security self-corrected on the Gate-1 Q1 scope and co-signed the swap.

### Issue 3: `cross_boundary_scope` over-engineered with row-level diff parsing + real-git tests
**Problem**: Implementer's first cut of `cross_boundary_scope` implemented the row-level user-story exemption tightening as a 3-bucket predicate over diff hunks with `tempfile::TempDir` + real `git init`/`add`/`commit` integration tests. User flagged the test approach as over-engineered.
**Resolution**: Two-step rollback per user direction. (a) Drop the tightening entirely — preserve bash whole-file exemption verbatim (§Decisions item 4); the 2026-05-19 absorption finding stays open under `docs/TODO.md`. (b) Restructure as pure-function policy + thin orchestrator; tests at the pure-function level with hand-built string inputs. Then extended to a SoT-only-git rule (§Decisions item 5) consolidating `Command::new("git")` to `common::git_changes` across all policy modules.

### Issue 4: `validate-metric-coverage` divergence vs bash
**Problem**: Rust port reported 4 mh-service metrics as uncovered; bash reported clean. Initial classification as "pre-existing" was wrong.
**Resolution**: Root-caused to bash's single-line-only emission-extraction regex. The 4 metrics use multi-line `counter!(...)` invocations; bash silently skipped them. Rust port uses `(?s)` DOTALL via `MACRO_INVOCATION_WITH_FIRST_ARG_RE` — strictly better than bash. Closed real coverage gaps via 11 new component tests (§Decisions item 7). Triggered the regex inheritance audit (§Decisions item 8).

### Issue 5: `validate-kustomize` Rust port missed R-15/R-17/R-18/R-19
**Problem**: Original Rust port at 302 LoC vs bash's 507; only R-16 + R-20 implemented. `detect_kustomize_tool` probed for tools but never invoked `kustomize build` to render manifests for the missing checks. R-18 (security-context regression detection) had real security weight.
**Resolution**: User authorized fix-in-loop. New `kustomize_tools.rs` (472 LoC) implements the 4 missing checks + tool detection + per-check WARN-on-tool-absent fallback matching bash (§Decisions item 10).

### Issue 6: Implementer stall pattern after LLM rate-limit incidents
**Problem**: Twice during this devloop (post-layer-all-stall around 03:00 2026-05-21; post-Gate-2-stall around 22:20 2026-05-22), the implementer agent stopped processing assigned work without sending a verdict or completion signal. Both times, only Lead intervention (file-system spot-check + explicit re-ping) restarted progress.
**Resolution**: User-side workaround — bump `CLAUDE_CODE_MAX_RETRIES` + `API_TIMEOUT_MS` in `~/.claude/settings.json` to survive longer rate-limit incidents. Lead-side workaround in future devloops — periodic file-system spot-checks instead of trusting idle notifications as completion. Filed as feedback to Claude Code re documented subagent retry behavior + auto-resume + orchestrator stall detection.

### Issue 7: Mid-loop `emit_fail` double-emission discovery
**Problem**: Plan committed to `emit_fail("<stable-token>")` before `anyhow::bail!`. Initial implementation double-emitted STATUS=FAIL because `main.rs` catch-all also emits via `reason_token(&e)`.
**Resolution**: Refactored all Phase-1/Phase-2 modules to bail with a slugifiable message that flows through `reason_token`'s sluggify path. One-STATUS-per-failure invariant preserved. Pattern matches Wave-1's `alert_rules::run` precedent.

---

## Lessons Learned

1. **"Build it correctly now" beats "defer it" by a wide margin in this devloop's pattern.** Every theoretical deferral surfaced empirically within the same loop: brace-counter (§Decisions item 1), kustomize gaps (item 10), `address` blast radius (item 2), ts-name-guard multi-line (closed in-loop after user pushback). The silent-missing-validation cost dominates the overly-strict-early cost when the surface is internal tooling with comprehensive unit test coverage.

2. **Empirical validation against bash IS port-fidelity verification.** The Phase-2 92% FP rate, the metric-coverage divergence, and the kustomize gap were all caught only by running the Rust port against the live tree and diffing against bash. Plan-time review correctly cleared the architecture; only execution exposed the gaps. Spot-tests-vs-bash should be a standard Phase-N exit criterion in any future port-devloops.

3. **The "guards don't scan themselves" structural rule subsumes a category of friction.** Hand-curated `CANONICAL_PATTERN_HOMES` + per-call-site annotations + `// guard:ignore(...)` comments all evaporate once the boundary is named. Future devloops touching guard tooling should default to extending `is_scan_exempt` over enumerating exemption sites.

4. **SoT-only-git rule produces testable code by construction.** Forcing `Command::new("git")` into a single helper module is what made the pure-function policy + thin-orchestrator pattern tractable across 4 cross-boundary modules. Same pattern likely transfers to any other external-tool dispatch (kubeconform, kustomize, kubectl) where the tool's exit shape can be encoded as a typed result.

5. **The collapsed-single-devloop trade-off accepted at user-story Revision-7 was the right call IN AGGREGATE but the cost compounded mid-loop.** Reviewability degraded — 8 reviewers had to absorb ~6,500 net LoC at one Gate, surface findings, re-verify after fix-ups, sometimes re-verify after subsequent in-loop scope additions. ~30 distinct findings across 8 reviewer panels. Future single-devloop large scope-creep absorption should explicitly budget for "Gate-3-finds-something" iteration overhead (~2-4 round-trips per reviewer) on top of the implementation budget.

6. **Bash had real blind spots that documented their own falsifiability conditions.** `validate-metric-coverage.sh` explicitly said "Single-line macro invocations only. multi-line macro invocations (none currently exist in source tree; the regex is single-line by design)" — the comment was true when written and became false later without anyone noticing. Rust port's multi-line awareness via `(?s)` DOTALL caught it. Future port-devloops should treat bash's hedge-comments as falsifiability conditions to test rather than design parameters to preserve.

7. **Cross-boundary classification table is load-bearing for Gate 2 framing.** The 67-row table at Gate 1 (post-classification-sanity-guard) was what let reviewers focus on policy logic instead of re-litigating who owns what. Reviewer-side challenges to classification were zero across the full panel — sign that the Gate-1 sanity guard + plan-template structure absorbed that conversation correctly.
