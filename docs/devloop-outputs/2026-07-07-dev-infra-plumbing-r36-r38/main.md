# Devloop Output: Dev Infra Plumbing (R-36/R-37/R-38)

**Date**: 2026-07-07
**Task**: Cert SHA-256 fingerprints + 14d WebTransport leaf validity (R-36); Kind dev overlays for AC/GC NodePort with 127.0.0.1 listenAddress (R-37); demo org seed in setup.sh (R-38)
**Specialist**: infrastructure
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task-4`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e78dfc3f6e90ba48f4d8365ca9acd91e52b30b27` |
| Branch | `feature/browser-client-join-task-4` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete (Gate 3 committed db070fc)` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `RESOLVED-DEFERRED` |
| Semantic Guard | `CLEAR (SAFE)` |
| Database (conditional) | `CLEAR` |

**Gate 3 (Final Approval)**: all 8 verdicts terminal, none ESCALATED. One review finding (cert-reuse guard ignored validity-window upper bound → legacy 365d WT leaf never downgraded to 14d, Chrome-rejected) raised by @code-reviewer, endorsed by @security, FIXED in-diff (strict no-slack form `-checkend 86400 && ! -checkend $((days*86400))`; ≤14d reuse invariant holds). @security §6.4 co-sign AUTHORIZED → commit carries `Approved-Cross-Boundary: security` trailer. Post-fix: all 33 Layer-3 guards pass. Two accepted deferrals (both RESOLVED-DEFERRED verdicts): DRY port-scattering extraction + operations auto-rollout-restart→R-49/task#20, both tracked in docs/TODO.md.

**Gate 1 (Plan Approval)**: PASSED — all 8 reviewers confirmed. Classification-sanity guard `STATUS=OK` (after `dt-guard` release build). `generate-dev-certs.sh` reconciled to §6.4 GSA edit per @security owner ruling; commit requires `Approved-Cross-Boundary: security` trailer. Accepted deferral pre-noted by @operations: auto `kubectl rollout restart` in secret-recreate path deferred to R-49/task #20 (manual recovery sequence is the correctness floor).

**Gate 2 (Validation)**: PASSED for this changeset. `./scripts/layer-all.sh` per-layer: L1 compile OK, L2 fmt OK, L3 guards OK (incl. `validate-cross-boundary-classification`, `validate-cross-boundary-scope`, `no-hardcoded-secrets` all PASSED), L5 clippy+rust-tests OK, L6 audit N/A (`cargo audit`/`pnpm audit`/`buf breaking` all passed; no changeset-relevant audit surface), L7 env-tests OK. **L4 (test) RED — environment-precondition failure, NOT a changeset regression**: `sdk-svelte:test:component` + `web-app:test:component` are Vitest browser-mode (`@vitest/browser-playwright`, `browser: 'chromium'`) tests that fail with the Playwright "browser not installed" banner because Chromium is absent in this local env (`~/.cache/ms-playwright/` empty; browser download times out). Both packages are **byte-identical to the Start Commit** (`git diff e78dfc3 -- packages/sdk-svelte packages/web-app` empty) and this devloop touches **0 `packages/**` files** — the failure is pre-existing and unfixable by this infra diff. Owned by R-35 (devloop image must pre-cache Chromium) / R-43 (component tests run "in CI on GitHub-hosted Linux runners with Playwright-installed Chromium"). Node unit tests all passed. Not routed to implementer; not an attempt consumed.

---

## Task Overview

### Objective
Dev-infrastructure plumbing to unblock the browser-client-join happy path (user story 2026-05-02), task #4:
- **R-36**: Extend `scripts/generate-dev-certs.sh` to compute SHA-256 of the DER leaf for `mc-webtransport.crt` + `mh-webtransport.crt`, write to a stable `infra/docker/certs/fingerprints.json` consumable via `MC_CERT_SHA256`/`MH_CERT_SHA256`. Idempotent. WebTransport leaf validity ≤ 14 days, ECDSA P-256 (Chrome `serverCertificateHashes` requirement).
- **R-37**: Kind exposure for AC + GC HTTP — `extraPortMappings` (TCP) in the static `infra/kind/kind-config.yaml` mapping host ports (AC 8443, GC 8444) to the existing NodePort services, with `listenAddress: 127.0.0.1` (loopback-only, explicit security requirement). Dev/E2E only.
- **R-38**: Seed one `organizations` row (`subdomain = 'demo'`) in `infra/kind/scripts/setup.sh` so browser sign-up has a default org. Idempotent (`ON CONFLICT DO NOTHING`). No DB schema change.

### Scope
- **Service(s)**: Dev infrastructure only (cert-gen script, Kind config, setup.sh). No service (Rust) code.
- **Schema**: No — R-38 seeds a data row, no migration.
- **Cross-cutting**: Yes — cert crypto (security), cert validity/rotation (operations), org seed (database), E2E consumability (test).

### Debate Decision
NOT NEEDED — plumbing within existing ADR-0013/ADR-0030 dev-environment patterns; requirements fully specified in the user story (R-36/R-37/R-38).

### Lead setup notes (context handed to implementer)
- **R-37 current state**: NodePort overlays *already exist* — `infra/kubernetes/overlays/kind/services/ac-service/nodeport.yaml` (nodePort 30082) and `gc-service/nodeport.yaml` (http 30180, grpc 30051), applied by `setup.sh`. The dynamic `kind-config.yaml.tmpl` (devloop-helper) already maps these with `listenAddress: ${HOST_GATEWAY_IP}`. The gap is the **static** `infra/kind/kind-config.yaml` (used by `setup.sh` for manual dev) which currently maps only observability + MC/MH UDP ports — it needs AC/GC TCP `extraPortMappings` (8443→30082, 8444→30180) with `listenAddress: 127.0.0.1`.
- **R-36 current state**: `DAYS_CERT=365` for all service certs; ECDSA P-256 already in place; no fingerprint file exists. Decide whether 14d applies only to the WebTransport leaves (mc/mh) vs the auth-localhost cert (task title: "14d WebTransport leaf validity").
- **R-38 current state**: `seed_test_data()` already seeds a `devtest` org via `ON CONFLICT DO UPDATE`; add the `demo` org (`ON CONFLICT DO NOTHING` per requirement).
- Related tech-debt: `docs/TODO.md` §Port Constant Scattering (Kind / K8s / Env-Tests) — the 8443/8444 hardcodes may intersect.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/generate-dev-certs.sh` | **Domain-judgment; ADR-0024 §6.4 crypto-primitive GSA edit** (owner **@security**) | **§6.4 GSA edit per @security's owner ruling** (§6.2 monotonicity — owner's upgrade governs, not negotiated down): the SHA-256 DER-leaf fingerprint IS the browser's `serverCertificateHashes` trust anchor, i.e. an ADR-0027-approved crypto primitive "wherever referenced," so this falls on the §6.4 surface even though no NEW primitive is introduced. **Commit MUST carry** `Approved-Cross-Boundary: security — DER SHA-256 cert-pin fingerprint on ADR-0027 crypto surface; P-256 signing unchanged, no new primitive`. @security's final co-sign is contingent on verifying the fingerprint computation (SHA-256, DER-not-PEM, correct leaf, base64/`atob`) at Start Review. **@operations** co-reviews the 14d WT-leaf rotation-cadence lens (PKI policy, not itself a §6.4 primitive). |
| `infra/kind/kind-config.yaml` | Mine, Minor-judgment | Loopback-only `listenAddress: 127.0.0.1` binding co-reviewed by **@security**. 2 NEW AC/GC TCP rows + hardening the 4 EXISTING MC/MH UDP rows to `127.0.0.1` (@security sibling rule). Observability rows deliberately left wildcard (LAN dashboard access). |
| `infra/kind/scripts/setup.sh` (new `seed_demo_org()` + `create_cluster()` recreate-cliff warning) | **Mine** (infrastructure); seed is a data-row | `organizations` INSERT (columns, ON CONFLICT semantics): **@database in-place conditional co-review; NO owner-trailer** (not a GSA, not schema evolution — @database wording). Data-row seed in my own `setup.sh`, not in `db/migrations/**` → does NOT trigger §6.4; the existing devtest-seed precedent supports in-place-not-spin-out (§6.3). The `create_cluster()` warning is @operations-flagged infra plumbing (mine). |
| `.gitignore` (add `fingerprints.env`) | Mine, Mechanical | **@security** (R-33 no-secrets-in-code — fingerprints are public but kept out of the tree for hygiene) |
| `docs/TODO.md` (one-line extension of §Port Constant Scattering) | Mine, Mechanical | Note the new static 8443/8444 host-port hardcodes alongside the existing MC/MH port-scattering entry (@code-reviewer #6) |

---

## Planning

### R-36 — cert fingerprints + 14d WebTransport leaf validity (`scripts/generate-dev-certs.sh`)

1. **Validity window scoping decision**: the ≤14-day cap applies **only to the MC + MH WebTransport leaf certs**, NOT the `auth-localhost` cert.
   - *Justification*: Chrome's 14-day ceiling is a constraint of the `serverCertificateHashes` (ephemeral self-signed pin) trust path only — the path MC/MH use. `auth-localhost` is trusted the ordinary way (dev CA imported into the trust store); it never travels the `serverCertificateHashes` path, so no 14-day ceiling applies. Dropping it to 14d would only add regeneration toil. Stays at 365d. Aligns with the task-4 decomposition ("auth/non-WT certs unchanged at 365 days", story line 343).
   - Implementation: add `DAYS_WT_CERT=14`; keep `DAYS_CERT=365`. Parameterize `generate_service_cert` to take an explicit `days` argument (readability over an implicit global).

2. **Conditional (re)generation — the idempotence + skew fix (@test #1, @operations #4)**. `setup.sh` invokes this script **twice** per full run (`create_mc_tls_secret` → run #1, `create_mh_tls_secret` → run #2). The current unconditional regen means run #2 mints a NEW `mc-webtransport.{key,crt}` AFTER run #1 already baked the old one into the `mc-service-tls` Secret → deployed MC cert ≠ on-disk cert ≠ `fingerprints.json` → silent `serverCertificateHashes` mismatch at E2E (R-44). Fix: `generate_service_cert` **regenerates a leaf only when** (a) cert or key file is missing, OR (b) the cert is expiring within a 24h buffer (`openssl x509 -checkend 86400`), OR (c) the CA was (re)generated this run (a new CA invalidates leaves signed by the old one — tracked via a `CA_REGENERATED` flag), OR (d) `--force` is passed. Otherwise it prints "reusing existing … (valid)" and returns. Effect: within a single run, run #2 reuses run #1's leaves unchanged → Secret, on-disk cert, and fingerprint all agree; and same-day re-runs of `setup.sh` are fully stable (up to ~13 days of stable reuse for the 14d WT leaves). Adds a `--force` flag (leaf regen) alongside the existing `--force-ca`.

3. **Fingerprint compute** (always recomputed + rewritten every invocation, from whatever certs are on disk — so `fingerprints.json` can never drift from the deployed leaf; @operations #4). For each of `mc-webtransport.crt` / `mh-webtransport.crt`, SHA-256 the **DER of the full leaf cert** (not SPKI/pubkey), pipefail-safe capture, no line-wrap:
   `b64="$(openssl x509 -in <crt> -outform DER | openssl dgst -sha256 -binary | openssl base64 -A)"`

4. **Encoding decision**: **standard base64** (not base64url, not hex) is the canonical value the client/Playwright consume.
   - *Justification*: the already-committed consumer `packages/web-app/src/lib/config.ts` decodes with `atob()` (standard base64) into the `serverCertificateHashes` bytes, and `packages/web-app/vite/fingerprints.ts` reads keys `MC_CERT_SHA256` / `MH_CERT_SHA256`. `vite.config.ts` proxies AC/GC over plain HTTP (`http://127.0.0.1:8443/8444`) — so AC/GC serve plain HTTP in dev-Kind (no TLS termination in this story's scope; @test #3 answered). I must match that committed contract (task #15 shipped). Hex is emitted too as a convenience/debug field but the base64 keys are load-bearing.

4b. **Expiry hint + recovery path (@operations #2/#3)**: on every run, print each WT leaf's `notAfter` timestamp and the recovery one-liner. Recovery on an already-running cluster is NOT just re-running this script (that only rewrites PEMs) — it is: re-run `./infra/kind/scripts/setup.sh` (regenerates via the conditional path when expiring, recreates the `mc/mh-service-tls` Secrets, redeploys) **and** `kubectl rollout restart deployment/mc-0 deployment/mc-1 deployment/mh-0 deployment/mh-1 -n dark-tower` (apply alone won't restart pods on a secret-only change — pre-existing setup.sh behavior, flagged for the R-49 runbook, not fixed here to avoid scope creep) **then** restart `pnpm dev` (reloads the browser-side fingerprint). The hint text names this full sequence so an operator isn't left with fresh PEMs but stale pods + a wrong browser fingerprint before R-49 lands.

5. **Output files** (both rewritten every run — idempotent; both gitignored):
   - `infra/docker/certs/fingerprints.json`:
     ```json
     {
       "generated_at": "<ISO-8601 UTC>",
       "MC_CERT_SHA256": "<std-b64>",
       "MH_CERT_SHA256": "<std-b64>",
       "mc_cert_sha256_hex": "<hex>",
       "mh_cert_sha256_hex": "<hex>",
       "mc_cert_expires_at": "<ISO-8601 UTC>",
       "mh_cert_expires_at": "<ISO-8601 UTC>"
     }
     ```
     Keys `MC_CERT_SHA256`/`MH_CERT_SHA256` are the ONE canonical key for the load-bearing base64 value — **no camelCase alias, no `_B64` alias** (@test: dual keys are a drift hazard). They match BOTH R-36's env-var names AND the committed `fingerprints.ts` tolerant parser. `*_sha256_hex` and `*_expires_at` are DISTINCT debug/diagnostic data, NOT alternate encodings of the base64 key.
     - **`*_cert_expires_at`** (@observability): leaf `notAfter` in ISO-8601, so an operator can distinguish "cert expired" from "fingerprint stale" on an opaque WebTransport refusal (routine given the 14d window). Computed via `date -u -d "$(openssl x509 -in <crt> -enddate -noout | sed 's/notAfter=//')" +%Y-%m-%dT%H:%M:%SZ`.
   - `infra/docker/certs/fingerprints.env` (sourceable by Playwright global-setup / shell):
     ```sh
     export MC_CERT_SHA256="<std-b64>"
     export MH_CERT_SHA256="<std-b64>"
     ```
6. **Post-generation sanity assertion** (@test testability): after writing, assert each base64 value decodes to exactly 32 bytes (`openssl base64 -d | wc -c` == 32) and print a one-line confirmation naming the file + both keys — so a malformed/empty file surfaces as a clear error at generation time, not as an opaque browser handshake refusal downstream.
7. **Env-var naming note**: R-36 (governing) says `MC_CERT_SHA256`/`MH_CERT_SHA256`; two task-decomposition rows (#4, #18) drifted to a `_B64` suffix. I follow R-36 + the committed consumer; @test confirmed no `_B64` aliases (task #18 global-setup will align to the no-suffix names).
8. **`.gitignore`** (@security): add an explicit `infra/docker/certs/fingerprints.env` line — a bare `.env` glob does NOT match `fingerprints.env`. (`fingerprints.json` is already ignored at line 39.)

### R-37 — Kind static exposure for AC + GC HTTP (`infra/kind/kind-config.yaml`)

Add two TCP `extraPortMappings` to the static config (the dynamic `.tmpl` already has these under `${HOST_GATEWAY_IP}`; NodePort Services already exist at 30082/30180):
- host **8443** → containerPort **30082** (AC HTTP), `listenAddress: "127.0.0.1"`, `protocol: TCP`
- host **8444** → containerPort **30180** (GC HTTP), `listenAddress: "127.0.0.1"`, `protocol: TCP`

**GC gRPC (30051) intentionally NOT mapped**: R-37 is scoped to AC sign-up + GC *join HTTP* for the host-side Chromium flow. gRPC is cluster-internal for this story; adding it would over-scope. The dynamic `.tmpl` keeps it for the devloop-helper topology; the static manual-dev config does not need it.

**Loopback requirement**: `listenAddress: "127.0.0.1"` on both — explicit security requirement so a laptop on a hostile network never exposes dev AC/GC to the LAN. Dev/E2E only; production exposure out of scope.

**UDP-sibling hardening (@security, same-owner/same-mechanism-sibling rule — accepted)**: the existing MC/MH WebTransport UDP mappings in the SAME file (containerPorts 30433/30435/30434/30436, host 4433/4435/4434/4436) currently bind `0.0.0.0` (LAN-exposed). Since the static config is the manual-host-dev topology where the host browser reaches MC/MH WT at `localhost`, I will add `listenAddress: "127.0.0.1"` to those **four UDP mappings** too — same-file, ~4-line hardening, no functional loss (there is no kind/Docker UDP constraint requiring `0.0.0.0` here). **Static-vs-`.tmpl` divergence stays intentional** (@code-reviewer): static → `127.0.0.1`; the `.tmpl` keeps `${HOST_GATEWAY_IP}` for the devloop-container topology — do NOT "align" them.

**Observability mappings (Prometheus 30090/9090, Grafana 30030/3000, Loki 30080/3100) deliberately left on wildcard** — a DECISION, not an oversight (@security accepted; closes @code-reviewer's consistency ask): LAN access to dev dashboards from a second device is a legitimate dev pattern, and these are a different risk class from the browser-client dataplane/signaling (the browser always reaches AC/GC/MC/MH at localhost → those get loopback). I will add a **one-line comment** next to the three observability mappings recording the intentional wildcard bind + the residual risk @security flagged (hostile-network exposure of dev Loki logs / Grafana foothold — dev-only, out of this task's scope to fix), so it reads as deliberate at Gate 2.

**Cluster-recreate cliff (@operations A/B)**: `extraPortMappings` only bind at `kind create cluster`; an operator reusing an existing cluster gets nothing new until teardown+recreate. Add a one-line warning in `setup.sh create_cluster()` whenever it reuses an existing cluster — naming the teardown+recreate command to pick up new port mappings, and noting a host-port collision makes `kind create` fail loudly with "port is already allocated" (free the port and retry). Cheapest durable fix pending R-49; keeps the change from being a silent no-op.

### R-38 — seed demo org (`infra/kind/scripts/setup.sh`)

Add a `seed_demo_org()` function (called in `main()` right after `seed_test_data()`), mirroring the existing inline `kubectl exec … psql -c` pattern:
```sql
INSERT INTO organizations (subdomain, display_name)
VALUES ('demo', 'Demo Organization')
ON CONFLICT (subdomain) DO NOTHING;
```
- **`ON CONFLICT DO NOTHING`** per R-38 (contrast the existing `devtest` seed's `DO UPDATE` — following the requirement verbatim; a demo org should not be clobbered on re-run).
- **Columns**: `(subdomain, display_name)` — both real columns; `display_name` is the only NOT-NULL-without-default beyond `subdomain` (verified against `migrations/20250118000001_initial_schema.sql`: `plan_tier` defaults `'free'`, `max_concurrent_meetings` defaults `10`). So the INSERT is valid and the demo org gets sane defaults. @database to confirm the 2-column set is acceptable vs. matching the fuller `devtest` column set.
- **Inline, not a sibling SQL file**: chosen for consistency with the existing `seed_test_data()` inline pattern (avoids introducing a new `seeds/` dir + `kubectl exec -i … < file` stdin plumbing for a single row). R-38 permits either. **No DB schema change.**

### Discoverability (@test B)
Add a one-line entry to `setup.sh print_access_info` naming the host-side E2E passthrough (`AC: http://127.0.0.1:8443`, `GC: http://127.0.0.1:8444` — loopback NodePort passthrough) so a human running `setup.sh` sees the ports before the R-49 runbook / R-30 README land. In-tree home chosen: `print_access_info` (alongside the existing AC/GC URL block).

### Known limitations / R-49 handoff (@operations durable-record condition)
- **Cert-secret recreate does NOT auto-restart MC/MH pods.** On an already-running cluster, regenerating an expired WT leaf + recreating the `mc/mh-service-tls` Secret does not by itself restart the pods serving the old leaf (`kubectl apply` is a no-op on unchanged Deployment specs). The expiry hint prints the full manual sequence (`setup.sh` → `kubectl rollout restart deployment/mc-0 mc-1 mh-0 mh-1 -n dark-tower` → restart `pnpm dev`). **Auto rollout-restart on secret recreate is deliberately deferred to R-49 (task #20)** — it's a behavior change to shared `setup.sh` firing on ALL runs (regression surface), out of R-36 plumbing scope. Also recorded as a `docs/TODO.md` line pointing at task #20 so #20 inherits it. Manual breadcrumb is the correctness floor; auto-restart is convenience on top.

### Security co-sign (ADR-0024 §6.4 — @security ruling)
@security ruled the `generate-dev-certs.sh` change falls inside §6.4 ("ADR-0027-approved crypto primitives, wherever referenced") because the **SHA-256 DER-leaf fingerprint IS the browser's `serverCertificateHashes` cert pin** (a security-load-bearing crypto reference), even though no new primitive is introduced. The commit touching `generate-dev-certs.sh` MUST carry:
```
Approved-Cross-Boundary: security — DER SHA-256 cert-pin fingerprint on ADR-0027 crypto surface; P-256 signing unchanged, no new primitive
```
Co-sign is contingent on @security examining the actual computation at Start Review (SHA-256 not another digest, DER-not-PEM, correct leaf cert, base64 matching the consumer's `atob()`).

### Verification (Gate 2 preconditions I will run before "Ready for validation")
- `shellcheck scripts/generate-dev-certs.sh infra/kind/scripts/setup.sh`
- **No-thrash idempotence (@test A)**: run `generate-dev-certs.sh` twice; assert (1) `MC_CERT_SHA256`/`MH_CERT_SHA256` values are **byte-identical across both runs**, and (2) `mc-webtransport.crt`/`mh-webtransport.crt` are **unchanged on run #2** (content hash) — proving the reuse path was taken, not a silent regen. `generated_at` is expected to differ (consumer ignores it — do NOT assert on it). Also assert each base64 decodes to exactly 32 bytes.
- `kubeconform` / `kind`-schema sanity on `kind-config.yaml` (YAML parse + kind config lint).

---

## Implementation Summary

Files changed (git diff --stat): `.gitignore` (+2), `docs/TODO.md` (+4/-1), `infra/kind/kind-config.yaml` (+38/-8), `infra/kind/scripts/setup.sh` (+41), `scripts/generate-dev-certs.sh` (+199/-20).

**R-36 — `scripts/generate-dev-certs.sh`**
- `generate_service_cert` now takes an explicit `days` arg. `DAYS_WT_CERT=14` for mc/mh WT leaves; `DAYS_CERT=365` (auth-localhost) and `DAYS_CA=3650` unchanged. Verified: mc/mh notAfter = notBefore + 14d; auth = +365d; all ECDSA P-256.
- **Conditional regeneration**: a leaf is reused unless missing / expiring within 24h (`openssl x509 -checkend 86400`) / CA regenerated this run (`CA_REGENERATED` flag) / `--force`. New `--force` flag added alongside `--force-ca`.
- Fingerprints: SHA-256 of the **DER of the full leaf**, standard base64 via `openssl base64 -A` (+ hex + ISO-8601 `*_expires_at` diagnostics). Written to `infra/docker/certs/fingerprints.json` (canonical keys `MC_CERT_SHA256`/`MH_CERT_SHA256`, no aliases) + sourceable `fingerprints.env`, **rewritten every run** from on-disk certs. Post-gen assertion: each base64 decodes to exactly 32 bytes (fixed a decode bug — single-line base64 needs `openssl base64 -d -A`).
- Expiry hint prints each WT leaf `notAfter` + the full manual recovery sequence (setup.sh → rollout restart → pnpm dev).

**R-37 — `infra/kind/kind-config.yaml`**: added 2 TCP `extraPortMappings` (host 8443→30082 AC, 8444→30180 GC, `listenAddress: "127.0.0.1"`); hardened the 4 existing MC/MH UDP mappings to `127.0.0.1`; observability trio left wildcard with an explicit intentional-decision comment. GC gRPC 30051 not mapped. Validated via YAML parse + per-mapping assertions (9 mappings total, correct proto/listenAddress lineup).

**R-38 — `infra/kind/scripts/setup.sh`**: new `seed_demo_org()` (`INSERT INTO organizations (subdomain, display_name) VALUES ('demo','Demo Organization') ON CONFLICT (subdomain) DO NOTHING`) called in `main()` after `seed_test_data()`. Used the direct-`if` exit-code form (shellcheck-clean + failure branch reachable under `set -e`).

**Carry-forwards**: `create_cluster()` recreate-cliff + collision warning (`warn_port_mapping_cliff`, fires on both reuse paths); `print_access_info` 8443/8444 host-side lines; `.gitignore` explicit `fingerprints.env`; `docs/TODO.md` §Port Constant Scattering extension + R-49 auto-rollout-restart handoff line.

**Verification (all green)**:
- `shellcheck` (v0.10.0): `generate-dev-certs.sh` **CLEAN**; `setup.sh` has only 3 **pre-existing** findings (SC1090 line 63 DT_PORT_MAP source; SC2181 lines 554 & 569 in `seed_test_data`) — my additions introduce **zero** new findings. Both files `bash -n` clean.
- No-thrash: two consecutive runs → mc/mh cert files unchanged on run #2 (reuse path taken) + `MC_CERT_SHA256`/`MH_CERT_SHA256` byte-identical; `--force`/`--force-ca` regen paths verified; leaf chains to CA; `fingerprints.env` sources cleanly.
- Fingerprint correctness: base64 decodes to 32 bytes and matches an independent `openssl … DER | dgst -sha256` recompute; keys match the committed `packages/web-app/vite/fingerprints.ts` parser.
- `kind-config.yaml`: valid YAML; 9 mappings assert correct (AC/GC/MC/MH loopback, obs wildcard, no 30051). `kind`/`kubeconform` not installed locally (kind config is not a standard k8s API type; the static config is exercised at cluster bring-up) — flagged for the Gate-2 pipeline.

Commit trailer required (per @security §6.4 ruling): `Approved-Cross-Boundary: security — DER SHA-256 cert-pin fingerprint on ADR-0027 crypto surface; P-256 signing unchanged, no new primitive` (team-lead commits at Gate 3 after co-sign).

---

## Code Review Results

**Verdicts**: @observability CLEAR · @security CLEAR (+ §6.4 co-sign AUTHORIZED after empirical pipeline verification) · @operations RESOLVED-DEFERRED (the pre-agreed R-49 auto-rollout-restart deferral) · @test CLEAR · @code-reviewer 1 fix-now finding (resolved below) · @dry-reviewer / @database / @semantic-guard pending.

**FIXED — @code-reviewer (fix-now): cert-reuse guard ignored validity-window length → R-36 ≤14d migration gap.**
`scripts/generate-dev-certs.sh` reuse guard originally used only the lower bound (`openssl x509 -checkend 86400`). On a dev tree that predates this story, the mc/mh WebTransport leaves already exist at **365-day** validity (minted by the old script for the MC/MH QUIC env-tests); they passed the 24h check and were reused forever, never regenerated to 14 days. Chrome rejects a `serverCertificateHashes` pin whose validity exceeds 14 days regardless of hash match → silent WebTransport failure on the realistic upgrade path, defeating R-36's purpose.
Fix (added the upper bound to the reuse condition): `&& ! openssl x509 -checkend $(( days * 86400 )) -noout -in "$cert_file" >/dev/null 2>&1`. Net semantics — reuse iff `24h < remaining_validity < days`: a too-long legacy leaf OR a near-expiry leaf both regenerate; a correctly-sized leaf is reused.
**Refinement (@code-reviewer follow-up, @security invariant co-tracked)**: the initial fix used a `+ 43200` (12h) slack, which would permit reusing a leaf with up to 14d12h remaining — literally violating @security's invariant "no reused MC/MH leaf has >14d remaining validity" (which @security re-verifies by probing the upper bound). Dropped the slack. It was not load-bearing for no-thrash: a leaf's reuse-check never runs in the same invocation that minted it (setup.sh's two `generate` calls are seconds apart), so a fresh leaf always has remaining strictly `< days` at reuse-check → still reused. Regenerating exactly at the boundary is the safe direction (also covers a backward clock skew). Reuse now strictly implies remaining `< days` ≤ 14d. A code comment documents that the guard bounds REMAINING validity as a **proxy** for the validity WINDOW (@security's option (a)): it catches the common legacy case; the residual ~year-old-365d-window-decayed-to-≤14d-remaining edge is fail-closed (Chrome refuses the >window pin → dev re-runs → self-corrects). We deliberately avoid `notAfter-notBefore` date math (GNU-vs-BSD `date -d` fragility) for that non-biting edge.
Verified empirically (both the initial fix and the refinement): a simulated legacy 365-day mc/mh leaf is regenerated to exactly a 14-day window on a no-`--force` run; auth-localhost (intended window 365d) is still correctly reused; fresh 14-day leaves remain reused on rerun (no-thrash preserved); the `≤14d` invariant probe (`openssl x509 -checkend $((14*86400))` on each reused leaf returns "will expire" = reusable) passes for mc/mh; shellcheck still CLEAN, `bash -n` OK.

---

## Accepted Deferrals

- **Auto rollout-restart on cert-secret recreate → R-49 (task #20)** (@operations-agreed at planning; durably recorded in "Known limitations / R-49 handoff" + a `docs/TODO.md` line). On a running cluster, regenerating an expiring WT leaf + recreating the `mc/mh-service-tls` Secret does not restart the pods serving the old leaf; the operator runs the manual sequence printed by the expiry hint. Automating it is a behavior change to shared `setup.sh` firing on all runs — out of R-36 scope.
