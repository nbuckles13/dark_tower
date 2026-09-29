# Devloop Output: ADR-0038 devloop 2 — migration Job + content-tagged images

**Date**: 2026-09-28
**Task**: ADR-0038 §Implementation item 2: migrations image + in-cluster Job; image tags derived from image content and wired into the Kind environment root; host `sqlx` path retired. Carried: consolidate onto the shared `configMapGenerator` parser; observability confirms retired `dashboard_configmap_label` rule
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present <!-- panel mode + Gate-1 tier (ADR-0037 §D2); see the Tier row in Loop State -->
**Branch**: `feature/adr0038-task2-3`
**Duration**: ~2 days wall-clock (2026-09-28 → 2026-09-29, incl. a host devloop-image rebuild pause)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ad7b53d6a9a269f5adb383167bf119f0ce0fe9d2` |
| Branch | `feature/adr0038-task2-3` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | spawned |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `2` (human review / `--continue`) |
| Security | spawned |
| Test | spawned |
| Observability | spawned |
| Code Quality | spawned |
| DRY | spawned |
| Operations | spawned |
| Database (conditional) | `database` |
| Semantic Guard | not spawned — no check surface (shell/YAML/Dockerfile/guard-tooling Rust; no credential-bearing types) |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Human Review (Iteration 2)

**Feedback**: "finish Gate 3: final layer-all + commit, note that infra/devloop/Dockerfile was updated to get the rebuild to devloop container rebuild to work"

- **Host action done:** the user ran `infra/devloop/devloop.sh --rebuild`. The container now carries `sqlx-cli 0.8.6` (= Cargo.lock), `cargo-llvm-cov 0.8.7` and `cargo-audit 0.22.2`, so the `SQLX_CLI_VERSION_MISMATCH` Layer-4 red is gone.
- **Dockerfile edit (user, post-review):** needed for the rebuild to succeed. `cargo-llvm-cov` and `cargo-audit` are pinned `--locked --version "=…"`, because unpinned installs resolved deps needing a newer rustc than the base image. See §Implementation Summary F. @operations, @security, @database, @code-reviewer and @dry-reviewer reviewed it with no blocking findings.
- **Gate-3 re-review:** a fresh reviewer panel confirmed every earlier fix and raised three new findings, all fixed in this iteration:
  - Operations N1: runbook note on a stale pinned cargo-audit.
  - Observability O5: the noop migration path now also deletes the Succeeded pod.
  - Test #8: pure `agree_on_generation`, with tests.
- **Final Gate 2:** `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final tree ran with `TOTAL_RESULT=N/A`, which is a pass.
  - L1, L2, L3, L5 and L7 are OK.
  - L4 and L6 are N/A only because of proto's intentional-gap placeholders; `cargo-test-passed`, `nx-test-passed`, `cargo-audit-passed`, `pnpm-audit-passed` and `buf-breaking-passed` are all OK.
  - WARN: Layer 3 `BUDGET_BREACH` (130s vs 20s), known and noted under §Residuals.

---

## Task Overview

### Objective
ADR-0038 §Implementation item 2: migrations run as an in-cluster Job from an image built from `migrations/`, before rollouts; every first-party image is tagged by its own content and wired into the Kind environment root; the host `sqlx` path is retired. Carried: one `configMapGenerator` parser in `dt-guard`; observability confirms the retired `dashboard_configmap_label` rule.

### Scope
- **Service(s)**: none (no service code). Kind bring-up (`setup.sh`), devloop-helper `rebuild`/`rebuild-all`, service manifests (image placeholder only), new `db-migrate` image + Job, `dt-guard` kustomize machinery
- **Schema**: No (`migrations/` not edited; `_sqlx_migrations` continuity preserved — same table/checksums sqlx-cli wrote before)
- **Cross-cutting**: Yes (every service's deploy path; database run mechanism)

### Debate Decision
NOT NEEDED - ADR-0038 already decided the shape; this devloop resolves its two Open Questions (below) within it.

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/kind/scripts/setup.sh` | Mine | — |
| `scripts/setup.test.sh` | Mine | — |
| `infra/kubernetes/overlays/kind/kustomization.yaml` (header comment) | Mine | — |
| `infra/services/ac-service/statefulset.yaml` (image placeholder tag) | Mine | — |
| `infra/services/gc-service/deployment.yaml` (image placeholder tag) | Mine | — |
| `infra/services/mc-service/mc-*-deployment.yaml` (image placeholder tag) | Mine | — |
| `infra/services/mh-service/mh-*-deployment.yaml` (image placeholder tag) | Mine | — |
| `infra/services/postgres/network-policy.yaml` (ingress from db-migrate) | Mine | — |
| `infra/services/db-migrate/**` (new: Job + NetworkPolicy + kustomization) | Not mine, Minor-judgment | database |
| `infra/docker/db-migrate/Dockerfile` (new: sqlx-cli + migrations) | Not mine, Minor-judgment | database |
| `crates/devloop-helper/src/commands.rs` | Mine | — |
| `crates/devloop-helper/src/protocol.rs` | Mine | — |
| `infra/devloop/dev-cluster` (help text) | Mine | — |
| `crates/dt-guard/src/kustomize.rs` (SERVICE_BASES += db-migrate) | Mine | — |
| `crates/dt-guard/src/kustomize_configmaps.rs` (parser consolidation) | Mine | — |
| `crates/dt-guard/src/common/kustomize_generators.rs` (parser consolidation) | Mine | — |
| `crates/dt-guard/src/kustomize_tools.rs` (R-18 SECURITY_CONTEXT_KINDS += Job, CronJob — decided by @security, Q2) | Not mine, Minor-judgment | security |
| `docs/decisions/adr-0038-dev-cluster-deploy-parity.md` (Open Questions resolved) | Mine | — |
| `docs/decisions/adr-0030-host-side-cluster-helper.md` (rebuild row) | Mine | — |
| `docs/LOCAL_DEVELOPMENT.md` (sqlx-cli no longer needed for the cluster) | Mine | — |
| `docs/TODO.md` (close item B `:latest` race; update item A + "Skip unchanged image builds") | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `infra/skaffold.yaml` (retired — deleted) | Mine | — |
| `infra/lib/cargo-lock-version.sh` (new: Cargo.lock version reader, sourced by setup.sh + devloop.sh) | Mine | — |
| `infra/devloop/Dockerfile` (pin sqlx-cli to Cargo.lock version) | Mine | — |
| `infra/devloop/devloop.sh` (SQLX_CLI_VERSION build-arg at both build sites; stale `:latest` comment) | Not mine, Minor-judgment | operations |
| `infra/devloop/entrypoint.sh` (migration failure fails loudly) | Not mine, Minor-judgment | operations |
| `crates/mc-service/src/lib.rs` (comment: drop `infra/skaffold.yaml` from a path list) | Not mine, Mechanical | meeting-controller |
| `docs/decisions/adr-0013-local-development-environment.md` (skaffold superseded note) | Mine | — |
| `docs/decisions/adr-0024-agent-teams-workflow.md` (rollback line drops `skaffold delete`) | Mine | — |
| `docs/runbooks/devloop-validation.md` (§6.7 rows: setup.sh sub-causes incl. migration-Job triage, stale rebuild NOTE) | Mine | — |
| `docs/runbooks/gc-deployment.md` (§2 migration-Job note) | Not mine, Minor-judgment | operations |
| `docs/runbooks/ac-service-deployment.md` (migration-Job note) | Not mine, Minor-judgment | operations |
| `infra/docker/ac-service/README.md` (dead `dark-tower/*` image names) | Mine | — |
| `crates/dt-guard/src/env_config.rs` (ANCHOR comment rewrite + containment via shared `generator_data`) | Mine | — |
| `scripts/lang/rust/test.sh` (drop masking `has_pending_migrations`; loud sqlx check; unconditional `migrate run`; --no-fail-fast) | Not mine, Minor-judgment | test |
| `scripts/lang/rust/behavior-equivalence.test.sh` (3 migration cases; drop "no pending" mock) | Not mine, Minor-judgment | test |
| `scripts/layer3.sh` (wire behavior-equivalence.test.sh) | Mine | — |
| `crates/env-tests/src/fixtures/kube.rs` (configmap_name_for + REDEPLOY_HINT) | Not mine, Minor-judgment | test |
| `crates/env-tests/src/fixtures/metrics.rs` (settle remedy → REDEPLOY_HINT) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/01_mh_deployment_config.rs` (resolved names; staleness test retired) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/26_mh_quic.rs` (restart-remedy comment) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/28_mh_egress_admission.rs` (resolved names; REDEPLOY_HINT) | Not mine, Minor-judgment | test |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` (resolved names) | Not mine, Minor-judgment | test |
| `scripts/layer7.sh` (env-test cargo --no-fail-fast) | Not mine, Minor-judgment | operations |
| `scripts/layer7.test.sh` (test runners never stop at the first failure) | Not mine, Minor-judgment | operations |
| `infra/docker/*-service/Dockerfile` (deviation; cargo-chef install under the build.jobs cap; builder pinned to bookworm) | Mine | — |
| `docs/specialist-knowledge/operations/INDEX.md` (stale `deploy_only_service()` pointer) | Not mine, Mechanical | operations |

---

## Planning

*Revision 2 — folds in Gate-1 input from @test, @dry-reviewer, @code-reviewer, @database, @operations and the Lead's rulings (skaffold retired; entrypoint.sh in scope; R1 via helper status; R2 recorded).*

### Problem, in mechanism terms
1. **Images are named, not addressed.** Every first-party image is `localhost/<svc>-service:latest`, a mutable name shared by every devloop on the host. Which code runs depends on what `:latest` pointed at when it was last `kind load`ed. Getting new code into pods therefore needs an unconditional `rollout restart`: helper `cmd_rebuild` restarts every workload of the service even when nothing changed. `:latest` is also the cross-slug wrong-code race in TODO §Devloop Container Resource Hygiene item B.
2. **Migrations are a host side-effect.** `setup.sh:run_migrations` port-forwards Postgres and runs host `sqlx migrate run`. It WARNs and continues when `sqlx` is absent, which is a masked failure. It runs only on full setup, so a new migration on a warm Layer-7 cluster (gates that don't touch `infra/kind/`) is never applied. The devloop container's `entrypoint.sh` has the same masking against its unit-test DB.
3. **Two `configMapGenerator` parsers** exist in `dt-guard`.

### A. Content-derived tags: one derivation, every build site
**Where it is derived.** Only `infra/kind/scripts/setup.sh:content_tag <image-id>` derives a tag. It returns `sha-<first 16 hex>` of the image ID and rejects anything that is not a 64-hex sha256 ID.
- Every build site calls it. There are two build sites today:
  - `setup.sh`, via a new `build_content_tagged_image`. It runs `<runtime> build --iidfile`, calls `content_tag`, then `<runtime> tag`. It never produces `:latest`.
  - The helper's `cmd_rebuild`. Its own build, `load_image_to_kind`, `restart_deployment` and `Service::{image_tag,dockerfile}` are deleted, and `rebuild`/`rebuild-all` become calls to setup.sh (§D).
- The migrations image goes through the same derivation.
- Nothing in Rust derives a tag.
- The overlay wiring consumes the result (next bullet), so there is exactly one derivation.

**Every `:latest` site:**
| Site | Change |
|---|---|
| `infra/services/ac-service/statefulset.yaml`, `gc-service/deployment.yaml`, `mc-service/mc-{0,1}-deployment.yaml`, `mh-service/mh-{0,1}-deployment.yaml` | `image:` becomes `localhost/<svc>-service:render-required` |
| `Service::image_tag()` | deleted |
| `setup.sh:891/893` | replaced |

The prose mentions elsewhere (`devloop.sh:279` comment, TODO B) are updated.

**Which repos need a tag** is derived from the rendered root (`extract_manifest_images | grep ^localhost/`), plus `localhost/db-migrate` from its own base. The repo `localhost/<name>` maps to `infra/docker/<name>/Dockerfile` by convention, so there is no hand-kept service list.

**Resolution, per repo:** in order:
1. The ref built in this run.
2. Otherwise, the ref currently deployed. For services this is read from the live workloads found by the render. For migrations it is the image of the `Complete` `db-migrate-*` Job.
3. Otherwise, fail loudly: `no content-tagged image for <repo>; re-run without --skip-build`. A deployed `:latest` from a pre-change cluster is rejected, not reused.

**How tags reach the root without dirtying the tree.** `render_env_overlay` (the existing temp wrapper, one render) now always renders:
- `images:` `newTag` for every first-party repo;
- the advertise-address merges, only when `DT_HOST_GATEWAY_IP` is set.

`apply_env_root` always applies the wrapper, for the static topology too. No `kustomize edit`. In git the image field is the placeholder above. A plain `kubectl apply -k` of the root pulls `:render-required`, which nothing loads, so it fails loudly with ErrImagePull and never silently runs a stale `:latest`. The root's header comment says so.

**Tag source: image ID vs build-input hash (Lead asked for this explicitly).**
- **Decision: the image ID.** It is the image's own content digest, so a tag can never name different bytes. With a warm cache, unchanged inputs mean every layer is cached, so the ID and the tag are the same.
- **Trade-off @operations raised.** After a build-cache miss (prune), unchanged code rebuilds to a new ID and rolls those workloads once. That is an extra rollout, never a wrong one.
- **Why not an input hash:**
  1. It LIES when an input it doesn't hash changes. The floating base tags (`rust:1.91-slim`, `distroless/cc-debian12`) and the distro packages from `apt-get update` all change bytes under an unchanged hash. The same tag would then name a different image, `kind load` would replace it in the node, and nothing would roll: stale code with a green gate. That is the masked-failure class the ADR exists to remove.
  2. The input set is "the build context after `.dockerignore` + Dockerfile + build args + base image digests". Hashing that faithfully means re-implementing `.dockerignore` matching in bash, which is a second copy of the context definition and would drift.
  3. It gives no per-service precision today. Every service Dockerfile `COPY . .`s the whole context, so any source change changes all four input hashes, exactly as it changes all four image IDs.
- **The cache-miss window is small.** Each devloop has a fresh cluster and a warm cache for its lifetime. `devloop.sh` cleanup prunes only unused cache, at teardown.
- **Per-service precision** (a GC-only change not rolling MC/MH) needs reproducible, narrowed builds. That is recorded against the existing TODO "Skip unchanged service image builds", not done here.

**@test's 1a–d** are written for an input hash, so they map to the ID design as follows:
- (a)/(c): `content_tag` is a pure function. Same ID gives the same tag, one changed hex digit gives a different tag, and both `sha256:`-prefixed and bare forms are accepted. A non-sha256 value, an empty value or a short ID FAILS, which is the positive control against a constant-returning function.
- (b)/(d) (mtime, enumeration order, excluded files): these are properties of the engine's layer cache and `.dockerignore`, not of our code. Asserting them hermetically would test podman, not us.
- (e) is covered: the rendered wrapper carries `sha-…` for every first-party repo and no `:latest`/`:render-required`.
- "Migrations tag changes when a file under `migrations/` changes": `migrations/` is the last `COPY` layer, so a changed file is a new layer and a new ID. It is covered at Gate 2 by construction, not hermetically.

**Load:** `kind load`. This resolves the ADR Open Question in favour of `kind load` over a local registry: simpler, no per-devloop registry container, port or node trust configuration. A registry can later replace `load_image_to_kind` without changing the shape. Only refs built in this run are loaded.

**No unconditional restart.** `--only <svc>` loses its `rollout restart`. The apply changes a pod template iff the tag changed.

**Image garbage (@operations 8).** After a successful converge, the ref THIS cluster previously deployed for each repo is removed if a new one replaced it:
- on the host, with `<runtime> rmi`;
- in the Kind node, with `<runtime> exec <node> crictl rmi`, where the node comes from `kind get nodes --name`.

Failure is a WARN (hygiene, not correctness). I don't prune every `sha-*` on the host because another devloop's in-flight build → save window would lose its image. The profile is the same as today's `build_image` old-ID `rmi`. I don't use a blanket in-node `crictl rmi --prune` because it would also drop unused-but-needed preloads. Item A's "precise per-slug rmi at cleanup" still needs a record of deployed refs that outlives the cluster; TODO A is updated with that. It is task-sized (devloop.sh cleanup contract, operations co-owned).

### B. Migrations as an in-cluster Job
**Image `localhost/db-migrate`** (`infra/docker/db-migrate/Dockerfile`, context = repo root with the existing `.dockerignore`):
- Builder stage:
  - `COPY .cargo/` so the build.jobs cap applies; setup.test.sh (E) is extended to `cargo install`.
  - Then `cargo install sqlx-cli --locked --no-default-features --features postgres --version =${SQLX_CLI_VERSION}` (no TLS feature, since in-cluster Postgres is plaintext).
- Runtime stage: `gcr.io/distroless/cc-debian12:nonroot`, then `COPY migrations/ /migrations/` as the LAST layer. `migrations/` is read in place and never copied elsewhere in the tree, and the sqlx-cli layer stays cached.
- Entrypoint: plain `sqlx migrate run --source /migrations --connect-timeout <n>`. No `--ignore-missing`, `--dry-run` or `--target-version`, so stock sqlx fails loudly on a checksum mismatch, a dirty row, or an applied version missing from the source (parity with the retired host path).

**`SQLX_CLI_VERSION`: one derivation from `Cargo.lock`.** A new sourced helper, `infra/lib/cargo-lock-version.sh:cargo_lock_version <crate>` (the `sqlx` entry, currently 0.8.6), is used by:
- `setup.sh`, as a build-arg for db-migrate;
- `infra/devloop/devloop.sh`, as a build-arg for the devloop image, following the existing `read_node_version`/`NODE_VERSION` pattern at both build sites. `infra/devloop/Dockerfile:52` becomes `--version =${SQLX_CLI_VERSION} --locked`. This completes @database's pin-both invariant.

A missing or ambiguous `sqlx` entry fails the build.

**Job (`infra/services/db-migrate/`): `job.yaml`, `network-policy.yaml`, `kustomization.yaml`.**
- Job settings:
  - `backoffLimit: 1`, explicit `activeDeadlineSeconds`, `restartPolicy: Never`;
  - `automountServiceAccountToken: false`, runAsNonRoot/65532, readOnlyRootFilesystem, drop ALL, seccomp RuntimeDefault;
  - requests/limits in line with the other workloads.
- **Credentials:** `secretKeyRef` into **`postgres-secret`**, the database's own Secret and root-managed, not another service's (@database 4). `DATABASE_URL` is composed with `$(POSTGRES_USER)`-style dependent-env expansion, so there is no literal in the manifest or the image, and `describe` shows only the template. It connects as the owning `darktower` role, the identity the host path used, so existing clusters see the same `_sqlx_migrations` rows and the first run is a no-op.
- **DB readiness (@database 6):**
  1. `deploy_postgres` waits for pg_isready Ready before the Job.
  2. sqlx `--connect-timeout`.
  3. `backoffLimit` gives one retry.
  4. The Job's `activeDeadlineSeconds` bounds everything, so it fails loudly with a timeout.
- **NetworkPolicy:**
  - The Job's own policy: podSelector `app: db-migrate`; Egress to kube-dns:53 and to `app: postgres`:5432 only; no ingress.
  - `infra/services/postgres/network-policy.yaml` gains ingress from `app: db-migrate`, TCP 5432. The old port-forward bypassed the netpol through the kubelet; the Job pod doesn't.
- **Not in the root's `resources`; ordered before it** (@operations 1, @database 7). Sequence in every converge:

  `deploy_postgres` (idempotent; applies the Postgres NetworkPolicy that admits the Job) → `run_migration_job` (wait Complete) → [full setup only: seeds / secrets / collector] → `apply_env_root` → `wait_for_env_root`.

  So no AC/GC pod on a new image starts before the schema is migrated. Seeds and `--provision-org` run after the Job. The (D1) byte-identical-subset pin is unchanged, and postgres is still a subset of the root. The Job's base is not in the root, so it is not in the pin.
- **Naming (ADR Open Question: resolved, the name carries a hash).** The Job is rendered through its own temp wrapper:
  - `images:` newTag;
  - then a JSON patch renaming it `db-migrate-<first 10 hex of sha256(the rendered Job with its tag)>`.

  The hash covers the migrations (through the image ID) and the Job spec, so:
  - an unchanged set gives the same name and `apply` is a no-op;
  - any change gives a new Job, never an immutable-template error.
- **`run_migration_job`:**
  - A same-name Job whose condition is `Failed` is deleted and re-applied, with a log line saying so. That is a retry on the next invocation; the earlier failure was already loud. A same-name `Complete` Job is NOT trusted blindly: the wait still reads its condition (@test 2d).
  - The wait polls conditions until `Complete` or `Failed`. The deadline is DERIVED from the render's `activeDeadlineSeconds` plus a margin.
  - Outcomes, each with a line-anchored token:

    | Outcome | Token | Also |
    |---|---|---|
    | `Failed` | `MIGRATION_FAILURE: … REASON=migration-failed` | Job describe, pod logs, events. For a checksum/"previously applied but has been modified" error the remedy line says to recreate the dev cluster: `dev-cluster teardown && dev-cluster setup` / `teardown.sh && setup.sh`, since there are no down migrations |
    | Deadline | `REASON=migration-timeout` | same diagnostics |
    | Success | — | The Job's log is printed into the setup log (evidence kept). Older `db-migrate-*` Jobs are deleted by label (history = the current Job only; no TTL, because the Job object is the no-op record) |

    Both failure outcomes return 1: `setup.sh` exits non-zero, the helper errors, and Layer 7 raises `PRECONDITION_FAILURE cluster-setup-failed/cluster-rebuild-failed`. No new layer7 token is needed.
- **Retired:** `run_migrations`, including its port-forward, its `sqlx-cli not installed` warn-and-continue branch and its `cargo install` hint.
- **`infra/devloop/entrypoint.sh:34-36` (Lead ruling):** fails loudly. If `DATABASE_URL` is set and `/work/migrations` exists, a missing `sqlx` or a failing `sqlx migrate run` exits non-zero with a message; the "may already be applied" tolerance goes, since sqlx is idempotent.
  - I'll check whether a non-zero exit there should abort container start or be reported the way its pg_isready loop is; I'll settle that with @operations at implementation.
  - **Host follow-up:** rebuild the devloop image.
- **Explicitly out of scope, not missed instances (@dry-reviewer 3, @database 9):** these target the container-local unit-test DB, not the cluster, and are already loud:
  - `scripts/lang/rust/test.sh:96-103`'s `sqlx migrate` against the local test DB;
  - `.sqlx/` offline data;
  - migration file contents.

### C. One converge path in `setup.sh`; every entry point after the change
Converge: `build (unless --skip-build) → kind load built refs → resolve refs → deploy_postgres → run_migration_job → apply_env_root (wrapper with tags) → wait_for_env_root → prune superseded refs`.

| Entry point | After this change |
|---|---|
| helper `setup` → `setup.sh --yes` (full) | create cluster → Calico → preload third-party → build all first-party + db-migrate → load → postgres, redis → **migration Job** → seeds → secrets → collector → root with tags → wait |
| helper `rebuild <svc>` | NEW: `setup.sh --yes --only <svc>`, which builds svc + db-migrate, then converges (others keep their deployed refs) |
| helper `rebuild-all` | NEW: `setup.sh --yes --rebuild-all`, which builds every first-party repo + db-migrate, then converges |
| helper `deploy <svc>` → `setup.sh --yes --skip-build --only <svc>` | builds nothing; all refs are the deployed ones; migration Job re-applied by its deployed ref (no-op by name); root applied; wait |
| `layer7.sh` (a) setup / (b) teardown+setup / (c0) `deploy <svc>` / (c) `rebuild-all` | unchanged script. Each routes through the rows above, so every gate now runs the migration Job and applies content tags. A new migration reaches a warm cluster at (c) |
| `layer7.sh` (e2) container-side observability apply | unchanged (no service images in that overlay); no new container-side write is added |
| `iterate.sh` (Telepresence) | untouched; scale-down/up only, unaffected |
| `skaffold` | **retired** (Lead Q1): `infra/skaffold.yaml` deleted, and every reference removed or marked historical (`docs/LOCAL_DEVELOPMENT.md`, ADR-0013 superseded note, ADR-0024 rollback line, `crates/mc-service/src/lib.rs` comment, INDEX, this main.md's rollback template line). There are no CI or `.gitignore` references (checked) |

`--rebuild-all` is an internal mode; ADR step 3 absorbs it into `deploy`.

### D. Helper (`crates/devloop-helper`)
- `rebuild` and `rebuild-all` run setup.sh through ONE shared command builder, also used by `cmd_deploy`. It carries `--yes`, `DT_CLUSTER_NAME`, `DT_PORT_MAP`, `DT_HOST_GATEWAY_IP` and the provider env. Without them the apply would revert the devloop advertise addresses.
- Deleted: `load_image_to_kind`, `restart_deployment`, `Service::{image_tag,dockerfile}`.
- **Status (R1, Lead ruling):** `parse_pod_health` no longer counts a `phase=Succeeded` pod as not-ready. A finished Job pod is not an unhealthy one. `Failed` and everything else still count.
- The verbs, allowlist and socket surface are unchanged.
- Unit tests:
  - the argv/env the builder produces for `rebuild <svc>`, `rebuild-all` and `deploy <svc>`;
  - `parse_pod_health` with a Succeeded pod (healthy), a Failed pod (unhealthy), and only-Succeeded pods (not vacuously healthy: `total` counts only non-Succeeded pods, and 0 of those is unhealthy).

### E. `dt-guard` (machinery; no policy content change)
- `kustomize_configmaps.rs` reads entries through `common::kustomize_generators::parse_config_map_generators`.
- Deleted: `parse_generators`, `Generator`, `Source`, `Mode`, `push_item`, `unquote`, `basename`, `declares_generator_section`, and the `strip_inline_comment` import (if no other use remains).
- **`labels` are NOT added to `GeneratorEntry`.** `last_applied_len(name, data)` never used them; `METADATA_ALLOWANCE_BYTES` covers labels, per its doc. The line scanner parsed them only for the retired label rule. (This answers @test 3b; @code-reviewer 5 agrees.) The fixture with a label literally called `name` is kept and re-pointed: it asserts the shared parser takes the generator name, not the label.
- **Values:** the shared module gains `read_env_file` returning `Vec<(key, value)>`, built on `checked_pair`. `read_env_file_keys` becomes its projection, and literals are split through the same `checked_pair`.
  - This TIGHTENS the size check: a quote-wrapped value or a non-`KEY=VALUE` line in an env file is now a finding instead of being silently skipped or trimmed. That's the fail-loud direction, and it gets its own test.
- **Containment:** ONE shared source resolver, `generator_data(repo_root, kust_dir, entry) -> Result<BTreeMap<key, value>>`, via `path_safety::resolve_cited_path`. `generator_keys` derives from it, so env-config gets containment too.
- **Errors become findings:**
  - An unreadable or unparseable kustomization, a `configMapGenerator:` that is null or not a list, a nameless entry, or a malformed env file becomes a per-file `configmap_annotation_size` finding naming the file; there is no `?` abort.
  - The per-file vacuity check is subsumed: "declares but yields zero" can now only be a null section (an Err, hence a finding) or an explicit `[]`, which is legitimately empty.
  - The whole-tree `measured == 0` guard is kept.
- **Tests:** every existing test stays. The fixture tests (GRAFANA, REORDERED, ZERO_INDENT, section-at-parent-indent, nameless, declared-but-unparsed) are re-pointed at the shared parser or `check()`. Assertions on `Generator`/`Source` internals are rewritten onto `GeneratorEntry` and `generator_data`. The count per module is kept or grows, and any change is itemised in the Implementation Summary.
- **Other scanners and module list:**
  - `kustomize.rs::extract_declared_generator_files` (the R-20 basename scanner, documented as retained) is LEFT as is.
  - Module docs in both files are updated.
  - `SERVICE_BASES` += `db-migrate`, so R-15/16/17 build, orphan-check and schema-check the new base.
  - R-18 `Job` membership is @security's call (Q2).

### F. Tests: new/changed files and wiring
| File | Change | Wired by |
|---|---|---|
| `scripts/setup.test.sh` | changed: `content_tag` pure cases; ref resolution (built > deployed > loud fail; `:latest` rejected); wrapper render carries `sha-` tags for every first-party repo, no `:latest`/`:render-required`, advertise merges only with a gateway; migration Job render (hash name; stable when unchanged, changes when the spec or tag changes); `run_migration_job` with PATH-stubbed kubectl for Complete (rc 0), Failed (rc≠0, `REASON=migration-failed`, pod logs dumped, and in a full `main` run `seed_test_data`/root apply NOT reached), never-completes (distinct `REASON=migration-timeout`), and pre-existing Failed same-name (deleted and re-applied, not counted done); `--only gc` restarts nothing and tags gc with the built ref; `--skip-build` builds nothing and uses deployed refs; host-sqlx absence (no `sqlx` invocation, no `sqlx-cli not installed` branch); (E) extended to `cargo install`; (D4/D5) updated for "always the wrapper" | existing `scripts/layer3.sh` wiring |
| `crates/devloop-helper` unit tests | new cases above | Layer 4 |
| `crates/dt-guard` unit tests | re-pointed and added | Layer 4 |
| `scripts/layer7.test.sh`, `scripts/layer-all.test.sh` | unchanged; must stay green (layer7 stubs setup.sh; no token or lane change) | existing |

No new `*.test.sh`. No Layer 7 REASON-token or lane change, so `docs/runbooks/devloop-validation.md`'s token list is unchanged. §6.7 gains a "migration Job failed / timed out" triage line.

### G. Docs
- `docs/LOCAL_DEVELOPMENT.md`: no host sqlx-cli for the cluster. "setup runs the migration Job"; logs via `kubectl logs -n dark-tower -l app=db-migrate`; checksum-drift remedy = recreate the cluster; skaffold sections removed.
- `docs/runbooks/gc-deployment.md` §2 and `ac-service-deployment.md` (~151): a short note that the migration Job is the mechanism (the dev and future prod shape).
- `docs/runbooks/devloop-validation.md` §6.7: the triage entry.
- ADR-0038 Open Questions: two resolved, with the reasoning above.
- ADR-0030: `rebuild` row updated.
- `docs/TODO.md`: close B; update A and "Skip unchanged service image builds".
- INDEX.md.

**Rollback** (@operations 11): recorded in the Rollback section and in the `setup.sh` header.
- A bad migration in the dev cluster: recreate the cluster (no down migrations).
- A bad image: revert, then `rebuild`/`rebuild-all`. The previous tag was pruned from this cluster, so it is rebuilt; with a warm cache that means the same ID and the same tag.

### H. Carried observability ask
@observability confirms the `dashboard_configmap_label` retirement (commit `2abbd05d`). Recorded under Code Review Results → Observability.

### Risks
- **R1 — finished Job pods read as unhealthy.** The helper's `parse_pod_health` counts every pod in `dark-tower`.
  - The fix is in the helper (§D).
  - Interim, needed ONLY because this devloop's Gate 2 (and any devloop whose host helper predates this change) runs the OLD helper: after printing the Job's log into the setup log, `run_migration_job` deletes the Succeeded pod, never a Failed one. The Job object stays `Complete`, so the no-op property holds, and the evidence is kept in the setup/helper log.
  - It is marked `INTERIM (ADR-0038 step 3): remove once every running helper ignores Succeeded pods`. Without it this devloop's own Gate 2 cannot pass, because status would never read healthy.
- **R2 — ADR-0030 corollary.** Gate 2 runs the host-built helper, so the new `rebuild`/`rebuild-all`/status paths are unvalidated in-run.
  - This is harmless here: this diff touches `infra/kind/`, so every gate does teardown + the BRANCH's setup.sh (content tags + Job).
  - The old helper's `rebuild-all` then builds and loads `:latest` and restarts pods whose template names the sha refs: a wasted restart, not wrong code.
  - Mitigations: helper unit tests (§D). After merge, `devloop.sh` rebuilds the helper from REPO_ROOT at launch.
  - **Remaining action:** the first post-merge Layer 7 exercises the new paths.
- **R3 — cold db-migrate build** compiles sqlx-cli once (~2–4 min), cached thereafter.
- **R4 — host follow-ups:** rebuild the devloop image (sqlx-cli pin + entrypoint change) and the helper, both after merge.

### Open questions for reviewers
- **Q2 (@security):** add `Job` to R-18 `SECURITY_CONTEXT_KINDS`? This is policy content. The Job is written compliant either way.

### Revision 3: Gate-1 conditions folded in (these supersede anything in §A–§H they contradict)

**Lead**
- `entrypoint.sh` is IN scope; its Classification row has been in the table since rev 2. Per @operations it is fixed by **deleting** the block at `infra/devloop/entrypoint.sh:33-37`, not by softening it. That block is a masked duplicate of `scripts/lang/rust/test.sh:run_migrations_if_needed`, which already migrates the unit-test DB and `exit 1`s on failure.
  - `infra/devloop/Dockerfile:52` pins sqlx-cli with `--version =${SQLX_CLI_VERSION} --locked`. `SQLX_CLI_VERSION` is a build-arg passed by `devloop.sh` at both `podman build` sites (the `NODE_VERSION` pattern). Its value comes from the shared `infra/lib/cargo-lock-version.sh`, the same reader `setup.sh` uses; there are no two awk copies.
  - **Host follow-up:** rebuild the devloop image.
- **R1:** the helper fix lands (below). The `setup.sh` Succeeded-pod delete stays as an INTERIM, commented as such, and its removal is recorded in ADR-0038 §Implementation step 3 so it isn't lost.

**Helper status (`parse_pod_health`)**
- A pod is excluded from `total` iff `phase == Succeeded` AND it has an `ownerReferences` entry with `kind: Job`. Every other pod counts exactly as today, including a Failed Job pod and a Succeeded pod not owned by a Job.
- A comment at the site notes that Deployment and StatefulSet pods (restartPolicy Always) can't reach Succeeded anyway. Ownership is the key because the exemption must not extend to anything else.
- A namespace with only exempt pods has `total == 0`, which is unhealthy (the existing `total > 0`).

**Helper command builder (S5)**
- One `setup_sh_command(ctx, verb) -> Command` is the only place a setup.sh invocation is constructed.
- `verb` is a closed enum `{Rebuild(Service), RebuildAll, Deploy(Service)}`. argv is fixed flags plus `Service::as_str()`, and env is `DT_CLUSTER_NAME`, `DT_PORT_MAP`, `DT_HOST_GATEWAY_IP` (validated) and the provider env. No container-supplied string reaches argv or env.
- Gateway validation and the port-map path are shared with `cmd_deploy` through the builder, not duplicated.
- `rebuild-all` is one setup.sh call, not a loop.
- Execution still goes through `run_command_streaming` with the `CancelSignal`.

**Security**
- **Q2:** `SECURITY_CONTEXT_KINDS` becomes `{Deployment, StatefulSet, Job, CronJob}`. The doc comment is rewritten: the list is policy content owned by security, not bash parity. DaemonSet stays out as the host-access carve-out (node-exporter, promtail), and the "Known consequence" paragraph is reworded.
- The ANCHOR comments in `kustomize_tools.rs:117-142` and `env_config.rs:196-209` are rewritten. R-18 = {Deployment, StatefulSet, Job, CronJob}. env-config = {Deployment, StatefulSet, DaemonSet}, with Job/CronJob moot there because it walks only `CANONICAL_SERVICES`. The factual error is fixed: a Job's pod spec is at `spec.template.spec`, and only CronJob nests under `jobTemplate`. The `check_security_context` fn doc points at the const instead of restating kinds. env-config does not add Job.
- **S1 (credentials): adopting @database's safer shape.**
  - The Job gets `PGUSER`, `PGPASSWORD` and `PGDATABASE` via `secretKeyRef` into `postgres-secret`, plus `DATABASE_URL=postgres://postgres.dark-tower.svc.cluster.local:5432`, which carries no userinfo.
  - That avoids three problems: `$(VAR)` ordering and expansion, URL-unsafe passwords (@database 3), and any chance of an error message echoing a password-bearing URL.
  - At implementation I'll verify, from the sqlx-postgres 0.8.6 source in the cargo registry, that URL parsing starts from the `PG*` env defaults, and record the result. If it doesn't, I fall back to `$(VAR)` with the secret refs ordered before `DATABASE_URL` and a URL-safe comment.
  - Dumped Job logs go through a userinfo redaction, `postgres(ql)?://[^@/[:space:]]*@` → `postgres://<redacted>@`. It never dumps `get job -o yaml` or `env`.
  - The live "wrong-password error does not echo the URL" check is a Gate-2/cluster action, recorded as a residual. The URL has no credentials by construction, so the exposure it tests for doesn't exist.
- **S2:** setup.test.sh asserts the rendered db-migrate Job has no literal password value and DOES contain a `secretKeyRef` to `postgres-secret` (the positive control).
- **S3:**
  - The postgres ingress rule for `app: db-migrate` puts namespaceSelector and podSelector in ONE `from` element, as the ac and gc rules do.
  - The Job's own policy has `policyTypes: [Ingress, Egress]` with no ingress rules, so ingress is default-deny.
- **S4a:** `imagePullPolicy: Never` on the Job and on the 6 first-party containers, set in the bases next to the `:render-required` placeholder. A Kind-loaded `localhost/` image must never be pulled, and a prod overlay sets both registry and policy.
- **S4b:**
  - db-migrate's builder uses `ARG RUST_VERSION` with the same default as the service Dockerfiles. setup.test.sh adds a drift check that every `infra/docker/*/Dockerfile` `ARG RUST_VERSION=` default is identical. There are already 4 re-typed copies; this makes them one checked value rather than a 5th unchecked one.
  - distroless: the same `cc-debian12` family and tag convention as the services, not a digest. The reason: the tag is the image ID, so any upstream distroless change produces a new tag, a new Job name and a pending-only re-run. The change is visible and harmless, not silent.
- **S4c:** the deployed-ref reader accepts only `^<repo>:sha-[0-9a-f]{16}$` (a regex); anything else fails loudly.

**Operations**
- **Transition message.** The no-content-tag failure names the repo and the ref it found, then the fix: `dev-cluster rebuild-all`, or `dev-cluster teardown && dev-cluster setup` (host: `setup.sh --rebuild-all`).
- **Node GC.** Superseded refs of THIS cluster are removed in the Kind node with `<runtime> exec <node> crictl rmi <ref>`, and on the host with `rmi` (no `-f`). Both are WARN-only.
  - A comment at the prune site names the cross-slug race: slug A's rmi can land between slug B's build and its load, and B then fails loudly, never with wrong code.
  - kubelet image GC stays as the backstop, and the comment says so.
- **Runbooks.**
  - `devloop-validation.md` symptom catalogue gets a "migration Job failed" entry: logs from the setup dump or `kubectl logs -n dark-tower -l job-name=db-migrate-<hash> --prefix`; checksum drift means restore the file or recreate the cluster, and never hand-edit `_sqlx_migrations`; a Failed Job is deleted and re-created on the next invocation.
  - One line each in `gc-deployment.md` §2 and `ac-service-deployment.md` ~§151.
- **Retired names.** skaffold refs are removed from `LOCAL_DEVELOPMENT.md` (the install block, `skaffold dev`, the command list), `INDEX.md:28`, ADR-0024:167 (becomes `kubectl delete`) and ADR-0013 (superseded note). `infra/docker/ac-service/README.md`'s dead `dark-tower/*` image names are fixed; it goes into the Classification table.

**Observability**
- The label-rule retirement is CONFIRMED and recorded.
- **O1.** On Failed or timeout, logs are dumped for ALL attempts (`kubectl logs -l job-name=<job> --all-containers --prefix --tail=-1`), redacted, BEFORE any cleanup. The one-line failure message carries the Job's terminal condition reason: `BackoffLimitExceeded`, `DeadlineExceeded`, or `setup-poll-timeout`.
- **O2.** On success the Job's log (sqlx's "Applied …" lines) is printed to the setup output before the interim pod delete.
- **O3.** The helper predicate fix is above.

**Database (non-blocking asks, now in plan)**
- (a) `cargo_lock_version` matches `name = "<crate>"` exactly and fails loudly on zero matches or more than one version, with tests.
- (b) The PG* env shape above.
- (c) Residual recorded in the Job comment and in main.md: if the Postgres PVC is wiped while the Complete Job survives, the name-match makes the apply a no-op. Services then fail loudly at the first query, and teardown recreates both.

**DRY**
- One `deployed_ref <repo>` reader keyed by repo; it covers both the workloads and the Complete Job.
- The Job-name hash is computed in exactly one function (`migration_job_name`).
- Literal and env pairs go through the shared `checked_pair`.
- Stale references are updated (`devloop.sh:279`, TODO B closed and its body marked resolved, `LOCAL_DEVELOPMENT.md` host-sqlx sections, the ac README).

**Test (P1)** Old-Job pruning uses `-l app=db-migrate --field-selector metadata.name!=<current>`, and a test asserts the current name never reaches a delete.

**Rollback / residuals added to §Risks**
- R5: "unchanged inputs give the same image ID" relies on the warm build cache and is NOT hermetically tested.
- R6: live verification that sqlx's error doesn't echo the URL is a Gate-2 action; it's moot by construction with the PG* shape.
- R7: wiped PVC plus a surviving Complete Job (above).

### Tests by name

**`scripts/setup.test.sh`** (wired in `scripts/layer3.sh`)

| Area | Cases |
|---|---|
| content_tag / iidfile | `content-tag-same-id-same-tag`, `content-tag-different-id-different-tag`, `content-tag-accepts-sha256-prefix`, `content-tag-rejects-non-sha256`, `content-tag-rejects-empty`, `iidfile-empty-fails` |
| repo derivation | `repos-derived-exact-set` (`localhost/{ac,gc,mc,mh}-service` + `localhost/db-migrate`), `repos-derived-zero-fails`, `repo-without-dockerfile-fails` |
| ref resolution | `resolve-built-wins`, `resolve-deployed-used-when-not-built`, `resolve-none-fails-loud-names-fix`, `resolve-deployed-latest-rejected`, `resolve-deployed-render-required-rejected`, `resolve-two-deployed-refs-fails`, `resolve-kubectl-read-failure-distinct` |
| render | `wrapper-every-repo-tagged`, `wrapper-no-latest-or-placeholder-survives` (whole wrapper render + db-migrate render), `wrapper-advertise-only-with-gateway`, `wrapper-pullpolicy-never` |
| Job render | `job-render-named-by-hash`, `job-name-stable-when-unchanged`, `job-name-changes-with-tag`, `job-name-changes-with-spec`, `job-render-no-literal-password`, `job-render-has-secretkeyref` (S2 positive control), `postgres-ingress-db-migrate-single-from-element` (S3) |
| run_migration_job | `migrate-complete-rc0`, `migrate-failed-rc-token-logs-all-attempts`, `migrate-failed-reason-in-message`, `migrate-timeout-distinct-token`, `migrate-existing-failed-deleted-and-recreated`, `migrate-existing-complete-noop-condition-read`, `migrate-deadline-derived` (fixture value changes the wait), `migrate-no-deadline-fails`, `migrate-prune-never-current`, `migrate-interim-deletes-succeeded-only`, `migrate-interim-delete-failure-fails-setup`, `migrate-success-log-printed`, `migrate-log-redacts-userinfo` |
| setup-level | `main-migrate-failure-stops-before-seeds-and-root` (marker files), `host-sqlx-never-invoked` (PATH-stub `sqlx` records nothing), `no-sqlx-cli-branch-left` (static) |
| entry points | `only-gc-restarts-nothing`, `only-gc-builds-gc-and-db-migrate-and-tags-gc-built`, `skip-build-builds-nothing-uses-deployed`, `rebuild-all-builds-every-repo`, `prune-superseded-host-and-node-warn-only` |
| cargo_lock_version | `cargo-lock-version-exact-match`, `cargo-lock-version-missing-fails`, `cargo-lock-version-ambiguous-fails`, `cargo-lock-version-not-fooled-by-sqlx-core` |
| drift / static | `dockerfile-rust-version-defaults-agree`, (E) extended to `cargo install`, `entrypoint-has-no-sqlx-migrate` (static: the block is deleted, which is the hermetic answer to your 2g) |

**`crates/devloop-helper`**
- `parse_pod_health_job_succeeded_pod_excluded_exact_counts`
- `parse_pod_health_job_failed_pod_is_not_ready`
- `parse_pod_health_succeeded_pod_without_job_owner_is_not_ready`
- `parse_pod_health_succeeded_replicaset_owned_is_not_ready`
- `parse_pod_health_mixed_running_succeeded_pending_exact`
- `parse_pod_health_only_job_pods_is_not_healthy`
- `setup_sh_command_rebuild_argv_env`
- `setup_sh_command_rebuild_all_argv_env`
- `setup_sh_command_deploy_argv_env`
- `setup_sh_command_env_identical_across_verbs`
- `cmd_rebuild_propagates_setup_failure_and_streams_marker` (tempdir stub, REAL `run_command_streaming`)
- `cmd_rebuild_ok_when_setup_succeeds`
- `cmd_rebuild_all_propagates_setup_failure`
- `cmd_rebuild_cancel_kills_setup_child` (sleep-with-child pattern)

Tests deleted along with the dead fns will be listed in the Implementation Summary (any that exist for `image_tag`/`dockerfile`/load/restart).

**`crates/dt-guard`**
- `common::kustomize_generators`: `read_env_file_value_with_equals`, `read_env_file_empty_value`, `read_env_file_rejects_*` (every rejection shape), `literals_split_through_checked_pair`, `generator_data_escaping_source_is_err`, plus the re-pointed fixtures (GRAFANA, REORDERED, ZERO_INDENT, section-at-parent-indent, `labels_name_key_is_not_generator_name`).
- `kustomize_configmaps`: every existing test kept or re-pointed onto `check()`/`GeneratorEntry`, including `a_nameless_entry_is_a_finding`, `a_declared_but_null_section_is_a_finding`, `collecting_zero_generators_is_a_finding_not_a_pass`, and `a_malformed_env_file_is_now_a_finding` (the tightening).
- `env_config`: `generator_source_escaping_repo_is_finding_not_key_lookup`.
- `kustomize_tools`: `job_missing_security_context_is_flagged`, `cronjob_missing_security_context_is_flagged`.
- `kustomize`: `service_bases_include_db_migrate`, `real_db_migrate_base_renders_security_clean` (R-15/R-18 on the real base; skips only as R-15 does, when no tool is present).
- Count delta against @test's baseline (generators 7, configmaps 20, kustomize 11, content_addressing 7, env_config 44): each removed test gets one line in the Implementation Summary.

**Unchanged:** `scripts/layer7.test.sh` and `scripts/layer-all.test.sh` run unchanged and must stay green.

### Revision 3.1: Lead rulings of 2026-09-28 (supersede Revision 3 where they differ)
- **`scripts/lang/rust/test.sh` (Lead ruling; @operations and @test).** Once the `entrypoint.sh` block is deleted, `test.sh` is the ONLY thing that migrates the unit-test DB, and its gate masks failure today:
  - `has_pending_migrations` runs `sqlx migrate info 2>/dev/null || true | grep pending`.
  - If `sqlx` is absent, or `migrate info` fails on checksum drift, it silently skips `migrate run`.

  The fix:
  - Delete `has_pending_migrations`.
  - `run_migrations` first checks `command -v sqlx` and, if it's missing, exits non-zero with `sqlx-cli not found … (the devloop image installs it; see infra/devloop/Dockerfile)`.
  - Then it ALWAYS runs `sqlx migrate run`, which is idempotent; a no-op costs milliseconds. A non-zero exit stops the script before cargo.

  Correction: rev 3 said `run_migrations_if_needed` "already exit 1s on failure". That was only half true, and this fixes the other half.
- **Harness.** `scripts/lang/rust/behavior-equivalence.test.sh` is currently NOT wired into any layer, and its fake `sqlx` printing "no pending" only passed the grep by accident.
  - It gets wired into `scripts/layer3.sh` via `run_and_emit`, like the other self-tests.
  - The "no pending" mock semantics are deleted.
  - Three cases are added: `rust-test-sqlx-absent-fails-before-cargo` (non-zero; the cargo shim's argv log is empty), `rust-test-migrate-run-failure-fails-before-cargo` (the shim's `migrate run` exits 1; non-zero; cargo not invoked), and `rust-test-migrate-run-unconditional-then-cargo` (positive control: the recorded sqlx argv is `migrate run`, with no `migrate info`, and cargo is invoked).
  - If the harness is red once wired, it is fixed in this loop.
- **`REASON=image-unresolved`** (@operations 3). This is a `setup.sh` token, not a Layer 7 lane. The resolution failure prints `IMAGE_UNRESOLVED: no content-tagged image for <repo> (deployed: <ref|none>); build it with 'dev-cluster rebuild-all' (or 'dev-cluster teardown' + 'dev-cluster setup'; host: setup.sh --rebuild-all) REASON=image-unresolved`. A failed kubectl read of deployed refs is a distinct `REASON=image-ref-read-failed`. setup.test.sh asserts both tokens (`resolve-none-fails-loud-names-fix`, `resolve-kubectl-read-failure-distinct`). The `devloop-validation.md` triage line lists `image-unresolved`, `migration-failed` and `migration-timeout`.
- **Corrections to §F.**
  - "No new `*.test.sh`" still holds: no file is new. But two existing harnesses change, and `behavior-equivalence.test.sh` becomes newly wired.
  - "`layer7.test.sh`/`layer-all.test.sh` unchanged" still holds.
  - The test table gains these rows:

    | File | Change | Wired by |
    |---|---|---|
    | `scripts/lang/rust/test.sh` | gate removed, loud sqlx check, unconditional `migrate run` | Layers 4/5 |
    | `scripts/lang/rust/behavior-equivalence.test.sh` | 3 cases above, "no pending" mock removed | **newly** `scripts/layer3.sh` |
- **`real_db_migrate_base_renders_security_clean`**: with no kustomize tool it prints a visible SKIP line, exactly as R-15 does, not a silent `return`.
- **DATABASE_URL** carries no `sslmode`, so the default `prefer` applies (@database).
- **S1 verification (sqlx PG* env defaulting), done 2026-09-28 against `sqlx-postgres-0.8.6` source:**
  - `src/options/parse.rs:parse_from_url` starts from `Self::new_without_pgpass()`.
  - That seeds username, password and database from `PGUSER`, `PGPASSWORD` and `PGDATABASE` (`src/options/mod.rs`, around lines 57-84).
  - The URL then overrides only non-empty parts: username if non-empty, password if present, database if the path is non-empty.

  So `DATABASE_URL=postgres://postgres.dark-tower.svc.cluster.local:5432` plus the PG* secretKeyRefs connects as `postgres-secret`'s identity. The PG* shape is adopted, and no `$(VAR)` fallback is needed.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

Implemented per plan rev 3.1. `./scripts/layer-fast.sh`: **TOTAL_RESULT=N/A, rc 0** (L1 OK, L2 OK, L3 OK, L4 N/A, L5 OK, L6 N/A).

### A. Content-derived image tags (`infra/kind/scripts/setup.sh`)
| Item | Before | After |
|------|--------|-------|
| Tag | `localhost/<svc>-service:latest` (mutable, shared across devloops) | `localhost/<name>:sha-<first 16 hex of image ID>`, derived ONLY in `content_tag()` |
| Build | `build_image` (`-t :latest`, rmi old) | `build_content_tagged_image` (`--iidfile`, content_tag, tag, `kind load`); a missing or empty iidfile fails |
| Which repos | hand list `ac gc mc mh` | `first_party_repos()`, derived from the root render plus the db-migrate base; zero repos, or a repo with no `infra/docker/<name>/Dockerfile`, fails |
| Refs for non-built repos | n/a (`:latest`) | `deployed_refs()` (the one reader, keyed by repo) → `resolve_image_refs()`: built, else a single deployed `sha-` ref (regex), else `IMAGE_UNRESOLVED … REASON=image-unresolved`; an unreadable cluster gives `REASON=image-ref-read-failed` |
| Root wiring | plain root (static) / wrapper (devloop) | ALWAYS the rendered wrapper: `images: newTag` for every first-party repo, plus advertise merges only with a gateway |
| Base manifests | `:latest`, `IfNotPresent` | `:render-required`, `imagePullPolicy: Never` (a plain apply fails loudly) |
| Restarts | `--only` `rollout restart`ed the service | none: the apply rolls exactly what changed |
| Garbage | rmi the old `:latest` ID | `prune_superseded_images`: this cluster's replaced refs, host `rmi` plus in-node `crictl rmi`, WARN-only; the cross-slug race is commented |

### B. Migration Job
- `infra/docker/db-migrate/Dockerfile`: the sqlx-cli version (`=${SQLX_CLI_VERSION}`, `--locked`, `postgres` only) is built under the `.cargo/` cap. It runs on distroless `cc-debian12:nonroot`, with `migrations/` as the LAST layer. The entrypoint is `sqlx migrate run --source /migrations --connect-timeout 30`.
- `infra/services/db-migrate/`:
  - Job: backoffLimit 1, activeDeadlineSeconds 300, restartPolicy Never, no SA token, runAsNonRoot 65532, readOnlyRootFilesystem, drop ALL, seccomp RuntimeDefault, requests and limits.
  - Env: PGUSER, PGPASSWORD and PGDATABASE come from `postgres-secret`; `DATABASE_URL` holds the host and port only.
  - NetworkPolicy: Ingress and Egress listed, egress to DNS and postgres:5432 only.
  - The postgres NetworkPolicy gains ingress from `app: db-migrate` as ONE `from` element.
- `run_migration_job`:
  - Renders through `render_migration_job` and names the Job `db-migrate-<sha256(rendered Job)[:10]>` via `migration_job_name`, the only derivation of that name.
  - The wait deadline is derived from `activeDeadlineSeconds` plus `DT_JOB_WAIT_MARGIN_SECONDS` (default 60); a missing `activeDeadlineSeconds` fails.
  - Existing Job: if it is `Complete`, nothing is applied (its condition is still read). If it is `Failed`, it is deleted and re-created.
  - `Failed` → `MIGRATION_FAILURE … (reason=<Job reason>) REASON=migration-failed`. Timeout → `REASON=migration-timeout` (reason=setup-poll-timeout).
  - On failure it dumps redacted logs from all attempts, plus describe and events, BEFORE any cleanup.
  - On success:
    1. Prints the redacted Job log.
    2. INTERIM (ADR-0038 step 3): deletes the Succeeded pod only; a failed delete fails setup.
    3. Prunes the other `db-migrate-*` Jobs, excluding the current one via field-selector.
- Retired: `run_migrations` (port-forward + host `sqlx`, silent skip).
- Ordering in every path: postgres → Job Complete → seeds/secrets/collector (full setup) → root → wait.

### C. One converge path
`deploy_services <svc|otel|all>` → `build_first_party_images` → `converge_env_root` (resolve → postgres → Job → root → wait → prune).
- `--only <svc>` builds the service and db-migrate.
- `--rebuild-all` (new) builds every repo; combining it with `--only` or `--skip-build` is rejected.
- `--skip-build` builds nothing.
- Full setup follows the same order.
- Container-side cluster writes: none added.

### D. Helper (`crates/devloop-helper`)
- `SetupVerb {Rebuild, RebuildAll, Deploy}` goes through `setup_sh_command()`. That is the ONLY place the setup.sh argv and env are built: fixed flags plus `Service::as_str()`, with DT_CLUSTER_NAME, DT_PORT_MAP, the validated DT_HOST_GATEWAY_IP and the provider.
- `run_setup_verb` streams through `run_command_streaming` with the CancelSignal.
- Deleted: the direct build, `load_image_to_kind`, `restart_deployment`, `Service::{image_tag,dockerfile}`. `Service::ALL` is now test-only. No tests existed for the deleted functions, so none were deleted.
- `parse_pod_health`: pods with `phase==Succeeded` AND a Job-kind ownerReference are excluded from `total`.

### E. dt-guard (machinery; policy-content change only where @security ruled)
- `kustomize_configmaps.rs` reads through `common::kustomize_generators`.
  - The whole line parser is deleted: `parse_generators`, `Generator`, `Source`, `Mode`, `push_item`, `unquote`, `basename`, `declares_generator_section`, `resolve_source_path`, `resolve_data`, and the `strip_inline_comment` import.
  - Any parse error becomes a per-file finding.
  - The remedy text no longer tells people to "carry the Grafana sidecar label".
- Shared module additions:
  - `checked_pair` now returns `(key, value)`.
  - `read_env_file` (pairs); `read_env_file_keys` projects from it.
  - `GeneratorEntry::literal_pairs`.
  - `generator_data(repo_root, kust_dir, entry)`, containment-gated with distinct escape and missing errors; `generator_keys` projects from it. env-config passes `repo_root` and gains containment.
- R-18 `SECURITY_CONTEXT_KINDS` = {Deployment, StatefulSet, Job, CronJob}, per @security's ruling; the doc is rewritten as security-owned policy and the DaemonSet carve-out is kept.
- Both ANCHOR comments were rewritten, including the `spec.template.spec` fix.
- `SERVICE_BASES` += `db-migrate`.
- Tree: `dt-guard kustomize` OK (23 consumed ConfigMaps); `env-config` OK.
- Test counts vs @test's baseline, no net loss:

  | Module | Before | After |
  |---|---|---|
  | generators | 7 | 13 |
  | configmaps | 20 | 25 |
  | kustomize | 11 | 13 |
  | content_addressing | 7 | 7 |
  | env_config | 44 | 45 |
  | kustomize_tools | — | +3 |

  Labels assertions were dropped from the re-pointed fixtures (the rule is retired, and the size model never used labels). The `ZERO_INDENT` stray-bullet tail moved into its own fixture, which the YAML parser rejects and `check` reports.

### F. Unit-test DB migration + devloop image
- `scripts/lang/rust/test.sh`: `has_pending_migrations` is deleted. `run_migrations` fails loudly without sqlx, always runs `migrate run`, and a failure exits before cargo.
- `infra/devloop/entrypoint.sh`: the masked migrate block is deleted.
- `infra/devloop/Dockerfile`: `ARG SQLX_CLI_VERSION` (required), `--locked --version`.
  - **Post-review host edit (user, 2026-09-29), needed for `devloop.sh --rebuild` to succeed:** `cargo-llvm-cov` and `cargo-audit` are now pinned too (`ARG CARGO_LLVM_COV_VERSION=0.8.7`, `ARG CARGO_AUDIT_VERSION=0.22.2`) and installed with `--locked --version "=…"`. Unpinned, `cargo install` resolved their newest dependencies, which needed a newer rustc than the base image. The values match what the previous image carried. Bumping them is a deliberate Dockerfile edit.
- `infra/devloop/devloop.sh`: `read_sqlx_cli_version` passes the build-arg at both build sites via `infra/lib/cargo-lock-version.sh` (the one reader, shared with setup.sh; exact name match, and zero or more than one version fails).

### G. Tests (all green)
- `scripts/setup.test.sh`: 296 cases, up from 160. All plan-listed cases are included, plus mutation-checked S3 and harness checks.
- `scripts/lang/rust/behavior-equivalence.test.sh`: now WIRED in `scripts/layer3.sh` as `rust-test-verb-selftest`; 12 cases, including the 3 migration cases. Mutation check: the old `test.sh` fails 4 of them.
- `crates/devloop-helper`: 199 passed (14 new), stable over 8 consecutive runs.
- `crates/dt-guard`: 648 passed.
- `layer7.test.sh` (277) and `layer-all.test.sh` are unchanged and green.

### Deviations from the plan (reviewers notified)
1. **`infra/docker/*-service/Dockerfile` (4 files).** Extending setup.test.sh (E) to `cargo install` (plan item) found that `RUN cargo install cargo-chef` in every service Dockerfile compiled OUTSIDE the build.jobs cap. Fixed in-tree: `WORKDIR /build` + `COPY .cargo/ .cargo/` before it. One-time rebuild of the chef layer.
2. **`behavior-equivalence.test.sh` was red once wired** (as @test predicted might happen). `scripts/test.sh` is polyglot now and passes `--workspace` to vitest. Fixed by scoping the dispatcher run to `DEVLOOP_DISPATCH_INCLUDE_LANGS=rust`, which is the equivalence under test.
3. **Helper-test flake (ETXTBSY).** A stub setup.sh written by the test process could be "text file busy" when a parallel test thread forked. The stub is now installed by a child process (`install -m 0755`).
4. **`log_error` inside `$(...)` was swallowed.** Functions used in command substitution (`content_tag`, `first_party_repos`, `render_migration_job`, `job_active_deadline`) now log to stderr. Caught by the `migrate-no-deadline-says-so` test.
5. `docs/specialist-knowledge/operations/INDEX.md`: stale `deploy_only_service()` pointer (Mechanical, operations).

### Gate 2 attempt 1 fix — glibc mismatch in db-migrate
**Problem:** both Job attempts died at exec with `/usr/local/bin/sqlx: … version 'GLIBC_2.39' not found`. The builder `rust:1.91-slim` floats with Debian and is now trixie (glibc newer than 2.36). The runtime `gcr.io/distroless/cc-debian12` is bookworm (glibc 2.36).

**Fix:** builders now name their release explicitly: `rust:${RUST_VERSION}-slim-bookworm`, which matches distroless `cc-debian12`. I applied this to ALL FIVE `infra/docker/*/Dockerfile`s, not only db-migrate. The service images carry the same latent hazard: the same floating tag on top of a debian12 runtime, and it breaks whenever their binaries pick up a newer glibc symbol.

**Guard:** `setup.test.sh` gets `dockerfile-builder-runtime-same-release`, run over every rust-built Dockerfile.
- Every rust builder must pin exactly one `-slim-<codename>`.
- Every `gcr.io/distroless/cc-debianN` stage must be that release (codename→N map: bookworm=12, trixie=13; an unknown codename fails).
- Non-vacuous: at least 5 Dockerfiles are checked.
- Negative controls: the Gate-2 shape (floating `-slim`) trips, and a trixie builder on a debian12 runtime trips.

**S4 (base pinning):** unchanged in spirit. Both bases are release-pinned tags, not digests. A tag drift inside a release shows up as a new image ID, which means a new tag and a new Job, so it is visible and never silent. The residual class (a release mismatch) is now statically guarded.

**Not verifiable in-container:** there is no podman here, so the rebuilt image is only proven at Gate 2 attempt 2.

### Gate 2 attempt 2 (operator lane; not diff-caused): a half-built cluster was reused — patch made, then REVERTED
**Problem:** attempt 1 left a half-built cluster. Layer 7 then went not-ready → `dev-cluster setup` → "Cluster exists, reusing" → `kubectl create -f calico.yaml` → AlreadyExists on every object → exit 1, in 10s. It never reached the `infra/kind/` teardown arm. The Lead tore down the stale cluster before the retry.

**Patch made, then reverted (user decision):** I made `install_calico` skip the `create` when `daemonset/calico-node` already existed, with three setup.test.sh cases. It was **reverted**: `install_calico` is byte-identical to the start commit, and the three cases and the stub's `daemonset` arm are gone.
- **Why:** the patch gave setup.sh a way to REUSE a half-built cluster. ADR-0038 §1 rejects that: a failed build must never be reused, and a cluster only claims a blueprint after it is successfully built.
- **Where the real fix lives:** `provision`'s recorded-after-success blueprint hash (ADR-0038 step 3). The user confirmed step 3 lands before any real usage.
- **Recorded:** ADR-0038 §Implementation step 3 now carries this observation, and notes that Layer 7's check-health-then-reuse order is retired with it.

**Kept:** the 4 `reuse-safe-create-*` cases. They pin PRE-EXISTING behaviour on its own merits: the imperative Secrets and namespaces use `create --dry-run=client -o yaml | apply -f -`, which the NORMAL path needs, because `deploy_services` re-creates AC/MC/MH secrets on every `--only ac|mc|mh` against a live, successfully built cluster. Nothing in them makes setup reuse a half-built cluster.

**Gate 2 retry (reported by the Lead):** it reached the env-tests. The migration Job applied every migration, which PROVES the attempt-1 glibc fix. It then failed in env-test 01; the user is ruling on how to handle that.

### Gate 2 attempt 3 → env-test adaptation (user ruling; one extra Layer 7 attempt granted)
**Why:** the retry reached the env-tests and failed in env-test 01. Story 2 was written before step 1, and the absorb (`2abbd05d`) missed `crates/env-tests`: the tests still read ConfigMaps by their bare generator names (`mh-service-config`, `mc-service-config`), which no longer exist once names are content-addressed.

**A. Env-tests adapted to content-addressed ConfigMaps** (owner @test):
1. **`env_tests::fixtures::kube::configmap_name_for(instance, base)`** is the ONE resolver. It reads the live pods for `instance` (terminating ones ignored), collects every ConfigMap name their spec references (env `configMapKeyRef`, `envFrom`, `configMap` and projected volumes), and keeps the generations of `base` (`base` or `base-<lowercase-alnum hash>`; `mh-service-config-extra-<h>` does not match).
   - It panics loudly on zero or more than one generation (a rollout in flight, or a pod not wired to `base`).
   - It uses no label selector on ConfigMaps, because superseded generations are not pruned.
   - Unit-tested: generation matching, and reading references from env, envFrom, volumes and projected sources.
   - Used everywhere a bare generated name was:
     - env-test 01: per-instance `shared_configmap_for`, plus `shared_configmap()`, which asserts both MH instances run ONE generation; the wiring assertion now checks each pod's refs against ITS generation; and the `mc-service-config` read.
     - env-test 28: the `mc`/`mh-service-config` reads (×4).
     - env-test 29: two reads through one resolved name.
   - `configmap_key` / `configmap_u64` take the resolved name, as their doc now says.
2. **`test_mh_pods_started_after_configmap_last_changed` is DELETED**, together with its helpers `assert_pod_started_after_configmap_change` and `assert_k8s_timestamp`, and `fetch_configmap`'s `--show-managed-fields`, which only it needed. 01's module doc marks (d) RETIRED and explains why: a changed value is a new ConfigMap name, hence a new pod template, hence a rollout, and Layer 7 waits for every rollout. The "Expected red on a hand-applied value change" section is rewritten as "Stale values after a config change".
3. **`REDEPLOY_HINT`** (one constant in `fixtures/kube.rs`, commented "ADR-0038 step 3 changes this to `dev-cluster deploy`") names `dev-cluster rebuild-all` / `./infra/kind/scripts/setup.sh --rebuild-all`. Every CONFIG-STALENESS remediation references it:
   - 01's parity, ceiling and resolver messages.
   - 01's two "deploy through the Kind overlay" messages. These advised a plain `kubectl apply -k` of a service overlay, which is now actively WRONG because the bases carry `:render-required` image placeholders.
   - 28's two "pods predate the ConfigMap" messages.
   - **Runtime-state remediations → `FRESH_CLUSTER_HINT` (user ruling):** three messages need a fresh pod, not config convergence, because a converge does not restart pods whose image/config is unchanged:
     - 01's startup-log rotation;
     - 28's MH stream-ceiling ratchet;
     - 26's wedged injected generation.
     
     They now reference ONE second constant, `fixtures::kube::FRESH_CLUSTER_HINT`: "`dev-cluster teardown`, then re-run; Layer 7 rebuilds a missing cluster (after ADR-0038 step 3: `provision`)". Each message keeps its explanation of why a converge won't fix it.
     - **Why not `kubectl rollout restart`:** env-test readers are mostly devloop agents in the container. A direct kubectl write there bypasses the host helper (ADR-0030/0038), and the helper has no restart verb by design. No `rollout restart` instruction remains in `crates/env-tests`.
     - **Scope:** `setup.sh:print_access_info` keeps its "Reset Runtime State Only" restart lines, because that is host output for a developer who has host kubectl.
     - **ADR-0038 §Implementation step 3** now records that these three tests assume a freshly started pod, will recur under `deploy`'s no-unconditional-restart rule, and are to be made fresh-pod-independent rather than given a helper restart verb.
4. **Remaining bare generated names:** I grepped `crates/env-tests` for the full rendered set (`ac/gc/mc/mh-service-config`, `mc/mh-{0,1}-config`, `grafana-*`, `loki-config`, `otel-collector-config`, `prometheus-config`, `prometheus-rules`, `promtail-config`, `redis-config`). None remain outside `configmap_name_for` call sites, the `SHARED_CONFIGMAP` base constant, and the resolver's unit tests.

**B. Test runners never stop at the first failure** (owner @operations):
- `scripts/lang/rust/test.sh`: `cargo test --no-fail-fast`.
- `scripts/layer7.sh`: `cargo test --no-fail-fast -p env-tests --features all`, and its header.
- TS L4 (nx run-many + vitest) and L7 Playwright already run everything, so no flags were added.
- ONE static block in `scripts/layer7.test.sh` pins it:
  - both cargo commands carry `--no-fail-fast`;
  - `scripts/lang/ts/test.sh` and `layer7.sh` carry no `--nxBail` / `--nx-bail` / `--bail` / `--max-failures`;
  - the Playwright config and every `packages/*/vitest*` config carry no `maxFailures` / `bail:`;
  - non-vacuity checks that each pinned runner is present.
- layer-all.sh's cross-layer fail-fast is untouched.

### Residuals
- **R2 (ADR-0030 corollary):** Gate 2 runs the host-built helper, so the new rebuild, rebuild-all and status paths are covered by unit tests only.
  - On this branch every gate tears down and re-runs the BRANCH's setup.sh, because the diff touches `infra/kind/`.
  - The interim Succeeded-pod delete keeps the old helper's status healthy.
  - **Host follow-ups after merge:** rebuild the helper (devloop.sh does this at launch) and the devloop image (`devloop.sh --rebuild`: sqlx-cli pin + entrypoint). The first post-merge Layer 7 run exercises the new `rebuild-all`.
- **R5:** "Unchanged inputs keep the same image ID" relies on a warm build cache; this is not tested hermetically.
- **R6:** Live check that sqlx's connection error does not echo a URL is a Gate-2/cluster action. It is moot by construction: the URL carries no userinfo, which I verified in the sqlx-postgres 0.8.6 source.
- **R7:** A wiped Postgres PVC plus a surviving Complete Job means the migrations are not re-run. That failure is loud at the first query, and teardown recreates both. This is commented in `job.yaml`.
- **Layer 3 runtime:** setup.test.sh went from about 2s to about 18s (many real kustomize renders). This devloop's Layer 3 measured ~130s. The budget is a WARN, but the next addition should look at batching renders.

---

## Files Modified

```
 crates/devloop-helper/src/commands.rs              | 620 ++++++++++++------
 crates/devloop-helper/src/protocol.rs              |  32 +-
 crates/dt-guard/src/common/kustomize_generators.rs | 257 ++++++--
 crates/dt-guard/src/env_config.rs                  |  52 +-
 crates/dt-guard/src/kustomize.rs                   |  60 +-
 crates/dt-guard/src/kustomize_configmaps.rs        | 665 ++++++-------------
 crates/dt-guard/src/kustomize_tools.rs             |  74 ++-
 crates/mc-service/src/lib.rs                       |   2 +-
 docs/LOCAL_DEVELOPMENT.md                          |  83 ++-
 docs/TODO.md                                       |  12 +-
 .../adr-0013-local-development-environment.md      |  12 +-
 docs/decisions/adr-0024-agent-teams-workflow.md    |   2 +-
 .../decisions/adr-0030-host-side-cluster-helper.md |   6 +-
 .../adr-0038-dev-cluster-deploy-parity.md          |  29 +-
 docs/runbooks/ac-service-deployment.md             |   7 +
 docs/runbooks/devloop-validation.md                |   2 +-
 docs/runbooks/gc-deployment.md                     |   7 +
 docs/specialist-knowledge/infrastructure/INDEX.md  |  16 +-
 docs/specialist-knowledge/operations/INDEX.md      |   2 +-
 infra/devloop/Dockerfile                           |  12 +-
 infra/devloop/dev-cluster                          |   6 +-
 infra/devloop/devloop.sh                           |  36 +-
 infra/devloop/entrypoint.sh                        |  11 +-
 infra/docker/ac-service/Dockerfile                 |   4 +
 infra/docker/ac-service/README.md                  |   5 +
 infra/docker/gc-service/Dockerfile                 |   4 +
 infra/docker/mc-service/Dockerfile                 |   4 +
 infra/docker/mh-service/Dockerfile                 |   4 +
 infra/kind/scripts/setup.sh                        | 714 ++++++++++++++++-----
 infra/kubernetes/overlays/kind/kustomization.yaml  |  18 +-
 infra/services/ac-service/statefulset.yaml         |   9 +-
 infra/services/gc-service/deployment.yaml          |   9 +-
 infra/services/mc-service/mc-0-deployment.yaml     |   9 +-
 infra/services/mc-service/mc-1-deployment.yaml     |   9 +-
 infra/services/mh-service/mh-0-deployment.yaml     |   9 +-
 infra/services/mh-service/mh-1-deployment.yaml     |   9 +-
 infra/services/postgres/network-policy.yaml        |  13 +
 infra/skaffold.yaml                                | 106 ---
 scripts/lang/rust/behavior-equivalence.test.sh     |  79 ++-
 scripts/lang/rust/test.sh                          |  32 +-
 scripts/layer3.sh                                  |   4 +
 scripts/setup.test.sh                              | 436 +++++++++++--
 42 files changed, 2306 insertions(+), 1176 deletions(-)
(new, untracked) infra/docker/db-migrate/Dockerfile, infra/services/db-migrate/{kustomization,job,network-policy}.yaml, infra/lib/cargo-lock-version.sh
```

### Key Changes by File
See Implementation Summary §A–§F.

---

## Devloop Verification Steps

Final `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (2026-09-29, final tree), `TOTAL_RESULT=N/A` (pass), 1812s:

| Layer | Result | Duration (s) | Notes |
|-------|--------|--------------|-------|
| 1 Compile | OK | 6 | |
| 2 Format | OK | 6 | |
| 3 Guards | OK | 130 | WARN BUDGET_BREACH (budget 20s); see §Residuals |
| 4 Test | N/A | 294 | proto N/A (intentional gap); `cargo-test-passed`, `nx-test-passed` |
| 5 Lint | OK | 10 | |
| 6 Audit | N/A | 6 | proto N/A; `cargo-audit-passed`, `pnpm-audit-passed`, `buf-breaking-passed` |
| 7 Env-tests | OK | 1360 | dev-cluster rebuild + Rust env-tests + browser E2E |

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED-FIXED (re-review confirmed 2026-09-29)
**Findings**: 1 found, 1 fixed, 0 deferred

- **F1 — the S2 needle was typed from memory (`dev_password`).**
  - **Fix:** it is now READ from `infra/services/postgres/secret.yaml` (`POSTGRES_PASSWORD`), with its own non-empty assertion (`job-render-password-needle-read`) before `assert_absent`.
  - **Also added:** a same-shape check that the user value never appears as a literal `value:`.
  - setup.test.sh: 320 passed.

### Test Specialist
**Verdict**: RESOLVED-FIXED (re-review confirmed 2026-09-29)
**Findings**: 8 found, 8 fixed, 0 deferred

1. **behavior-equivalence red / vacuous after the sqlx drift check.**
   - The shim now answers `--version` with the Cargo.lock version, derived through the ONE reader, and does not log it (landed with database F1).
   - migrate-fails also asserts "Failed to run migrations" (attribution).
   - Version-mismatch cases exist as `rust-test-sqlx-version-drift-*`: non-zero exit, both versions named, no migrate, no cargo.
   - `run_equivalence` asserts that BOTH argv logs are non-empty and carry `cargo test … <first arg>` before diffing.
   - 22 passed.
2. **`is_generation_of` was looser than its doc.** It now requires exactly `base-` + a 10-char lowercase-alnum hash. A pod referencing the BARE base is a loud "not content-addressed" error. There are negative tests for the bare base, `base-extra`, `base-extra-<hash>`, a wrong base and uppercase.
3. **`configmap_name_for` panic paths were untested.** The logic is extracted into the pure `resolve_generation(pods_json, instance, base) -> Result`, and the caller panics on `Err`. Tests cover one generation, a terminating old generation ignored, an initContainer reference, zero generations, two generations, only-terminating pods, missing `items`, and a bare base.
4. **Single-instance resolution.** The new fixture `configmap_name_for_all(&[instances], base)` asserts that every instance agrees. It is used in 01 (`shared_configmap()` and the `mc-service-config` read), 28 (resolved ONCE per test into `mc_config`/`mh_config`) and 29.
5. The unused `use env_tests::NAMESPACE` in 28 is removed.
6. **The cancel test could mistake a zombie for a survivor.** `process_alive()` now also treats a gone `/proc/<pid>/stat` or state `Z` as dead.
7. **The env-config escape test wrote into the shared system temp dir.** It now builds the repo at `td/repo` via a new `fixture_at` and places `outside.env` in `td/`.
8. **(Gate-3 re-review) `configmap_name_for_all`'s agreement check was untested** (it called kubectl). Extracted the pure `agree_on_generation(instances, base, names) -> Result`; tests `instances_on_one_generation_agree`, `instances_on_different_generations_is_an_error`, `no_instances_is_an_error`.

### Observability Specialist
**Verdict**: RESOLVED-FIXED (re-review confirmed 2026-09-29)
**Findings**: 2 found, 2 fixed, 0 deferred

- **O4 — the no-op migration path logged "Migrations applied".**
  - **Problem:** when the Job of this name was already Complete at entry, `run_migration_job` fell through to the success tail. It ran a `kubectl logs` on a pod that had already been deleted (so the log got "No resources found" where the record belongs), printed "Migrations applied" (false), and ran a pointless pod delete.
  - **Fix:** the Complete-at-entry branch sets `noop`. That path skips the log dump and the pod delete, and says "Migrations unchanged (job/<name> already Complete); nothing applied." The older-Job prune still runs.
  - **Tests:** setup.test.sh's `migrate-existing-complete-*` block now asserts "Migrations unchanged", the absence of "Migrations applied", no `logs` call, no pod delete, and that the prune still runs. The real-run case asserts "Migrations applied". 318 passed.
- O1–O3 were confirmed implemented by @observability.
- **O5 (Gate-3 re-review) — the noop path skipped the interim Succeeded-pod delete.** A converge interrupted between Job Complete and the delete left the pod behind; the next converge saw Complete-at-entry and never removed it (an old helper then reads "cluster did not become healthy"). **Fix:** the delete moved after the if/else in `run_migration_job`, so it runs on both paths (`--ignore-not-found`, Succeeded-only). `migrate-existing-complete-no-pod-delete` → `migrate-existing-complete-still-deletes-succeeded-pod`. 320 passed.

**Carried ask (ADR-0038 item 2)**: retired `dashboard_configmap_label` rule (commit `2abbd05d`) — **CONFIRMED by @observability at Gate 1**: its hazard (a dashboard generated but never loaded) is covered structurally by R-20 (JSON↔generator coverage, both directions) and R-21 `configmap_unreferenced` (a group off Grafana's projected volume fails; pinned by `dashboard_group_generated_but_not_mounted_fails`; R-21 fails closed without a kustomize tool); the provisioning-YAML sub-case disappeared with the sidecar.

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED (re-review confirmed 2026-09-29)
**Findings**: 6 found, 6 fixed, 0 deferred

1. `is_job_owned` had split `parse_pod_health` from its rustdoc. It now sits above that doc, which is back on `parse_pod_health`.
2. `PodHealthSummary::is_healthy()` is the one health rule. `cmd_status` calls it, and the tests assert it instead of a retyped copy.
3. **Root fix for swallowed diagnostics.** `log_error`/`log_warn` write to STDERR, and every per-call `>&2` is dropped. Callers that capture already use `2>&1` (layer7.sh `--provision-org` included); setup.test.sh stays green.
4. `deploy_services`: the case now does secrets only. The build list is derived afterwards: `MIGRATE_REPO` plus `localhost/${target}-service`, `otel` → migrations only, `all` → every repo. One mapping, not four copies.
5. The env-config escape test now lives in an owned tempdir; same fix as Test 7.
6. Test 28 resolves each ConfigMap once per test (`mc_config`/`mh_config`); same fix as Test 4. The re-review residual (the threshold read at ~l.359 re-resolved) now reuses `&mh_config`, so there is ONE resolution per test and the threshold comes from the same generation as the limits.

Re-review: F1–F5 verified by @code-reviewer; F6 residual fixed afterwards.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED (re-review confirmed 2026-09-29)

**True duplication findings** (entered fix-or-defer flow), all fixed:
1. `fixtures/metrics.rs` `SETTLE_NOT_ABOVE_INTERVAL` spelled a container-side `kubectl apply -k …/observability/`. It now uses `{REDEPLOY_HINT}` (the root composes observability).
2. `cmd_setup` built its own setup.sh Command. It now uses `SetupVerb::Setup { skip_observability }` through `setup_sh_command`. `port_map_shell_path(ctx)` is the one home of the `port-map.env` path, used by both the writer and the builder. New test: `setup_sh_command_setup_argv_env`; the env-identical test covers all four verbs.
3. The test.sh host hint was unpinned. It is now `--version =<derived>` (landed with database F1) and tested by `rust-test-sqlx-absent-hint-is-pinned`.
4. `first_party_repos_of <dir>` is the one "which first-party repos does this render name" pipeline. `root_repos` and `first_party_repos` use it.
5. ADR-0038 step 3 now names all three sites that spell the remedy (`REDEPLOY_HINT`/`FRESH_CLUSTER_HINT`, setup.sh `IMAGE_UNRESOLVED`, runbook §6.7), so the rename reaches every one of them.

**Extraction opportunities** (appended to `docs/TODO.md`): None.

### Operations Reviewer
**Verdict**: RESOLVED-FIXED (re-review confirmed 2026-09-29)
**Findings**: 4 found, 4 fixed, 0 deferred

- **F1 — DeadlineExceeded reported as migration-failed.** When the Job's own `activeDeadlineSeconds` expires it ends as `Failed/DeadlineExceeded`, which is the common timeout path. That case now emits `REASON=migration-timeout` with the unreachable/not-loaded hint, and never "read sqlx's error". Any other Failed reason stays `migration-failed`. Code and the §6.7 runbook text now agree. New setup.test.sh cases `migrate-deadline-exceeded-*` (5 asserts) include the absence of `migration-failed`.
- **F2 — `print_access_info` still recommended `rollout restart` to pick up code.** It now leads with "Deploy Code/Config Changes (rolls only what changed)": `setup.sh --only <svc>` / `--rebuild-all`. The restart lines are kept, relabelled "Reset Runtime State Only (does NOT pick up new code)".
- **F3 — stale layer7.sh comments.** Comment-only restatements:
  - `__wait_cluster_ready` header: why the poll remains.
  - The MH-forward live-pod filter.
  - (c0) is labelled INTERIM (step 3), redundant on a current helper, and kept for an old one.
  - (e) poll rationale.
  - (e2) is labelled INTERIM: a current helper's rebuild-all already applies the observability overlay; kept for the old-helper case.
  
  layer7.test.sh: 303 passed.
- **N1 (Gate-3 re-review) — a stale pinned cargo-audit vs the always-fresh advisory DB** could red Layer 6 as `cargo-audit-failed` with no RUSTSEC ID. The runbook §6.3 row now names the fix: bump `CARGO_AUDIT_VERSION` + `devloop.sh --rebuild`, never a suppression.

### Database Reviewer (conditional)
**Verdict**: RESOLVED-FIXED (re-review confirmed 2026-09-29; host image rebuilt — `sqlx-cli 0.8.6` = Cargo.lock)
**Findings**: 1 found, 1 fixed, 0 deferred

- **F1 — the devloop image's sqlx-cli pin can drift silently after a `sqlx` bump.**
  - **Fix:** `scripts/lang/rust/test.sh:run_migrations` now derives the wanted version through the ONE reader (`infra/lib/cargo-lock-version.sh`) and compares it with `sqlx --version`.
  - **On mismatch:** it fails loudly before migrating or testing, with its own token: `SQLX_CLI_VERSION_MISMATCH: sqlx-cli <have> != Cargo.lock sqlx <want> … Rebuild the devloop image: infra/devloop/devloop.sh --rebuild (host)`. No version number is hard-coded anywhere.
  - **One source:** the check and the devloop image pin read the SAME value — `devloop.sh:read_sqlx_cli_version` → `cargo_lock_version` → `--build-arg SQLX_CLI_VERSION` → `infra/devloop/Dockerfile`. (User ruling: land it as a HARD check.)
  - The sqlx-absent host hint is now pinned (`--version =<derived>`).
  - **Tests:** behavior-equivalence.test.sh gains:
    - `rust-test-sqlx-version-drift-*`: non-zero exit, its own token plus the devloop rebuild remedy, both versions named, no migrate, no cargo;
    - `rust-test-sqlx-absent-hint-is-pinned`.
    
    The match → proceeds case is the positive control `rust-test-migrate-run-unconditional-then-cargo`, where the shim reports the Cargo.lock version. 23 passed.
  - **Consequence, found immediately:** the CURRENT devloop image carries `sqlx-cli 0.9.0` (unpinned install), against Cargo.lock `sqlx 0.8.6`. It has already drifted, so Layer 4 in this container fails loudly until the devloop image is rebuilt on the host (`devloop.sh --rebuild`, which now pins 0.8.6). This is the check doing its job; it is not masked.

### Semantic Guard Reviewer
Not spawned — no check surface (see Loop State).

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

Examples:

```
- `docs/TODO.md` §Observability Debt — orphan recording-site audit follow-up
- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract record_token_refresh_metrics
```

or:

```
- (none surfaced in this devloop)
```

**This devloop:**

- `docs/TODO.md` §Devloop Container Resource Hygiene & Build Isolation — per-slug image rmi at cleanup (item A)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `ad7b53d6`
2. Review all changes: `git diff ad7b53d6..HEAD`
3. Soft reset (preserves changes): `git reset --soft ad7b53d6`
4. Hard reset (clean revert): `git reset --hard ad7b53d6`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `kubectl delete -f` if manifests were applied; a bad migration in the dev cluster = recreate the cluster (no down migrations); a bad image = revert + `dev-cluster rebuild-all`
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit. The helper delegation, setup.sh tag derivation, `:render-required` base placeholders and the migration Job must move together (base placeholders without the render = ErrImageNeverPull; helper delegation without `--rebuild-all` = unknown option). The dt-guard parser consolidation and R-18 Job/CronJob are separable (revert either alone safely). Safe direction: revert all; a cluster built from the new tree must be recreated after a revert (its workloads reference `sha-` refs the old flow never loads). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```

---

## Gate 1 — Plan Confirmations (Lead)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (rev 3; S1–S5, Q2) |
| Test | confirmed (rev 3.1) |
| Observability | confirmed (rev 3; also confirms `dashboard_configmap_label` retirement) |
| Code Quality | confirmed (rev 2, holds for rev 3) |
| DRY | confirmed (rev 3) |
| Operations | confirmed (rev 3 + agreed 3.1 edits) |
| Database (conditional) | confirmed (rev 2, holds for rev 3) |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Lead rulings: skaffold retired; entrypoint.sh migration block deleted (ops), test.sh masked `migrate info` gate fixed + behavior-equivalence harness wired into Layer 3; helper status fix lands with setup.sh Succeeded-pod delete as a commented INTERIM (removal in ADR-0038 step 3). Plan approved 2026-09-28.

## Gate 2 Status (Lead)

| Attempt | Result | Notes |
|---------|--------|-------|
| 1 | FAIL (L7) | L1 OK, L2 OK, L3 OK, L4 N/A, L5 OK, L6 N/A. L7 `PRECONDITION_FAILURE REASON=cluster-rebuild-failed` ← `REASON=migration-failed`: db-migrate `sqlx` binary needs `GLIBC_2.39`, absent in the runtime base. **Diff-caused** → implementer lane; consumes Layer-7 attempt 1/2. The fail-loud path worked as designed (all-attempt logs, describe, events, nothing after it ran). |
| 2 | FAIL (L7, operator lane) | L1-6 OK/N/A. L7 `PRECONDITION_FAILURE REASON=cluster-setup-failed` in 10s: attempt 1's half-built cluster was reused and the pre-existing `install_calico` `kubectl create` hit AlreadyExists. **Not diff-caused**, so it does not consume a Layer-7 attempt (retry once). Routed an idempotency fix for the reuse path (fresh path unchanged); Lead tears down the stale cluster before the retry. |
| 3 (L7 attempt 2/2) | FAIL (L7 env-tests) | L1-6 OK/N/A. Fresh cluster: all images built on bookworm, **migration Job applied every migration** (glibc fix proven), bring-up OK. Env-test binary 01 FAILED 5/7: `configmaps "mh-service-config" not found`. The ConfigMap is hash-suffixed since ADR-0038 step 1; env-tests 01/28/29 look it up by the bare name. **Pre-existing on HEAD** (story-2 absorb `2abbd05d` didn't adapt the env-tests), not diff-caused, but a test FAIL, so the L7 budget is exhausted. Later env-test binaries and browser E2E did not run (cargo stops at the first failing binary). Escalated to user. |

### User rulings after Gate 2 attempt 3 (2026-09-28)
- Calico reuse-path patch reverted: ADR-0038 §1 says a half-built cluster must never be reused, and step 3's provision closes that. The user confirmed step 3 lands before any real usage.
- Env-tests adapted to content-addressed ConfigMaps IN SCOPE (the story-2 absorb missed crates/env-tests): names resolved from the pod spec; the staleness test deleted; one recovery-command constant.
- All test runners run to completion: `--no-fail-fast` on both cargo test invocations (L4 rust, L7 env-tests); TS/playwright already run-all, pinned by assertion.
- One extra Layer-7 attempt granted by the user.

| Attempt | Result | Notes |
|---------|--------|-------|
| 4 (aborted) | not counted | Aborted by the Lead in Layer 3: setup.sh was edited mid-run (operations' early findings). Layer 7 had not started. |
| 5 (user-granted extra L7 attempt) | **PASS** | L1 OK, L2 OK, L3 OK, L4 N/A, L5 OK, L6 N/A, L7 OK (1674s): `env-tests-passed` (run with `--no-fail-fast`), `browser-e2e-passed`. Tree unchanged during the run (hash check). TOTAL_RESULT=N/A (the aggregate of the N/A layers). |

## Gate 2 Result: PASS (Lead)

## Gate 3 Verdicts (Lead, in progress)

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | RESOLVED-FIXED | F1: no-password test reads the needle from secret.yaml (it was a hard-coded literal). R6 residual (live wrong-password run) accepted: redaction covers it. |
| Test | RESOLVED-FIXED | 7 findings fixed (non-vacuous harness 22/0, strict generation matcher, pure resolver + panic-path tests, all-instances resolve, unused import, zombie-aware cancel, owned tempdir). Residuals R5–R7 + the three fresh-pod env-tests tracked in ADR-0038 step 3. |
| Observability | RESOLVED-FIXED | O4: no-op migration path says "unchanged". Re-confirms the `dashboard_configmap_label` retirement (R-20 + R-21). |
| Code Quality | RESOLVED-FIXED | 6 findings fixed (doc placement, is_healthy(), log_error/log_warn to stderr, derived svc→repo, owned tempdir, single CM resolve in 28). ADR-0038/0030/0034/0002 compliant. |
| DRY | RESOLVED-FIXED | 5 findings fixed (REDEPLOY_HINT in metrics.rs, one setup_sh_command, pinned hint, first_party_repos_of, ADR step-3 rename sites). No TODO entries. |
| Operations | RESOLVED-DEFERRED | F1–F3 fixed; deferral: per-slug image rmi at cleanup (docs/TODO.md item A). |
| Database | RESOLVED-FIXED | F1: sqlx-cli ↔ Cargo.lock drift is a HARD check via the shared reader (user ruling). Re-checked after the harness changes: 22 passed, non-vacuous. |

**Pending host action (user ruling):** this container has sqlx-cli 0.9.0 vs Cargo.lock 0.8.6, so the drift check reds L4. Before the final layer-all.sh, run `infra/devloop/devloop.sh --rebuild` on the host (pins sqlx-cli 0.8.6), then resume: `/devloop "finish Gate 3" --continue=2026-09-28-adr0038-migration-job-content-tagged-images`. On resume: run layer-all.sh (all post-Gate-2 fixes invalidate the prior verdict), collect any outstanding verdicts, commit.
