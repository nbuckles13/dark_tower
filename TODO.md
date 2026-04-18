# TODO

Operational tech-debt tracker. Entries live here when they can't be fixed in
the devloop that surfaces them — typically because the fix crosses ownership
boundaries or requires judgment that belongs to a different specialist.

## ADR-0031 Convention Follow-ups

Non-blocking refinements to the conventions + guard, for future devloops.

### Alert aggregation conventions: instance / region / global

`alert-conventions.md` doesn't currently document **when to use per-instance, per-region, or fleet-wide aggregation** in alert expressions. Different signal classes call for different aggregation levels, and the missing convention is producing real user-facing blind spots.

**Motivating example — `MCGCHeartbeatWarning`**: uses unqualified `sum(rate(...)) / sum(rate(...))` across the fleet. Catches acute fleet-wide failure. Silently misses sustained regional partial degradation: if all MCs in a region persistently run at ~7% heartbeat error rate (e.g., flaky cross-AZ network, regional GC endpoint degradation), the fleet aggregate hovers around ~5% and never crosses the 10% threshold. User-visible failures in that region accumulate without the alert firing.

Note: this gap is **not new** — the pre-consolidation two-tier alert had the same shape. FU#2's consolidation didn't introduce it; it just made the absence of regional coverage more visible.

**The general problem**. Alert expressions implicitly choose an aggregation level. Each level catches a different failure class:

- **Instance-level** (`by (pod)` or `by (replica)`) — single-instance crashes, deploy failures, individual-node resource pressure. Usually `warning` when load-balancing compensates; `page` when it can't (e.g., single-instance services). Dark Tower's current alerts largely skip this level.
- **Regional** (`by (region)` / `by (zone)` / `by (az)`) — shared-infrastructure issues that present as "everyone in one place is equally degraded." Cross-AZ network, regional dependency outage, regional upstream provider degradation. **Today absent from every Dark Tower service's alert set.**
- **Fleet-wide** (unqualified aggregate or `sum by (service)`) — genuine service-wide failure where healthy instances can't cover. Current default in all our warnings and pages.

Related observations that cluster under this gap:
- Observability reviewer's FU#2 forward-looking note about the `MCGCHeartbeatWarning` severity being load-bearing on ≥ 2-replica topology. Same shape: fleet cardinality is an unstated assumption in the alert shape.
- Service specialists authoring their first alerts under ADR-0031 don't currently have guidance on which aggregation level to pick for which signal. MH exemplar-first devloop made defensible default choices per rule but didn't have conventions to lean on.

**Desired output**:
1. A new §"Aggregation-level conventions" in `docs/observability/alert-conventions.md`. Specifies which signal classes warrant which aggregation level, with anchor examples per level, including when to pair multiple (e.g., fleet `warning` + regional `warning` covering different failure modes).
2. Audit existing `<svc>-alerts.yaml` files for each service. Flag alerts whose current aggregation level leaves a gap. Likely produces a batch of `MCGCHeartbeatRegional`-style additions.
3. Consider a `[reviewer-only]` guidance rule in the conventions doc's rule index making aggregation-level-choice a review prompt when authoring new alerts.

**Debate-worthy**. Affects every service's alert authoring; touches the shared conventions doc. Probably wants `/debate "alert aggregation conventions — when to use per-instance, regional, and fleet-wide aggregation"` with observability + operations + all service specialists before drafting the conventions update. Don't bolt on as a bunch of individual alerts without settling the shape.

Owners: observability (conventions owner) + meeting-controller (surfaced the motivating case) + all service specialists at debate time. No deadline.

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

### GC `status` → `status_code` + `status_category` split

- Affected metrics: `gc_http_requests_total`, `gc_http_request_duration_seconds`
  (and any other GC metric currently emitting the overloaded `status` label).
- Ripple: every HTTP-route panel/alert in `gc-*.json` and `gc-alerts.yaml`
  that selects on `status`, plus any alertmanager routing config matching on
  the label.
- Schema change (not a rename): GC today emits a categorized
  `status` (`success`/`error`/`timeout`), which drifts from AC's raw
  `status_code` and conflates two dimensions. Split into `status_code` (raw
  HTTP code, matching AC's canonical name) + `status_category` (categorized
  bucket) so queries can aggregate either way. AC already emits `status_code`
  and has no categorized variant; no AC change needed.
- Because this is a schema change rather than a pure rename, existing
  dashboards/alerts won't just need find-and-replace — categorized queries
  need to switch to `status_category`, code-based queries to `status_code`.
  Deferred to its own devloop so the migration can be sequenced carefully
  (coordinated-migration discipline: metrics + dashboards + alerts +
  alertmanager routing land atomically, or behind a staged dual-emit).

Owner: `global-controller` specialist. No deadline; non-blocking.

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
