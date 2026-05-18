# ADR-0034: Guard Pipeline as Rust Binary (`dt-guard`)

**Status**: Accepted

**Date**: 2026-05-18

**Deciders**: Lead (synthesis); specialists infrastructure, security, operations, dry-reviewer, code-reviewer, test, observability (debate participants — see `docs/debates/2026-05-17-guard-toolchain-supersede/debate.md`). **Consensus reached 2026-05-18 with unanimous β pick (average satisfaction 84.3, all 7 specialists WOULD_ACCEPT=yes); cross-cutting specialists security/test/observability/operations all above ADR-0024 §5.7 70-floor.**

---

## Context

An earlier outcome from the 2026-05-14 debate (`docs/debates/2026-05-14-python-guard-pipeline-strategy/`) adopted a system-only-stdlib-Python posture with 5 structural meta-guards policing the carve-out and 5 tripwires deferring real toolchain decisions. On user reflection that outcome was structurally incoherent:

- 5 meta-guards police 1 Python lib module (355 LoC) — toolchain overhead inverted vs. the code it supported
- `Approved-Stdlib-Exception:` trailer is the rule's escape hatch, not its enforcement
- Python 3.11 build-time pin makes every Debian point-release an ADR amendment
- "Tripwire to future uv" defers real plumbing — we pay maintenance now for a posture we'll abandon when convenient
- "Python sometimes" is the worst pattern for adding new guards — every author forks on heredoc-vs-lib, bash-vs-python, does-this-break-a-tripwire

The user explicitly rejected half-measures, carve-outs, and tripwires-to-future-X. A successor debate (2026-05-17) constrained specialists to pick **one** toolchain and own it, scored on three criteria: consistent implementation pattern, ease of adding new guards, reliability of guard issue detection (positive AND negative).

### Workload character

The guard pipeline is **largely string/text parsing**: YAML/JSON structure, Rust source code (metric-macro extraction), Markdown (doc-citations), regex catalogs. Not numeric work; not async I/O. Eight guards under `scripts/guards/simple/` invoke `python3` today (six via inline heredocs; three via `scripts/guards/lib/doc_cite_extract.py`).

### Constraints

- ADR-0033's per-language wrapper convention at `scripts/lang/<X>/` (Rust, TS, Proto first-class)
- ADR-0033 §14 zero-flake budget for Layers 1-6; §90s p95 always-run wall-clock budget
- Cross-cutting specialist veto floor: any cross-cutting specialist below 70 satisfaction at consensus requires explicit user risk acceptance under ADR-0024 §5.7
- Lead committed publicly to NOT drive fast convergence (per user feedback on the prior debate); positions resolved by evidence, not pressure

### Four options debated

- **(α) Full Python**: uv + ruff + pip-audit + pytest. Python equivalent to Rust/TS/Proto.
- **(β) Rust binary `dt-guard`**: new crate at `crates/dt-guard/`. Subcommands. Cargo toolchain reused.
- **(γ) Pure bash + jq/yq**: rewrite all 8 Python-using guards. Single-language guard codebase.
- **(δ) Open**: TypeScript via existing pnpm/Node toolchain.

**γ received zero votes.** δ received one R1 vote (code-reviewer) which flipped to β in R2 once `serde_yaml` typed-deserialization with `deny_unknown_fields` was shown to be strictly stronger than `tsc --strict`. **α received two R1 votes (observability, test) which both flipped to β in R3** once their named falsifiable conditions were met with evidence.

## Decision

We adopt **β: a single Rust binary `dt-guard` at `crates/dt-guard/`**, exposing one subcommand per guard policy. Every shell guard becomes a 3-5 line wrapper that invokes `dt-guard <subcommand>` and emits per ADR-0033 §6 wrapper contract. Python exits the guard pipeline entirely.

**There is no carve-out to police; the toolchain enforces the invariants the meta-guards were synthesizing.** The five tripwires and four meta-guards from the prior debate's outcome all evaporate — not because they are replaced by equivalents, but because the boundary they were defending no longer exists. The single surviving sizing heuristic (subcommand count ≥10 → consider splitting into focused binaries; §When-to-Revisit) is not a tripwire or commitment device — it is the universal code-review judgment applied to any crate.

### 1. Crate Boundary

```
crates/dt-guard/
├── Cargo.toml                  # workspace member; deps: regex, serde, serde_yaml, walkdir, clap, once_cell, anyhow
├── src/
│   ├── main.rs                 # ~60 LoC: clap subcommand dispatcher + STATUS line emission
│   ├── lib.rs                  # ~20 LoC: pub mod declarations only
│   ├── cite_extract.rs         # ~400 LoC: doc-citation extraction + resolve_cited_path
│   ├── alert_rules.rs          # ~250 LoC: PromQL window walk + severity + for-clause policy
│   ├── metric_macros.rs        # ~180 LoC: counter!/gauge!/histogram! shared kernel
│   ├── dashboard_panels.rs     # ~200 LoC
│   ├── grafana_datasources.rs  # ~150 LoC: UID dedup + Loki-label half
│   ├── infrastructure_metrics.rs # ~100 LoC
│   ├── metric_labels.rs        # ~350 LoC: PII denylist + cardinality heuristic
│   ├── application_metrics.rs  # ~150 LoC
│   ├── common/
│   │   ├── mod.rs              # status emission, error types
│   │   ├── duration.rs         # parse_prometheus_duration + tests
│   │   ├── path_safety.rs      # containment gate (shared with cite_extract)
│   │   └── status.rs           # STATUS=OK|FAIL REASON=... emission helpers
│   ├── ignore.rs               # # guard:ignore marker parsing (hash + html flavors)
│   └── secret_patterns.rs      # ~60 LoC: 7-pattern HYGIENE_PATTERNS set (canonical home for secret-detection regex catalog; collapses cross-stack dupe per §6)
└── tests/
    ├── doc_cite_resolve.rs     # 7-case resolve_cited_path security suite (security veto-blocking)
    ├── cite_extract_parity.rs  # 17-case Python-vs-Rust extraction parity fixture (test commitment)
    └── ... (one per subcommand: end-to-end via assert_cmd)
```

**Library + thin binary**: `lib.rs` re-exports policy modules; `main.rs` is pure clap dispatch + STATUS emission with no business logic. Same shape as existing `crates/devloop-helper/`. Total estimated crate LoC: **~1050-1500** (infrastructure's revised aggregate per R2 port test).

### 2. Test Placement (Two-Tier)

- **Unit tests live inline at the bottom of each `src/<module>.rs`** in `#[cfg(test)] mod tests` blocks. Co-locates tests with the function under test. Idiomatic Rust; matches every existing Dark Tower crate.
- **Integration tests at `crates/dt-guard/tests/<scenario>.rs`** for cross-subcommand and binary-surface scenarios via `assert_cmd`. The 7-case `resolve_cited_path` security suite lands here; the 17-case Python-parity fixture lands here.
- **Security/veto-blocking suites that share a contract** (e.g., the 7-case `resolve_cited_path` suite, the 17-case parity fixture) land in `tests/` for discoverability — convention applies to fixture-table or invariant-table suites where the N-case framing is itself documentation. A contributor adding case 8 sees seven sibling test functions and follows the pattern.

**Zero new test-organization pattern.** `cargo test --workspace` (ADR-0033 Layer 4) runs everything. No separate `unittest discover` wire-in. **No new Layer-3 pre-step** as the prior outcome required.

### 3. Wrapper Shape

Every shell guard collapses to:

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DT_GUARD="${DT_GUARD:-$REPO_ROOT/target/release/dt-guard}"
[[ -x "$DT_GUARD" ]] || { echo "STATUS=FAIL REASON=dt-guard-binary-missing"; exit 1; }
"$DT_GUARD" <subcommand> --root "$REPO_ROOT" "$@"
```

~7 LoC. Author decisions for a new guard: (1) subcommand name + match-arm in `main.rs`, (2) typed struct hierarchy (= YAML schema, enforced by `#[serde(deny_unknown_fields)]`), (3) `check()` function body. **Three decisions vs. the prior outcome's 5-7.**

### 4. Lookbehind Resolution

Rust `regex` rejects lookaround by construction — that's the linear-time DFA guarantee and β's load-bearing reliability property.

Three patterns in `doc_cite_extract.py` use negative lookbehind (`_PATH_PREFIX` at line 39, plus its two consumers `BARE_LINE_CITE_RE` line 43 and `SYMBOL_CITE_RE` line 49). One additional small site exists at `validate-dashboard-panels.sh:193` which collapses to `\bMETRIC\b` directly (verify-at-PR-time per security review: confirm during Wave 2 Day 6 dashboard-panels port; if the site turns out non-trivial, apply the §4 boundary-class pattern as a third instance).

**Resolution: restructure via positive left-boundary class**, NOT adopt `fancy-regex`. The latter would forfeit the linear-time guarantee that distinguishes β from α on the security-relevant cite-extractor.

Concrete port (security R2 + test R3 verified):

```rust
const PATH_PREFIX: &str =
    r"(?:^|[\s\(\[\{`'\"<>,;=|])([A-Za-z_][\w./\-]*\.[a-z]{1,5})";

static BARE_LINE_CITE_RE: Lazy<Regex> = Lazy::new(||
    Regex::new(&format!(r"{PATH_PREFIX}:(\d+)(?:-(\d+))?\b")).unwrap()
);

static SYMBOL_CITE_RE: Lazy<Regex> = Lazy::new(||
    Regex::new(&format!(r"{PATH_PREFIX}::([A-Za-z_]\w*)\b")).unwrap()
);
```

Boundary class includes the 5-char expansion `,;=|` from test's R3 fixture audit (covers theoretical divergence cases that have zero present-tense instances in the production corpus). Caller-side adjustment is ~5 LoC: slice `m.get(1)` instead of `m.get(0)` to skip the non-capturing boundary char; column-offset reporting uses `m.get(1).unwrap().start()` (unchanged).

**Behavioral equivalence locked at PR time** via `crates/dt-guard/tests/cite_extract_parity.rs` — a 17-case Python-vs-Rust extraction-parity fixture (test commitment). Future restructuring cannot drift from prior Python behavior without breaking this test.

### 5. Containment Gate — `resolve_cited_path`

```rust
pub fn resolve_cited_path(repo_root: &Path, cited: &str) -> Option<PathBuf> {
    let abs_target = repo_root.join(cited);
    let resolved = std::fs::canonicalize(&abs_target).ok()?;
    let root_real = std::fs::canonicalize(repo_root).ok()?;
    if resolved == root_real { return Some(resolved); }
    if !resolved.starts_with(&root_real) { return None; }
    Some(resolved)
}
```

`Path::starts_with` is component-wise, not string-prefix — eliminates the `root_real + os.sep` footgun that the Python version needs. **`.ok()?` on `canonicalize` exercises the OSError-graceful branch via real filesystem behavior on dangling symlinks** — no `unittest.mock.patch` against CPython implementation details. Strictly stronger durability across stdlib version changes (per test R3 retraction of R1 "pytest mock best-in-class" framing).

Security's 7-case veto-blocking test set ports unchanged (case 6 OSError-graceful becomes a real dangling-symlink test, not a mock):

```rust
#[test] fn containment_positive() { /* tmpdir + real file */ }
#[test] fn traversal_escape_returns_none() { /* "../etc/passwd" */ }
#[test] fn absolute_path_escape_returns_none() { /* "/etc/passwd" */ }
#[cfg(unix)] #[test] fn symlink_escape_returns_none() { /* symlink → outside */ }
#[cfg(unix)] #[test] fn symlink_inside_resolves() { /* intra-repo symlink */ }
#[cfg(unix)] #[test] fn dangling_symlink_returns_none() { /* canonicalize Err */ }
#[test] fn cited_path_dot_resolves_to_repo_root() { /* "." → root_real */ }
```

`#[cfg(unix)]` replaces `os.supports_symlinks` skip-guard cleanly.

### 6. Structural Duplication — Impossible by Construction

dry-reviewer's R1 inventory of present-tense regex re-inlines (`LAZY_REASON_RE` literally re-declared at `validate-dashboard-panels.sh:96` AND `validate-metric-labels.sh:214` despite `doc_cite_extract.py:58` being the canonical home) **becomes a compile-time error** under β:

```rust
// crates/dt-guard/src/ignore.rs — canonical home
pub(crate) static LAZY_REASON_RE: Lazy<Regex> = Lazy::new(||
    Regex::new(r"^(test|tmp|todo|fix ?me|wip)\b").unwrap()
);
```

(Visibility is `pub(crate)`, not `pub` — the canonical home is shared across `dt-guard`'s internal modules but not exported to other workspace crates. Per infrastructure review.)

Workspace-level `clippy.toml`:

```toml
disallowed-methods = [
    { path = "regex::Regex::new",
      reason = "use a pub Lazy<Regex> static in the canonical module; see crates/dt-guard/src/ignore.rs" }
]
```

Cargo refuses to build a second `LAZY_REASON_RE` static. `Regex::new` calls outside the canonical home fail clippy. **No meta-guard required.** The prior outcome's `validate-guards-lib-import-discipline.sh` + the README regex-table fenced block + `validate-no-multiline-python-heredocs.sh` all evaporate.

`#[allow(clippy::disallowed_methods)]` scoped to canonical module initializers is the documented escape; reviewer verifies it isn't sprinkled.

**Cross-stack duplication also collapses.** Today `validate-alert-rules.sh:85-103` (Python `HYGIENE_PATTERNS` heredoc) and `no-hardcoded-secrets.sh:96,144` (bash regex literals) carry the same 7-pattern secret-detection set in two languages — dry-reviewer's R1 cross-stack finding. Under β both become consumers of `dt_guard::secret_patterns::HYGIENE_PATTERNS` — `validate-alert-rules.sh` consumes it natively via `dt-guard alert-rules-policy` (Wave 2 Day 3-5), and `no-hardcoded-secrets.sh` flips to a `dt-guard secret-scan` subcommand at Wave 3 Day 11. Dual-consumer, single-SoT Rust kernel; same pattern as `path_safety::resolve_cited_path` consumed by both cite-extract and alert-rules::validate_runbook_url. Without this Wave-3 step the §6 "Impossible by Construction" title would carry an asterisk; with it, the cross-stack dupe class also evaporates.

### 7. `dt-guard --explain` Debug Surface (Observability Commitment)

Every subcommand supports an `--explain <input>` flag that prints the matched span + the policy that flagged it + the source location in the crate. This replaces the `python3 -i` REPL workflow observability called out as a residual ergonomics concern under β.

Cheap to add at design time; trivially expensive to retrofit. Lands with the cite-extract subcommand (PR #1) so the pattern is set from day 1.

### 8. (D)-Complement Slate — Preserved

Per the per-guard vendor-coverage matrix at `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md` (load-bearing factual artifact from the predecessor debate — **not deleted**), two vendor-native complements earn their place as parallel gates:

| # | Guard / subcommand | Vendor tool | Shape under β |
|---|--------------------|-------------|---------------|
| D-1 | `dt-guard alert-rules-policy` | `promtool check rules` | Parallel gate. Subcommand wrapper invokes `promtool` first; on success runs bespoke Dark-Tower policy (runbook-URL path-traversal, severity allowlist, for-floor-with-expr-window-exemption, annotation hygiene, lazy-ignore). Both must pass. |
| D-2 | `dt-guard grafana-datasources` | Grafana provisioning dry-run | Parallel gate. UID-uniqueness + name-validity via `grafana cli --dry-run` against pinned `grafana/grafana@sha256:…` image. Loki-label cross-reference stays bespoke. |

D-3 (yq schema-validate sweep) remains dropped per observability matrix row analysis. D-4 (dashboard-panels) remains deferred contingent on D-2 success.

Acceptance criteria from the predecessor debate's analysis: fixture-injected bug Python misses AND zero false-positives on production artifacts before promotion from advisory to gate. **The "no wholesale-replace" contract** from observability's matrix row 1 footnote applies — `validate-alert-rules.sh::validate_runbook_url` is a path-traversal + symlink-escape gate, not a URL scan, and promtool does not cover it. Under D-1 the in-house traversal validator stays, refactored to consume `dt_guard::common::path_safety::resolve_cited_path` as the **single source of truth for containment** (same function the doc-citation guards use). Dual consumer, single SoT, single test suite.

### 9. Layer-3 Per-Guard Timeout

`scripts/guards/run-guards.sh` per-guard `timeout --kill-after="${GUARD_KILL_AFTER_SECS:-5}s" "${GUARD_TIMEOUT_SECS:-30}s"` wrapper **lands as strategy-independent hardening**; closes the Layer-3 hang vulnerability regardless of toolchain choice. Exit 124 → `STATUS=FAIL REASON=guard-timeout-<name>`; exit 137 → `STATUS=FAIL REASON=guard-timeout-kill-<name>`. Test owns the PR; operations reviews. (Carried over from the predecessor debate's analysis as one of the few strategy-independent recommendations that survived the reframe.)

### 10. Incremental Migration — 8 PRs over 2-3 Weeks

The migration is **guard-by-guard, not big-bang**. Each PR is independently revertable. `cargo test` (ADR-0033 Layer 4) and `cargo audit` (Layer 6) gate every PR from day 1.

| Day | Deliverable | Guards flipped |
|-----|-------------|----------------|
| 1-2 | `crates/dt-guard/` scaffold + `main.rs` + `lib.rs` + `common/` + `cite_extract` subcommand + `--explain` surface | 3 doc-citation guards (A, C, and `validate-alert-rules.sh`'s `is_lazy_reason` import) flip wrappers; `scripts/guards/lib/doc_cite_extract.py` + `__pycache__/` + `sys.path.insert` deleted in same PR |
| 3-5 | `alert-rules-policy` subcommand | `validate-alert-rules.sh` flips wrapper |
| 6 | `dashboard-panels` subcommand | `validate-dashboard-panels.sh` flips |
| 7 | `metric-labels` subcommand | `validate-metric-labels.sh` flips |
| 8 | `application-metrics` subcommand | `validate-application-metrics.sh` flips |
| 9 | `infrastructure-metrics` subcommand | `validate-infrastructure-metrics.sh` flips |
| 10 | `grafana-datasources` subcommand | `grafana-datasources.sh` flips |
| 11 | Remove `python3-yaml` apt from `infra/devloop/Dockerfile` | — |

Calendar time ~2-3 weeks with normal PR review cadence. **Working end-state on day 2** (cite-extract subcommand + 3 guards live).

## Consequences

### Positive

- **One mental model for guards.** Author writes a Rust module + subcommand entry; reviews like every other `crates/` member; tests run under `cargo test --workspace`. No fork between "heredoc or lib?", "bash or python?", "stdlib or PyPI?", "trip a tripwire?".
- **Re-inlining is structurally impossible.** dry-reviewer's three present-tense `LAZY_REASON_RE` re-inlines at HEAD become compile-time errors. `clippy.toml`'s `disallowed_methods` for `Regex::new` is ~5 LoC; the prior outcome's three meta-guards for the same property delete.
- **No ReDoS class.** Rust `regex` is linear-time DFA. The catastrophic-backtracking risk on `_PATH_PREFIX` that the prior outcome flagged is gone by construction, not bounded by SIGALRM + line-cap defenses.
- **Path-containment gate strictly stronger.** `std::fs::canonicalize` errors naturally on dangling symlinks; case 6 (OSError-graceful) is a real filesystem test, not a `unittest.mock.patch` mocking CPython implementation details. Test durable across stdlib version changes.
- **Compile-time YAML schema validation.** `serde_yaml` + `#[serde(deny_unknown_fields)]` + typed enums catches alert-rule schema drift at parse time, not at regex-search time. Stronger than `tsc --strict` (Rust has no `any` escape) and unavailable under α without mypy bolt-on.
- **Toolchain inherited, not added.** Cargo + clippy + cargo-audit + cargo-test all already in the pipeline (ADR-0033 Layers 1/4/5/6). Zero new image surface; zero new trust chains; zero new audit gates.
- **Always-run budget cheapest of the four options.** ~100ms cold-start across 8 guards (native binary) vs α ~103ms (per test R2 measurement; equivalent in practice) vs δ ~640-960ms (Node startup × 8) vs γ varies but bash regex performance penalty dominates on multi-hundred-LoC walks.
- **Scaffolding collapses.** The Python `scripts/lang/python/` directory the prior outcome would have created is never built. No `_python_env.sh`, no pre-positioned audit stub, no `pyproject.toml`, no `uv.lock`, no `requirements.txt`. The `scripts/lang/<X>/` convention from ADR-0033 stays at three languages (Rust, TS, Proto); β is *not* a fourth language — it's a workspace member of the existing Rust toolchain.
- **All five tripwires from the prior outcome evaporate.** No PyPI-dep gating, no module-count threshold, no heredoc-size limit, no LoC ceiling, no interpreter-drift watch. The carve-out machinery that exists *because* α can't enforce its own invariants is unnecessary when the toolchain enforces them natively.

### Negative

- **One-time rewrite cost.** ~1050-1500 LoC of Rust ported from ~600-800 LoC of Python kernels + 355 LoC `doc_cite_extract.py`. Infrastructure's R2 port test came in at 1.03× on the function observability named as her falsifiable test; aggregate estimate 1.1-1.3×. **Calendar time 2-3 weeks** with incremental shipping (vs α's 3-5 days but with permanent toolchain footprint).
- **Rust author pool smaller than Python.** Future guard contributors must work in Rust. The repo's existing Rust crate count (5 services + multiple test-utils) means the contributor pool largely overlaps; not zero cost but bounded.
- **REPL-debugging ergonomics regress.** `python3 -i` interactive false-positive triage is not available under cargo. Mitigation: `dt-guard --explain <subcommand> <input>` debug surface (§7) replaces the REPL workflow for the common case of "show me what matched and why."
- **Lookbehind restructuring labor.** Three patterns require port-side restructuring with ~5 LoC caller-side adjustment + ~30 LoC of test scaffolding to lock boundary semantics. One-time cost in one module (`cite_extract.rs`), not a recurring tax on new guards.
- **YAML-deserialization ceremony.** `#[derive(Deserialize)]` struct hierarchies for alert-rule YAML / dashboard JSON are ~1.3-1.5× LoC of Python's dict-walk. Real loss-case; offset by compile-time schema validation Python can't offer.
- **`cargo build --release` Layer-1 budget cost.** dt-guard incremental cold build ~5-15s; ~0s warm with sccache. Operations to monitor; mitigation is workspace-split (`dt-guard-core` lib + `dt-guard` bin) only if modules cross 30, not preemptively.

### Neutral

- **`serde_yaml` maintenance status**: archived in 2024-03 by dtolnay. Pin to a maintained fork (`serde_yml` or `serde_norway`). Security flagged this in their R1; infrastructure to pick at PR #1 with explicit cargo-audit verification.
- **Subcommand sprawl risk by 2026-11**: bounded by naming discipline at the subcommand boundary — each subcommand names a single cross-cutting concern; catch-all names (`util`, `helpers`, `common`, `validate`) rejected at code-review. Tripwire-equivalent: ≥10 subcommands triggers re-debate on splitting into 2-3 focused binaries. **One** lightweight code-review checklist item vs. the prior outcome's five structural meta-guards.
- **The per-guard vendor-coverage matrix at `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md` remains canonical** for the (D)-complement slate. Not deleted; cited by §8.

## Implementation Status

### Wave 1 (Days 1-2): Foundation + cite-extract

| Section | Component | Status | Owner |
|---------|-----------|--------|-------|
| 1 | `crates/dt-guard/Cargo.toml` + workspace registration | ❌ Pending | infrastructure |
| 1 | `src/main.rs` clap dispatcher + STATUS line emission | ❌ Pending | infrastructure |
| 1 | `src/lib.rs` module declarations | ❌ Pending | infrastructure |
| 1 | `src/common/{mod,duration,path_safety,status}.rs` | ❌ Pending | infrastructure |
| 4 | `src/cite_extract.rs` + lookbehind restructure | ❌ Pending | infrastructure (pair: security on regex review, test on parity fixture) |
| 5 | `src/common/path_safety.rs` `resolve_cited_path` | ❌ Pending | security (paired with test on 7-case suite) |
| 7 | `--explain` debug surface on cite-extract | ❌ Pending | infrastructure |
| 2 | `tests/doc_cite_resolve.rs` (7-case security veto-blocking) | ❌ Pending | test (paired with security) |
| 2 | `tests/cite_extract_parity.rs` (17-case Python-vs-Rust) | ❌ Pending | test |
| 6 | Workspace `clippy.toml` `disallowed_methods` for `regex::Regex::new` | ❌ Pending | dry-reviewer |
| 3 | Flip 3 doc-citation guards to `dt-guard cite-extract` wrappers | ❌ Pending | infrastructure |
| 10 | Delete `scripts/guards/lib/doc_cite_extract.py`, `__init__.py`, `__pycache__/` rationale | ❌ Pending | infrastructure |
| 9 | `run-guards.sh` per-guard timeout (strategy-independent) | ❌ Pending | test (operations reviews) |

### Wave 2 (Days 3-10): Remaining subcommands, one PR each

| Day | Component | Status | Owner |
|-----|-----------|--------|-------|
| 3-5 | `src/alert_rules.rs` + `alert-rules-policy` subcommand + flip `validate-alert-rules.sh` | ❌ Pending | infrastructure (pair: observability on policy semantics, security on traversal-gate single-SoT) |
| 6 | `src/dashboard_panels.rs` + flip `validate-dashboard-panels.sh` | ❌ Pending | infrastructure (pair: observability) |
| 7 | `src/metric_labels.rs` + flip `validate-metric-labels.sh` | ❌ Pending | infrastructure (pair: observability, security on PII denylist) |
| 8 | `src/application_metrics.rs` + flip `validate-application-metrics.sh` | ❌ Pending | infrastructure (pair: observability) |
| 9 | `src/infrastructure_metrics.rs` + flip `validate-infrastructure-metrics.sh` | ❌ Pending | infrastructure (pair: observability) |
| 10 | `src/grafana_datasources.rs` + flip `grafana-datasources.sh` | ❌ Pending | infrastructure (pair: observability, operations on Grafana provisioning) |

### Wave 3 (Day 11): Cleanup

| Component | Status | Owner |
|-----------|--------|-------|
| Remove `python3-yaml` apt from `infra/devloop/Dockerfile` | ❌ Pending | infrastructure |
| Flip `no-hardcoded-secrets.sh` to consume `dt_guard::secret_patterns` via a new `dt-guard secret-scan` subcommand — closes the cross-stack `HYGIENE_PATTERNS` dupe per §6 | ❌ Pending | infrastructure (paired with security on pattern review, dry-reviewer on consolidation verification) |
| Operations runbook update: `docs/runbooks/devloop-validation.md` adds `dt-guard` triage section (§6.3.1 nested under existing §6.3) covering stale-binary, subcommand-not-found, and bona-fide policy-violation failure shapes | ❌ Pending | operations |

### Wave 4 (T+2, follow-up devloop): (D)-Complement slate

| # | Component | Status | Owner |
|---|-----------|--------|-------|
| D-1 | `dt-guard alert-rules-policy` invokes `promtool check rules` as parallel gate | ❌ Pending | operations + observability |
| D-2 | `dt-guard grafana-datasources` invokes `grafana cli --dry-run` against `grafana/grafana@sha256:…` | ❌ Pending | operations + observability |

Status values: ✅ Done | 🚧 In Progress | ❌ Pending | ⏸️ Deferred

## Alternatives Considered

- **System-only-stdlib Python + 5 meta-guards + 5 tripwires** (the predecessor debate's converged outcome, 2026-05-14). 7/7 specialist consensus on a hybrid: keep Python as a "second-tier" carve-out under `scripts/lang/python/` with stdlib-only-plus-apt-`python3-yaml` posture, five structural meta-guards (`validate-python-stdlib-only.sh`, `validate-no-multiline-python-heredocs.sh`, `validate-python-version-pin.sh`, `validate-guards-lib-import-discipline.sh`, `validate-python-test-coverage.sh`), and five tripwires (T-1 first PyPI dep, T-2 4th module, T-3 >500 LoC, T-4 heredoc >5 lines, T-5 CI-vs-devloop interpreter drift) deferring real toolchain plumbing. **Rejected on user reflection** as structurally incoherent: five meta-guards police one 355-LoC Python module (toolchain overhead inverted vs. the code it supported); the `Approved-Stdlib-Exception:` trailer is the rule's escape hatch, not its enforcement; the Python 3.11 build-time pin makes every Debian point-release an ADR amendment; tripwire-to-future-uv defers real plumbing for a posture intended to be abandoned when convenient. "Python sometimes" is the worst pattern for new-guard authors — every contribution forks on heredoc-vs-lib, bash-vs-python, does-this-break-a-tripwire. The successor debate (2026-05-17) constrained specialists to pick one toolchain and own it; both original-α voters (observability, test) flipped to β in R3 once their pre-stated falsifiable conditions were met. The half-measure pattern is foreclosed under β: there is no Python in the pipeline to carve out, no carve-out to police, and no meta-guards needed because the Rust toolchain enforces the invariants the meta-guards were synthesizing.

- **(α) Full Python (uv + ruff + pip-audit + pytest)**. R1: 2 votes. R3: 0 votes. Rejected once the load-bearing rewrite-cost argument was empirically refuted by infrastructure's port test (1.03× LoC ratio on the function observability named). The "α's lower one-time cost (3-5 days) vs β's higher one-time cost (2-3 weeks)" trade resolves toward β because α's lower numerator is over a *permanent* toolchain footprint (uv + ruff + pip-audit + version pin + custom ruff plugin for cross-file regex dedup + 4 meta-guards retained for the regex-canonical-home enforcement gap that ruff alone can't catch).

- **(γ) Pure bash + jq/yq**. R1: 0 votes. Rejected at debate open. Rewriting `validate-metric-labels.sh`'s 1009 LoC of PII denylist + cardinality heuristics in bash is, charitably, a 2× LoC expansion with worse test ergonomics. The `_PATH_PREFIX` lookbehind has no portable bash-regex equivalent (POSIX ERE forbids it; `grep -P` PCRE has ReDoS surface). Bash + bats-core for the 7-case symlink test has the worst mocking ergonomics of any option.

- **(δ) TypeScript via existing pnpm/Node toolchain**. R1: 1 vote (code-reviewer). R2: 0 votes (code-reviewer flipped δ→β). Rejected once code-reviewer worked through the comparison: `serde_yaml::from_str::<TypedSchema>` with `#[serde(deny_unknown_fields)]` is *strictly stronger* than `tsc --strict` because Rust has no `any` escape, exhaustive `match` on enums is enforced by the compiler vs TS's `never`-trick, and clippy::pedantic is a higher lint posture than eslint on every dimension both cover. Plus the repo-language-count consequence: δ adds TS to a pipeline that had bash + Python; β collapses to bash + Rust which the repo already has at scale (5 service crates).

- **β-with-fancy-regex**. Adopt the `fancy-regex` crate to preserve Python's lookbehind patterns verbatim. Rejected by security R2: `fancy-regex` is backtracking, has ReDoS surface, and adopting it for the security-sensitive cite extractor would collapse β's reliability=10 score to ~7 — same ReDoS class as α/δ. The DFA property is the security headline; trading it for porting convenience inverts the value proposition.

- **β with a tiny Python helper for one module**. Explicitly out of scope — this is the hybrid pattern the user rejected. If Python persists anywhere, the carve-out machinery comes back.

## Implementation Notes

- **`serde_yaml` fork selection** (security R1 concern): infrastructure picks between `serde_yml` (drop-in replacement, active maintenance) and `serde_norway` (alternative fork) at PR #1, with explicit cargo-audit verification on the selected crate's dep tree.

- **Lookbehind port is one-time, one module.** The restructuring cost lives in `cite_extract.rs` only. Other subcommands parse YAML/JSON via typed deserialization and do not use lookbehind. Guard authors writing new subcommands do not encounter lookbehind concerns.

- **17-case parity fixture is veto-blocking**. The `cite_extract_parity.rs` test locks the behavioral equivalence between today's Python and the Rust port. Future restructuring cannot drift without breaking this test. **Veto-blocking per ADR-0024 §5.7** — test specialist will reject PR #1 if missing. Test owns ongoing fixture-maintenance and case-addition discipline as new lookbehind-class patterns are added to dt-guard.

- **Test-reliability principle (test R3 retraction).** When evaluating test ergonomics across toolchain candidates, distinguish *ergonomics* (mock cleanliness, decorator sugar, fixture syntax) from *reliability* (does the test exercise the real production code path). **Reliability dominates.** The R1 framing of "pytest's `monkeypatch.setattr('os.path.realpath', side_effect=OSError)` is best-in-class" inverted the ranking — it scored the cleanest mock when the right scoring was *which toolchain doesn't need the mock*. Rust's `canonicalize` exercises the OSError branch via real dangling symlinks; the test pins behavior the implementation naturally exhibits. **This principle generalizes**: in future polyglot decisions, tests that pin natural failure modes beat tests that pin mocked failure modes.

- **clippy's `disallowed_methods` resolves by fully-qualified path**, not textual match. Authors writing `use regex::Regex; Regex::new(...)` are caught equivalently to `regex::Regex::new(...)` — clippy walks back to the canonical definition via the type-checker. Worth noting so a future contributor doesn't try to "harden" the lint with a textual `Regex::new` regex (which would be both weaker and brittle to formatter changes).

- **Per-policy fixture suite at `crates/dt-guard/tests/fixtures/`** (observability commitment): each policy ships with one positive + one negative fixture covering its named failure mode. Same shape as the (D)-complement acceptance criteria — fixture-injected bug + zero production false-positives — applied to bespoke policy code.

- **No `Approved-Stdlib-Exception:`-style trailer** under β. The toolchain enforces what trailers tried to police. Crates.io advisories surface via `cargo audit` (Layer 6); new dep additions require security review per ADR-0033 §11 (existing convention).

- **`dt-guard --explain <subcommand> <input>` debug surface** lands with the cite-extract subcommand (PR #1). Format: prints matched span + policy that flagged it + source-file location. This is the REPL-replacement for false-positive triage.

- **The runbook section for `dt-guard` triage in `docs/runbooks/devloop-validation.md`** is shorter under β than the prior outcome's distributed tripwire surface (per operations R1 analysis). One canonical section: "On `dt-guard <subcommand>` failure, reproduce locally with `dt-guard <subcommand> --explain <input>`; source at `crates/dt-guard/src/<subcommand>.rs`; STATUS/REASON format per ADR-0033 §6."

## When to Revisit

- **Subcommand count ≥10**: re-debate whether to split `dt-guard` into 2-3 focused binaries (e.g., `dt-cite`, `dt-yaml-policy`).
- **Cold-cache Layer 1 `cargo build` exceeds budget**: workspace-split (`dt-guard-core` lib + `dt-guard` bin) becomes load-bearing.
- **A new structured-text validation surface emerges that doesn't fit `regex` + `serde_yaml`** (e.g., a real PromQL parser is needed, not just window-walking). Evaluate `prometheus-http-api` or grammar-driven tools.
- **`cargo audit` flake rate or false-positive rate degrades**: revisit audit triage workflow.
- **Rust author pool issues at 6 months**: if guard contributions are blocked on Rust unfamiliarity in measurable ways, revisit the trade. (Mitigation candidates: better `--explain` ergonomics, expanded code-comment culture, pair-author conventions.)

## Participants

- **observability** (R1 86α → R2 78α → R3 **82β**): authored the vendor-coverage matrix that remains canonical. Held α through R2 on rewrite-cost grounds; named the falsifiable port test (`find_qualifying_expr_window`). **Flipped α→β in R3** after infrastructure's port came in 1.03× on the function she chose herself. Explicit honesty: *"My LoC trajectory was 3× → 1.5-2× → 1.1-1.3×. I was wrong, progressively less wrong, and now the right number is small enough that the rewrite-cost argument doesn't carry α."* Refused R2 alternative grounds as face-saving.

- **test** (R1 82α → R2 74α → R3 **81β**): the reliability voice. Raised the negative-lookbehind challenge to β as load-bearing in R1; explicit flip condition: "<20 LoC of Rust with passing fixture." **Flipped α→β in R3** after verifying security's port against the production corpus (17 cases, 13/13 byte-identical; 4/17 theoretical divergence with zero production instances). Retracted R1 "pytest mock best-in-class" framing on OSError-graceful: Rust `canonicalize` is *strictly stronger* than `unittest.mock.patch` mocking CPython implementation detail. Owns 17-case parity fixture commitment.

- **security** (R1 84β → R2 **82β**): held β throughout. Delivered the lookbehind port sketch (~15 LoC Rust + behavioral-equivalence walkthrough on `gc-service.dark-tower.svc.cluster.local:5432`) that met test's flip condition. Booked port cost honestly (ease 7→6). Names DFA-vs-backtracking as load-bearing security property; explicit rejection of `fancy-regex` as collapsing reliability=10.

- **operations** (R1 86β): held β with strongest aggregate score (9/9/9). Cheapest always-run cost (~100ms vs δ ~800ms). Runbook impact analysis: one canonical section shorter than the prior outcome's distributed tripwire surface. (D)-slate reshape: D-1 + D-2 become parallel-gate invocations from `dt-guard` subcommands.

- **infrastructure** (R1 86β → R2 **88β**): owns `crates/`. Delivered the falsifiable port test of `find_qualifying_expr_window` at 1.03× LoC ratio (35 Python → 36 Rust). Authored the crate-boundary spec. Named incremental migration path (8 PRs, each independently revertable). Aggregate estimate revised observability's R2 to 1.1-1.3× → ~1050-1500 LoC total.

- **code-reviewer** (R1 84δ → R2 **87β**): the consistency voice. **Flipped δ→β in R2** after working through the `serde_yaml` typed-enum comparison to `tsc --strict`: Rust's no-`any` escape + exhaustive `match` enforcement is structurally stronger, and the repo-language-count consequence cuts against δ. Explicit position-on-change: *"This is the kind of round-1 → round-1.5 update the debate protocol is supposed to surface."*

- **dry-reviewer** (R1 84β): held β with the strongest consistency score (10/10). The structural-duplication argument is theirs: present-tense regex re-inlines at HEAD (`LAZY_REASON_RE` at `validate-dashboard-panels.sh:96` AND `validate-metric-labels.sh:214`) become compile-time errors under β via `pub(crate) static Lazy<Regex>` + `clippy.toml disallowed_methods`. The prior outcome's anti-drift meta-guards "exist almost entirely because the underlying language strategy was incoherent."

## Debate Reference

See: `docs/debates/2026-05-17-guard-toolchain-supersede/debate.md` (the debate of record for this ADR)
See: `docs/debates/2026-05-14-python-guard-pipeline-strategy/debate.md` (predecessor debate; its converged outcome was reconsidered on user reflection and reframed under this ADR — preserved as historical record)
See: `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md` (canonical (D)-complement input from the predecessor debate; load-bearing factual artifact, not deleted)

## References

- ADR-0002 (`#[expect]`-over-`#[allow]` convention; applies to dt-guard naturally)
- ADR-0019 (DRY-reviewer cross-service duplication classification)
- ADR-0024 (Agent Teams workflow + §5.7 cross-cutting-specialist veto floor)
- ADR-0027 (Approved crypto / approved-default-only stance)
- ADR-0033 (Polyglot validation pipeline; β is *not* a fourth language — it's a workspace member of the existing Rust toolchain; ADR-0033's per-language wrapper convention stays at three languages)
- `infra/devloop/Dockerfile` (Wave 3 cleanup)
- `scripts/guards/run-guards.sh` (per-guard timeout from §9, strategy-independent)
- `crates/devloop-helper/` (existing Rust crate convention β mirrors)
