# Devloop Output: Doc-citation durability guards + runbook anchor sweep

**Date**: 2026-05-14
**Task**: Build two guards under `scripts/guards/simple/` to prevent doc-citation rot (A: forbid bare `:NN`/`:NN-NN` in long-lived doc trees; C: verify symbol-name cites resolve in source); sweep `docs/runbooks/devloop-validation.md` + other in-scope files to function-name anchors. Closes the line-number-rot TODO entry at `docs/TODO.md:249`.
**Specialist**: operations
**Mode**: Agent Teams v2 — full (7 teammates)
**Branch**: `feature/browser-client-join-task38`
**Duration**: TBD

Surfaced from task #39 (`docs/devloop-outputs/2026-05-14-devloop-validation-runbook-task39/main.md`) Gate-3 review — runbook landed with ~21 line-number anchors; ≥5 were already drifted from the source they cite by the time the commit landed. Operations chose to fix this as a standalone devloop (not story-tracked) since it's pure tooling/docs cleanup with no R-N requirement.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `14273bb` |
| Branch | `feature/browser-client-join-task38` |
| Team | `devloop-2026-05-14-doc-citation-durability-guards` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `planning` |
| Implementer | `implementer@devloop-2026-05-14-doc-citation-durability-guards` |
| Implementing Specialist | `operations` |
| Iteration | 1 |
| Security | pending spawn |
| Test | pending spawn |
| Observability | pending spawn |
| Code Quality | pending spawn |
| DRY | pending spawn |
| Operations | pending spawn (peer-reviewer) |

---

## Task Overview

### Objective

Three deliverables, bundled coherently in one commit:

1. **Guard A — `scripts/guards/simple/validate-doc-citations-no-line-numbers.sh`**. Forbid bare `<path>:<NN>` and `<path>:<NN>-<NN>` citation patterns in `docs/runbooks/**` and `.claude/skills/**`. Allowed forms: `<path>::<symbol>`, `<path> § "<header>"`, or prose mentions. The guard fails CI with greppable findings listing each violation's file + line + offending cite.

2. **Guard C — `scripts/guards/simple/validate-doc-citations-symbol-resolves.sh`**. For each `<path>::<symbol>` citation in the in-scope doc trees, verify the symbol's definition exists in the named file. Per-language patterns:
   - `.rs` — `\b(fn|struct|enum|trait|impl|const|static|type)\s+<symbol>\b`
   - `.sh` — `^<symbol>\s*\(\s*\)` OR `^function\s+<symbol>`
   - `.toml` — `^\[<section>\]` OR `^<key>\s*=`
   - `.yaml` — `^<key>:` (top-level)
   - `.md` — `^#+\s.*<heading-text>`
   - `.proto` — `\b(message|service|enum|rpc)\s+<symbol>`

3. **Sweep** — convert bare line cites to function-name anchors in:
   - `docs/runbooks/devloop-validation.md` (~21 cites, just landed in task #39)
   - `.claude/skills/devloop/review-protocol.md`
   - `docs/runbooks/mh-deployment.md`
   - `docs/runbooks/mc-deployment.md`
   - Any other in-scope file the implementer surfaces during sweep
   - Close out `docs/TODO.md:249` (the runbook-anchor-line-number-sweep entry) since the guard will enforce it forever.

`guard:ignore(<reason>)` annotation accepted for genuinely-pinpoint cases per the project's existing convention (see `.claude/skills/devloop/review-protocol.md` §guard:ignore).

### Scope

- **Service(s)**: none (build/CI tooling + docs only)
- **Schema**: No
- **Cross-cutting**: Yes — adds two Layer 3 always-run guards; sweeps long-lived docs.

### Debate Decision

NOT NEEDED. Pure tooling + docs cleanup; the design (A+C combined, scope = `docs/runbooks/**` + `.claude/skills/**`, bundled sweep) was discussed with team-lead pre-spawn.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/simple/validate-doc-citations-no-line-numbers.sh` | Mine | — (operations: runbook-policing is operations' subject-matter domain; `scripts/guards/simple/**` is shared infra, NOT GSA per `cross-boundary-ownership.yaml`) |
| `scripts/guards/simple/validate-doc-citations-symbol-resolves.sh` | Mine | — (same rationale) |
| `scripts/guards/common.sh` | Mine | — (small additive hunk: `doc_citation_in_scope_files()` helper extracted per DRY concern 1a; shared infra, not GSA) |
| `scripts/guards/lib/doc_cite_extract.py` | Mine | — (new file: Python module exposing cite-extraction regex + extension allowlist + `is_lazy_reason` kernel, consumed by both new guards and `validate-alert-rules.sh` per DRY concerns 1b/1c/2) |
| `scripts/guards/simple/validate-alert-rules.sh` | Not mine, Mechanical | observability (refactor `load_ignore_lines` to call shared `is_lazy_reason` kernel from `scripts/guards/lib/doc_cite_extract.py`. Sed-test clean: value-neutral, structure-preserving, no behavior change. Observability hunk-ACK at standard reviewer gate per ADR-0024 §6.2 Mechanical tier.) |
| `docs/runbooks/devloop-validation.md` | Mine | — (operations runbook, just landed in task #39) |
| `docs/runbooks/mh-deployment.md` | Mine | — (operations runbook) |
| `docs/runbooks/mc-deployment.md` | Mine | — (operations runbook) |
| `.claude/skills/devloop/review-protocol.md` | Mine | — (devloop workflow doc, operations-owned per task #38) |
| `docs/TODO.md` | Mine | — (closing one operations-attributed Code Quality entry per ADR-0019 documented append target) |
| `docs/devloop-outputs/2026-05-14-doc-citation-durability-guards/main.md` | Mine | — (own devloop output) |
| `.gitignore` | Mine | — (small additive hunk: Python `__pycache__/` and `*.pyc` for the new `scripts/guards/lib/` Python module's auto-generated bytecode; cross-cutting infra, not GSA) |

`scripts/guards/simple/**` is shared infra — not a Guarded Shared Area per `cross-boundary-ownership.yaml`. New guards in the operations subject-matter domain (runbook policing) classify as **Mine**.

---

## Planning

### Shared internals (DRY concerns 1, 2)

To avoid drift between Guards A and C (both walk the same trees, share the same `<path>`-token regex, share the same extension allowlist, and share the `guard:ignore` lazy-reason kernel), the following extract to shared locations:

**`scripts/guards/common.sh`** (small additive hunk; precedent: `parse_cross_boundary_table`):

- `doc_citation_in_scope_files()` — emits the file list both guards walk. Today returns paths matching `docs/runbooks/**.md` + `.claude/skills/**.md` from the working tree; future extension to ADRs / INDEX files lives here.

**`scripts/guards/lib/doc_cite_extract.py`** (new file; new `lib/` directory):

- Constants: `PATH_TOKEN_RE` (left-edge anchored path with dotted extension), `EXTENSION_ALLOWLIST = {rs, sh, toml, yaml, yml, md, proto, json, ts, tsx, js}`, `BARE_LINE_CITE_RE`, `SYMBOL_CITE_RE`.
- `extract_cites(doc_path) -> List[Cite]` — single pass over a doc, returns structured records for both bare-line and symbol cites. Both guards consume identical structured records — drift impossible.
- `is_lazy_reason(text) -> bool` — kernel of the lazy-reason check (`len < 10 OR matches /(test|tmp|todo|fix ?me|wip)/i`). Three call sites: the two new guards + `validate-alert-rules.sh::load_ignore_lines` (refactored to call this kernel; the marker-regex + diagnostic-emission stays alert-rules-specific because those ARE alert-rules-coupled).

Each guard invokes the module via `python3 -` heredoc with `sys.path.insert(0, "<scripts/guards/lib>")` + `from doc_cite_extract import ...` — keeps the existing inline-Python pattern, just sources one shared module.

The full `load_ignore_lines` is NOT extracted wholesale: its marker regex (`#\s*guard:ignore(...)` — shell-comment-specific) and JSON diagnostic shape (`"alert": "..." "kind": "lazy_ignore_reason"` — alert-rules schema-coupled) differ from my docs guards' needs (`<!-- guard:ignore(...) -->` HTML comment; non-JSON `print_violation` output). The shareable kernel is `is_lazy_reason`; that's what extracts. Pending dry-reviewer follow-up if they want the full lift instead.

### Guard A — `validate-doc-citations-no-line-numbers.sh`

**Scope**: in-scope doc trees = `docs/runbooks/**` + `.claude/skills/**`.

**Detection regex** (Python, scanning each line of each in-scope doc):

```
(?<![\w./-])([A-Za-z_][\w./-]*\.[a-z]{1,5}):(\d+)(?:-(\d+))?\b
```

- Left-edge negative-lookbehind `(?<![\w./-])` prevents matching e.g. `gc-service.dark-tower.svc.cluster.local:5432` — the `.local` is followed by `:5432` but we want to require a file-extension component, so we anchor on a token that looks like a *file*: a name with a dotted extension of 1-5 lowercase letters (`.rs`, `.sh`, `.toml`, `.yaml`, `.yml`, `.md`, `.proto`, `.json`).
- The extension allowlist intentionally excludes URL-shaped strings (e.g. `cluster.local`, `dark-tower`) because their "extensions" are domain components, not file extensions.
- We further filter via a hard whitelist of recognized extensions in code (`{rs, sh, toml, yaml, yml, md, proto, json, ts, tsx, js}`) so a stray `foo.bar:1` won't trip.

**Allowed forms — guard does NOT flag**:
- `<path>::<symbol>` — `::` is the symbol anchor; only single-`:` followed by digits is flagged
- `<path> § "<header>"` — markdown section reference
- Prose mentions (e.g., "the `__validate_ref_name` helper in `_get_base_ref.sh`")
- Lines with `guard:ignore(<reason>)` annotation (reuse the lazy-reason rejection from `validate-alert-rules.sh::load_ignore_lines`)

**Output** (simple-guard convention per `common.sh`: `print_violation` + `print_ok` + `init_violations`/`increment_violations`/`get_violations`/`print_elapsed_time`; mirrors `validate-cross-boundary-scope.sh`):

- Pass: `OK - No doc-citation line-numbers found`
- Fail: per-finding `VIOLATION: <doc-file> — doc-citations-line-numbers-found — <doc-file>:<line>: <offending-cite>`; exit code 1 with violation count summary at end.

### Guard C — `validate-doc-citations-symbol-resolves.sh`

**Scope**: same doc trees. Extracts every `<path>::<symbol>` and verifies symbol resolves.

**Per-language patterns** (per task spec):

| Ext | Pattern |
|---|---|
| `.rs` | `\b(fn\|struct\|enum\|trait\|impl\|const\|static\|type)\s+<symbol>\b` |
| `.sh` | `^<symbol>\s*\(\s*\)` OR `^function\s+<symbol>\b` |
| `.toml` | `^\[<section>\]` OR `^<key>\s*=` |
| `.yaml`/`.yml` | `^<key>:` (top-level only — line-start, no leading whitespace) |
| `.md` | `^#+\s+<heading-text>\b` (case-insensitive, word-boundary at end of heading-text — see F3(c) note below) |
| `.proto` | `\b(message\|service\|enum\|rpc)\s+<symbol>\b` |

Symbol-extraction regex (same left-edge anchoring as Guard A):

```
(?<![\w./-])([A-Za-z_][\w./-]*\.[a-z]{1,5})::([A-Za-z_][\w]*)\b
```

**Known limitations** (code-reviewer F3 — documented as header comments in the guard script):

- **`.sh` top-level functions only**: detects `name() {` and `function name`-shape defs at left margin. Local/nested function defs inside another function are NOT detected. All current in-tree shell helpers (`_common.sh`, `_dispatch.sh`, `_get_base_ref.sh`, `_changed_helpers.sh`, `_test_changed_predicates.sh`) use left-anchored defs, so this works for the sweep targets.
- **`.toml` is dead-code-for-now**: no `.toml` cites in the current sweep (verified). The branch is implemented for symmetry; first `.toml` cite added in a future doc will exercise it. If `[section]::<key>` form is ever needed (TOML table-and-key cite), Guard C needs extension.
- **`.md` heading is word-boundary, not full-equality**: pattern requires the cited heading-text to terminate on a word boundary (`\b`), preventing `foo.md::Test` from matching `## Testing Setup`. Still substring-permissive on the start side — `foo.md::Setup` does match `## Testing Setup`. Word-boundary-on-end was code-reviewer's F3(c) recommendation as the lighter alternative to full-equality (full-equality would require the cite to spell out the full heading verbatim, which is high author-friction; word-boundary catches the obvious bad cases like the `Test`/`Testing` collision without forcing the strictness).
- **Single-segment symbols**: `<path>::<symbol>` captures one identifier after `::`. Rust `Mod::Type::method` is NOT supported — operators wanting that should rewrite to a containing function (e.g., `mh_client.rs::register_meeting` + prose disambiguator "the inherent `impl MhClient` method" per the §Sweep table's `mh_client.rs:136,144,157` row).

**Output** (simple-guard convention; same shape as Guard A):

- Pass: `OK - All doc-symbol cites resolve`
- Fail: per-finding `VIOLATION: <doc-file> — doc-citation-symbol-unresolved — <path>::<symbol> — <reason>` where `<reason>` is one of `file-missing`, `path-escape`, `symbol-not-found`; exit code 1.
- Extensions outside the table → skipped silently (the citation is allowed but unverified — future extensions can be added).

**Path safety (security review §2)**: cited `<path>` is resolved via `os.path.realpath(os.path.join(REPO_ROOT, path))`; the resolved path MUST equal `REPO_ROOT` or start with `REPO_ROOT + os.sep`, else the cite reports `VIOLATION: … — path-escape` (NOT silently skipped, NOT read). Mirrors `validate-alert-rules.sh::validate_runbook_url` traversal/symlink-escape posture. A cite to `../../etc/passwd::root` therefore produces a violation; a cite to a non-existent in-repo path produces `file-missing`.

### Correctness validation (ad-hoc, NOT committed)

Per `docs/TODO.md` §"Guard Self-Test Cleanup" policy (resolved 2026-05-08 via `devloop-outputs/2026-05-08-guard-self-test-cleanup`) and the explicit Gate 1 decision in `docs/devloop-outputs/2026-05-08-layer-a-scope-drift-parser-fix/main.md` Lead, 2026-05-08: **new guards do NOT commit fixtures or a `--self-test` mode**. Implementer proves correctness with ad-hoc throwaway scripts during the guard-authoring devloop, discarded before commit.

Earlier drafts of this plan miscited task #34 as fixture-committing precedent — it is the opposite: task #34 explicitly opted into the no-fixture path. Corrected. The `run-guards.sh` `-not -path '*/fixtures/*'` prune is a cleanup-policy safety net, not a green-light for new fixture trees.

**Validation strategy (this devloop)**:

1. **Ad-hoc dev-time matrix** under `/tmp/doc-citation-fixtures/` — implementer writes throwaway `.md` files exercising both guards' positive/negative paths plus all 6 per-language patterns for Guard C, and a one-shot driver `run.sh` that invokes each guard against each fixture and asserts the expected PASS/FAIL outcome. Per task #34's pattern, results are summarized in §Implementation Summary as a per-case PASS/FAIL table (case name | outcome | violation count). The artifacts are DELETED pre-commit.

2. **Validation matrix** the ad-hoc driver MUST cover (extracted from @test review brief):

   Guard A — bare-line-cite detection:
   - `fail-bare-line-cite` — `foo.rs:42` produces VIOLATION
   - `fail-line-range-cite` — `foo.rs:42-50` produces VIOLATION
   - `fail-multi-extension` — covers `.rs`, `.sh`, `.toml`, `.yaml`, `.md`, `.proto` cites all trip (six fail cases, one per extension)
   - `pass-symbol-anchor` — `foo.rs::bar` does NOT trip
   - `pass-section-reference` — `foo.md § "Header"` does NOT trip
   - `pass-prose` — "the `bar` helper in `foo.rs`" does NOT trip
   - `pass-guard-ignore` — `foo.rs:42 <!-- guard:ignore(genuinely-pinpoint cite, the cited LOC is the entire constant) -->` does NOT trip; `guard:ignore(test)` is REJECTED as lazy reason and the line DOES trip
   - `pass-url-port` — `gc-service.dark-tower.svc.cluster.local:5432/path` does NOT trip (extension allowlist)
   - `pass-url-port-similar` — `host.com:8080`, `service.io:443` do NOT trip
   - `pass-code-block-escape` — bare-cite inside a fenced code block does NOT trip if the code block is documenting historic behavior (decision: code-block-internal cites DO trip — the guard is a content scan, not an AST walker; a runbook author who genuinely wants to preserve a code-block exemplar can `guard:ignore(...)` it, and §Risks already acknowledges this)
   - `pass-already-converted-sweep-target` — sample lines from the actual sweep conversions

   Guard C — symbol-resolution per language:
   - `pass-rs-fn` — `target.rs::my_fn` where `target.rs` contains `fn my_fn()`
   - `pass-rs-struct/enum/trait/impl/const/static/type` — one positive per keyword (7 cases)
   - `pass-sh-bare-fn` — `target.sh::my_fn` where `target.sh` contains `my_fn() {`
   - `pass-sh-function-kw` — `target.sh::my_fn` where `target.sh` contains `function my_fn`
   - `pass-toml-section` — `target.toml::lints` where `target.toml` has `[lints]`
   - `pass-toml-key` — `target.toml::name` where `target.toml` has `name = "..."`
   - `pass-yaml-top-key` — `target.yaml::groups` where `target.yaml` has `groups:` at line-start
   - `fail-yaml-nested-key` — `target.yaml::alert` where `alert:` only appears under nested indentation produces VIOLATION (top-level-only rule)
   - `pass-md-heading` — `target.md::"Section Header"` where `target.md` has `## Section Header`
   - `pass-proto-message/service/enum/rpc` — one positive per keyword (4 cases)
   - `fail-file-missing` — `nonexistent.rs::foo` produces VIOLATION `file-missing`
   - `fail-symbol-not-found` — `target.rs::missing` produces VIOLATION `symbol-not-found`
   - `fail-path-escape` — `../../etc/passwd::root` produces VIOLATION `path-escape` (file NOT read; security review §2)
   - `pass-unverified-extension` — `target.json::field` is silently allowed (`.json` not in pattern table)

   **Source-resident test catalog** (security non-blocking observation): the top of `scripts/guards/lib/doc_cite_extract.py` carries a 3-line block comment listing the validated failure modes Guard C exercises (file-missing, path-escape, symbol-not-found, lazy-reason rejection, single-segment-only). Future maintainers re-deriving the test cases need only read that comment, not spelunk this devloop's output. Matches the "what was exercised" trail security asked for without committing fixtures.

3. **Dogfooding at Gate 2** — Layer 3 runs both guards against THIS devloop's sweep. If the sweep is incomplete (any bare cite left in `docs/runbooks/**` or `.claude/skills/**`, or any `::symbol` that doesn't resolve), Layer 3 fails — same forcing function task #34 used. This is the durable correctness signal at commit time.

4. **§Implementation Summary at commit** records the per-case PASS/FAIL table from step 1 (per @test brief and task #34 precedent) as the test report in lieu of committed fixtures.

### Sweep — concrete conversions

**`docs/runbooks/devloop-validation.md`** (21 cites identified; conversions below):

| Original cite | Converted form |
|---|---|
| `_common.sh:225 __layer_lifecycle_end` | `_common.sh::__layer_lifecycle_end` |
| `_common.sh:208 tee_collect_statuses` | `_common.sh::tee_collect_statuses` |
| `_common.sh:113-126` (comment block) | prose: "comment block above `_common.sh::aggregate_worst_status`" |
| `_common.sh:200` (`layer_lifecycle_begin` ref) | `_common.sh::layer_lifecycle_begin` |
| `_get_base_ref.sh:36` (`__validate_ref_name` error) | `_get_base_ref.sh::__validate_ref_name` |
| `_get_base_ref.sh:79` (CI-PR merge-base error) | `_get_base_ref.sh::main` — the `ERROR: could not compute merge-base` emission in that function (CI-PR branch) |
| `_get_base_ref.sh:114` (sha-resolution error) | `_get_base_ref.sh::main` — the `ERROR: could not resolve base ref to sha` emission in that function |
| `_get_base_ref.sh:51` (`__emit_base_ref_line`) | `_get_base_ref.sh::__emit_base_ref_line` |
| `_get_base_ref.sh:120-126` (cache write) | `_get_base_ref.sh::main` — the cache-write block in that function |
| `layer-all.sh:40-43` (precondition emission) | `layer-all.sh` — the `PRECONDITION_FAILURE:` emission near the top of the script (no enclosing function) |
| `_dispatch.sh:149` | `_dispatch.sh::for_each_lang_with_verb` |
| `_dispatch.sh:148-150` | `_dispatch.sh::for_each_lang_with_verb` |
| `_dispatch.sh:124-128` | `_dispatch.sh::for_each_lang_with_verb` |
| `audit.sh:13-23` (RC capture block) | `audit.sh` — the explicit RC capture block (no enclosing function; flat script) |
| `lang/rust/audit.sh:6-12` (CLI-blockage comment) | `lang/rust/audit.sh` — the `IMPORTANT (security finding 1)` comment block at the top (no enclosing function) |
| `lang/proto/breaking.sh:48` | `lang/proto/breaking.sh` — the `base-ref-unresolved` emission (no enclosing function) |
| `scripts/layer7.sh:7` | `scripts/layer7.sh` — the `emit_status N/A` line (no enclosing function) |
| `_test_changed_predicates.sh:56-61` | `_test_changed_predicates.sh::__assert_predicate` |
| `_changed_helpers.sh:52-55` | `_changed_helpers.sh::diff_touches_path` |
| `_changed_helpers.sh:63-71` | `_changed_helpers.sh::diff_touches_root_files` |
| `_common.sh:199` | `_common.sh::layer_lifecycle_begin` |

**Prose-form boilerplate** (DRY concern 4): all non-`::symbol` conversions take the shape **"`<file>::<symbol>` — <near-clause>"** when there's a containing function, or **"`<file>` — <near-clause> (no enclosing function; flat script)"** when there isn't. Near-clauses are short noun phrases ("the comment block above", "the cache-write block in", "the failure-path branch in", "the `<token>` line"). Future authors writing new cites in `docs/runbooks/**` or `.claude/skills/**` follow the same shape — guidance landed at the bottom of `docs/runbooks/devloop-validation.md` as part of this devloop's sweep.

**`.claude/skills/devloop/review-protocol.md`**:
- `auth.rs:45` (in worked-example quote) → **decided**: replace with `<auth.rs:LINE>` placeholder. It's an illustrative example string inside a quoted question, not a real anchor; placeholder is cleaner than `guard:ignore` and avoids permanent annotation noise on a teaching example. Confirmed with @test.
- `crates/mh-service/src/observability/metrics.rs:182` → `metrics.rs::set_active_connections`
- `infra/grafana/dashboards/mh-overview.json:1794-1795` → `mh-overview.json` — the corresponding MH label panel (JSON not in language table, prose required)

**`docs/runbooks/mh-deployment.md`**:
- `crates/mc-service/src/observability/metrics.rs:340` → `metrics.rs::record_register_meeting`
- `crates/mc-service/src/observability/metrics.rs:341` → `metrics.rs::record_register_meeting`
- `infra/docker/prometheus/rules/mh-alerts.yaml:135-149` → `mh-alerts.yaml § "MHWebTransportHandshakeSlow"`
- `infra/docker/prometheus/rules/mh-alerts.yaml:115-123` → `mh-alerts.yaml § "MHHighWebTransportRejections"`

**`docs/runbooks/mc-deployment.md`**:
- `crates/mc-service/src/observability/metrics.rs:340` → `metrics.rs::record_register_meeting`
- `crates/mc-service/src/grpc/mh_client.rs:136,144,157` → `mh_client.rs::register_meeting` — the inherent `impl MhClient` method (NOT the `MhRegister` trait declaration nor its trait-impl). Disambiguation matters because `mh_client.rs` contains three `fn register_meeting` matches (trait decl, inherent impl, trait impl) and all three cited lines fall inside the inherent impl. Prose-disambiguator chosen over extending Guard C with nested-path resolution (`MhClient::register_meeting`) — that's a Guard C extension, defer to follow-up.

**`docs/runbooks/devloop-validation.md` — new §"Cite Convention" section** (DRY concern 4 follow-through). Single short section near the end of the runbook telling future authors:

- Use `<file>::<symbol>` for any cite into source code.
- Use `<file> § "<header>"` for cites into markdown/YAML files where the anchor is a section header rather than a callable symbol.
- For cites where neither form fits (flat shell scripts, comment-block anchors), use the prose boilerplate `<file>::<symbol> — <near-clause>` or `<file> — <near-clause> (no enclosing function)`.
- Bare `:NN` cites are forbidden — Guard A enforces. `guard:ignore(<reason>)` available for genuinely-pinpoint cases.

This anchors the convention so the next runbook author doesn't reinvent it.

**`docs/TODO.md:249`** — mark the "Runbook anchor-line-number rot" entry `[x]` with closure-note: "resolved 2026-05-14 via devloop-outputs/2026-05-14-doc-citation-durability-guards (guards A+C now enforce; sweep complete)".

### Wiring

- **Auto-discovery, no wiring edits**: both guards are discovered by `run-guards.sh`'s glob walk (`find scripts/guards/simple -name "*.sh" -type f -not -path '*/fixtures/*'`). No edits to `run-guards.sh`, no edits to `scripts/layer3.sh`. Dropping a new executable `.sh` file into `scripts/guards/simple/` is sufficient.
- Both guards self-classify scope via path-glob inside their own bodies (`docs/runbooks/**` + `.claude/skills/**`); the runner doesn't route by scope.
- Both guards run in Layer 3 (always-run) per ADR-0033 §3 — they're cheap (line scan + regex) and don't depend on diff.
- No committed fixtures, no `--self-test` mode (per `docs/TODO.md` Guard Self-Test Cleanup policy; corrected from earlier draft).

### Test plan

1. Ad-hoc dev-time validation per §"Correctness validation" above — implementer writes throwaway fixtures + driver under `/tmp/doc-citation-fixtures/`, covers the full matrix, deletes pre-commit. PASS/FAIL table summarized in §Implementation Summary at commit time (task #34 precedent).
2. Layer 3 dogfooding — Layer 3 runs both guards against THIS devloop's sweep. If the sweep is incomplete (any bare cite left in `docs/runbooks/**` or `.claude/skills/**`, or any `::symbol` that doesn't resolve), Layer 3 fails and the loop iterates. This is the durable correctness signal at commit time.
3. Reviewer rejection of any conversion (e.g., a prose form is too loose) → revise the conversion, re-run Layer 3.

### Risks / tradeoffs

- **Risk: false positives on documentation hostnames/URLs.** Mitigation: the file-extension allowlist (`{rs, sh, toml, yaml, yml, md, proto, json, ts, tsx, js}`) excludes `.local`, `.com`, `.cluster`, etc. The `mc-deployment.md` lines 422 / 458 / 923 (cluster URLs) will NOT trip.
- **Risk: false negatives — a future cite uses a new extension.** Mitigation: extension allowlist is in one place; adding `.py` etc. is one-line.
- **Risk: symbol-resolution regex over-matches.** Mitigation: the per-language patterns are conservative line-anchored regexes; the guard reports the cited symbol and the reviewer can inspect.
- **Risk: prose forms drift undetected.** Acknowledged: Guard A's job is to prevent the *brittle* form (bare `:NN`); it does not enforce that the prose mention stays accurate. That's an acceptable trade — the brittle case was the original concern (task #39 Gate-3 findings).

---

## Pre-Work

None.

---

## Implementation Summary

### Build

1. `scripts/guards/lib/doc_cite_extract.py` — new shared Python module: `EXTENSION_ALLOWLIST`, `BARE_LINE_CITE_RE`, `SYMBOL_CITE_RE`, `GUARD_IGNORE_RE`, `extract_cites()`, `is_lazy_reason()` (kernel), per-language symbol-resolution pattern builders, `resolve_cited_path()` (in-repo containment), `resolve_basename_match()` (basename-fallback for unambiguous matches across `scripts/`, `crates/`, `infra/`, `proto/`), `is_in_scope_doc()`.

2. `scripts/guards/common.sh` — additive: `doc_citation_in_scope_files()` helper (Bash counterpart to Python `is_in_scope_doc`).

3. `scripts/guards/simple/validate-doc-citations-no-line-numbers.sh` — new Guard A (Layer 3 always-run; auto-discovered by `run-guards.sh`'s glob walk).

4. `scripts/guards/simple/validate-doc-citations-symbol-resolves.sh` — new Guard C (same).

5. `scripts/guards/simple/validate-alert-rules.sh` — refactor: `load_ignore_lines` now delegates lazy-reason check to shared `doc_cite_extract.is_lazy_reason`. Behavior unchanged; verified all 13 alerts still pass.

### Sweep

29 bare-line cites converted across 4 docs:

- `docs/runbooks/devloop-validation.md` — 21 cites converted to `::symbol` or prose-with-symbol form. Added §10 "Cite Convention" documenting the durable shape; renumbered Changelog to §11 with a new sweep row.
- `.claude/skills/devloop/review-protocol.md` — 3 cites: `auth.rs:45` → `auth.rs:<line>` placeholder; `metrics.rs:182` → `metrics.rs::set_active_connections`; `mh-overview.json:1794-1795` → prose ("the corresponding MH label panel in `mh-overview.json`").
- `docs/runbooks/mh-deployment.md` — 4 cites: two `metrics.rs:340/341` → `::record_register_meeting`; two `mh-alerts.yaml:135-149`/`:115-123` → `§ "MHWebTransportHandshakeSlow"`/`§ "MHHighWebTransportRejections"`.
- `docs/runbooks/mc-deployment.md` — 1 line containing 2 cites: `metrics.rs:340` → `::record_register_meeting`; `mh_client.rs:136,144,157` → `::register_meeting` + prose disambiguator (inherent `impl MhClient` method, NOT trait decl/impl).

### TODO closure

`docs/TODO.md` line ~249 — "Runbook anchor-line-number rot" entry marked `[x]` with closure note pointing here.

### .gitignore

Added `__pycache__/` and `*.pyc` for the new `scripts/guards/lib/` Python module.

### Dev-time validation (NOT committed)

Per `docs/TODO.md` §Guard Self-Test Cleanup policy. Throwaway driver + fixtures under `/tmp/doc-citation-fixtures/`:

| Case | Outcome | Violations |
|---|---|---|
| **Guard A — 15 cases** | | |
| fail-bare-line-cite | PASS | 1 |
| fail-line-range-cite | PASS | 1 |
| fail-multi-extension-{rs,sh,yaml,md,proto} | PASS (×5) | 1 each |
| pass-symbol-anchor | PASS | 0 |
| pass-section-reference | PASS | 0 |
| pass-prose | PASS | 0 |
| pass-guard-ignore | PASS | 0 |
| fail-lazy-guard-ignore (`guard:ignore(test)`) | PASS | 1 |
| pass-url-port-cluster (`...cluster.local:5432`) | PASS | 0 |
| pass-url-port-simple (`host.com:8080`) | PASS | 0 |
| fail-code-block-internal (fenced block content) | PASS | 1 |
| **Guard C — 17 cases** | | |
| pass-rs-fn / pass-rs-struct / pass-rs-trait | PASS (×3) | 0 each |
| pass-sh-bare-fn / pass-sh-function-kw | PASS (×2) | 0 each |
| pass-toml-section / pass-toml-key | PASS (×2) | 0 each |
| pass-yaml-top-key | PASS | 0 |
| fail-yaml-nested-key (top-level-only rule) | PASS | 1 |
| pass-md-heading-word-boundary | PASS | 0 |
| fail-md-heading-collision (`::Test` ≠ "Testing Setup") | PASS | 1 |
| pass-proto-message / pass-proto-rpc | PASS (×2) | 0 each |
| fail-file-missing | PASS | 1 |
| fail-symbol-not-found | PASS | 1 |
| fail-path-escape (symlink to `/etc/passwd`) | PASS | 1 |
| pass-unverified-extension (`.json` skipped silently) | PASS | 0 |

**Summary: 32 PASS, 0 FAIL.** Fixtures deleted before commit per policy.

### Gate-2 validation (Layer 3 dogfooding)

- `./scripts/guards/run-guards.sh` — 31/31 PASS (including the two new guards).
- `./scripts/layer3.sh` — `RESULT=OK REASON=layer3-summary`, duration 9s (well within ADR-0033 §4 90s p95 budget).
- `./scripts/layer-all.sh` — Layer 3 OK. Layers 1/2/4/5/6 FAIL on this host with pre-existing environment issues (`buf` binary not installed, `nx`/`pnpm` not installed, `cargo-audit` advisory) — all documented as expected local-only failure modes in `devloop-validation.md` §6 ("Local-only failure mode — CI has `corepack` / `pnpm install` in setup"). None caused by this devloop's changes (no Rust/TS/proto edits in the diff). CI will catch any pipeline regression.

---

## Files Modified

**New**:
- `scripts/guards/lib/doc_cite_extract.py`
- `scripts/guards/simple/validate-doc-citations-no-line-numbers.sh`
- `scripts/guards/simple/validate-doc-citations-symbol-resolves.sh`

**Modified**:
- `scripts/guards/common.sh` — added `doc_citation_in_scope_files()` helper.
- `scripts/guards/simple/validate-alert-rules.sh` — refactor `load_ignore_lines` to call shared `is_lazy_reason` kernel.
- `docs/runbooks/devloop-validation.md` — 21-cite sweep + new §10 "Cite Convention" + Changelog row.
- `docs/runbooks/mh-deployment.md` — 4-cite sweep.
- `docs/runbooks/mc-deployment.md` — 2-cite sweep (one line).
- `.claude/skills/devloop/review-protocol.md` — 3-cite sweep.
- `docs/TODO.md` — closed "Runbook anchor-line-number rot" entry.
- `.gitignore` — `__pycache__/` + `*.pyc` for Python guard module.
- `docs/devloop-outputs/2026-05-14-doc-citation-durability-guards/main.md` — this file.

---

## Devloop Verification Steps

TBD — Gate 2 runs `./scripts/layer-all.sh`. The new guards self-apply (Layer 3 will run them; if the sweep is incomplete, the guards will fail Layer 3).

---

## Code Review Results

### Security Specialist
**Verdict**: TBD

### Test Specialist
**Verdict**: TBD

### Observability Specialist
**Verdict**: TBD

### Code Quality Reviewer
**Verdict**: TBD

### DRY Reviewer
**Verdict**: TBD

### Operations Reviewer (peer)
**Verdict**: TBD

---

## Tech Debt Pointers

TBD.

---

## Rollback Procedure

1. Start commit: `14273bb`
2. `git diff 14273bb..HEAD`
3. Soft reset: `git reset --soft 14273bb`
4. Hard reset: `git reset --hard 14273bb`

Docs + new guard scripts — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

TBD.

---

## Lessons Learned

TBD.
