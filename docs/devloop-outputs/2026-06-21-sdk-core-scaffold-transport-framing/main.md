# Devloop Output: sdk-core scaffold + transport + framing

**Date**: 2026-06-21
**Task**: Bootstrap `packages/sdk-core/` (R-9 strict TS + R-10 Vite lib build), `IWebTransport` interface (R-13), `BrowserWebTransport` with `__DEV_TRUST_FINGERPRINT__`-gated `serverCertificateHashes` (R-14), 4-byte BE length-prefix framing codec (R-16 framing portion), ~10 framing/transport unit tests.
**Specialist**: client
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task-9-11`
**User Story**: `docs/user-stories/2026-05-02-browser-client-join.md` (task #9)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `53464c0ee76f9fa363583dc4288db3e57c967111` |
| Branch | `feature/browser-client-join-task-9-11` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-d8381c90` |
| Implementing Specialist | `client` |
| Iteration | `1` |
| Security | `security@session-d8381c90` |
| Test | `test@session-d8381c90` |
| Observability | `observability@session-d8381c90` |
| Code Quality | `code-reviewer@session-d8381c90` |
| DRY | `dry-reviewer@session-d8381c90` |
| Operations | `operations@session-d8381c90` |
| Semantic Guard | `semantic-guard@session-d8381c90` |

### Gate 1 — Plan Confirmation: ALL 7 CONFIRMED ✅ (classification guard: STATUS=OK)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (Gate 2 notes: cap accumulated buffer ≤MAX; bundle-test path must match guard) |
| Test | confirmed (all 4 resolved: Mock from test-utils; len-prefix not proto-vectored; fast-check→R-41 w/ comment+TODO; bundle-content→test:component tier) |
| Observability | confirmed (note: type framing/transport errors as named variants for later R-24/25/26) |
| Code Quality | confirmed (R-13 connect-as-factory sound per ADR-0028 §159; ownership lens correct) |
| DRY | confirmed (Pattern A correct; note: tighten TODO #58 to name the compiler type-assignment as the drift guarantee) |
| Operations | confirmed (notes: ensure project.json lint target; TODO root-lint entry needs owner+slug; eyeball lockfile diff at Gate 2) |
| Semantic Guard | confirmed (Gate 2 notes: bundle test also assert `__DEV_TRUST_FINGERPRINT__`+fingerprint literal absent; typed framing errors carry len+MAX) |

---

## Task Overview

### Objective
Stand up the `sdk-core` package skeleton and its WebTransport abstraction + framing codec, the foundation later tasks (SignalingClient, MediaTransport, MeetingSession) build on.

In-scope requirements:
- **R-9** (client): strict TS config wiring for the new package (root `tsconfig.base.json` already strict; assess ESLint/Prettier/svelte-check lint-task scope at planning).
- **R-10** (client): `sdk-core` builds via Vite library mode → ESM + CJS + `.d.ts`, `@darktower/` scope (publish config only, no publish).
- **R-13** (client): `IWebTransport` interface in `sdk-core` — canonical home (see DRY note below).
- **R-14** (client, security): `BrowserWebTransport` wrapping browser `WebTransport`; `serverCertificateHashes` gated behind build-time `__DEV_TRUST_FINGERPRINT__` (false + tree-shaken in prod). Dev fingerprint via env/config — never hardcoded/committed.
- **R-16 (framing portion only)** (client, meeting-controller, protocol): 4-byte big-endian length-prefix codec on raw byte payloads, max frame ≤ 64 KiB matching MC's `MAX_MESSAGE_SIZE`. The full `SignalingClient` (proto encode/decode, JoinRequest) is a LATER task — out of scope here.
- ~10 framing/transport unit tests (Vitest, against `MockWebTransport` from `@darktower/test-utils`).

### Scope
- **Service(s)**: client SDK only (`packages/sdk-core/`).
- **Schema**: No.
- **Cross-cutting**: New TS package; root lint/build config touch possible (R-9).

### Wire contract reference (R-16)
MC's framing (`crates/mc-service/src/webtransport/connection.rs:590-655`): 4-byte BE `u32` length prefix + payload; rejects `msg_len == 0` (empty) and `msg_len > 64*1024`. The codec MUST match this exactly (read/write symmetry, partial-read buffering across stream chunks, max-frame enforcement).

### Canonical-home note (DRY, R-13)
`packages/test-utils/src/contracts/IWebTransport.ts` currently holds a minimal `IWebTransport` (Pattern A — parallel structural decl, tracked in `docs/TODO.md`). This task lands the canonical `IWebTransport` in `sdk-core`. Implementer + DRY reviewer decide at planning: keep Pattern A (sdk-core declares its own, MockWebTransport satisfies both via structural typing — no circular dep) vs. flip test-utils to consume sdk-core. Default expectation: **keep Pattern A** unless the shape meaningfully diverged. No prod-build dev-dep edge from sdk-core onto test-utils.

### Debate Decision
NOT NEEDED — implementation within ADR-0028 boundaries; framing contract is fixed by MC.

---

## Cross-Boundary Classification

(Implementer populates the authoritative table in the plan. Expected: nearly all rows `Mine` — new `packages/sdk-core/**` files. Root `package.json` / lint-config edits for R-9, if any, classify carefully; `packages/sdk-core/**` is NOT a Guarded Shared Area.)

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/sdk-core/package.json` | Mine | — |
| `packages/sdk-core/project.json` | Mine | — |
| `packages/sdk-core/tsconfig.json` | Mine | — |
| `packages/sdk-core/tsconfig.build.json` | Mine | — |
| `packages/sdk-core/vite.config.ts` | Mine | — |
| `packages/sdk-core/vitest.config.ts` | Mine | — |
| `packages/sdk-core/vitest.component.config.ts` | Mine | — |
| `packages/sdk-core/src/globals.d.ts` | Mine | — |
| `packages/sdk-core/src/index.ts` | Mine | — |
| `packages/sdk-core/src/framing/length-prefix.ts` | Mine | — |
| `packages/sdk-core/src/transport/IWebTransport.ts` | Mine | — |
| `packages/sdk-core/src/transport/BrowserWebTransport.ts` | Mine | — |
| `packages/sdk-core/src/transport/types.ts` | Mine | — |
| `packages/sdk-core/src/transport/errors.ts` | Mine | — |
| `packages/sdk-core/src/**/__tests__/*.test.ts` | Mine | — |
| `packages/sdk-core/tests/bundle-content.test.ts` | Mine | — |
| `packages/sdk-core/README.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `pnpm-lock.yaml` | Not mine, Mechanical | infrastructure |

**Scope decisions (devloop-local, not deferrals):**
- **R-9 ESLint/Prettier/svelte-check root toolchain — out of this task's scope.** ESLint/Prettier are not set up anywhere in the repo; every package's `lint` target is `tsc --noEmit`. Introducing a root lint toolchain is a monorepo-bootstrap change (root `package.json` devDeps + new configs + every package's `project.json`), infrastructure-owned per R-8 precedent; svelte-check additionally needs Svelte (a later task). In-scope here: sdk-core's own `tsconfig` + a working `tsc --noEmit` `lint` target (repo convention, CI-enforced via `nx run-many -t lint`). Root-lint-toolchain to be routed to infrastructure as its own task + tracked in `docs/TODO.md`.
- **`@vitest/coverage-v8` tooling — deferred to R-41 (Lead decision).** The R-41 ≥90% coverage *gate* is itself a later task; this scaffold's obligation is the ~10 framing/transport tests (delivered 18, every transport/framing line executing). `@vitest/coverage-v8` is a workspace-wide test-tooling devDep (test-utils shares the gap) whose addition would widen `pnpm-lock.yaml` beyond the additive sdk-core subtree and break the Mechanical lockfile classification. Layer-3 presence-based `test-coverage` guard passes. Tracked in `docs/TODO.md` (slug `vitest-coverage-v8-tooling`, Owner: test).
- **R-13 `IWebTransport` canonical home — Pattern A retained.** sdk-core declares the canonical interface; the minimal `packages/test-utils/src/contracts/IWebTransport.ts` copy stays; `MockWebTransport` satisfies both via TS structural typing. No package dep edge either direction (test-utils is a sdk-core devDependency only). Matches `docs/TODO.md` #58; shape unchanged so the revisit-trigger is not hit.

---

## Planning

Plan delivered by implementer 2026-06-21. Key API surface:
- **Framing** (`src/framing/length-prefix.ts`): `MAX_MESSAGE_SIZE = 64*1024`; `encodeFrame(payload): Uint8Array` (4-byte BE u32 prefix + payload; throws on zero-length and >max); `class FrameDecoder` with `push(chunk): Uint8Array[]` streaming parser (buffers partial prefix/body across chunks, multiple frames per chunk, rejects len==0 and len>max before buffering body). Matches MC `connection.rs:590-655` exactly.
- **Transport**: canonical `IWebTransport` (`ready`/`closed` promises, `datagrams`, `createBidirectionalStream()`, `close(info?)` — identical shape to existing test-utils copy; `connect` modeled as a factory on `BrowserWebTransport`, not an interface method, to preserve structural compat with the browser API + mock). `BrowserWebTransport implements IWebTransport`; `serverCertificateHashes` assembled only inside `if (__DEV_TRUST_FINGERPRINT__) {...}` (Vite `define` injects `false` → branch tree-shaken from prod bundle); fingerprint value from runtime config, never hardcoded.
- **Build**: Vite library mode (ESM `.mjs` + CJS `.cjs` + `.d.ts` via vite-plugin-dts), `@darktower/sdk-core` scope, `private`, closed exports map — mirrors test-utils.
- **Tests** (~11): 10 framing/transport unit tests + `tests/bundle-content.test.ts` (production `vite build`, asserts no `serverCertificateHashes` string in `dist/` — R-14 forcing function per `docs/TODO.md` #130).

---

## Gate 2 — Validation Pipeline (`scripts/layer-all.sh`)

| Layer | Verb | Result | Notes |
|-------|------|--------|-------|
| 1 | Compile | OK | cargo build + nx typecheck (sdk-core, test-utils) |
| 2 | Format | OK | cargo fmt + nx format (buf) |
| 3 | Guards | OK | 33/33 pass — incl. `no-dev-trust-path-in-prod-bundle`, `name-guard-dt-client`, `no-secrets-in-ts`, `exports-map-closed`, `validate-cross-boundary-{classification,scope}` |
| 4 | Test | OK (for changeset) | **sdk-core: 13 unit + 3 component passed**; test-utils 14 passed (`nx-test-passed`). One Rust flake — see below. |
| 5 | Lint | OK | cargo clippy + nx lint |
| 6 | Audit | OK | cargo-audit + pnpm-audit (no new advisories) + buf-breaking |
| 7 | Env-tests | N/A | `wave2-pending` (Layer 7 not active in this environment) |

**Attempt 1** failed at Layer 3: `validate-cross-boundary-scope` flagged `src/globals.d.ts` + `vitest.component.config.ts` missing from the plan table (Lead omission, not a code issue) — fixed by adding the rows. **Attempt 2**: all layers green except one flaky Rust test.

**Post-review final re-validation (after all 4 fixes landed): FULLY GREEN.** Layers 1 OK / 2 OK / 3 OK / 4 OK (Rust suite passed, 165s — the timing test passed, confirming the flake) / 5 OK / 6 N/A / 7 N/A. Zero FAIL lines (`TOTAL_RESULT=N/A` reflects only the N/A layers 6/7; worst real layer = OK).

**Flaky-test disposition (does NOT consume a Gate-2 attempt):** `ac_service::services::token_service::tests::test_issue_user_token_timing_attack_prevention` failed once in Layer 4. It is (a) a timing-based test (inherently non-deterministic), (b) in `crates/ac-service` which this TS-only devloop did not touch (zero Rust changes in the working tree — the Rust suite ran only because the pipeline diffs against the branch mergebase, which carries the branch's prior Rust history), (c) **confirmed to pass on isolated re-run** (`cargo test -p ac-service --lib ...test_issue_user_token_timing_attack_prevention` → `ok`). Per ADR-0033/skill flaky-failure handling, treated as infrastructure-class flakiness: Gate 2 passes for this devloop's changeset. Follow-up recommendation (out of scope, owner auth-controller/test): harden the timing-attack test against non-determinism.

---

## Code Review Results

### Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | **CLEAR** | Empirically verified prod `dist/` clean of `serverCertificateHashes` + `__DEV_TRUST_FINGERPRINT__` (true DCE, not runtime guard); DoS reject-before-buffer confirmed; field-rename judgment-call approved |
| Test | **RESOLVED-DEFERRED** | 1 fixed (BrowserWebTransport runtime coverage, unit now 18); 1 deferred w/ tracking (`@vitest/coverage-v8` provider → R-41) |

**Gate 3 outcome: APPROVED.** No escalations. 3 CLEAR (Security, Code Quality, Semantic Guard) + 3 RESOLVED-FIXED (Observability, DRY, Operations) + 1 RESOLVED-DEFERRED (Test). Four findings raised during review, all fixed in-changeset (typed `TransportError`; collapsed `BrowserWebTransportLike`; `npx`→`pnpm exec`; BrowserWebTransport runtime tests). One accepted deferral (below).

## Accepted Deferrals

- `docs/TODO.md` §R-14 Transition / tooling (`vitest-coverage-v8-tooling`) — install `@vitest/coverage-v8` for the R-41 ≥90% coverage gate (Owner: test; test-utils shares the gap).

(Scope decisions — `@vitest/coverage-v8` rationale, fast-check→R-41, R-9 root-lint→infrastructure, Pattern A — are recorded under §Cross-Boundary Classification scope notes, not here; those are scope boundaries, not findings left in the diff.)

## Files Modified

New package `packages/sdk-core/` (18 files): build/Nx config (package.json, project.json, tsconfig{,.build}.json, vite.config.ts, vitest{,.component}.config.ts, README.md), source (index.ts barrel, globals.d.ts, framing/length-prefix.ts, transport/{IWebTransport,BrowserWebTransport,types,errors}.ts), tests (framing/__tests__/length-prefix.test.ts, transport/__tests__/BrowserWebTransport.test.ts, tests/bundle-content.test.ts). Shared: `docs/TODO.md` (tracking entries — Mine), `pnpm-lock.yaml` (additive, Mechanical/infrastructure). Tests: 18 unit + 3 component, all green.
| Observability | **RESOLVED-FIXED** | 1 finding fixed: added typed `TransportError`/`TransportErrorCode` + test 11d (symmetry with `FramingError`; keeps R-24/25 a pure add) |
| Code Quality | **CLEAR** | no findings; both judgment-calls verified guard-necessary; Ownership Lens confirms all-Mine + pnpm-lock Mechanical |
| DRY | **RESOLVED-FIXED** | 1 finding fixed: collapsed `BrowserWebTransportLike` into `IWebTransport` (−10 LoC); Pattern A held; keep test-only alias (not `dependsOn ^build`) |
| Operations | **RESOLVED-FIXED** | 1 finding fixed: `npx`→`pnpm exec` (hermetic/R-34) in bundle test; lockfile verified additive; TODO entries well-formed |
| Semantic Guard | **SAFE/CLEAR** | no findings; both Gate-1 strengthening notes verified in code |
