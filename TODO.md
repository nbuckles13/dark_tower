# TODO

Operational tech-debt tracker. Entries live here when they can't be fixed in
the devloop that surfaces them — typically because the fix crosses ownership
boundaries or requires judgment that belongs to a different specialist.

## ADR-0031 Alert Migration

Existing alert files are grandfathered into `validate-alert-rules.sh` lenient
mode until the owner service specialist completes migration per ADR-0031.

**Hard deadline: 2026-06-30** (90 days from 2026-04-17 allowlist creation).
CI surfaces this date in every guard run via the `[LEGACY]` WARN output.

Reference: [ADR-0031](docs/decisions/adr-0031-service-owned-dashboards-alerts.md).
Migration guidance: [`docs/observability/alert-conventions.md`](docs/observability/alert-conventions.md).

### `infra/docker/prometheus/rules/gc-alerts.yaml`

**Owner**: `global-controller` specialist.
**Deadline**: 2026-06-30.
**Reviewers**: observability (threshold plausibility), operations (severity routing), security (annotation hygiene recheck).

Violations to resolve:

1. **Severity labels**: 7 rules currently use `severity: critical`, which is
   outside the ADR-0031 `{page, warning, info}` set. Migrate each to `page`
   or `warning` per `docs/observability/alert-conventions.md` §severity-taxonomy.
   Service specialist decides page-vs-warning per alert based on user-impact
   calibration. NOT mechanical — judgment required. Affected rules:
   - `GCDown`
   - `GCHighErrorRate`
   - `GCHighLatency`
   - `GCMCAssignmentSlow`
   - `GCDatabaseDown`
   - `GCErrorBudgetBurnRateCritical`
   - `GCMeetingCreationStopped`
2. **Runbook URLs**: 23 `runbook_url` annotations use absolute
   `https://github.com/yourorg/dark_tower/blob/main/docs/runbooks/...` form.
   Rewrite to repo-relative `docs/runbooks/...` form. Closes the exfil-on-click
   vector per ADR-0031 line 92.
3. **Alertmanager routing**: the severity rename will require a concurrent
   update to `docs/observability/alerts.md` §Alert Routing (lines ~710-735).
   No `alertmanager.yml` in repo at time of writing; verify again at migration
   time in case one has since been added.

Acceptance criteria:
- All severities in `{page, warning, info}`.
- All `runbook_url` values start with `docs/runbooks/` and the target file exists.
- `scripts/guards/simple/validate-alert-rules.sh` passes strict mode against the file.
- Entry removed from `scripts/guards/simple/alert-rules.legacy-allowlist`.
- `EXPECTED_ALLOWLIST_COUNT` decremented in `scripts/guards/simple/validate-alert-rules.sh`.
- `docs/observability/alerts.md` §routing updated if severity routing labels changed.

### `infra/docker/prometheus/rules/mc-alerts.yaml`

**Owner**: `meeting-controller` specialist.
**Deadline**: 2026-06-30.
**Reviewers**: observability, operations, security.

Violations to resolve:

1. **Severity labels**: 6 rules currently use `severity: critical`. Migrate per
   `docs/observability/alert-conventions.md` §severity-taxonomy. Judgment
   call. Affected rules:
   - `MCDown`
   - `MCActorPanic` (edge case — actor supervision bounds user impact;
     specialist should weigh pageability against automatic remediation)
   - `MCHighMailboxDepthCritical`
   - `MCHighLatency`
   - `MCHighMessageDropRate`
   - `MCGCHeartbeatFailure`
2. **Runbook URLs**: 20 `runbook_url` annotations use absolute GitHub URLs.
   Rewrite to repo-relative `docs/runbooks/...` form.
3. **Alertmanager routing**: same as gc-alerts.yaml.

Acceptance criteria: same as gc-alerts.yaml (strict guard pass, allowlist
entry + `EXPECTED_ALLOWLIST_COUNT` decrement).

## Post-Migration Protocol

When a file completes migration:
1. Remove its line from `scripts/guards/simple/alert-rules.legacy-allowlist`.
2. Decrement `EXPECTED_ALLOWLIST_COUNT` in
   `scripts/guards/simple/validate-alert-rules.sh` to match.
3. Run `scripts/guards/run-guards.sh` locally; `validate-alert-rules` should
   pass strict mode for that file.
4. Remove the corresponding entry from this TODO.md file.

The guard fails CI if the allowlist active-line count doesn't match the pin,
so forgetting step 2 is caught automatically.

## ADR-0031 Convention Follow-ups

Non-blocking refinements to the conventions + guard, for future devloops.

### `for:` floor should recognize expr-window patterns

The current `for: ≥ 30s` floor in `validate-alert-rules.sh` assumes `for:` is
the flap-suppression mechanism. A legitimate alternative pattern uses the
expr window for flap suppression and `for: 0m` for immediate fire-on-match:

```yaml
- alert: PanicDetected
  expr: increase(panic_total[5m]) > 0   # 5m window smooths transient noise
  for: 0m                                # fire as soon as the expr is truthy
```

Surfaced during this devloop when `mc-alerts.yaml:33` (`MCActorPanic`) hit the
guard's `for: ≥ 30s` rule despite being a correctly-designed immediate-fire
alert. Workaround: bumped to `for: 30s` (negligible detection delay since the
rule window is 5m). Proper fix is a guard enhancement:

- Detect rate/increase/sum_over_time expressions with `[Nm]` windows where
  N ≥ 30s.
- When detected, exempt the rule from the `for:` floor.
- Update alert-conventions.md §`for:` conventions to document the pattern
  and its exemption.

Owner: `operations` specialist. No deadline; non-blocking.
