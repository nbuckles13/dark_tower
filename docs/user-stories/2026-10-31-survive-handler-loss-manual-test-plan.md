# Manual Test Plan — Survive a Handler Loss (story 4, task 31 / T-2)

Story: [2026-10-31-survive-handler-loss.md](2026-10-31-survive-handler-loss.md). Design:
ADR-0039 §4 (migration), §7 (addressing, `mh.sh`), §9 (verification). Requirement: R-41.

A deliberately short human pass: real browsers hear and see each other through an MH crash and
through planned drains. Everything else the story requires is proved by the automated suites
(Rust `40_resilience`, browser `mh-replacement.spec.ts`); this plan adds a human eye on what
those read only through tokens, metrics and the e2e bus: whether media actually comes back and
what the gap feels like. Observed times are evidence, never pass/fail thresholds (ADR-0036 §10).

## Setup

Static Kind cluster built from the story branch, on the host, from the repo root:

```bash
./infra/kind/scripts/teardown.sh
./infra/kind/scripts/setup.sh
DT_TEST_TONE=1 DT_TEST_PATTERN=1 scripts/dev-web.sh
```

A meeting gets `GC_MH_INITIAL_HANDLERS_PER_MEETING` (2) handlers and the Kind baseline is
`MH_BASELINE` (2), so a meeting lands on both baseline MHs. `mh.sh signal` is owner-gated and
never touches `baseline`, so swap the baseline for `manual` MHs before anyone joins:

```bash
M=./infra/kind/scripts/mh.sh
$M up --owner manual                 # three times: two carry the meeting, one spare
$M drain <b1> --owner baseline       # the two baseline slots from `$M list`
$M drain <b2> --owner baseline
```

Per-MH load (client connections and registered meetings per slot, read from each pod's
`/metrics`) — run between steps to see which slots carry the
meeting:

```bash
for p in $(kubectl -n dark-tower get pods -l app=mh-service -o name); do
  echo "$p"; kubectl get --raw "/api/v1/namespaces/dark-tower/${p/pod\//pods/}:8083/proxy/metrics" \
    | grep -E '^mh_(active_connections|media_registered_meetings)[ {]'; done
```

Three participants, each a separate Chrome profile signed up with its own display name, join one
meeting at `http://demo.localhost:5173`. Windows stay uncovered (covered windows are throttled).

## Steps and expected results

1. **Baseline.** Everyone sees every other participant's moving test pattern and hears their tone.
2. **Crash a handler that is not forwarding.** `$M signal <k> KILL --owner manual` on a meeting
   slot. Expected: no visible or audible effect; the pod restarts in place.
3. **Crash the forwarding handler.** `$M signal <k> KILL --owner manual` on the other meeting slot.
   Expected: a short media gap (crash detection plus replacement), then audio and video return by
   themselves on a replacement handler, with no rejoin and no reload.
4. **Planned transfer.** `$M drain <k> --owner manual` on a slot carrying the meeting. Expected: no
   gap — the replacement is added before the draining handler is dropped.
5. **Clean up.** Everyone leaves; `$M up --owner baseline` twice; `$M drain --all --owner manual`.

## Run record — 2026-11-16, Nathan Buckles, WSL2 + Windows Chrome, story branch at `7e1edb9e`

| Step | Result | Observation |
|------|--------|-------------|
| 1 | PASS | All three see and hear each other. Meeting on slots 2 and 3, 3 client connections each; slot 4 idle spare. |
| 2 | PASS | Killed slot 2: no impact on audio or video (it was not forwarding). Slot 2 restarted in place. |
| 3 | PASS | Killed slot 3: audio and video stopped for about 6–7 s, then recovered cleanly with no rejoin. Afterwards the meeting was on slots 3 (new incarnation) and 4 (the former spare). |
| 4 | PASS | Drained slot 4, then slot 2: absolutely no impact on audio or video for either drain. The meeting ended on slot 3 alone. |
| 5 | — | Restore baseline after the run. |

## Not run

- **ADR-0039 §9 long-meeting planned transfer**: a planned transfer in a meeting older than
  max(join JWT TTL, media_dial TTL), proving dial credentials are re-minted rather than reused.
  Step 4 ran in a meeting a few minutes old. The automated suites cover the credential path;
  the human pass of the long-meeting case was not run.
- Stranded pair (two clients that cannot reach the surviving handler): covered by
  `mh-replacement.spec.ts` (STRANDED leg), not walked by hand.
