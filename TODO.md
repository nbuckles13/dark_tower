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

<!-- `/devloop` cross-ownership friction entry removed 2026-04-18: resolved by
ADR-0024 §6 (Cross-Boundary Ownership Model). Operational surface updates
(SKILL.md, review-protocol.md, agent definitions) tracked as Implementation
Items #17-32 of ADR-0024. Debate record:
docs/debates/2026-04-18-devloop-cross-ownership-friction/debate.md -->

