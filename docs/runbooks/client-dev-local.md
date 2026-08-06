# Runbook: Local Browser-Client Dev Environment

**Service(s)**: web-app demo + AC / GC / MC / MH (local Kind)
**Owner**: operations
**Last Updated**: 2026-08-06
**Executable companion**: `scripts/dev-web.sh`

> ## ⚠ NON-GOAL BANNER — THIS IS NOT A PRODUCTION PATTERN
>
> This is the **dev-loop runbook**. Production deployment, CSP/COEP/COOP headers, CDN serving,
> and synthetic probes are tracked separately under **ADR-0028 §10**.
>
> Nothing below is a production pattern. Named explicitly, so none of it travels:
>
> - **A self-signed dev CA** plus Chrome `serverCertificateHashes` pinning for the MC/MH
>   WebTransport leaves. Production uses real chains and no pinning.
> - **`demo.localhost` and hosts-file shims** to get a subdomain into the `Host` header.
> - **Host-published loopback NodePorts.** Real clusters do not publish service ports to a laptop.
> - **A single-node Kind cluster with `disableDefaultCNI`** and Calico installed by a shell script.
> - **A seeded `demo` organization** created by raw SQL during bring-up.
> - **Fixed, repo-committed OAuth client credentials.** These are *not secrets* — they are
>   identical on every developer's machine and live in the repo. The hazard is not that they leak;
>   it is carrying the *mechanism* (fixed seeded client secrets, printed to a terminal) into an
>   environment where those values are supposed to be secret. Values live in
>   `infra/kind/scripts/setup.sh` and `docs/LOCAL_DEVELOPMENT.md`; they are deliberately not
>   reproduced here.
> - **AC on host port 8443 and GC on host port 8444** (the `hostPort` values in
>   `infra/kind/kind-config.yaml`) **are plain HTTP, not TLS**, despite the
>   TLS-conventional port numbers. Both services bind plain TCP listeners in dev-Kind. The
>   sign-up form therefore puts a password on the wire in cleartext (loopback only, so the
>   exposure is bounded) — **use a throwaway password**, not one you use anywhere else.
>
> This runbook also **deliberately diverges from `docs/runbooks/TEMPLATE.md`**, which is
> alert-shaped (Alert / Severity / Impact / Blast Radius / Escalation / Post-Incident). No alert
> fires on a dev laptop, there is no SLO or blast radius, and there is no oncall to page. What
> transfers — the per-scenario Symptom → Diagnosis → Fix shape — is kept and extended in §5.
> `docs/runbooks/devloop-validation.md` is the existing precedent for a non-alert operational
> runbook in this directory, so this is house practice rather than one author's preference.
> Please do not "correct" it back to the template.

---

## Table of Contents

- [§0 Which machine am I on?](#0-which-machine-am-i-on)
- [§1 Which cluster am I running?](#1-which-cluster-am-i-running)
- [§2 Prerequisites](#2-prerequisites)
- [§3 Bring-up](#3-bring-up)
- [§4 Is the join real?](#4-is-the-join-real)
- [§5 Failure modes (F1–F11)](#5-failure-modes-f1f11)
- [§6 Teardown](#6-teardown)
- [§6.5 Automated checks that exist today](#65-automated-checks-that-exist-today)
- [§7 Not on this branch](#7-not-on-this-branch)
- [Related runbooks and docs](#related-runbooks-and-docs)
- [Changelog](#changelog)

---

## 0. Which machine am I on?

**This is the single most important section in the document.** "Host" means two different
computers here, and almost every first-run failure is a value that is correct on one of them and
wrong on the other.

```
┌─────────────────────────────── Windows box ───────────────────────────────┐
│                                                                            │
│   Chrome  ──────────────────────────────────────────────────┐              │
│     │  http://demo.localhost:5173  (the app)                │              │
│     │  https://127.0.0.1:4433/4434 (QUIC — MC/MH, direct)   │              │
│     │                                                        │              │
│     │  reads: C:\Windows\System32\drivers\etc\hosts          │              │
│     ▼                                                        ▼              │
│  ┌────────────────────────── WSL2 ──────────────────────────────────────┐  │
│  │                                                                       │  │
│  │  Vite dev server  :5173  ──proxy──►  AC :8443  ·  GC :8444 (TCP)     │  │
│  │  podman rootlessport ──► Kind node ──► NodePorts ──► pods            │  │
│  │  kubectl · cargo · pnpm · env-tests · tcpdump · ss                   │  │
│  │                                                                       │  │
│  │  reads: /etc/hosts   (a DIFFERENT file from the one Chrome reads)     │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
```

| I am about to… | Which machine | Notes |
|---|---|---|
| Open the app, watch DevTools, read an error string | **Windows** (Chrome) | The browser never runs in WSL2. |
| Run `dev-web.sh`, `setup.sh`, `kubectl`, `cargo`, `pnpm` | **WSL2** | Every shell command in this runbook, unless it says otherwise. |
| Run `tcpdump`, `ss`, `getent` | **WSL2** | The packets cross into WSL2's stack. |
| Edit `/etc/hosts` | **WSL2** | Affects `curl`, `getent`, env-tests. **Not Chrome.** |
| Edit `C:\Windows\System32\drivers\etc\hosts` | **Windows** | Affects Chrome. Usually unnecessary — see §2.1. |
| Edit `.wslconfig` | **Windows** (`%UserProfile%`) | Requires `wsl --shutdown` to apply. |
| Edit `/etc/wsl.conf` | **WSL2** | Requires `wsl --shutdown` to apply. |

**Convention used below**: every command block is prefixed with the machine it runs on. There are
**two hosts files and two config files**, one pair on each machine, and they are not
interchangeable. When this runbook says "the hosts file" without qualification, that is a bug in
this runbook — please fix it.

---

## 1. Which cluster am I running?

There are **two Kind topologies** in this repo. They are deliberately not aligned (the static
config's own header comment says so), and the browser demo works with exactly one of them.

| | **Static — the demo path** | **Devloop helper — the agent path** |
|---|---|---|
| Brought up by | `./infra/kind/scripts/setup.sh` | `infra/devloop/dev-cluster setup` |
| Config | `infra/kind/kind-config.yaml` (committed) | rendered from `infra/kind/kind-config.yaml.tmpl` |
| Host ports | the fixed `hostPort` values in that file | allocated per-clone, 20000–29999 |
| Published on | `127.0.0.1` (loopback, per R-37) | the podman host-gateway IP |
| MC/MH advertise | the committed IPv4 literals in the configmaps | patched to the gateway IP at deploy time |

**Use the static path.** `scripts/dev-web.sh` defaults to the static host ports, and reads the
MC/MH advertise addresses from the **committed configmap files on disk** — both of which are only
true under the static topology.

`infra/devloop/dev-cluster` is not an alternative you can choose: it is a client that talks to the
devloop helper over a unix socket at `/tmp/devloop/helper.sock`, which exists only inside a running
devloop container. From a plain WSL2 shell it will fail at the socket check.

> **You will see `dev-cluster setup` elsewhere in the repo and it is correct there.**
> `scripts/layer7.sh`, `infra/devloop/devloop.sh`, ADR-0030 and several `docs/TODO.md` entries all
> use it correctly. The rule is not the string, it is **the audience**: a human on the WSL2 host
> gets `./infra/kind/scripts/setup.sh`; the devloop container gets `dev-cluster setup`.

> **Two different bind policies, both deliberate — do not "fix" the second one.**
> The browser-flow ports (AC, GC, MC, MH) bind `listenAddress: 127.0.0.1` per R-37, so a laptop on
> a hostile network never exposes dev services to the LAN. The three **observability** ports
> (Prometheus, Grafana, Loki) deliberately bind the wildcard address instead: viewing dev dashboards
> from a second device is a legitimate pattern and they are a different risk class. That is
> **scoped-and-accepted, not a defect** — the rationale is in `infra/kind/kind-config.yaml`'s own
> comments. If it reads as a defect and someone "corrects" it, the second-device case breaks.
>
> The residual risk is real though, and this runbook sends you to Grafana twice (§4.1, §4.2):
> **Grafana is wildcard-bound *and* ships `admin/admin`.** On an untrusted network that combination
> is a one-step foothold, not merely a visible dashboard. On such a network, either don't bring up
> the observability stack, or add `listenAddress: "127.0.0.1"` to those three port mappings in your
> local copy for that session.

### 1.1 The distinguishing test — which one am I on?

Numbers won't tell you this if you don't already know your own ports. These three behavioural
facts will, and none of them is a copied constant:

```bash
# --- WSL2 ---
kubectl config current-context     # kind-dark-tower  => static.  kind-<task-slug> => devloop clone
ls -d /tmp/devloop-*/ports.json    # exists => devloop clone.  no such file => static
echo "${DT_HOST_GATEWAY_IP:-<unset>}"   # anything but <unset> => you are on / heading for the devloop path
```

**Authorities** (never copied into this document): static host ports are the `hostPort` entries
under `extraPortMappings` in `infra/kind/kind-config.yaml`; dynamic ports are in the port-map JSON,
allocated by `crates/devloop-helper/src/ports.rs`.

> **Path trap.** The port map is `/tmp/devloop-<slug>/ports.json` **on the WSL2 host** but
> `/tmp/devloop/ports.json` **inside the container** — `infra/devloop/devloop.sh` bind-mounts the
> former onto the latter. You are on the host, so use the `-<slug>` form. Copying the container
> path finds nothing and looks exactly like being on the static topology.

---

## 2. Prerequisites

### 2.1 Windows side

> **Not verifiable from this environment.** Everything in §2.1 runs on the Windows box. It was
> not executed or confirmed from WSL2 while writing this runbook, and is stated as mechanism
> plus fallback rather than as verified behaviour.

1. **Chrome.** WebTransport with `serverCertificateHashes` is a Chromium feature; the demo has no
   Firefox/Safari path.

2. **WSL2 mirrored networking is required.** In `%UserProfile%\.wslconfig`:

   ```ini
   [wsl2]
   networkingMode=mirrored
   ```

   Then, in PowerShell: `wsl --shutdown` (it reapplies on next launch).

   *Why*: in WSL2's default NAT mode, Windows cannot reach a WSL2 process bound to WSL2-localhost,
   and UDP is not forwarded — which the QUIC path to MC/MH requires. Mirrored mode makes Windows
   and WSL2 share one network stack. The Vite dev server binds localhost only (it sets no `host`
   in `packages/web-app/vite.config.ts`), so without mirrored mode the browser cannot reach the app
   either. See F9.

   *The TCP-only claim about NAT mode comes from the 2026-07 manual bring-up session, not from a
   documented platform guarantee.*

3. **`demo.localhost` almost certainly needs no Windows hosts entry.** Chromium implements
   RFC 6761 §6.3: names under the `.localhost` TLD resolve to loopback without consulting any hosts
   file. **This has not been empirically confirmed from this environment.**

   **Fallback if `demo.localhost` does not resolve in Chrome**: add the entry to
   `C:\Windows\System32\drivers\etc\hosts` (as Administrator) — **not** WSL2's `/etc/hosts`, which
   Chrome never reads:

   ```
   127.0.0.1  demo.localhost
   ```

### 2.2 WSL2 side

`kind`, `kubectl`, and `podman` (or docker). Install instructions, **and the WSL2 inotify /
kernel-keyring `sysctl` limits**, are owned by `docs/LOCAL_DEVELOPMENT.md` under
"Prerequisites → System Limits". Do not skip the limits: without them `kube-proxy` lands in
`CrashLoopBackOff` with "too many open files", and pods fail with "disk quota exceeded". That
document owns those values; they are deliberately not duplicated here.

### 2.3 WSL2 persistence — stop `/etc/hosts` being regenerated

WSL rewrites `/etc/hosts` on boot unless told not to. In WSL2's `/etc/wsl.conf`:

```ini
[network]
generateHosts=false
```

Then `wsl --shutdown` from Windows. Without this, the `demo.localhost` line from §3 Step 2
disappears on every restart — see F6. Note this makes the entry permanent, which is why §6
covers removing it.

### 2.4 Node and pnpm

Both are pinned in the repo, and the pins are **not restated here** — a second copy would drift:

- Node: **`.nvmrc`**
- pnpm: the **`packageManager`** field in the root `package.json`

`scripts/dev-web.sh` reads both at runtime and prints the exact `nvm install` / `corepack prepare`
command with the value already substituted. Run it rather than transcribing a version by hand.
See F2, F3, F4 for the three ways this goes wrong.

---

## 3. Bring-up

Order: **cluster → hosts entry → dev server → browser.** Demo-org seeding and dev-cert generation
are *not* operator steps; `setup.sh` does both (see Step 1).

### Step 0 — Run the preflight first

```bash
# --- WSL2 ---
scripts/dev-web.sh --check
```

This is the fast path and it is the companion to this document: it checks the whole local topology
against the repo's own pins and prints the exact fix command for anything missing. Read
`scripts/dev-web.sh --help` for the current check list — it is maintained in the script and
deliberately not re-enumerated here.

**Read its two severities correctly:**

- **`✗` HARD FAIL** — nothing works without it. The script stops.
- **`!` WARN** — only part of the demo breaks, and the script tells you which part, then starts
  the server anyway. Sign-up and create-meeting run over TCP through the Vite proxy and are
  unaffected by every WebTransport-related warning; **only the join step breaks.**

**One limitation to know.** `scripts/dev-web.sh::check_wt_endpoint` reads the MC/MH advertise
addresses from the committed configmap **files on disk**, not from the live cluster. On the static
topology those agree. On a devloop cluster the live ConfigMap has been patched and the on-disk file
is stale, so the check validates an address nobody is using — **a green preflight there does not
mean the join will work.** See §1 and F8.

### Step 1 — Bring up the cluster

```bash
# --- WSL2 ---
./infra/kind/scripts/setup.sh
```

First run takes several minutes (Calico, images, migrations). **Make sure `DT_HOST_GATEWAY_IP` is
unset before you run this** — if it is exported, `setup.sh` rewrites the MC/MH advertise addresses
and the join will fail in a way that looks exactly like F1. See F8.

**What `setup.sh` did for you, and how to verify it:**

| It did | Verify |
|---|---|
| Seeded the `demo` org (`infra/kind/scripts/setup.sh::seed_demo_org`, idempotent) | `kubectl exec -n dark-tower postgres-0 -- psql -U darktower -d dark_tower -c "select subdomain from organizations"` |
| Generated dev certs + fingerprints, via `scripts/generate-dev-certs.sh` | `ls infra/docker/certs/fingerprints.json` |
| Created the `mc-service-tls` / `mh-service-tls` secrets | `kubectl describe secret mc-service-tls -n dark-tower` |
| Ran migrations, deployed AC/GC/MC/MH, started port-forwards | `kubectl get pods -n dark-tower` |

The `psql` check runs inside the pod deliberately, so no connection string with a password lands in
this runbook or in your shell history. And use `kubectl describe secret`, never
`kubectl get secret -o yaml` — `describe` answers the diagnostic question (does the secret exist,
with the right keys) without dumping a private key into your scrollback and from there into a
ticket.

Running `scripts/generate-dev-certs.sh` by hand is a **rotation** step, not a first-run step — see
F7.

### Step 2 — The WSL2 hosts entry

```bash
# --- WSL2 ---   (affects curl / getent / env-tests. NOT Chrome — see §2.1)
echo '127.0.0.1  demo.localhost' | sudo tee -a /etc/hosts
```

This is append-only and it outlives the cluster, so §6 covers removing it. glibc has no
`.localhost` special case, which is why WSL2-side tooling needs the line even though Chrome does
not.

### Step 3 — Install and launch

```bash
# --- WSL2 ---
scripts/dev-web.sh
```

This runs `pnpm install`, generates the protobuf-es client, and starts Vite.

**Manual fallback** (what the script does, if you need to do it by hand):

```bash
# --- WSL2 ---
pnpm install
pnpm nx run web-app:dev
```

Use the **Nx** form. `pnpm --filter web-app dev` bypasses the task graph and the generated protobuf
code is never produced — see F5.

### Step 4 — Drive the demo

```
--- Windows / Chrome ---
http://demo.localhost:5173
```

Plain **`http`**, port **5173**. (8443 is AC's port, not the app's.) The `demo.` prefix is
required: the Vite auth proxy preserves the `Host` header so AC can extract the org from the
subdomain, per ADR-0020.

Then: **Sign up** → **Create meeting** (copy the code) → **Join** (paste it). Open a second tab or
profile at the same URL, join the same code, and the first tab should show the new participant.

Use a throwaway password — see the banner.

---

## 4. Is the join real?

A green UI is not evidence that the join worked. This section is how you find out, and §4.2 is the
reason it needs its own section.

### 4.0 Triage ladder — cheapest signal first

Work down this ladder. Most readers should never reach rung 4.

| Rung | What | Answers |
|---|---|---|
| 1 | Which state did the client stop at, and what is the on-screen error? | *Which hop failed* |
| 2 | `kubectl logs` on that hop's pods | *Did the server see anything?* |
| 3 | That hop's Prometheus counter | *Arrived-and-rejected vs never-arrived* |
| 4 | `tcpdump` | *Did packets leave the Windows host, and to which address family* |

Rungs 2 and 3 are free and already deployed. Reaching for `tcpdump` first is what made this take a
session to diagnose the first time.

**Rung 1 — what the browser can and cannot tell you.** The client state machine is
`idle → fetching-token → connecting-mc → joining → joined` (plus `disconnecting`), defined in
`packages/sdk-core/src/session/events.ts::MeetingSessionState` and rendered raw at
`data-testid="meeting-state"`.

Three things to know before you read it:

- **The state readout settles at `disconnecting`, not at the state that failed.** `join()`'s catch
  path calls `disconnect()`, so by the time you look, the resting value is `disconnecting`. The
  failing *stage* is visible in the `stateChange` stream, not in the final readout.
- **There is no browser-side join log, by design.** `logJoinEvent` exists in the SDK but has no
  **production** call sites (only its own unit tests) — R-26 emission is deferred (`docs/TODO.md` §Observability Debt). And the
  `dt_client_*` metrics are off unless `VITE_TELEMETRY_ENDPOINT` is set, and even then they export
  to the GC telemetry proxy, never to the console. **So the browser gives you exactly two things:
  the thrown error string and the state it stopped at.** Nothing client-side observes the server —
  which is precisely why F1 is expensive to diagnose.
- **Where those two things actually are — and they are not on the same surface.** The error string
  renders in the DOM at `data-testid="last-error"`. The state renders at
  `data-testid="meeting-state"`, and the full transition sequence is on the replay bus at
  `window.__darktower_test__.events`.

  > **The replay bus carries NO `error` event for a failed join.** On a failure it shows
  > `stateChange: connecting-mc` then `stateChange: disconnecting`, and nothing else — because the
  > SDK *rejects the join promise* rather than emitting `error` while the join is unsettled; the
  > `error` event only fires for post-join failures. The DOM still shows the string, fed by the
  > view's own catch. So the bus and the DOM legitimately disagree, and the surface most people
  > reach for first is the empty one. An absent `error` event is **not** evidence that nothing went
  > wrong.

### 4.1 MC server-side ground truth

MC logs `Join succeeded` at `info`, target `mc.webtransport.connection`, with `connection_id`,
`meeting_id` and `participant_id` — emitted from
`crates/mc-service/src/webtransport/connection.rs::handle_connection`.

```bash
# --- WSL2 ---
kubectl logs -n dark-tower -l app=mc-service --prefix --tail=200 \
  | grep 'Join succeeded'
```

Four things make this check trustworthy rather than misleading:

- **Use `-l app=mc-service`, not one pod.** There are two MC instances (`mc-0`, `mc-1`). A join
  that landed on `mc-1` is invisible if you tail only `mc-0`, and absence-of-log then reads as
  failure when it isn't. `--prefix` tells you which instance served it — you need that for rung 3,
  which is per-pod.
- **`-l` does not show terminated pods.** If MC restarted mid-diagnosis you are reading only the
  live container.
- **Logs are JSON.** A plain `grep` still matches; for readable output:

  ```bash
  # --- WSL2 ---
  kubectl logs -n dark-tower -l app=mc-service --tail=200 \
    | jq -r 'select(.target=="mc.webtransport.connection")
             | [.timestamp, .level, .fields.message] | @tsv'
  ```

- **Do not narrow `RUST_LOG` to `mc_service=…`.** The deployed filter is `info,mc_service=debug`,
  and the bare `info` global is what makes this line visible: the event's target is
  `mc.webtransport.connection`, which does **not** prefix-match `mc_service`. Narrowing to
  `mc_service=debug` silently hides the very line you are looking for. Use `mc=debug` if you want
  the accept-level `debug` lines too.

**Or, in Grafana Explore** (`{namespace="dark-tower", app="mc-service"} |= "Join succeeded"`) —
noting §1's warning that Grafana is wildcard-bound with default `admin/admin` credentials, so on an
untrusted network prefer `kubectl logs` and leave the observability stack down.
`kubectl logs` is primary — it needs no Grafana and no datasource, which matters in a runbook whose
whole subject is first-run breakage. But Loki has one advantage: **it survives a pod restart**,
where `kubectl logs` shows only the current container and `--previous` buys you exactly one. If MC
crash-looped, Grafana is where the evidence still is.

### 4.2 What each green signal actually proves

**A signal is never trustworthy or untrustworthy. It is trustworthy for a specific claim, and
useless for every other one.** When a signal disappoints you, the move is *not* "find a more
reliable signal" — it is: state the claim you actually need, then find the signal that measures
that claim. Skipping the first half is how you replace one wrong answer with another.

The test, which you can run on signals this runbook never mentions:

> For every signal, finish the sentence **"this proves ___"**. If you cannot fill the blank without
> the words *working*, *fine*, or *healthy*, you do not have a signal — you have a vibe. Go back and
> name the claim.

**Start with the case where every signal is green and correct, and there is no media path at all.**

If GC returns an empty `media_servers` list, `MediaTransport`'s `connectAll` resolves
**successfully with `[]`**: its all-failed throw is guarded on having at least one URL, so zero
URLs never enters the throw branch. The client reaches `joined`. MC logs `Join succeeded` — because
signaling genuinely did succeed. **Nothing lied and nothing malfunctioned, and there are zero media
connections.** Nobody designed this; it is emergent from a guard condition, which is why reading
the R-21 design notes would not have warned you.

The second case is deliberate: `connectAll` resolves when **≥1 of N** MH connects (R-21
active/active — one dead MH must not fail a join), and an MH counts as connected iff its transport
became ready *and* the single connect envelope wrote. There is **no frame I/O beyond that
envelope** on this branch. So:

| Signal | This proves | It does **not** prove |
|---|---|---|
| Client state `joined` | Signaling completed, and **either** ≥1 MH accepted a connect envelope **or the fan-out was empty** | media flows; that every MH is up; that there was any MH at all |
| MC `Join succeeded` | **Signaling** completed | anything about the media hop |
| `mediaConnected` / a `mediaConnections` entry | that MH's WebTransport handshake opened | that media flows |
| `mh_webtransport_connections_total{status="accepted"}` | a QUIC **connection attempt** (Initial packet) reached that MH | that a session was established; that the join completed |

**The completeness signal exists — it is server-side.** The client always sends exactly one
`MediaConnectionUpdate` carrying the per-MH reports once `connectAll` settles, on both the success
and failure paths, and *before* the transition to `joined`. There is no empty-reports early return,
so it is sent even when the report set is empty. MC handles it in
`crates/mc-service/src/webtransport/connection.rs::handle_media_connection_update`.

Read it as a **counter delta**, not from the logs:

```bash
# --- WSL2 ---
kubectl port-forward -n dark-tower deployment/mc-0 8081:8081 &
curl -s localhost:8081/metrics | grep mc_participant_mh_status_total
# attempt a join, then re-run the curl and compare
```

`mc_participant_mh_status_total{state}` is incremented once per recorded status entry, so the delta
across one join is the per-state fan-out count directly. Needs neither Grafana nor Prometheus.

| Delta across one join | Means |
|---|---|
| **no increment at all**, *and* `Join succeeded` present | **Empty fan-out.** GC returned no `media_servers`. Zero media connections, everything upstream green. Confirm against the `media_servers` list in the GC join response — see the caveat below. |
| `< N` increments | Partial fan-out — the UI renders this identically to a complete join. |
| `= N`, all `state="connected"` | Full fan-out — **and still only proves each MH accepted a connect envelope.** |

`N` is the number of entries in the `media_servers` the client was handed. Check **both** MC
instances, or use the one `--prefix` in §4.1 identified as serving the join — the metrics endpoint
is per-pod and has no label-selector equivalent.

> **Caveat on the empty row — the counter cannot distinguish three cases.** The increment happens
> *inside* the loop over status entries, so an empty report set produces **no increment at all**.
> What a flat counter actually proves is only *"no MH status entry was ever recorded on the
> participant actor"*, which has three causes: an empty fan-out, the client never sending an
> update, or an update that reached MC and then failed on the handoff to the participant actor.
>
> **`media_servers` is the discriminator, not just a confirmation.** If the GC join response's
> `media_servers` list is **empty**, this is a genuine empty fan-out. If it is **non-empty** and the
> counter is still flat, it is *not* — the update reached MC but never reached the participant actor
> (`record_mh_statuses` returned `Err`, e.g. the actor was gone). MC logs that at `debug` on target
> `mc.webtransport.connection`, which the deployed `info,mc_service=debug` filter drops for the
> reason given in §4.1 — so raise the level for that target if you need to confirm it. Note this is
> §4.1's own prefix-match trap landing on the one line that would otherwise disambiguate the case.
>
> Do **not** try to read `statuses_count` from the logs. It is a span field, and MC emits no event
> on this handler's success path — `fmt::layer().json()` writes one line per *event*, spans appear
> only as context on events, and no span open/close events are configured. So a `jq` filter on
> `mc.media_connection_update` prints nothing on a healthy join, which in a document that trains you
> to read absence as evidence is exactly the wrong signal to reach for.

### 4.3 Packet capture — the last rung

Only when rungs 2 and 3 both show nothing.

```bash
# --- WSL2 ---
sudo tcpdump -ni any -c 20 'udp and port 4433'
```

(4433 is MC-0's WebTransport port in the default single-cluster mapping, per
`infra/kind/kind-config.yaml`.)

- **This assumes your own machine.** `sudo tcpdump` on a shared or corp-managed host reads other
  people's traffic — get authorization first.
- **`-c 20` is deliberate.** An unbounded privileged capture left running in a forgotten pane is
  the real foot-gun here. It self-terminates.
- **UDP only, on purpose.** There is no TCP counterpart in this runbook for the AC/GC hop, and that
  is not an oversight: 8443/8444 are plain HTTP, so a capture there would record sign-up passwords
  and bearer tokens in cleartext. Do not add one.
- **QUIC payload is encrypted.** You are reading packet *presence and direction*, not contents —
  which is exactly what the F1 question needs.
- **Never paste a capture into a ticket or chat.**

> **Expect silence, and do not diagnose it as a fault.** Because the SDK performs no frame I/O
> after the connect envelope, a **healthy** join on the UDP path looks like: QUIC handshake, one
> bidirectional stream, one small write, then **quiet**. There is no media plane on this branch at
> all. A reader who watches the traffic flatline and concludes the system is broken has
> misdiagnosed a working system.

`tcpdump` proves packets reached the host. It does not prove media flows, because media does not
flow on this branch.

### 4.4 Roster check

Open a second Chrome tab or profile at `http://demo.localhost:5173` and join the same code. The
first tab's participant list should gain the new participant. This exercises the MC→client
notification path, which none of the signals above cover.

---

## 5. Failure modes (F1–F11)

Each scenario gives a **discriminator** — something you can run to tell it apart from the
scenarios that share its symptom. F1/F8 and F1/F9 are symptom-identical pairs; without a
discriminator you would be guessing.

### F1 — Join dies at `connecting-mc`; sign-up and create work fine

**Symptom.** Sign-up and create-meeting succeed. Join hangs, then fails with **one** of:

```
SIGNALING: Signaling transport closed
SIGNALING: Join timed out before a JoinResponse was received
```

Both are the same root cause; which one you get depends on whether Chrome's QUIC handshake gives up
first or the SDK's 15-second join deadline does. (The displayed text is `code: message`, and `code`
is the coarse `SIGNALING`, not the inner `TRANSPORT` — see
`packages/web-app/src/lib/errorText.ts::errorText`.) The state readout settles at `disconnecting`;
the failing stage is `connecting-mc`.

**Discriminator.**

```bash
# --- WSL2 ---
kubectl port-forward -n dark-tower deployment/mc-0 8081:8081 &
curl -s localhost:8081/metrics | grep mc_webtransport_connections_total
# attempt a join, then re-run the curl and compare
```

`accepted` **flat** *and* no `Failed to receive session request` warn ⇒ **no QUIC session reached
the MC accept loop** ⇒ F1 (or F8, or nothing published on the port). Check **both** instances —
a flat counter on `mc-0` when the join landed on `mc-1` reads as F1 and isn't. If `accepted` moved,
this is not F1; see the table in F7.

Then rung 4:

```bash
# --- WSL2 ---
getent ahosts localhost | head -1        # ::1 first => mirrored mode prefers IPv6
ss -uln | grep -E ':(4433|4434)'         # which family actually has a listener
sudo tcpdump -ni any -c 20 'udp and port 4433'
```

**Cause.** Under mirrored networking the browser resolves `localhost` to IPv6 `::1`, but podman's
rootlessport publishes IPv4 `127.0.0.1` only. QUIC to `::1` gets no reply, and **UDP has no
happy-eyeballs fallback**, so signaling times out. AC and GC keep working over TCP because TCP
*does* fall back — which is what makes this look like an MC-specific bug rather than an
address-family problem.

**Fix.** MC and MH must advertise the IPv4 literal, not `localhost`. This is already the committed
default in `infra/services/mc-service/mc-0-configmap.yaml` (key
`MC_WEBTRANSPORT_ADVERTISE_ADDRESS`) and its `mc-1` / `mh-0` / `mh-1` siblings. If a live ConfigMap
disagrees with the committed file, something patched it — go to F8.

**Why absence-of-signal is the evidence here.** `accepted` is recorded the moment the accept loop
yields — on arrival of the QUIC Initial packet, **before the TLS handshake** and before any JWT
work — so its absence is a real boundary fact. But it establishes only *"no QUIC Initial reached
the MC accept loop"*, which still bundles three causes: packets never left the Windows/WSL2
boundary, packets were dropped at podman/NodePort, or nothing is listening on the MC WebTransport
port at all. `ss` and `tcpdump` split those.

A failed handshake is **not** among them — that increments `accepted` and then warns, which is the
F7 row. This is exactly why the counter separates F1 from F7 rather than merely narrowing the
search.

**Why it recurs.** It doesn't, once the configmaps are right — but F8 reproduces the identical
symptom from a different cause, so re-check F8 before re-diagnosing F1.

---

### F2 — `node` or `pnpm` "not found", or the wrong version, in a brand-new shell

**Symptom.** `dev-web.sh` hard-fails on Node or pnpm. `which node` finds nothing, or finds a
version that doesn't match the pin.

**Discriminator.** `type nvm` reports "not found" *and* `ls ~/.nvm/nvm.sh` exists. That
combination means nvm is installed but not loaded — distinct from F3, where nvm loads fine and just
points at the wrong version.

**Cause.** nvm is a shell function sourced from `~/.bashrc`, not a binary on `PATH`. If the loader
block is commented out, nothing loads it.

**Fix.** Uncomment the nvm loader block in `~/.bashrc`, then **start a new shell**.

**Why it recurs.** It doesn't, once `.bashrc` is fixed — but it looks identical to F3 and F4 from
the error message alone.

---

### F3 — Correct Node version in one terminal, wrong in the next

**Symptom.** `dev-web.sh` passed an hour ago; now it hard-fails on the Node version. Nothing was
edited.

**Discriminator.** `nvm current` disagrees with `nvm alias default`. (In F2, `nvm` is not a command
at all.)

**Cause.** `nvm use <v>` is **current-shell-only**. A new terminal, or a WSL restart, reverts to the
default alias.

**Fix.** Persist it — `dev-web.sh` prints the command with the pinned version substituted:

```bash
# --- WSL2 ---
nvm alias default <the version in .nvmrc> && nvm use <same>
```

**Why it recurs.** By construction: session-scoped shell state. Every new terminal is a fresh
chance to hit it until the default alias is set.

---

### F4 — `corepack`: "Cannot find matching keyid"

**Symptom.** `corepack enable` / `corepack prepare` fails with a signing-key error, or `pnpm` is on
`PATH` but `pnpm --version` fails.

**Discriminator.** The error names a *keyid*. A missing binary is F2; a version mismatch that
otherwise runs is F3.

**Cause.** The corepack bundled with the older pinned Node carries stale pnpm signing keys.

**Fix.** Upgrade corepack **before** enabling it:

```bash
# --- WSL2 ---
npm install -g corepack@latest && corepack enable && corepack prepare pnpm@<the packageManager pin> --activate
```

**Why it recurs.** It reappears whenever the Node pin moves backwards relative to the pnpm pin.

---

### F5 — Vite cannot resolve `signaling_pb.js`

**Symptom.** The dev server starts, then fails with an import-resolution error for
`signaling_pb.js`. Grepping the repo for `signaling_pb.ts` finds nothing, which makes the import
look wrong rather than missing.

**Discriminator.** You launched with `pnpm --filter web-app dev` (or bare `vite`) rather than
`pnpm nx run web-app:dev`.

**Cause.** The web-app `dev` target declares a dependency on `proto-gen`'s `codegen` target, whose
output is the generated protobuf-es code under `packages/sdk-core/src/proto/` — **gitignored**, so
it does not exist in a fresh clone. `pnpm --filter` selects a package and runs its script directly,
bypassing the Nx task graph, so codegen never runs. The specifier is `.js` while the generated file
is `.ts` (NodeNext ESM), which is why the name you grep for isn't the name in the error.

**Fix.** `pnpm nx run web-app:dev` — which is what `scripts/dev-web.sh` does. Nx caches codegen, so
it is ~free once the proto is unchanged.

**Why it recurs.** `pnpm --filter <pkg> <script>` is the idiomatic pnpm invocation and reads as
correct. It just isn't a task-graph entry point.

---

### F6 — `demo.localhost` stops resolving in WSL2 after a restart

**Symptom.** `dev-web.sh` warns that `demo.localhost` does not resolve. `curl http://demo.localhost:5173`
from WSL2 fails to resolve the name.

**Discriminator.** `getent hosts demo.localhost` returns nothing in WSL2, **and** the entry you
added is gone from `/etc/hosts`.

**Cause.** WSL regenerates `/etc/hosts` on boot unless `/etc/wsl.conf` disables it.

**Fix.** Re-add the line, then apply the permanent guard from §2.3:

```bash
# --- WSL2 ---
echo '127.0.0.1  demo.localhost' | sudo tee -a /etc/hosts
```

**Scope — this is narrower than it looks, and worth being honest about.** What breaks is
**WSL2-side** name resolution only: `dev-web.sh`'s own preflight check, and any WSL2 `curl` or
browser hitting `demo.localhost`. It does **not** break Windows Chrome, which resolves `.localhost`
natively (§2.1). And it does **not** break the Rust env-tests: they connect to `localhost` and pass
the org subdomain as an HTTP **`Host` header** string — nothing resolves `devtest.localhost` or
`demo.localhost`, and `localhost` always resolves.

That last point is §0's mechanism again, in a form that catches people: a **`Host` header** and a
**DNS name** look like the same value and are correct in different boundaries. It is why "add it to
`/etc/hosts`" *feels* like it should fix an AC org-routing problem, and doesn't.

**Why it recurs.** Every WSL restart, until `generateHosts=false` is set.

---

### F7 — Browser refuses the MC/MH WebTransport handshake

**Symptom.** Sign-up and create work; join fails at the media or signaling step. `dev-web.sh` may
warn that `infra/docker/certs/fingerprints.json` is missing — it does not exist in a fresh clone,
since the whole directory is generated and gitignored.

**Discriminator.** Unlike F1, the MC-side counter **moves**:

| `mc_webtransport_connections_total{status="accepted"}` | `warn` `Failed to receive session request` | `Join succeeded` | Reading |
|---|---|---|---|
| flat | absent | absent | **F1 / F8** — nothing reached the accept loop |
| **increments** | **present** | absent | **F7** — packets arrive; QUIC/TLS or H3 setup failed |
| increments | absent | absent | Handshake fine, join failed later — see `mc_session_join_failures_total{error_type}` |
| increments | absent | present | Real *signaling* join (media still unproven — §4.2) |

`accepted` is recorded when a QUIC Initial packet arrives, *before* the TLS handshake, which is
what makes F1 and F7 separable at all. (`status="rejected"` means a capacity refusal — vanishingly
unlikely on a laptop, and **not** a cert problem.)

**Cause.** The fingerprint the browser pins no longer matches the leaf the pods serve — because the
file is missing, because the leaf rotated, or because the dev server is still holding a stale value
in memory.

**Fix — four steps, in this order.** Regenerating certs alone is not enough, and neither is
restarting Vite: on a running cluster, `kubectl apply` is a no-op on an unchanged Deployment spec,
so recreating the TLS secret does **not** restart the pods still serving the old leaf. This is the
sequence `scripts/generate-dev-certs.sh` itself prints when a leaf is near expiry:

```bash
# --- WSL2 ---
./infra/kind/scripts/setup.sh
#   regenerates expiring leaves, recreates the mc/mh-service-tls Secrets, redeploys
kubectl rollout restart deployment/mc-0 deployment/mc-1 deployment/mh-0 deployment/mh-1 -n dark-tower
#   'apply' alone does not restart pods on a secret-only change
# then restart the dev server:
scripts/dev-web.sh
#   the browser-side fingerprints are read at Vite CONFIG time — a running server never re-reads them
```

To confirm which leaf is deployed, compare fingerprints — never dump the PEM:

```bash
# --- WSL2 ---
cat infra/docker/certs/fingerprints.json
kubectl describe secret mc-service-tls -n dark-tower    # names + byte sizes, no values
```

**Why it recurs — on a fixed schedule.** The MC/MH leaves use a **14-day** validity window
(`DAYS_WT_CERT` in `scripts/generate-dev-certs.sh`). That is not a value we chose: **Chrome rejects
a `serverCertificateHashes`-pinned certificate whose validity window exceeds 14 days**, so the cap
is external and must not be "helpfully" lengthened. Expiry is therefore a routine event roughly
every other week, not a first-run-only problem.

> Automating the rollout-restart inside `setup.sh` is tracked in `docs/TODO.md` under
> "Port Constant Scattering". It is **not** done, and this runbook does not do it — this runbook
> documents the manual sequence.

---

### F8 — Join dies exactly like F1, but the advertise address was rewritten

**Symptom.** Identical to F1: AC/GC fine, join dead at `connecting-mc`, same two error strings.

**Discriminator.** This is the whole point of the scenario:

```bash
# --- WSL2 ---
echo "${DT_HOST_GATEWAY_IP:-<unset>}"
kubectl get configmap mc-0-config -n dark-tower \
  -o jsonpath='{.data.MC_WEBTRANSPORT_ADVERTISE_ADDRESS}'
```

If the variable prints anything, or the live value is a gateway IP rather than the IPv4 loopback
literal committed in `infra/services/mc-service/mc-0-configmap.yaml`, this is F8. `setup.sh` also
logs each rewrite as it happens (`Patching MC-0 advertise address: …`), so the setup output is a
second confirming signal.

**Cause.** `DT_HOST_GATEWAY_IP` was still exported from a previous devloop session. `setup.sh`
treats it as "you are building a devloop cluster" and patches all four MC/MH advertise addresses to
that IP — which the Windows browser cannot dial.

**Fix.**

```bash
# --- WSL2 ---
unset DT_HOST_GATEWAY_IP
./infra/kind/scripts/setup.sh
```

Re-running setup is not enough on its own if the export is still live in your shell.

**The F1/F8 distinction, stated once.** F1 is the advertise address being **correct** with the wrong
address *family* (IPv6 `::1` vs an IPv4-only listener). F8 is the advertise address being
**rewritten to a different host entirely**. Same dead join, opposite fixes.

**Why it recurs.** Any shell that has sourced a devloop environment carries the variable. It
survives as long as that shell does.

---

### F9 — Chrome cannot reach the app at all, but the preflight is green

**Symptom.** `dev-web.sh` reports everything fine and Vite is clearly running, but Chrome shows a
flat connection failure at `http://demo.localhost:5173`. Nothing loads — this is not a join
failure.

**Discriminator.** `curl -I http://127.0.0.1:5173` **from WSL2** succeeds while Chrome fails.
Use the IP literal, not `demo.localhost` — the name drags F6 into the test, so a reader with both
faults would see curl fail and wrongly conclude "not F9".
Preflight-green plus browser-refused is the signature: `dev-web.sh` runs inside WSL2, where the
bind is perfectly reachable, so it cannot see this class of problem at all.

**Cause.** WSL2 is in default **NAT** mode rather than mirrored. Vite binds localhost only (it sets
no `host`), and in NAT mode Windows cannot reach a WSL2 process bound to WSL2-localhost.

**Fix.** Set `networkingMode=mirrored` per §2.1, then `wsl --shutdown` from Windows.

The alternative — binding Vite to `0.0.0.0` and browsing the WSL2 IP — changes the origin and
breaks the `Host`-header story the AC auth proxy depends on. Mirrored mode is the supported path.

**Why it recurs.** `.wslconfig` lives on the Windows box and is easy to lose across machine
rebuilds; NAT is the WSL2 default, so this is the state you regress *to*.

---

### F10 — GC returns a CORS failure (only if you changed the config)

**Symptom.** GC calls fail with a CORS error in the DevTools console.

**Discriminator.** `echo $VITE_GC_BASE_URL` is non-empty. **On the default configuration this
cannot happen** — `gcBaseUrl` defaults to empty (same-origin) and the AC origin template resolves
to the page's own origin, so every API call is a same-origin relative path that Vite proxies
server-side. Same-origin requests are never preflighted, so the GC allowlist is inert on the
documented path.

**Cause.** Pointing `VITE_GC_BASE_URL` at GC directly (e.g. to bypass the proxy while debugging)
makes the calls genuinely cross-origin, and GC's allowlist is fail-closed by design.

**Fix.** Add the **page** origin — `http://demo.localhost:5173` — to
`infra/kubernetes/overlays/kind/services/gc-service/configmap-cors-patch.yaml`. This overlay patch
is the single edit point; the production base ships an empty allowlist and **stays** empty, and the
allowlist is never widened to `*`.

The common mistake is allowlisting the *target* you just typed rather than the page origin, then
concluding CORS is broken.

**Why it recurs.** Only if the env override is re-set. Unsetting it returns you to the
same-origin path.

---

### F11 — Vite crashes at launch with "cannot find native binding"

**Symptom.** `pnpm install` succeeds, but Vite (via `scripts/dev-web.sh` or
`pnpm nx run web-app:dev`) then dies at startup naming a missing native binding — e.g.
`Cannot find module '@rolldown/binding-linux-x64-gnu'` / "cannot find native binding". Nothing
starts, so sign-up / create / join never enter the picture. On a *fresh* attempt you may instead
see `pnpm install` itself refuse with `ERR_PNPM_UNSUPPORTED_ENGINE` — the same root cause caught
earlier (see the sub-cases).

**Discriminator — is this even F11?** The error names an **engines** violation or a **native
binding** load failure — not a *missing* binary (that is F2) and not a version that otherwise runs
(F3/F4). `scripts/dev-web.sh --check` prints a **✗ bundler probe** line naming
`@rolldown/binding-linux-x64-gnu`; that line is the F11 signature. Crucially this is the exact case
the script's Node check only **WARNs** on ("same major, likely fine") — F11 is where "same major"
is *not* fine, because the workspace floor is a *minor*, not merely a major. A reader whose Node is
simply on the wrong `nvm` alias is in F3, not here — confirm the binding/engines wording first.

Once F11 is confirmed, split the two sub-cases — they have different *minimal* fixes:

| Sub-case | Signal | Minimal fix |
|---|---|---|
| **(a)** Node is *below* the engines floor | `pnpm install` refused with `ERR_PNPM_UNSUPPORTED_ENGINE` (Wanted = the `engines.node` range, Got = your version) | **Upgrade Node** first, then install |
| **(b)** Node *satisfies* the floor, but `node_modules` is stale | `node --version` already meets the floor, yet the bundler probe still ✗ | **Reinstall only** — do *not* touch Node |

**Cause.** Vite 8 / rolldown load a native binding at import time. That binding
(`@rolldown/binding-linux-x64-gnu`) is an **optional** dependency whose own `engines` require Node
at or above the workspace floor — the floor value lives in the root `package.json` `engines.node`
and the lockfile and is **not restated here** (§2.4; a second copy would drift). Under a Node
*below* that floor, pnpm **silently skips** the engines-mismatched optional binding: the install
still succeeds and the gap only surfaces at Vite launch. `engine-strict=true` in the repo `.npmrc`
now turns sub-case (a) into a loud `pnpm install` failure. But a `node_modules` tree installed
*earlier* under a below-floor Node (sub-case b) keeps the gap until it is reinstalled: once pnpm has
recorded the optional binding as skipped, a plain `pnpm install` over the existing tree may not
re-evaluate/re-fetch it — **removing `node_modules` is what forces re-resolution** of the
now-satisfiable optional dep.

**Fix.** The always-safe superset — do this if unsure; it covers both sub-cases:

```bash
# --- WSL2 ---
nvm install "$(cat .nvmrc)"     # the pinned Node — satisfies the floor by construction
rm -rf node_modules             # forces re-resolution of the skipped optional binding
pnpm install
```

For sub-case (a) the `nvm install` is the load-bearing step; for sub-case (b) the
`rm -rf node_modules && pnpm install` pair is — never a *bare* `pnpm install` for (b), for the
re-resolution reason in **Cause**. The superset is harmless either way; prefer it unless you have a
reason to minimise. Re-run `scripts/dev-web.sh --check` — the bundler probe should go green.

**Why it recurs.** Whenever the running Node drifts below the workspace floor: a new machine, a
reset `nvm alias default` (see F3), or a `node_modules` carried across a floor-raising dependency
bump — the vite 8 / rolldown 1.2.1 bump on 2026-08-05 is what first exposed this. The pins are
single-sourced (`.nvmrc`, root `package.json` engines) precisely so the fix is "match the repo,"
not "guess a version"; a tracked drift-guard (`docs/TODO.md`, §Developer Experience) will fail
validation if `.nvmrc`, the devloop-image Node pin, the lockfile floor, and root `engines.node`
ever disagree.

---

## 6. Teardown

```bash
# --- WSL2 ---
./infra/kind/scripts/teardown.sh
```

Deletes the Kind cluster and cleans up orphaned port-forward processes.

**Then remove the hosts entry.** §2.3 deliberately made it survive reboots, so it outlives the
cluster and will silently shadow `demo.localhost` later:

```bash
# --- WSL2 ---
sudo sed -i '/[[:space:]]demo\.localhost$/d' /etc/hosts
getent hosts demo.localhost    # expect no output
```

Generated certs under `infra/docker/certs/` are gitignored and harmless to leave, but they hold
private keys — delete the directory if you are done with the branch.

---

## 6.5 Automated checks that exist today

Once §3 Step 1 has completed, the Rust env-tests are runnable with **no further setup** —
`setup.sh` starts port-forwards at exactly the ports the suite defaults to, and seeds both the
`devtest` org the tests use and the `demo` org the browser demo uses.

```bash
# --- WSL2 ---
cargo test -p env-tests --features smoke          # fast cluster health (~30s)
cargo test -p env-tests --features smoke,flows    # + service flows, incl. the AC→GC→MC join
cargo test -p env-tests --features all            # everything (~8-10 min)
```

> **Bare `cargo test` runs ZERO env-tests.** The crate has no default features. A reader who runs
> bare `cargo test`, sees green, and concludes the cluster is validated has validated nothing.

`crates/env-tests/tests/24_join_flow.rs` is a real end-to-end join across AC, GC and MC;
`26_mh_quic.rs` covers the MH QUIC path. This is the same Rust suite the devloop's Layer 7 gate runs
(`scripts/layer7.sh`, `--features all`); failure-mode triage lives in
`docs/runbooks/devloop-validation.md` §6.7. Layer 7 **also** runs a browser E2E suite (Playwright)
after the Rust env-tests — diff-triggered and gated on its own preconditions; it is described just
below.

**Two splits that will confuse you if nobody names them.** Both are §1's mechanism applied one
layer down:

- **Access path.** env-tests reach AC and GC through `kubectl port-forward` on the *pod* ports;
  the browser reaches them through Kind `extraPortMappings` on the *host* ports. Same services,
  two paths, different failure modes — **port-forwards die silently when a pod restarts, NodePort
  mappings don't.** If the suite starts failing to connect after a pod bounce, re-run `setup.sh` or
  restart the forwards.
- **Organization.** env-tests register against `devtest`; the browser demo uses `demo`. Both are
  seeded. A suite using a different org than your browser session is expected, not broken.

The web-app's own tests, both through Nx so proto codegen runs first (F5):

```bash
# --- WSL2 ---
pnpm nx run web-app:test:component   # Vitest 4 browser mode (Chromium) component tests
pnpm nx run web-app:test:unit        # Node-tier prod-bundle-content assertion (R-14)
```

`test:unit` is not a general unit suite — it asserts that dev-only strings are absent from a
production build, and does not exercise the join path at all.

The **browser E2E suite** (the Playwright harness from story tasks #18/#19) runs through Playwright,
not Nx, and unlike the two Vitest tiers above it needs the cluster up *and* the dev-cert
fingerprints present (§3):

```bash
# --- WSL2 ---
pnpm --filter @darktower/web-app test:e2e
```

`packages/web-app/playwright.config.ts` + `packages/web-app/e2e/` (specs, `global-setup.ts`,
fixtures) own this; its `global-setup.ts` reads the dev-cert fingerprints from
`infra/docker/certs/fingerprints.json` (the `fingerprints.env` beside it is the shell-sourceable
form of the same MC/MH pair — one writer, `scripts/generate-dev-certs.sh`). This is the same lane
`scripts/layer7.sh` runs as a **diff-triggered** step *after* the Rust env-tests and only if they
pass; it is gated on `fingerprints.json` and a Playwright Chromium being present (missing either is a
Layer-7 `PRECONDITION_FAILURE`, not a silent skip). `packages/web-app/e2e/README.md` owns the
specifics.

---

## 7. Not on this branch

- **Browser E2E now EXISTS on this branch** — an explicit correction, because this section
  previously said it did not. Story tasks #18/#19 landed `packages/web-app/playwright.config.ts` and
  the `packages/web-app/e2e/` specs + `global-setup.ts`, and `scripts/layer7.sh` runs them as a
  diff-triggered lane after the Rust env-tests. `infra/docker/certs/fingerprints.json` now has a
  **second** consumer — the Playwright `global-setup.ts`, alongside the pre-existing Vite config
  (`packages/web-app/vite/fingerprints.ts`) — while the sourceable `fingerprints.env` remains the
  shell form with **no code consumer** (the prior §7 note's "fingerprints.env … has no consumer"
  was, and stays, true for `.env`). See §6.5 for how to run it; kept here as a correction so the
  prior "no Playwright" note is not trusted.
- **No media plane.** MH connections perform the auth handshake and nothing more: no SFrame, no
  WebCodecs, no datagrams. See the silence note in §4.3.
- **No browser-side join logging or client metrics by default** — see §4.0 rung 1.
- **The devloop-container cluster** is a different topology and is not what this runbook
  documents — see §1.
- **Anything production.** See the banner.

---

## Related runbooks and docs

Each of these **owns** the content named, so it is cross-linked rather than copied. If you find
yourself updating the same fact in two of these files, one of them is wrong.

- `docs/LOCAL_DEVELOPMENT.md` — owns cluster prerequisites, the WSL2 inotify/keyring `sysctl`
  limits, generic cluster troubleshooting, and the seeded dev credential values.
- `packages/web-app/README.md` — owns the client-side quick start, the `VITE_*` configuration
  table, transport/scheme semantics, and the E2E contract surface (`data-testid` set and the
  `window.__darktower_test__` replay bus) that tasks #18/#19 consume.
- `docs/runbooks/devloop-validation.md` — owns validation-pipeline failure triage (§6.7 for
  Layer 7 / env-tests) and the **cite convention** this runbook follows (§10).
- `docs/runbooks/gc-deployment.md` — owns GC smoke tests (§ "Smoke Tests") and GC deployment
  verification.
- `docs/runbooks/mc-incident-response.md`, `mh-incident-response.md` — own the MC/MH QUIC incident
  scenarios that the dev-time failures here mirror.
- `scripts/dev-web.sh` — owns the executable preflight check list. This runbook owns the *why* and
  the diagnosis; the script owns *what is checked*.

---

## Changelog

| Date | Author | Changes |
|------|--------|---------|
| 2026-07-29 | operations (task #20) | Initial creation (R-49). Two-machine topology, two-topology cluster split, bring-up, join-verification ladder, F1–F10, teardown, env-tests section. Deliberately diverges from `TEMPLATE.md` — see the banner. |
| 2026-08-06 | infrastructure (task #61) | Added **F11** (Vite "cannot find native binding" — engines-skipped optional binding under a below-floor Node) with (a)/(b) sub-case split; updated §5 header + ToC. Reconciled §6.5/§7 with reality: the Playwright browser-E2E lane (tasks #18/#19) now exists and runs diff-triggered in Layer 7 — corrected the stale "no Playwright" §7 note and the §6.5 cross-reference. Cross-boundary edit into this operations-owned runbook, confirmed by operations at Gate 1/Gate 3. |
