# Devloop Output: Key every per-kind metric label by MediaKind

**Date**: 2026-10-26
**Task**: Session 2 of `2026-10-25-mediakind-keyed-signaling`: derive `media_kind` + `content_kind` metric labels from proto `MediaKind` (one projection per language), Rust↔TS↔taxonomy drift test, metric-label literal guard, GC telemetry-filter coverage, catalog/dashboards/alerts/runbooks/collector, and port the two bash proto guards onto `proto_lex`.
**Specialist**: meeting-controller (paired with client)
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/user-see-each-other`
**Duration**: ~3h wall clock (2026-10-26 ~19:20 → ~22:20)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `235c9f379b06d21940d38bf77323741e976c7dd9` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (meeting-controller) |
| Implementing Specialist | `meeting-controller` |
| Tier | `full` |
| Iteration | `1` |
| Security | done (RESOLVED-FIXED) |
| Test | done (RESOLVED-FIXED) |
| Observability | done (RESOLVED-FIXED) |
| Code Quality | done (RESOLVED-FIXED) |
| DRY | done (RESOLVED-FIXED) |
| Operations | done (RESOLVED-FIXED) |
| Paired Client | done (RESOLVED-FIXED) |
| Protocol | done (RESOLVED-FIXED) |
| Infrastructure | done (RESOLVED-FIXED) |
| Semantic Guard | not spawned: no checks.md surface (metric labels, guard ports; no credential/key material handled at runtime) |

---

## Task Overview

### Objective
Session 2 of `2026-10-25-mediakind-keyed-signaling`: key every per-kind METRIC LABEL by the
proto `MediaKind`, as session 1 keyed every per-kind signalling FACT. Concretely:
`media_kind` plus a co-carried `content_kind` (`main`/`screen`; the declared `slides`
renamed) produced by ONE projection per language; a drift check binding the Rust runtime,
the SDK record, the collector rules and GC to the taxonomy; guards confining kind-label
value literals to the two projection homes; the telemetry surface (collector, GC,
dashboards, alerts, runbooks, catalogs) updated; and the two remaining bash proto guards
ported onto `dt-guard`'s `proto_lex` (ADR-0034 part (a)).

### Scope
- **Service(s)**: mc-service (projection, recorders), gc-service (`MEDIA_DATAPOINT_EXTRA`), common (`CONTENT_KIND_LABEL`), env-tests, dt-guard, sdk-core + web-app (paired client), OTel collector, Prometheus rules, Grafana dashboards, docs.
- **Schema**: No
- **Cross-cutting**: Yes (MC, GC, client, collector, guards, observability docs)

### Debate Decision
NOT NEEDED — the design was ruled at session-1 Gate 1 (observability option (b)); the remaining choices (drift-test home, flat subcommands, collector co-presence, `max_streams`) were Lead/owner rulings recorded in §Planning and §Gate 3.

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
| `crates/mc-service/src/media_kind.rs` | Mine | — |
| `crates/mc-service/src/observability/metrics.rs` | Mine | — |
| `crates/mc-service/src/media_signaling/outcome.rs` (anchor test moves out) | Mine | — |
| `crates/mc-service/src/**` (`from_proto` call sites, `.label()` → `.labels()` at recorders) | Mine | — |
| `crates/common/src/observability/labels.rs` (`CONTENT_KIND_LABEL`) | Not mine, Minor-judgment | observability |
| `crates/gc-service/src/services/telemetry_filter.rs` (`MEDIA_DATAPOINT_EXTRA` += `CONTENT_KIND_LABEL`) | Not mine, Minor-judgment | global-controller |
| `crates/gc-service/tests/telemetry_proxy_tests.rs` | Not mine, Mechanical | global-controller |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` (`CONTENT_KIND_*`, all-or-none rule, MH rule widened) | Not mine, Domain-judgment (paired review) | test + observability |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/36_mc_media_kind_labels.rs` | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/31_gc_telemetry.rs` | Not mine, Minor-judgment | test |
| `crates/dt-guard/src/common/kind_labels.rs` (new: projection-home path constants, key set, labels.rs ident resolution) | Not mine, Domain-judgment (machinery infra; key/home membership observability) | infrastructure + observability |
| `crates/dt-guard/src/metric_labels.rs` (rule `kind_label_literal`) | Not mine, Domain-judgment | infrastructure + observability |
| `crates/dt-guard/src/ts_metric_naming.rs` (TS `kind_label_literal`) | Not mine, Domain-judgment | infrastructure + client |
| `crates/dt-guard/src/proto_no_any.rs` (new port) | Not mine, Domain-judgment (policy unchanged) | infrastructure + protocol |
| `crates/dt-guard/src/internal_proto_no_key_material.rs` (new port) | Not mine, Domain-judgment (policy unchanged) | infrastructure + security |
| `crates/dt-guard/src/common/proto_lex.rs` (doc: two bash consumers now ported) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/main.rs` (two subcommands) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/lib.rs` (two subcommands) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/common/mod.rs` (new common modules) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/tests/proto_ports_e2e.rs` (new) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/common/ident.rs` (new: shared identifier-boundary match) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/common/markdown_code.rs` (new: the one fence + inline-span reader) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/common/ts_lex.rs` (`skip_ws` hoisted) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/todo_tracking.rs` (fence skip on `common::markdown_code`) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/tests/common/mod.rs` (rule-id inventory resolves shared `pub use` ids) | Not mine, Minor-judgment | infrastructure |
| `docs/observability/slos.md` (kind query rule) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mh-service.md` (`KindLabelOnMh` rename) | Not mine, Mechanical | observability |
| `docs/specialist-knowledge/observability/INDEX.md` (`KindLabelOnMh` rename) | Not mine, Mechanical | observability |
| `crates/dt-guard/tests/metric_labels_e2e.rs` (kind-rule fixtures + scaffold) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/tests/alert_rules_e2e.rs` (co-occurrence fixture) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/tests/dashboard_panels_e2e.rs` (co-occurrence fixture) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/tests/client_metrics_export_fixtures.rs` (kind inputs in the valid tree + kind fixtures) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/simple/validate-proto-no-any.sh` (thin wrapper) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/simple/validate-internal-proto-no-key-material.sh` (thin wrapper) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/validate-proto-no-any.test.sh` (deleted) | Not mine, Minor-judgment | infrastructure |
| `scripts/guards/validate-internal-proto-no-key-material.test.sh` (deleted) | Not mine, Minor-judgment | infrastructure |
| `scripts/layer3.sh` (two selftest rows removed) | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` (§6.3 rows for the two guards) | Not mine, Minor-judgment | operations |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Not mine, Domain-judgment (paired) | client |
| `packages/sdk-core/src/**` (call sites + `__tests__`) | Not mine, Domain-judgment (paired) | client |
| `packages/web-app/e2e/clientMetricNames.ts` | Not mine, Domain-judgment (paired) | client |
| `packages/web-app/e2e/mcMetrics.ts` | Not mine, Domain-judgment (paired) | client |
| `packages/web-app/tests/clientMetricNames.test.ts` | Not mine, Domain-judgment (paired) | client |
| `infra/services/otel-collector/collector.yaml` (keep_keys, exact content_kind rule, pair rule, max_streams) | Not mine, Minor-judgment | observability |
| `scripts/otel-collector/acceptance_driver.py` (content_kind cases) | Not mine, Minor-judgment | observability |
| `docs/observability/label-taxonomy.md` (rows, projection table, dated note) | Not mine, Domain-judgment | observability |
| `docs/observability/metrics/mc-service.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | observability |
| `docs/observability/dashboards.md` | Not mine, Minor-judgment | observability |
| `docs/observability/alerts.md` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/mc-media.json` | Not mine, Minor-judgment | observability |
| `infra/grafana/dashboards/client-media.json` | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Not mine, Minor-judgment | observability |
| `infra/docker/prometheus/rules/client-alerts.yaml` | Not mine, Minor-judgment | observability |
| `docs/runbooks/mc-incident-response.md` (dated notes) | Not mine, Minor-judgment | operations |
| `docs/runbooks/client-dev-local.md` (dated notes) | Not mine, Minor-judgment | operations |
| `docs/protocol/CONVENTIONS.md` (guard pointers) | Not mine, Mechanical | protocol / operations |
| `AI_DEVELOPMENT.md` (guard pointers) | Not mine, Mechanical | protocol / operations |
| `docs/specialist-knowledge/*/INDEX.md` (guard pointers) | Not mine, Mechanical | protocol / operations |
| `crates/dt-guard/src/client_metrics_export.rs` (cross-artifact drift rules `kind_projection_ts_drift`/`kind_collector_rule_drift`/`kind_key_not_kept`/`kind_projection_vacuous`; content_kind fixtures; red fixtures) | Not mine, Minor-judgment | infrastructure + observability |
| `crates/dt-guard/src/alert_rules.rs` (rule `kind_label_co_occurrence`) | Not mine, Domain-judgment | infrastructure + observability |
| `crates/dt-guard/src/dashboard_panels.rs` (rule `kind_label_co_occurrence`) | Not mine, Domain-judgment | infrastructure + observability |
| `crates/dt-guard/src/common/rust_consts.rs` (new: `rust_str_consts`/`rust_str_array` hoisted from `client_metrics_export.rs`, second consumer `kind_labels.rs`) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/common/proto_lex.rs` (`scopes()` helper) | Not mine, Minor-judgment | infrastructure |
| `crates/dt-guard/src/common/proto_files.rs` (new: `proto_files()` moved out of `proto_per_kind_fields.rs`) | Not mine, Mechanical | infrastructure |
| `crates/dt-guard/src/proto_per_kind_fields.rs` (import moved `proto_files`) | Not mine, Mechanical | infrastructure |
| `scripts/guards/common.sh` (comment: `guard_seam_root` consumer list) | Not mine, Mechanical | infrastructure |
| `docs/runbooks/mc-deployment.md` (deploy order + post-deploy ramp note) | Not mine, Minor-judgment | operations |
| `packages/web-app/e2e/multi-party-hear.spec.ts` (R-29 read-backs assert `content_kind="main"`) | Not mine, Domain-judgment (paired) | client |
| `packages/web-app/e2e/multi-party-see.spec.ts` (R-29 read-backs assert `content_kind="main"`) | Not mine, Domain-judgment (paired) | client |
| `packages/sdk-core/src/index.ts` (exports) | Not mine, Domain-judgment (paired) | client |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Not mine, Mechanical | infrastructure / — |
| `docs/specialist-knowledge/meeting-controller/INDEX.md` | Not mine, Mechanical | infrastructure / — |
| `docs/TODO.md` (session-2 entry closed, ADR-0034 part a, test.sh pointers, subcommand count, MH-never gap narrowed) | Not mine, Mechanical | — |
| `docs/runbooks/mh-deployment.md` (client query snippets, dated note) | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` (client query snippets, dated note) | Not mine, Minor-judgment | operations |


---

## Planning

### Mechanism restatement

Today "which labels does a media kind get" is answered in five places that can drift
independently: MC's `MediaKindLabel::{label, from_proto}`, the SDK's `MEDIA_KIND_LABELS`,
the taxonomy row, the collector's exact rule, and env-tests' `MEDIA_KIND_VALUES` (a fourth
restatement the TODO entry does not name; GC is the fifth consumer but holds the KEY only).
Only the VALUE SET is pinned (two independent row parses); the MAPPING from proto `MediaKind`
is not pinned anywhere cross-language, and nothing stops a call site from writing
`MEDIA_KIND_LABEL => "video"`. The class is "a closed label vocabulary derived from a wire
enum"; `media_kind`/`content_kind` are its only members today (no same-owner sibling found:
`slot_state` is already derived from the proto enum via `slot_state_label`).

### Design

**1. The projection (one per language, from proto `MediaKind`).** The taxonomy gains a
PROJECTION TABLE (the SSoT both languages test against):

| Wire `MediaKind` | `media_kind` | `content_kind` |
|---|---|---|
| `MEDIA_KIND_AUDIO` | `audio` | `main` |
| `MEDIA_KIND_VIDEO_CAMERA` | `video` | `main` |
| `MEDIA_KIND_VIDEO_SCREEN` | `video` | `screen` |
| `MEDIA_KIND_UNSPECIFIED` | (no labels: refused at every parse boundary) | |

Rust (`media_kind.rs`): `MediaKindLabel` keeps its role (served kind; `label()` = the
`media_kind` value). New `ContentKind { Main, Screen }` with `label()`, and
`KindLabels { media_kind: MediaKindLabel, content_kind: ContentKind }` with
`KindLabels::from_proto(MediaKind) -> Option<Self>` (exhaustive, no wildcard) — THE
projection, replacing `MediaKindLabel::from_proto` (only tests call it). Served kinds get
`MediaKindLabel::labels(self) -> KindLabels` (camera ⇒ `main`), pinned by a test that
`KindLabels::from_proto(k.to_proto()) == Some(k.labels())` for every served kind. The four
value literals exist ONLY in the two `label()` matches in this file. `CONTENT_KIND_LABEL`
joins `common::observability::labels`. Every MC recorder carrying `MEDIA_KIND_LABEL` adds
`CONTENT_KIND_LABEL => l.content_kind.label()` (all-or-none), incl. zero-init loops
(`KindLabels` over served kinds: series exist for exactly the reachable tuples; no
`screen` series is pre-published because no kind is served as screen). Name kept as
`MediaKindLabel` (357 refs/28 files; a rename is churn, not design — say if you disagree).

TS (`mediaMetrics.ts`, @paired-client): `MEDIA_KIND_LABELS` replaced by a projection record
keyed by the generated proto enum, in a FIXED source shape the Rust drift test parses:
`[MediaKind.AUDIO]: { media_kind: 'audio', content_kind: 'main' }` etc., typed
`satisfies Record<Exclude<MediaKind, MediaKind.UNSPECIFIED>, …>` so a new wire value is a
compile error. `MediaKindMetrics` takes a proto `MediaKind` and spreads both labels.

**2. Drift test.** SUPERSEDED: see Gate-1 revisions, "Drift test, re-homed per the Lead's ruling".

**3. Literal guard (`kind_label_literal`).** SUPERSEDED in part: the literal-value form below is replaced by the positive `.label()` form, see Gate-1 revisions "Literal guard". New `dt-guard/src/common/kind_labels.rs`:
`RUST_PROJECTION_HOME`, `TS_PROJECTION_HOME` (named path constants), the key set
`{media_kind, content_kind}`, and ident→key resolution read from
`crates/common/src/observability/labels.rs` (vacuous if either key does not resolve). Rust
(`metric_labels.rs`, via the existing `metric_macros` walker): a label arg whose key is a
kind key (literal, or an ident resolving to one, last path segment) and whose value is a
string literal → finding, unless the file is the home. TS (`ts_metric_naming.rs` scope):
`media_kind:`/`content_kind:` (bare or quoted key) with a string/template literal value
outside the TS home → finding. Self-test bar: specific-finding asserts per form (literal
key, const key, `common::…::CONST`, brace form, TS quoted/bare/template); positive controls
on the REAL tree that MUST hit (≥ N kind-labelled MC macro sites recognised; the TS home
recognised) so the matcher is provably live; a neutered-matcher proof (empty key set ⇒ the
red fixtures go green).

**4. GC (third consumer): covered, not exempted.** GC production code holds the KEY only and
is deliberately value-agnostic (the collector is the value bound); it is inside the guard's
scan scope like every crate, with nothing to find. `MEDIA_DATAPOINT_EXTRA` gains
`CONTENT_KIND_LABEL` (7→8). Its fixtures' `"audio"`/`"video"` are opaque payload exercising
key admission (the `"bogus"` case proves value-agnosticism) — the same treatment test
fixtures get in every crate, stated in the module doc.

**5. Telemetry surface (observability to steer per-panel).** Taxonomy rows (`slides`→`screen`,
`content_kind` CARRIED), projection table, collector `keep_keys` + exact `content_kind` rule,
acceptance-driver cases (`main`, `bogus`→`invalid`, int), catalogs, dashboards/alerts/runbooks
each with a dated "2026-10-26: series identity, not traffic — `content_kind` co-carried;
every series reads `main` until screen share ships" note. My default: no selector changes
(no screen traffic exists); `by (media_kind)` aggregations stay correct; observability rules.
env-tests hygiene: all-or-none rule (a stored MC/client series with `media_kind` lacks
`content_kind` or vice versa ⇒ violation) and `Rule::MediaKindOnMh` widened to both keys.

**6. Proto guard ports.** `dt-guard proto-no-any` and `dt-guard internal-proto-no-key-material`
on `proto_lex` tokens; same rule tokens, same vacuity tokens, same messages' substance, no
suppression path; `--root` replaces the `guard_seam_root` seam (kept in `common.sh`: still
used by `validate-dev-cluster-verbs.sh`). Every `.test.sh` case becomes an in-module test
asserting the specific token; `.test.sh` files and their `layer3.sh` rows deleted; wrappers
keep their paths so existing doc pointers stay valid.

### Gate-1 revisions (folded reviewer input, 2026-10-26)

**Projection / recorders (observability P2, protocol A1-A3).** Rust: `KindLabels::from_proto`
is THE projection, exhaustive with no `_` arm. `MediaKindLabel::labels(self)` is DEFINED as
`KindLabels::from_proto(self.to_proto())`: no second served→pair mapping exists. The
Unspecified arm is unreachable from a served kind, and the code says so via `expect` with a
reason or a const-asserted helper. Recorder signatures take `KindLabels`, never
`MediaKindLabel`, so a recorder cannot stamp `media_kind` without `content_kind`. Confirmed
(Q2): `MediaKindLabel::from_proto` has NO production caller (`outcome.rs` tests and
`assignments.rs` test only), so no MC series can carry `screen` today. Stated once in the
taxonomy and the MC catalog as a pointer to the projection: "no series emits `screen` until
content share ships". TS (@paired-client): `MEDIA_KIND_LABEL_PROJECTION` keyed by proto
`MediaKind`, `satisfies Record<Exclude<MediaKind, UNSPECIFIED>, …>`. The lookup THROWS on a
missing entry (protobuf-es enums are open at runtime). Vitest covers UNSPECIFIED and an
out-of-range value. `MEDIA_KIND_LABELS` is retired.

**Drift test, re-homed per the Lead's ruling (DRY A, CQ-1, test G1-G3, T7, protocol A3).**
The check is split by what each leg needs.

- **Rust runtime leg, mc-service.** `media_kind_label_matches_the_taxonomy_row` moves
  from `outcome.rs` to `media_kind.rs` and is EXTENDED. It reads the `media_kind` and
  `content_kind` rows and the projection table through ONE key-taking row reader. It
  sweeps `MediaKind::try_from(0..=8)`, and each case must hold:
  - every Ok value has exactly one table row equal to the runtime `KindLabels::from_proto`
  - every Err maps to no labels
  - each row's value set equals the union of its projection column
  It supersedes `media_kind_label_projects_every_wire_kind_without_a_wildcard`.
  `MediaKindLabel::from_proto` is deleted outright, with no shim. The type and `Video`
  variant docs are rewritten so they no longer claim to be the projection. The checks are
  a pure fn `runtime_drift(taxonomy) -> Vec<Drift>` with adverse tests: a row disagreeing
  with Rust, a missing row, a row value not projected, and a missing table, which reports
  as vacuity. A further vacuity case (@test): the table must parse EXACTLY 4 rows (3 labelled wire kinds plus UNSPECIFIED) with no duplicate wire value, reported with its own reason, distinct from content drift.
- **Same anchors (@protocol, DRY).** Both table readers (mc-service, and dt-guard `client_metrics_export.rs`) anchor on the SAME taxonomy heading and row markers. Each carries a one-line note naming the other, since the crate boundary prevents sharing one parser.
- **Cross-artifact legs, dt-guard.** New rules in `client_metrics_export.rs`, which already
  has the one reader of each artifact: `LABEL_TAXONOMY` via `markdown_table`, the
  collector via `collector_keep_keys` and a sibling `collector_exact_rule(cfg, key)`, GC
  via `rust_str_array`/`rust_str_consts`, and the SDK via `ts_lex`. The rules are:
  - `kind_projection_ts_drift`: the TS record (`[MediaKind.X]: { media_kind, content_kind }`
    entries, read through `ts_lex` string literals) must equal the taxonomy table.
  - `kind_collector_rule_drift`: the exact anchored rule for each key must equal its row's
    value set. An unanchored or unparenthesised rule is a finding.
  - `kind_key_not_kept`: each kind key must be in `keep_keys`, in GC
    `MEDIA_DATAPOINT_EXTRA`, and in the taxonomy `pass =` list.
  - `kind_projection_vacuous`: a construct is missing, or the parse did not find EXACTLY 3
    TS entries, 2 collector rules and 2 rows. This token is distinct from the content
    tokens.

  The comparison is a pure fn over texts. Each adverse test asserts ONE specific finding:
  TS camera→`screen`, `^(main|slides)$`, unanchored `^audio|video$`, `content_kind`
  dropped from keep_keys, from EXTRA, and from `pass =`, a reflowed or dropped TS entry
  (must report as vacuity), and a doc still saying `slides`. Red fixtures for G4 and Rule 3
  cover `content_kind` in `#base` but missing from keep_keys or EXTRA. If `kind_labels.rs`
  also needs `rust_str_consts`, it is hoisted to `common/rust_consts.rs`, not copied.
- **TS, runtime only (@paired-client).** `projectMediaKind(kind)` looks up the record and
  throws on UNSPECIFIED or an unknown number. Its vitest covers every generated
  non-UNSPECIFIED value, plus UNSPECIFIED and 99, which must throw. The TS row test is
  DELETED; TS does no taxonomy parse.
- **env-tests.** It keeps its own row-pinned `MEDIA_KIND_VALUES` + `CONTENT_KIND_VALUES`.
  Stated reason: it cannot depend on mc-service or dt-guard (ADR-0028).

**Literal guard (infra 5/10/11, test G4, DRY 2, observability Q3, paired-client 1).**
- **Rust, positive form.** In any metric macro outside the Rust home, the value of a kind
  key (a literal key, or an ident resolved from `LABELS_RS`, matched on its last path
  segment) must be a `….label()` method call. Anything else is `kind_label_literal`.
- **Stated bound.** The rule cannot prove the receiver's type. `x.label()` on an unrelated
  enum passes. The drift test and the type-taking recorders close that gap.
- **MH-never at source.** Any kind key in a metric macro under `crates/mh-service/` is
  `kind_label_on_mh`. This is the first source-level MH-never check, so the taxonomy
  sentence is updated and the §Guard Coverage Gaps entry is narrowed to what remains: a
  const re-imported under another name, or a non-macro emission.
- **TS.** After `ts_lex` comment-strip:
  - object key `media_kind`/`content_kind` (bare or quoted) with a string or template
    literal value
  - PromQL selector `(media_kind|content_kind)=~?"<letter>` inside string/template text,
    where `"${…}"` interpolation is green
  - scope is `packages/{sdk-core,web-app}/src/**` plus `packages/web-app/e2e/**` non-spec
    helpers, for this rule only
- **Literals in comments** are green on both sides: both scanners comment-strip first.
  There is a test for each.
- **Vacuity.** Each of these is vacuous: a missing `RUST_PROJECTION_HOME`,
  `TS_PROJECTION_HOME` or `LABELS_RS`; a home holding zero kind literals; an unresolved key
  const.
- **Self-test bar.** Every red asserts (rule, file, line). Real-tree positive controls must
  show that both homes were scanned with literals SEEN > 0, that MC kind-labelled macro
  sites were recognised > 0, and that a planted `MEDIA_KIND_LABEL => "audio"` in a copy of
  real `metrics.rs` hits. There is an MH fixture that must hit. The neutered-matcher proof
  empties the key set through a `check_with` seam, after which the red fixtures go green.
- **Test-code scope (DRY B, CQ-2).** Rust exempts test code via `common::test_code_filter`
  (`is_scan_exempt`, `compute_test_block_ranges`/`is_line_in_test_block`), the same path
  `metric_labels.rs` already uses. The Rust rule is macro-only, so test assertions such as
  `.with_labels(&[(MEDIA_KIND_LABEL, "audio")])` are out of its positions by construction.
  TS reuses `ts_metric_naming.rs`'s existing `EXCLUDED_PATH_PATTERNS`/`EXCLUDED_FILE_SUFFIXES`
  exclusions for `__tests__`/`.test.ts`/`.spec.ts`.
- **e2e scope (infra).** The `packages/web-app/e2e/**` widening is a SEPARATE named constant,
  `KIND_LITERAL_EXTRA_SCOPE`, never added to `SCOPE_PREFIXES`. A test shows an e2e-helper
  fixture hits `kind_label_literal` and not `metric_name`.
- **Regex.** There is no new `Regex::new`. Rust matching reuses the `metric_macros`/
  `metric_labels` walker. TS matching walks `ts_lex` tokens and string-literal bodies byte
  by byte. Key-const resolution reuses `rust_str_consts`, hoisted to `common/` once it has
  its second consumer.
- **Stale-home check.** Each home-path constant must exist AND contain the projection
  (literals seen > 0). Otherwise the finding is `kind_label_vacuous`, a distinct token.

**Query co-occurrence (observability Q1/P6, operations 3/4).** I accept the overrule. Every
PromQL expression that names `media_kind` also names `content_kind`:
- grouping becomes `by (media_kind, content_kind)`
- the current camera-tuned video alerts and panels get `content_kind="main"`: decoder-error
  ratio, slot_unfulfilled p95, sender_server_muted
- the per-kind KEK ratio at `mc-alerts.yaml:813` groups by both labels
- legends become `{{media_kind}}/{{content_kind}}`
- `max by` gauges and the `ignoring(media_kind) group_left()` ratio become
  `ignoring(media_kind, content_kind)`, and each is checked to mean the same thing

`alert_rules.rs` and `dashboard_panels.rs` both walk expressions, so they get the new rule
`kind_label_co_occurrence` from a shared helper in `common/kind_labels.rs`. Its positive
control is that a fixture naming only `media_kind` reds. Each affected file and section
gets a dated note.

**Collector / GC / ops (infra 1/13-15, security D1-D3, operations 1/2/4, observability
Q4).** In `collector.yaml`:
- `content_kind` goes into `keep_keys`
- a rewrite to `invalid` with an exact anchored `^(main|screen)$` rule, guarded by `!= nil`
  and `IsString`
- the `max_streams` ledger is recomputed:
  - honest today: unchanged, ×1, because everything is `main`
  - honest with screen: video ×2
  - adversarial: per-kind multiplier 3→9
  - the cap is raised if headroom falls below 3 runs

The acceptance driver gets content_kind cases mirroring media_kind (main passes; bogus,
prefix, suffix and int become `invalid`; the nil-guard does not stamp kind-free series).
`scripts/otel-collector/acceptance.sh` is extended, not forked. GC admits `content_kind` at
the MediaDatapoint tier only. It gets a twin of `media_kind_value_is_not_checked_here`, and
the drop test at resource/span scope extends to the 8-key fixture. `client_metrics_export`
already derives `MEDIA_DATAPOINT_EXTRA` and resolves `#base`'s keys, so it needs no new
rule. I add a red fixture: `content_kind` in `#base` but missing from keep_keys, then
missing from EXTRA. Deploy order goes into Rollback §7 and `mc-deployment.md`. ONE commit is
the revert unit. The receiving side (GC + collector) rolls out BEFORE MC/client and rolls
back AFTER them. The post-deploy note says rate()/increase() and `for:` timers ramp on the
new series, which is expected and not an incident. `dt-guard kustomize` must stay green on
the rule ConfigMap size.

**env-tests (test G6, T6, observability P9).** The all-or-none rule flags a stored MC or
client series that carries one key without the other. Its non-empty precondition is that at
least one stored series carries BOTH keys, and it has a vacuity reason distinct from the
content one. `Rule::MediaKindOnMh` is widened to both keys. Tests 32, 36 and 31 get
content_kind twins. No test asserts a `screen` VALUE (absent = 0); screen is pinned at the
projection level.

**Proto ports (protocol B1-B7, security A1-A9/B/C, infra 2-9, test G5, Lead).**
- **Subcommands.** Two flat subcommands, `proto-no-any` and
  `internal-proto-no-key-material` (42 total; TODO:633 updated).
- **Shared discovery.** `proto_files()` moves to `common/proto_files.rs`, and each caller
  maps empty to its own `*-scan-vacuous`.
- **Scan surface.** Both ports scan `Token::Open.header` AND `Token::Statement.text`. New
  tests:
  - rpc header `rpc Foo(.dark_tower.signaling.v1.MeetingKekUpdate) returns (Ack)` → cross-package-type
  - `rpc X(google.protobuf.BytesValue) returns (Y) { option deprecated = true; }` → bytes-field
  - rpc header with `google.protobuf.Any` → proto-any-type
- **Patterns kept verbatim.** The bytes regex, the cross-package extractor (leading dot
  included, exact FQN equality), and the exact plain import form.
- **Rule tokens.** All eight stay byte-identical, in an exhaustive `Rule::id()`.
- **Structural identity markers.** A depth-0 `package dark_tower.internal.v1` statement and
  a depth-0 `service MediaHandlerService` Open header.
- **Enum check.** It reads signaling.proto Open headers and is TIGHTENED to depth 0, as
  security offered (A5). That is a named policy change, acked by @security.
- **Allowlists.** `ALLOWED_TYPES` / `ALLOWED_IMPORTS` become `pub const`s with the
  @security comment, now reading "top-level enum". Membership is unchanged.
- **Depth-0 enum resolution (@security ack).** Each `ALLOWED_TYPES` entry resolves as
  follows:
  - a depth-0 `message X` → allowlist-not-enum
  - no depth-0 declaration of X → scan-vacuous, even when a nested enum or message named X
    exists (tested)
  - a depth-0 enum plus a nested message of the same name → green (tested)
- **One scope walker (DRY C).** A policy-free `proto_lex::scopes(tokens)` yields
  (header, depth, line, body statements). `proto_per_kind_fields::derive_vocabulary` and
  the no-key-material enum check both use it, so there is no second enum-scope walker.
- **No suppression.** The ports do not import `crate::ignore`, and no shared suppression
  applies by default. A test plants `bytes k = 90; // guard:ignore(reason here)`, which
  must still red.
- **Root.** The wrapper passes the repo root derived from its own location and ignores
  run-guards' `$1`. `--root` is a production flag, not a DEVLOOP_TEST seam.
- **Vacuity.** Per-file vacuity does not stop the no-any scan, but the run still reds.
- **Lexer deltas, recorded in the module doc and runbook rows 526/527.**
  - New refusals, fail-closed: `'` outside strings, and unbalanced braces, both mapped to
    `*-scan-vacuous`.
  - Behaviour that now differs from bash: `//` and `/*` inside strings are no longer
    stripped or refused.
  - Findings cite real line numbers.
- **Doc fan-out.**
  - the runbook :510, :526-527, §8 :831-832
  - `common.sh:44`
  - the `proto_lex.rs` header
  - CONVENTIONS :27
  - INDEX files (security, code-reviewer, test, operations, protocol, infrastructure)
  - TODO :1647 / :3592 pointers
- **Wiring.** Both new modules are registered in `test_registration`.

#### Port parity (bash case → in-module test)

| Bash case | Test fn |
|---|---|
| no-any `real-tree-clean` | `no_any::the_real_tree_is_clean_and_non_vacuous` (no `--root`, files > 0) |
| no-any `import-plain` / `-public` / `-weak` / `-no-space` | `no_any::any_import_forms_hit` (4 asserted findings) |
| no-any `type-field` / `-leading-dot` / `-repeated` / `-map-value` / `-split-lines` | `no_any::any_type_spellings_hit` |
| (new) rpc header `Any` | `no_any::any_in_an_rpc_header_hits` |
| no-any `comment-mention-only`, `longer-identifier` | `no_any::comments_and_longer_identifiers_stay_green` |
| no-any `block-comment`, `no-proto-files`, `empty-proto-file` | `no_any::vacuity_cases` (+ `'` and unbalanced braces) |
| (new) neutered | `no_any::neutered_matcher_goes_green` |
| nkm `p0-real-tree-passes`, `p1-synthetic-baseline-passes` | `nkm::the_real_tree_is_clean_and_non_vacuous`, `nkm::synthetic_baseline_is_clean` |
| nkm `c1-pos-1..6`, `c1-oneof` | `nkm::every_bytes_spelling_hits` (7, each asserting not-vacuous) |
| (new) rpc header `BytesValue` | `nkm::bytes_value_in_an_rpc_header_hits` |
| nkm `c1-neg-total-bytes`, `-bytes-sent-label`, `-comment-prose` | `nkm::byte_counters_and_prose_stay_green` |
| nkm `c2-pos-1..6` | `nkm::every_cross_package_spelling_hits` (each asserting not bytes-field) |
| (new) rpc header cross-package | `nkm::cross_package_in_an_rpc_header_hits` |
| nkm `c2-neg-canonical-transport-mode`, `-comment-prose` | `nkm::canonical_allowlisted_enum_and_prose_stay_green` |
| nkm `enum-only` | `nkm::allowlisted_name_declared_as_message_hits` (+ nested-enum-only → vacuity, depth-0 tightening) |
| nkm `c3-import`, `c3-import-public-form` | `nkm::non_allowlisted_imports_hit` (+ weak) |
| nkm `ws-split-bytes` (`bytes\n raw`), `ws-split-type`, `ws-spaced-type`, `ws-spaceless-import` | `nkm::whitespace_bypasses_hit` |
| nkm `vac-internal-missing`/`-empty`/`-identity-markers`/`-signaling-missing`/`-allowlist-entry-absent`/`-block-comment` | `nkm::vacuity_cases` (+ `'`, unbalanced, nested `service`) |
| nkm `seam-ignored-without-devloop-test` | N/A: seam removed; `--root` is an explicit production arg |
| (new) suppression | `nkm::guard_ignore_marker_is_not_honoured` |
| (new) neutered | `nkm::neutered_matcher_goes_green` |

### Split with @paired-client (tracked deliverables)
@paired-client: the TS projection + throw-on-missing + vitests (runtime exhaustiveness,
UNSPECIFIED/99 throw, both-keys-or-neither; the TS taxonomy-row test is DELETED, with no TS taxonomy parse); SDK
call sites + `index.ts` exports; e2e `clientMetricNames.ts`/`mcMetrics.ts` derived from the
projection; **multi-party-hear/see R-29 read-backs assert `content_kind="main"` end to end**;
client.md, client-media.json, client-alerts.yaml (co-occurrence edits), client runbook;
verifies `client_metrics_export` RED-then-GREEN; reviews the TS guard rule. Me: everything
else in the table.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Paired Client | confirmed |
| Protocol | confirmed |
| Infrastructure | confirmed |

Lead rulings at Gate 1: (1) proto guard ports ship as flat subcommands (42; the Lead's grouping request was withdrawn on @infrastructure's objection — it stays at the split trigger and pre-decides the split debate); (2) drift-test home: cross-artifact legs (TS, collector, keep_keys, GC, taxonomy `pass =`) in `dt-guard` `client_metrics_export.rs` on its existing readers; mc-service `media_kind.rs` keeps only the Rust-runtime leg; TS runtime-only tests. `validate-cross-boundary-classification.sh`: STATUS=OK.

### Gate 3 — Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S-1 collector lone-key; rulings: pair-poisoning + max_streams 1000 accepted |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | TF-1/TF-2 guard vacuity; pair-poisoning covered only by acceptance.sh |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | F1 half-pairs, F2 per-clause co-occurrence, F3 docs PromQL guard |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | R1–R8 |
| DRY | RESOLVED-FIXED | 7 | 7 | 0 | F1–F7; closes session-1 bash proto-guard deferral |
| Operations | RESOLVED-FIXED | 4 | 4 | 0 | OPS-1–4; max_streams 1000 re-ruled on honest headroom |
| Paired Client | RESOLVED-FIXED | 3 | 3 | 0 | PC-1–3 |
| Protocol | RESOLVED-FIXED | 3 | 3 | 0 | P-1–3; no proto/** edits (no GSA) |
| Infrastructure | RESOLVED-FIXED | 6 | 6 | 0 | IR-1–6; WalkDir deviation accepted |

Semantic Guard: not spawned (see Loop State).

---

## Pre-Work

None.

---

## Implementation Summary

### Projection (Rust; TS by @paired-client)
| Item | Before | After |
|------|--------|-------|
| Label projection | `MediaKindLabel::from_proto` (screen → `video`, no `content_kind`) | `KindLabels::from_proto` → `{media_kind, content_kind}`, exhaustive, `UNSPECIFIED` → `None`; `MediaKindLabel::from_proto` deleted |
| Served-kind labels | `MediaKindLabel::label()` at every recorder | `MediaKindLabel::labels()` = `SERVED_LABELS`, built from the one projection at COMPILE time (`project_served`, const-evaluated) |
| Recorder signatures (7) | `kind: MediaKindLabel` | `labels: KindLabels`; both keys always stamped |
| Zero-init | `MediaKindLabel::ALL` | `KindLabels::served()` (`audio/main`, `video/main`; no `screen` tuple) |
| Key constant | `MEDIA_KIND_LABEL` | + `CONTENT_KIND_LABEL` (`common::observability::labels`) |
| Taxonomy | `content_kind` declared (`main`, `slides`), not carried | carried (`main`, `screen`), projection table (the SSoT hub), query rule, MH-never at source |

### Drift (split per Lead ruling)
- mc-service `media_kind.rs`: `runtime_drift(taxonomy)` + extended `media_kind_label_matches_the_taxonomy_row` (try_from 0..=8 sweep, exact 4-row table, row vs code, row vs table) and adverse tests.
- dt-guard `client_metrics_export.rs`: `kind_projection_ts_drift`, `kind_collector_rule_drift`, `kind_key_not_kept` (keep_keys, GC EXTRA, `pass =`), `kind_projection_vacuous`; pure `kind_projection_findings` + adverse tests; real-tree positive control.

### Guards
- `common/kind_labels.rs` (homes, key set, key-const resolution, co-occurrence); `common/rust_consts.rs` (hoisted `rust_str_consts`).
- `metric-labels`: `kind_label_literal` (positive `.label()` form), `kind_label_on_mh`, `kind_label_vacuous`; real-tree sites ≥10, planted literal hits, neutered proof.
- `ts-name-guard-dt-client`: `kind_label_literal` (object + PromQL forms, FULL scope incl. `packages/web-app/e2e/**` via its own scope const), `kind_label_vacuous`; home literals seen == 6.
- `alert-rules-policy` / `dashboard-panels`: `kind_label_co_occurrence`.
- Proto ports (`proto-no-any`, `internal-proto-no-key-material`) on `proto_lex` with `scopes()` and `common/proto_files.rs`; parity table in §Planning.

### Telemetry surface
- GC `MEDIA_DATAPOINT_EXTRA` 7 → 8; collector keep_keys + exact `^(main|screen)$` rule + value-agnostic pair-poisoning rule; `max_streams` 500 → 1000 with the recomputed ledger; acceptance driver content_kind cases.
- env-tests: `CONTENT_KIND_*`, `Rule::KindLabelsSplit` (all or none) with a non-empty precondition in 32, MH rule widened, 31/36 twins.
- MC dashboards/alerts/catalog/runbooks: both keys in every expr; dated notes; deploy order in `mc-deployment.md`.

### Gate-3 additions
- Collector co-presence: a datapoint carrying ONE kind key is completed to `invalid`/`invalid` (lone-key rules ahead of the pair-poisoning rules), so all-or-none holds at the store; per-name adversarial tuples 6; ledger 493 per-kind / 588 forged-on-every-name (stated as a FLOOR) with a memory line; `max_streams` 500 → 1000.
- Co-occurrence per UNIT (each `{…}` selector, each grouping/vector-matching clause; legends one unit), and a docs PromQL pass in `alert-rules-policy` over `docs/observability/**` + `docs/runbooks/**` (fences of any language + inline spans; `doc_promql_vacuous`), on the new single fence/inline reader `common/markdown_code.rs` (also adopted by the alerts.md inventory, `IdentityPolicy::parse_doc`, `identity_label_cases`, `todo_tracking`).
- Shared helpers: `common/ident.rs`, `proto_lex::{starts_with_keyword, enum_values}` (top-level only), `ts_lex::skip_ws`, `kind_labels::{KIND_LABEL_CO_OCCURRENCE_RULE_ID, co_occurrence_message}`; `KindLabelOnMh` rename; TS guard runs the kind rule before the scaffold exit with a combined `verdict`; Rust home check counts the projection's constructs; walkers propagate errors.

### Additional Changes
- `scripts/otel-collector/acceptance.sh` PASSED at Gate 2 (Lead): lone-key, pair-poisoning and exact-rule cases; `streams_limit` 1000, RSS ~205 MB.

## Files Modified

```
 AI_DEVELOPMENT.md                                  |   2 +-
 crates/common/src/observability/labels.rs          |  31 +-
 crates/dt-guard/src/alert_rules.rs                 | 337 ++++++++-
 crates/dt-guard/src/client_metrics_export.rs       | 777 +++++++++++++++++++--
 crates/dt-guard/src/common/mod.rs                  |   5 +
 crates/dt-guard/src/common/proto_lex.rs            | 176 ++++-
 crates/dt-guard/src/common/ts_lex.rs               |  11 +
 crates/dt-guard/src/dashboard_panels.rs            |  43 ++
 crates/dt-guard/src/lib.rs                         |   2 +
 crates/dt-guard/src/main.rs                        |  26 +
 crates/dt-guard/src/metric_labels.rs               | 395 ++++++++++-
 crates/dt-guard/src/proto_per_kind_fields.rs       |  72 +-
 crates/dt-guard/src/todo_tracking.rs               |  21 +-
 crates/dt-guard/src/ts_metric_naming.rs            | 440 +++++++++++-
 crates/dt-guard/tests/alert_rules_e2e.rs           |  37 +-
 .../tests/client_metrics_export_fixtures.rs        | 148 ++--
 crates/dt-guard/tests/common/mod.rs                |  27 +
 crates/dt-guard/tests/dashboard_panels_e2e.rs      |  16 +
 crates/dt-guard/tests/metric_labels_e2e.rs         |  71 +-
 crates/env-tests/src/fixtures/metric_hygiene.rs    | 218 ++++--
 crates/env-tests/tests/31_gc_telemetry.rs          |  86 ++-
 crates/env-tests/tests/32_media_metric_hygiene.rs  |  17 +-
 crates/env-tests/tests/36_mc_media_kind_labels.rs  |  25 +-
 crates/gc-service/src/services/telemetry_filter.rs |  78 ++-
 crates/gc-service/tests/telemetry_proxy_tests.rs   |  66 +-
 crates/mc-service/src/actors/meeting_media.rs      |   8 +-
 crates/mc-service/src/media_kind.rs                | 474 ++++++++++++-
 .../mc-service/src/media_signaling/assignments.rs  |   4 +-
 crates/mc-service/src/media_signaling/outcome.rs   |  49 --
 crates/mc-service/src/observability/metrics.rs     | 120 ++--
 crates/mc-service/src/webtransport/connection.rs   |  12 +-
 crates/mc-service/src/webtransport/server.rs       |   7 +-
 docs/TODO.md                                       |  17 +-
 docs/observability/alerts.md                       |  30 +-
 docs/observability/dashboards.md                   |  20 +-
 docs/observability/label-taxonomy.md               |  99 ++-
 docs/observability/metrics/client.md               | 151 ++--
 docs/observability/metrics/mc-service.md           |  78 ++-
 docs/observability/metrics/mh-service.md           |   4 +-
 docs/observability/slos.md                         |   4 +-
 docs/protocol/CONVENTIONS.md                       |   3 +-
 docs/runbooks/client-dev-local.md                  |  17 +-
 docs/runbooks/devloop-validation.md                |   9 +-
 docs/runbooks/mc-deployment.md                     |  38 +-
 docs/runbooks/mc-incident-response.md              |  66 +-
 docs/runbooks/mh-deployment.md                     |  14 +-
 docs/runbooks/mh-incident-response.md              |  23 +-
 docs/specialist-knowledge/code-reviewer/INDEX.md   |   2 +-
 docs/specialist-knowledge/infrastructure/INDEX.md  |   6 +-
 .../meeting-controller/INDEX.md                    |   4 +-
 docs/specialist-knowledge/observability/INDEX.md   |   2 +-
 docs/specialist-knowledge/operations/INDEX.md      |   2 +-
 docs/specialist-knowledge/protocol/INDEX.md        |   4 +-
 docs/specialist-knowledge/security/INDEX.md        |   2 +-
 docs/specialist-knowledge/test/INDEX.md            |   2 +-
 infra/docker/prometheus/rules/client-alerts.yaml   |  30 +-
 infra/docker/prometheus/rules/mc-alerts.yaml       |  21 +-
 infra/grafana/dashboards/client-media.json         | 222 +++---
 infra/grafana/dashboards/mc-media.json             |  52 +-
 infra/services/otel-collector/collector.yaml       |  85 ++-
 packages/sdk-core/src/index.ts                     |   6 +-
 .../__tests__/audioPipeline.multiSender.test.ts    |   4 +-
 .../lifecycle/__tests__/audioPipeline.test.ts      |   1 +
 .../lifecycle/__tests__/keyframePolicy.test.ts     |   5 +-
 .../lifecycle/__tests__/unfulfilledSlots.test.ts   |   5 +-
 .../lifecycle/__tests__/videoReceiver.test.ts      |   9 +-
 packages/sdk-core/src/media/mediaKind.ts           |   4 +-
 .../pipeline/__tests__/egress.multiTarget.test.ts  |   9 +-
 .../pipeline/__tests__/ingress.attribution.test.ts |   5 +-
 .../pipeline/__tests__/ingress.hopSender.test.ts   |   5 +-
 .../__tests__/ingress.rejectTokens.test.ts         |   5 +-
 .../__tests__/ingress.verification.test.ts         |   9 +-
 .../__tests__/nonceUniqueness.property.test.ts     |   7 +-
 .../media/pipeline/__tests__/receiveLanes.test.ts  |   9 +-
 .../media/pipeline/__tests__/videoEgress.test.ts   |   5 +-
 .../src/media/pipeline/__tests__/videoFixtures.ts  |   5 +-
 .../src/media/setup/__tests__/measurement.test.ts  |   9 +-
 .../src/media/setup/__tests__/mediaMetrics.test.ts | 110 ++-
 packages/sdk-core/src/media/setup/mediaMetrics.ts  | 122 +++-
 packages/sdk-core/src/signaling/slotStateMap.ts    |   2 +-
 packages/web-app/e2e/clientMetricNames.ts          |  29 +-
 packages/web-app/e2e/mcMetrics.ts                  |  26 +-
 packages/web-app/e2e/multi-party-hear.spec.ts      |  14 +-
 packages/web-app/e2e/multi-party-see.spec.ts       |  20 +-
 packages/web-app/tests/clientMetricNames.test.ts   |  58 +-
 scripts/guards/common.sh                           |   4 +-
 .../validate-internal-proto-no-key-material.sh     | 225 +-----
 scripts/guards/simple/validate-proto-no-any.sh     | 114 +--
 ...validate-internal-proto-no-key-material.test.sh | 186 -----
 scripts/guards/validate-proto-no-any.test.sh       |  95 ---
 scripts/layer3.sh                                  |  17 -
 scripts/otel-collector/acceptance_driver.py        |  41 +-
 92 files changed, 4280 insertions(+), 1693 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/mc-service/src/media_kind.rs` | `ContentKind`, `KindLabels::from_proto` (THE projection), const-derived `SERVED_LABELS`, runtime drift test vs taxonomy |
| `crates/mc-service/src/observability/metrics.rs` | 7 per-kind recorders take `KindLabels`; zero-init over `KindLabels::served()` |
| `crates/gc-service/src/services/telemetry_filter.rs` | `MEDIA_DATAPOINT_EXTRA` 7 → 8 (`CONTENT_KIND_LABEL`) |
| `infra/services/otel-collector/collector.yaml` | keep_keys, exact `^(main\|screen)$`, lone-key + pair rules, ledger, `max_streams` 1000 |
| `docs/observability/label-taxonomy.md` | `content_kind` carried, projection table (SSoT hub), query rule |
| `crates/dt-guard/src/client_metrics_export.rs` | `kind_projection_*` / `kind_collector_rule_drift` / `kind_key_not_kept` legs |
| `crates/dt-guard/src/metric_labels.rs`, `ts_metric_naming.rs` | `kind_label_literal`, `kind_label_on_mh`, `kind_label_vacuous` |
| `crates/dt-guard/src/alert_rules.rs`, `dashboard_panels.rs` | `kind_label_co_occurrence` (per unit) + docs PromQL pass |
| `crates/dt-guard/src/proto_no_any.rs`, `internal_proto_no_key_material.rs` | bash guard ports on `proto_lex` |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` | `KindLabelsSplit`, `KindLabelOnMh` (both keys), content values |

## Devloop Verification Steps

Gate 2 = `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree: **TOTAL_RESULT=OK**, TOTAL_DURATION=2144s. Log: scratchpad `gate2.log`.

| Layer | Verb | Result | Duration |
|-------|------|--------|----------|
| 1 | Compile (rust, ts, proto) | OK | 5s |
| 2 | Format | OK | 4s |
| 3 | Guards (all, incl. the two ported proto guards, kind-label rules, docs PromQL pass) | OK | 164s |
| 4 | Test (rust nextest, ts vitest) | OK | 164s |
| 5 | Lint | OK | 1s |
| 6 | Audit (cargo/pnpm audit, buf breaking) | OK | 8s |
| 7 | Env-tests (dev-cluster + Rust env-tests + browser e2e) | OK — `env-tests-passed`; browser e2e 30 passed (9.0m) | 1798s |

Fast-check history: attempt 1 FAIL at Layer 4 (`a_missing_row_is_named_and_a_duplicate_or_absent_table_is_vacuous`) was a race — the Lead's run started while the implementer was landing the derived-count change; not counted as a diff failure. Every subsequent fast check was green.

**Collector acceptance harness** (`scripts/otel-collector/acceptance.sh`, not part of any layer; run by the Lead per `gc-deployment.md` because `collector.yaml` changed): **RESULT: PASS** against the dev cluster — all `mk-*` and `ck-*` cases incl. `ck-lone-media`, `ck-lone-content`, `ck-poisoned-by-media`, nil-guard and verbatim-name cases. Collector self-telemetry: `streams_limit 1000`, `streams_tracked 27`, RSS ≈205MB, `receiver_refused_metric_points 0`.

---

## Code Review Results

Verdict table: §Planning → Gate 3 — Review Verdicts.

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 1 found, 1 fixed, 0 deferred

S-1: the collector's pair-poisoning skipped LONE kind keys, so the stream ledger was a floor written as a bound → lone-key completion rules, ledger restated as a FLOOR, transform scope (client metrics only) stated. Proto ports verified at parity (all five tokens, header scan, `guard:ignore` not honoured, depth-0 enum resolution). Rulings: pair-poisoning and `max_streams` 1000 accepted.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

TF-1: the TS kind rule ran after the targets-absent exit (wrong root reported OK) → runs first; empty-root test. TF-2: the Rust home check was a substring test, and two vacuity branches were untested → counted construct check + present-but-broken home, zero-sites and value-less TS home tests.

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 3 found, 3 fixed, 0 deferred

F1: collector half-pairs (= S-1/OPS-1/PC-1). F2: co-occurrence checked per expression let a mixed ratio pass → per-unit check. F3: docs PromQL was unguarded → docs pass in `alert-rules-policy` (18 doc hits fixed). Rulings: co-presence with the stale-tab cost accepted; pair-poisoning approved.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 8 found, 8 fixed, 0 deferred

R1 `KindLabelOnMh` rename; R2 one identifier-boundary helper; R3 two long fns split into legs/helpers; R4 `KindTexts` struct; R5 proto lex error surfaced; R6 one co-occurrence message; R7 `tempfile`; R8 destructuring + `const fn served()`.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
- F1 identifier boundary ×3 → `common/ident.rs`; import keyword → `proto_lex::starts_with_keyword` (fixed)
- F2 co-occurrence id/message ×2 → `common/kind_labels.rs` (fixed)
- F3 `MediaKind` value extraction ×2 → `proto_lex::enum_values` (fixed)
- F4 `skip_ws` ×2 → `common/ts_lex.rs` (fixed)
- F5 unscoped `pass =` reader → `identity_label_cases` (fixed)
- F6 sibling-reader notes at the three taxonomy readers (fixed)
- F7 two remaining hand-rolled fence readers → `markdown_code` (fixed)

**Extraction opportunities** (appended to `docs/TODO.md`):
None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 4 found, 4 fixed, 0 deferred

OPS-1 ledger undercount (half-pairs) → co-presence, 6-tuple ledger; OPS-2 memory line for the cap raise; OPS-3 non-atomic browser rollout/revert note in `mc-deployment.md`; OPS-4 runbook PromQL now names both labels (and is guarded by the docs pass).

### Semantic Guard Reviewer
**Verdict**: CLEAR (not spawned)
**Native verdict**: n/a
**Findings**: 0 found, 0 fixed, 0 deferred

Not spawned: no `checks.md` surface (metric labels and guard ports; no credential or key material handled at runtime).

### Paired Client
**Verdict**: RESOLVED-FIXED — PC-1 lone-key collector hole, PC-2 TS guard masking, PC-3 PromQL `'`/backtick and computed-key forms.

### Protocol
**Verdict**: RESOLVED-FIXED — P-1 `LabelledMediaKind` constructor type (client), P-2 top-level `enum MediaKind` only, P-3 INDEX pointer. No `proto/**` edits.

### Infrastructure
**Verdict**: RESOLVED-FIXED — IR-1 masking, IR-2 home check, IR-3 ledger tail, IR-4 rollout note, IR-5 tempfile, IR-6 walkers propagate errors. WalkDir (not `git ls-files`) for the docs pass accepted.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `235c9f379b06d21940d38bf77323741e976c7dd9`
2. Review all changes: `git diff 235c9f37..HEAD`
3. Soft reset (preserves changes): `git reset --soft 235c9f37`
4. Hard reset (clean revert): `git reset --hard 235c9f37`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit is the unit — GC filter (`MEDIA_DATAPOINT_EXTRA`) + collector (keep_keys + `^(main|screen)$`) + MC + client + rules + dashboards. Safe direction: the receiving side (GC + collector) rolls out BEFORE MC/client and is reverted AFTER them; a client emitting `content_kind` into a GC that does not admit it counts a datapoint PII drop on every honest client and strips the label. The proto-guard ports are independently revertable. Browser tabs do not revert atomically: tabs on the new bundle keep sending `content_kind` into a reverted GC (drop counter ticks until reload — expected). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Collector bound written as a fact while it was a floor
**Problem**: the first pair-poisoning rule only fired on an `invalid` key; a patched client omitting one key kept half-pairs, so the stream ledger's "5 tuples" undercounted (11 per kind) — raised independently by four reviewers.
**Resolution**: lone-key completion rules ahead of pair-poisoning (all-or-none at the store); ledger recomputed at 6 tuples incl. absent/absent, plus the forged-on-every-name figure, stated as a FLOOR; acceptance cases for both lone-key shapes (PASSED at Gate 2).

### Issue 2: Drift test first planned in the wrong home
**Problem**: the plan put the TS-source and collector parses inside an mc-service unit test — a second TS reader without `ts_lex` and a second collector reader.
**Resolution**: Lead ruling — the runtime leg stays in mc-service; the file-vs-file legs moved onto `client_metrics_export.rs`'s existing readers; expected row/entry sets derived from the proto enum rather than hand counts.

### Issue 3: Edits racing the Lead's validation runs
**Problem**: a Layer-4 failure reported by the Lead came from a run that started before a test fix landed; and an `edit` script anchored on pre-`cargo fmt` text silently did not apply once.
**Resolution**: re-ran layer-fast after every change before reporting; anchors asserted (`assert old in s`) so a missed edit fails the script.

### Issue 4: Deleted tracked files broke a tracked-file guard
**Problem**: deleting the two `.test.sh` files left them tracked, so `no-insecure-browser-flags` (which lists tracked files) failed until the deletions were staged.
**Resolution**: Lead staged both deletions (`git rm --cached`).

## Lessons Learned

1. A budget figure is only as good as the rule it assumes: write the adversarial case from the matcher's gates (`!= nil` here), not from the values it was meant to see, and say whether the number is a bound or a floor.
2. A rule "every expression names both labels" must be checked at the unit that can go wrong (each selector and clause); checked per expression, a mixed ratio passes while folding screen into camera.
3. Derive expected counts from the authoritative artifact (the proto enum) and still assert them exactly; a hand-written `3/2/2` passes silently once the artifact grows.
4. A guard's ordering is part of its correctness: an early exit or early bail ahead of a rule turns a wrong root or a second finding into a false OK.
5. "THE one parser" is a claim reviewers will check — when introducing a shared reader, move every existing consumer in the same change or name the holdouts.

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
