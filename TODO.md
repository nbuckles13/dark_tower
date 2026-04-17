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

## `/devloop` skill: cross-ownership friction on small changes

**Seed a `/debate` — input needed from all specialists before changing the skill.**

### Motivating cases

1. **ADR-0031 alert-rules devloop (2026-04-17)**: operations implementer needed to touch `gc-alerts.yaml` and `mc-alerts.yaml` (~60 lines of mechanical YAML edits: severity renames + URL rewrites) to make the newly-authored guard pass against existing files. "That's GC/MC specialist territory" framing produced a grandfather-allowlist mechanism (~80 LOC of guard complexity + TODO entries + follow-up devloop slots) to avoid the edit. In retrospect, the edits were mostly mechanical with 2-3 genuine judgment calls; the allowlist mechanism was disproportionate.
2. **MCActorPanic `for: 0m` fix in the same devloop**: one-line change to `mc-alerts.yaml` by the operations implementer. Required an explicit Lead ruling to authorize. Should have been trivial.

### The tension

`/devloop` today has an implicit rule: "the file's owner specialist implements changes to it." This default is correct for changes requiring domain judgment (threshold tuning, behavior changes, API semantics) but produces disproportionate ceremony when:
- The change is mechanical (renames, format conformance, path updates, comment fixes)
- The change is a minor defensive adjustment (bumping a `for:` duration up to match convention)
- The file-touching is incidental to the primary work (convention-driven cleanup that naturally spans services)

In those cases the ownership-boundary fetish produces: elaborate workarounds, multiple devloops where one would do, Lead-level adjudication thrash, infrastructure we'll delete in months.

### Known design axes

1. **Define "acceptable cross-boundary edit."** Options:
   - By size (`≤ N lines`) — crude, easily gamed, wrong axis. A 5-line threshold tune is high-judgment; a 100-line sed is not.
   - By change category — mechanical vs. minor-judgment vs. domain-judgment. Tracks the thing that actually matters but requires the implementer to self-classify honestly.
   - By file path × change pattern — e.g., "any specialist may rename across the tree; only the owner may change semantics." Probably the cleanest rule but needs careful category definitions.
2. **Owner involvement model.** Three levels, probably all needed:
   - **Review-only** — owner sees it in the standard reviewer gate.
   - **Approval-required** — owner must explicitly ACK the specific cross-boundary hunk (not just the overall PR).
   - **Owner-implements** — route to a separate devloop with owner as implementer.
3. **Ownership detection.** The skill already has a keyword → specialist map for auto-detection; extending to a file-path → specialist map is straightforward. But needs care for shared areas (`crates/common/**`, `proto/**`, `docs/observability/**`).
4. **Default posture.** When the implementer surfaces a cross-boundary edit, should the default be "proceed with review" or "defer to owner"? The just-finished devloop showed that "defer" as a default produces large ceremony costs. But flipping to "proceed" risks specialists stepping on each other's core domains.

### Constraints the debate should respect

- **Don't overcomplicate the skill.** Every rule added to `/devloop`'s SKILL.md increases the Lead's coordination surface. The skill is already dense; the Lead has finite attention.
- **Preserve genuine cross-cutting safety.** `crates/common/**`, `proto/**`, and auth-critical paths genuinely need multi-specialist involvement. Any rule must not weaken those.
- **The simplest rule beats the cleverest mechanism.** The allowlist's complexity came from trying to satisfy contradictory scope adjudications; a clear default would have prevented the whole thing.

### Desired output

An update to `.claude/skills/devloop/SKILL.md` (or a new short protocol file referenced from it) that gives the implementer and Lead a clear answer to "can I just edit this file?" in common cases, without inflating the skill.

Owner: Lead (@team-lead). No deadline; schedule the `/debate` when bandwidth permits.
