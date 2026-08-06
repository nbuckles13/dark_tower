# Manual Test Plan — Browser Client Join (pre-close validation)

**Story**: [2026-05-02-browser-client-join](2026-05-02-browser-client-join.md)
**Date**: 2026-08-05
**Status**: Not started
**Purpose**: Build confidence in the E2E suite before `/close-story` by manually covering
the seams automation structurally cannot see: real Windows Chrome over mirrored
networking, two distinct users, rendered UI (tests read the dev-only event bus),
leave/teardown, and the observability surface as a whole.

References: `docs/runbooks/client-dev-local.md` (launch + failure modes F1–F9),
`packages/web-app/e2e/` (current suite), `docs/observability/metrics/*.md` (catalogs).

Result convention: mark each checkbox, note anomalies inline under the step. Any
defect found routes through `/devloop` before story close.

---

## Phase 0 — Cold start (WSL2)

```bash
unset DT_HOST_GATEWAY_IP                                  # F8: stale export rewrites advertise addrs
scripts/dev-web.sh --check                                # preflight only
./infra/kind/scripts/setup.sh                             # several minutes first run
grep demo.localhost /etc/hosts || echo '127.0.0.1 demo.localhost' | sudo tee -a /etc/hosts
scripts/dev-web.sh                                        # leave running
```

Windows Chrome → `http://demo.localhost:5173`.

- [ ] Preflight green (fingerprint WARN acceptable only before `setup.sh` has run)
- [ ] Page renders in Windows Chrome
- [ ] If Chrome fails but WSL2 `curl -I http://127.0.0.1:5173` works → mirrored-networking
      regression (F9) — stop and fix `.wslconfig` before proceeding

Access points: Prometheus `:9090`, Grafana `:3000` (`admin/admin`), Loki via Grafana
Explore. AC `:8443`, GC `:8444` (plain HTTP on loopback — throwaway passwords only).

## Phase 1 — Happy path with per-hop observability verification

Sign up → create meeting → join. After each hop, verify the metric moved (Prometheus)
and the log event fired (Loki or `kubectl logs -n dark-tower -l app=<svc>`).

| # | Hop | Metric must move | Log evidence |
|---|-----|------------------|--------------|
| 1 | Register | `ac_token_issuance_total{status="success"}` | AC `auth_events` audit row |
| 2 | Create | `gc_meeting_creation_total{status="success"}` | GC "Meeting created successfully" |
| 3 | Join (GC) | `gc_meeting_join_total{participant="user",status="success"}`, `gc_mc_assignments_total{status="success"}` | GC join info with `mc_id, mh_ids` |
| 4 | Join (MC) | `mc_session_joins_total{status="success"}`, `mc_connections_active` ↑ | MC "JWT validation succeeded" |
| 5 | Media handshake | `mh_webtransport_connections_total{status="accepted"}` on BOTH mh-0/mh-1, `mh_active_connections` ↑, `mc_participant_mh_status_total{state="connected"}` ↑ | MH "Connection established for registered meeting" |

- [ ] All five hops verified (metrics + logs)
- [ ] Four overview dashboards (`ac/gc/mc/mh-overview`) populate — no "No data" panels
      where traffic exists
- [ ] Zero alerts firing during the healthy run (Prometheus → Alerts)
- [ ] Reminder: green UI ≠ working join — metrics are ground truth (runbook §4)

Useful queries:

```promql
sum(increase(gc_meeting_join_total{participant="user",status="success"}[5m]))
histogram_quantile(0.95, sum by(le)(rate(gc_mc_assignment_duration_seconds_bucket{status="success"}[5m])))  # expect < 0.02
```

```logql
{app="gc-service"} |= "Meeting created successfully"
{app="mc-service"} |= "JWT validation succeeded"
{app="mh-service"} |= "Connection established for registered meeting"
```

## Phase 2 — Two DISTINCT users (E2E gap: suite reuses one account in two contexts)

Second Chrome profile or incognito window: register a different account, join the same
meeting code.

- [ ] User 1's roster shows user 2 with the correct display name (rendered UI, not bus)
- [ ] Participant IDs distinct
- [ ] `mc_connections_active` = 2
- [ ] MC logs show both joins — check BOTH mc-0 and mc-1 (assignment may split)

## Phase 3 — Leave / teardown / rejoin (uncovered at any tier browser-side)

Close user 2's tab.

- [ ] User 1's roster drops user 2 (note how long it takes: ______)
- [ ] `mc_connections_active` back to 1; `mh_active_connections` decremented
- [ ] `mc_mh_notifications_received_total{event_type="disconnected"}` moved
- [ ] MC/MH logs show disconnect cleanup

User 2 rejoins the same code.

- [ ] Clean re-entry: new participant ID, roster correct on both sides

## Phase 4 — Error UX as a human sees it (E2E asserts codes; this checks pixels)

For each: error is readable and actionable on screen, app remains usable, and a
successful join immediately after the failure works WITHOUT a page refresh.

- [ ] Wrong password at sign-in → clear error, then successful sign-in works
- [ ] Join with garbage code (fails client-side shape check) → clear error
- [ ] Join well-formed-but-unknown code → clear error (404 path), no session drop,
      then successful join of a real code works

## Phase 5 — Telemetry proxy reality check

With the app open ~10 min: `sum(increase(gc_telemetry_ingest_total[10m]))`.

Two acceptable outcomes — the point is to record which one is true:

- [ ] SDK emits: 202s visible in the network tab for `/api/v1/telemetry/v1/*`,
      `status="success"` counter moves; note `gc_telemetry_pii_attributes_dropped_total`
- [ ] SDK does not emit yet (client metrics catalog was stubbed this story): counter
      flat AND `GCTelemetryProxySilent` does not page (day-1 zero-baseline gating)

Observed reality: ______________________

Note: `gc_http_*` normalizes the telemetry route to `endpoint="/other"` — do not use an
`endpoint=~".*telemetry.*"` selector (tracked in `docs/TODO.md` §Observability Debt).

## Phase 6 — Resilience drill (optional)

With a session live on mc-0: `kubectl rollout restart deployment/mc-0 -n dark-tower`.
No reconnection logic exists this story — pass criterion is a CLEAN, VISIBLE failure.

- [ ] No zombie "joined" UI, no silent hang
- [ ] Fresh join works after the pod is back

Skip the cert-expiry (F7) drill unless the 14-day window is near — runbook drill, not
product behavior.

---

## Promote-to-E2E verdicts

| Candidate | Verdict | Reasoning |
|-----------|---------|-----------|
| Distinct-user two-party join + roster | **Yes** | Identity seam untested browser-side; one extra registration fits the 4/run budget |
| ParticipantLeft / teardown / rejoin | **Yes — highest value** | Uncovered anywhere browser-side; needs a small `participantLeft` bus event + `mc_connections_active` delta assertion |
| Join-after-error recovery | **Yes, cheap** | One assertion appended to each existing negative spec |
| Roster rendered-DOM assertion | **Yes, trivial** | Ties bus truth to rendered UI |
| Guest join from browser | No (this story) | No guest UI in demo app — note in next story's test requirements |
| Telemetry ingest from real SDK | Defer | Decide from Phase 5's observed reality |
| Cert-expiry / networking / pod-restart drills | No | Ops drills, runbook material; pod-restart becomes E2E-worthy when reconnection ships |

The four "Yes" items = one `/devloop` task (test specialist), to run BEFORE
`/close-story` so the story ships with its own gaps closed.

## Known doc drift (fix at close)

- `docs/runbooks/client-dev-local.md` §7 still says "no Playwright E2E yet" — written by
  task #20 before #18/#19 landed the suite.
