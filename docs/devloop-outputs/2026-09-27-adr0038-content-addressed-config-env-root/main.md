# Devloop Output: ADR-0038 devloop 1 — content-addressed configuration + the Kind environment root

**Date**: 2026-09-27
**Task**: ADR-0038 §Implementation item 1: every consumed ConfigMap hash-suffixed or checksum-annotated (service configs, OTel collector, Grafana datasources); `infra/kubernetes/overlays/kind/` becomes the applied root
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/adr0038`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f1e9b7c959b108e434512cb9115801268eef219d` |
| Branch | `feature/adr0038` |
| Lead Model | `claude-opus-5-5` (all teammates spawned with model override `opus` = Opus 5.5 per user request) |

---

## Gate 3 Result: APPROVED (Lead)

All six verdicts: Security CLEAR, Observability CLEAR, DRY CLEAR; Test RESOLVED-FIXED (R-21 fail-token unit tests), Code Quality RESOLVED-FIXED (doc back-pointer), Operations RESOLVED-FIXED (stale runbook path; NetworkPolicy-generation TODO). **Zero deferred, zero escalated.** Semantic Guard not spawned (no check surface: kustomize/YAML/shell, no credential-bearing types).

Post-fix re-validation (review fixes are guard-Rust + tests + docs; no deploy-affecting file changed, so the run5 Layer-7 pass stands): L1-6 re-run all green (L4/L6 N/A aggregate; cargo-test incl. 6 new fail_token tests OK, clippy OK, guards OK). 

**Accepted Deferrals: none** — no finding was left in the diff. (The `docs/TODO.md` NetworkPolicy-generation entry documents a pre-existing, orthogonal kubectl quirk for a future GitOps no-drift gate; it is not a deferral of this devloop's work.)

---

## Gate 3 Verdicts (Lead)

| Reviewer | Verdict |
|----------|---------|
| Security | **CLEAR** — 0 findings; all 7 conditions verified; posture improved |
| Test | **RESOLVED-FIXED** — 1 finding (R-21 fail-token tests), fixed in-loop |
| Observability | **CLEAR** — 0 findings; hashing, dashboard volume, R-21, Promtail guard, paths, single wait, docs all verified |
| Code Quality | **RESOLVED-FIXED** — 1 finding (doc back-pointer), fixed in-loop |
| DRY | **CLEAR** — 0 findings; P1-P3+N1 landed, no new duplication |
| Operations | **RESOLVED-FIXED** — 2 findings (stale runbook path; netpol-generation TODO), fixed in-diff |

---

## Gate 2 Result: PASS (Lead)

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (run5, cold build after host podman-cache prune): L1-3,5 OK; L4/L6 N/A (self-justifying — cargo-test + nx-test OK, buf-breaking OK); **L7 OK** (1738s: cold 4-service build + env-tests-passed + browser-e2e-passed). Zero FAIL/PRECONDITION. Cluster healthy.

Live-cluster evidence (`gate2-evidence.sh`, gateway 10.255.255.254):
1. `kubectl diff`: DIFF_RC=1 — **only** `metadata.generation 1->2` on gc/mc/mh-service NetworkPolicies. Pre-existing kubectl `from: []` (allow-all ingress) server-normalization quirk: spec byte-identical, generation re-bumps on every apply (verified 5->6), rolls **no** pod. NOT drift from this change (NetworkPolicies untouched by the diff); orthogonal to content-addressing. Flagged for @operations awareness.
2. Re-apply: **UIDS_UNCHANGED=yes (17 pods)** — no unconditional restart. ✓
3. One-key change in `mc-0-config.env`: **only mc-0 rolled**; revert re-converged clean (REVERTED_CLEAN=yes). ✓ (the core content-addressing invariant)
4. Grafana Ready, **all 15 dashboard files mounted** via projected volume (DASHBOARDS_MOUNTED=all). ✓
5. Retired `grafana-sidecar` Role/RoleBinding **gone** (NotFound); Grafana pod has **no SA token** (verified manually; script echo miss only). ✓
6. Deployed `mc-0-config-gfd5755hdd` (hash-suffixed) carries advertise addr `https://10.255.255.254:22212` merged via wrapper, not patched. ✓

Infra notes (not code): host podman build-cache corruption from the OOM background-shell reaper required a host `podman builder prune -af && podman image prune -af` + full teardown before a clean cold build succeeded; no Gate-2 attempt consumed (operator lane). Evidence-script `bash -c 'set --' "$1"` render bug fixed in the Lead copy (workdir passed via env var).

---

## Gate 2 Status (Lead)

- Layers 1-6: **OK** (run3). L4/L6 N/A are documented self-justifying statuses (cargo-test + nx-test both OK).
- Layer 7: **PRECONDITION_FAILURE** — host podman build-cache corruption (operator lane; no Gate-2 attempt consumed).
  - Trigger: the OOM background-shell reaper killed gate2-run2 mid image-build; podman's layer store was left inconsistent.
  - run3 failed at gc-service build STEP 2/5 with `getting top layer info: layer not known`.
  - Operator-lane retry (recreate refused — control plane alive; then forced teardown+setup) cleared that error but surfaced a STALE phantom layer: `RUN test -n "$CARGO_BUILD_JOBS"` resolved via `--> Using cache`.
  - Verified NOT diff-caused: `CARGO_BUILD_JOBS` / `scripts/lang/_cargo_jobs.sh` exist nowhere in the tree or git history; `build_image()` is byte-identical to HEAD; no Dockerfile is in the diff.
  - Fix is HOST-side (podman not reachable from the devloop container; builds run host-side via the ADR-0030 helper, whose verbs have no cache-prune): `podman builder prune -af && podman image prune -af` on the host, then re-run Gate 2.
- Live-cluster evidence script staged at scratchpad `gate2-evidence.sh` (moved out of the output dir — it tripped `validate-cross-boundary-scope` scope_drift as an untracked file; that was Gate-2 attempt 1).

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (infrastructure, opus) |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | not spawned (no check surface: YAML/kustomize/shell manifests, no credential-bearing types) |

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

## Task Overview

### Objective
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

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
| `infra/services/ac-service/**` (ConfigMap YAML -> `config.env` generator source) | Mine | — |
| `infra/services/gc-service/**` (ConfigMap YAML -> `config.env` generator source) | Mine | — |
| `infra/services/mc-service/**` (ConfigMap YAML -> `*.env` generator sources) | Mine | — |
| `infra/services/mh-service/**` (ConfigMap YAML -> `*.env` generator sources) | Mine | — |
| `infra/services/redis/**` (ConfigMap YAML -> `redis.conf` generator source) | Mine | — |
| `infra/services/otel-collector/**` (collector config -> `collector.yaml` generator file) | Not mine, Minor-judgment | observability |
| `crates/env-tests/src/fixtures/metrics.rs` (doc pointer) | Not mine, Mechanical | test |
| `infra/kubernetes/observability/**` (loki/promtail config split into generator files) | Not mine, Minor-judgment | observability |
| `infra/kubernetes/overlays/kind/**` | Mine | — |
| `infra/grafana/kustomization.yaml` | Not mine, Minor-judgment | observability |
| `infra/grafana/deployment.yaml` (dashboards via one projected volume; LIST initContainer removed; `automountServiceAccountToken: false`) | Not mine, Minor-judgment | observability |
| `infra/grafana/rbac.yaml` (grafana-sidecar Role/RoleBinding removed) | Not mine, Minor-judgment | observability |
| `docs/LOCAL_DEVELOPMENT.md` (Grafana dashboards troubleshooting) | Not mine, Minor-judgment | observability |
| `infra/kind/scripts/setup.sh` | Mine | — |
| `scripts/setup.test.sh` | Mine | — |
| `crates/dt-guard/src/common/pod_spec.rs` (new) | Mine | — |
| `crates/dt-guard/src/common/kustomize_generators.rs` (new) | Mine | — |
| `crates/dt-guard/src/common/mod.rs` | Mine | — |
| `crates/dt-guard/src/kustomize_content_addressing.rs` (new, R-21) | Mine | — |
| `crates/dt-guard/src/lib.rs` | Mine | — |
| `infra/skaffold.yaml` | Mine | — |
| `crates/dt-guard/src/env_config.rs` (resolve generator sources) | Mine | — |
| `crates/dt-guard/src/kustomize.rs` (R-16 generator files; R-21 wiring; root build target) | Mine | — |
| `.github/workflows/ci.yml` (assert kubectl present for R-21) | Mine | — |
| `crates/dt-guard/src/grafana_datasources.rs` (read promtail config file, missing file = error) | Not mine, Domain-judgment (paired) | observability |
| `scripts/dev-web.sh` | Mine | — |
| `scripts/dev-web.test.sh` | Mine | — |
| `crates/mh-service/src/config.rs` (path strings in messages/comments/test assertion; new `every_infra_path_this_file_cites_resolves` test — the fired TODO trigger) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/tests/common/accept_loop_rig.rs` (comment path) | Not mine, Mechanical | media-handler |
| `crates/mc-service/src/config.rs` (comment paths) | Not mine, Mechanical | meeting-controller |
| `crates/mc-service/tests/common/mod.rs` (comment paths) | Not mine, Mechanical | meeting-controller |
| `crates/env-tests/tests/*.rs` (comment paths) | Not mine, Mechanical | test |
| `packages/web-app/**` (comment paths) | Not mine, Mechanical | client |
| `infra/grafana/dashboards/client-media.json` (description path) | Not mine, Mechanical | observability |
| `docs/observability/**` (paths) | Not mine, Mechanical | observability |
| `docs/specialist-knowledge/*/INDEX.md` (paths) | Not mine, Mechanical | each INDEX owner |
| `docs/TODO.md` (paths) | Not mine, Mechanical | operations |
| `docs/runbooks/*.md` (paths: Mechanical; literal `kubectl get/edit configmap <name>` commands rewritten for hashed names: judgment) | Not mine, Minor-judgment | operations |

---

## Planning

### Mechanism restated

"Every pod-consumed configuration object's identity is derived from its rendered content, and the
environment is applied as one declarative root." The ConfigMap framing is the instance; the wider
class also contains the in-tree **Secrets** pods consume (`gc/mc/mh/postgres/redis` `secret.yaml`,
`infra/grafana/secret.yaml`). Recommendation: leave Secrets out of this devloop's invariant because
ADR-0038 §1 makes secret material provision-layer (a recipe change is a blueprint change → cluster
rebuild, devloop 3); flagged for Lead decision rather than silently narrowed.

### Findings that shape the design (verified with `kubectl kustomize`, kustomize v5.5.0)

1. Kustomize hashes ONLY generator-originated ConfigMaps. A plain `ConfigMap` resource cannot be
   hashed (tested: `kustomize.config.k8s.io/needs-hash` / `internal.config.kubernetes.io/needs-hash`
   annotations ignored; `configMapGenerator behavior: merge|replace` over a plain resource stays
   unhashed). So every consumed ConfigMap must become a `configMapGenerator`.
2. The hash is computed on the FINAL render: overlay strategic-merge patches (OTel/CORS) and a
   wrapper-level `behavior: merge` generator both change the suffix and the references rewrite. So the
   Kind overlay patches keep working unchanged.
3. `generatorOptions.disableNameSuffixHash: true` cannot be overridden per-generator (tested).
4. Absolute paths are rejected in `resources:`; a relative path out of a temp dir works.

### Changes

**A. Service ConfigMaps → generators (ac, gc, mc, mh; shared + per-instance).** Each
`configmap.yaml` becomes an env-file generator source (`config.env`, `mc-0-config.env`, …), comments
preserved, values unquoted (all current values are simple scalars; verified no `#`/newline hazards
beyond URLs, which kustomize keeps verbatim). Labels move to `options.labels`; `namespace:` set on
each generator (load-bearing, same reason as the existing `prometheus-config` note). Workload
manifests unchanged — kustomize rewrites every `configMapKeyRef`/`envFrom`/volume name.

**B. File-shaped ConfigMaps → `files:` generators.** `otel-collector-config` (`collector.yaml`),
`redis-config` (`redis.conf`), `loki-config` + `promtail-config` (data split out of
`loki-config.yaml`/`promtail-config.yaml` into `loki.yaml`/`promtail.yaml`, generated in
`infra/kubernetes/observability/kustomization.yaml` with `namespace:`).

**C. Grafana.** Move `grafana-datasources` + `grafana-dashboards-config` (pod-mounted) into a new
`infra/grafana/provisioning/kustomization.yaml` sub-base (hashed; the parent's Deployment references
rewrite — verified). Dashboard ConfigMaps keep `disableNameSuffixHash: true` in the parent: they are
not pod-template consumed — the k8s-sidecar watches them by label and hot-reloads, and stale hashed
generations would be loaded as duplicates. This is the "stable name required" case; no checksum
annotation is needed because nothing in a pod template consumes them.

**D. setup.sh — the root is what bring-up applies.**
- `apply_env_root`: `kubectl apply -k infra/kubernetes/overlays/kind/`. When `DT_HOST_GATEWAY_IP`
  is set, the per-cluster advertise addresses are NOT `kubectl patch`ed onto live ConfigMaps any more
  (that addresses a literal name, and imperative drift would be reverted by the next apply): a
  temp wrapper kustomization references the root (relative path) and merges
  `M{C,H}_WEBTRANSPORT_ADVERTISE_ADDRESS` into `m{c,h}-{0,1}-config` via `behavior: merge`
  literals, so the per-cluster values are content-addressed too and a gateway/port change rolls
  exactly those four pods. Rendering is a separate pure function so `setup.test.sh` can render it.
- `main()`: cluster → Calico → namespaces → third-party preload → build+load the 4 service images →
  pre-apply postgres + redis + otel-collector sub-overlays with their readiness gates (migrations need
  postgres; R-54 fail-hard OTel init needs the collector) → migrations + seeds (unchanged; devloop 2)
  → imperative secrets/TLS (unchanged; devloop 3) → `apply_env_root` → one wait phase (observability
  readiness + ac/gc/mc/mh rollouts). `deploy_observability` and per-service `deploy_*_service` apply
  calls go away; the per-service `rollout restart` after the advertise patch goes away (the hash does it).
- `--only <svc>`: build that image (unless `--skip-build`), `apply_env_root`, `rollout restart` that
  service's workloads ONLY if an image was built (tags are still `:latest` until devloop 2), wait.
  `--only otel` = apply root + wait. The helper's `deploy` (`--skip-build --only`) therefore
  converges the whole tree, which is the ADR direction.
- Pre-applied sub-overlays must be byte-identical subsets of the root render or the root apply would
  flip them: new `setup.test.sh` case renders each pre-applied sub-overlay and the root and asserts
  every sub-overlay document appears verbatim in the root; plus a case rendering the gateway wrapper
  asserting the four advertise values land and the hashed names differ from the plain root.

**E. Guards (fail loudly on drift of the invariant).**
- `dt-guard kustomize` new **R-21 content-addressed ConfigMaps**: on every rendered build target,
  every ConfigMap referenced from a pod template (volumes incl. projected, `envFrom.configMapRef`,
  `env.valueFrom.configMapKeyRef`) must carry a kustomize hash suffix (`-[245-9bcdfghkmt]{10}`,
  kustomize's encoding alphabet) and exist in the render. This is what keeps the invariant complete
  when someone adds a plain ConfigMap later.
- `dt-guard kustomize` R-16: files declared by generators (`envs:`/`files:`) count as referenced.
- `dt-guard env-config`: resolve `configMapGenerator` entries in each service's kustomization.yaml
  (name + keys from `envs`/`literals`/`files`) into the same name-scoped model; findings point at the
  `.env` file. Fixtures/tests updated; same rule ids and status tokens.
- `dt-guard grafana-datasources`: read `promtail.yaml` directly instead of CM `data.promtail.yaml`.

**F. Literal-name / path fallout (complete, not partial).** `scripts/dev-web.sh` (+ test) reads the
per-instance `.env` files and prints a live-value command that resolves the hashed name through the
Deployment; runbook `kubectl get|edit configmap <literal>` commands rewritten (read via the workload's
reference; edit = change the tree and re-apply the root); every repo path reference to the old
`configmap.yaml` files updated, including line-number cites (historical records — ADRs, debates,
user-stories, devloop-outputs, process reviews — left as written). `infra/skaffold.yaml` switched from
raw `services/*/*.yaml` globs (which would deploy no ConfigMaps) to kustomize on the Kind root.

### Layer 7 stays correct
Full-rebuild gates run setup.sh end-to-end → root applied. `rebuild-all` (images + restart) is
untouched. The helper `deploy` verb now converges the whole tree instead of one service.

### Scope boundary — Secrets (Lead decision)
Secrets are NOT content-addressed in this devloop (ADR-0038 §2 scopes content-addressing to
ConfigMaps; §1 moves secret material to `provision`, devloop 3). Consequence: until devloop 3, a
change to an in-tree Secret does not roll its consumers. No committed Secret moves into a ConfigMap or
generator; the Grafana admin Secret stays a Secret. This is an ADR scope boundary, not a deferral.

### Gate-1 responses (reviewer pre-plan concerns)
- **Inventory — every pod-consumed ConfigMap and its mechanism.** Generator hash suffix:
  `ac/gc/mc/mh-service-config`, `mc-0/mc-1/mh-0/mh-1-config`, `redis-config`,
  `otel-collector-config`, `loki-config`, `promtail-config`, `grafana-datasources`,
  `grafana-dashboards-config`; already hashed and untouched: `prometheus-config`, `prometheus-rules`.
  Kind overlay patches (OTel ac/gc/mc, GC CORS) stay strategic-merge patches in the overlay and
  change the final hash (verified). A patch that stops matching fails the build, it doesn't create a
  second ConfigMap. Excluded, with reason: `grafana-dashboards-*`. They are sidecar-watched by label
  and hot-reloaded, no pod template consumes them, and hashed generations would duplicate dashboards
  and uids. They keep stable names. Postgres consumes no ConfigMap. No checksum annotations
  anywhere, so no hand-maintained hash exists.
- **Dashboards split (observability #1).** Datasources and dashboards-config move to a hashed
  sub-base, `infra/grafana/provisioning/kustomization.yaml`. The parent keeps file-wide
  `disableNameSuffixHash: true`, which now covers ONLY dashboard groups, so a new dashboard group is
  stable by construction and doesn't depend on remembering a per-entry flag. A per-entry true flag
  can't be overridden per-generator either way (verified). The split is explained in comments in
  both kustomizations.
- **Advertise-address patches (all reviewers).** They become a render input: a wrapper
  kustomization that references the root and does `behavior: merge` on the four per-instance
  generators. It is written to a mktemp dir from a pure render function, with the root path derived
  from PROJECT_ROOT and ports re-validated. No `kubectl patch`, no post-apply restart. Relation to
  devloop 3: this function is the per-slug parameter step that the `deploy` render absorbs, because
  the values come from the helper's port allocation, which the ADR already puts in the blueprint.
  I chose a temp dir over a gitignored file in the tree: a missing gitignored file would break every
  guard build of the root.
- **Readiness order (operations/test #5).** postgres, redis and otel-collector are pre-applied via
  their own Kind sub-overlays with today's readiness gates. Migrations and seeds follow, then the
  secrets. Only then is the root applied, followed by one wait phase covering the observability
  waits (from `deploy_observability`) and the ac/gc/mc/mh rollouts. setup.test.sh gets a new case:
  each pre-applied sub-overlay's documents must appear verbatim in the root render, so the root
  apply is a no-op on them.
- **Guard vacuity (test #2, code-reviewer #2).** env-config resolves `configMapGenerator` entries
  (`envs`/`literals`/`files`) into its name-scoped model:
  - A declared source that is missing or unreadable is a hard failure.
  - Positive control: a test runs `analyze()` on the real tree. It asserts every canonical service
    resolves at least one generator ConfigMap with at least one key, zero `configmap_not_found`, and
    that the orphan check is live. It removes a referenced key in a fixture and requires the finding.
  - I'm keeping the declaration model rather than switching to rendered output. Overlay coverage
    stays the documented, TODO-tracked gap. Changing the guard's resolution model is its own task.
- **Invariant proof (test #4).** New `dt-guard kustomize` R-21 runs on every rendered build target
  and fails if either condition holds:
  - (a) a ConfigMap referenced from a pod template (volumes, projected, envFrom, configMapKeyRef) is
    not hash-suffixed (`-[245-9bcdfghkmt]{10}`);
  - (b) such a reference doesn't resolve to a ConfigMap in the same namespace in the render. This
    catches the silent cross-namespace name-reference skip (security/code-reviewer).
  
  Positive control, derived from the render: every non-dashboard ConfigMap in the render must be
  referenced by some pod template. That fails if the reference walker goes blind, with no hardcoded
  N. (c) "A data change changes the consumer's template": a setup.test.sh case copies `infra/` to a
  temp dir, changes one key in `mc-0-config.env`, re-renders the root, and asserts only the mc-0
  Deployment template differs.
- **grafana_datasources fail-open (observability #3).** The Loki-label check reads `promtail.yaml`
  directly. A missing or unparseable file becomes an error, not `None`. I don't touch the
  prometheus kustomization, so infrastructure_metrics.rs and alert_rules.rs are unaffected.
  `alert_rules_loaded.rs` reads only `prometheus-rules`, which is unchanged. I'm updating the
  metrics.rs:202 pointer.
- **Security.**
  - No `--prune` anywhere.
  - Every `network-policy.yaml` is untouched.
  - Kind-only patches (CORS, NodePorts) stay in the overlay.
  - Every `apply -k` uses a PROJECT_ROOT-derived path.
- **Runbooks (operations).** In ac-service-deployment, ac-service-incident-response, gc-deployment,
  gc-incident-response, mc-deployment, mc-incident-response, mh-incident-response and
  client-dev-local, each `get` resolves the live name through the workload's reference (a
  jsonpath on the Deployment/StatefulSet). Each `edit` becomes "change the tree file, re-apply the
  root".
- **Old generations / rollback (operations).**
  - Nothing prunes old generations in this devloop (no `--prune`, which would risk the imperative
    Secrets). They are inert, and the runbook gets a manual label-scoped cleanup command.
  - Rollback: revert the tree and re-apply the root. Pods go back to the prior hash name, which is
    re-created if it was cleaned up. Rollback never depends on the old object surviving.
- **Stateful restarts (operations).** Acknowledged: a change to `redis.conf`, loki, promtail,
  datasources or the collector config now rolls Redis (dropping dev MC session state), Loki,
  Promtail, Grafana or the collector respectively. That is the intended semantics. The first apply
  after this lands rolls every consumer once.
- **Layer 7 evidence (test #6).** Gate 2 cluster runs are the Lead's. For the re-apply evidence I'll
  hand the Lead a snippet: capture pod UIDs, apply the root, compare, then change one key, apply,
  and compare. I have no cluster access in this container to do it myself.

### Gate-1 round 2 (commitments)

**Conversion fidelity (code-reviewer 1, security j — hard gate).** Empty values are written as
`KEY=` (for example `CORS_ALLOWED_ORIGINS=`), with no quotes anywhere. Two mechanisms:
- A one-shot before/after diff at Gate 3. Every build target (bases, per-service overlays,
  observability, Kind root) is rendered at `f1e9b7c` and at HEAD, hash suffixes are stripped, and
  each ConfigMap's `data` is compared keyed by the unhashed name. It must be byte-identical. The
  diff output is recorded in main.md.
- A permanent check: the shared env-file reader rejects a value whose first and last characters are
  both `"` or both `'`, and rejects a line that is not `KEY=VALUE`, a `#` comment, or blank. Either
  is a hard error.

**One structured generator parser (Lead reconcile; code-reviewer 3, DRY P2).** One serde-based
`configMapGenerator` parser, `crates/dt-guard/src/common/kustomize_generators.rs`, returns entries
`{name, namespace, envs, files (key=path aware), literals}`, plus an env-file key reader (keys only,
the quote rejection above, a malformed line is an error). R-16, R-21 and env-config consume it.
`extract_declared_generator_files` stays for R-20/alert-rules, whose `/`-bullet property it
documents. No second line scanner.

**Pod ConfigMap-reference enumeration (DRY P1).** `pod_spec`/`containers`, plus one iterator
yielding `(ref kind, name, key?)` over `configMapKeyRef`, `envFrom.configMapRef`,
`volumes[].configMap` and projected sources, move to `crates/dt-guard/src/common/pod_spec.rs`.
env-config and R-21 each keep their own policy (env-config still refuses `envFrom` coverage).

**R-21 (test 1–3, code-reviewer 2, security e, DRY P3, observability note).** R-21 and its messages
name ConfigMaps only.
- Build targets now include the Kind root `overlays/kind/` itself. The gateway wrapper is covered
  by the setup.test.sh wrapper case: it renders the real wrapper, asserts every pod-referenced
  ConfigMap there resolves in the render with the suffix, and asserts its Secrets are identical to
  the plain root.
- It FAILS, never WARNs, when no kustomize tool is present or a render fails. Today
  `detect_kustomize_tool` returns `None` and every build-dependent check degrades to WARN; R-21
  alone gets a FAIL token, `kustomize-content-addressing-unverifiable`. Layer 3 always has kubectl:
  the devloop image installs v1.32.3 (`infra/devloop/Dockerfile`) and the GitHub ubuntu runners
  ship it.
- Vacuity is a distinct token: zero build targets, or zero pod-template ConfigMap references on a
  target that has workloads.
- The Kind root's consumed-ConfigMap count is carried on the OK status line.
- Positive control, derived from the render: every ConfigMap without the
  `grafana_dashboard: "1"` label (the label the sidecar keys on, not a name prefix) must be
  referenced by some pod template.
- The suffix regex is exactly one `Lazy<Regex>` in `kustomize.rs`, citing kustomize's
  `hasher.encode` alphabet. It is never restated in shell.
- Fail-open residual (a hand-named ConfigMap whose name happens to end in 10 alphabet characters) is
  closed: the name minus the suffix must also be a generator-declared name in the tree (via the
  shared parser).
- Unit tests: a plain ConfigMap referenced via `configMapKeyRef`, `envFrom`, a volume, and a
  projected source each FAIL; a hashed-looking ref absent from the render FAILs; one valid case
  passes.

**env-config positive controls (test 4).** Fixtures covering:
- an orphan key in an `.env` source → `orphan_configmap_key`;
- a missing key → `key_not_in_configmap`;
- keys supplied via `literals:` and via `files:`;
- an unreadable source → hard error;
- a real-tree test.

The Gate-3 summary records the real-tree status line before and after.

**setup.test.sh (test 5–7, code-reviewer 4–5, security 2c, DRY N1).** Runs at Layer 3 inside the
devloop container (`scripts/layer3.sh`, `setup-disk-guard-selftest`). kubectl-dependent cases FAIL
loudly if `kubectl` is absent; they do not skip. Cases:
- Subset identity. Each pre-applied sub-overlay (postgres StatefulSet, redis StatefulSet,
  otel-collector Deployment) renders non-empty with its expected kind, and every one of its
  documents appears verbatim in the root render.
- Wrapper. The 4 advertise values land. The 4 `m{c,h}-{0,1}-config` names differ from the plain
  root. `ac-service-config` and all other ConfigMap names are unchanged. No Secret differs from the
  plain root.
- Instance set is derived, not listed. The wrapper's instances come from globbing the per-instance
  generator sources `infra/services/m[ch]-service/m[ch]-*-config.env`. A test asserts that set
  equals the per-instance generators in the root render. A missing `<SVC>_<N>_WEBTRANSPORT_PORT`
  fails loudly.
- One apply path. `apply_env_root` is the only function that applies the root, and the wrapper
  choice lives inside it. PATH-stubbed kubectl cases:
  - gateway set → the wrapper dir is applied
  - gateway unset → the root is applied
  - wrapper render failure → abort non-zero, no apply at all, no plain-root fallback
- `--only`. The root is applied. `rollout restart` happens only when an image was built:
  `--skip-build` means no restart. A grep assertion checks no `kubectl patch configmap` or
  `--prune` remains in setup.sh.
- Data change. A temp copy of `infra/` with one key changed in `mc-0-config.env`: only the mc-0
  template differs.

**Wrapper hygiene (security 2a/2b, ops 3, code-reviewer 5).** Before any rendering:
- The existing IPv4 and non-0.0.0.0 validation runs on `DT_HOST_GATEWAY_IP`.
- Each port must match `^[0-9]+$` and fall in 1..65535.
- Literals are quoted.

The wrapper is created with `mktemp -d` and removed by a `trap` on EXIT and ERR (and on function return). No
`--load-restrictor` change. A render failure aborts with no fallback.
`overlays/kind/kustomization.yaml` gains a header saying devloop clusters must apply through
`setup.sh:apply_env_root`, because a plain apply reverts the advertise addresses.
Skaffold (dev-only) is pointed at the plain root, and its header says so. It has no gateway
parameters, so it serves the host path whose defaults are the plain values.

**Waits (ops 4–5, observability).**
- The single wait phase covers every workload the root owns: ac/gc/mc/mh, redis, postgres,
  otel-collector, and exactly the six from today's `deploy_observability` (prometheus, loki,
  promtail, grafana, kube-state-metrics, node-exporter). It is used by full setup AND by `--only`.
- On a timeout it names the workload and dumps `kubectl get pods -o wide`, plus `describe` and
  recent events for that workload's pods. That surfaces CreateContainerConfigError, FailedMount and
  CrashLoopBackOff.

**grafana_datasources (observability 1–2).** Fails closed, with its own reason token, when
`promtail.yaml` is missing or unparseable or yields zero labels. The positive control reads the
real file and asserts `app` and `namespace` are present; both are verified in-tree as
`target_label`s. A missing-file test. The path sweep explicitly covers:
- `crates/env-tests/src/fixtures/metrics.rs:202`
- `crates/env-tests/tests/31_gc_telemetry.rs:12`
- `packages/web-app/e2e/mcMetrics.ts:45`
- the `grafana_datasources.rs` finding text

**Runbooks (ops 1–2, 6).** I grep `configmap` across `docs/runbooks/` and rewrite every hit, not a
hand-picked set. Changes:
- The prod incident runbooks gain a fast-rollback line: `kubectl rollout undo deployment|statefulset/<x>`.
  It works because the prior ReplicaSet/ControllerRevision references the prior hashed ConfigMap,
  which still exists.
- No "delete old generations" command is published; this supersedes the label-scoped cleanup
  command offered in Gate-1 responses. A cleanup that deletes every non-current generation breaks
  `rollout undo` (CreateContainerConfigError/FailedMount). The runbook states it explicitly: any
  pruning must spare ConfigMaps referenced by retained ReplicaSets/ControllerRevisions.
- mc-deployment.md gains a line that a `redis-config` change is now a stateful Redis restart.

**Security.**
- No Secret value lands in any generator source or the wrapper.
- `mh-service-secrets` is removed from setup.sh (Lead decision). The in-tree
  `infra/services/mh-service/secret.yaml` is the single source of truth. Nothing needs it before
  the root apply: MH pods are first created by that apply.
- A Gate-3 resource-list diff (kind/namespace/name) compares the old per-sub-overlay applies with
  the root render. Only ConfigMap renames may differ, and every NetworkPolicy must be present.
- CORS and NodePort patches stay in the overlay.

**setup.sh hygiene (code-reviewer 6).**
- `deploy_{ac,gc,mc,mh}_service`, `deploy_observability` and `create_mh_secrets` are deleted, not
  stubbed.
- The legacy `delete configmap grafana-dashboards` line is dropped. It cleaned up a pre-generator
  monolithic ConfigMap from clusters that predate the Kind-per-devloop model. Nothing references
  that name, and a stray copy would be inert.

**Gate-2 evidence snippet for the Lead (test 7).** On the Layer-7 cluster, record in main.md:
1. `kubectl diff -k <wrapper root>` → exit 0.
2. Capture pod UIDs for all workloads, re-apply the root, and show the UIDs unchanged.
3. Change one key in `mc-0-config.env`, re-apply, and show only mc-0 rolls. Revert.

env-tests 26, 30 and 33 are must-pass.

### Deviation 1 (approved by @team-lead, agreed by @observability): Grafana dashboards are content-addressed too

**Premise corrected.** The plan kept dashboard ConfigMaps stable-named on the belief that the
k8s-sidecar watches and hot-reloads them. It does not. `infra/grafana/deployment.yaml` ran it as a
`METHOD: LIST` **initContainer**, which lists once at pod start and exits. The manual
`rollout restart deployment/grafana` recorded in `docs/observability/dashboards.md` and `docs/TODO.md`
was the consequence. Dashboards are therefore pod-consumed configuration that never rolls. Leaving
them stable-named would ship the invariant partial.

**Change.**
- Every dashboard group is a hashed generator. The provisioning sub-base is folded back into
  `infra/grafana/kustomization.yaml`, with no `disableNameSuffixHash` anywhere.
- The groups are mounted into `/var/lib/grafana/dashboards` by ONE `projected` volume.
  - Sources are not `optional:`, so a missing group fails pod start.
  - Filenames are unique and the provider path is unchanged.
- Removed: the LIST initContainer, the `grafana-sidecar` Role/RoleBinding and the
  `grafana_dashboard` labels.
- Security additions:
  - `automountServiceAccountToken: false` on the Grafana pod.
  - `setup.sh:delete_retired_resources()`, called from `apply_env_root`. It is a one-time
    `--ignore-not-found` delete of the retired Role/RoleBinding: converging a removed resource
    without `--prune`, and removable once devloop 3 rebuilds every cluster. The leftover RoleBinding
    would otherwise keep granting configmaps get/list/watch to the still-present SA.
- R-21's dashboard label exemption is deleted. A fixture where a generated dashboard group is
  missing from the projected volume must FAIL (`configmap_unreferenced`). This replaces the
  formerly unguarded "missing labels block" step.
- Docs:
  - `docs/observability/dashboards.md` §Kubernetes is rewritten.
  - `docs/LOCAL_DEVELOPMENT.md` no longer says "restart Grafana".
  - The `docs/TODO.md` "R-20 checks … never that its group is LABELLED" entry is deleted, because
    the label and the restart it described no longer exist.

**Size check.** Raw group sizes: ac 107 KB, gc 149 KB, mc 170 KB, mh 135 KB, client 38 KB,
errors 14 KB. Every group is under both the 1 MiB ConfigMap cap and the 256 KiB
`last-applied-configuration` annotation cap of the client-side apply that `setup.sh` has always
used. The apply mode is unchanged (no `--server-side`, no `replace`). The projected total is
~612 KB, and projected volumes have no aggregate cap beyond the node's tmpfs.

Old stable-named `grafana-dashboards-*` ConfigMaps linger on existing clusters until a rebuild.
They are inert: nothing selects by label any more, and nothing references the names.

### Risks
- Old hashed generations accumulate until pruned (ADR-accepted hygiene cost).
- A config change to a stateful consumer now restarts it: `redis.conf` rolls Redis (dev MC session state lost; in prod a stateful restart), loki/promtail roll Loki/Promtail.
- Anyone hand-patching a live ConfigMap now sees no effect / is reverted — intended; runbooks updated.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (round 2) |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

Note: INDEX.md / review-protocol.md were passed to teammates as a first-action Read instruction rather than inlined (80KB+ of INDEX content).

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### Content-addressed ConfigMaps (ADR-0038 §2)
| ConfigMap | Before | After |
|-----------|--------|-------|
| `ac/gc/mc/mh-service-config`, `mc-0/mc-1/mh-0/mh-1-config` | plain `configmap.yaml` resources, stable names | `configMapGenerator` over `infra/services/<svc>/{config,mX-N-config}.env`, hash-suffixed |
| `otel-collector-config` | inline `data.config.yaml` | generator over `infra/services/otel-collector/collector.yaml` |
| `redis-config` | inline `data.redis.conf` | generator over `infra/services/redis/redis.conf` |
| `loki-config`, `promtail-config` | inline in `loki-config.yaml` / `promtail-config.yaml` | generators over `observability/{loki,promtail}.yaml` |
| `grafana-datasources`, `grafana-dashboards-config`, `grafana-dashboards-{ac,gc,mc,mh,client,errors}` | generated, `disableNameSuffixHash: true`; dashboards read once at pod start by a LIST initContainer | hashed; dashboards mounted through one projected volume (Deviation 1) |
| `prometheus-config`, `prometheus-rules` | already hashed | unchanged |

**Fidelity evidence (security j, code-reviewer 1).** Every build target was rendered at `f1e9b7c` and
at HEAD: 7 service bases, 7 Kind service overlays, the observability base and overlay, `infra/grafana`,
and the Kind root. Hash suffixes were stripped and each ConfigMap's `data` compared as rendered text,
keyed by name. **Every ConfigMap is byte-identical except `grafana-dashboards-client`, whose only
difference is the intended path-sweep edit to the `client-media.json` banner text**
(`otel-collector/configmap.yaml` → `otel-collector/collector.yaml`). `CORS_ALLOWED_ORIGINS` renders
as `""` (empty) in both.

The resource list of the root render (kind/namespace/name, hash stripped) is 86 before and 86 after,
with no additions or removals. It includes all 7 NetworkPolicies.

### The environment root is what bring-up applies
- `setup.sh:apply_env_root()` is the only place that applies `infra/kubernetes/overlays/kind/`.
  - With `DT_HOST_GATEWAY_IP` set, it applies a temporary wrapper rendered by
    `render_env_overlay()`. The wrapper uses `behavior: merge` for the four derived
    `m{c,h}-N-config` advertise literals. The temp dir is created with `mktemp -d` and removed by a
    subshell EXIT trap.
  - If the render fails, it aborts with no fallback.
- Full setup order:
  1. Build and load the four images.
  2. Pre-apply postgres, redis and the otel collector as subset sub-overlays, with their gates.
  3. Run migrations and seeds.
  4. Create the imperative Secrets/TLS.
  5. Run `apply_env_root` and `wait_for_env_root`.

  `wait_for_env_root` derives its 15 workloads from the render and dumps pods, describe output and
  events on a timeout.
- `--only <svc>` builds that one image (unless `--skip-build`), applies the root, restarts only the
  workloads that run the rebuilt image, and waits for every root workload.
- Removed:
  - `deploy_{ac,gc,mc,mh}_service`, `deploy_observability`, `create_mh_secrets` (the in-tree
    `mh-service/secret.yaml` is now the single source).
  - the legacy `delete configmap grafana-dashboards`.
  - all `kubectl patch configmap`.

### Guards
- New module `kustomize_content_addressing.rs`, rule R-21, run on every build target including the
  new Kind-root target:
  - Every pod-template ConfigMap reference must be hash-suffixed, and the name without the suffix
    must be a generator declared under `infra/`. The regex is defined once, citing kustomize's
    encoder.
  - Each reference must resolve in the workload's namespace.
  - Render-derived positive control: every non-dashboard ConfigMap must be referenced.
  - Fails closed when no tool is present (`kustomize-content-addressing-unverifiable`) or the check
    is vacuous (`…-vacuous`).
  - The OK line now carries the count: `kustomize-clean[-kubeconform-skipped]-16-consumed-configmaps`.
- One structured generator parser, `common/kustomize_generators.rs`, is shared by R-16, R-21 and
  env-config. The env-file reader rejects quote-wrapped values, leading/trailing whitespace and
  malformed lines.
- The pod-template walker and ConfigMap-reference enumerator are lifted into `common/pod_spec.rs`.
  env-config and R-21 each keep their own policy.
- `env-config` resolves generator sources. Its OK token changes:
  - before: `env-config-clean-4-services-6-workloads`
  - after: `env-config-clean-4-services-6-workloads-8-configmaps`
- `grafana-datasources` reads `promtail.yaml` directly and fails closed
  (`grafana-datasources-promtail-labels-unavailable`).
- `otel-collector` joins R-16/R-15's `SERVICE_BASES`; it was previously unchecked.

### Additional Changes
- `scripts/dev-web.sh` derives its instance set from the same glob as `advertise_instances()`, reads
  `.env` sources, and gives a live-value command that resolves the hashed name. It has a new
  CANNOT VERIFY branch for zero sources.
- Runbooks: every literal-name `kubectl get/edit configmap` was rewritten. Added `rollout undo` fast
  levers, the "pruning must spare referenced generations" warning, and the Redis stateful-restart
  note. Every "no content hash" statement was corrected.
- The `mh-service` remediation-path TODO trigger fired and is resolved by
  `every_infra_path_this_file_cites_resolves`.
- `ci.yml` asserts kubectl before `layer-all.sh`.
- `infra/skaffold.yaml` deploys the Kind root through kustomize.

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Env-tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Wall-clock time for dev-cluster rebuild + env-test run; pass/fail summary; log path}

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review Results → Semantic Guard Reviewer below for its findings.)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (appended to `docs/TODO.md`):
{One bullet per `docs/TODO.md` entry added, citing the section heading the entry was added under, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Semantic Guard Reviewer
**Verdict**: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED
**Native verdict**: SAFE / UNSAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: {count} found, {count} fixed, {count} deferred

{Per-finding `[check-name]: file/path.rs:line - description` block, or "No findings"}

{Note any findings folded with Code Reviewer at Gate 3 §Deduplication, with "(also flagged by code-reviewer)" attribution.}

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

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied

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
