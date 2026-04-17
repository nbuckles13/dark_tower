# Devloop Output: ADR-0031 metric-labels guard + label taxonomy

**Date**: 2026-04-17
**Task**: Implement `scripts/guards/simple/validate-metric-labels.sh` + `docs/observability/label-taxonomy.md` per ADR-0031 prerequisite #3 + label portion of #4. Final ADR-0031 prerequisite.
**Specialist**: observability (implementer); security co-reviews denylist composition per ADR-0031 §Ownership split
**Mode**: Agent Teams (light)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c10dde2384331de675226fd0eca119745d6538ca` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-metric-labels-guard` |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `CLEAR (security@devloop-metric-labels-guard) — 2 findings fixed in revision pass` |
| Code Quality | `RESOLVED (code-reviewer@devloop-metric-labels-guard) — 1 finding fixed (probe_tmp.rs) + 1 non-blocking nit (allowlist speculation)` |
| Context Reviewer | code-reviewer (third reviewer for --light) |

---

## Task Overview

### Objective

Land the third and final ADR-0031 prerequisite: metric-labels guard + label-taxonomy doc.

### Scope

- `scripts/guards/simple/validate-metric-labels.sh` — Rust source parser (multi-line metric macro invocations), PII denylist, cardinality budgets per ADR-0011.
- `docs/observability/label-taxonomy.md` — shared label names, PII denylist spec, cardinality budgets, `# pii-safe:` escape-hatch usage guide.
- Posture: "mechanical is mechanical." Pre-existing metric definitions that violate: mechanical fixes (rename label, add bound) land in-devloop; domain-judgment calls (legitimate need for high-cardinality) flag to Lead.

### Debate Decision

NOT NEEDED — ADR-0031 + ADR-0011 + ADR-0029 define guard semantics. Security co-owns denylist composition per ADR-0031 §Ownership split.

---

## Reference

- Spec: `docs/decisions/adr-0031-service-owned-dashboards-alerts.md` §Prerequisite guardrails #3
- Cardinality budgets: `docs/decisions/adr-0011-observability.md`
- Bounded-labels precedent: MC `errors.rs:error_type_label()`, MH `errors.rs:error_type_label()`
- Guard structure precedent: `scripts/guards/simple/validate-alert-rules.sh`, `scripts/guards/simple/validate-dashboard-panels.sh`
- Conventions precedent: `docs/observability/alert-conventions.md`, `docs/observability/dashboard-conventions.md`

---

## Implementation Summary

Final ADR-0031 prerequisite. Survey-first discipline: implementer scanned all 4 `crates/*/src/observability/metrics.rs` files for PII/cardinality issues before acting — found zero pre-existing violations. Guard + taxonomy + fixtures land in-devloop; no migrations needed.

### New artifacts
- `scripts/guards/simple/validate-metric-labels.sh` — Rust macro-parsing guard. Python3 balanced-paren walker handles multiline `counter!(...)` spans, full-path + short-form macros, string literals, and `//` + `/* */` comments cleanly.
- `scripts/guards/simple/fixtures/metric-labels/` — 25 fixtures covering PII denylist (Cat A + Cat B + raw_* prefix), cardinality (length, unbounded sources), snake_case, `# pii-safe` escape hatch, comment-stripping, adversarial injection probe.
- `docs/observability/label-taxonomy.md` (~431 lines, mirrors alert-conventions.md + dashboard-conventions.md shape).
- `TODO.md` extended with "ADR-0031 label-canonicalization follow-ups" section (3 coordinated-migration entries: AC/GC path/endpoint, MC heartbeat_type, event_type).

### Guard rules (all ADR-0011 + ADR-0031 §Prereq #3 compliant)
- **PII denylist Category A (non-bypassable)**: 12 secrets — `password`, `passwd`, `api_key`, `apikey`, `secret`, `bearer_token`, `access_token`, `refresh_token`, `session_token`, `id_token`, `private_key`, `privkey`, `signing_key`, `jwt`, `auth_header`, `authorization`, `token`. `# pii-safe` cannot suppress. Narrow allowlist for `token_*` shape: `token_type` (currently used in GC/MC/MH).
- **PII denylist Category B (bypassable with `# pii-safe`)**: `email`, `phone`/`phone_number`, `display_name`, `user_id`, `name`, `address`, `ip`/`ip_addr`/`ipv4`/`ipv6`, `device_id`, `ssn`, `dob`, `passport`, `driver_license`, `credit_card`/`card_number`, `cvv`, `username`/`nickname`/`handle`, `user_agent`, `fingerprint`, `latitude`/`longitude`/`geolocation`/`geoip`, `postal_code`/`zip`/`zipcode`.
- **Prefix denylist**: bare `raw_*` labels.
- **Hashed-suffix allow**: `_hash`, `_sha256`, `_digest` etc. exempt Category B (not Category A).
- **Snake_case enforcement**: label keys + metric names.
- **Cardinality (source-level)**: string literal values > 64 chars, metric names > 64 chars.
- **Unbounded-value detection**: `Uuid::new_v4()/to_string()`, `request_path`, user-controlled variables.
- **`# pii-safe: <reason>` escape hatch**: ≥10-char reason, lazy-reason reject (test/tmp/todo/fixme/wip). Scopes to Category B + snake_case only; cannot bypass Cat A or cardinality.

### Scope ruling applied mid-flight
Canonical-naming was initially guard-enforced (implementer's first pass flagged aliases like `svc_type`, `http_method`). Lead ruled reviewer-only: renaming would ripple into every PromQL reference across dashboards + alerts just landed in `c10dde2`/`f5f53f8`. Implementer cleanly reverted; drift documented in `label-taxonomy.md` §Current Drift + 3 TODO.md coordinated-migration entries.

### Review cycle
Clean posture throughout. Security flagged 2 must-fix items (Cat A denylist gaps, comment parse-error); implementer accepted wholesale + extended with ~15 security-proposed extensions. Code-reviewer flagged 1 must-fix (probe_tmp.rs leaked into production crate path); implementer took the stronger option (moved to fixtures as regression test, deleted from crate). Second round: Lead ruled bare `token` → Cat A with narrow allowlist. One non-blocking nit (code-reviewer on speculative allowlist entries) accepted; implementer trimmed to currently-used only.

---

## Files Modified

**New** (4):
- `scripts/guards/simple/validate-metric-labels.sh` (~900 lines)
- `scripts/guards/simple/fixtures/metric-labels/` (25 fixtures)
- `docs/observability/label-taxonomy.md` (~431 lines)

**Modified** (1):
- `TODO.md` (+58 lines: "ADR-0031 label-canonicalization follow-ups" section)

No changes to production `metrics.rs` files (clean survey).

---

## Devloop Verification Steps

- L1 (cargo check): PASS
- L2 (cargo fmt --check): PASS
- L3 (guards): **18/18 PASS** — new `validate-metric-labels` included. Self-test 25/25 fixtures.
- L4/L5 (tests, clippy): trivial — no Rust changes.
- L6 (cargo audit): pre-existing vulnerabilities (not this devloop's concern).
- L7 (semantic): Lead-judgment SAFE — all changes are new shell/fixture/doc files; no Rust/service surface touched.
- L8 (env-tests): skipped — no Rust/service changes.

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 2 raised, 2 fixed (denylist gaps; comment parse-error bug)

Security co-owned denylist composition per ADR-0031. Probe battery verified all 12 Category A secrets + 16 Category B extensions + `raw_*` prefix. Category A non-bypass invariant confirmed. `strip_comments_preserve_layout()` pre-pass locks in via `pass-commented-out-macro.rs` fixture. Adversarial probe preserved as `fail-adversarial-probe.rs` regression fixture.

One documented limitation (non-blocking): unterminated `counter!(` inside a **string literal** still trips parse_error. Extremely rare edge case; documented appropriately.

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 1 raised, 1 fixed (probe_tmp.rs leaked into production crate path)

**ADR Compliance**: ADR-0011 cardinality-budget numerics map 1:1 to guard behavior (≤1000 combos/metric reviewer-only because static analysis can't compute runtime cardinality; ≤64 char literal enforced at `MAX_LITERAL_VALUE_LENGTH`; 5M fleet series runtime-enforced via Prometheus `sample_limit`). Rust macro-parser stress-tested on full-path + short-form + multi-line + comment edge cases. Shell/Python quality matches precedent.

Non-blocking nit (accepted): allowlist originally had 5 speculative `token_*` entries; implementer trimmed to only `token_type` (currently used) — YAGNI.

---

## Rollback Procedure

1. Start commit: `c10dde2384331de675226fd0eca119745d6538ca`
2. Soft reset: `git reset --soft c10dde2`
3. No schema or deployment changes — simple git revert is sufficient.
