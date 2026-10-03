# Runbook: Local Browser-Client Dev Environment

**Service(s)**: web-app demo + AC / GC / MC / MH (local Kind)
**Owner**: operations
**History**: `git log --follow -- docs/runbooks/client-dev-local.md`
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
- [Secure Context and Media Setup](#secure-context-and-media-setup)
- [§3 Bring-up](#3-bring-up)
- [§4 Is the join real?](#4-is-the-join-real)
  - [§4.5 "I joined and I hear nothing" — media triage ladder](#45-i-joined-and-i-hear-nothing--media-triage-ladder)
- [§5 Failure modes](#5-failure-modes)
- [§6 Teardown](#6-teardown)
- [§6.5 Automated checks that exist today](#65-automated-checks-that-exist-today)
- [§7 Not on this branch](#7-not-on-this-branch)
- [Related runbooks and docs](#related-runbooks-and-docs)

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
| Brought up by | `./infra/kind/scripts/setup.sh` | `dev-cluster provision && dev-cluster deploy` (`infra/devloop/dev-cluster`) |
| Config | `infra/kind/kind-config.yaml` (committed) | rendered from `infra/kind/kind-config.yaml.tmpl` |
| Host ports | the fixed `hostPort` values in that file | allocated per-clone, 20000–29999 |
| Published on | `127.0.0.1` (loopback, per R-37) | the podman host-gateway IP |
| MC/MH advertise | the committed IPv4 literals in the per-instance `*-config.env` files | rendered to the gateway IP at deploy time (`infra/kind/scripts/deploy.sh:apply_env_root`) |

**Use the static path.** `scripts/dev-web.sh` defaults to the static host ports, and reads the
MC/MH advertise addresses from the **committed configmap files on disk** — both of which are only
true under the static topology.

`infra/devloop/dev-cluster` is not an alternative you can choose: it is a client that talks to the
devloop helper over a unix socket at `/tmp/devloop/helper.sock`, which exists only inside a running
devloop container. From a plain WSL2 shell it will fail at the socket check.

> **You will see `dev-cluster provision` / `dev-cluster deploy` elsewhere in the repo and they are
> correct there.** `scripts/layer7.sh`, `infra/devloop/devloop.sh` and ADR-0030 use them correctly.
> The rule is not the string, it is **the audience**: a human on the WSL2 host gets
> `./infra/kind/scripts/setup.sh` (which runs provision.sh, then deploy.sh, then the host
> port-forwards); the devloop container gets `dev-cluster provision && dev-cluster deploy`.

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

   **1a. WebCrypto Ed25519 must be available unflagged** — ADR-0036 §3 signs every media frame,
   and the SDK sends and accepts NO unsigned frames under any degradation. If it is unavailable,
   `packages/sdk-core/src/media/frame/ed25519.ts` throws a named `SdkError` at the capability
   probe rather than degrading, and any failure to obtain or run a verifier maps to a DROPPED
   frame, never to an accepted one.

   **Three results, stated separately because two of them are what constrain the code.** All three
   were **executed against Google Chrome for Testing 151.0.7922.34 on Linux/WSL2** (the
   Playwright-pinned Chromium at `/opt/ms-playwright/chromium-1234`), **NOT on Windows Chrome** —
   this environment cannot run it. The engine-version claim is empirical; the Windows-specific
   claim below is documentary.

   | # | Probe | Result | What it constrains |
   |---|---|---|---|
   | 1 | `crypto.subtle.generateKey({name:'Ed25519'})`, PKCS#8 seed import, raw public-key import, sign, verify | **All succeed, unflagged.** The pinned test seed reproduces its public key and signature byte for byte | No `@noble/ed25519` fallback ships. A fallback branch could never execute in any gate we run while still counting toward the coverage threshold, and it would put the private scalar in raw JS memory against ADR-0028 §5 |
   | 2 | `crypto.subtle.importKey('raw', new Uint8Array(16), 'AES-GCM', …)` | **SUCCEEDS** — a 16-byte AES key is accepted | There is **no platform backstop** against AES-128. The refusal is ours, on seal, open *and* KEK unwrap (`sframe.ts::rejectWrongAesKeyLength`) |
   | 3 | `crypto.subtle.importKey('raw', new Uint8Array(0), {name:'HMAC',…})` | **REFUSED** — `DataError: HMAC key data must not be empty` | RFC 9605's empty HKDF salt must be supplied as 64 ZERO BYTES. That is exact, not a workaround: HMAC zero-pads a short key to the hash block size, so an empty key and a HashLen-zero key are the same key by construction |

   **Documentary, for the Windows half**: WebCrypto Ed25519 is a Blink/BoringSSL feature with no
   platform-specific gate, and Chrome has shipped it unflagged since **Chrome 137**. The probe above
   ran on Chrome 151. This is *not* written as "verified on Windows", because it was not.

   **If you repeat this probe, serve the page over `http://127.0.0.1` or `https://`.** On a `data:`
   URL the origin is opaque, so the page is not a secure context and `crypto.subtle` is
   **`undefined`** — which reads exactly like "Ed25519 is missing". The first attempt at this probe
   failed that way.

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

## Secure Context and Media Setup

**Unnumbered deliberately, and the spelling is frozen.** `scripts/dev-web.sh`'s header points at
`#secure-context-and-media-setup`, `scripts/dev-web.test.sh` pins both the slug and several claims
in the prose below, and §4's triage ladder links here for the facts. Renaming this heading, or
adding punctuation to it, breaks all three; numbering it would break it twice over, since the number
would then move whenever a section is inserted. No subheading under it contains the words "secure"
and "context" together, because the guard finds this section by the FIRST such heading in the file
and would otherwise slugify the wrong one.

### The whole media pipeline is gated, all or nothing

`getUserMedia`, **WebCodecs**, **WebCrypto** and **WebTransport** are all gated on a
[secure context](https://w3c.github.io/webappsec-secure-contexts/). The demo needs every one of
them — microphone in, Opus out, SFrame encryption and Ed25519 signing, and the QUIC datagrams that
carry the result — so on a non-secure origin the media path is not degraded, it is **absent**.

**Absent, not erroring**, and that distinction is the whole reason this section exists:

- `navigator.mediaDevices` is `undefined` — so there is no permission prompt to deny, and no
  "microphone blocked" indicator. Nothing appears to happen.
- `crypto.subtle` is `undefined`.
- The `AudioEncoder` / `AudioDecoder` and `WebTransport` constructors are not defined.

Sign-up and create-meeting keep working, because those are HTTP through the Vite proxy and need
none of the four.

**The join does NOT.** MC signalling runs over WebTransport and has no fallback
(`SignalingClient` holds a single `WebTransportConnectFn`), so the client dies at
**`connecting-mc`** and never reaches `joined`. No roster ever appears.

**And the screen tells you nothing about the origin.** `BrowserWebTransport` throws
`WebTransport is not available in this environment` — but that string is attached as
`Error.cause`, which the demo deliberately never renders (R-23 redaction: `SdkError.toJSON()` is a
fixed allowlist and `cause` is not on it). What `data-testid="last-error"` actually shows is the
generic:

```
SIGNALING: Signaling transport closed
```

which is **byte-identical to what F1, F8 and F9 produce**. There is no distinguishing text anywhere
on screen, and no console message naming the origin. So this failure does not merely *resemble* an
MC-reachability problem — at rung 1 it is indistinguishable from one, and a developer who works §4's
ladder will find nothing wrong at every rung. That dead end is the moment someone starts looking for
a browser switch, which is exactly what this section exists to prevent.

**The discriminator is the origin in your address bar, and it costs nothing.** Check it before rung
1: if the host is not a `.localhost` name or a loopback literal, stop — the ladder has nothing to
find. Note *why* that is the discriminator: not because the error is helpful, but because it is
useless.

To confirm rather than infer, expand the error's `cause` chain in devtools — the underlying
`WebTransport is not available in this environment` is there. It is **only** there; nothing in the
UI shows it.

### `http://<org-subdomain>.localhost:5173` IS potentially trustworthy

Plain HTTP, and correct. Chrome treats an origin as *potentially trustworthy* when its host is a
loopback literal (`127.0.0.1`, `[::1]`) **or a `.localhost` name** — `.localhost` is reserved for
loopback by RFC 6761, and Chrome implements the subdomain case, so `demo.localhost` qualifies just
as `localhost` does. This is a Chrome statement, not a cross-browser one; the demo is Chrome-only
(ADR-0028) and other engines are not promised here.

That is why the whole demo runs over `http://` with no TLS anywhere in front of Vite, and it is why
the `demo.` prefix costs you nothing: the subdomain is needed for the `Host` header (§3 Step 4) and
it stays potentially trustworthy.

### What breaks it: a non-loopback HTTP origin

Reaching the same dev server at `http://<lan-ip>:5173`, or by machine name from another host, is
**not** potentially trustworthy. All four APIs vanish at once, so the join fails at `connecting-mc`
exactly as a cert or reachability problem would — and nothing on screen names the origin as the
cause.

The fix is the origin, and there are exactly two approved forms:

- use a `.localhost` name or a loopback literal — on Windows/WSL2 this is what §2.1's hosts entry is
  for; or
- terminate real TLS in front of the dev server.

**Do not reach for a browser flag.** Chrome has settings that force an insecure origin to be treated
as trustworthy, and they are prohibited here — in scripts, config, tests and runbooks alike, checked
by `dt-guard no-insecure-browser-flags`. They do not just relax the origin check: MC and MH trust in
dev flows exclusively through `serverCertificateHashes` pinning, and the flags people pair with this
one disable exactly that. The result is a demo that appears to work while the property the pinning
exists to prove is gone. If the origin is not a secure context, change the origin.

### No microphone on this machine? Launch Chrome with synthesized capture

A laptop with no input device, a VM, or a shared demo box can still take part in a call. Chrome
can synthesize a capture stream and auto-accept the permission prompt:

```
--use-fake-device-for-media-stream --use-fake-ui-for-media-stream
```

The OTHER participants will hear a steady tone rather than your voice. You never hear yourself —
loopback is removed (story 2 R-3; a solo participant correctly hears silence, with every slot
reading "no one to fill this slot yet"). To run several participants on one machine, prefer the
test-tone build (`DT_TEST_TONE=1 scripts/dev-web.sh`), which gives each participant its own
frequency so "I hear A and C but not B" is audible, and avoids one microphone feeding N tabs.

These are the same two flags `packages/web-app/playwright.config.ts` launches with, which is where
the browser E2E suite gets its deterministic audio; keep the two in step. They are **not** security
bypasses — they inject a device and answer a prompt, and change no trust decision — which is why
they are permitted where the flags in the previous subsection are not.

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

- **`✗` HARD FAIL** — the demo will not work, so the script stops and does **not** start the dev
  server. Every hard fail prints the next command to run. Fix the `✗` items and re-run.
- **`!` WARN** — a convenience or a side path is degraded. The script names which, then starts the
  server anyway.

Which check carries which severity is maintained in the script and printed by
`scripts/dev-web.sh --help` — as with the check list above, it is deliberately not re-enumerated
here.

**The WebTransport checks are hard fails, and that is a recent change.** The cert-fingerprint check
and the MC/MH listener checks used to warn. That was correct when the demo's success criterion was
sign-up and create-meeting — both run over TCP through the Vite proxy and are unaffected by any
WebTransport problem. It is no longer the criterion: the demo is now *N+1 participants hearing each
other through MH* (`docs/decisions/adr-0036-media-flow.md`), so MC and MH reachability is the
demonstrated path, not a side path. Warning there produced the worst available outcome — a demo that starts,
appears to join, and silently returns no audio. See §4 for telling a real join from an optimistic
one.

**One limitation to know — it now blocks rather than misleads.**
`scripts/dev-web.sh::check_wt_endpoint` reads the MC/MH advertise addresses from the committed
config files (`*-config.env`) **on disk**, not from the live cluster. On the static topology those
agree. On a devloop cluster the live ConfigMap is rendered with the gateway address and the on-disk
file holds the host default, so the check
validates an address nobody is using.

Before the escalation that produced a misleading green. Now the hazard runs both ways: **a red
preflight on a devloop cluster does not necessarily mean the join is broken — and it stops the dev
server from starting.** That is expected on the devloop topology (§1, F8), not a regression. Check
the live value before believing the failure (substitute the pod the preflight named):

```bash
# --- WSL2 ---
# ConfigMap names are content-hash-suffixed (ADR-0038); resolve the one the pod uses.
kubectl get configmap -n dark-tower -o jsonpath='{.data.MH_WEBTRANSPORT_ADVERTISE_ADDRESS}' \
  "$(kubectl get deployment/mh-0 -n dark-tower \
     -o jsonpath='{.spec.template.spec.containers[?(@.name=="mh-service")].env[?(@.name=="MH_WEBTRANSPORT_ADVERTISE_ADDRESS")].valueFrom.configMapKeyRef.name}')"
```

If that disagrees with the on-disk file, the preflight is reporting stale data and the cluster may
be healthy. `dev-web.sh` targets the static host topology (§1); a devloop cluster is not its
intended input, and the validation path there is the Rust env-tests (§6.5).

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
| Seeded the `demo` org (`infra/kind/scripts/deploy.sh::seed_demo_org`, idempotent) | `kubectl exec -n dark-tower postgres-0 -- psql -U darktower -d dark_tower -c "select subdomain from organizations"` |
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

> **Expect little on this port, and do not diagnose it as a fault.** This capture watches **MC's**
> port, which carries signalling only: a healthy join shows the QUIC handshake, the join exchange,
> then sparse traffic — MC writes when something structural changes (a join, leave, mute or KEK
> rotation), not continuously. **Audio never crosses this port**: media datagrams go to the MH
> ports (the MH entries in `infra/kind/kind-config.yaml`), and a participant nobody holds sends
> none at all (F12). A reader who watches this port go quiet and concludes media is broken has
> misread which hop they are watching.
>
> *Recorded correction (story 2 task 17): this note previously said "there is no media plane on
> this branch at all" — true when §4 was written, false since the media stories landed. A reader
> trusting it would have dismissed a media fault as expected silence.*

`tcpdump` proves packets reached the host. It does not prove media flows — for that, work the
media ladder in §4.5.

### 4.4 Roster check

Open a second Chrome tab or profile at `http://demo.localhost:5173` and join the same code. The
first tab's participant list should gain the new participant. This exercises the MC→client
notification path, which none of the signals above cover.

---

### 4.5 "I joined and I hear nothing" — media triage ladder

**This is the section the media stories exist for.** Localising a silent call is the whole point:
audio passes through **capture → encrypt → sign → uplink → MH forward → downlink → decrypt →
verify → playback**, and the hazard is that *every signal reads green while no audio arrives*. §4.2's
question is the right one here — for each signal, finish *"this proves ___"* without using
*working*, *fine*, or *healthy*.

**Turn the counters on first.** `dt_client_*` metrics are off unless `VITE_TELEMETRY_ENDPOINT` is
set. With it set they export to the GC telemetry proxy, **never to the console** — see the note at
the end of this section about where they can and cannot be read.

#### The ladder, cheapest signal first

| Rung | Read | Localises to |
|---|---|---|
| 1 | Is `dt_client_media_frames_sent_total` moving? | **capture / mute / nobody holds you** vs everything downstream — flat is CORRECT for a participant nobody holds, a solo participant always (F12). *Moving/flat is the whole read:* the counter is **per datagram**, one per target handler, so a sender directed at two handlers posts ~2x its capture rate — never compare its rate against a capture rate here. |
| 2 | Is `dt_client_media_frames_received_total` moving? | **delivery** (another sender → MH → this client) vs receive-side processing. A solo participant receives nothing by design (R-3). |
| 3 | Is `dt_client_media_frames_accepted_total` moving? | **crypto/parse** vs **playback** |
| 4 | `dt_client_media_frames_dropped_total{reason}` | the exact receive step that rejected, **by name** |
| 5 | `dt_client_media_send_dropped_total{reason}`, `dt_client_media_send_queue_depth` | **uplink back-pressure** |
| 6 | Server-side counters (`mh_media_*`, `mc_media_*`) | whose service to open |
| 7 | Packet capture | last rung; **credential-bearing**, see below |

Most silent calls resolve at rung 1, 3 or 4. Reaching for a packet capture first is the mistake §4.0
already warns about, and it is worse here because the artifact carries a JWT.

#### What each green signal actually proves

| Signal | This proves… | It does **NOT** prove |
|---|---|---|
| `dt_client_media_mute_transitions_total{action}` moved to `unmute` | the app **believes** it is unmuted | that the capture device produced a sample. Client mute is enforced at capture, so a stuck mute produces silence with no error anywhere. |
| `dt_client_media_frames_sent_total` rising | datagrams were encrypted, signed, and left the device — one per target handler, so a two-handler sender posts two per captured frame; this is not a capture rate | that MH accepted or forwarded any of them |
| `dt_client_media_send_dropped_total{reason}` flat | the SDK's bounded egress queue is not shedding | that the frames reached the network. `transport_send_refused` and `not_connected` are **fleet contracts that read zero forever** — any non-zero is a bug, not a load condition. |
| `dt_client_media_send_queue_depth` low | no application-layer back-pressure right now | anything about the transport queue beneath it |
| `dt_client_media_frames_received_total` rising | **datagrams arrived at the wire.** Counted before any parse, verification or decryption — deliberately, so *"nothing is arriving"* is distinguishable from *"arriving and failing to open"* | that a single one was openable, let alone audible |
| `dt_client_media_frames_dropped_total{reason}` flat **and** `received` flat | nothing to reject, because nothing arrived | that the uplink worked |
| `dt_client_media_frames_accepted_total` rising | frames were verified, replay-checked, decrypted and **handed to the audio decoder** | **that anything was played.** A frame handed to a decoder is not a frame that was heard. |
| `dt_client_media_decoder_errors_total` flat | the decoder did not report an error | that the output device is live, or that the `AudioContext` is running |

**The accounting identity that makes rungs 2–4 trustworthy:**

> **`received = accepted + sum(drops by reason)`**

It holds at the crypto/parse boundary **by construction** — every datagram leaves the receive path
through exactly one of those two channels. **It does not hold at playback.** A frame lost between
decoder handoff and audible decrements nothing on the right-hand side, which is precisely why
`accepted` is named `accepted` and not `played`.

#### Reading the fork at rung 2

| Rung 1 | Rung 2 | Meaning |
|---|---|---|
| `sent` **flat** | — | Nobody holds you (a solo participant always — **correct, stop**), or capture or mute-release never resumed. **Not a transport problem.** Go to F12. |
| `sent` rising | `received` **flat** | Either nobody is being sent to you (every peer muted, or no source for your slots — **correct**) or a source MC says is active is not arriving. F13 splits these on `dt_client_media_receive_source_deficit_total` first. Go to F13. |
| `sent` rising | `received` rising, `accepted` **flat** | Arriving and failing to open. Read rung 4's `reason` label — it names the step. Go to F15 if the reason is key material. |
| `sent` rising | `accepted` rising, still silent | The fault is **downstream of decoder handoff**. Go to F14. |

#### Rung 4 — the `reason` label is the diagnosis

`dt_client_media_frames_dropped_total{reason}` carries the frozen frame-reject vocabulary, and the
tokens are emitted **individually** rather than collapsed. Grouped by what they tell you:

| Reason group | Tokens | Means |
|---|---|---|
| **Key material** | `no_kek_for_generation`, `kek_generation_stale`, `no_roster_entry` | Expected transiently at join and after a KEK rotation. **Sustained is the signal.** For why a KEK was refused or retained as it was, read `dt_client_media_kek_install_refusals_total{outcome}` and `dt_client_media_kek_retention_anomalies_total{outcome}`. → F15 |
| **Crypto** | `unwrap_failed`, `decrypt_failed`, `signature_invalid` | `unwrap_failed` is the KEK unwrap (key **distribution**); `decrypt_failed` is the SFrame payload (key schedule or sender). Two AES-GCM failures on one path routing to opposite owners. |
| **Structural / codec** | `unknown_version`, `reserved_flag_bit_set`, `payload_length_exceeds_max`, `payload_length_exceeds_available`, `truncated`, `extensions_too_large`, `extensions_malformed`, `trailing_bytes` | A wire-format disagreement. `unknown_version` specifically is the version-skew tell — see `mh-deployment.md` §Rollout With Media Flowing. |
| **Protocol violation** | `no_transmit_key` | Neither a cached key nor a usable wrap. Not a third key reason. |
| **Replay** | `replay_detected` | The replay window rejected it. |

#### Rung 6 — server-side, and which service to open

```promql
# Is MH forwarding at all?
sum(rate(mh_media_frames_forwarded_total{direction="egress"}[5m]))

# Is MH shedding on its egress queue bound?  `connection_closed` is routine.
# `no_subscriber` is routine only as a SHORT tail after a sender's last holder
# leaves; SUSTAINED means a client is sending against an empty edge set: MC's
# directive and MH's policy disagree, OR a client is ignoring its directive --
# see MH Scenario 17 for the discriminator.
sum by(reason) (rate(mh_media_frames_dropped_total{direction="egress"}[5m]))

# Does MC agree about the policy MH applied?  After an MC restart this split can
# read clean BY DESIGN: the restart arm is recorded on the adoption counter below
# (and a WARN "adopting it as a floor" at mc.register_meeting.trigger), never
# under an outcome label.
sum by(outcome) (increase(mc_media_policy_pushes_total[15m]))
sum by(outcome) (increase(mc_media_policy_generation_adoptions_total[15m]))
```

**Aggregation floor is pod or service.** There is no `by(meeting_id)`, no `by(participant)` and no
`by(sender)` on any media-path metric, anywhere — by design, not by omission. A per-stream,
time-ordered series of frame sizes is a voice-activity trace, so the dimension does not exist to be
grouped by.

**And for the same reason: do not raise a log level to diagnose this.** The incident that motivates
the level change is the incident that produces the sensitive trace. There is no debug-logging rung
in this ladder and there will not be one.

#### Where these counters can actually be read

**`dt_client_*` metrics DO reach Prometheus as of 2026-09-23.** This section previously said they
did not — that was true then and is the reason the export path was built. Query them in Prometheus
or Grafana normally. Three things about their shape will otherwise cost you an afternoon:

- **Only the 14 media metrics are exported.** The five join-flow metrics
  (`dt_client_join_attempts_total` and friends) are deliberately NOT exported, because they carry
  `meeting_id_hash` and ADR-0036 §11 grandfathers that label *as emitted*, not *as stored in central
  Prometheus*. So an empty query for a join-flow metric is correct behaviour, not a broken client.
  The exported set is the `Exported:` markers in `docs/observability/metrics/client.md`.
- **`instance` on a client series names the COLLECTOR POD, not a browser.** `honor_labels` is left
  at its default (false), so the scrape's own `job`/`instance` win and the SDK's identity lands
  under `exported_job="darktower-sdk-core"`. There is no `exported_instance` and there cannot be —
  a per-session `service.instance.id` is barred as a participant dimension (§11). **Grouping a
  `dt_client_*` series by `instance` groups by collector**, and every browser collapses into one
  series per `{client_version, org_id, key_custody}`. That is deliberate: it is what lets the
  collector sum across browsers without a per-participant label.
- **An empty result no longer means one thing.** Five controls on the collector's metrics path drop
  client data by design — the name allowlist, the temporality gate (a stale browser bundle emitting
  cumulative), `keep_keys`, the stream cap, and `memory_limiter` under pressure — and all five look
  identical to "no browser is running". Ladder for telling them apart:
  `gc-deployment.md` §When the client series go quiet. Start with
  `up{job="otel-collector"}`.

The collector log is still a useful first rung for "emitted at all": `kubectl logs -n dark-tower
-l app=otel-collector` shows the `debug` exporter's received-metric names and counts, **before** any
of the five drops above are applied — so a name visible there but absent from Prometheus localises
the problem to the collector's filtering rather than to the client.

Server-side counters (`mh_media_*`, `mc_media_*`) reach Prometheus normally; rung 6 works.

**Related**: MC's key-delivery remedies live in
[`mc-incident-response.md` Scenario 16: Missing Key Material](mc-incident-response.md#scenario-16-missing-key-material);
the datagram-drop and keepalive story lives in
[`mh-incident-response.md` Scenario 17: Media Datagram Drop](mh-incident-response.md#scenario-17-media-datagram-drop).
**Both are pointed at, not restated here** — thresholds and remedies are owned in one place.

---

## 5. Failure modes

Each scenario gives a **discriminator** — something you can run to tell it apart from the
scenarios that share its symptom. F1/F8 and F1/F9 are symptom-identical pairs; without a
discriminator you would be guessing.

> **Maintainers — keep these anchors stable.** This heading deliberately carries no F-count, so
> `#5-failure-modes` does not change as entries are added (rule and rationale:
> `docs/observability/metrics/mc-service.md` §`mc_media_sender_binding_responses_total`).
> F-entry headings are linked from other docs by slug and by heading title, so do not retitle an
> `### F<n> — …` heading without updating every referrer (`grep -rn 'client-dev-local.md' docs crates scripts packages .claude`).

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
default in `infra/services/mc-service/mc-0-config.env` (key
`MC_WEBTRANSPORT_ADVERTISE_ADDRESS`) and its `mc-1` / `mh-0` / `mh-1` siblings. If a live ConfigMap
disagrees with the committed file, it was rendered with a gateway address — go to F8.

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

**Symptom.** Sign-up and create work; join fails at the media or signaling step, or the join looks
clean and no audio comes back.

**`dev-web.sh` HARD FAILS on a missing `infra/docker/certs/fingerprints.json`** and does not start
the dev server — it does not exist in a fresh clone, since the whole directory is generated and
gitignored. (It used to warn. It cannot any more: the demonstrated path is now audio through MH, so
a warning there produced a demo that started, appeared to join, and returned nothing.)

**But the preflight being green does not mean the fingerprints are RIGHT.** The check is
`[[ -s "$FINGERPRINTS_JSON" ]]` — presence only. A **stale-but-present** file passes it, and the
browser still refuses the handshake. That asymmetry is the whole reason F7 exists as an entry
separate from F1/F8: the cheap check cannot see this failure, so the discriminator below is the
only thing that can.

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

**Fix — two steps, in this order.** Regenerating certs alone is not enough, and neither is
restarting Vite. This is the sequence `scripts/generate-dev-certs.sh` itself prints when a leaf is
near expiry:

```bash
# --- WSL2 ---
./infra/kind/scripts/setup.sh
#   provision renews an expiring leaf and hashes the PUBLIC certs into its blueprint (ADR-0038),
#   so a renewed leaf REBUILDS the cluster: fresh mc/mh-service-tls Secrets, fresh MC/MH pods
#   (no rollout-restart step); deploy then converges the application onto it
# then restart the dev server:
scripts/dev-web.sh
#   the browser-side fingerprints are read at Vite CONFIG time — a running server never re-reads them
```

**Not a browser setting.** The remedy is the sequence above. A browser setting that turns
off certificate validation is never the fix here: MC/MH trust in dev flows entirely through
`serverCertificateHashes` pinning, so disabling validation does not restore the media path — it
leaves a demo that appears to work while the property the pinning exists to prove is gone. This is
the branch where that temptation actually bites, because the preflight is GREEN (it checks presence,
not freshness) and `dev-web.sh`'s own warning never printed.

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

> Since ADR-0038 step 3 no manual rollout-restart is needed: a renewed leaf changes provision's
> blueprint, so the next `setup.sh` (host) or `dev-cluster provision` (devloop) rebuilds the
> cluster with the new Secrets. Expect that rebuild (~6 min) roughly every 13 days on a
> long-lived cluster.

---

### F8 — Join dies exactly like F1, but the advertise address was rewritten

**Symptom.** Identical to F1: AC/GC fine, join dead at `connecting-mc`, same two error strings.

**Discriminator.** This is the whole point of the scenario:

```bash
# --- WSL2 ---
echo "${DT_HOST_GATEWAY_IP:-<unset>}"
kubectl get configmap -n dark-tower -o jsonpath='{.data.MC_WEBTRANSPORT_ADVERTISE_ADDRESS}' \
  "$(kubectl get deployment/mc-0 -n dark-tower \
     -o jsonpath='{.spec.template.spec.containers[?(@.name=="mc-service")].env[?(@.name=="MC_WEBTRANSPORT_ADVERTISE_ADDRESS")].valueFrom.configMapKeyRef.name}')"
```

If the variable prints anything, or the live value is a gateway IP rather than the IPv4 loopback
literal committed in `infra/services/mc-service/mc-0-config.env`, this is F8. `deploy.sh` also
logs it when it applies (`Applying environment root … (devloop advertise addresses via <ip>)`), so
the setup output is a second confirming signal.

**Cause.** `DT_HOST_GATEWAY_IP` was still exported from a previous devloop session. `deploy.sh`
(via `setup.sh`) treats it as "you are building a devloop cluster" and renders all four MC/MH advertise addresses
with that IP — which the Windows browser cannot dial.

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
`@rolldown/binding-linux-x64-gnu`; that line is the F11 signature. The script's Node check
now **fails** a Node outside `package.json` `engines.node` (the range `pnpm install` enforces), so
a below-floor Node is caught at preflight; F11 remains for a `node_modules` installed under an
*earlier* below-floor Node, which a correct Node now cannot repair without a reinstall. A reader whose Node is
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


### F12 — Joined, unmuted, and `dt_client_media_frames_sent_total` is flat

**Symptom.** The meeting is joined, the UI shows unmuted, and no frames are leaving the device.

**Discriminator.** `dt_client_media_frames_sent_total` flat **while**
`dt_client_media_mute_transitions_total{action="unmute"}` has incremented. That pair separates this
from every downstream fault: nothing left, so nothing downstream can be at fault. (The converse does
not hold: the counter is per datagram, one per target handler, so non-flat does not by itself mean
frames were captured at the expected rate.)

**First, rule out the healthy case: nobody holds you.** Since story 2 removed loopback (R-3), a
participant that no other participant holds in a receive slot — **a solo participant always** — is
sent an EMPTY target set by MC, and the SDK builds and sends nothing on an empty set. That reads
exactly as this discriminator: unmuted, `sent` flat, no error anywhere. It is correct behaviour.
Sources: `DirectiveOutcome::EmittedEmptyTargets` in `crates/mc-service/src/media_signaling/directive.rs`
and `submit()` in `packages/sdk-core/src/media/pipeline/egress.ts`; the MC-side view is
[`mc-incident-response.md` §Client media signalling](mc-incident-response.md#client-media-signalling--where-to-look-no-scenario-number-yet).
If you are alone in the meeting, stop here. Peers being present is **not** the test — they may
legitimately not hold you (their slots filled by others). The test is whether some peer shows you in
an `active` slot (their `data-slot-state`; MC's view is
[`mc-incident-response.md` Scenario 18](mc-incident-response.md#scenario-18-a-participant-hears-only-part-of-the-roster)).
If one does and your `sent` is still flat, it is a real F12 and the steps below apply.

> **Recorded correction (story 2 task 17).** This entry previously named only a stuck mute, a
> capture device that never started or an ended track as causes. After loopback was removed a reader
> trusting that list, testing alone, would have chased a capture fault in a client that was doing
> what MC told it: send nothing.

**Why.** Client mute is enforced **at capture** — while muted, nothing is encoded, so nothing enters
the egress queue and nothing is dropped. Mute is therefore invisible on the drop counters by design.
A stuck mute, a capture device that never started, or a `getUserMedia` track that ended produces
exactly this: silence, with no error on any path.

**Fix.**
1. Confirm the microphone permission and the secure-context requirements in
   [§Secure Context and Media Setup](#secure-context-and-media-setup) — the whole media pipeline is
   gated all-or-nothing, so a non-trustworthy origin produces a silent capture failure, not an error.
2. In devtools, check the `MediaStreamTrack`'s `readyState` and `muted` — a track that has `ended`
   never resumes and needs re-acquiring.
3. If no microphone exists on the machine, relaunch Chrome with synthesized capture — see
   [§No microphone on this machine?](#no-microphone-on-this-machine-launch-chrome-with-synthesized-capture).

---

### F13 — `sent` rising, `received` flat: transmitting into something that is not returning

**Symptom.** Frames are leaving the device; nothing is coming back.

> **Recorded correction (story 2 task 17).** This entry used to say: "In loopback that is
> unambiguous — you should be hearing yourself." Loopback is gone (story 2 R-3): **you never hear
> yourself**, and `received` counts OTHER participants' audio, which your own `sent` has no bearing
> on. A reader trusting the old line would have treated "`received` flat" as a failed round trip and
> gone hunting a reaped NAT binding or stale MH policy while the meeting was simply giving them
> nobody to hear. (A solo participant does not reach this entry at all: it is directed to send
> nothing, so its `sent` is flat — see F12's first paragraph.)

**Discriminator — fork FIRST on whether MC has directed anyone at you.** `sent` rising only proves
that somebody holds you. Whether anybody is being sent TO you is a separate fact, and it has a
client-side reading:

| `dt_client_media_receive_source_deficit_total` | slot rows' `data-slot-state` | Meaning |
|---|---|---|
| flat | only `source_muted`, `fewer_sources` or `zero_requested`, **and the roster agrees** — those peers really are muted, there really are fewer unmuted peers than slots, or you declared zero slots | **Correct behaviour.** Silence is what the wire says. Stop. |
| flat | `fewer_sources` **while the roster shows unmuted peers you should hear** | Those peers are unreachable from you: you share no CONNECTED handler, so MC names them in `unreachable_sender_ids` instead of a slot. A connectivity fault — [`mc-incident-response.md` Scenario 18](mc-incident-response.md#scenario-18-a-participant-hears-only-part-of-the-roster). |
| flat | `awaiting-assignment` on every slot, or a media error naming the slot cap | MC never assigned your slots — F18 (receive-slot cap), or MC signalling (§4.1). |
| flat | `unspecified`, or any token MC does not emit today | An MC defect — [`mc-incident-response.md` §Client media signalling](mc-incident-response.md#client-media-signalling--where-to-look-no-scenario-number-yet) lists which slot states are reachable. File against `meeting-controller`. |
| flat | `active` | Inside the deficit counter's grace interval (a fresh or just-remapped assignment is not counted yet). Re-read after one export interval; still flat with nothing heard → F14. |
| **rising** | `active` | **MC says a source is active and nothing is arriving.** This is the fault this entry is for; continue below. |

The deficit counter counts ACTIVE assignments that decoded no frame for a whole export interval
(definition, grace rule and the export interval it is measured in: `docs/observability/metrics/client.md` §`dt_client_media_receive_source_deficit_total`);
the slot-state tokens are the wire vocabulary rendered raw into the DOM
(`packages/web-app/src/lib/slotState.ts`). With a source active and nothing arriving, **THERE IS NO
UNIQUE CLIENT-SIDE DISCRIMINATOR** between the two remaining causes:

1. a **NAT binding reaped** during a mute longer than the QUIC keepalive interval — a reaped binding
   takes the downlink with it; or
2. **MH holding stale or absent policy** — MH has no edge from that sender to you.

(If `received` is rising but you hear only some of the people you should, that is F19, not this
entry: the same deficit signal with some sources arriving.)

**Fix — work the fork in this order, cheapest first. Rung 1 is the one that separates them:**

1. **Is the QUIC connection still up?** A reaped binding shows as **connection failure or keepalive
   distress** — the transport reports a closed or timing-out connection. **MH-not-forwarding leaves
   a perfectly healthy connection.** This single observation does the discrimination the counters
   cannot.
2. **If the connection is healthy**, check for generation divergence:
   `sum by(outcome) (increase(mc_media_policy_pushes_total[15m]))`. Anything other than `match` (and
   the `handler_id_mismatch` diagnostic, which fires on every ordinary MH restart) sends you to
   [`mc-incident-response.md` Scenario 15](mc-incident-response.md#scenario-15-media-generation-divergence).
   Since story 2 any structural change (join, leave, declaration, mute) re-pushes, so a meeting with
   churn may clear by itself; a QUIESCENT meeting does not (no periodic re-assert yet) — force a
   structural change there. **After an MC restart the split can read clean by design**: the
   restart arm is recorded on `mc_media_policy_generation_adoptions_total` (and a WARN "adopting it
   as a floor" at `mc.register_meeting.trigger`), never under an `outcome` label — read that counter
   too before concluding there is no divergence.
3. **Only then** compare the configured keepalive interval against the mute duration that preceded
   the symptom.
4. **Only then** take a packet capture.

> **A packet capture or a devtools HAR of a join is CREDENTIAL-BEARING** — the join carries the
> meeting JWT. Do not attach one to a ticket, an issue, or a chat message. Named access, encrypted
> at rest, deleted on a deadline with an owner.

**Also possible and cheaper to rule out**: MH was rolled underneath you. MH sheds media sessions on
restart by design and, in this build, they do not recover on their own — see
[`mh-deployment.md` §Rollout With Media Flowing](mh-deployment.md#rollout-with-media-flowing). A
rejoin fixes it; that is the expected behaviour, not a bug.

Full triage: [`mh-incident-response.md` Scenario 17](mh-incident-response.md#scenario-17-media-datagram-drop).

---

### F14 — `dt_client_media_frames_accepted_total` is rising and you still hear nothing

**Symptom.** Every counter reads healthy across the whole path. Frames sent, received, accepted. Silence.

**Discriminator.** `accepted` rising with `dt_client_media_frames_dropped_total` flat. That combination
proves the entire crypto and parse path succeeded, so the fault is **downstream of the decoder
handoff** — and no counter in the media path reaches past that boundary.

**Why the counters stop here.** `accepted` is named `accepted` and **not `played`** precisely because
a frame handed to a decoder is not a frame that was heard: the decoder can error, the output can be
discarded, and the audio context can be suspended. The `received = accepted + sum(drops)` identity
holds at the crypto/parse boundary, not at playback, so a healthy identity is fully compatible with
total silence.

**With several senders, "`accepted` rising" no longer means every sender is working.** The
counters carry no sender dimension (by design — §4.5), so `accepted` rising proves that SOME
sender's frames reached the decoder, not that all did. "I still hear nothing" can mean one sender
works and the one you are listening for does not — a reading that was unrepresentable in loopback,
where the only sender was you. Before working the browser-side steps below, ask whether you hear
**anyone**: if one person is audible and another is not, the playback path is fine and this is a
per-source fault — go to F19 and
[`mc-incident-response.md` Scenario 18](mc-incident-response.md#scenario-18-a-participant-hears-only-part-of-the-roster).
The steps below are for silence from **everyone** while `accepted` rises.

**Fix**, in order — all of these are browser-side and none needs the cluster:
1. **Is the `AudioContext` running?** Chrome suspends it until a user gesture. In devtools, check its
   `state`; `suspended` is the single most common cause of this exact symptom and it produces no
   error.
2. **Is the output device the one you are listening to?** Check the OS/browser output selection —
   Chrome can route to a device that is not your headphones.
3. **`dt_client_media_decoder_errors_total`** — non-zero narrows it to the decoder. Flat rules the
   decoder out but rules nothing else out; it covers only part of this segment.
4. Tab muted, page volume, or OS mixer. Unglamorous and genuinely common.

---

### F15 — Sustained key-material drops: `no_kek_for_generation`, `kek_generation_stale` or `no_roster_entry`

**Symptom.** `dt_client_media_frames_received_total` rising, `accepted` flat, and
`dt_client_media_frames_dropped_total{reason}` climbing on one of the three key-material tokens.

**Discriminator.** The `reason` label, and **the three arms have different remedies — split on it
first**:

- `no_kek_for_generation` — the frame's wrap announces a KEK generation NEWER than any this client
  holds: MC's push has not arrived. Check `dt_client_media_kek_updates_total{source}` —
  `join_response` flat across a join means the KEK was never delivered; `kek_update` flat while peers
  rotate means the rotation push is not arriving.
- `kek_generation_stale` — the wrap announces a generation OLDER than this client still retains (the
  current generation plus at most one previous, kept for `min(W/2, ceiling)` where W is MC's
  `kek_rotation_debounce_seconds`). The remedy is MC-side (`MC_KEK_ROTATION_DEBOUNCE_SECONDS`).
- `no_roster_entry` — no usable identity key for the sender, **including the case where MC published
  an empty key**. This one also means signature verification cannot run at all, not merely decryption.

Before going server-side, read the two client counters that explain the KEK arms:
`dt_client_media_kek_retention_anomalies_total{outcome}` (`floor_substituted` = an MC older than the
retention field, expected during a deliberate MC rollback; `below_rewrap_latency` = MC's W/2 under
the client's re-wrap latency; `ceiling_clamped` is a configuration state, not a fault) and
`dt_client_media_kek_install_refusals_total{outcome}` (`conflicting_key`, `older_generation`,
`malformed` — a KEK message the client refused, keeping what it held). `sender_not_assigned` is
**not** a key-material token and does not belong to this scenario.

**All three are EXPECTED as brief transients** at join and immediately after a KEK rotation. **A burst of
a few frames at join is normal and is not this failure mode.** Sustained is the signal.

**Fix.** The remedy is server-side, in MC's KEK-and-roster delivery path, and it is documented in one
place:
[`mc-incident-response.md` Scenario 16: Missing Key Material](mc-incident-response.md#scenario-16-missing-key-material).
**Do not duplicate the remedies here** — that scenario carries the per-arm split, the server-side
counters that corroborate the roster arm, and the reason the KEK arm has no server-side counter at
all.

**Two things to know before you go there**, because they change how you read a quiet system:
- **An alert MAY have fired — and its silence still is not evidence of health.**
  `MCMediaMissingKeyMaterial` became able to fire on 2026-09-23 (§4.5, *Where these counters can
  actually be read*), so check it. But it is a *ratio* over a 5-minute window held for 15 minutes,
  so a short local repro will not trip it, and the series can go missing entirely without the alert
  firing — five collector-side controls drop client data by design and all of them look like "no
  browser is running".
- **Do not add a log line to check whether the KEK is present.** ADR-0036 §11 puts KEK and
  transmit-key material inside the credential-leak guard's scope for exactly this moment. A KEK in a
  log is a KEK in the log pipeline.

### F16 — "I could hear everyone, then went silent after someone left"

**Symptom.** Audio from some or all other participants stops at, or shortly after, the moment a
participant **leaves**. You stay joined, the roster is correct, and the slot grid may still show the
remaining senders as active.

**Why a leave matters.** Since story 2, every roster removal triggers a meeting KEK rotation
(debounced to once per W, MC's `MC_KEK_ROTATION_DEBOUNCE_SECONDS`). Every remaining member receives a
new KEK and generation, and every sender re-wraps its transmit keys. If the new KEK does not reach
you, or you retire the previous generation while frames under it are still arriving, frames are
dropped for key material and you hear silence. **This is the rotation signature, and the
discriminator is leave correlation.** A silence that starts on a leave is this entry. A silence that
starts on a join is F15's join path, and a silence present from the start is §4.5.

**Where to look, cheapest first.**
1. **Is ANY client series present in Prometheus at all?** Query `dt_client_media_frames_sent_total`
   (the solo-safe liveness series). If nothing comes back, the client metrics pipe is dead, and
   **a dead pipe leaves `MCMediaMissingKeyMaterial` green**. Every rung below reads empty for the same
   reason. Fix the pipe first (`VITE_TELEMETRY_ENDPOINT` set? then the quiet-series ladder in
   `gc-deployment.md` §When the client series go quiet), and only then continue.
2. `dt_client_media_frames_dropped_total{reason}` over the minutes around the leave.
   `no_kek_for_generation` means the new KEK did not arrive. `kek_generation_stale` means the old one
   was retired too early. A short burst right after the leave is expected. Sustained is the signal.
3. **Did the rotation reach you?** `dt_client_media_kek_updates_total{source="kek_update"}` should step
   once per rotation. Flat across a leave means the rotation push did not arrive. On the MC side,
   `mc_meeting_kek_generated_total{trigger="participant_left"}` shows whether MC rotated at all.
4. **Did you keep the previous generation?** Read `dt_client_media_kek_generations_retained_total` (a
   **counter**: one increment per install that kept the previous generation) against
   `dt_client_media_kek_updates_total{source="kek_update"}`. If rotations arrive while the retained
   counter stays flat, nothing is being retained, and you get a gap at every rotation. Neither series
   alone says anything.

**Fix.** The remedy lives in one place, the rotation arm of
[`mc-incident-response.md` Scenario 16](mc-incident-response.md#rotation-arm--silence-after-someone-left)
(per-recipient push outcomes, the retention-expiry rung, and why raising W usually does not help).
**Do not duplicate it here.** The immediate unblock for you is a **rejoin**. Click the **Create**
nav, then **Join**, and enter the same code. That is a fresh join, and it carries the current KEK.
A page reload also works, but it drops the in-memory sign-in (`App.svelte`), so you then have to sign
in again. MC never re-sends a KEK push that was dropped.

### F17 — "I can hear some people but not all"

**Symptom.** In a meeting with more participants than your receive-slot count, you hear the people
who joined first and not the ones who joined later. Nothing errors, and no drop counter moves.

**Why.** This is **static fill** (story 2 R-2): your client declares N receive slots
(`VITE_DT_RECEIVE_SLOTS`, capped by MC's `MC_MAX_RECEIVE_SLOTS`), and MC fills them **in join order**
from the senders you share a connected handler with, and never reselects. A slot changes only when its
occupant leaves. It is then refilled by the earliest-joined sender not yet assigned. **A late joiner
is inaudible to you until someone you can hear leaves.** That is the designed behaviour, not a fault.

**Discriminator.**
- **Effective N**: count the cells in the in-meeting slot grid (`data-testid="slot-list"`, one cell
  per declared slot, each naming its sender). The E2E bus `receiveSlots.declared` and the console
  line `[dt-media-slots] declaring N=...` show the same number. The server cap is
  `mc_media_receive_slot_cap`, also on the bus as `receiveSlots.serverCap`.
- If every cell is filled and the unheard participants are the latest joiners, it is static fill.
- If a cell says fewer sources, or an unheard participant is marked unreachable on the roster, it is
  not static fill. Unreachable (no shared handler) is
  [`mc-incident-response.md` Scenario 18](mc-incident-response.md#scenario-18-a-participant-hears-only-part-of-the-roster).
  An over-cap N that hears nobody at all is F18.

**Fix.** Raise N, up to the cap (`VITE_DT_RECEIVE_SLOTS=<n> scripts/dev-web.sh`), or accept it.
**There is no debugger for slot selection** beyond the slot grid. Speaker-based selection (story 5)
replaces static fill.

### F18 — "I hear nobody", and the media error names the receive-slot cap

**Symptom.** Start audio fails and the in-meeting view shows a `media-error` like
`SIGNALING: this client is configured to receive 5 audio slots, but the meeting controller allows
at most 4 ... Lower VITE_DT_RECEIVE_SLOTS ... or raise MC_MAX_RECEIVE_SLOTS`. The participant hears
nobody; others may still hear them.

**Why it is loud.** N (`VITE_DT_RECEIVE_SLOTS`, the client's declared receive-slot count) is a
REQUEST bounded by MC's `MC_MAX_RECEIVE_SLOTS`, which MC advertises on
`JoinResponse.max_receive_slots`. MC rejects an over-cap declaration **whole** — never clamps it — so
the SDK refuses it before sending rather than presenting as one silent participant, and counts it on
`dt_client_media_receive_slots_rejected_total`. `dt_client_media_receive_source_deficit_total` reads
**0** for this failure (there are no active assignments to be silent) — do not read that as health.

**Discriminator.**
- The effective N and the cap are on the E2E bus (`receiveSlots`: `declared`, `source`,
  `serverCapState`, `serverCap`) and in the browser console line `[dt-media-slots] declaring N=...
  receive slots (...); server cap ...`. `source: default` means `VITE_DT_RECEIVE_SLOTS` was not set
  and the SDK default applied.
- `serverCapState: unknown` means the MC predates the advertised cap: the SDK declares anyway and MC
  rejects, which surfaces as a session `error` and counts on MC's
  `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}` — see the
  receive-capability rejection section of [`mc-incident-response.md`](mc-incident-response.md).
- `serverCapState: invalid` (a present 0) is an MC contract violation; the console WARNs with the
  value. Report it against MC.

**Fix.** Make them agree — never by clamping: `VITE_DT_RECEIVE_SLOTS=<n> scripts/dev-web.sh` with
`n` at or below the cap (the preflight prints both and refuses a mismatch), or raise
`MC_MAX_RECEIVE_SLOTS` in the MC ConfigMap. The browser knob is **not** covered by `dt-guard
env-config`; the runtime value on the bus is the check.

### F19 — Receive-source deficit rising: MC says a source is active and nothing decodes

**Symptom.** `dt_client_media_receive_source_deficit_total` is increasing (client-media board,
*Receive Path - Silent Active Sources*). Participants report hearing some people but not others.

**What it means.** Per export interval the SDK counts ACTIVE assignments — MC assigned a sender to
one of this client's declared slots and did not mark it source-muted — from which it decoded **no**
frame for the whole interval. DTX is off, so a live, unmuted sender always produces frames: MC's
claim and the receiver's observation disagree. A newly assigned slot gets one interval of grace, so
a join or re-map alone does not tick it.

**Where to look next, cheapest first.**
1. Is anything arriving? `dt_client_media_frames_received_total` flat → the frames never reach the
   client: MH forwarding. Work
   [`mh-incident-response.md` Scenario 17: Media Datagram Drop](mh-incident-response.md#scenario-17-media-datagram-drop)
   and [Scenario 16](mh-incident-response.md#scenario-16-ingress-datagrams-received-but-never-read)
   (and Scenario 15 if senders are not binding).
2. Arriving but dropped? `dt_client_media_frames_dropped_total{reason}` — key material → F15;
   `sender_not_assigned` → MC's assignment and MH's routing disagree (MC routing).
3. Accepted but not decoded? `dt_client_media_decoder_errors_total` and
   `dt_client_media_decode_queue_dropped_total`.
4. Nothing on the client side explains it → MC routing: the slot state MC pushed does not match the
   edges it programmed on MH. See `mc-incident-response.md` (media routing / generation divergence).

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
`26_mh_quic.rs` covers the MH QUIC path; `27_mc_slot_placement.rs` is the multi-party proof of the
shared-handler edge model (every participant offered both handlers; mock clients produce partial
connectivity by which handlers they actually open a media session to; real datagrams forwarded on
the handler that owns each edge) and is the one test here that needs **both** MH pods healthy and
reachable — it hard-fails with `Triage MH handler-set health` (fewer than two handlers offered) or
`Triage MH reachability` (a mock could not open a session to a handler it was told to use) rather
than adapting to a single handler. All three share one WebTransport client
(`src/fixtures/mc_session.rs`), so a connect/framing/join symptom that hits all three at once is
that fixture, not three regressions. This is the same Rust suite the devloop's Layer 7 gate runs
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
- ~~**No media plane.**~~ **Corrected (story 2 task 17): the media plane EXISTS on this branch** —
  SFrame-encrypted Opus over MH datagrams, triaged by §4.5 and F12–F19. The bullet previously said
  MH connections carried no datagrams; a reader trusting it would have treated a silent call as
  expected and skipped §4.5.
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
