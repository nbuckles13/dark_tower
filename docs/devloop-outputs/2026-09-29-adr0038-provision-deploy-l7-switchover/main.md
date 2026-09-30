# Devloop Output: ADR-0038 step 3 — provision/deploy verbs + Layer 7 switch-over

**Date**: 2026-09-29
**Task**: ADR-0038 Implementation step 3 — `provision` / `deploy` verbs + Layer 7 switch-over
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/adr0038-task3`
**Duration**: ~5h45m (2026-09-29 20:45 → 2026-09-30 02:35)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e0f40a8a6cd69c45ecfa8c8eb19ac91cb0a3f794` |
| Branch | `feature/adr0038-task3` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired Media Handler | `paired-media-handler` (Domain-judgment: MH config gauges for env-test 01) |

---

## Task Overview

### Objective
Implement ADR-0038 §Implementation step 3 (`docs/decisions/adr-0038-dev-cluster-deploy-parity.md`):
`dev-cluster provision` (deterministic blueprint render shared by build and check, hash recorded
in-cluster after success) and `dev-cluster deploy` (apply the one Kind environment root);
helper allowlist reduced to the verbs; `layer7.sh` becomes `provision && deploy && tests`;
`rebuild-all`, the diff-based infra trigger, and story-2 task 10's two interim diff-based arms
retired; interim Succeeded-pod delete removed; verb rename carried to all three remedy spellings;
env-tests 01/26/28 made fresh-pod-independent; the 2026-09-03 TODO and the two collector TODOs
closed; `gc-deployment.md` collector procedures rewritten (operations decision on config-only exemption).

### Scope
- **Service(s)**: devloop helper, Kind setup, Layer 7, env-tests, runbooks
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — ADR-0038 is the decision record.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/kind/scripts/provision.sh` (new: the provision implementation + blueprint render/check/record) | Mine | — |
| `infra/kind/scripts/deploy.sh` (new: the deploy implementation, moved out of setup.sh) | Mine | — |
| `infra/kind/scripts/lib/common.sh` (new: shared prelude, moved out of setup.sh) | Mine | — |
| `infra/kind/scripts/setup.sh` (shrinks to host entry: provision + deploy + host extras; `--provision-org`) | Mine | — |
| `infra/kind/scripts/teardown.sh` (sources lib/common.sh for logging; own charset validator kept) | Mine | — |
| `infra/kind/scripts/iterate.sh` (sources lib/common.sh for logging) | Mine | — |
| `scripts/setup.test.sh` (re-pointed at the three scripts; blueprint + deploy cases) | Mine | — |
| `crates/devloop-helper/src/protocol.rs` (wire surface: verbs + `Request` fields) | Not mine, Domain-judgment | security |
| `crates/devloop-helper/src/commands.rs` (`cmd_provision`/`cmd_deploy`, one script-command builder, recreate, status) | Mine | — |
| `crates/devloop-helper/src/ports.rs` (drop the dead `observability_deployed` input) | Mine | — |
| `crates/devloop-helper/src/main.rs` (socket tests: retired verbs / argument fields rejected) | Mine | — |
| `crates/devloop-helper/src/error.rs` (invalid_command message lists VERBS; retired InvalidService) | Mine | — |
| `crates/devloop-helper/src/logging.rs` (comment/test verb names) | Mine | — |
| `crates/dt-guard/src/kustomize.rs` (comment: deploy.sh applies the root) | Mine | — |
| `crates/dt-guard/src/kustomize_configmaps.rs` (comment: the one root apply) | Mine | — |
| `.dockerignore` (comment: build path citation) | Mine | — |
| `infra/devloop/dev-cluster` (client verbs + help) | Mine | — |
| `infra/devloop/devloop.sh` (eager bring-up = `provision` then `deploy`; stale comments) | Not mine, Minor-judgment | operations |
| `scripts/layer7.sh` (Phase 1 = provision, deploy; diff arms + `__apply_observability_overlay` retired; token renames) | Mine | — |
| `scripts/layer7.test.sh` | Mine | — |
| `scripts/layer-all.test.sh` (stub token spelling only) | Mine | — |
| `scripts/guards/simple/validate-dev-cluster-verbs.sh` (new drift guard) | Mine | — |
| `scripts/guards/validate-dev-cluster-verbs.test.sh` (new self-test) | Mine | — |
| `scripts/layer3.sh` (wires the guard self-test) | Mine | — |
| `crates/env-tests/src/fixtures/kube.rs` (`REDEPLOY_HINT`, `FRESH_CLUSTER_HINT`) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` ((c) startup-log parity made fresh-pod-independent) | Not mine, Domain-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` (wedged-generation remedy) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (ratchet: current occupancy as baseline) | Not mine, Domain-judgment | test |
| `crates/env-tests/tests/00_cluster_health.rs` (moved-function citation) | Not mine, Mechanical | test |
| `crates/env-tests/src/fixtures/auth_client.rs` (moved-function citation) | Not mine, Mechanical | test |
| `crates/env-tests/src/fixtures/mh_grpc.rs` (moved-function citation) | Not mine, Mechanical | test |
| `crates/env-tests/tests/31_gc_telemetry.rs` (O3: settled per-instance baseline) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/35_mc_server_mute_teardown.rs` (O3: settled baselines) | Not mine, Minor-judgment | test |
| `crates/mh-service/src/observability/metrics.rs` (six config gauges in `publish_egress_admission`) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/src/main.rs` (startup-event comment: no longer a test contract) | Not mine, Domain-judgment | media-handler |
| `crates/mh-service/tests/stream_admission_integration.rs` (gauge == enforcing config field, distinct fixture values) | Not mine, Domain-judgment | media-handler |
| `docs/observability/metrics/mh-service.md` (catalog entries + "every gauge publish_egress_admission sets") | Not mine, Domain-judgment | observability |
| `infra/grafana/dashboards/mh-media.json` (three config-gauge panels) | Not mine, Domain-judgment | observability |
| `scripts/generate-dev-certs.sh` (`--check-renewal`: read-only "would renew" predicate) | Mine | — |
| `infra/kind/scripts/lib/cluster-db.sh` (new, NOT in the blueprint: `dt_psql`, `DT_PG_*`, org-cap validation) | Mine | — |
| `docs/runbooks/gc-deployment.md` (§OTel Collector Upgrade Discipline rewrite; banner removed) | Not mine, Domain-judgment | operations |
| `docs/runbooks/ac-service-deployment.md` (retired verb/flag spellings; moved-function citations) | Not mine, Mechanical | operations |
| `docs/runbooks/ac-service-incident-response.md` (retired verb/flag spellings; moved-function citations) | Not mine, Mechanical | operations |
| `docs/runbooks/gc-incident-response.md` (retired verb/flag spellings; moved-function citations) | Not mine, Mechanical | operations |
| `docs/runbooks/mc-deployment.md` (retired verb/flag spellings; moved-function citations) | Not mine, Mechanical | operations |
| `docs/runbooks/mh-deployment.md` (retired verb/flag spellings; moved-function citations) | Not mine, Mechanical | operations |
| `docs/runbooks/mh-incident-response.md` (retired verb/flag spellings; moved-function citations) | Not mine, Mechanical | operations |
| `docs/runbooks/client-dev-local.md` (retired verb/flag spellings; moved-function citations) | Not mine, Mechanical | operations |
| `docs/runbooks/devloop-validation.md` (§6.7 tokens + sub-causes; §8 self-heal text) | Mine | — |
| `docs/observability/dashboards.md` (deploy command spelling) | Not mine, Mechanical | observability |
| `packages/web-app/README.md` (`dev-cluster setup` mention) | Not mine, Mechanical | client |
| `packages/web-app/e2e/README.md` (`--skip-observability` mention) | Not mine, Mechanical | client |
| `packages/web-app/e2e/global-setup.ts` (Prometheus-missing message) | Not mine, Mechanical | client |
| `packages/web-app/vite.config.ts` (moved-function citation) | Not mine, Mechanical | client |
| `.claude/skills/devloop/SKILL.md` (§Recover-a-dead-cluster verbs) | Not mine, Mechanical | team-lead |
| `docs/decisions/adr-0038-dev-cluster-deploy-parity.md` (status, step-3 notes, Open Question resolved) | Mine | — |
| `docs/decisions/adr-0030-host-side-cluster-helper.md` (allowlist table + write-command list) | Not mine, Domain-judgment | security |
| `docs/decisions/adr-0033-*.md` (Layer 7 text; `_changed_helpers` consumer list) | Mine | — |
| `docs/decisions/adr-0013-local-development-environment.md` (host command example) | Mine | — |
| `docs/LOCAL_DEVELOPMENT.md` | Mine | — |
| `docs/TODO.md` (close 2026-09-03 L7 entry, two collector entries, gc-deployment.md entry, L7 teardown busy-tolerance entry; status note on the TLS-Secret entry) | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/operations/INDEX.md` (moved-function pointers) | Not mine, Mechanical | operations |
| `docs/specialist-knowledge/database/INDEX.md` (moved-function pointers) | Not mine, Mechanical | database |
| `docs/specialist-knowledge/auth-controller/INDEX.md` (line-number citation → symbol) | Not mine, Mechanical | auth-controller |
| `infra/kubernetes/overlays/kind/kustomization.yaml` (header: deploy.sh applies it) | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` (stale restart comment) | Mine | — |
| `infra/kubernetes/observability/prometheus.yml` (comments: deploy's rollout wait) | Mine | — |
| `infra/services/**` (comments / citations; Gate-3: GC `DATABASE_URL` and MC `REDIS_URL` composed from the owner Secrets with dependent env vars, literals deleted; `redis-secrets` becomes a content-addressed `secretGenerator`; postgres/secret.yaml ownership header) | Mine | — |
| `crates/env-tests/src/fixtures/metrics.rs` (Gate-3: the one `settled_baseline`) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mh_config.rs` (new, Gate-3: `gauge_verdict` + its unit test at Layer 4) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/mod.rs` (registers mh_config) | Not mine, Mechanical | test |
| `crates/env-tests/tests/34_mc_kek_rotation.rs` (Gate-3: uses the shared `settled_baseline`) | Not mine, Mechanical | test |
| `scripts/guards/common.sh` (Gate-3: `guard_seam_root`, the one DEVLOOP_TEST scan-root seam) | Mine | — |
| `scripts/guards/simple/dev-cluster-verbs-allow.txt` (new: per-entry allowlist for the retired-spelling denylist) | Mine | — |
| `scripts/guards/simple/validate-internal-proto-no-key-material.sh` (uses `guard_seam_root`) | Mine | — |
| `scripts/guards/simple/validate-subdomain-regex-sync.sh` (uses `guard_seam_root`) | Mine | — |
| `scripts/guards/validate-subdomain-regex-sync.test.sh` (fixture copies common.sh) | Mine | — |
| `infra/docker/db-migrate/Dockerfile` (comments + the required-arg message) | Mine | — |
| `infra/docker/ac-service/README.md` (built-by citation) | Mine | — |
| `infra/lib/cargo-lock-version.sh` (consumer comment) | Mine | — |
| `scripts/dev-web.sh` (moved-function citations) | Mine | — |

---

## Planning

*Revision 1 (implementer, for Gate 1).*

### Problem, in mechanism terms

Layer 7 decides what to do to the cluster from `git diff merge-base(origin/main, HEAD)` — teardown+setup
on an `infra/kind/` diff, `deploy <svc>` on a service-manifest diff, a container-side observability apply on an
observability diff — and otherwise runs `rebuild-all`. Each rule is history-dependent (ADR-0038 Context), and
the cluster carries no record of what it was built from, so a half-built cluster is indistinguishable from a
good one (step 2's Gate-2 attempt 2). The fix is two idempotent operations keyed on CONTENT compared against
what the cluster RECORDS: `provision` (platform; destroy+rebuild only when the recorded blueprint differs or is
missing) and `deploy` (application; converge every run). Layer 7 = `provision && deploy && tests`, no diff.

### A. File split — why three scripts (and a lib), not one setup.sh

The blueprint hashes the provisioning IMPLEMENTATION by whole file (ADR §1: "no hand-kept list"). If the deploy
code stayed in the same file, every edit to the deploy path (tags, Job, root apply) would change the blueprint
and destroy the cluster: correct but the slow rebuild ADR-0038 exists to remove. So the implementation is split
by LIFECYCLE:

| File | Contents | In the blueprint? |
|---|---|---|
| `infra/kind/scripts/lib/common.sh` (new) | cluster-name validation, `DT_PORT_MAP` sourcing, gateway validation, `KUBE_CONTEXT`/`KUBECTL`, logging, `container_cmd`, runtime detect/prereqs, `load_image_to_kind`, `extract_manifest_images`, `dt_psql` + `DT_PG_*`, `ORG_MAX_CONCURRENT_MEETINGS` validation | yes (provision sources it) |
| `infra/kind/scripts/provision.sh` (new) | blueprint render/check/record, create cluster (from `DT_KIND_CONFIG`), Calico, namespaces, secrets + TLS (the dev stand-in for a secret manager) | yes (itself) |
| `infra/kind/scripts/deploy.sh` (new) | third-party preload, content-tagged builds, Postgres/Redis, migration Job, seeds, collector, root apply, wait, prune | **no** |
| `infra/kind/scripts/setup.sh` (shrinks) | host one-stop entry: `provision.sh` → `deploy.sh` → port-forwards / Telepresence / access info; `--provision-org` (per-run org, container-runnable) | no |

`setup.sh` keeps its host meaning ("bring my dev cluster up / up to date") so the many host docs saying "run
`./infra/kind/scripts/setup.sh`" stay true; it is no longer what a gate runs. `--provision-org` stays in setup.sh
(layer7's `SETUP_SH` seam, the subdomain-regex guard site and its citations are unchanged); it is neither
platform nor application. **ADR wording deviation, recorded in ADR-0038:** the ADR says "setup.sh shrinks to the
implementation behind provision"; the implementation behind provision is `provision.sh`, split out so the
blueprint can hash it by whole file.

Retired flags: `--only`, `--rebuild-all`, `--skip-build` (their only callers were the retired helper verbs).
New: `provision.sh [--yes] [--check] [--blueprint]`, `deploy.sh [--yes]`. The code motion is mechanical
(functions move verbatim except where §B–§D change them); `validate-doc-citations-symbol-resolves` catches every
`setup.sh:<fn>` citation left pointing at a moved function, and I update them all (list in the table above).

### B. Provision (`provision.sh`)

**Render** (`blueprint_render`, pure: reads files, runs `kind version`, touches no cluster, writes nothing). A
line-oriented manifest, one input per line, so a mismatch can name WHAT changed:

```
dt-blueprint v1
kind-config sha256:<content of $DT_KIND_CONFIG>        # the helper's rendered per-slug config, or infra/kind/kind-config.yaml on the host
kind-version <`kind version` output>                    # pins the default node image
provider <podman|docker>
file infra/kind/scripts/provision.sh sha256:…
file infra/kind/scripts/lib/common.sh sha256:…
file scripts/generate-dev-certs.sh sha256:…             # the secret/TLS RECIPE
tls-cert infra/docker/certs/{ca,mc-webtransport,mh-webtransport}.crt sha256:…   # PUBLIC certs (see below)
```

- **Implementation files are not a hand-kept list.** `provision.sh` sources its libs and invokes its recipe
  scripts only through ONE array (`PROVISION_INPUTS`) that the render also iterates; `setup.test.sh` pins, statically,
  that provision.sh has no `source`/`.`/`${PROJECT_ROOT}/…sh` call outside that array. Calico's version is a
  constant in provision.sh, so it is covered by the file hash.
- **Recipe, never key bytes.** Secret generation inputs are hashed (`generate-dev-certs.sh`, and the AC secret
  literals, which live in provision.sh). No `.key` file and no Secret value is read.
- **Why the public certs are in (my addition — reviewers, please check).** The MC/MH WebTransport leaves are
  14-day certs (Chrome's `serverCertificateHashes` cap); `generate-dev-certs.sh` renews a leaf within 24h of
  expiry. Today renewal rides on every full setup. Once provision runs rarely, a cluster older than 14 days would
  serve an EXPIRED leaf and the browser suite would fail for a reason no diff explains. The expiry is a time
  input the recipe alone cannot capture. So provision first MATERIALIZES the host inputs (runs the idempotent
  `generate-dev-certs.sh`, which renews only when due), then renders; the render carries each public `.crt`'s
  sha256 (not key bytes, and stable between renewals — it is not a random-per-render product). A renewal changes
  the blueprint → one rebuild every ~13 days, and a host `--force` regeneration can never leave the cluster's
  TLS Secret out of step with `fingerprints.json`. `--blueprint` (print only) does NOT materialize — it stays pure.
- The render is hashed with `sha256sum` host-side. **ADR wording:** "computed host-side by the helper" — here it
  is computed host-side by the provision implementation the helper runs; one implementation then serves the
  helper path AND the host `setup.sh` path (a Rust compare plus a shell compare would be two copies of the rule).

**Check / record.** `provision.sh` (helper path: `--yes`):
1. materialize inputs; render; hash.
2. Cluster absent → build. Cluster present → read `kube-system/configmap/devloop-blueprint` (`--ignore-not-found`):
   - equal → `BLUEPRINT unchanged <hash>`; exit 0 (no-op; nothing else runs).
   - absent/different → print the section-level diff against the recorded manifest (`BLUEPRINT changed:
     kind-config, file infra/kind/scripts/provision.sh`) or `BLUEPRINT missing (half-built or pre-ADR-0038
     cluster)`, then `kind delete cluster` and build. Interactive TTY without `--yes`: prompt; `N` exits 1 (it
     never proceeds onto a mismatched platform).
   - the record cannot be READ (kubectl error other than not-found) → fail loudly (`REASON=blueprint-unreadable`),
     never destroy on uncertainty (same fail-closed rule as `recreate`); remedy names `dev-cluster recreate`.
3. Build = `kind create cluster --config $DT_KIND_CONFIG` → Calico → namespaces → AC secrets, MC/MH TLS Secrets.
4. **Only then** re-render and compare with step 1's hash (a tree edit during the build fails loudly rather than
   recording a blueprint that was not built), and `kubectl create configmap devloop-blueprint -n kube-system`
   with `hash` and `manifest`. A failure anywhere before this leaves no record → the next provision rebuilds.
   That is the half-built-cluster fix; the check-health-then-reuse order and `create_cluster`'s "exists,
   reusing" branch (and `warn_port_mapping_cliff`) are deleted — provision never reuses a cluster it did not
   record.

`provision.sh --check`: steps 1–2 read-only → `BLUEPRINT_MATCH` rc 0, or `BLUEPRINT_STALE … REASON=blueprint-stale|blueprint-missing`
rc 1. `deploy.sh` runs it first (§C), so a host `deploy.sh` onto a stale/half-built platform fails loudly naming
`provision`; one render serves check, build and deploy's guard.

**NON-COLLAPSE:** provision MUST destroy a healthy cluster whose blueprint changed; `recreate` MUST refuse to
destroy a healthy one. The reciprocal comment moves from layer7's retired infra arm to `provision.sh`.

### C. Deploy (`deploy.sh`) — every run, identically

`--check blueprint` → prereqs → **preload third-party images missing from the node** (`crictl inspecti` in the
node; today every full setup reloads all of them) → **build every first-party image** (content tag; **load into
Kind only when the node lacks that ref** — the ref is content-addressed, so presence means same bytes; this is what
keeps an unchanged deploy cheap) → Postgres + Redis (subset apply, wait) → migration Job → seeds (idempotent `ON
CONFLICT`) → collector (subset apply, `rollout status`; R-54 ordering) → root apply → wait for every workload → prune.

- **No unconditional restart** anywhere; content-addressed ConfigMaps + content tags roll exactly what changed.
- **Ref resolution simplifies.** Deploy always builds, so every ref is the one built this run; the
  built > deployed > `IMAGE_UNRESOLVED` chain and its `--skip-build` fallback are dead and are deleted. The
  deployed-ref reader stays (prune needs the pre-apply refs; a read failure keeps `REASON=image-ref-read-failed`).
  `render_env_overlay` already refuses a root repo without a content ref, so the invariant keeps a loud check.
  This removes the `IMAGE_UNRESOLVED` remedy site rather than renaming it; ADR notes say so.
- **Interim Succeeded-pod delete removed** from `run_migration_job` (ADR carry). The pod and its logs now stay;
  the next run's old-Job prune removes them with their Job. `parse_pod_health` already exempts them.
- **`delete_retired_resources` (grafana-sidecar RBAC) removed:** its comment says "remove once every cluster has
  been rebuilt by provision" — and every existing cluster HAS no blueprint record, so its first provision
  rebuilds it. Nothing can carry that Role forward.
- The collector's pre-apply now runs on EVERY deploy, not only full setup: a collector change rolls (and is
  Ready-gated) before the services the same deploy rolls.

### D. Helper (ADR-0030 trust boundary — @security)

Verbs after: **`provision`, `deploy`, `teardown`, `recreate`, `restore-kubeconfig`, `status`, `cancel`.** All
take NO argument. Retired: `setup`, `rebuild <svc>`, `rebuild-all`, `deploy <svc>`.
- `Request` shrinks to `{token, command}` with `deny_unknown_fields`: a client sending `service` or
  `skip_observability` is rejected at deserialization. `Service` enum, `reject_all_args` (now structurally
  implied: there is no argument field to accept) and the `--skip-observability` path go. (`setup
  --skip-observability` was already broken end to end: setup.sh rejects the flag.)
- ONE builder `script_command(ctx, Script::{Provision, Deploy})` replaces `setup_sh_command`: fixed path + fixed
  flags (`--yes`) + env `DT_CLUSTER_NAME`, `DT_PORT_MAP`, validated `DT_HOST_GATEWAY_IP`, `DT_KIND_CONFIG`
  (helper-rendered path) + provider. No container-supplied string reaches argv or env.
- `cmd_provision`: allocate ports → render kind-config → write ports.json + port-map.env → verify ports only if
  the cluster does not exist (an existing cluster holds its own ports; if provision rebuilds it, `kind create`
  fails loudly on a foreign holder) → `provision.sh` → container kubeconfig. The helper no longer runs `kind
  create` itself (provision.sh does, from the same rendered file it hashed).
- `cmd_deploy`: `deploy.sh`.
- `cmd_recreate` = teardown + provision + deploy (was teardown + setup); the bound/re-confirmation logic is
  untouched. `read_observability_deployed` goes (observability is always in the root).
- `status`: `setup_in_progress` (wire name kept — layer7 greps `Setup in progress:`) becomes true for any
  cluster-building write: `provision | deploy | recreate`, so layer7's busy-retry waits out an eager deploy too.
- Tests: argv/env of both scripts identical except path; unknown-field rejection for `service` and
  `skip_observability`; every retired verb → `InvalidCommand`; streaming failure/cancel through the real
  `run_command_streaming` (reusing step 2's stub pattern); `setup_in_progress` for each op.

### E. Layer 7 (`scripts/layer7.sh`)

Phase 1 becomes: helper gate (unchanged) → **(a) `provision`** (busy-tolerant `__dev_cluster_write`) →
**(b) `deploy`** (same) → (d) ports.json → (e) health wait + self-heal → (f) observability readiness → (h) org
→ (g) browser preconditions → (i) MH forwards. Deleted: the `__cluster_ready`-then-setup reuse order, the
`infra/kind/` teardown arm, the per-service `deploy <svc>` arm (c0), `rebuild-all` (c), the container-side
observability apply (e2) and `__apply_observability_overlay` (no container-side cluster write remains), the
`_get_base_ref.sh` call and the `_changed_helpers.sh` source (layer7 inspects no diff).
- Tokens: `cluster-setup-failed` → `cluster-provision-failed`, `cluster-rebuild-failed` → `cluster-deploy-failed`,
  `observability-apply-failed` retired. STEP names: `provision`, `deploy`. The self-heal's rollout-wedged text
  says "after deploy". The health wait stays (it checks the HELPER's view after deploy's own rollout wait); its
  "helper older than devloop 2" rationale is dropped.
- `docs/TODO.md` "layer7 teardown has no busy-tolerance" closes: there is no teardown call left; both writes go
  through the one busy-tolerant path.
- layer7.test.sh: fake dev-cluster models `provision`/`deploy` (its unknown-verb catch-all stays loud, so a
  stray `rebuild-all` reds); cases: provision fails → `cluster-provision-failed`; deploy fails →
  `cluster-deploy-failed`; order is provision then deploy, exactly once each (marker log); busy on either →
  poll + one retry; an `infra/kind/` / service-manifest / observability diff triggers NOTHING extra (static:
  no `diff_touches_path`/`_get_base_ref` left in layer7.sh; and a run with a staged `infra/kind/` change
  calls exactly provision+deploy); the old e2 cases deleted.

### F. Remedy spellings + drift guard

| Site | After |
|---|---|
| `kube.rs` `REDEPLOY_HINT` | `` `dev-cluster deploy` (devloop container) or `./infra/kind/scripts/deploy.sh` (host) `` |
| `kube.rs` `FRESH_CLUSTER_HINT` | per @test's ruling on §G; if it survives: `` `dev-cluster teardown`, then `dev-cluster provision` and `dev-cluster deploy` (or re-run Layer 7, which provisions a missing cluster); host: teardown.sh && setup.sh `` |
| setup.sh `IMAGE_UNRESOLVED` | deleted with the dead fallback (§C) |
| migration checksum remedy (deploy.sh) | recreate = `dev-cluster teardown` + `provision` + `deploy` |
| devloop-validation.md §6.7 | rows rewritten for the new tokens/sub-causes |

**Guard `validate-dev-cluster-verbs.sh`** (Layer 3, auto-discovered + self-test wired in layer3.sh). They can't
share a constant, so the guard enforces the vocabulary instead of the sites: SSoT = the client's accepted verbs
(`infra/devloop/dev-cluster`'s case arms). (1) the helper's `parse_command` accepts exactly that set; (2) every
`dev-cluster <word>` spelled in live code/docs (crates, scripts, infra, runbooks, skills, LOCAL_DEVELOPMENT,
packages; excluding history: devloop-outputs, debates, user-stories, decisions, TODO.md) is a real verb; (3) every
`{setup,provision,deploy}.sh --<flag>` spelled there is a flag that script's parser accepts. Anti-vacuity: pinned
non-zero counts per enumerated remedy site (kube.rs, §6.7) and a failing self-test for each rule (retired verb,
retired flag, helper/client set mismatch, empty site).

### G. Env-tests 01 / 26 / 28 (Domain-judgment — @test, please rule)

- **28 (ratchet):** already counts admissions as deltas from a baseline. The fresh-pod assumption is the first-join
  503 ("handlers full before start") and the sizing against the ABSOLUTE ceiling. Proposal: read each instance's
  current `mh_media_egress_edges` as the occupancy baseline and size the meeting to overshoot the REMAINING
  headroom (`ceiling - occupied`, per the handler GC places on); the 503 message becomes a genuine precondition
  ("a meeting never released its streams: MC never completed EndMeeting") without a fresh-cluster remedy.
- **01 (c) startup-log parity:** the startup event is lost to kubelet log rotation on a long-lived pod. Options:
  (i) read the event from Loki for that pod UID when `kubectl logs` no longer has it (Loki is optional in the
  crate — would need a defined no-Loki outcome); (ii) compare against MH's published config gauges where they exist
  (`mh_media_egress_edges_limit`, `…registered_meetings_limit`, budget, ceiling) and drop the other rows from (c);
  (iii) `kubectl logs --since-time=<pod start>` doesn't help (rotation). My preference: (i) with (ii) as the
  primary for the rows that have a gauge. Your call.
- **26 (wedged generation):** since task 12 a wedge self-heals (MC's EndMeeting carries its own mc_id) and the id
  is a fresh meeting per run, so the test already doesn't depend on a fresh pod; only the remedy text changes
  (drop the fresh-cluster remedy; name the gRPC `EndMeeting` recovery).
- If none of the three still needs a fresh cluster, `FRESH_CLUSTER_HINT` is deleted.

### H. Operations decisions (@operations, please rule)

1. **Collector config-only exemption** from the separate-change-window rule (gc-deployment.md). Facts for the
   ruling: a config-only change now renames the ConfigMap and rolls the singleton on `deploy`; the rollout is
   `maxUnavailable 0 / maxSurge 1` Ready-gated on `:13133`, so a config the collector REJECTS still stalls with
   the old pod serving (the exemption's original reason survives); but a config that STARTS yet stops serving
   4317 goes Ready, the old pod is terminated, and AC/GC/MC fail init on their next restart — the image-change
   hazard. Also: `deploy` rolls the collector (and waits) before the services in the same run. I'll write the
   §Config-only changes / §Rollback / §Mitigation text to your ruling and delete the "silently stale" procedure
   (uid/startTime/managedFields legs) — content addressing retired that hazard, and the same retirement already
   happened to env-test 01(d).
2. Layer 7 token renames (`cluster-provision-failed`, `cluster-deploy-failed`; `observability-apply-failed`
   retired) and devloop.sh's eager path (`dev-cluster provision && dev-cluster deploy`).
3. Cost profile: provision rebuild (~6 min) only on blueprint change, incl. a TLS renewal every ~13 days per
   long-lived cluster; every existing cluster rebuilds ONCE on its first post-merge provision (no record).

### I. ADR-0038 open question — platform components: **the root (deploy)**

Postgres, Redis, the OTel collector and the observability stack stay in the environment root: normal tasks change
them (every story-2 dashboard/alert/scrape/collector-allowlist task), they are content-addressed so an unchanged
apply is a no-op (Postgres does not restart), and they already are root resources. Provision holds only what cannot
change in place: node topology/ports (kind config), CNI, namespaces, secret material. Recorded in the ADR.

### J. Docs

ADR-0038 (status → Accepted-with-steps-done? — I'll set "Implemented" only if the Lead agrees; step-3 notes: file
split, render contents incl. TLS certs, IMAGE_UNRESOLVED deletion, open question resolved), ADR-0030 (verb table,
write-command list, no-argument wire surface), ADR-0033 (Layer 7 row; `_changed_helpers` consumer list),
ADR-0013 example, LOCAL_DEVELOPMENT.md (host flow `setup.sh` = provision+deploy; `deploy.sh` to redeploy;
`provision.sh --blueprint`), devloop-validation.md §6.7/§8, SKILL.md recovery steps, runbooks (mechanical
spellings), INDEX files, TODO closures (2026-09-03 L7; "No collector deploy route"; "Collector config-staleness
window"; "gc-deployment.md's OTel-collector change procedures"; "layer7 teardown busy-tolerance").

### Risks / host actions

- **R1 — ADR-0030 corollary, and it bites THIS devloop's Gate 2 (@team-lead).** The helper is built from the host
  checkout; the new layer7 calls `provision`/no-arg `deploy`, which the running (old) helper rejects
  (`InvalidCommand` → `PRECONDITION_FAILURE cluster-provision-failed`). Gate 2 Layer 7 needs the helper rebuilt
  from THIS tree first (host: rebuild/restart the helper for this slug — devloop.sh relaunch). I will not add
  old-verb fallbacks to layer7 (that would be a masked compatibility path). Every existing cluster then rebuilds
  once (no record).
- R2 — the helper paths are unit-tested; the provision/deploy scripts are hermetically tested with stubbed
  kind/kubectl/podman; the live proof is Gate 2.
- R3 — `setup.test.sh` runtime (Layer 3 budget already WARNs at ~130s); I'll keep new cases stub-only (no extra
  real kustomize renders beyond what moved).

### Sizing

One devloop, but large (helper protocol, three scripts, layer7, guard, env-tests, ~20 docs). I believe it fits
one session; the only piece I'd consider splitting is §G if @test's ruling on 01 needs new MH instrumentation
(e.g., config gauges MH doesn't publish) — that would be media-handler/observability work and I'd surface it
rather than build it here.

### Revision 2 — Gate-1 input folded in (supersedes rev 1 where they differ)

**Security (S1–S8 + corollary)**
- S1: the final `HelperCommand` set is `provision`, `deploy`, `teardown`, `recreate`, `restore-kubeconfig`,
  `status`, `cancel`. Recovery stays because layer7's self-heal still uses both. `Setup`, `Rebuild`,
  `RebuildAll` and `Deploy(Service)` are DELETED, so they parse as `invalid_command`, and no aliases are kept.
  Both new verbs are write verbs (write slot and cancel). They take zero arguments. The mechanism is stronger
  than `reject_all_args`: `Request` has no argument fields left (`{token, command}`, `deny_unknown_fields`), so
  a stray `service` or `skip_observability` fails at deserialization, before parse and before any exec.
  `--skip-observability` is not kept. Parse tests:
  - `deploy` with `service:"gc"` is rejected;
  - `provision` with `skip_observability:true` is rejected;
  - an unknown extra field is rejected;
  - a command carrying metacharacters or control characters is rejected;
  - each retired verb gives `invalid_command`;
  - `provision` and `deploy` are accepted and are writes.
  ADR-0030's API table is updated in the same change.
- S2: the builder uses fixed argv literals. The env comes only from `ctx` (slug-derived cluster name, the helper's
  own port-map and kind-config paths, the validated gateway, the provider).
- S3: the exact render inputs are listed in §B. Nothing under `infra/docker/certs/` is read except the two PUBLIC
  leaf certs the TLS Secrets carry (`mc-webtransport.crt`, `mh-webtransport.crt`). No `.key`, no CA key, no
  `.env`, no Secret value, no `fingerprints.*`. The CA is not needed, because a CA regeneration regenerates both
  leaves. Why these two certs are hashed is in §B (expiry). **Record content (please confirm):** `hash` (full
  sha256) plus `manifest`. The manifest is the render lines: section labels such as `kind-config`,
  `file infra/kind/scripts/provision.sh` and `tls-cert mc-webtransport.crt`, each with a sha256 of NON-secret
  content. Observability's `CHANGED=` needs these, and none of them is secret material or a path to secret
  material. If you want the record to hold the digest only, I drop `CHANGED=` to `-`.
- S4: the record is written as the last step, after the re-render matches. Cancel or failure leaves no record.
  A missing or unparseable record rebuilds. A record that cannot be READ (a kubectl error other than not-found)
  fails loudly and neither skips nor destroys. The compare is an exact string compare.
- S5: secret and TLS creation run in provision only. deploy never creates, regenerates or overwrites them.
  Validation, the 0400 mounts and the no-`set -x` rule are unchanged. The creation code moves verbatim.
- S6: after this change layer7 has zero container-side kubectl writes other than the unchanged `--provision-org`.
  The env-test changes are read-only (gauges and `kubectl get`).
- S7: confirmed. `parse_pod_health` exempts a pod only when `phase==Succeeded` AND it has a `kind: Job`
  ownerReference.
- S8: the remedy strings name only live verbs, and the drift guard (§F) enforces it.
- **Corollary / version skew** (also operations Q3): the old helper answers `invalid command: provision
  (invalid_command)`. layer7's `__dev_cluster_write` callers recognise `(invalid_command)` and fail with
  `PRECONDITION_FAILURE helper-verb-unsupported`, remedy: "the host helper predates this tree's verbs: rebuild
  and restart it (re-run infra/devloop/devloop.sh on the host)". There is no fallback to old verbs. The token is
  reachable (skew), so it is not the dead-token class. In the other direction, an old client talking to a new
  helper gets `invalid_command`, and the new helper's message lists the valid verbs. `preflight-story.sh` has no
  helper verb-currency check today (grepped). I'm not adding one: layer7's token is where skew surfaces.

**Semantic guard.** No secret, key or kubeconfig bytes go to logs or errors. The helper streams the scripts'
output as before, and the scripts never echo secret YAML (the existing `create … --dry-run -o yaml | apply -f -`
pipes stay unprinted). Every new `map_err` carries `e`.

**Observability**
- O1/(3):
  - `wait_for_env_root` prints one line: `DEPLOY_FAILED REASON=rollout-failed WORKLOADS=<ns/kind/name,…>`.
  - The collector gate prints `DEPLOY_FAILED REASON=collector-rollout-failed WORKLOADS=dark-tower/deployment/otel-collector`.
  - layer7 lifts the `DEPLOY_FAILED` line into the `cluster-deploy-failed` cause.
  - `observability-apply-failed` is removed from both layer7.sh and devloop-validation.md.
  - The `cluster-deploy-failed` row carries the Prometheus hint (`logs deploy/prometheus`) and the Grafana hint
    (`describe pod -l app=grafana`).
- O2/(2): every provision run (including `--check`) prints ONE line:
  `BLUEPRINT ACTION=none|rebuild|check REASON=match|missing|changed|unreadable RECORDED=<hash12|none> CURRENT=<hash12> CHANGED=<sections|->`.
  - It appears on helper stderr (helper.log) and in the Layer-7 stream.
  - `--check`'s REASON uses the same vocabulary.
  - hash12 is the first 12 hex of the one `sha256sum`.
  - STEP timings: `provision` and `deploy` separately.
- O3 fresh-state audit (all env-tests; per-file verdicts below, fixes by @test's ruling):

  | File | Verdict |
  |---|---|
  | 00, 10, 20–25, 27, 32, 33, 34, 40 | SAFE (presence-only / per-run org+users / settled deltas) |
  | 29 | SAFE; leaked-registration accumulation → covered by the registered-meetings PRECONDITION added for 26 |
  | 30 `zero_initialized_counter_is_present_at_zero_on_a_running_pod` | **SAFE — NO CHANGE (rev 2.2, @test ruling)**: `mc_actor_panics_total{actor_type="controller"}` cannot be non-zero on a live pod (the controller is the supervision root; its panic kills the process), and present-AT-ZERO is the subject — a baseline check would accept an increment-created series, the zero-init regression it guards |
  | **26** :1100 `non_match_baseline` read once unsettled, asserted not risen at :1146 | **AT-RISK (false fail, low)**: settle it (`poll_until_stable` + `service_job_scrape_settle`) |
  | 26 :871, 28 :208/:211, 31 `before`, 35 :193/:340/:494 | unsettled baselines → false-PASS risk only (secondary evidence). Fix: same settle helper (small; propose fixing in this loop — @test to rule) |
  | 01 (c) | AT-RISK, handled in §G |
- O4: agreed. Use gauges (see §G).
- O5: when the collector TODO entries close, the text states that:
  - the hash-named ConfigMap retires the kubelet-cache-lag residual;
  - "configured → rollout" is now Layer 7's normal path on every gate;
  - the `dt_client_*`-under-`job="otel-collector"` presence check stays OPEN as the stricter optional control;
  - `deploy_otel_collector`'s pre-apply and its 180s `rollout status` are kept.
- O6: confirmed; `delete_retired_resources` is deleted.

**Operations**
- The ruling is adopted: all collector changes are one class. The runbook rewrite follows items 1–6 of your
  ruling: banner gone, name-binding check, rollback as git revert + `deploy`, `rollout undo` emergency-only,
  triage step 4. It also says (a) that deploy's collector pre-apply + `rollout status` before the root is the
  dev-cluster form of the separate window, and that it catches a rejected or never-Ready config but NOT one that
  goes Ready and dies later; and (b) that the window rule is for live-traffic environments, while in the
  disposable dev cluster one apply is accepted. The 1620/1624/1626 TODOs close with the 1626 residual recorded.
- Q1: yes. A failed or timed-out collector `rollout status` stops deploy BEFORE the root apply, with
  `DEPLOY_FAILED REASON=collector-rollout-failed WORKLOADS=dark-tower/deployment/otel-collector`, exit non-zero,
  and layer7 reports `cluster-deploy-failed` carrying that line.
- Q2: prune touches (a) older migration Jobs, excluding the current one, and (b) first-party images. It NEVER
  touches ConfigMap generations, and I'm adding no ConfigMap pruning. **Changed from step 2 so that
  `rollout undo` keeps working:**
  - keep = this deploy's refs ∪ the refs the cluster ran before this deploy (the previous ReplicaSet's images).
  - Evicted = the node's `sha-*` refs of that repo not in keep, i.e. two or more generations old. They are
    removed from the node (`crictl rmi`) and from the host store (`rmi`, no `-f`).
  - A failure is WARN-and-continue with `PRUNE_WARN REASON=image-prune-failed REF=<ref>`.
- Q3: see the corollary above.
- Q4, every `dev-cluster setup` caller, all rewritten. Run-story and preflight have none.

  | Caller | Rewritten to |
  |---|---|
  | `infra/devloop/devloop.sh:815` (eager) | `provision && deploy` |
  | `.claude/skills/devloop/SKILL.md:762` | `teardown`, then `provision` + `deploy` |
  | `docs/runbooks/client-dev-local.md:111,125,128` | host `setup.sh` / container `dev-cluster provision && dev-cluster deploy` |
  | `packages/web-app/README.md:65` | same |
  | `scripts/layer7.sh` hints at ~:1064, :1168, :1253, :1435 ("ensure 'dev-cluster setup' completed") | `provision` |
  | helper comments | updated |
  | `infra/devloop/dev-cluster` usage + "Valid commands" line | updated |
- Cost note: in devloop-validation.md §6.7 and the ADR, "every existing cluster rebuilds ONCE on its first
  post-merge provision (no record)".

**DRY**
- #1: every spelling site you listed is covered in the rev-2 caller table, the rev-1 §F table and the
  Classification table, and the guard proves it.
- #2: the SSoT is protocol.rs (`HelperCommand::name()` strings). The guard checks:
  - that the `dev-cluster` client's case arms and its "Valid commands" line equal that set;
  - that every `dev-cluster <verb>` in the swept sites is in it;
  - that every `{setup,provision,deploy}.sh --flag` is a flag its parser accepts.

  It uses a positive control per remedy site, meaning pinned non-zero extraction counts.
- #3: one render, in bash. Rust does no rendering and no hashing. The idiom is `sha256sum`, recorded in full and
  displayed as the first 12 hex.
- #4: variants, arms, fake-client arms and tests are deleted together.

**Test**
- 28, adopted as ruled:
  - sizing stays at the FULL ceiling and the delta counters are untouched;
  - PRECONDITION reads and prints per-instance `mh_media_egress_edges`, `mh_media_registered_meetings`, the
    ceiling and `_limit` gauges;
  - the first-join 503 reports observed occupancy as a leak finding ("handlers hold N/ceiling streams no live
    meeting releases"), not a restart.
- 26: a PRECONDITION reads `mh_media_registered_meetings` against `mh_media_registered_meetings_limit`, with its
  own message, and the recovery prose drops the fresh-cluster remedy (EndMeeting via the gRPC forward). Plus the
  O3 settle fixes.
- `FRESH_CLUSTER_HINT` is DELETED once 01/26/28 no longer reference it. `REDEPLOY_HINT` names `dev-cluster deploy`
  and `./infra/kind/scripts/deploy.sh`. The third site (`IMAGE_UNRESOLVED`) is deleted with the dead fallback.
  The drift guard is the test that fails on drift.
- **01 (c): route (a), published gauges.** Route (b) (Loki) is rejected: it moves the rotation wall to retention
  and brings in a second optional evidence path. Gauges already exist for 3 rows: `max_total_egress_edges` →
  `mh_media_egress_edges_limit`; `max_registered_meetings` → `mh_media_registered_meetings_limit`;
  `egress_budget_bps` → `mh_media_egress_budget_bytes_per_second` × 8. **Six rows have no gauge:**
  `max_egress_streams_per_meeting`, `max_candidate_sources_per_egress`, `policy_apply_timeout_ms`,
  `max_muted_sources_per_meeting`, `stream_cost_audio_bps`, `stream_cost_video_bps`. **DECISION NEEDED
  (@team-lead, @test):**
  - (a1) add those gauges to MH's `publish_egress_admission` in THIS loop. That is new MH telemetry: names, the
    metric catalog `docs/observability/metrics/mh-service.md`, and metric-coverage test references in
    `crates/mh-service/tests/`. It is a Domain-judgment edit, so media-handler (+ observability) would join the
    panel.
  - (a2) drop the six rows from (c), with the reason recorded in the module doc, and file an MH instrumentation
    TODO. The fresh-pod invariant holds, but process-read coverage of six rows lapses until that task.

  My recommendation is (a1). It is about six `gauge!` lines plus catalog and test references, and it completes
  the check rather than deferring it. Either way (c) keeps its distinct no-data branch (gauge absent means
  PRECONDITION, not a mismatch) and the PII policy.
- Self-test cases (1)–(6) are adopted as listed. In detail:
  - (1) the order is provision → deploy → suites. A provision failure means no deploy and no suites; a deploy
    failure means no suites. Checked with markers.
  - (2) a git PATH-stub that records `merge-base`/`diff` calls, with a positive control showing it is live.
  - (3)/(4) in setup.test.sh. Check, build and `deploy.sh`'s guard all call `blueprint_render`, pinned statically
    as a single definition with three call sites.
  - (5) in the helper tests.
  - (6) the Succeeded-pod delete is absent, with a positive control that the stubbed kubectl saw the Job wait.


### Revision 2.1 — second Gate-1 round (supersedes rev 1/rev 2 where they differ)

**Lead rulings recorded.** (a1): the six MH config gauges are built in this loop, with @paired-media-handler as owner. The O3 fixes are in scope. ADR-0038 Status stays **Proposed**; a dated note records that steps 1–3 are implemented. **Known host-side step before Gate 2:** the operator rebuilds and restarts the helper from this tree. Without that, Layer 7 fails with `helper-verb-unsupported`, and that failure is by design.

**Blueprint and certs (Lead item 4, security confirmed).** The public leaf `.crt` hashes stay in the render. Provision materializes the certs before it renders, so the build consumes exactly what was hashed; there is no circularity. When there are no certs yet (a first provision), `--check` renders `tls-cert <name> missing`. That can never match a record, so the result is REASON=missing/changed and a rebuild.

**Code quality**
- CQ1, the lib split by lifecycle:
  - `lib/common.sh` is hashed and definitions-only. It holds only what provision uses: name/port-map/gateway validation *functions*, `KUBECTL`/context, logging, runtime detect/prereqs and `container_cmd`.
  - `lib/cluster-db.sh` is NOT hashed. It holds `dt_psql`, `DT_PG_*` and the org-cap validation, and is sourced by `deploy.sh` (seeds) and `setup.sh` (`--provision-org`).
  - `load_image_to_kind` and `extract_manifest_images` move to deploy.sh. provision loads no images.
  - `infra/lib/cargo-lock-version.sh` is deploy-only and not in PROVISION_INPUTS.
- CQ2, the PROVISION_INPUTS static pin. It covers every way a script can be reached: `source`, `.`, `bash|sh <path>`, `${PROJECT_ROOT}`/`${SCRIPT_DIR}`/`$(dirname …)`-relative paths and bare `./x.sh`. A positive control requires the real provision.sh to match exactly the array's entries (N > 0). A failing fixture (a provision.sh that sources something outside the array) must go red.
- CQ3, a lockstep rename. The status field `setup_in_progress` becomes `cluster_write_in_progress`, the line `Setup in progress:` becomes `Cluster write in progress:`, and layer7's greps and fake client change with it. The set is `provision|deploy|recreate|teardown`; teardown is included because layer7's busy-retry must wait it out too. It gets a layer7.test.sh case on the new text.
- CQ4, the guard matches command positions only. Regexes (ERE):
  - `.md`: a backticked form `` `(\S*/)?dev-cluster ([a-z][a-z-]*)`` … and lines inside ``` fences matching `^\s*(\$ )?(\S*/)?dev-cluster ([a-z][a-z-]*)`.
  - `.sh`/`.rs`/`.ts`: `(["'`(]|/devloop/)dev-cluster ([a-z][a-z-]*)`.
  - Prose such as "dev-cluster helper" is not matched.
  - A self-test extracts from the REAL files and must produce the exact expected verb set.
  - For the prose that the guard can't see ("rebuilds when infra/kind changes"), I sweep by hand with grep: `infra/kind/ change`, `diff-based`, `rebuild-all`, `--skip-build`, `--only`, `deploy <svc>`, `dev-cluster setup`.
- CQ5 (+ obs rev-2.1 note: a REAL provision whose materialize step renewed a leaf also reports `tls-renewal-due` in CHANGED=, next to the `tls-cert …` sections, so one grep finds every renewal; a setup.test.sh case pins it), `--check` flags a renewal that is due. `generate-dev-certs.sh` gains `--check-renewal`, a read-only mode that exits 1 iff its own reuse predicate would renew a leaf. It is ONE predicate: the same function decides "renew" in normal mode. `provision.sh --check` calls it and reports `REASON=changed CHANGED=tls-renewal-due`, so a host `deploy.sh` on an expiring leaf names `provision`.
- A comment at the port-verify skip: an existing cluster holds its own ports; if provision rebuilds it, `kind create` fails loudly on a foreign holder.

**DRY**
- D1: protocol.rs exposes `pub const VERBS: &[&str]`. An in-crate unit test asserts that `parse_command` accepts exactly VERBS and rejects each retired verb. The client gets one `VERBS="provision deploy …"` line; its case dispatch and usage validate against that line (unknown verb means an error that lists VERBS). The guard compares the two single-line literals.
- D2: remedy-site controls are ≥1 hit per site, never an exact count.
- D3: `docs/decisions/adr-0030-host-side-cluster-helper.md` is swept, while the other ADRs stay historical. Every scan root is scope-live: a missing root is a hard failure, never 0 hits.
- D4: `teardown.sh` and `iterate.sh` source `lib/common.sh` for logging, which is safe because the lib is definitions-only. `validate_cluster_name` is NOT collapsed: teardown keeps its charset-only validator, and the setup.test.sh charset-sync check is re-pointed at lib/common.sh.
- D5: token lists at devloop-validation.md :167 and :826 are updated. The §6.7 rows gain the setup-level sub-causes `blueprint-stale|blueprint-missing|blueprint-unreadable|tls-renewal-due`, `collector-rollout-failed`, `rollout-failed` and `image-prune-failed` (WARN), each under its Layer-7 token. The optional token-vs-runbook guard is not taken this round.
- D6: the §6.7 IMAGE_UNRESOLVED paragraph and every `--skip-build`/`--only` runbook mention go.

**Security (T1–T4, plus confirmations from the earlier S-block)**
- T1: provision.sh derives the AC `DATABASE_URL` from `infra/services/postgres/secret.yaml` (`POSTGRES_USER`/`POSTGRES_PASSWORD`/`POSTGRES_DB`), and that file is in PROVISION_INPUTS, i.e. in the render. **Rule, stated in provision.sh and the ADR: anything provision reads is a render input.** The PROVISION_INPUTS pin enforces it for files that are sourced or executed; data files are read only through a `provision_input <path>` accessor that asserts membership.
- T2: an ADR note plus a comment at the record write: "the blueprint record is an anti-drift control, NOT integrity or tamper-evidence (provision.sh is container-writable in CLONE_DIR; the container's kubeconfig can rewrite the record)."
- T3: the existing injection tests are kept and re-pointed at `{token, command}`. The added or confirmed cases are all rejected before exec:
  - metacharacters;
  - embedded whitespace (`"deploy mc"`, `"provision "`);
  - NUL and newline;
  - an oversized payload (MAX_REQUEST_SIZE);
  - an unknown field alongside a valid verb.
- T4: setup.test.sh asserts the stubbed `kind delete cluster --name <DT_CLUSTER_NAME>` with that exact value. When DT_CLUSTER_NAME is unset and provision is non-interactive (`--yes`) and would DESTROY, it fails loudly and never defaults. Creating a missing default cluster still works for the host.
- Semantic guard: `Request` gets a manual `Debug` that redacts `token`, and no rejection path formats the raw body. The manifest and CHANGED= hold section labels, paths, sha256 values and the `kind version` string only, never a provision.sh line and never a per-secret line. The secret creation does not use `-o yaml` to stdout or `set -x`, and errors don't echo literals. Every new `map_err` carries `e`.

**Test, all adopted**
- Env-test 28, the agreed design:
  - Sizing is unchanged (max ceiling, `size_s9_meeting`).
  - Per-instance `mh_media_egress_edges` and `mh_media_registered_meetings` are read after the gauges are present. They drive (a) a PRECONDITION that every instance has headroom ≥ `slots`, and (b) the first-join-503 message.
  - Both messages print gauge occupancy and "GC answered 503" side by side without claiming the two agree, name the leak condition, and give no fresh-cluster remedy.
  - The discriminator is `rejected_stream_ceiling` alone.
  - Rejecting instance(s) are the ones whose counter moved. ADMITTED must have moved on a rejecting instance.
  - "Exactly one moved" is logged, not asserted.
  - The module doc is updated accordingly.
- Env-test 01(c): all nine `LOGGED_POLICY_BOUNDS` rows become `(key, gauge, Conversion)`:
  - Conversion is `Identity | BitsToBytesFloor | BitsToBytesCeil | MsToSeconds`.
  - (rev 2.2) The deployed ConfigMap value (from that instance's own generation) is converted by the SAME expression MH's config load uses, and the comparison is exact: budget `deployed_bits / 8` (integer floor, `crates/mh-service/src/config.rs` budget conversion); costs `deployed_bits.div_ceil(8)`; timeout: the INTEGER compare `(gauge * 1000.0).round() as u64 == deployed_ms`, never a float equality. The value round-trips through Prometheus text exposition and API formatting; the comment still names MH's `/ 1000.0` as the source, and a negative or non-finite timeout gauge is the shape-changed message. A comment at each comparison names that source. No tolerance and no extra "must be a multiple" failure: `convert(deployed)` is total for all four variants, because a non-multiple-of-8 bit value is valid config that MH floors or ceils. The only "not exact" branch is on the GAUGE side, keyed off the Conversion variant (Identity/BitsToBytes* rows must be integral; the MsToSeconds row only needs to be finite and non-negative, because 2500ms legitimately publishes 2.5): for an integer row, a gauge that is not integral (`fract() != 0`), is negative, or is above 2^53 fails with its own message ("gauge {name} on {instance} is not an integral value: the publisher's shape changed"), distinct from the mismatch and no-data branches.
  - A gauge absent on an instance within the bound is the no-data branch; a wrong value is the mismatch branch. Each has a distinct message.
  - The no-log-line PII policy stays.
  - `CONFIG_LOADED_MESSAGE` is deleted.
  - The module doc sections "startup log line is a TEST CONTRACT", (c) and "What this does NOT prove" are rewritten.
  - Fetch is via `gauge_by_instance_present`, one query per gauge.
- Env-test 26: remedy text only (gRPC `EndMeeting` recovery), plus the O3 settle fixes at :871 and :1100.
- `FRESH_CLUSTER_HINT` is deleted.
- O3 fixes, @test designs (rev 2.2):
  - 30: SAFE, NO CHANGE (see the O3 table).
  - ONE rule for every baseline read that a later delta or "not risen" check compares against: take the baseline from `poll_until_stable(&prom, promql, service_job_scrape_settle(&prom).await, bound, msg)` and use its RETURNED map; never re-read.
    - `msg` leads with `PRECONDITION: baseline did not stabilise`, prints both maps, and says it is an environment/scrape condition, not the assertion under test.
    - `bound` reuses each file's existing scrape-convergence constant; otherwise one named constant per file, with a one-line why.
    - 31's `before` (a cluster-wide `sum()` scalar today, `pii_dropped_sum`) converts to the per-instance map form (`sum by (instance)`), and its checks become per-instance deltas.
  - **Completeness sweep** of `crates/env-tests/tests/` for `instance_counter_map(` / `query_promql(` / `poll_until_stable(` reads:

    | Site | Verdict |
    |---|---|
    | 26:716, :763, :1098 (`wait_for_*_stable`), 26:314, :1017; 34:101/157/158 (`settled_baseline`) | already settled |
    | 26:871 (`mc_participant_mh_status_counter` baseline) | baseline, unsettled → FIX |
    | 26:1100 (`non_match_baseline`) | baseline, unsettled → FIX |
    | **26:1522 (`baselines` = server_muted, compared at :1592)** | baseline, unsettled, **missed by the audit** → FIX |
    | 28:208/:211 (admitted / rejected) | baseline → FIX |
    | 31:612/:643/:674 (`before` via `pii_dropped_sum`, :316) | baseline, scalar → FIX (per-instance form) |
    | 35:193, :339-340, :494, :495 | baseline → FIX |
    | 26:809/:973 (helper fns called by the sites above) | helpers; settling happens at the callers |
    | 30:43, :85, :336, :520, :545, :566, :586; 32:154 | not a baseline (presence / instant-state queries) |
- Self-test asks (a)–(j), all adopted as written:
  - (a) An unreadable record: the kind-delete marker is ABSENT, the kubectl stub was reached, and `REASON=blueprint-unreadable`. The TTY-`N` path gets the same checks.
  - (b) Fail kind create, then Calico, then secrets, one at a time. Each failure means no record-create call, and the next run prints `REASON=missing` and rebuilds.
  - (c) A stub mutates a PROVISION_INPUTS file between render 1 and render 2. That fails loudly and records nothing.
  - (d) Identical inputs give an identical manifest and hash. Each section class, changed once, changes the hash, and CHANGED= names exactly that section. A deploy.sh-only change leaves the hash UNCHANGED.
  - (e) `--blueprint` never invokes generate-dev-certs and makes no kubectl or kind write.
  - (f) deploy.sh's guard on missing/stale fails loudly naming `provision`, with zero build or apply stub calls.
  - (g) The PROVISION_INPUTS pin has a failing fixture.
  - (h) There is no `delete pod` against the Job pod, plus a stub-reached control.
  - (i) layer7 runtime case: a staged `infra/kind/` change gives exactly provision+deploy. The grep is secondary, and the retired e2, c0 and rebuild-all cases are deleted.
  - (j) Each guard rule has a failing fixture, with ≥1-per-site controls.

**Media-handler gauges (spec = @paired-media-handler's design + @observability's conditions)**
- Six gauges in `publish_egress_admission`, same call site, each `.set()` from the ONE enforcing field, `key_custody=operator` only:
  - `mh_media_egress_streams_per_meeting_limit`
  - `mh_media_candidate_sources_per_egress_limit`
  - `mh_media_muted_sources_per_meeting_limit`
  - `mh_media_policy_apply_timeout_seconds` (ms/1000)
  - `mh_media_stream_cost_audio_bytes_per_second`
  - `mh_media_stream_cost_video_bytes_per_second` (enforced bytes, ceiled)
- No rename: I keep `publish_egress_admission`, so the catalog line "every gauge publish_egress_admission sets" stays true.
- The fn doc comment is updated. The main.rs startup-event comment is rewritten as operator-facing only, and the log line is kept.
- `stream_admission_integration.rs` asserts gauge == config field for all six, with a distinct fixture value per field.
- Catalog entries go in `docs/observability/metrics/mh-service.md`. Three panels go in `mh-media.json`, next to the existing `_limit` panel and not on the Budget panel.
- No alerts.

**Operations.** §Rollback states the limit: undo reaches back ONE generation (the prune keep-set). The first post-merge rebuild of every cluster is documented in §6.7 and in the ADR.

**Rev 2.2 cross-check against the second-round messages.** DRY (a)–(d) and semantic-guard (1), (3), (5) were already folded into rev 2.1: D2 ≥1 per site; D3 ADR-0030 swept and scan roots scope-live; D4 lib definitions-only, teardown/iterate source it, charset-sync re-pointed; D5 :167/:826 plus the new rows, which also get a `helper-verb-unsupported` row; the Security block's last bullet covers the manifest content, the redacted `Debug` on `Request`, and the manual prose sweep (`rebuild-all|diff_touches_path|merge-base|--skip-build|setup_sh_command|infra/kind/ change|diff-based` over live paths, run before review). Rev 2.2 changes only: 30 stays SAFE, the exact conversion expressions, the baseline rule plus the completeness sweep (one extra site found, 26:1522), and `tls-renewal-due` on real runs. Nothing on the user's "fix every class" list grows the task significantly, so nothing is cut.


---

## Implementation Summary

Implemented per plan rev 2.2. `./scripts/layer-fast.sh`: **TOTAL_RESULT=N/A, rc 0** (L1 OK, L2 OK, L3 OK, L4 N/A — proto placeholder only, `cargo-test-passed`, L5 OK, L6 N/A — `cargo-audit-passed`). WARN: Layer 3/4 BUDGET_BREACH (known).

**Known host-side step before Gate 2 (Lead ruling):** the operator rebuilds and restarts the devloop helper from THIS tree (re-run `infra/devloop/devloop.sh`). An older helper rejects `provision`/`deploy` and Layer 7 fails loudly as `helper-verb-unsupported` — by design, no fallback. Every existing cluster rebuilds once on its first provision (no blueprint record).

- **Kind scripts** (`infra/kind/scripts/`): `provision.sh` (blueprint render/check/record, cluster/Calico/namespaces/Secrets+TLS, `BLUEPRINT` decision line, destroy only the named cluster), `deploy.sh` (moved deploy code; guard `provision.sh --check`; loads only missing refs; preloads only missing third-party images; collector-before-root with `collector-rollout-failed`; `DEPLOY_FAILED … WORKLOADS=`; two-generation image prune; interim Succeeded-pod delete, retired RBAC delete and the IMAGE_UNRESOLVED fallback removed), `setup.sh` (host entry + `--provision-org`), `lib/common.sh` (hashed, definitions-only), `lib/cluster-db.sh`; `scripts/generate-dev-certs.sh --check-renewal` (one reuse predicate). `teardown.sh`/`iterate.sh` source the lib for logging.
- **Tests**: `scripts/setup.test.sh` 438 passed (new §P provision cases incl. asks (a)–(g), T4, TLS renewal both paths; deploy main cases incl. (f)/(h)); `scripts/layer7.test.sh` 335 passed (provision/deploy order, both tokens, old-helper token, runtime no-diff proof with a git stub + positive control); new `scripts/guards/validate-dev-cluster-verbs.test.sh` 25 passed (wired in layer3.sh).
- **Helper** (forked slice): `VERBS` = provision/deploy/teardown/recreate/restore-kubeconfig/status/cancel, `Request {token, command}` + redacted Debug, `script_command`, `cmd_provision`/`cmd_deploy`/`cmd_recreate`, `cluster_write_in_progress`; `cargo test -p devloop-helper` 173 passed ×3.
- **MH gauges + env-tests** (forked slice): six config gauges, catalog + 3 panels; env-test 01(c) over nine gauges, 26/28/31/35 settle fixes, 28 occupancy precondition, `FRESH_CLUSTER_HINT` deleted; `REDEPLOY_HINT` → `dev-cluster deploy`.
- **Docs**: ADR-0038 (status note, step-3 notes, open question resolved), ADR-0030 (verb table, no-argument wire surface), ADR-0033, ADR-0013, LOCAL_DEVELOPMENT.md, devloop-validation.md (§3 token list, §6.7 rows, sub-causes, §8), gc-deployment.md collector rewrite (ops ruling), mechanical runbook/INDEX/citation fixes, TODO closures. Guard `validate-dev-cluster-verbs.sh`: 122 command spellings live. Manual prose sweep for retired vocabulary run over live paths.
- **Gate 3 T9 (amended)**: provision failures classified at the source like deploy's (`PROVISION_FAILED REASON=… STEP=…`, EXIT trap, `pstep`); one shared bounded env probe `lib/common.sh:classify_env_failure` (`runtime-unreachable` via `timeout N <runtime> info`; `apiserver-unreachable` via `/readyz`, provision only after `kind create cluster`); `check_host_ports` → `port-held` (TCP by bounded connect; UDP from `/proc/net/udp{,6}`, address-aware — ops T9-2); `node_runtime_incapable` → `runtime-incapable` after a failed `kind create` (TODO §G's nested-podman `sethostname` ceiling stays in the operator lane — ops T9-1; `container_cmd` moved to `lib/common.sh`); `operator-declined`; deploy's disk-check `<runtime> info` bounded (a hung runtime was an unbounded wait) → `runtime-unreachable`. layer7.sh: one list `CLUSTER_ENV_REASONS`, one router `__cluster_failure_lane <verb>` (`FAIL provision-failed` / `FAIL deploy-failed` for the tree). Tests: `setup.test.sh` 576/0 (incl. `udp_port_bound` over a partly absent table set — mawk-safe; incl. test T10: no `timeout` on PATH is `prerequisite-missing` for both verbs and in the classifier, never `runtime-unreachable`; incl. UDP held / wildcard / other-address, runtime-incapable probe fail / pass / no-image / hang / not-on-success) (pre- vs post-create probe, runtime-unreachable, a hang case per probe bounded at `DT_ENV_PROBE_TIMEOUT=1`, port-held with a real listener + positive control, operator-declined, provision prerequisite-missing via hermetic PATH, one line per failure); `layer7.test.sh` 509/0 (two-verb route table, drift over deploy.sh + provision.sh + lib/common.sh). layer-fast rc 0; L3 161s (was 135s; `BUDGET_TOTAL_BREACH` pre-existing, +26s from the new provision/deploy invocations).

---

## Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired Media Handler | confirmed |

---

## Gate 3

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | SEC-1 Calico manifest sha256 pin (independently verified upstream) |
| Test | RESOLVED-FIXED | 10 | 10 | 0 | T1-T8; T9 cluster-failure lane partition (provision+deploy); T10 timeout prerequisite |
| Observability | RESOLVED-FIXED | 6 | 6 | 0 | DEPLOY_FAILED step trap, helper.log decision lines, ACTION=refuse, dashboards/catalog, runbook |
| Code Quality | RESOLVED-FIXED | 9 | 9 | 0 | PROVISION_INPUTS pin tightened, #[expect], pg_secret_value SSoT, SCRIPT_ARGS, 28 missing-series |
| DRY | RESOLVED-FIXED | 5 | 5 | 0 | shared settled_baseline, guard_seam_root, GC/MC creds from owner Secrets; compose-creds TODO is separate-env tracking, not a deferral |
| Operations | RESOLVED-FIXED | 5 | 5 | 0 | F1-F3 + T9-1 runtime-incapable, T9-2 UDP port-held (re-verified post-T9) |
| Semantic Guard | RESOLVED-FIXED | 7 | 7 | 0 | native SAFE (0 checks.md findings); 7 stale-prose items outside checks.md, all fixed |
| Paired Media Handler | RESOLVED-FIXED | 4 | 4 | 0 | F1-F4 |

---

## Accepted Deferrals

- (none surfaced in this devloop)

Tracking entries added to `docs/TODO.md` that are NOT deferrals of findings (separate scope, per Lead rulings): docker-compose dev-credential copies (§Cross-Service Duplication, separate environment); the runner/layer DEVLOOP_TEST seam sites (pre-existing entry, guard-root sub-case resolved here); the `dt_client_*` Prometheus-presence check (split out of a closed entry).

---

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, attempt 1: **PASS** (`TOTAL_RESULT=N/A`, rc 0, 1408s). Host helper rebuilt and restarted from this tree by the operator first.

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 6s |
| 2 Format | OK | 5s |
| 3 Guards | OK | 179s (over the 90s budget, which was already exceeded before this change) |
| 4 Test | N/A (proto placeholder; cargo 4634 passed / 0 failed, nx OK) | 264s |
| 5 Lint | OK | 2s |
| 6 Audit | N/A (proto placeholder; cargo/pnpm audit, buf breaking OK) | 3s |
| 7 Env-tests | OK (env-tests-passed, browser-e2e-passed) | 949s |

Layer 7 demonstrated the new flow end to end: `BLUEPRINT ACTION=rebuild REASON=missing` → `STEP=provision` 105s (the one-time rebuild every pre-existing cluster gets, since none has a record) → deploy's own `BLUEPRINT ACTION=check REASON=match` → `STEP=deploy` 445s → env-tests + browser E2E green. Both decision lines are recorded on the `helper.log` audit entries.

---

## Rollback Procedure

Start commit `e0f40a8a`. Safe-revert unit: **the whole commit.** The helper wire (`protocol.rs` VERBS), the `dev-cluster` client, `layer7.sh`'s verbs and routing, and `provision.sh`/`deploy.sh` must move together, and a partial revert leaves a client calling verbs the helper rejects. After a revert, the host helper must be rebuilt from the reverted tree. The cluster needs no manual action: the old flow's `setup` ignores the blueprint ConfigMap.

---

## Lessons Learned

1. Moving from "restart everything every gate" to "converge" surfaced a whole class of fresh-state assumptions (unsettled baselines, deploy-failure lane routing) beyond the three env-tests the ADR named. An audit sweep, not the ADR's list, set the true scope.
2. When deploy starts applying the tree, the failure-lane default (operator versus implementer) becomes a correctness issue, not a triage detail. Classify it at the source, and fail closed to the implementer lane.
