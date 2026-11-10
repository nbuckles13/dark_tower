# Devloop Output: GC registration rule (endpoint allowlists, supersede/reject, atomic registration)

**Date**: 2026-11-09
**Task**: Story 4 task GC-2 — GC RegisterMH endpoint allowlists, supersede/reject rule, atomic registration transaction
**Specialist**: global-controller
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~6h (2026-11-09 → 2026-11-10; 2 review iterations, 2 Layer-7 attempts)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d4d6e5089bb06728b8083d7c02e0d59aa8083ddb` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `global-controller` |
| Tier | `full` |
| Iteration | `2` |
| Security | `security` |
| Test | `paired-test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired Infrastructure | `paired-infrastructure` |
| Paired Protocol | `paired-protocol` |
| Paired Auth Controller (internal.proto GSA co-owner) | `paired-auth-controller` |
| Database (conditional) | `database` |

---

## Task Overview

### Objective
See task prompt (run-story task #10, Story 4 GC-2). No two LIVE MH incarnations share an endpoint in GC's registry; sequential reuse permitted; a live serving MH is never displaced; endpoints come only from per-zone allowlists.

### Scope
- **Service(s)**: gc-service (+ proto comment, deploy.sh render, docs, dashboard, env-tests)
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — governed by ADR-0039 §7 and `docs/debates/2026-10-28-mh-replacement-and-migration/gc-register-rule.txt`.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/gc-service/src/mh_endpoint_policy.rs` | Mine — new: typed per-zone allowlists, canonical endpoint grammar, `check()`, config parsers; unit tests | — |
| `crates/gc-service/src/lib.rs` | Mine — module decl | — |
| `crates/gc-service/src/config.rs` | Mine — 5 REQUIRED keys, `MissingEnvVar` literal per key, `InvalidMhEndpointAllowlist`, one error-format helper, tests | — |
| `crates/gc-service/src/grpc/mh_service.rs` | Mine — arm 0 + single outcome-recording/wire-mapping site; logs; tests | — |
| `crates/gc-service/src/grpc/mod.rs` | Mine — shared endpoint bounds helper + `MAX_ENDPOINT_LENGTH` | — |
| `crates/gc-service/src/grpc/mc_service.rs` | Mine — uses the shared helper; comment that the ADR-0039 §7 rule is MH-scoped | — |
| `crates/gc-service/src/repositories/media_handlers.rs` | Mine (database reviews every query) — one advisory-locked txn, FOR UPDATE selects, pure `decide()`, DELETE by ids, guarded upsert, `register_mh_txn` | — |
| `crates/gc-service/src/repositories/meeting_controllers.rs` | Mine — `HealthStatus::try_from_db_str` (strict parse; `from_db_str` delegates) | — |
| `crates/gc-service/src/repositories/mod.rs` | Mine — re-exports | — |
| `crates/gc-service/src/observability/metrics.rs` | Mine (observability reviews) — `gc_mh_registrations_total` closed label enum + recorder + 6-series zero-init; `register_mh` -> `register_mh_txn` | — |
| `crates/gc-service/src/main.rs` | Mine — `MhService::new` wiring | — |
| `crates/gc-service/Cargo.toml` | Mine — `ipnet` (workspace) | — |
| `Cargo.toml` | Mine (Mechanical) — `ipnet = "2"` in `[workspace.dependencies]` (already in the lock graph) | — |
| `Cargo.lock` | Mine (Mechanical) — ipnet edge for gc-service | — |
| `crates/gc-service/tests/**` | Mine — new `mh_registration_tests.rs`, `mh_registration_metrics_integration.rs`; call-site updates; `mh_config_vars()`; rewrite-relying test converted | — |
| `crates/gc-test-utils/src/**` | Mine — new `mh_endpoint_allowlist.rs` (test allowlist, one home; `mh_config_vars()`); harness wiring | — |
| `infra/services/gc-service/config.env` | Mine — 5 keys: 4 EMPTY (render-required), SCHEMES=grpc with the h2c/S6 comment | — |
| `infra/services/gc-service/deployment.yaml` | Not mine, Mechanical — 5 per-key `configMapKeyRef` env entries | infrastructure |
| `infra/kind/scripts/deploy.sh` | Not mine, Domain-judgment (paired) — `gc_mh_allowlist_literals()` + `behavior: merge` generator in `render_env_overlay` (paired-infrastructure wrote) | infrastructure |
| `scripts/setup.test.sh` | Not mine, Domain-judgment (paired) — render assertions, derived expectations, negatives (paired-infrastructure wrote) | infrastructure |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Not mine, Minor-judgment (paired) — pointer to `gc_mh_allowlist_literals` | infrastructure |
| `proto/dark_tower/internal/v1/internal.proto` | Not mine, Domain-judgment (GSA) — comment-only: RegisterMH endpoint-rule block above `RegisterMHRequest`; one-line pointers on `webtransport_endpoint`, `grpc_endpoint`; GSA co-owners auth-controller + security also confirm per intersection rule | protocol |
| `proto/dark_tower/signaling/v1/signaling.proto` | Not mine, Domain-judgment (GSA; security also confirms per GSA rule) — comment-only: retarget `MediaServerInfo.media_handler_url`'s pointer from ADR-0039 §7 to the rule block above `RegisterMHRequest` (the GC-2 item in TODO's story-4 signalling runtime-rules list) | protocol |
| `infra/grafana/dashboards/gc-overview.json` | Not mine, Minor-judgment — registrations panel | observability |
| `docs/observability/metrics/gc-service.md` | Not mine, Minor-judgment — catalog entry; `register_mh_txn` note | observability |
| `docs/runbooks/gc-deployment.md` | Not mine, Minor-judgment — 5 config rows; reject triage; rolling-update window | operations |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment (paired-infrastructure wrote) — deploy-failure row `mh-removed-still-selectable`; `mh-pool-failed` on a missing `replaced`; table indent fix | operations |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment (paired-infrastructure wrote) — §6.7 MH pool step notes the removed-MH settle | operations |
| `crates/env-tests/tests/03_gc_deployment_config.rs` | Not mine, Domain-judgment (paired) — allowlist keys as hard `configMapKeyRef`s (paired-test wrote) | test |
| `crates/env-tests/tests/44_gc_mh_registration.rs` | Not mine, Domain-judgment (paired) — new, feature `flows`: fresh-MH registration leg (paired-test wrote) | test |
| `crates/env-tests/src/fixtures/mh_pool.rs` | Not mine, Domain-judgment (paired) — `RunOwner`, `up`, `teardown` (paired-test wrote) | test |
| `crates/env-tests/src/fixtures/gc_fleet.rs` | Not mine, Domain-judgment (paired) — new: live GC pod helper hoisted from 03; key-list mirror + drift test (paired-test wrote) | test |
| `crates/env-tests/src/fixtures/auth_client.rs` | Not mine, Minor-judgment (paired) — `org_subdomain()` made pub: the per-run org subdomain is the mh.sh owner token (one rule, guarded by subdomain-regex-sync); paired-test wrote | test |
| `crates/env-tests/src/fixtures/mod.rs` | Not mine, Mechanical (paired) — module decl | test |
| `crates/env-tests/README.md` | Not mine, Minor-judgment (paired) — flow list | test |
| `docs/specialist-knowledge/test/INDEX.md` | Not mine, Minor-judgment (paired) — pointers | test |
| `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md` | Not mine, Minor-judgment (paired) — "GC registration (GC-2: R-25)" subsection | test |
| `docs/TODO.md` | Mine — S6 entry: allowlist sentence; new RegisterMC allowlist entry (Lead ruling) | — |
| `docs/specialist-knowledge/global-controller/INDEX.md` | Mine — pointers | — |

---

## Planning

### Problem in mechanism language
A registry keyed by a SELF-ASSERTED id (`handler_id`) carries secondary attributes (two endpoints) that other parties DIAL with credentials. Two properties must hold: (i) the attributes come only from an operator-configured address space (bounds where a credential is sent), and (ii) among LIVE rows the attribute is unique, while sequential reuse is allowed. Today GC enforces neither: scheme-prefix check only, and `ON CONFLICT DO UPDATE` silently rewrites endpoints.

**Wider class surfaced (sizing call to Lead, not silently dropped):** MC registration (`RegisterMC`) has the same shape — GC dials MC's `grpc_endpoint` carrying GC's `service.write.mc` bearer, MH dials MC's advertised endpoint — with only an http/https prefix check. ADR-0039 §7 scopes the rule to MH; an MC allowlist needs its own keys, render (per-instance helper ports), runbook rows and env-test: task-sized. Proposal: a TODO entry (owner security + global-controller) and a code comment at MC's validator stating the rule is deliberately MH-scoped. Lead to rule.

### Design
**Arm 0 — `mh_endpoint_policy.rs` (typed, parsed at startup).**
- Config (all REQUIRED; missing = `MissingEnvVar(key)`; malformed = `InvalidMhEndpointAllowlist("<KEY>: entry '<e>' ...")`, one format helper so every message starts with the key): comma-split, trim; empty value or empty element = failure (never "allow all").
  - `GC_MH_WT_ALLOWED_HOSTS`: exact entries (lowercase DNS name or canonical IPv4/IPv6 literal) or `.suffix` (DNS name with >=1 non-numeric label). Rejected at load: `*`, `.`, bare `.`, trailing dot, uppercase, IP-shaped suffix.
  - `GC_MH_WT_ALLOWED_PORTS`, `GC_MH_GRPC_ALLOWED_PORTS`: one port-set parser, 1..=65535.
  - `GC_MH_GRPC_ALLOWED_SCHEMES`: validated by grammar only (lowercase RFC 3986 scheme token, non-empty; no closed in-code set, so the S6 flip is config-only as ADR-0039 §7 states); `grpc` today (code + config comment: cleartext h2c — no TLS, no peer authentication of the MH server; MC's bearer crosses it in cleartext (S6); flips by config only to a TLS scheme when S6 mTLS lands, spelling decided then).
  - `GC_MH_GRPC_ALLOWED_CIDRS`: `ipnet::IpNet` (new workspace dep; already in the lock graph via reqwest/hyper-util). Host bits set = rejected (no silent widening); `/0` rejected.
- Endpoint grammar (not `url::Url`: it treats `grpc://` as opaque and normalises https hosts, which would make the stored string differ from the compared address). Hand-written closed grammar `scheme "://" host ":" port` (no trailing "/": `https://h:443/` is rejected, positive twin `https://h:443`, same pair for grpc): ASCII printable only, no whitespace/control, no userinfo/query/fragment/path. **Canonical form is required, not normalised**: lowercase host; IPv4/IPv6 text must equal `IpAddr::to_string()` of itself (rejects leading zeros, short forms, non-canonical v6); IPv6 bracketed, no zone id; IPv4-mapped IPv6 rejected; port explicit, decimal, no leading zeros, 1..=65535; scheme lowercase. The string is stored as received, so raw-string equality in arms 1/2 == address equality (database #6, security T2).
- WT zone: scheme `https` only (the `http://` acceptance goes); host = an EXACT entry (DNS name, or IP literal compared as parsed `IpAddr` — Kind's WT hosts are exact IP entries, 127.0.0.1 / the gateway IP, and must match), or a `.suffix` entry matching on a label boundary, never the apex; a `.suffix` entry NEVER matches an IP-literal host; port in set.
- gRPC zone: scheme in set; host MUST be an IP literal (`IpAddr::from_str`, hostnames rejected outright) inside a CIDR; port in set.
- Rejection carries `{field, component: scheme|host|port|syntax}`; Status = INVALID_ARGUMENT `"<field>: <component> not allowed"` — never echoes the allowlist. Log warn with handler_id, field, component, endpoint as `?` Debug.

**Arms 1–3 — `MediaHandlersRepository::register_mh(pool, &NewMhRegistration, staleness_bound) -> Result<MhRegistrationOutcome, GcError>`** (outer fn times the whole txn and records `register_mh_txn` ONCE: `success` for every completed decision including the reject arms — the query ran and the rule refused — `error` only for sqlx failures; inner fn holds the txn body):
1. `BEGIN`; first statement `SELECT pg_advisory_xact_lock(hashtext('gc_mh_registry'))` on `&mut *tx` (key in one const; begin/lock errors -> `GcError::Database(e)`, never an unlocked path).
2. `SELECT webtransport_endpoint, grpc_endpoint FROM media_handlers WHERE handler_id=$1 FOR UPDATE`; either endpoint differs -> explicit `tx.rollback()`, `EndpointChanged`, row untouched.
3. `SELECT handler_id, health_status, webtransport_endpoint=$2 AS wt_match, grpc_endpoint=$3 AS grpc_match, last_heartbeat_at < NOW() - $4 * INTERVAL '1 millisecond' AS stale FROM media_handlers WHERE handler_id<>$1 AND (webtransport_endpoint=$2 OR grpc_endpoint=$3) FOR UPDATE` — bound = GC-1's `MhRegistryConfig::staleness_bound()` via the existing `bind_ms()`; DB clock only (comment: NOW() is txn start, so lock wait makes rows look fresher -> errs toward reject, the safe direction).
4. Pure `decide(&[ConflictRow]) -> Decision` (exhaustive `HealthStatus` match, no `_` arm) (unit-tested; `health_status` parsed into a closed enum — an unknown DB value is `GcError::Internal`, never supersede): any row matching one-sided while fresh AND pending/healthy/degraded -> `AddressConflict` (whole request rejected, rollback, no writes — covers the new-row-overlaps-two-rows case); else every matched row superseded; `match = both` if any matched row matched both, else `one_sided`.
5. `DELETE FROM media_handlers WHERE handler_id = ANY($1)` (decided ids). Under the advisory lock + FOR UPDATE nothing that respects the locks can delete them first (retention DELETE blocks on the row locks), and row locks block EVERY writer (old-version replicas included), so a short count is unreachable while row locks hold — a bug in this function: `GcError::Internal` + rollback (database review correction; the real mixed-version gap is an old replica INSERTing an overlapping new row or rewriting its own row outside both locks — runbook).
6. Upsert: `INSERT ... ON CONFLICT (handler_id) DO UPDATE SET region, max_streams, health_status='pending', last_heartbeat_at=NOW(), updated_at=NOW() WHERE media_handlers.webtransport_endpoint=EXCLUDED.webtransport_endpoint AND media_handlers.grpc_endpoint=EXCLUDED.grpc_endpoint` — endpoints are never SET; `rows_affected != 1` -> `GcError::Internal` (defence against a step-2 regression). The silent rewrite is gone (obligation b).
7. `COMMIT`. Only DB statements inside the txn.
No migration, no unique index, no endpoint index.

**Types:** repo returns its own `MhRegistrationDecision { Registered, Superseded{match, ids}, AddressConflict{conflicting_ids}, EndpointChanged }`; the label vocabulary is a separate closed enum in metrics.rs; one mapping at the handler; neither enum has an unreachable variant.

**Handler mapping (single site in `mh_service.rs`):** `Registered` -> `ok`; `Superseded{match, ids}` -> `superseded{match}` + info log with new + superseded ids; both answer `RegisterMHResponse{accepted=true}`. `EndpointChanged` -> FAILED_PRECONDITION `"endpoint_changed"`; `AddressConflict` -> FAILED_PRECONDITION `"address_conflict"` (no other row's id/endpoints in the Status; logged warn with ids). DB errors -> `Status::internal`, NO registration outcome: covered by `gc_db_queries_total{operation="register_mh_txn",status="error"}` (stated in code + catalog). Pre-rule validation (handler_id/region/max_streams) stays uncounted, as today.

**Telemetry:** closed label enum `MhRegistrationOutcome { Ok, Superseded(MhSupersedeMatch), AddressConflict, EndpointChanged, InvalidEndpoint }` with `as_labels()` (metrics.rs only); one recorder; `match` exists only on Superseded by construction. One increment per registration call (not per deleted row). Zero-init exactly 6 series; present-at-zero asserts + negative (no superseded without match, no match on other outcomes). Catalog entry, gc-overview panel `sum by (outcome, match) (increase(...[$__rate_interval]))`, ADR-0032 integration test. No id labels (obligation e).

**Proto (comment-only, GSA):** arms 0–3 verbatim from gc-register-rule.txt, then a separate "Notes on the rule" paragraph with the additions (draft `/tmp/gc2/proto_block.txt`). Block replacing the one-line header above `RegisterMHRequest`: arms 0–3 carried from gc-register-rule.txt, config keys named, "Kind values derived, never retyped", stale = `GC_MH_STALENESS_MULTIPLIER` × `GC_MH_LOAD_REPORT_INTERVAL_MS` (no number), MH retries on address_conflict until the row goes stale, outcome vocabulary as GC telemetry labels (not wire fields), wire mapping (accepted=true for ok/superseded; rejects are gRPC Status, never accepted=false; status strings are diagnostic, callers must not branch on them), "not peer authentication; does not close S6", RegisterMH itself remains authenticated (service token, unchanged), handler_id stays MH-asserted, "MC never reacts to GC registry rows". Draft goes to protocol/auth-controller/security before landing. Field pointers on the two endpoint fields only.

**Config/render:** base `config.env`: `GC_MH_GRPC_ALLOWED_SCHEMES=grpc` (policy); the other four EMPTY (topology; empty = startup failure naming the key, so a converge that skipped the render crash-loops loudly instead of running permissive or silently rejecting all MHs — operations' (b) placeholder alternative would start GC and reject every MH, a quieter outage). `deploy.sh` (paired infra): `gc_mh_allowlist_literals()` prints the four rendered keys — WT hosts = distinct sorted hosts recovered from `mh_slot_table` URLs with a round-trip check through `advertise_url`; WT ports = sorted slot host ports; CIDRs = `kind_pod_subnet`; gRPC port = derived from the rendered mh-template's `MH_GRPC_ADVERTISE_ADDRESS` (`cached_render root`), failing loudly unless it matches `^[a-z][a-z0-9+]*://\$\(POD_IP\):([0-9]+)$` exactly once (no `grpc` or `50053` literal in deploy.sh). SCHEMES stays a base policy value (not derived): a TLS flip of MH alone makes registration fail closed, loudly, until GC's config flips too. Emitted as a `behavior: merge` generator over `gc-service-config` before the devloop early return (host-mode and devloop). setup.test.sh assertions with derived expectations.
Test values: one fixture in `gc-test-utils` (`test_mh_endpoint_allowlist_vars()`), used by the harness and tests.

**Tests** (folding paired-test A–F + database 7/8 + security list):
- Unit (`mh_endpoint_policy.rs`): grammar negatives each with a positive twin — userinfo, query/fragment/path, whitespace/control, implicit port, port 0/leading zero, uppercase host, non-canonical IPv4/IPv6, mapped v6, zone id, hostname in grpc, `.1` vs 10.0.0.1, label boundary (`evilexample.com`, apex), `http://` WT; config table per key (missing / malformed / empty / trailing comma / wildcard / host-bits CIDR / /0 / unknown scheme), error names the key. `decide()` table.
- `#[sqlx::test]` through `MhService::register_mh`: arm 0 per zone with cross-zone input (WT endpoint using a gRPC-zone address; gRPC endpoint using a WT-zone address), no row written; arm 1 WT-only and gRPC-only change on a healthy aged row -> row byte-identical (endpoints, status, heartbeat); identical re-register -> pending + heartbeat refreshed; 2a fresh healthy both-match -> superseded; 2b × {draining, unhealthy, stale-healthy} with one-sided match from WT side and from gRPC side; 2c × {pending, healthy, degraded}; fresh-vs-stale same-shape pair in ONE test; new row overlapping two rows (both stale -> both deleted; one fresh -> reject, no writes). ADR-0032: snapshot, `assert_delta(1)` on the expected cell, `assert_unobserved` on every other cell; `register_mh_txn` once per call.
- Concurrency (deterministic): test holds the session advisory lock built from the repo's pub lock-key const (never retyped), spawns two overlapping registrations, polls `pg_locks` until 2 ungranted advisory waiters (failure-only deadline), releases, joins. Scenarios (a) one-sided overlap -> {ok, address_conflict}; (b) identical endpoints -> {ok, superseded(both)}, one row. Invariant: `GROUP BY endpoint HAVING count>1` empty for both columns, row count asserted. current_thread (MetricAssertion per-thread).
- Existing tests: call sites updated for the new signature; `db_metrics_integration` register test renamed to `register_mh_txn`. **Correction at implementation:** one test DID rely on the silent rewrite (`mh_registry_tests.rs::test_mh_reregistration_updates_handler`, re-registering one id with new endpoints); converted, not deleted, to the endpoint_changed arm (row unchanged) plus the identical-endpoints positive arm.
- Env-test (paired-test spec, Gate 1): 03 gains the new keys as hard `configMapKeyRef`s. New `44_gc_mh_registration.rs` (feature `flows`; smoke 03 stays read-only) ALWAYS brings up one fresh MH via the `mh_pool` fixture (owner = run token) and cleans up on every exit path with `mh.sh drain --owner <run token> --all` (output-independent: covers success, `not-available-timeout` — where `up` deliberately leaves the Deployment — and a panic before any parse; `catch_unwind` → drain → `resume_unwind`; a non-OK drain STATUS (drain-timeout, stuck-terminating, mh-pool-busy, kubectl-failed, ...) fails the test even when the body passed, naming the REASON and printing the same `--all` reclaim command; both failures reported; `mh-no-free-slot` its own failure token; the failure JSON's slot is parsed for the diagnostic only). Non-vacuous: MH `set_ready()` only after `register()` returns Ok, so a serving `up` proves the CURRENT GC accepted a real slot WT address + pod-IP gRPC endpoint (a GC-only roll never re-registers MHs — the upgrade revision excludes gc-service-config — so old rows prove nothing). Assertions: all 6 series present per replica before `up`; replica set identical after (`gc-replica-set-changed`); cross-replica delta(ok + superseded) == 1; per replica invalid_endpoint and address_conflict present, 0, unmoved. No retyped numbers. Reject path: `up` fails with REASON `not-available-timeout` (exact token per mh-sh-contract.txt; `mh-rollout-failed` is upgrade's); BEFORE the drain, the failure message carries the cross-replica invalid_endpoint/address_conflict deltas (or "no GC reject observed"); mh.sh's own derived availability bound, no test-side timeout.

**Docs:** runbook rows (5), mixed-version window during GC roll (old replicas still rewrite without lock — rule guaranteed only once all replicas are new), existing rows not re-validated on allowlist narrowing, MH retries rejects forever with backoff and stays unselectable (triage: GC warn line + `mh.sh list`). test-gates GC registration subsection. TODO S6 line verbatim. INDEX pointers.

**Rollback:** converge from the previous checkout; old GC ignores extra keys; no schema change.

---

## Pre-Work

None

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test (paired) | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Infrastructure (paired) | confirmed |
| Protocol (paired) | confirmed |
| Auth Controller (paired) | confirmed |
| Database | confirmed |

---

## Implementation Summary

- **Arm 0** — `crates/gc-service/src/mh_endpoint_policy.rs` (new): canonical endpoint grammar (`scheme://host:port`, no userinfo/path/query/fragment/trailing `/`, lowercase, canonical IP text, no mapped v6/zone id, explicit port without leading zeros), per-zone `check()` returning `{field, component}`, and the four config parsers (hosts, port set, scheme set — grammar only per the Lead ruling, CIDR set via `ipnet` with host-bits and `/0` rejected). `config.rs`: `MH_ENDPOINT_ALLOWLIST_KEYS`, `mh_endpoint_allowlist_from_vars()` (one-line `MissingEnvVar` per key), `ConfigError::InvalidMhEndpointAllowlist` with one format helper (key first).
- **Arms 1–3** — `MediaHandlersRepository::register_mh(pool, &NewMhRegistration, staleness_bound) -> MhRegistrationDecision`: advisory xact lock first (`MH_REGISTRY_LOCK_KEY`), arm-1 select FOR UPDATE, conflict select FOR UPDATE with SQL `stale` on GC-1's bound, pure `decide()` (exhaustive status match; unknown DB status → Internal via strict `HealthStatus::try_from_db_str`), DELETE by decided ids (short count → Internal + rollback), upsert that never SETs endpoints (WHERE equality; rows≠1 → Internal). One `register_mh_txn` sample per call (`success` incl. rule rejects).
- **Handler** — `MhService::new(pool, &MhRegistryConfig, MhEndpointAllowlist)`; one recording + wire-mapping site; Status messages `<field>: <component> not allowed`, `endpoint_changed`, `address_conflict` (nothing echoed); warn logs on rejects (id, field/component, endpoints as Debug), info on supersede with both ids. Shared `grpc::validate_endpoint_bounds` used by MC too; MC validator comment states the rule is MH-scoped.
- **Telemetry** — `MhRegistrationOutcome` / `MhSupersedeMatch` label enums, `record_mh_registration`, 6-series zero-init (render test asserts the exact 6 and `match` only on superseded; retired `register_mh` label absent), catalog entry, gc-overview panel (`sum by(outcome, match)`, legend `{{outcome}} {{match}}`).
- **Config/render** — base `config.env`: 4 topology keys EMPTY, `GC_MH_GRPC_ALLOWED_SCHEMES=grpc` with the h2c/S6 comment; 5 hard `configMapKeyRef`s; `deploy.sh:gc_mh_allowlist_literals()` + merge generator, `setup.test.sh` assertions (paired-infrastructure; 1033 passed).
- **Proto** — the Gate-1-approved block (`/tmp/gc2/proto_block.txt`) landed verbatim above `RegisterMHRequest`; comment-only.
- **Tests** — `tests/mh_registration_tests.rs` (11: arm 0 per zone with cross-zone inputs, arm 1 per endpoint incl. arm-1-wins-over-arm-2, 2a, 2b ×4 incl. both sides, 2c ×3, near-bound fresh/stale pair, two-row overlap, pre-rule no-outcome, two deterministic concurrency tests — mutation-checked: with the lock statement removed both fail at "must queue on the registry lock"); `tests/mh_registration_metrics_integration.rs` (exact rendered label sets via a real Prometheus recorder); `decide()` table + fail-closed status unit tests; policy grammar/config tables; config key tables. Test allowlist has one home (`gc-test-utils/src/mh_endpoint_allowlist.rs`, `mh_config_vars()`), schemes read from config.env.
- **Env-tests** (paired-test) — 03 hard refs for the 5 keys; new `44_gc_mh_registration.rs` + `mh_pool` `up`/`teardown` + `gc_fleet.rs`; test-gates.md subsection.
- **Docs** — runbook rows + reject triage + rolling-update window; TODO S6 line + RegisterMC allowlist entry; INDEX pointers.

`buf lint`/`buf breaking`: layer-fast Layer 6 `STATUS=OK REASON=buf-breaking-passed`; `cargo build -p proto-gen` via Layer 4 build: OK.

---

## Issues Encountered

- **Gate 2, Layer 7 attempt 1: browser `client-mute-roster.spec.ts:119` failed, "1 of 2 media_servers connected; missing https://10.255.255.254:29024" (slot 4).** Root cause: env-test 44 (this task) ran last before the browser suite and removed its run-owned MH with `mh.sh drain --owner --all`. MH does not yet report DRAINING on SIGTERM (that is MH-3, story task 20), so GC kept the dead row `healthy` and selectable until it passed the staleness bound B = k × I (30 s deployed). A meeting created inside that window was assigned the dead handler, and nothing replaces a dead handler yet (MC-B/C). This is the ADR-designed GC lag, newly exposed because 44 is the first leg that removes an MH right before the browser suite. Fix (paired-test, in the `mh_pool` teardown): after a successful drain, wait until no GC replica counts the removed MH as healthy (per replica `gc_mh_available{state="healthy"}` <= live MH pods), with a failure-only ceiling of deployed k × I + one tick + the scrape ceiling (`removed-mh-still-selectable`). Once MH-3 lands, the wait shrinks to about one tick with no code change. Same class in deploy: `mh.sh upgrade` and the baseline-shrink drain had the same window, previously masked by env-test runtime. Lead ruling (a): paired-infrastructure added `wait_removed_mh_unselectable` at the end of `converge_mh_pool` (only when an MH was removed; per GC pod via the apiserver proxy; bound = the largest published `gc_mh_staleness_threshold_seconds` + 2 × the GC health-checker tick read from source; token `mh-removed-still-selectable`; setup.test.sh D8c, 1059/0). MH-3 acceptance line + both waits recorded in docs/TODO.md §Media Path Obligations. Also closed here: the P2-filed TODO item to retarget `signaling.proto` `MediaServerInfo.media_handler_url` to the RegisterMH rule block (comment-only; protocol + security confirmed).

## Code Review Results

| Reviewer | Finding | Disposition |
|---|---|---|
| database | Short-DELETE comment/runbook blamed a lock-bypassing writer; row locks block every writer, so it is a pure invariant (GC bug); real mixed-version gap = old replica INSERTs an overlapping row / rewrites its own | Fixed (code comment, runbook, plan text) |
| semantic-guard, code-reviewer F1 | `tx.rollback().await?` on the two Internal exits could replace the breach with a rollback error | Fixed: `rollback_keeping()` returns the breach, plus the rollback error if any |
| observability F1, code-reviewer F2 | Label shape spelled in recorder and zero-init | Tried a single `Vec<Label>` helper: `dt-guard metric-labels` rejects non-literal labels and `counter-zero-init` needs a literal `counter!` in the main-called entrypoint. **Resolved by the SSoT drift-guard alternative** (CLAUDE.md: "or add a guard that fails validation on drift"): two literal sites, required by the guards, with a code note naming the constraint. The exact-6-series render test and the exact-render integration test catch drift. Both reviewers accepted it as fixed; nothing remains in the tree |
| observability F2 | `register_mh_txn` status=error also covers invariant breaches | Fixed: wording in catalog (2 places), metrics.rs, media_handlers.rs, mh_service.rs; runbook pointer |
| observability F3 | Log fields per outcome | Fixed: `EndpointChanged { webtransport, grpc }` → `changed` field; bounds branch logs `field` + `component=syntax`; catalog lists fields per outcome |
| code-reviewer F3 | Key list not proven complete | Fixed: reverse test (map of only the listed keys parses) |
| code-reviewer F4 | `#[allow]` in new code | Fixed: `#[expect(clippy::unwrap_used, reason)]` where the lint fires (`mh_registration_tests.rs`); removed where it never fires (lib test module, metrics integration — clippy's in-test allowance) |
| paired-test F1–F3 | Bounds branch untested; no WT port reject; suffix twin unasserted | Fixed: `arm0_endpoint_bounds`, derived port case, `assert_outcome` on the suffix twin |
| dry-reviewer | `mh_pool.rs::list()` read MH_RESULT a second, looser way | Fixed by paired-test (`run_mh_sh` + `parse_list_value`) |
| operations | Remedy sentence said "fix config.env" for rendered keys; CrashLoop row missed empty allowlist | Fixed |
| paired-infrastructure F1–F3 | Same remedy issue; host-mode-only values unlabelled; short-DELETE cause | Fixed (values labelled host-mode/devloop + live-value command) |
| security S-1 | Kind CIDR = whole pod subnet; NetworkPolicy is the in-cluster bound | Fixed: runbook row, config.env comment, TODO S6 clause |

`./scripts/layer-fast.sh` green after the fixes (layers 1–6 OK).

**Iteration 2 (after Gate 2 Layer-7 attempt 1):** settle-wait deltas — observability F4 (recompute positive control, now +2 since the first read; absent/0 never settles) and F5 (frozen-gauge diagnostics); code-reviewer G1 (namespace derived, not hardcoded) and G2 (tick derived from source in both waits); dry-reviewer (one predicate: states `pending|healthy|degraded`, live = `mh.sh list` state `serving`, both pinned by `settle_predicate_matches_deploy_sh` with ANCHOR (DRY) comments); paired-infrastructure's own catch that `healthy`-only lets a dead row hide behind a `pending` replacement; paired-test T1–T3 (per-pod, live filter, recompute cases in setup.test.sh D8c); paired-infrastructure R1 (absent Rust baseline never settles); operations (environment-first failure lane `mh_settle_failed`, immediate-failure runbook clause). All fixed; `setup.test.sh` 1107/0, `layer7.test.sh` 687/0, layer-fast green.

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | S-1 Kind CIDR honesty; GSA co-owner confirmed proto block + signaling.proto hunk; iteration-2 delta clean |
| Test (paired) | RESOLVED-FIXED | 6 | 6 | 0 | iter-2 T1–T3 on setup.test.sh D8c; GC-side tests reviewed independently; env-tests co-authored (independent read by code-reviewer) |
| Observability | RESOLVED-FIXED | 5 | 5 | 0 | F1 resolved via the drift-guard alternative; iter-2 F4 recompute positive control, F5 diagnostics |
| Code Quality | RESOLVED-FIXED | 6 | 6 | 0 | iter-2 G1 namespace derived, G2 tick/ceiling docs; Also gave the independent read of deploy.sh, setup.test.sh and env-tests 03/44 |
| DRY | RESOLVED-FIXED | 4 | 4 | 0 | iter-2: bash↔Rust settle predicate pinned by settle_predicate_matches_deploy_sh; no new extraction opportunities |
| Operations | RESOLVED-FIXED | 5 | 5 | 0 | Independent read of the deploy.sh render; iter-2 env-first classification, runbook predicate, restart re-baseline |
| Semantic Guard | RESOLVED-FIXED (SAFE) | 1 | 1 | 0 | error-context-preservation on rollback |
| Infrastructure (paired) | RESOLVED-FIXED | 5 | 5 | 0 | iter-2 R1 absent-baseline; Runbook; deploy.sh and setup.test.sh co-authored |
| Protocol (paired, GSA owner) | CLEAR | 0 | 0 | 0 | Comment-only; arms 0–3 mechanically verified verbatim; signaling.proto pointer confirmed |
| Auth Controller (paired, GSA co-owner) | CLEAR | 0 | 0 | 0 | No auth-routing change; lock idiom correct |
| Database | RESOLVED-FIXED | 1 | 1 | 0 | Short-DELETE cause wording |

Lead rulings: (1) the MC-registration allowlist is task-sized (new keys, render, runbook, env-test) and outside ADR-0039 §7's MH scope, so it gets a `docs/TODO.md` entry plus a code comment. (2) `GC_MH_GRPC_ALLOWED_SCHEMES` gets grammar-only validation per ADR-0039 §7 "by config only" (security accepted). (3) The pre-existing deploy.sh removed-MH window is fixed now (option (a)), not deferred.


---

## Devloop Verification Steps

Gate 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`), final run: L1–L6 OK; L7 OK (Rust env-tests including the new 44, plus 30/30 browser E2E); `TOTAL_RESULT=OK`, 1705 s. Attempt 1 failed at L7 browser E2E (Issue 1, below), so L7 consumed 1 of 2 attempts.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `d4d6e5089bb06728b8083d7c02e0d59aa8083ddb`
2. `git diff d4d6e508..HEAD`
3. `git reset --soft d4d6e508` / `git reset --hard d4d6e508`
4. No schema change: rollback = converge from the previous checkout (old GC ignores the extra keys).
7. **Safe-revert unit**: **the whole commit.** Config keys, deployment.yaml refs and the deploy.sh render must move together: a GC binary without the render crash-loops, and a render without the binary is inert. The proto comment documents the enforcement. Reverting only the enforcement leaves the proto block describing a rule GC does not apply.

---

## Issues Encountered & Resolutions

### Issue 1: Gate 2 Layer 7 attempt 1 — browser E2E failure (client-mute-roster R-15)
**Problem**: Layers 1–6 OK and all Rust env-tests passed, including 44. Browser E2E failed 1 of 30: the client was assigned MH `https://10.255.255.254:29024`, which is slot 4, env-test 44's run-owned MH. That MH was drained or torn down around the start of the browser suite, so the WT handshake never completed. Log: `/tmp/gc2-all.log`.
**Resolution** (iteration 2; L7 attempt 1 consumed): root cause as in §Issues Encountered above. Both places that remove an MH now wait until it is unselectable, using one predicate:
- **Env-test teardown**: `RunOwner::teardown_and_settle` calls `gc_fleet::wait_removed_mh_unselectable`.
- **deploy.sh**: `converge_mh_pool` runs `wait_removed_mh_unselectable` + `mh_settle_failed` (Lead ruling (a): fix the pre-existing deploy window now, don't defer).

The predicate, checked per GC replica and never summed:
- `gc_mh_available{state=~"pending|healthy|degraded"}` <= the `serving` slots in `mh.sh list`;
- AND the `count_available_mh` success counter has risen at least +2 since the baseline, re-baselined on a counter reset.

The drift guard `settle_predicate_matches_deploy_sh` pins the two copies together. Failures are classified environment-first. The MH-3 acceptance line is in `docs/TODO.md`. Iteration 2 also closed the signaling.proto `media_handler_url` pointer TODO (comment-only GSA; protocol and security confirmed).
