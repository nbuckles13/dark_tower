# TODO

Operational tech-debt tracker. Entries live here when they can't be fixed in
the devloop that surfaces them — typically because the fix crosses ownership
boundaries or requires judgment that belongs to a different specialist.

## ADR-0031 Convention Follow-ups

Non-blocking refinements to the conventions + guard, for future devloops.

## ADR-0031 label-canonicalization follow-ups

Non-blocking coordinated renames surfaced during the 2026-04-17
ADR-0031 Prereq #3 devloop (metric-labels guard + taxonomy). Each rename
requires a **coordinated migration** across four surfaces atomically:

1. `crates/<svc>-service/src/observability/metrics.rs` — rename the label key.
2. `infra/grafana/dashboards/<svc>-*.json` — update every PromQL `expr` that
   selects on the old label.
3. `infra/docker/prometheus/rules/<svc>-alerts.yaml` — update every alert
   `expr` that selects on the old label.
4. `infra/docker/alertmanager/*.yaml` routing config if any rule matches on
   the renamed label.

A staged rename (old + new both emitted for one deploy cycle, queries
updated, then old removed) avoids a visibility gap but takes two PRs per
service. Single-atomic-PR is acceptable with a deploy-window announcement.
Guards will catch missing dashboard/alert updates: `validate-dashboard-panels.sh`
and `validate-alert-rules.sh` fail on references to removed metrics/labels.

Canonical-name target list lives in
`docs/observability/label-taxonomy.md` §Shared Label Names (reviewer-only).

### AC `path` / GC `endpoint` → canonical `endpoint`

- Affected metrics: `ac_http_requests_total`, `ac_http_request_duration_seconds`,
  `gc_http_requests_total`, `gc_http_request_duration_seconds`.
- Ripple: every HTTP-route panel/alert in `ac-*.json`, `gc-*.json`,
  `ac-alerts.yaml`, `gc-alerts.yaml`.
- GC's `status` (categorized `success`/`error`/`timeout`) also drifts from
  AC's raw `status_code`. Consider aligning on both at once — emit
  `status_code` (raw) + `status_category` (categorized) from GC, drop the
  overloaded `status`. AC already emits `status_code`; no AC change needed.

Owners: `auth-controller` + `global-controller` specialists (coordinate).
No deadline; non-blocking.

### MC bare `type` → `heartbeat_type`

- Affected metrics: `mc_gc_heartbeats_total` (label `type`),
  `mc_gc_heartbeat_latency_seconds` (label `type`).
- Ripple: any MC heartbeat panel/alert that selects on `type`. The bare
  `type` name shadows a very generic identifier and makes cross-service
  dashboards ambiguous (e.g., grouping by `type` across metrics would mix
  heartbeat-type with unrelated dimensions).

Owner: `meeting-controller` specialist. No deadline; non-blocking.

### MC + MH `event` / AC `event_type` → canonical `event_type`

- Affected metrics: `mc_mh_notifications_received_total` (`event`),
  `mh_mc_notifications_total` (`event`), `ac_audit_log_failures_total`
  (`event_type`).
- Ripple: MC + MH notification dashboards/alerts; AC audit-log alerts.
- AC is already canonical — only MC and MH rename.

Owners: `meeting-controller` + `media-handler` specialists (coordinate).
No deadline; non-blocking.

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
