# ADR-0038: The Dev Cluster Deploys the Way Production Will

**Status**: Proposed

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

Until then, today's behaviour (a full rebuild on every gate) stays: slow but correct.

## Open questions

- Local registry vs `kind load` for images (a local registry makes step 1 identical to a
  production push; `kind load` is simpler).
- Where the migrations image comes from (a small `sqlx-cli` image plus `migrations/`),
  and whether the Job name carries the migrations content hash so an unchanged set is a
  no-op rather than a re-run.
- Whether platform components (Postgres, Redis, the observability stack itself) belong to
  `provision` or to the environment root; the rule of thumb is that anything a normal
  task changes belongs in the root.

## References

- ADR-0030 (devloop cluster helper; allowlist trust boundary)
- ADR-0033 (polyglot validation pipeline; Layer 7)
- ADR-0035 (story runner; authority gate)
- `docs/TODO.md`: "Layer 7 cannot deploy an `infra/services/**` change" (2026-09-03)
- Devloop `2026-09-26-l7-cluster-refresh-correct-cheap` (rejected approach, commit `d58e496`)
