# Devloop Output: OTel collector → Prometheus client-metrics pipeline, both R-27 blockers fixed

**Date**: 2026-09-23
**Task**: Build the pipe that carries browser SDK metrics into Prometheus, correctly labelled AND correctly summed (story `docs/user-stories/2026-09-21-hear-each-other.md`, task #3, R-27): collector image bump + `deltatocumulative` + Prometheus exporter with metrics-path filtering, GC telemetry-filter label allowlist widening, scrape job, network policy, working-state doc updates. Full task text: `/home/dev/.cache/devloop/story-runs/story-runner/2026-09-21-hear-each-other/task-3.prompt`.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/hear-each-other`
**Duration**: 2026-09-23 → 2026-09-24 (two sessions; the second resumed after an `auth-expired` interruption)

---

## Loop Metadata

| Field        | Value                                      |
| ------------ | ------------------------------------------ |
| Start Commit | `c3a0d459ba80d99bdf93b15bdf5b04bff7624268` |
| Branch       | `feature/hear-each-other`                  |
| Lead Model   | `claude-opus-5-5`                          |

---

## Loop State (Internal)

| Field                      | Value                                                                                             |
| -------------------------- | ------------------------------------------------------------------------------------------------- |
| Phase                      | `complete`                                                                                        |
| Implementer                | `implementer`                                                                                     |
| Implementing Specialist    | `infrastructure`                                                                                  |
| Tier                       | `full`                                                                                            |
| Iteration                  | `1` (attempt 2 of task 3; attempt 1 escalated in planning — evidence preserved in §Prior Attempt) |
| Security                   | `security` (paired; GC filter charge)                                                             |
| Test                       | `test`                                                                                            |
| Observability              | `observability` (paired)                                                                          |
| Code Quality               | `code-reviewer`                                                                                   |
| DRY                        | `dry-reviewer`                                                                                    |
| Operations                 | `operations` (paired)                                                                             |
| Semantic Guard             | `semantic-guard`                                                                                  |
| Global Controller (paired) | `paired-global-controller`                                                                        |
| Client (paired)            | `paired-client`                                                                                   |

---

## Task Overview

### Objective

Client `dt_client_*` series reach Prometheus correctly labelled and correctly summed so `MCMediaMissingKeyMaterial` has a working detector. Clears blocker 1 (collector 0.103.1 cannot sum same-identity delta datapoints) and blocker 2 (GC telemetry filter strips `reason`/`key_custody`/`outcome`/`action`/`source`).

### Scope

- **Service(s)**: otel-collector, Prometheus, gc-service (telemetry filter)
- **Schema**: No
- **Cross-cutting**: Yes — paired global-controller, security, observability, operations, client

### Debate Decision

NOT NEEDED — design recorded in the story plan and re-scoped at c3a0d45.

---

## Cross-Boundary Classification

| Path                                                                                                                                   | Classification            | Owner (if not mine)                                               |
| -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- | ----------------------------------------------------------------- |
| `infra/services/otel-collector/configmap.yaml`                                                                                         | Mine                      | —                                                                 |
| `infra/services/otel-collector/deployment.yaml`                                                                                        | Mine                      | —                                                                 |
| `infra/services/otel-collector/service.yaml`                                                                                           | Mine                      | —                                                                 |
| `infra/services/otel-collector/network-policy.yaml`                                                                                    | Mine                      | —                                                                 |
| `infra/kubernetes/observability/prometheus.yml`                                                                                        | Mine                      | —                                                                 |
| `infra/kind/scripts/setup.sh` (stale readiness-gate comment; Gate-2 fix: anchored `extract_manifest_images()`) | Mine | — |
| `scripts/layer7.sh` (`VITE_TELEMETRY_ENDPOINT` export in the browser-e2e block; R10)                                                   | Mine                      | —                                                                 |
| `scripts/setup.test.sh` (emi-* regression cases for the image extractor) | Mine | — |
| `scripts/lang/_test_helpers.sh` (file-backed `command_not_found_handle`; `report_results` folds it in) | Mine | — |
| `scripts/lang/_changed_helpers.test.sh` (missing-helper regression case) | Mine | — |
| `scripts/dev-web.sh` (`VITE_TELEMETRY_ENDPOINT`, same-origin via the dev proxy; R10)                                                   | Mine                      | — (client reviews the SDK-facing value)                           |
| `scripts/otel-collector/**` (new: acceptance harness, reads the committed configmap)                                                   | Mine                      | —                                                                 |
| `crates/dt-guard/src/client_metrics_export.rs` (new guard module: machinery)                                                           | Mine                      | —                                                                 |
| `crates/dt-guard/src/main.rs` (subcommand registration)                                                                                | Mine                      | —                                                                 |
| `crates/dt-guard/src/lib.rs` (module registration)                                                                                     | Mine                      | —                                                                 |
| `crates/dt-guard/src/alert_rules.rs` (guard POLICY CONTENT: a doc comment asserting in the present tense that `MCMediaMissingKeyMaterial` can never match — falsified by this change, rewritten as a past event) | Not mine, Domain-judgment | observability (owner-implements; machinery would be mine, but this is policy content per CLAUDE.md) |
| `scripts/guards/simple/client-metrics-export.sh` (wrapper)                                                                             | Mine                      | —                                                                 |
| `crates/gc-service/src/services/telemetry_filter.rs`                                                                                   | Not mine, Domain-judgment | global-controller (+security), `--paired-with`                    |
| `crates/gc-service/src/handlers/telemetry.rs` (R12: thread `UserClaims.org_id` into the filter; pipeline doc comment)                  | Not mine, Domain-judgment | global-controller (+security), `--paired-with`; GC owns the shape |
| `crates/gc-service/tests/telemetry_proxy_tests.rs` (one assertion: forwarded `org_id` value == JWT claim; R12)                         | Not mine, Domain-judgment | global-controller, `--paired-with` (pre-authorised single edit)   |
| `packages/sdk-core/src/telemetry/telemetryConfig.ts` (R1: DELTA temporality)                                                           | Not mine, Domain-judgment | client, `--paired-with`                                           |
| `packages/sdk-core/src/telemetry/__tests__/telemetryConfig.delta.test.ts` (new: DELTA selection unit test) | Not mine, Domain-judgment | client, `--paired-with` |
| `packages/sdk-core/src/telemetry/__tests__/telemetryConfig.test.ts` (existing ctor-arg test updated) | Not mine, Domain-judgment | client, `--paired-with` |
| `packages/web-app/src/lib/session.ts` (R15: token-provider parameter) | Not mine, Domain-judgment | client, `--paired-with` |
| `packages/web-app/src/main.ts` (R15: telemetry setup moved out) | Not mine, Domain-judgment | client, `--paired-with` |
| `packages/web-app/src/App.svelte` (R15: provider reads the session token) | Not mine, Domain-judgment | client, `--paired-with` |
| `packages/web-app/src/lib/config.ts` (R15: same-origin assertion) | Not mine, Domain-judgment | client (+security), `--paired-with` |
| `packages/web-app/src/__tests__/telemetryOrigin.test.ts` (new) | Not mine, Domain-judgment | client, `--paired-with` |
| `packages/web-app/src/__tests__/joinMeeting.test.ts` (stub signature) | Not mine, Mechanical | client |
| `packages/web-app/e2e/fixtures.ts` (scope-lock comment on `assertTokenOnlyJoinTraffic`) | Not mine, Minor-judgment | client (+test, security) |
| `packages/web-app/e2e/README.md` (§Artifacts: telemetry header is now a token carrier) | Not mine, Minor-judgment | client (+security) |
| `crates/gc-service/src/observability/metrics.rs` (telemetry endpoint attribution + test) | Not mine, Domain-judgment | global-controller (+security), `--paired-with` |
| `docs/observability/metrics/gc-service.md` (endpoint cardinality line) | Not mine, Minor-judgment | observability |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` (one doc sentence: `orgId` is advisory, GC stamps the authenticated org UUID; R12) | Not mine, Domain-judgment | client, `--paired-with`                                           |
| `crates/mh-service/src/observability/metrics.rs` (one reserved-word comment line at `MediaDropReason`; no code change)                 | Not mine, Minor-judgment  | media-handler                                                     |
| `crates/env-tests/tests/32_media_metric_hygiene.rs` (R4) | Not mine, Domain-judgment | observability + test |
| `crates/env-tests/src/fixtures/metric_hygiene.rs` (R4: selector + rationale) | Not mine, Domain-judgment | observability + test |
| `docs/user-stories/2026-09-21-hear-each-other.md` (task-13 row/prompt: delta bullet satisfied here)                                    | Not mine, Minor-judgment  | client (story owner: lead)                                        |
| `docs/observability/metrics/client.md` (scope block, Job Label, `Exported:` markers, gauge caveat)                                     | Not mine, Domain-judgment | observability (owner-implements)                                  |
| `docs/observability/alerts.md` (premise block; telemetry-pipeline rules) | Not mine, Domain-judgment | observability (owner-implements) |
| `docs/observability/dashboards.md` (premise block) | Not mine, Domain-judgment | observability (owner-implements) |
| `docs/observability/alert-conventions.md` (§Metric expiration, §Threshold provenance) | Not mine, Domain-judgment | observability (owner-implements) |
| `infra/grafana/dashboards/client-media.json` (banner, description, 8 per-panel strings, queue-depth caveat)                            | Not mine, Domain-judgment | observability (owner-implements; operations reviews)              |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (MCMediaMissingKeyMaterial premise block only; expr unchanged)                          | Not mine, Domain-judgment | operations (owner-implements)                                     |
| `infra/docker/prometheus/rules/otel-alerts.yaml` (dormancy reason, not the dormancy)                                                   | Not mine, Minor-judgment  | operations (owner-implements)                                     |
| `docs/runbooks/gc-deployment.md` (blast radius, rollback, upgrade discipline) | Not mine, Domain-judgment | operations (owner-implements) |
| `docs/runbooks/mc-incident-response.md` (Scenario 16) | Not mine, Domain-judgment | operations (owner-implements) |
| `docs/runbooks/client-dev-local.md` (§4.5) | Not mine, Domain-judgment | operations (owner-implements) |
| `docs/runbooks/devloop-validation.md` (§8 rows: `TRIAGE_VIOLATION` premise corrected for the widened job selector; `AlreadyExists` row added after Gate 2 attempt 1) | Not mine, Minor-judgment | operations + test |
| `docs/TODO.md` (R15 token-scope entry; §Observability Debt closure pending legs 0-3) | Not mine, Minor-judgment | observability + operations + security |
| `docs/specialist-knowledge/*/INDEX.md` (pointer updates only if a cited premise moved)                                                 | Not mine, Mechanical      | respective owner                                                  |

---

## Planning

### Mechanism restatement (and the wider class it exposed)

The invariant is: **every curated client media series reaches Prometheus as ONE correctly-summed, correctly-labelled, identity-free series per `{client_version, org_id, key_custody[, discriminator]}`, however many browsers share it.** Stated that way it has six links, not the two the task named. Each is a place where the value silently becomes wrong or absent:

| #   | Link                                         | State at c3a0d45                                                                                                                                                                                                                                                                                    | Fix here                                                                                    |
| --- | -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| 0   | SDK emits DELTA                              | **cumulative** (no `temporalityPreference`), so the collector fix would be a no-op on real input                                                                                                                                                                                                    | R1: DELTA in `telemetryConfig.ts` + unit test                                               |
| 0b  | Browser emits at all                         | `VITE_TELEMETRY_ENDPOINT` set nowhere, so no browser exports                                                                                                                                                                                                                                        | `scripts/layer7.sh` browser block                                                           |
| 1   | GC passes the discriminator keys             | strips `reason`/`outcome`/`action`/`source`/`key_custody`                                                                                                                                                                                                                                           | R6 datapoint-only tier                                                                      |
| 2   | Collector sums N same-identity delta writers | 0.103.1 replaces/drops (attempt 1); **0.161.0 + `delta_to_cumulative` ALONE still drops**: the first point of a writer whose start predates the stream's start (ErrOlderStart), and any point that is not newer than the head (ErrOutOfOrder), so browser clock skew and late exports silently drop | bump + `delta_to_cumulative` **+ collector-clock restamp of delta points** (evidence below) |
| 3   | Names/labels survive export                  | exporter suffixing                                                                                                                                                                                                                                                                                  | `add_metric_suffixes: false`                                                                |
| 4   | Prometheus scrapes it                        | no job, no netpol, no port                                                                                                                                                                                                                                                                          | job + port + ingress                                                                        |

The wider class the restatement surfaced: **every timestamp and label value a browser sends is untrusted input**, not just `client_version`. So the value-shape controls cover all four enum keys plus `client_version`, `key_custody` is pinned at both GC (drop on mismatch) and the collector (set), and timestamps are re-stamped. `org_id` is closed by GC server-stamping it from the authenticated `UserClaims.org_id` (R12, superseding the R5 deferral).

### Collector image: `otel/opentelemetry-collector-contrib:0.161.0@sha256:fd328de2552466ad78385e1b1289c3f2402b1c45f265b252aab1955b42845ac1`

- Latest stable (released 2026-09-16; 0.162.0 is nightly-only). Pinned tag **and** digest (security F).
- **Component proof from the pulled digest, not release notes:** `otelcol-contrib components` run in-cluster from that digest lists `delta_to_cumulative` (the `deltatocumulative` alias still loads but logs a deprecation, so the config uses the new name), `filter`, `transform`, `attributes`, `memory_limiter`, receiver `otlp`, exporters `debug` + `prometheus`, extension `health_check`. The full output was captured to `/tmp/devloop/otel-harness/components-0.161.0.txt`. The runbook pre-upgrade checklist gets the command plus the expected names (ops 3).
- `health_check` on `/` :13133 still returns ready (the harness pod's readiness probe is exactly that probe), so the probes, `setup.sh`'s `kubectl wait` and `00_cluster_health.rs` are unaffected (ops 4). The full-stack re-verification happens at implementation.

### Planned collector config (metrics pipeline only; traces stay `[otlp] -> [debug]`)

`receivers: [otlp]` -> `processors: [memory_limiter, filter/client_metric_names, filter/client_metric_temporality, transform/client_metric_labels, transform/client_metric_clock, delta_to_cumulative]` -> `exporters: [debug, prometheus]`. `memory_limiter` also goes first on traces.

- **`filter/client_metric_names`** (control a): strict include of exactly the 14 names below. The comment records that it is an anti-forgery control (a client-origin series can never share a name with a server-authored one; GC never inspects `metric.name`), carries `ANCHOR (DRY):` to `client.md` plus the guard, and records observability's reasoning for excluding the 5 join-flow metrics. It is not a prefix rule.
- **`filter/client_metric_temporality`**: drops client Sum/Histogram points that are not DELTA. This makes ops' "cumulative SDK + new tag" cell and the mixed-fleet window _absent_ rather than _wrong_. The harness case SDK-S5 proves it: a cumulative writer sharing a delta writer's identity is excluded, and the delta total is intact. The gauge is unaffected (no temporality).
- **`transform/client_metric_labels`**:
  - (control b) `keep_keys` with exactly 7 keys in two commented groups: `{client_version, org_id, key_custody}` is the §11 contract set (observability); `{reason, outcome, action, source}` is the GC-widened discriminators (GC + security). `meeting_id_hash` is out by omission. The comment names `MediaTransport.ts` ~417-419 and the three-list non-collapse (GC `ALLOWLIST` vs datapoint tier vs `keep_keys`, both failure directions, DRY D-4), and states that `le` is not a key.
  - `set(key_custody, "operator")` (security B2).
  - (control c) `client_version` not matching `^[0-9A-Za-z.+-]{1,32}$`, or not a string, is rewritten to `invalid`.
  - `reason`/`outcome`/`action`/`source` not matching the **anchored** `^[a-z_]{1,64}$` are rewritten to `invalid`. The cap has its own justification at the config site: the longest real emitted token is `payload_length_exceeds_available` (32), and the cap bounds bytes per series, not count. It deliberately cites no other construct; `nameGuard.ts:18` bounds metric names, not label values (DRY). Width is 64, per main's R11 re-rule. Every statement is **nil-guarded** (`attributes[k] != nil and ...`) so that no label is conjured onto the metrics that never carry it (paired-client traps 1/2).
  - The comment frames the regex as a charset/PII-shape control that excludes emails, UUIDs, hex and base64, and notes that the length cap bounds bytes per series, not series count.
  - It states per key what bounds each value, and never rounds up:
    - four keys charset-checked (bytes and charset, NOT count);
    - `client_version` shape-checked;
    - `key_custody` pinned;
    - `org_id` server-stamped by GC (R12).
  - The sentinel `invalid` collides with no live `RejectReason`, send-drop, outcome, action or source token; the collision check is part of the harness.
  - It carries security's principle: a value allowlist and a shape check fail in opposite directions, so prefer the loud one. This is why the check is not a token list; a derived list would have sentinelled the alert's own `RejectReason` tokens.
  - No `org_id` regex: the value is server-stamped by GC (R12).
  - All values are checked against real emissions: `0.0.0` / `0.0.0-test`, the `RejectReason` union (longest `payload_length_exceeds_available`, 32), the send-drop, mute, KEK-source and wrap-outcome tokens, and task 13's `kek_generation_stale`.
  - Added harness value assertions:
    - a long injected lowercase-run value becomes `invalid`;
    - a metric sent without `reason` leaves with no `reason`;
    - `reason="no_kek_for_generation"` passes unchanged;
    - `client_version="0.0.0"` survives.
- **`transform/client_metric_clock`** (delta points only): `set(time, Now())`, `set(start_time, time)`. The comment also states what it **erases**: a per-SDK reset signal (a `start_time` change). Erasing it is correct for a fleet sum over joining and leaving browsers, but it is irreversible at this layer, so any future need for client-reset visibility means revisiting the restamp. The collector clock is the ordering authority at the trust boundary. This is **not** attempt 1's rejected zeroing workaround: that one depended on the prometheus exporter's `start == prev.end` internals and hid genuine resets. This one gives `delta_to_cumulative` monotonic, collector-owned timestamps, so its documented ordering contract holds. It hides no reset, because a delta has no reset semantics: each point is an increment. It closes both honest skew and security's far-future denial-of-detection (R2 case).
- **`delta_to_cumulative`**: `max_stale: 15m`, `max_streams: 500`.
  - `max_stale` must be at least `metric_expiration` + the longest `dt_client_*` range window. Harness S2 shows why: a reset that becomes visible while pre-reset samples are still inside a `rate()` window under-counts whenever the new value exceeds the old.
  - `max_streams` is a fail-closed guard, not a capacity estimate (observability's corrected wording). Streams are bounded by metrics × observed label values × live `org_id`s, NOT by browser count. On the dev cluster `org_id` is a per-run dimension: `layer7.sh` provisions a fresh org per run and R12 stamps its UUID, so streams from every run still inside `max_stale` coexist. 500 tolerates several overlapping runs. The comment will not claim a specific overlap count until operations confirms the suite cadence.
  - The derivation lives in `alert-conventions.md` (observability's home); the config comment points there and does not restate `5m` (DRY D-6).
- **`prometheus` exporter**: `endpoint: 0.0.0.0:8889`, `add_metric_suffixes: false`, `metric_expiration: 10m` (pointer comment), and `resource_to_telemetry_conversion` left off, with a comment saying the contract labels are datapoint attributes (`mediaMetrics.ts` ~206-218) and the SDK resource set (`telemetryConfig.ts` ~116-121) is not a curated label set. The comment also notes that the exporter adds `otel_scope_name`/`otel_scope_version` downstream of `keep_keys`.
- **`memory_limiter`** first (ops 5b): `check_interval: 1s`, `limit_percentage: 80`, `spike_limit_percentage: 20` (cgroup-relative). Measured RSS on 0.161.0 under the harness is **~201-210 MiB** (`otelcol_process_memory_rss`, no limit set), against the current 256Mi limit. Limit raised to **512Mi** (request 256Mi) with the measured number in the comment. RSS gets re-measured on Kind.

### Collector self-telemetry and QoS (ops A/B/C)

- **Drops become countable (ops A).** The metrics path drops at three places: the name filter, `keep_keys`, and the temporality filter. A stale cumulative tab fleet would otherwise render identically to "no browser".
  - Add `service.telemetry.metrics` served on `0.0.0.0:8888`, with container and Service port `otel-metrics`.
  - Add a netpol ingress rule from Prometheus with the same single-element shape as the :8889 rule.
  - Add a separate scrape job `otel-collector-telemetry`, so `up{job="otel-collector"}` keeps meaning "the client-series endpoint".
  - The drop signal uses metric names **read from the 0.161.0 image** during planning: `otelcol_processor_incoming_items` minus `otelcol_processor_outgoing_items`, by `processor`, and the `otelcol_deltatocumulative_*` family. No alert this loop.
- **Digest pin (ops B).** Pin `0.161.0@sha256:fd328de…` in `deployment.yaml` **only if** `setup.sh`'s preload path (`grep -oP 'image:…'` then `load_image_to_kind`) is proven end to end with the `tag@sha256` form on Kind at implementation. Otherwise stay on the tag and record in the runbook that the one-way floor rests on tag immutability. Either way the outcome is recorded here; an untested digest form never ships.
- **Resources (ops C).** `requests == limits == 512Mi` gives Guaranteed QoS. The collector is a singleton whose loss is a fleet-wide init outage (AC/GC/MC), so it should be evicted last. The comment gives the measured ~205 MiB unconstrained steady state as a floor, not a peak. It also records why `memory_limiter` must stay: `init_otel` is a connection probe (`common/src/observability/otel.rs` ~223-233), so refusing data never fails service init, and the limiter turns a would-be OOMKill into bounded telemetry loss.
- **Restamp scope (observability).** Both `time_unix_nano` and `start_time_unix_nano` are set to collector-arrival. The configmap comment gives only the what and the why (misaligned-start class). The reading guidance lives in `client.md`, owned by observability.

### Two-writer / idle / adversarial acceptance: DONE in planning (scratch namespace `otel-exp`, deleted)

The harness is a reproducible script (`run.sh` + `driver.py` + `sdk-writers.mjs`, currently `/tmp/devloop/otel-harness/`). It will be committed under `scripts/otel-collector/` and will read the **committed** configmap's `config.yaml`, so it tests what ships and the next collector bump reruns it (the runbook pre-upgrade checklist cites it). It runs in a scratch namespace and tears down on exit.

| Case                                                                                                                                                                                                                               | 0.103.1 (control, no d2c)                                                                         | 0.161.0 + d2c, no restamp                                                  | 0.161.0 planned config |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ---------------------- |
| S1 two same-identity delta writers, out of phase: counter / hist count / sum (want 36 / 6 / 2700)                                                                                                                                  | **7 / 3 / 600** (5,7,5,7 oscillation; "Misaligned starting timestamps", histogram points dropped) | **29 / 5 / 2000** (B's first point dropped: start older than the stream's) | **36 / 6 / 2700** PASS |
| S2 single writer: delta 5, 20 s idle, delta 6 (want 11)                                                                                                                                                                            | **6**                                                                                             | 11                                                                         | **11** PASS            |
| S4 security's adversarial case: 3 writers, honest + -30 s + +30 s skew + late/retried + far-future (year 2100), then honest (want 28)                                                                                              | n/a                                                                                               | not run (S1 already red)                                                   | **28** PASS            |
| S3 controls: unlisted name absent; `meeting_id_hash` stripped; 200-char and mis-shaped `client_version` -> `invalid`; forged `key_custody=end_to_end` -> `operator`; UUID `reason` -> `invalid`; no unit suffix, no `_total_total` | PASS                                                                                              | PASS                                                                       | PASS                   |
| **Real SDK** (`@opentelemetry/sdk-metrics` 2.10.0 + `exporter-metrics-otlp-proto` 0.221.0, sdk-core's pins, DELTA, proto): SDK-S1 two MeterProviders, B started later (36/6/2700)                                                  |                                                                                                   |                                                                            | PASS                   |
| Real SDK: SDK-S2 idle (5, four empty periodic flushes, 6 -> 11)                                                                                                                                                                    |                                                                                                   |                                                                            | PASS                   |
| Real SDK: SDK-S5 stale CUMULATIVE writer + DELTA writer, same identity (want the delta total 15)                                                                                                                                   |                                                                                                   |                                                                            | PASS                   |

Collector self-telemetry during the run: `otelcol_deltatocumulative_datapoints` equals every delta point sent, and no drop counters. `filter/client_metric_temporality` incoming 21 / outgoing 17, i.e. exactly the cumulative writer's 4 points. Logs are in `/tmp/devloop/otel-harness/*.log` and will be copied into §Devloop Verification Steps.

Reset semantics (test condition 1): the element that takes 29/5 to 36/6 is `transform/client_metric_clock`, the collector-clock restamp of DELTA points ahead of `delta_to_cumulative`. It is keyed on no identity label.

A delta stream carries no reset signal, so a restarting browser's deltas are _correctly_ summed into the fleet total. The only genuine reset of the stored cumulative is `delta_to_cumulative` forgetting the stream after `max_stale`. Two harness cases are added at implementation to show the restamp does not mask that reset:

- **S6 writer restart:** a MeterProvider is shut down and a fresh one continues. The total keeps accumulating correctly.
- **S7 stale reset:** a harness-only config override with `max_stale: 20s`, then an idle period longer than that. The stored cumulative restarts at the new delta (a reset Prometheus can see) and is NOT summed through. The idle→11 case stays below `max_stale`.

**S6/S7 observed in planning (final planned config, 0.161.0 digest):**

- S6 writer restart: 5, then a fresh MeterProvider adds 7, reads **12**. PASS.
- S7 with a `max_stale: 20s` override:
  - A 40 s idle still summed through (11). `delta_to_cumulative` forgets streams on a **periodic sweep**, so effective forgetting happens _after_ `max_stale`, by up to the sweep period.
  - A 130 s idle resets to **6**, a visible reset. PASS.
  - Forgetting late only lengthens accumulator memory, which is the safe side of `max_stale ≥ metric_expiration + window`. The config comment says `max_stale` is a lower bound on forgetting, not an exact time. The committed S7 case uses an idle comfortably past `max_stale` plus the sweep.

Also re-run on the final planned config (memory_limiter, `{1,64}`, no org_id regex): all of S1-S4 and the SDK cases pass. New value assertions pass too:

- `"X truncated X"` becomes `invalid` (anchor);
- a label never sent stays absent (nil-guard);
- `client_version=0.0.0`, `reason=no_kek_for_generation` and `reason=kek_generation_stale` survive.

RSS is 209-212 MiB.

Remaining for R2 at implementation: re-run the real-SDK cases **through GC** (a real user token against GC's `/api/v1/telemetry` on Kind), so the GC filter is in the path. This doubles as leg 2 observed on the wire.

### GC filter (R6; paired-global-controller's confirmed spec; security + DRY amendments)

- `ALLOWLIST: [&str; 12]` is unchanged. New `MEDIA_DATAPOINT_EXTRA: [&str; 5] = ["reason", "outcome", "action", "source", KEY_CUSTODY_LABEL]`. It is a delta only, so the union is computed and never written down (DRY D-1). `KEY_CUSTODY_LABEL` comes from `common::observability::labels`.
- A private `filter_attrs_with(attrs, kind, allow_extra, counts)` does `(ALLOWLIST ∪ allow_extra).contains(key) && value_is_scalar && custody_value_ok`.
- The public surface is two wrappers with a hard-coded tier: `filter_datapoint_attrs` (the five `dp.attributes` sites) and `filter_base_attrs` (the four exemplar sites, resource, scope, and all trace levels). No call site chooses a tier (DRY D-3b). The tier is not selected from `AttrKind`, because exemplars pass `AttrKind::Datapoint`.
- `custody_value_ok`: a `key_custody` value that is not `KEY_CUSTODY_OPERATOR` is **dropped**, never overwritten, and counted via the existing `DropCounts`. No new metric or label.
- No `metric.name` gate and no Rust mirror of the TS vocabularies.
- R12 `org_id` stamping follows paired-global-controller's shape:
  - `filter_metrics`/`filter_traces` take `authenticated_org_id: &str` from validated `UserClaims`.
  - A private `stamp_org_id` runs after the retain. It set-or-inserts on the five datapoint sites (folded into `filter_datapoint_attrs`) and overwrites only where already present at every other level.
  - Stamping is not counted as a drop.
  - `org_id` stays in the base 12.
  - Tests 7-10: spoofed value replaced, a single key, inserted when absent, overwrite-not-insert on spans, non-scalar dropped then stamped.
  - `telemetry_proxy_tests.rs` ~734 gains a forwarded-value assertion against the JWT claim.
- GC amendments A-C (security review of the stamping shape):
  - **A.** An empty `authenticated_org_id` never stamps. Any present `org_id` is dropped and counted, nothing is inserted, and a single `tracing::warn!` fires per request with no attribute values. The request does not fail. Test 12 asserts `org_id` is ABSENT.
  - **B.** Claims are read per request from `Extension<UserClaims>` and never cached.
  - **C.** The header states the principle: _GC may assert a value it has AUTHENTICATED; it may not synthesize a value it merely ASSUMES._
- Module header wording (DRY + security): _keys at every nesting level; value SHAPE for every key; value CONTENT for exactly two keys — `key_custody` (one legal value, dropped on mismatch, SSoT `common::observability::labels::KEY_CUSTODY_OPERATOR`) and `org_id` (server-stamped from the authenticated claim; the payload value is never read or trusted); neither the value domain nor the distinct-value COUNT of any other key._
- `handlers/telemetry.rs:14` pipeline comment updated.
- Tests:
  1. Five keys with distinct values survive byte-identical as `(key, value)` pairs next to a dropped `participant_id`; `counts.datapoint == 1`.
  2. The same five at resource and span level are dropped (5 each).
  3. The five on exemplar `filtered_attributes` are dropped.
  4. `key_custody="end_to_end"` is dropped and counted; `operator` survives.
  5. `ArrayValue` under `reason` is dropped.
  6. `allowlist_has_exactly_12_keys` is kept, and `media_datapoint_extra_has_exactly_5_keys` is added.
- Existing `telemetry_proxy_tests.rs` / env-test 31 stay unmodified. If either needs editing, I stop and tell GC first.

### SDK DELTA (R1; owner client)

- Extract a small `createMetricExporter(endpoint)` factory from `configureTelemetry`, used by both it and the test. It sets `temporalityPreference: AggregationTemporality.DELTA`, taken from the already-declared `@opentelemetry/sdk-metrics`; no new dependency. The proto package does not re-export `AggregationTemporalityPreference`.
- The comment records three things:
  - delta is deliberate, so aggregation happens in the collector;
  - `service.instance.id` is barred (§11);
  - the **enum coincidence**: the exporter compares against `AggregationTemporalityPreference.DELTA`, and both enums use `0`.
- The unit test builds the **real** exporter and asserts behaviour, not the argument: `selectAggregationTemporality(InstrumentType.COUNTER)` and `(HISTOGRAM)` both return `AggregationTemporality.DELTA`. That is what catches a renumbering.
- The story file's task-13 delta bullet is marked as satisfied by task 3.

### Prometheus side

- `prometheus.yml` job `otel-collector`: pod SD in `dark-tower`, `keep` on `app=otel-collector` plus container port `8889`, `scrape_interval: 10s`. The comment says `ANCHOR (DRY):` `DEFAULT_METRIC_EXPORT_INTERVAL_MS` in `telemetryConfig.ts`: scraping faster double-samples (DRY D-5).
- `honor_labels` stays at the default (false). The comment states it as a **security control**: a client-supplied `job`/`instance` can never impersonate a service job (security E). It also records the stored shape: `job="otel-collector"`, `instance` = collector pod, `exported_job="darktower-sdk-core"`, and **no** `exported_instance`, because GC strips everything but `service.name`/`service.version` and there is no `service.instance.id` (paired-client 4/6 corrects the prompt wording).
- The job is not keyed on the `services.rs` enumeration.
- Pull, not remote-write, is recorded in the comment with the `--web.enable-lifecycle` argument.
- The Deployment gets container port `prom-metrics` 8889 and the Service gets port 8889.
- `network-policy.yaml` gets a new ingress rule for :8889 with `namespaceSelector` + `podSelector` in **one** `from` element (AND), byte-consistent with the mh form. The egress rule is untouched; only its justification comment changes: DNS-only still holds because the prometheus exporter is pull-served and there is no network sink.
- Keep the default RollingUpdate; no `strategy:` block (ops 6).

### Guard (R7; machinery mine, membership content observability's)

New subcommand `dt-guard client-metrics-export`. It reads its sets from their SSoTs and hard-codes no names. Each extractor has a non-empty positive control, so a parse that silently yields ∅ fails red.

- **G1**: `{client.md headings with Exported: yes}` == `{collector include list}`, compared both directions and naming the side that diverged.
- **G2**: every `dt_client_*` literal in `mediaMetrics.ts` has a `client.md` heading carrying an explicit `Exported:` marker (yes or no). A new media metric, e.g. task 13's `kek_retention_violations_total`, is red until someone makes an export decision. This covers DRY's TS-vs-catalog direction without chaining it to `Exported` (DRY correction).
- **G3**: every `Exported: yes` name has an emitter literal somewhere in `packages/sdk-core/src` outside tests, so no permanently-empty exported series.
- **G4**: `keep_keys` ⊆ GC `ALLOWLIST` ∪ `MEDIA_DATAPOINT_EXTRA`, parsed from the Rust source with `KEY_CUSTODY_LABEL` resolved via `common`. This catches a kept key that GC always strips (DRY knock-on).
- **G5**: the collector `metric_expiration` is strictly greater than the longest range window over a `dt_client_*` selector in any loaded rule file, and `max_stale` ≥ `metric_expiration` + that window. Both keys must be present, so a renamed key goes red (DRY/obs disagreement resolved toward the guard).

The guard uses `metric_catalog.rs`'s parser and `alert_rules.rs`'s loaded-file predicate. Wrapper `scripts/guards/simple/client-metrics-export.sh`. Hermetic unit tests cover each rule's red and green paths.

**G6 (code-reviewer + security + DRY):** the sentinel `invalid` must collide with no token in either the TS vocabularies (`RejectReason`, `MEDIA_SEND_DROP_REASONS`, `MEDIA_MUTE_ACTIONS`, `MEDIA_KEK_SOURCES`, `ReportableWrapOutcome` / `receivePath.ts`) or MH's `MediaDropReason` (the Rust arms). The G6 comment states that it reads both.

- The MH leg's structural positive control is the arm count checked against `MediaDropReason::ALL`'s declared length.
- The TS leg's positive control is that `no_roster_entry` must be present.

Uniform anti-vacuity rules for G1-G6:

- An empty or short parse fails with a distinct `empty_input` reason token, never the content reason.
- Unit tests cover both red paths separately.
- The Rust arm/array parser is a shared helper that **parses only**. Each caller (G4, G6) keeps its own expected-count assertion and its own empty-parse reason. The contract is stated in a comment at the helper.
- Reserved-word notes ("`invalid` is the collector's sentinel; never a real token") go at `MEDIA_SEND_DROP_REASONS` (TS, `mediaMetrics.ts`, which is already a client row) and at `MediaDropReason` (Rust, `crates/mh-service/src/observability/metrics.rs`; Minor-judgment, media-handler).

### env-test 32 (R4; test's binding shape)

- Widen the job selector to include `otel-collector`.
- Positive control: `up{job="otel-collector"} == 1`. It is browser-independent and present on an idle cluster.
- Client-series presence is not an anchor. The absence checks (no meeting identifier, overclaim) evaluate over whatever client series exist.
- A site comment states that the idle-cluster leg is vacuous and that the real `meeting_id_hash`-absence proof is client task 15's browser read-back.
- The failure text and the `CLIENT_METRIC_PREFIX`/`check_series` rationale are rewritten on the new premise: the zero-member §11 exception holds because the name allowlist excludes the 5 join-flow metrics.

### Deletion triggers (R3) and the doc sweep

No dead-pipeline warning moves until legs 0-3 are demonstrated on committed artifacts on Kind. Leg 3 is a recorded observation, not a committed test:

1. Rebuild and redeploy GC plus the collector and Prometheus config.
2. Emit with **sdk-core's own code**, not the raw-OTel harness writers (paired-client 3). Preferred: a browser e2e spec with `VITE_TELEMETRY_ENDPOINT` set. Fallback if Chromium is unusable here: node driving sdk-core's built `configureTelemetry` plus `MediaMetrics.frameDropped('no_kek_for_generation')` through GC with a real user token. Which one ran is recorded under R14.
3. Query Prometheus with `MCMediaMissingKeyMaterial`'s exact selectors: `dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation|no_roster_entry"}` and `dt_client_media_frames_received_total`. The result must carry a real `reason` value, not the sentinel.
4. Record the query and result, plus `gc_telemetry_ingest_total{status="accepted"}` movement.

Then I send owners "legs 0-3 verified" with the evidence and they author their sites. Observability takes `client.md`, `alerts.md`, `dashboards.md`, `alert-conventions.md` and `client-media.json`. Operations takes `mc-alerts.yaml`, `otel-alerts.yaml`, `gc-deployment.md`, `mc-incident-response.md` and `client-dev-local.md`. I take the stale comments in files I'm editing: `setup.sh:676-681` and `deployment.yaml:28-31`, rewritten to present tense (AC/GC/MC live; MH infra-disabled, needs a 4317 egress rule first).

If any leg fails, the warnings are reworded to the narrow true residual (R3 fallback). The TODO §Observability Debt collector entry closes, and its ~11 citing sites are enumerated and cleared together (DRY D-7). The org_id entry is gone (fixed, R12); the guest entry is the single latent-gap home (R13).

### Enumeration (DRY D-3)

| Name                                                                                                                                                    | Export? | Why                                                                                                                                                                                                                    |
| ------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| the 13 `dt_client_media_*` names plus `dt_client_time_to_first_media_frame_ms` (the 14 in `mediaMetrics.ts`)                                            | IN      | media contract label set; observability R1                                                                                                                                                                             |
| `dt_client_media_send_queue_depth` (one of the 14)                                                                                                      | IN      | gauge, last-writer-wins with N browsers; observability documents it as single-writer-meaningful (R4)                                                                                                                   |
| `dt_client_join_attempts_total`, `_signaling_connection_total`, `_mh_connection_total`, `_time_to_signaling_ready_ms`, `_time_to_first_mh_connected_ms` | OUT     | §11 grandfathers `meeting_id_hash` as emitted, not its export to stored series. `mh_connection_total` spreads the join bag (`MediaTransport.ts` ~417-419); `keep_keys` is the backstop if it is ever name-allowlisted. |

### Test reviewer items

1. The acceptance is the table above, recorded here and copied with logs into §Devloop Verification Steps. The harness temporality is set in `sdk-writers.mjs` via the SDK's own `temporalityPreference`, the same knob R1 sets in `telemetryConfig.ts`.
2. GC unit tests are specified above; both halves are asserted on `(key, value)`.
3. **Path scrapeable, series gated on task 8:** the MH admission series `mh_media_stream_admission_total` has **no emitter yet**: MH task 8 adds it. Its scrape path is live today (`up{job="mh-service"} == 1` on both pods; mh netpol :8083 ingress from Prometheus). Nothing in this task touches that path. So the S1 diagnostic is NOT exercised in this loop. It becomes a client task 15 concern once task 8 lands.
4. No second `dt_client_*` read-back is committed here. Observability owns the shared name constant and client task 15 owns the assertion.

### Lead Rulings #2 folded in (R10-R14)

- **R10 (`VITE_TELEMETRY_ENDPOINT`)**: exported in `scripts/layer7.sh`'s browser-e2e block and in `scripts/dev-web.sh`. The value is same-origin via the Vite dev proxy to GC `/api/v1/telemetry`, so there is no CORS path. I will verify the proxy forwards that route, and add the route to the proxy config if it doesn't. Leg 3 checks that `gc_telemetry_ingest_total{status="accepted"}` moves.
- **R11 (value controls)**: an anchored, nil-guarded charset regex rewrites to the sentinel on the four enum keys; there are no vocabulary allowlists.
  - **Cap re-ruled by main to `{1,64}`** (R11 originally said `{1,40}`). Security has since ratified 64, with observability and paired-client agreeing. The measured floor is 32 (`payload_length_exceeds_available`), not 31. (The earlier `nameGuard.ts:18` citation is withdrawn: that 64 bounds metric names, not label values.) Too tight fails silent by sentinelling the alert's own token; too loose costs nothing because the charset is the control.
  - The comment and the single TODO home state the rest explicitly:
    - the regex bounds bytes and charset, NOT distinct-value count, on `reason`/`outcome`/`action`/`source`;
    - the patched-client argument applies to those four;
    - the accepted exposure is series-count inflation, which is visible in series count, `memory_limiter` and `max_streams`.
  - No regex on `org_id`, which is now server-authored (R12). "Cardinality" is used nowhere as a claim about the length cap.
  - Added harness assertions:
    - legal substring with illegal surroundings (`"X truncated X"`) becomes the sentinel;
    - a label never sent is still absent;
    - real values survive.
- **R12 (org_id overwrite)**:
  - Durable justification (observability + DRY): nothing was ever exported, so no historical series exists under the old subdomain semantics. The migration cost is zero now and will never be this low again.
  - GC set-or-inserts `org_id` from the authenticated `UserClaims.org_id` on client metric datapoints and on the trace path, so the label never means two things. The payload value is never trusted. Paired-global-controller owns the threading shape in `telemetry.rs`/`telemetry_filter.rs`, and `key_custody` stays drop-on-mismatch.
  - Tests: a spoofed `org_id` goes in and the claims value comes out, as `(key, value)`; a payload without `org_id` gets the claims value.
  - Consequence for the collector: `org_id` is now server-authored, so none of the six label values is left without a control. The comment wording becomes: four keys charset-checked, `client_version` shape-checked, `key_custody` pinned, `org_id` server-stamped.
  - Docs: observability updates `client.md` (the label is the org UUID) and the catalog. The org_id-injection TODO entry is dropped; the guest entry stays.
  - Stop rule: if claims are not available at filter time, GC and I tell main immediately.
- **R13**: the guest-telemetry gap stays as one latent-gap TODO entry, plus ops' pointer in Scenario 16. **Every N+1-demo browser must authenticate as a registered user**, because guest tokens 401 at the telemetry proxy and their telemetry disappears.
- **R14 (leg-3 record form)** in §Devloop Verification Steps:
  - the exact PromQL and exact result;
  - the committed collector tag and digest;
  - the SDK commit or bundle under test;
  - cluster state (pods, images, `up`);
  - **client task 15** named as the open standing-regression obligation, i.e. the committed browser read-back.

### Forward note for task 13 (story row edit alongside R1's delta-bullet correction)

Task 13's changeset now also edits `infra/services/otel-collector/configmap.yaml`: its four new client metrics join the name allowlist, and `mode` joins `keep_keys` for `dt_client_media_capture_source{mode}`. It also adds `Exported:` markers in `client.md`. G1/G2 make an uncatalogued or unexported client metric a red build. Observability has pre-ruled all four as `Exported: yes`. **Task 13's own GSA hop (`kek_generation_stale` in `proto/test-vectors/frame-v2.vectors.json`) is unchanged by task 3.**

### R15 — Blocker 5: the exporter sent no `Authorization` header

Found while setting up leg 3, and it would have made R10 a no-op: GC's telemetry route
is behind `require_user_auth` (a **route layer**, so it rejects before the handler), while
`createMetricExporter` passed only `url` and `temporalityPreference`. Every browser export
would 401 — the failure moving from "never exports" to "exports and is rejected", still an
empty Prometheus, still indistinguishable from an idle cluster.

- **Shape:** `TelemetryConfig.authTokenProvider?: () => string | undefined`, wired to the
  exporter's `headers` as an async factory. The exporter awaits it on EVERY export, so the
  token is read live.
- **Credential lifetime (security + semantic-guard):** the SDK holds the FUNCTION REFERENCE
  and never a resolved token. Per-export evaluation is load-bearing twice — rotation (the
  token appears at sign-in and changes on re-auth) and retention (a cached copy would be a
  second credential home that no sign-out path clears). The provider projects ONLY the token
  field; the export path never receives the session object.
- **No mechanical backstop, stated at the site:** `ts_retained_credentials.rs` keys on
  declaration members, so a closure-local memo matches nothing it indexes. The per-export
  unit test — two exports with different tokens yielding different headers, driven through
  the REAL exporter's factory — is the only executable defence.
- **Fail closed:** no header at all before login, never `Bearer undefined`. In practice the
  case does not arise: `PeriodicExportingMetricReader` skips the export when nothing was
  recorded, and every `dt_client_*` metric fires after sign-in.
- **Single credential home:** the provider is installed from `App.svelte`, where
  `AuthSession` lives, and the call moved out of `main.ts`. A module-level accessor was
  rejected — it would need its own clearing on sign-out and on the 401 drop, and would drift.
- **Diag logger, version-checked:** a dev/test-only `diag` error logger makes export failure
  audible. Verified at the pinned `otlp-exporter-base` 0.221.0 that the error payload carries
  no request context (status number only), so the logger cannot print the bearer. Recorded as
  version-checked at the site because `ts_pii.rs` would give no signal on a future bump.
- **Same-origin assertion** in `loadConfig()`, throwing rather than warning. Both values are
  resolved with `new URL(value, base)` — the base is kept on BOTH sides deliberately, since
  `gcBaseUrl` still defaults to `''` and `new URL('')` throws a `TypeError` on the DEFAULT
  path. Asserted against the GC API base, not the page origin, so a CDN topology still works.
- **Endpoint spelling:** `VITE_TELEMETRY_ENDPOINT` stays RELATIVE (`/api/v1/telemetry`).
  `validateUserProvidedUrl` resolves it against `location.href` at construction, so it tracks
  the per-run org subdomain automatically and is same-origin by construction. An absolute
  value would bake one subdomain into a build-time constant and go cross-origin on every other
  run — a silent CORS death. Absolute is required ONLY where there is no `location`: the node
  leg-3 fallback and the unit tests (`vitest.config.ts` is `environment: 'node'`).

**GC endpoint attribution (security's finding, same root cause).** `normalize_endpoint` had no
static arm for the telemetry paths, so a telemetry 401 landed in `/other` alongside every 404,
probe and scanner hit — recorded but unattributable. Both OTLP paths are now static arms (fixed
spec literals, two label values), with a test asserting they do not normalize to `/other`. This
is what lets leg 3 distinguish 401 from 202, and it is why the leg-3 evidence must report "no 202
observed" and "401s observed" as SEPARATE failures: a run that sent nothing also produces no 401.

**Accepted deferral (security's ruling): a telemetry-scoped token.** Not this loop. The SDK
already sends this same bearer to GC on the join path, so R15 changes frequency, not destination,
and the cost lands in a Guarded Shared Area. Recorded in `docs/TODO.md` with its honest benefit
and limit, not as a live defect.

### Semantic-guard question

No hunk constructs or populates a `dark_tower.internal.v1` message or touches mc-service, so items 11-13 stay silent.

---

## Gate 1 — Plan Confirmations

| Reviewer                   | Plan Status |
| -------------------------- | ----------- |
| Security (paired)          | confirmed   |
| Test                       | confirmed   |
| Observability (paired)     | confirmed   |
| Code Quality               | confirmed   |
| DRY                        | confirmed   |
| Operations (paired)        | confirmed   |
| Semantic Guard             | confirmed   |
| Global Controller (paired) | confirmed   |
| Client (paired)            | confirmed   |

---

## Lead Rulings (Gate 1)

Binding Lead rulings issued during planning: `/tmp/devloop/lead-rulings-1.md` (R1-R9) and `/tmp/devloop/lead-rulings-2.md` (R10-R14); the implementer folds them into §Planning. Summary: R1 SDK DELTA temporality in scope; R2 acceptance on real-emitter shape + three-writer skew case; R3 warnings removed only on legs 0-3 verified in-loop, else reworded; R4 hygiene premise-pin positive control; R6 GC datapoint-only tier; R7 14-name allowlist, client.md SSoT + guard; R8 ops fixes; R9 doc-premise ownership; R10 VITE_TELEMETRY_ENDPOINT in scope; R11 charset regex, cardinality residual stated; R12 GC overwrites org_id from authenticated claims (R5 deferral reversed on corrected premise); R13 guest gap latent TODO; R14 leg-3 record form. R11 cap re-ruled: `{1,64}` (charset is the control; too-tight fails silent, too-loose fails visible; floor token `payload_length_exceeds_available` = 32, measured).

Gate 1 closed 2026-09-23: all 9 confirmed; `validate-cross-boundary-classification.sh` → STATUS=OK.

---

## Prior Attempt

Attempt 1 (same slug, escalated in planning, never committed) recorded its two-writer evidence on 0.103.1; the recovered text is at `/tmp/devloop/prior-task3-attempt1-main.md` (container view) and summarised in `.devloop` escalation `task-3.escalation.json`. Its drafted "more blockers remain" doc corrections are NOT re-applied (task prompt).

---

## Implementation Summary

Five blockers stood between the client SDK and a queryable `dt_client_*` series. The task named
two; three more were found during planning and implementation, each with the same shape — a
component that looks wired and observes nothing.

| # | Blocker | Fix |
|---|---|---|
| 1 | The pinned collector cannot sum same-identity delta datapoints | Bump to 0.161.0 + `delta_to_cumulative` + a collector-clock restamp |
| 2 | The GC filter strips `reason`/`outcome`/`action`/`source`/`key_custody` | A datapoint-only five-key tier on the allowlist |
| 3 | The SDK exported CUMULATIVE, making the collector fix a no-op on real input | DELTA temporality on the exporter |
| 4 | No environment set `VITE_TELEMETRY_ENDPOINT`, so no browser emitted at all | Exported in `layer7.sh` and `dev-web.sh` |
| 5 | The exporter sent no `Authorization` header, so every export would 401 | A per-export async token provider |

### Collector (mine)

- Image `0.161.0`, with the verified digest recorded on the image line and the tag-vs-digest
  decision explained (the digest form is unproven through `setup.sh`'s preload path).
- Metrics pipeline: `memory_limiter` → name allowlist (14 curated names) → temporality gate →
  `keep_keys` + value-shape rewrites + `key_custody` set → collector-clock restamp →
  `delta_to_cumulative` → `[debug, prometheus]`. Traces unchanged except `memory_limiter`.
- `add_metric_suffixes: false`, `resource_to_telemetry_conversion` off, `metric_expiration: 10m`,
  `max_stale: 15m`, `max_streams: 500`.
- `service.telemetry.metrics` on :8888 so the five silent drops become countable.
- Container + Service ports for 8889/8888; a single-element (AND) Prometheus ingress rule; the
  DNS-only egress rule untouched with only its justification corrected.
- Guaranteed QoS at 512Mi, with the measurement and its denominator recorded.

### Prometheus (mine)

Two jobs: `otel-collector` (10s, matching the SDK cadence) and `otel-collector-telemetry`, kept
separate so `up{job="otel-collector"}` keeps meaning "the client-series endpoint". `honor_labels`
left false and documented as a security control. Neither job is keyed on the ac/gc/mc/mh
enumeration.

### GC (paired global-controller + security)

Base `ALLOWLIST` unchanged at 12; a five-key `MEDIA_DATAPOINT_EXTRA` delta admitted only at
datapoint sites through two hard-coded-tier wrappers, so none of the nine call sites chooses.
`key_custody` dropped unless it equals the one legal value. `org_id` server-stamped from the
authenticated claim (R12), set-or-insert on datapoints and overwrite-only elsewhere, with an empty
claim dropping rather than collapsing every tenant into `""`. Telemetry paths added to
`normalize_endpoint` so a telemetry 401 is attributable instead of landing in `/other`.

### Client (paired client)

DELTA temporality via an extracted `createMetricExporter`; a per-export async token provider; a
dev/test-only `diag` error logger (verified against the pinned exporter that it cannot print the
bearer); telemetry setup moved from `main.ts` to `App.svelte` so the token keeps one home; and a
same-origin assertion in `loadConfig()` that throws rather than warns.

### Guard and tests (mine)

New `dt-guard client-metrics-export` with six rules (G1-G6) and a uniform fail-closed
`empty_input` contract distinct from every content failure. `scripts/otel-collector/acceptance.sh`
commits the harness, reading the COMMITTED configmap so it tests what ships. `32_media_metric_hygiene`'s
premise pin — which this change would otherwise have blinded — replaced with a real check plus a
browser-independent positive control.

### Standing residual (operations)

Leg 3 is a RECORDED observation, not a committed test. Between this loop and client task 15 there
is no standing automated control proving the pipeline stays alive: the guard catches encoding
drift and the harness must be run deliberately. Client task 15's browser read-back is the standing
regression control, and it is the one open obligation this loop hands forward.

---

## Devloop Verification Steps

### A. Acceptance harness on the COMMITTED artifacts

`scripts/otel-collector/acceptance.sh` extracts the `config.yaml` block from the committed
`infra/services/otel-collector/configmap.yaml` and runs the committed image in a scratch
namespace (`otel-acceptance`, deleted on exit). Resolved image on every run:

```
image:   otel/opentelemetry-collector-contrib:0.161.0
imageID: docker.io/otel/opentelemetry-collector-contrib@sha256:fd328de2552466ad78385e1b1289c3f2402b1c45f265b252aab1955b42845ac1
```

**Anti-vacuity: the BEFORE column reproduces the failure, the AFTER column fixes it.**

| Case | 0.103.1 (pinned before) | 0.161.0 + `delta_to_cumulative` only | 0.161.0 + committed config |
|---|---|---|---|
| S1 two same-identity delta writers — counter / hist `_count` / `_sum` (want 36 / 6 / 2700) | **7 / 3 / 600** — 5,7,5,7 oscillation; `Misaligned starting timestamps`, histogram points dropped | **29 / 5 / 2000** — writer B's FIRST point dropped (its start predates the stream's) | **36 / 6 / 2700** PASS |
| S2 single writer, delta 5 → 20s idle → delta 6 (want 11) | **6** (silent under-count) | 11 | **11** PASS |
| S4 three writers: honest + −30s + +30s skew + late/retried + year-2100, then honest (want 28) | n/a | not run (S1 already red) | **28 / 28** PASS |
| S6 writer restart — a MeterProvider is shut down and a fresh one continues (want 12) | n/a | n/a | **12** PASS |
| S7 stale reset — `max_stale: 20s` override, idle 130s, then delta 6 | n/a | n/a | **6** PASS (visible reset, NOT summed through) |
| S3 filtering controls (13 assertions) | PASS | PASS | **PASS** |

The **S1 middle column is the load-bearing one**: it shows `delta_to_cumulative` ALONE does NOT
sum same-identity multi-writer streams, confirming the attempt-1 caveat. The element that closes
29→36 is `transform/client_metric_clock`, which is keyed on no identity label.

S6/S7 together are the reset-distinguishing pair: a delta carries no reset signal, so a restarting
browser MUST be summed into the fleet total (S6), while the one genuine reset — the accumulator
forgetting a stream — MUST stay visible (S7). **`max_stale` proved to be a LOWER BOUND, not an
exact time**: a 40s idle against a 20s `max_stale` still accumulated, because eviction runs on a
periodic sweep. Forgetting late only lengthens accumulator memory, which is the safe side of the
`max_stale >= metric_expiration + window` relation; the config comment says so.

S3's 13 assertions include the ones that pin the two OTTL traps and the real-value cases:
`"X truncated X"` → `invalid` (anchors), a metric sent without `reason` still has no `reason`
(nil-guards), and `client_version=0.0.0`, `reason=no_kek_for_generation` and
`reason=kek_generation_stale` all survive unchanged.

### B. Real-SDK acceptance (`acceptance.sh "" sdk`)

Driven by `scripts/otel-collector/sdk_writers.mjs` using sdk-core's OWN pinned packages
(`@opentelemetry/sdk-metrics` 2.10.0 + `exporter-metrics-otlp-proto` 0.221.0), DELTA, OTLP-proto,
post-GC resource shape. All PASS on the committed image + config:

- SDK-S1 two real MeterProviders, same identity, B started later → **36 / 6 / 2700**
- SDK-S2 idle (four empty periodic flushes) → **11**
- SDK-S6 writer restart → **12**
- SDK-S5 a stale CUMULATIVE writer sharing a delta writer's identity → delta total **15**, intact
  (the temporality gate excludes the cumulative writer rather than corrupting the stream)

### C. Component proof from the pulled digest

`otelcol-contrib components` run from the exact digest lists `delta_to_cumulative`, `filter`,
`transform`, `attributes`, `memory_limiter`, receiver `otlp`, exporters `debug` + `prometheus`,
extension `health_check`. Read from the image, not from release notes — 0.103.1 is the proof that
matters, since it loads this same config far enough to CrashLoop on the missing processor.

### D. Full stack initializes against the new image (deployed Kind cluster)

`dev-cluster rebuild gc`, then `kubectl apply -k` for the collector and observability overlays.

```
otel-collector-868768cfdd-zzsdt  READY=true
  imageID docker.io/otel/opentelemetry-collector-contrib@sha256:fd328de2…5ac1
ac-service-0/1, gc-service (both), mc-0, mc-1, mh-0, mh-1  READY=true
```

**Recorded accurately as 3-of-4, not 4-of-4.** AC, GC and MC set `OTEL_ENABLED=true` in the Kind
overlay and genuinely probe the collector at init. **MH does not** — it has no `OTEL_ENABLED` and
no 4317 egress rule, so its leg passes because it never runs, which would be a vacuity-class claim
if reported as "four of four green".

`health_check` on `/` :13133 still serves readiness on 0.161.0 — both probes, `setup.sh`'s
`kubectl wait`, and `00_cluster_health.rs` are unaffected.

### E. Scrape jobs live

```
otel-collector            192.168.91.247:8889  up=1
otel-collector-telemetry  192.168.91.247:8888  up=1
```

All eight pre-existing jobs still up=1.

### F. `otelcol_*` family classification (for observability's rules)

Read off the RUNNING pod on the committed tag, per family, with the three fields requested —
present-at-idle, value at idle, and how any non-zero was obtained.

| Family | Present at idle | Value at idle | Non-zero obtained |
|---|---|---|---|
| `otelcol_processor_filter_datapoints_filtered` | **ABSENT** | — | **INDUCED** (see below) → 1 |
| `otelcol_processor_incoming_items` / `_outgoing_items` (`otel_signal=traces`) | present | 765 / 765 | by design |
| `otelcol_processor_incoming_items` / `_outgoing_items` (`otel_signal=metrics`) | **ABSENT** | — | induced → 1 / 0 |
| `otelcol_receiver_refused_metric_points` | **ABSENT** | — | not induced (name verified present after first metric point, value 0) |
| `otelcol_receiver_failed_metric_points` | **ABSENT** | — | not induced (same) |
| `otelcol_exporter_send_failed_metric_points` | **ABSENT** | — | **not induced**, and name NOT yet observed |
| `otelcol_processor_memory_limiter_accepted_metric_points` | **ABSENT** | — | induced → 1 |
| `otelcol_deltatocumulative_streams_tracked` | **present** | 0 | harness (>0) |
| `otelcol_deltatocumulative_streams_limit` | **present** | 500 | gauge |
| `otelcol_deltatocumulative_streams_max_stale` | **present** | 900 | gauge |
| `otelcol_process_memory_rss` | **present** | 204 996 608 (195.5 MiB) | gauge |

**The headline for rule authors: every metric-flavoured family is state 2 — ABSENT until the first
metric datapoint flows, then present.** So absence means BOTH "never failed" AND "no client metrics
at all", and a bare `> 0` rule cannot tell them apart. Only the `deltatocumulative` gauges and the
process gauges are present-at-idle. The `deltatocumulative` family keeps the OLD spelling in its
metric names even though the processor is configured under the new `delta_to_cumulative` name.

**Note for whoever writes the drop rule:** `otelcol_processor_filter_datapoints_filtered` carries
NO `processor` label, so it cannot distinguish the name filter from the temporality filter. The
per-processor `incoming_items − outgoing_items` difference can. That difference is a subtraction of
two counters scraped together but not incremented atomically, so it can read transiently negative
at a scrape boundary — fine for triage, needs `clamp_min` before it is ever an alert.

**Induction method (a positive control, not an assumption):** a single OTLP datapoint named
`dt_client_not_allowlisted_probe_total` was POSTed to the live collector. Being outside the name
allowlist it is dropped by `filter/client_metric_names`, which makes the drop counters non-zero
while storing NOTHING — verified afterwards: `{__name__=~"dt_client_.+"}` returned **0 series**, so
the probe left no residue in the shared cluster.

### G. Memory

Measured RSS on the DEPLOYED collector: **195.5 MiB**. Against the previous 256Mi limit that is
~76%; against the new 512Mi request==limit it is ~38%. The bump is load-bearing, not comfort: two
new stateful components (`delta_to_cumulative` accumulators, exporter series held for
`metric_expiration`) both grow with series count.

### H. Legs 0-3: ALL VERIFIED

| Leg | Status |
|---|---|
| 0 — SDK emits DELTA | **VERIFIED** — unit test asserts the REAL exporter SELECTS delta for counters and histograms |
| 1 — collector sums same-identity deltas | **VERIFIED** on committed tag + committed config (§A, §B), and again end-to-end below |
| 2 — GC filter passes the five keys | **VERIFIED** in unit + integration tests AND observed on the live wire below (`reason` survives) |
| 3 — end-to-end, real SDK → GC → collector → Prometheus | **VERIFIED** (record below) |

#### Leg 3 record (R14 form)

**Artifacts under test**
- Collector: `otel/opentelemetry-collector-contrib:0.161.0`, running imageID
  `docker.io/otel/opentelemetry-collector-contrib@sha256:fd328de2552466ad78385e1b1289c3f2402b1c45f265b252aab1955b42845ac1`
- GC: rebuilt from this working tree (telemetry filter + `normalize_endpoint` arms)
- SDK: sdk-core's BUILT bundle `packages/sdk-core/dist/index.mjs` — the emission used
  `configureTelemetry` + `MediaMetrics` from that bundle, NOT a hand-rolled OTLP payload, so the
  names, label set, delta temporality and auth header are all the SDK's own
- Cluster: `devloop-hear-each-other`; AC/GC/MC/MH + collector + Prometheus all Ready

**Method.** A per-run org was provisioned, a REAL user registered against AC, and the resulting
user token supplied through the R15 `authTokenProvider`. Four separate node processes each emitted
7 `frameReceived`, one `frameDropped('no_kek_for_generation')`, one
`frameDropped('no_roster_entry')` and one `frameAccepted`, then flushed.

**Queries and exact results**

```
dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation|no_roster_entry"}
  -> reason="no_kek_for_generation"  4
     reason="no_roster_entry"        4
  full label set observed on each series:
     client_version="0.0.0"
     org_id="743bfaaf-c368-497d-bf54-44b679ca25fc"     <- the AUTHENTICATED org UUID
     key_custody="operator"
     job="otel-collector"   instance="192.168.91.247:8889"   <- the COLLECTOR pod
     exported_job="darktower-sdk-core"                        <- no exported_instance
     otel_scope_name="darktower-sdk-core"  otel_scope_version="0.0.0"

dt_client_media_frames_received_total
  -> 28            <- 4 separate processes x 7, SUMMED. The two-writer property,
                      demonstrated with real SDK emission on the live cluster.

(sum(rate(dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation|no_roster_entry"}[5m]))
 / sum(rate(dt_client_media_frames_received_total[5m]))) > 0.05
 and sum(rate(dt_client_media_frames_received_total[5m])) > 0
  -> 0.2857142857142857      <- THE FULL MCMediaMissingKeyMaterial EXPRESSION, non-empty.
                                The alert now evaluates and would fire (subject to for: 15m).
                                It has never before been able to match at any threshold.

{__name__=~"dt_client_.+",meeting_id_hash!=""}
  -> EMPTY         <- keep_keys proven on the live wire, not just in the harness

gc_http_requests_total{endpoint="/api/v1/telemetry/v1/metrics",status_code="202"}  -> 1+
gc_http_requests_total{endpoint="/api/v1/telemetry/v1/metrics",status_code="401"}  -> EMPTY
gc_telemetry_ingest_total                                                          -> 1+
```

**The two auth outcomes are reported separately, per paired-client's condition:** a 202 was
OBSERVED on the attributed endpoint (so telemetry was genuinely sent and accepted), and NO 401 was
observed on it (so auth succeeded). A run that sent nothing would also produce no 401, which is why
the 202 is asserted independently rather than inferred from the absence of the 401. Both labels are
attributable only because of the `normalize_endpoint` arms added in this change — previously they
collapsed into `/other`.

**Scope of the "401 → EMPTY" reading (paired-client + security, Gate 3).** It is POINT-IN-TIME for
the leg-3 run. `gc_http_requests_total` is cumulative, and every browser-e2e run produces exactly
**one** expected telemetry 401 (measured across the 10-test run: one `status 401`, one export failure,
both from the same spec; the nine others — five with real joins and teardowns — produced none). So
after any Layer-7 run `{endpoint="/api/v1/telemetry/v1/metrics",status_code="401"}` reads >= 1, and
**any future telemetry-401 alert must tolerate a baseline of one per browser-e2e run.**

*Mechanism.* `auth-rejection.spec.ts` ("unauthenticated meeting-join") injects a well-formed but
INVALID token. `MeetingSession.join()` fails and tears itself down, and its `flushMetrics()`
(`MeetingSession.ts:654`) runs inside that teardown — BEFORE `onSessionInvalid()` clears `auth`
(in the `catch` at `JoinMeeting.svelte:74`). So the flush carries that invalid token, and GC rejects
an invalid credential, not a missing one. Nothing reachable is lost: that interval holds only
join-flow counters, none on the collector's name allowlist. The `.catch` swallows the rejection, and
the console line is the dev/test `diag` logger working (`layer-7.log` has zero
`Bearer`/`Authorization` hits).

*Barred repair — do not re-litigate.* Snapshotting `auth.userToken` at teardown start so the final
flush can authenticate is ruled out by security. It creates a second home for a bearer credential at
the moment the first is being cleared, outlives the revocation it was meant to respect, and is
invisible to `ts_retained_credentials.rs` (a local at a teardown site). It is also pointless here:
the flush already holds the only token that session ever had.

*This is also a POSITIVE CONTROL, not just an explained anomaly.* It is live in-browser evidence,
through the real proxy, that GC's route-layer auth rejects an invalid bearer on the TELEMETRY path —
nothing else in this change shows that end to end. The two credential cases have DIFFERENT
instruments, and must not be read as one:
- an **invalid** credential is rejected by GC — **observed in-browser**, once per run, from
  `auth-rejection.spec.ts`;
- an **absent** credential produces no `Authorization` header at all (never `Bearer undefined`) —
  **unit-tested only**, in `telemetryConfig.delta.test.ts`, and **not observed end to end**.

**Residue:** the emitted series carry a per-run `org_id` and expire from the exporter after
`metric_expiration` (10m), so they self-clean. No fixture or seed data was left in the cluster.

### I. env-test 32 (R4) verified against the live cluster

```
running 3 tests
test no_metric_relabeling_narrows_this_suite_silently ... ok
test client_series_carry_no_meeting_identifier_when_present ... ok
test media_metric_labels_carry_no_meeting_identifier_and_no_overclaim ... ok
test result: ok. 3 passed
```

**Non-vacuous:** the run printed no "no `dt_client_*` series present" note, so the R1 client leg
genuinely evaluated over real client series (the leg-3 emissions, still inside
`metric_expiration`) rather than over an empty set.

**The positive control is proven capable of firing**, which is the point of having one:

```
up{job="otel-collector"}        -> 1          (control satisfied)
up{job="otel-collector-typo"}   -> EMPTY      (a wrong/missing job makes `.any()` false
                                               and the assertion panics)
```

So a wholesale-empty result — wrong selector, dead job, broken query path — cannot pass green.
That is the exact defect the test REPLACED: the old premise pin would have kept passing forever,
because client series moved to `job="otel-collector"` while its selector listed only the four Rust
services.

#### What this means for the deletion triggers

All of legs 0-3 are demonstrated on committed artifacts, so under R3 the dead-pipeline warnings
and deletion triggers ARE satisfied and the owner-implemented doc sites may move to the working
end-state. That signal has been sent to observability and operations.

---

## Code Review Results

Gate 3 closed in the interrupted session (verdicts recovered from `task-3.devloop.log` on resume, 2026-09-24; every reviewer confirmed nothing in flight under the tree freeze).

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-DEFERRED | 9 | 8 | 1 | Collector image digest structural verification deferred |
| Test | RESOLVED-DEFERRED | 2 | 1 (+ scope-lock comment) | 1 | Telemetry token-surface positive control → client task 15 |
| Observability (paired) | RESOLVED-DEFERRED | 1 | 1 | 3 residuals | Label keys unguarded; hedged gc-service.md counts; 3 alerts specified-not-loaded |
| Code Quality | RESOLVED-FIXED | 2 | 2 | 0 | Ownership Lens passed; no GSA path touched |
| DRY | RESOLVED-DEFERRED | 1 (Gate 3) | 1 | 1 | Label-key guard gap (shared with observability) |
| Operations (paired) | RESOLVED-DEFERRED | 4 | 3 | 1 | `setup.sh` non-idempotence over a half-built cluster |
| Semantic Guard | CLEAR | 0 | 0 | 0 | native SAFE |
| Global Controller (paired) | RESOLVED-FIXED | 2 | 2 | 0 | F1 duplicate `org_id` keys, F2 per-request warning volume |
| Client (paired) | RESOLVED-FIXED | 5 | 5 | 0 | |

At least one accepted deferral exists, so five reviewers landed on RESOLVED-DEFERRED.

---

## Accepted Deferrals

All entries have their body in `docs/TODO.md`, with an owner and a trigger.

- Collector image pinned by a mutable tag; digest checked by a runbook step only (security; owner infrastructure). Trigger: the next collector bump, or any move off local-preload Kind.
- Nothing guards a client metric's label keys, only its name (observability + dry-reviewer). Known next instance: task 13's `dt_client_media_capture_source{mode}`.
- Three telemetry-pipeline alerts are specified but not loaded (operations + observability).
- Three hedged cardinality counts remain in `docs/observability/metrics/gc-service.md` (observability).
- `setup.sh` is not idempotent over a half-built cluster: item (G) under §Devloop Container Resource Hygiene (operations).
- The telemetry token-surface positive control in `assertTokenOnlyJoinTraffic` is handed to client task 15 (test).

---

## Rollback Procedure

1. Start commit: `c3a0d459ba80d99bdf93b15bdf5b04bff7624268`
2. `git diff c3a0d459..HEAD`; `git reset --hard c3a0d459` for a clean revert.
3. Infra: re-apply the prior otel-collector (0.103.1) / prometheus manifests to Kind if applied.

---

## Issues Encountered & Resolutions

**Gate 2 attempt 1 — L7 `cluster-rebuild-failed`, caused by this diff.** `setup.sh`'s third-party
image preload extracted images from rendered Kustomize output with an UNANCHORED
`grep -oP 'image:\s+\K\S+'`. Rendered output includes ConfigMap literals, and a collector-config
comment ("…Measured on the pinned / # image: without this…") matched, so setup ran
`podman pull docker.io/without` and bring-up failed. Fixed on both sides: the comment is reworded,
and the extractor is hoisted into `extract_manifest_images()` and anchored to a real YAML key
(`^\s*(-\s+)?image:`), so no comment or prose can ever break bring-up again. Five `emi-*` cases in
`scripts/setup.test.sh` pin it; against the OLD regex three of them fail with exactly the Gate-2
value (`without`), so they are not vacuous. The anchored extractor yields the identical 10-image
set as before on the current tree.

**`run-story-selftest` containment failure during Gate-3 fix-up (concurrent writer, not the suite).**
One layer-fast run reported `run-story.test.sh: 453 passed, 0 failed` yet `STATUS=FAIL`: its EXIT-trap
containment check saw the real repo's `diff HEAD` hash change mid-run with `status --porcelain`
unchanged — an in-place edit to an already-modified file. Reviewers were editing concurrently at the
time (security's INDEX trim, observability's `alerts.md`/TODO edits). Standalone the suite held
containment twice, and a re-run on a quiescent tree (tree hash verified unchanged across the run) was
fully green. Recorded rather than smoothed over; the check was not touched.

**Retro (raised by security): concurrent reviewer edits vs. validation windows.** Reviewers editing concurrently is what tripped the containment check above. The lesson is not that reviewers should stay out of the tree, because the owner of a doc is the right person to change it. The gap was sequencing: the Lead asked for a last-minute INDEX line-cap fix before declaring a validation window. Fix: declare the window first, then ask for last-minute reviewer fixes. (Security's INDEX edit overran the 75-line cap, the guard caught it, and security fixed it at the implementer's request before the freeze. It was not a write made during a validation window.)

**Session interruption (2026-09-24 00:13Z, `auth-expired`).** The final Gate 2 re-run on the post-Gate-3 tree reached L1-L6 green (`/tmp/devloop/gate2-verdict`, per-file hashes). L7 then ended `RESULT=UNKNOWN REASON=no-child-statuses` when the session's OAuth credentials expired. On resume the tree was verified byte-identical to those hashes and the full pipeline was re-run (see Gate 2 Log).

**Formatting churn (self-inflicted, reverted).** A hand-run `pnpm nx format:write` reformatted ~630
unrelated files including every past devloop record; all reverted and the three in-scope docs
rebuilt from HEAD plus their semantic edits only. Not pipeline behaviour.

---

## Gate 2 Log (Lead)

- **Attempt 1** (layers 1-6 attempt 1): L1 OK, L2 OK, L3 OK, L4 N/A (wrapper aggregate), L5 OK, L6 N/A (no dep changes), **L7 PRECONDITION_FAILURE `cluster-rebuild-failed`**. The diff caused it: the collector-config comment `# image: without this, …` in `infra/services/otel-collector/configmap.yaml` survives into rendered kustomize output, and `setup.sh`'s unanchored `grep -oP 'image:\s+\K\S+'` preload extractor tried to pull `docker.io/without`. Routed to the implementer (implementer lane, uses the attempt). Fix: reword the comment and anchor the extractor. Log: `/tmp/devloop/gate2/attempt1.log`.
- **Attempt 2** (re-run after the fix): L1-L6 green again. **L7 PRECONDITION_FAILURE `cluster-setup-failed`** because attempt 1's aborted setup left the cluster half-built (Calico applied, no workloads), and `setup.sh` is not idempotent over that state (`AlreadyExists`). Operator lane, so no attempt was used. The Lead confirmed `Pods healthy: false` and ran `dev-cluster teardown` on the devloop's own broken cluster, then re-ran. Log: `/tmp/devloop/gate2/attempt2.log`.
- **Attempt 3** (fresh cluster; counts as layers 1-6 attempt 2): L1 OK, L2 OK, L3 OK, L4 N/A (aggregate; `cargo-test-passed`, `nx-test-passed`), L5 OK, L6 N/A (aggregate), **L7 OK** (`env-tests-passed`, `browser-e2e-passed` 10/10, duration 1532s). `TOTAL_RESULT=N/A`, exit 0. This is the same accepted shape as task 2's Gate 2. **GATE 2 PASS.** Observation routed to review: the browser-E2E log shows one `PeriodicExportingMetricReader: metrics export failed … non-retryable status 401` from a `flushMetrics` → `forceFlush` (session teardown path). Log: `/tmp/devloop/gate2/attempt3.log`, `/tmp/devloop/layer-7.log`.
- **Attempt 4: final re-run on the post-Gate-3 tree, 2026-09-24, resumed session.** The first try at 00:03Z reached L1-L6 green, then L7 ended `UNKNOWN no-child-statuses` when the session's OAuth expired. That is operator lane, so it used no attempt. On resume the tree was byte-identical to that run's per-file hashes. Full re-run: L1 OK, L2 OK (no `FMT_APPLIED`), L3 OK, L4 N/A (aggregate), L5 OK, L6 N/A (aggregate), **L7 OK** (`env-tests-passed`, `browser-e2e-passed`; the cluster was rebuilt because `infra/kind/` changed). `TOTAL_RESULT=N/A`, exit 0, 1359s. **GATE 2 PASS on the reviewed tree.** This also covers paired-global-controller's caveat: its two filter fixes had landed after attempt 3, and they now have cluster and browser coverage. Log: `/tmp/devloop/gate2/final2.log`.
