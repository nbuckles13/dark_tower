# Devloop Output: Client Telemetry Scaffolding (Obs Task A)

**Date**: 2026-06-24
**Task**: Client telemetry scaffolding — MetricsSink + sinks, nameGuard, global MeterProvider/WebTracerProvider, W3C trace propagation helper, bounded-event logger, client.md catalog stub (R-19, R-24, R-25, R-26, R-27)
**Specialist**: observability
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-12`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5ea7d720da6a37b22ef4230e4040d3d8cbac7daf` |
| Branch | `feature/browser-client-join-task-12` |
| User Story | `docs/user-stories/2026-05-02-browser-client-join.md` task #12 |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-c2944954` |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `security@session-c2944954` |
| Test | `test@session-c2944954` |
| Observability | `observability@session-c2944954` |
| Code Quality | `code-reviewer@session-c2944954` |
| DRY | `dry-reviewer@session-c2944954` |
| Operations | `operations@session-c2944954` |
| Semantic Guard | `semantic-guard@session-c2944954` |

---

## Task Overview

### Objective
Land the client-side telemetry scaffolding for the browser SDK (`@darktower/sdk-core`): the `MetricsSink` interface + production/dev/noop implementations, the `dt_client_*` naming guard, a single global OTel `MeterProvider` + `WebTracerProvider` configured via `MeetingSession.configure`, the `W3CTraceContextPropagator` registration, a `tracePropagation.injectIntoClientMessage` helper that works for both `ClientMessage` and `MhClientMessage` envelopes, a bounded-event structured logger, and the `docs/observability/metrics/client.md` catalog stub.

This is **scaffolding** — the actual emission sites (join-flow metrics, signaling trace injection) land in later tasks (#13+). Task #12 provides the surfaces those tasks consume.

### Scope
- **Service(s)**: client SDK — `packages/sdk-core/` (+ possibly `packages/test-utils/` for sink contract canonicalization)
- **Schema**: No
- **Cross-cutting**: Observability surface used by later client tasks. New npm deps (`@opentelemetry/*`).

### Requirements
- **R-19**: W3C propagator helper populates `trace_parent`/`trace_state` on outbound `ClientMessage` AND `MhClientMessage` envelopes via one injection path; root span `dt_client.join` per join (span creation itself is later task's emission, helper is here).
- **R-24**: `MetricsSink` abstraction in `packages/sdk-core/src/telemetry/`; `OtelMetricsSink` (OTLP-proto to GC `/api/v1/telemetry` with `keepalive`), `ConsoleMetricsSink` (dev), `NoopMetricsSink` (default). `nameGuard` rejects non-`dt_client_*` names (throw dev/test, warn prod). `InMemoryMetricsSink` already lives in test-utils.
- **R-25**: Catalog/definition of the `dt_client_*` join-flow metrics (documented in client.md; emission deferred).
- **R-26**: Bounded-`event`-enum structured logger (console-only this story) with required + excluded fields.
- **R-27**: `docs/observability/metrics/client.md` stub ("Story 1 stub — full catalog deferred").

### Key context for planning
- Proto TS types are **not yet generated** (task #6/#13). Proto `ClientMessage`/`MhClientMessage` already carry `trace_parent = 20` / `trace_state = 21` (verified in `proto/dark_tower/signaling/v1/signaling.proto`). The injection helper must be designed against a structural shape (e.g. `{ traceParent?: string; traceState?: string }`) rather than importing generated types that don't exist yet.
- `MeetingSession` does **not** exist yet (R-22 is a later task). The task names `MeetingSession.configure` as the configuration entry point — implementer must decide whether to introduce a minimal `MeetingSession` shell now or site the config entry point in the telemetry module and reconcile later. Resolve at Gate 1.
- `MetricsSink` contract already exists as a co-owned placeholder in `packages/test-utils/src/contracts/MetricsSink.ts`; task #12 establishes the canonical home in `sdk-core`. Pattern B coordinated convention — observability is the named convention author (ADR-0024 §6.5). Decide canonical-vs-re-export to avoid drift.
- `sdk-core` currently has **zero** runtime deps. This task adds `@opentelemetry/*` packages → `package.json` + `pnpm-lock.yaml` change → Layer 6 `pnpm audit` + supply-chain review (security/operations).

### Cross-Boundary / GSA notes
- No `proto/**` edits (GSA untouched — the proto trace fields already exist). The helper consumes the wire contract; it does not define it.
- New npm dependencies require full mode (already full).
- Plan must include the `## Cross-Boundary Classification` table (every touched file classified; most rows `Mine` — client/observability surface).

---

## Cross-Boundary Classification

| File | Classification | Owner |
|------|----------------|-------|
| `packages/sdk-core/src/telemetry/*.ts` (all new) | Mine | client/observability |
| `packages/sdk-core/src/telemetry/__tests__/*` | Mine | observability |
| `docs/observability/metrics/client.md` | Mine | observability |
| `packages/sdk-core/src/index.ts` | Mine | client |
| `packages/sdk-core/src/globals.d.ts` | Mine | client |
| `packages/sdk-core/vite.config.ts` | Mine | client |
| `packages/sdk-core/vitest.config.ts` | Mine | client |
| `packages/sdk-core/package.json` (OTel deps) | Mine | client |
| `packages/test-utils/src/contracts/MetricsSink.ts` (Pattern A parallel copy) | Minor-judgment | test |
| `pnpm-lock.yaml` (sdk-core OTel deps) | Mechanical | repo-root |
| `docs/TODO.md` (line 62 resolve + bundle-budget entry) | Minor-judgment | repo-root/process |
| `packages/sdk-core/src/errors/index.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/errors/__tests__/error-branches.test.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/errors/__tests__/error-hierarchy.test.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/http/origin.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/http/AuthApiClient.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/http/MeetingApiClient.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/http/__tests__/auth-api-client.test.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/http/__tests__/http-branches.test.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `packages/sdk-core/src/http/__tests__/meeting-api-client.test.ts` (pre-existing — `prettier --write` only) | Mechanical | client |
| `crates/dt-guard/src/ts_metric_naming.rs` (new `// dt-metric-name-dynamic:` escape hatch + tests) | Minor-judgment | observability-policy (metric naming, R-24/ADR-0011) + infrastructure-tooling (guard mechanism, ADR-0034) — co-owned |

No Guarded Shared Area paths touched (`proto/**` untouched — trace fields 20/21 already exist). Owner `test` confirmation on the Minor-judgment `test-utils` row obtained at Gate 1 (security, test, code-reviewer, dry all explicitly affirmed); re-affirmed at Gate 3 via Ownership Lens.

**Pattern A fallback (post-implementation)**: the canonical-home re-export FAILED the Nx build-graph check (`sdk-core:build → test-utils:build → sdk-core:build` circular dep — sdk-core already devDepends test-utils). Per the panel's pre-approved fallback, reverted to **Pattern A parallel structural copies**: canonical `MetricsSink` in sdk-core; `test-utils/src/contracts/MetricsSink.ts` is a shape-identical copy (no new package edge, no devDep, no lockfile edge from this). Anti-drift lock: `packages/sdk-core/src/telemetry/__tests__/contractParity.test.ts` — a compile-time mutual-assignability test that fails `tsc` if the two copies diverge. (DRY pre-approved intentional + machine-enforced duplication; test got its shape-identity guard.)

**Scope corrections (Lead-directed, pre-Gate-2)**:
- `eslint.config.js` edit (a `no-unused-vars` policy rule the implementer added) was **reverted** — it is a Minor-judgment cross-boundary change to a client-owned shared config, and no client reviewer is on this panel to confirm it. Unused bindings handled in-package instead (bare Noop method params via TS structural typing; optional `catch {}` binding).
- **9 pre-existing prettier-failing files** (`src/errors/`, `src/http/` from task #11 commit `889039f`) block sdk-core's whole-package `lint` target. Per **user decision**, a mechanical `prettier --write` of those files is **folded into this commit**, classified Mechanical/owner-client (review-only, no semantic change). Flagged in final report as a task #11 / CI process gap.

### Amended plan decisions (post Gate-1 review)
- **OTel externalized** (Vite `external: /^@opentelemetry\//`) — dist emits bare imports, `bundle-content.test.ts` expectations unchanged; `OtelMetricsSink` takes `Meter` via constructor injection (unit tier mocks OTel).
- **nameGuard** runtime `mode: 'throw' | 'warn'` param (default `throw`); both branches testable in one Vitest config. Regex pinned to the **compiled** guard `^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$` (`crates/dt-guard/src/ts_metric_naming.rs:68`), not the `{0,53}` doc form.
- **MetricsSink canonical** in sdk-core; `test-utils` `export type`-only re-export; new edge is a **devDependency**; `nx run-many -t build` topo-sort to be verified before "Ready for validation" (Pattern A fallback if Nx rejects).
- **`configureTelemetry()`** module fn (no MeetingSession shell); R-22 will delegate `MeetingSession.configure` → `configureTelemetry`.
- **Bounded `close_reason`** enum + `normalizeCloseReason` helper; client.md carries R-27 stub marker + declared-vs-emitted honesty note.

---

## Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (4 Gate-2 verification conditions) |
| Test | confirmed (after amendment — externalization, nameGuard mode seam, devDep/acyclic all resolved) |
| Observability | confirmed — 3 items folded into plan (compiled-regex pin, bounded `close_reason`, catalog honesty markers) |
| Code Quality | confirmed — both classifications stand; 2 Gate-2 verification notes (SoT removal, dep direction) |
| DRY | confirmed — wants explicit "Nx build graph stays acyclic" plan statement |
| Operations | confirmed — bundle-budget TODO.md entry wanted at Gate 2 |
| Semantic Guard | confirmed — Gate-2 watch-items only |

---

## Gate 2 — Validation

- **Pre-req resolved**: `dt-guard` release binary pre-built by Lead (TS-only change skips the Rust compile path that normally builds it; Layer 3 guards need it). Classification-sanity guard now machine-passes: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.
- **Attempt 1**: Layers 1,2,4,5,6 GREEN (pnpm-audit clean, buf-breaking clean, coverage ≥90% passed, lint clean, build clean); Layer 7 N/A. **Layer 3 FAILED** — 4 guards:
  1. `name-guard-dt-client` (3) — `OtelMetricsSink.ts:60/70/80` `meter.createX(name)` non-literal (generic sink forwards a runtime-validated variable; guard has no allow annotation). → routed to implementer.
  2. `no-pii-in-logs-ts` (3) — `ConsoleMetricsSink.ts:32/37/42` — `name` is in the PII token vocabulary; false positive. → routed to implementer (`// pii-safe:` annotation).
  3. `validate-cross-boundary-scope` — **Lead-fixed**: main.md used a glob + a combined vite/vitest cell; replaced with explicit one-path-per-row entries. Now `STATUS=OK no-drift`.
  4. `validate-todo-tracking` — **pre-existing task-#11 defect** (`2026-06-23-...task#11/main.md:428` inline-debt-body). **Lead-fixed** in a standalone commit `359f791` (meaning-preserving; committed separately so the scope guard's active-edit posture sees only task #12's main.md). Now `STATUS=OK`.
- **Pre-existing task-#11 defects surfaced by always-run guards this devloop** (3 total): 9 prettier-dirty files (folded mechanical fix, per user), the §Accepted-Deferrals inline-debt-body (standalone commit `359f791`). Root cause: task #11 (commit `889039f`) appears to have committed without the full guard/prettier suite — flagged for the final report.
- **Attempt 2: PASS.** Layers 1–5 OK; Layer 6 cargo-audit/pnpm-audit/buf-breaking all OK (aggregate label N/A); Layer 7 N/A (client env-test/Playwright harness wave2-pending — no cluster-observable behavior in this scaffolding change). Layer 3 = **33/33 guards passed, 0 failed** (`name-guard-dt-client` ✓, `no-pii-in-logs-ts` ✓). Coverage gate ✓. dt-guard 306 lib tests ✓.

---

## Gate 3 — Final Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Structural PII safety; deps exact-pinned; hatch safe. Noted pre-existing moderate js-yaml (nx toolchain) → separate infra dep-bump |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | Coverage 100/95.62; all sink/guard/inject cases covered; 9 prettier files pure reformat. Parity-header finding (shared w/ DRY+CQ) fixed |
| Observability | RESOLVED-FIXED | 2 | 2 | 0 | regex lockstep now bidirectional (new Rust test); catalog honesty + bounded close_reason confirmed; **blessed dt-guard hatch** (3/3) |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | ADRs 0011/0024/0028/0034 COMPLIANT; Ownership Lens all-correct; **blessed dt-guard hatch** (with mutation test); 3 doc/precision findings fixed |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | "shape-identical" header overstated parity guarantee — reworded. Pattern A + parity test confirmed. Extraction candidate → TODO.md (not a deferral) |
| Operations | CLEAR | 0 | 0 | 0 | Deps clean; bundle-budget TODO captured; **blessed dt-guard hatch** (1/3); failure-soft OK; task-#11 CI-gap analysis |
| Semantic Guard | CLEAR | 0 | 0 | 0 | credential-leak/error-context/escape-hatch all clean |

---

## Accepted Deferrals

**None.** All 12 findings raised across the 7 reviewers were fixed in-diff (zero deferred, zero spun-out). Verdicts: Security CLEAR, Test RESOLVED-FIXED, Observability RESOLVED-FIXED, Code Quality RESOLVED-FIXED, DRY RESOLVED-FIXED, Operations CLEAR, Semantic Guard CLEAR.

---

## Post-Completion Notes (non-deferrals)

<!-- Relocated out of §Accepted Deferrals (which is genuinely "None" for this
     devloop) so the always-run validate-todo-tracking guard sees a
     pointer-only/None deferrals section. Content below is unchanged — these are
     forward-looking notes and pre-existing-defect observations, NOT deferred
     findings left in this devloop's diff. Standalone doc-hygiene fix; see commit. -->

### Forward-looking follow-ups (NOT deferrals — in `docs/TODO.md`)
These are future-task dependencies authored as planned scope, not findings left in the diff:
- **Guard call-site checking** (#13/#14): extend `name-guard-dt-client` to also check `sink.counter('dt_client_*')` literal CALL SITES once emission lands — the `dt-metric-name-dynamic` marker covers generic-sink internals only, not a license to skip literal-call-site checking.
- **OTel bundle-size budget** (#15+): externalized OTel shifts bundling to web-app; track as a deliberate budget decision.
- **DRY extraction candidate**: name-guard prelude shared between `ConsoleMetricsSink` + `OtelMetricsSink` (N=2, below ADR-0019 threshold; Noop deliberately skips the guard — trigger is a 3rd name-guarded sink).

### Pre-existing task-#11 defects surfaced this devloop (flagged for follow-up, not task #12's work)
The always-run guards surfaced three latent defects in task #11's output (commit `889039f`), suggesting task #11's Gate 2 did not run the full guard/prettier suite over its changeset:
1. 9 sdk-core files (`src/errors/`, `src/http/`) committed prettier-dirty → mechanical `prettier --write` folded into this commit per user decision.
2. §Accepted-Deferrals inline-debt-body → fixed in standalone commit `359f791`.
Operations' recommendation: the ADR-0033 always-run formatting gate (which caught these here) is the durable fix; worth confirming the client CI lint gate runs the package `lint` target over the full changeset.

### Separate infra follow-up (not this task)
- Pre-existing **moderate** advisory `js-yaml` GHSA-h67p-54hq-rp68 via `nx > @yarnpkg/parsers` — dev-toolchain transitive, not browser-shipped, below the high-audit gate, not introduced by #12. Belongs in a separate infra dep-bump.

---

## Final Summary

**Status: COMPLETE.** Client telemetry scaffolding (R-19, R-24, R-25, R-26, R-27) landed in `@darktower/sdk-core`:
- `MetricsSink` (canonical) + `NoopMetricsSink` (default) / `ConsoleMetricsSink` (dev) / `OtelMetricsSink` (prod, constructor-injected Meter, OTLP-proto to GC `/api/v1/telemetry`, adaptive `keepalive`).
- `nameGuard` (runtime `throw`/`warn` seam; regex pinned verbatim + bidirectionally locked to the compiled CI guard).
- `configureTelemetry` (single global MeterProvider + WebTracerProvider, W3C propagator) — R-22's `MeetingSession.configure` will delegate here.
- `tracePropagation.injectIntoClientMessage<T extends TraceCarrier>` — one path for both `ClientMessage` + `MhClientMessage`.
- Bounded-event `logger` (allowlist-projecting, PII-safe by construction) + bounded `CloseReason` enum.
- `docs/observability/metrics/client.md` catalog stub (5 R-25 metrics, "Story 1 stub" + declared-vs-emitted markers).
- New `// dt-metric-name-dynamic:` guard escape-hatch (blessed 3/3 by Operations + Code Quality + Observability).

**Notable**: OTel externalized (dist 22.1 kB unchanged); Pattern A fallback (re-export hit the predicted Nx cycle) drift-locked by `contractParity.test.ts`; coverage 100% stmts / 95.62% branch; Gate 2 needed 2 attempts (Layer-3 guard fixes); 13 total reviewer findings (12 + the standalone task-#11 fix) all resolved.
