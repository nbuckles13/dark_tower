# ADR-0038: The Dev Cluster Deploys the Way Production Will

**Status**: Proposed — *implementation note 2026-09-29: Implementation steps 1–3 are implemented (step 3: devloop `2026-09-29-adr0038-provision-deploy-l7-switchover`). Acceptance is the user's call.*

**Date**: 2026-09-26

**Deciders**: user, infrastructure, operations, security (helper allowlist, ADR-0030), test (Layer 7)

---

## Context

Layer 7 validates the tree against a Kind cluster, and how that cluster is brought
up to date has grown by accretion rather than by design:

- **Correct only by accident, and slow.** `layer7.sh` tears the cluster down and
  rebuilds it when the diff touches `infra/kind/`, but the diff is taken against
  `merge-base(origin/main, HEAD)`. Any branch that ever changed `infra/kind/` therefore
  rebuilds on EVERY gate: ~6 minutes per gate (351s mean, 491s max over 24 recent
  gate logs), at least twice per task (Lead Gate 2 + runner authority gate).
- **That accidental rebuild is the only thing hiding a real gap.** Outside a rebuild,
  `rebuild-all` rebuilds images and restarts pods but never applies manifests, so an
  `infra/services/**` change (a ConfigMap key, a Deployment edit) is not deployed. It
  was observed live on 2026-09-03 (MC CrashLooping on a required key the stale
  ConfigMap lacked) and recorded in `docs/TODO.md`.
- **The dev cluster does not work the way production will.** Migrations run from the
  host through a `kubectl port-forward` with `sqlx-cli`, and are silently SKIPPED with a
  warning when `sqlx` is not installed. Secrets and TLS material are created
  imperatively by `setup.sh`, so `kubectl apply -k` alone cannot produce a startable
  MC or MH. Service ConfigMaps are plain resources, so a config change rolls nothing
  and every gate restarts every service unconditionally; in production an MC or MH
  restart drops live meetings. An environment root that composes the whole Kind
  deployment exists (`infra/kubernetes/overlays/kind/kustomization.yaml`), but nothing
  applies it — `setup.sh` only uses it to list images.
- **A diff-based fix was built and rejected** (devloop `2026-09-26-l7-cluster-refresh-
  correct-cheap`, commit `d58e496`, never absorbed). It decided what to apply from the
  git range since the task started, which is correct only if the cluster already
  matched the tree at that commit. Nothing recorded or verified that, and this
  workflow breaks it routinely: cherry-picks, absorbs, manual commits, rebases and the
  runner's own completion amend all land outside any task's range and would never
  deploy; a manifest applied at Gate 2 and then reverted, or discarded by an
  escalation reset, would stay live.

How often cluster-affecting files change is not rare: every story-2 task that touched
cluster configuration touched `infra/` (9 of 9; 8 of 9 outside dashboards and alert
rules), because media tasks add required service ConfigMap keys routinely. So "rebuild
whenever `infra/` changes" would still rebuild on nearly every gate.

## Decision

The dev cluster is managed the way a production environment is: as two layers with
different lifecycles, each an idempotent operation that converges to the tree. No
decision anywhere is derived from git history or file timestamps; every skip or
rollout decision is derived from **content hashes of inputs compared against what is
actually deployed**.

### 1. Provisioning — the platform, recreated only for immutable changes

`dev-cluster provision` makes sure a cluster exists that matches the **blueprint**:
everything provisioning consumes to build what cannot change in place.

- **Provisioning is `render` then `apply`, and the check is `render` then `hash`.** The
  render is a pure, deterministic function (the `terraform plan` / `kustomize build`
  shape) that produces the exact desired platform without touching a cluster: the fully
  rendered kind configuration (including the helper's stable per-slug port allocation),
  the pinned versions (kind node image, CNI, kind itself), the provisioning
  implementation, and the platform manifests provisioning applies. Because the check and
  the build use the same render, whatever the build would consume is exactly what gets
  compared — no hand-kept list of directories to drift as inputs are added.
- **Hash the recipe, not random products**: generated secret and TLS material is random,
  so the generation inputs (script + parameters) are hashed, never the key bytes.
- The blueprint hash is computed host-side by the helper, over content, never mtimes.
- It is **recorded inside the cluster** (a ConfigMap, e.g. `kube-system/devloop-blueprint`)
  after, and only after, a cluster is successfully built — so the record lives and dies
  with the cluster, and a half-built cluster never claims a blueprint.
- `provision` compares the current hash with the recorded one: equal → nothing; different
  or missing → tear down and rebuild, then record. This is the Terraform-state
  equivalent: plan against recorded state, replace only for what cannot change in place.
- Secret and TLS material for the dev environment is created here (the dev stand-in for a
  production secret manager), so the application layer only ever references it.

### 2. Delivery — the application, converged on every run

`dev-cluster deploy` converges the application to the tree, identically every time:

1. **Build images with content-derived tags** (derived from the image's own content, e.g.
   `mc-service:sha-<id>`) and load them into Kind — the dev stand-in for pushing to a
   registry. Unchanged code yields the same tag.
2. **Apply ONE environment root**: `kubectl apply -k infra/kubernetes/overlays/kind/`, which
   composes every service, the OTel collector and observability — as production would
   apply `overlays/prod`. Image references come from step 1.
3. **Run migrations as a Job in the cluster** from an image built from `migrations/`,
   before rollouts proceed (the production pre-upgrade-hook / sync-wave shape). sqlx's
   own `_sqlx_migrations` table (version + checksum) makes it run pending migrations
   only, and fail loudly if an applied migration's content changed.
4. **Wait once** for all rollouts.

Every ConfigMap that pods consume is content-addressed (`configMapGenerator` hash suffix,
or a checksum annotation where a stable name is required), so a config change changes the
pod template. Together with content-tagged images, applying the root rolls **exactly** the
workloads whose image or configuration changed and nothing else; there is no
unconditional restart.

### 3. Layer 7 and the helper

- Layer 7 is `provision && deploy && tests`. It no longer inspects diffs.
- The helper's allowlist (ADR-0030 trust boundary) is expressed in these verbs
  (`provision`, `deploy`, plus existing status/teardown/recovery verbs), replacing the
  per-service rebuild/deploy surface. `deploy` is also what a developer runs by hand.
- `setup.sh` shrinks to the implementation behind `provision` (and any shared pieces
  `deploy` reuses); it is no longer the place that decides what a gate does.

## Consequences

### Positive
- **History-independent correctness**: every run compares the tree with what is deployed,
  so cherry-picks, absorbs, reverts, resets and rebases cannot leave the cluster stale.
- **Fast**: only blueprint changes (rare; in story 2 only task D's work) pay the ~6-minute
  rebuild; everything else is apply + a migration Job + rolling only what changed.
- **Production-shaped**: one environment root, content-addressed artifacts, migrations as
  an in-cluster Job, and no restarts of unchanged stateful services — the same model a
  production pipeline (or a GitOps controller) runs against `overlays/prod`.
- **Fixes existing masked failures**: stale ConfigMaps after manifest changes, and
  migrations silently skipped when `sqlx` is absent.
- **Smaller trust boundary**: a few verbs instead of a per-service rebuild/deploy matrix.

### Negative
- Restructuring work across `setup.sh`, the helper, the Kind overlays, `layer7.sh` and
  service Deployment/ConfigMap manifests.
- Hash-suffixed ConfigMaps leave old generations behind until pruned (hygiene, not
  correctness).
- Out-of-band edits made directly with `kubectl` are overwritten by the next `deploy`
  (intended — the tree is the source of truth — but a behaviour change for anyone who
  hand-patches the dev cluster).

## Alternatives considered

- **Diff-based incremental apply from the task's start commit** (`d58e496`) — rejected:
  correctness depends on git history (see Context).
- **Hash the source directory `infra/kind/**`** — simpler, but drifts: provisioning
  already consumes inputs outside it (certificate generation, pinned versions, the
  helper's port allocation), and a hand-kept directory list misses each new one.
- **Rebuild whenever `infra/` (and `migrations/`) change, against a recorded hash** —
  correct, and simpler, but slow: nearly every story-2 gate would still rebuild.
- **Record the last-synced tree and apply only its diff** — correct if every apply updates
  the record atomically, but more machinery than applying everything, which is
  idempotent and takes seconds.
- **Run a GitOps controller (Argo CD / Flux) in Kind** — the most production-faithful, but
  heavy for a per-devloop cluster; this ADR adopts the same model without the controller,
  so adding one later changes the mechanism, not the shape.

## Implementation

Three devloops, in order; each leaves Layer 7 correct:

1. **Content-addressed configuration + the environment root**: every consumed ConfigMap
   hash-suffixed or checksum-annotated (service configs, OTel collector, Grafana
   datasources); `infra/kubernetes/overlays/kind/` becomes the applied root.
2. **Migration Job + content-tagged images**: a migrations image and Job; image tags
   derived from image content and wired into the root; the host `sqlx` path retired.
   Step-1 follow-ups carried here (they surfaced when story 2 was absorbed on top of
   step 1):
   - **One `configMapGenerator` parser.** Step 1 added the shared parser
     (`crates/dt-guard/src/common/kustomize_generators.rs`); story 2 task 12 brought a
     second one (`parse_generators` in `crates/dt-guard/src/kustomize_configmaps.rs`,
     behind the annotation-size rule). Consolidate onto the shared one, keeping both
     callers' tests green.
   - **Observability confirms the retired `dashboard_configmap_label` rule.** The absorb
     retired it (commit `2abbd05d`): it checked the label a Grafana k8s-sidecar selected
     on, and step 1 replaced the sidecar with a projected volume, which R-21 guards.
     This is a review ask, not new work.
3. **`provision` / `deploy` verbs + Layer 7 switch-over**: a deterministic blueprint render
   shared by build and check, its hash recorded in the cluster; helper allowlist reduced to the verbs; `layer7.sh` becomes
   `provision && deploy && tests`; `rebuild-all` and the diff-based infra trigger retired;
   the 2026-09-03 TODO entry resolved. This step also retires the two interim diff-based
   Layer 7 arms that story 2 task 10 adds (they reach this tree when story 2 is absorbed,
   so this step runs after that absorb):
   - `dev-cluster deploy <svc>` when the diff touches a service's manifests, and
   - a container-side `kubectl apply -k infra/kubernetes/overlays/kind/observability/`
     (plus a Prometheus rollout wait) when the diff touches the observability config.

   Both decide what to apply from `merge-base(origin/main, HEAD)`, the history-dependent
   rule this ADR rejects. Today they are masked: any branch that touched `infra/kind/`
   gets a full rebuild on every gate. Once that rebuild stops firing, they become the
   only deploy path. **Security note:** the observability apply is a cluster write
   from inside the devloop container, outside the helper allowlist (ADR-0030). It is
   an accepted interim exception, not a precedent. `deploy` (helper-side, applying the
   one environment root that already composes observability) replaces it, and no new
   container-side cluster write may be added in the meantime.

   Also carried from devloop 2 (so it is not lost): **remove the interim Succeeded-pod
   delete** in `infra/kind/scripts/deploy.sh:run_migration_job`. It exists only because a
   helper older than devloop 2 counted a finished Job pod as unhealthy; `parse_pod_health`
   now exempts Job-owned `Succeeded` pods, so once every running helper is built from a tree
   with that fix the delete is dead weight — and it throws away the pod's logs. `setup.sh
   --rebuild-all` (devloop 2's internal mode behind the helper's `rebuild-all`) is absorbed
   into `deploy` here.

   The verb rename (`rebuild-all` → `deploy`; fresh cluster → `provision`) must reach ALL THREE
   places that spell the remedy — they are in two languages and cannot share a constant:
   `crates/env-tests/src/fixtures/kube.rs` (`REDEPLOY_HINT`, `FRESH_CLUSTER_HINT`),
   `infra/kind/scripts/deploy.sh:resolve_image_refs` (the `IMAGE_UNRESOLVED` message), and
   `docs/runbooks/devloop-validation.md` §6.7 (the `cluster-rebuild-failed` sub-causes).

   Also observed at step 2's Gate 2: a failed setup leaves a half-built cluster that the next
   Layer 7 reuses (health check → `dev-cluster setup` reuse by name, which runs before the
   `infra/kind/` teardown arm). `provision`'s recorded-after-success blueprint hash closes it, and
   Layer 7's check-health-then-reuse order is retired with it.

   Env-tests 01 (startup-log parity), 26 (wedged generation) and 28 (stream-ceiling ratchet)
   assume a freshly started pod. Under `deploy`'s no-unconditional-restart rule, pods survive
   across gates, so these will recur. Make them fresh-pod-independent (e.g. 28 reads the
   current count as its baseline; 01 reads the in-effect config other than from the startup
   log) instead of adding a helper restart verb. Env-tests are meant to eventually target
   prod, which is why their fresh-pod and cluster-write assumptions must go rather than be
   accommodated.

   `deploy` is also the in-container route for OTel collector changes, so this step
   closes the two collector entries in `docs/TODO.md` ("No collector deploy route from
   inside the devloop container", "Collector config-staleness window"). It also rewrites
   the collector change procedures in `docs/runbooks/gc-deployment.md`, which a status
   banner marks as pre-ADR-0038 (TODO: "gc-deployment.md's OTel-collector change
   procedures"). **That rewrite carries an operations decision:** a config-only collector
   change now renames the ConfigMap and restarts the singleton collector on apply. Under
   R-54 fail-hard-at-init that is the same hazard as an image change, so whether the
   runbook's config-only exemption from the separate-change-window rule still holds is
   for operations to decide in this step, not to inherit.

Until then, today's behaviour (a full rebuild on every gate) stays: slow but correct.

### Step 3 implementation notes (2026-09-29)

- **Where "setup.sh shrinks to the implementation behind provision" landed.** The provision
  implementation is its own file, `infra/kind/scripts/provision.sh`, so the blueprint can hash
  the provisioning implementation BY WHOLE FILE without dragging in the deploy path (an edit to
  the deploy code must never rebuild a cluster). The deploy implementation is
  `infra/kind/scripts/deploy.sh`; `setup.sh` remains the host one-stop entry (provision, deploy,
  host conveniences) and `--provision-org`. Shared definitions live in `lib/common.sh` (hashed:
  only what provision uses) and `lib/cluster-db.sh` (not hashed).
- **Where the hash is computed.** Host-side, by the provision implementation the helper runs
  (`provision.sh`), not in Rust: ONE render (`blueprint_render`) serves the check, the build and
  deploy's own guard (`provision.sh --check`), for the helper and the host alike.
- **What the render contains.** The rendered kind config (the helper's per-slug render via
  `DT_KIND_CONFIG`), `kind version` (which pins the node image), the provider, every
  `PROVISION_INPUT` hashed whole (`provision.sh`, `lib/common.sh`, the TLS recipe
  `scripts/generate-dev-certs.sh`, and `infra/services/postgres/secret.yaml`, from which AC's
  `DATABASE_URL` is derived so the credential has one owner), and the PUBLIC MC/MH WebTransport
  leaf certificates the TLS Secrets carry. The certs are an addition to "hash the recipe": the
  14-day leaves expire, a time input no recipe hash captures, so provision first materializes
  the host inputs (the idempotent recipe renews a leaf only when due) and then renders; a renewal
  rebuilds the cluster (about every 13 days on a long-lived one; `CHANGED=tls-renewal-due`).
  Never key bytes, never a Secret value. Rule: anything provision reads is a render input
  (pinned by `scripts/setup.test.sh`).
- **The record** (`kube-system/configmap/devloop-blueprint`: the hash plus a per-section digest
  manifest, so a mismatch names what changed) is written as the last step of a successful build,
  after a re-render confirms nothing changed mid-build. It is an **anti-drift control, not
  integrity or tamper-evidence**: `provision.sh` is container-writable in the devloop clone, and
  the container's cluster-admin kubeconfig can rewrite the record. A missing or unparseable
  record rebuilds; an UNREADABLE one fails loudly and never destroys. Every run prints one
  `BLUEPRINT ACTION=… REASON=… RECORDED=… CURRENT=… CHANGED=…` line. Every cluster built before
  step 3 has no record, so it rebuilds once on its first post-merge provision.
- **`deploy` always builds every first-party image**, so the built > deployed > fail resolution
  of step 2 (and its `IMAGE_UNRESOLVED` remedy site) is gone rather than renamed; images are
  loaded into Kind only when the node lacks that content-addressed ref. The image prune keeps two
  generations — the current ref and the one the workload's own rollout history names as previous
  (newest non-current ReplicaSet / ControllerRevision: what `kubectl rollout undo` reaches, even
  after an unchanged redeploy); ConfigMap generations are not pruned. Every failed deploy ends
  with one `DEPLOY_FAILED REASON=… WORKLOADS=…` line (a specific reason, or `step-failed
  STEP=<step>` from main's EXIT trap); PostgreSQL/Redis are awaited with `rollout status`, never a
  label-selector pod wait that would match the old Ready pod on a warm cluster.
- **Platform manifests are verified.** The Calico manifest is fetched over HTTPS and applied only
  if its sha256 equals `CALICO_MANIFEST_SHA256` (pinned in `provision.sh`, so part of the
  blueprint): a moved tag or substituted download cannot change the platform unseen. An
  unreadable record is `BLUEPRINT ACTION=refuse`. The interim Succeeded-pod delete and the retired grafana-sidecar RBAC delete are
  gone (every cluster is rebuilt by its first provision).
- **Helper**: verbs `provision`, `deploy`, `teardown`, `recreate`, `restore-kubeconfig`,
  `status`, `cancel`, none with an argument (`Request` is `{token, command}`); `setup`,
  `rebuild`, `rebuild-all`, `deploy <svc>` and `--skip-observability` are retired. Remedy
  spellings (Rust, shell, markdown) are kept live by `scripts/guards/simple/validate-dev-cluster-verbs.sh`.
- **Layer 7** is `provision && deploy && tests` (tokens `cluster-provision-failed`,
  `cluster-deploy-failed`, `helper-verb-unsupported`). Both verbs' failures are routed by the
  `PROVISION_FAILED` / `DEPLOY_FAILED REASON=` each script classifies at the source, through one
  function (`scripts/layer7.sh:__cluster_failure_lane`) and one list
  (`CLUSTER_ENV_REASONS` — blueprint-*, `runtime-unreachable` / `apiserver-unreachable` from the
  shared bounded probes in `lib/common.sh:classify_env_failure` (provision asks the apiserver only
  after `kind create cluster`), disk, prerequisites, `port-held` (a host port the kind config maps: TCP by
  connect, UDP from `/proc/net/udp{,6}`), `runtime-incapable` (`kind create` failed and the same
  node-container shape fails on the node image — docs/TODO.md §G), `operator-declined`, an unreadable cluster) are the operator lane;
  everything else is `FAIL provision-failed` / `FAIL deploy-failed`, the implementer lane, because
  both verbs now run the tree on every gate; `observability-apply-failed` and every
  diff arm are gone, and no container-side cluster write remains but `--provision-org`.
- **Env-tests** 01/26/28 (and the wider fresh-state audit: 26, 28, 31, 35 baselines) are
  fresh-pod-independent: 01's in-effect config is read from MH's published config gauges, not the
  startup log; `FRESH_CLUSTER_HINT` is deleted.

## Open questions

- ~~Local registry vs `kind load` for images~~ — **Resolved (devloop 2): `kind load`.**
  Simpler (no per-devloop registry container, port or node trust configuration), and the
  shape is the same: a registry can later replace `load_image_to_kind` alone. The tag is the
  image's own ID (`<repo>:sha-<first 16 hex>`, `infra/kind/scripts/deploy.sh:content_tag`), not a hash of build
  inputs: an input hash would name different bytes under the same tag whenever an unhashed
  input changed (floating base images, apt packages) and nothing would roll. Cost: one extra
  rollout after a build-cache prune. Per-service precision (a GC-only change not rolling
  MC/MH) needs reproducible, narrowed builds — `docs/TODO.md` "Skip unchanged service image
  builds".
- ~~Where the migrations image comes from, and whether the Job name carries the content
  hash~~ — **Resolved (devloop 2):** `infra/docker/db-migrate/Dockerfile` — `sqlx-cli`
  pinned to Cargo.lock's `sqlx` plus `migrations/` as the last layer — run as
  `infra/services/db-migrate/`'s Job, applied ahead of the root. **Yes, the name carries a
  hash**: `db-migrate-<sha256 of the rendered Job, image tag included>`, so an unchanged set
  re-applies as a no-op and any change (migrations or Job spec) is a new Job — which also
  sidesteps Job-template immutability.
- ~~Whether platform components (Postgres, Redis, the observability stack itself) belong to
  `provision` or to the environment root~~ — **Resolved (step 3): the environment root.**
  Normal tasks change them (every story-2 dashboard, alert-rule, scrape-config and
  collector-allowlist task did), they are content-addressed so an unchanged apply is a no-op
  (Postgres does not restart), and they already were root resources. `provision` holds only
  what cannot change in place: the node topology and host ports (the kind config), the CNI,
  the namespaces and the Secret/TLS material.

## References

- ADR-0030 (devloop cluster helper; allowlist trust boundary)
- ADR-0033 (polyglot validation pipeline; Layer 7)
- ADR-0035 (story runner; authority gate)
- `docs/TODO.md`: "Layer 7 cannot deploy an `infra/services/**` change" (2026-09-03)
- Devloop `2026-09-26-l7-cluster-refresh-correct-cheap` (rejected approach, commit `d58e496`)
