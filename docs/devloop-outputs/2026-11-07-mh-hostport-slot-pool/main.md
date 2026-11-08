# Devloop Output: MH hostPort slot pool + single MH template (Story 4 I-2)

**Date**: 2026-11-07
**Task**: Story 4 task I-2 — MH hostPort slot pool (K=6), single MH Deployment template, deploy.sh wired to mh.sh; R-27 publish-leg + R-28 address-invariant env-tests
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/mh-failure-transfer`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `86647376df6772fa11f9f120cd0a7868cca155fb` |
| Branch | `feature/mh-failure-transfer` |
| Lead Model | `claude-opus-5-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `3` (resume after host-op) |
| Security | `security` |
| Test (paired) | `paired-test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Client (conditional reviewer: dev-web.sh) | `client` |
| Media Handler (conditional reviewer: MH crate test moves) | `media-handler` |
| Semantic Guard | `not spawned — diff is infra/manifests/scripts/tests; no credential/Debug/credential-lifetime surface` |

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
| `crates/devloop-helper/src/ports.rs` | Mine | — |
| `crates/devloop-helper/src/commands.rs` | Mine | — |
| `infra/kind/kind-config.yaml.tmpl` | Mine | — |
| `infra/kind/kind-config.yaml` | Mine | — |
| `infra/services/mh-service/deployment.yaml` (new; the ONE template) | Mine | — |
| `infra/services/mh-service/mh-0-deployment.yaml` (delete) | Mine | — |
| `infra/services/mh-service/mh-1-deployment.yaml` (delete) | Mine | — |
| `infra/services/mh-service/mh-0-config.env` (delete) | Mine | — |
| `infra/services/mh-service/mh-1-config.env` (delete) | Mine | — |
| `infra/services/mh-service/slot-template.env` (new; template-input placeholder) | Mine | — |
| `infra/services/mh-service/kustomization.yaml` | Mine | — |
| `infra/services/mh-service/service.yaml` | Mine | — |
| `infra/services/mh-service/network-policy.yaml` (comments) | Mine | — |
| `infra/services/mh-service/config.env` (comments) | Mine | — |
| `infra/services/mc-service/mc-0-deployment.yaml` (comments) | Mine | — |
| `infra/services/mc-service/mc-1-deployment.yaml` (comments) | Mine | — |
| `infra/services/mc-service/config.env` (comment) | Mine | — |
| `infra/services/media-codec-profile/*` (comments) | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/kustomization.yaml` | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/mh-fleet.env` (new; B, read directly by deploy.sh) | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/share-process-namespace-patch.yaml` (new) | Mine | — |
| `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml` (comment) | Mine | — |
| `infra/kubernetes/overlays/kind/services/media-codec-profile/configmap-test-profile-patch.yaml` (comment) | Mine | — |
| `infra/kind/scripts/deploy.sh` | Mine | — |
| `infra/kind/scripts/setup.sh` | Mine | — |
| `infra/kind/scripts/lib/common.sh` (static kind-config constant; jq prerequisite) | Mine | — |
| `infra/kind/scripts/provision.sh` (uses the static kind-config constant) | Mine | — |
| `scripts/setup.test.sh` | Mine | — |
| `scripts/layer7.sh` | Mine | — |
| `scripts/layer7.test.sh` | Mine | — |
| `docs/decisions/adr-0038-*.md` (parity allowlist entry: Kind-only shareProcessNamespace) | Mine | — |
| `crates/dt-guard/src/kustomize.rs` (real-tree test: Deployment/mh-template) | Mine (machinery) | — |
| `crates/dt-guard/src/env_config.rs` (fixtures modelling real files) | Mine (machinery) | — |
| `crates/dt-guard/src/kustomize_resource_field_refs.rs` (fixture rename) | Mine (machinery) | — |
| `crates/env-tests/tests/01_mh_deployment_config.rs` | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/tests/02_mc_deployment_config.rs` | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/tests/28_mh_egress_admission.rs` | Not mine, Minor-judgment (paired) | test |
| `crates/env-tests/tests/29_mh_meeting_teardown.rs` | Not mine, Minor-judgment (paired) | test |
| `crates/env-tests/tests/42_mh_address_invariant.rs` (new; R-28) | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/tests/43_mh_slot_publish_udp.rs` (new; R-27 publish leg, own binary) | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/src/fixtures/kube.rs` | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/src/fixtures/deployment_config.rs` | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/src/fixtures/mh_pool.rs` (new; `list()` reader only, T-1 extends) | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/src/fixtures/mh_address.rs` (new; pure R-28 verdict) | Not mine, Domain-judgment (paired) | test |
| `crates/env-tests/tests/41_gate_zero_hostport_policy.rs` (comments + PASS-line text) | Not mine, Mechanical (paired) | test |
| `crates/env-tests/README.md` | Not mine, Mechanical (paired) | test |
| `docs/specialist-knowledge/test/INDEX.md` | Not mine, Mechanical (paired) | test |
| `crates/env-tests/src/fixtures/mod.rs` | Not mine, Mechanical (paired) | test |
| `crates/env-tests/src/fixtures/gate_zero.rs` (comments + fixture names) | Not mine, Mechanical (paired) | test |
| `crates/env-tests/src/canary.rs` (fixture name) | Not mine, Mechanical (paired) | test |
| `scripts/dev-web.sh` | Not mine, Domain-judgment | client |
| `scripts/dev-web.test.sh` | Not mine, Domain-judgment | client |
| `crates/mh-service/src/config.rs` (test + doc comments) | Not mine, Minor-judgment | media-handler |
| `crates/mh-service/tests/common/otel_capture.rs` | Not mine, Minor-judgment | media-handler |
| `docs/runbooks/mh-deployment.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mh-incident-response.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `docs/runbooks/mc-deployment.md` (stale MH lines) | Not mine, Mechanical | operations |
| `docs/runbooks/mc-incident-response.md` (stale MH lines) | Not mine, Mechanical | operations |
| `docs/runbooks/gc-deployment.md` (one stale MH restart command) | Not mine, Mechanical | operations |
| `docs/runbooks/client-dev-local.md` | Not mine, Minor-judgment | client |
| `infra/grafana/dashboards/mh-media.json` (one description string) | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/mc-service.md` (one line) | Not mine, Minor-judgment | observability |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/media-handler/INDEX.md` | Not mine, Mechanical | media-handler |
| `docs/specialist-knowledge/observability/INDEX.md` | Not mine, Mechanical | observability |
| `docs/specialist-knowledge/operations/INDEX.md` | Not mine, Mechanical | operations |
| `docs/TODO.md` (stale mh-0/mh-1 pointers in existing entries; close/narrow the ones this resolves) | Not mine, Minor-judgment | operations |

---

## Planning

**Mechanism restatement.** The instance-language task is "replace mh-0/mh-1 with slots". The mechanism is: *a client-facing MH address is a property of a host-port slot, and every artifact that encodes "which MHs exist" (manifests, port maps, kind mappings, ConfigMap names, preflights, tests, runbook commands) must instead read the live pool (`mh.sh list --json`) or the one slot table.* The wider class this produces beyond the task text: `kube.rs:MH_INSTANCES` readers in tests 28/29 (not only 01/02), `PROFILE_READERS`, `advertise_instances()`'s MH half, setup.sh's printed `mh-service-0/1` URLs and `rollout restart deployment/mh-0 deployment/mh-1` hint, ~20 pre-existing wrong `deployment/mh-service` runbook commands (ops asked to fix in the same pass), `client-dev-local.md`, the grafana description, MC/codec-profile comments, four specialist INDEX files and stale `docs/TODO.md` pointers. All are in the table above. **Out of the class (not instances):** `"mh-1"`-style handler-id/URL strings in MC/GC/proto-gen/sdk unit tests and mh-service `mc_client` tests — synthetic IDs, untouched.

### 1. Slot pool (one home: `crates/devloop-helper/src/ports.rs`)
- `PortOffsets::MH_SLOT_BASE = 20`, `MH_SLOT_COUNT = 6` (K), `mh_slot_offsets()` iterator; MH_0_*/MH_1_* consts go. `MH_HEALTH = 31` (setup.sh's svc/mh-service port-forward; outside the slot range, not 104). Const asserts: slot range ⊂ 20..=29, disjoint from every other offset. Comment: changing K = helper rebuild + cluster recreate.
- **Node port == host port == pod hostPort == `MH_HOST_PORT`** per slot (one value, no second formula). Helper: `base+20+k`; static host mode: 4440..4445 (loopback). Pod containerPort stays 4434/UDP (MH bind unchanged); only `hostPort` varies (NP `UDP 4434` ingress covers every slot post-DNAT — S1).
- `PortEntries`: six MH fields → `mh_health: u16`, `mh_slots: Vec<u16>`. `verify_ports_available` iterates the slots. `write_port_map_shell`: `MH_HEALTH_PORT` (new offset); **no MH WebTransport lines** — deploy reads slots from the blueprint-verified kind-config (below), dev-web/env-tests from `mh.sh list`. Tests updated.
- `template_env_vars` renders `MH_SLOT_PORT_MAPPINGS` (the whole UDP block, from K, `listenAddress ${HOST_GATEWAY_IP}`, each mapping preceded by a machine marker `# dt:mh-slot=<k>`). `.tmpl` carries the one placeholder; dead MH health/gRPC (30830/30530/30831/30531) and 30434/30436 mappings go. Helper unit tests: rendered block == table (parse, non-vacuous, port/listenAddress/protocol UDP per slot — S3); **static `kind-config.yaml` pinned by the same test** (K marked mappings, 127.0.0.1, UDP, containerPort==hostPort, unique — DRY-2). Also: rendered kind-config contains no unsubstituted `${` (an old helper binary fails loudly).

### 2. Single template (`infra/services/mh-service/deployment.yaml`, Deployment `mh-template`)
- Byte-for-byte the mh-0 pod spec (env order, every hard/optional configMapKeyRef, POD_IP before MH_GRPC_ADVERTISE_ADDRESS, downward-API MH_POD_MEMORY_LIMIT_BYTES with literal `limits.memory`, container name `mh-service` (O-1), ports 8083/50053/4434-UDP named `webtransport`, tls mount (add `readOnly: true`; defaultMode unchanged — S5), probes, securityContext, restartPolicy Always). Labels app=mh-service + component=media; no `instance`. hostPort placeholder only on `webtransport` (mh.sh sets it); none on 8083/50053, no hostNetwork (S2). `strategy: Recreate` so an accidental `rollout restart`/`set image` on an `mh-s<k>` is sequential reuse, never a Pending surge.
- `MH_WEBTRANSPORT_ADVERTISE_ADDRESS`: HARD ref to base generator `mh-slot-template-config` (`slot-template.env`, sentinel `https://mh-slot-unassigned.invalid:1`), labelled `dark-tower.io/mh-template-input`. It exists only to satisfy env-config; deploy.sh **never applies** it, and mh.sh rewrites the ref to slot k's ConfigMap by label (create_mh) — an unrewritten pod fails CreateContainerConfigError (fail-closed; answers media-handler 4).
- kustomization: per-instance generators + resources go; `replacements:` = ONE entry (mh-template → itself), comment rewritten. service.yaml: per-instance NodePort Services deleted, ClusterIP mh-service kept. PDB kept. NP comments updated (no 30434).
- Kind overlay: `mh-fleet` generator (`mh-fleet.env`: `MH_BASELINE=2`, B's one home, label `dark-tower.io/mh-template-input`); `share-process-namespace-patch.yaml` (only here; comment: test instrumentation for the zombie leg, pod stays single-container — S4). ADR-0038 gains a short "Kind-only test instrumentation (parity allowlist)" entry. Hermetic setup.test.sh cases: base render has no `shareProcessNamespace`, Kind render has it; rendered template labels ⊇ `network-policy.yaml` podSelector.matchLabels read from the file (+ positive control) (S1); template JSON has no Secret data/PEM (S5).

### 3. deploy.sh
- `mh_slot_table()` — THE slot-table source (exposed for GC-2): parses `${DT_KIND_CONFIG:-infra/kind/kind-config.yaml}` (the file `check_blueprint` just proved the cluster was built from) for `# dt:mh-slot=<k>` mappings → `slot host_port address`, address = `https://<listenAddress>:<hostPort>` via ONE `advertise_url()` formatter also used by the MC merge (DRY-3). Fails loudly: zero slots, duplicate slot/port, non-UDP, containerPort≠hostPort, listenAddress ≠ DT_HOST_GATEWAY_IP when set (remedy: rebuild helper + `dev-cluster recreate`). `kind_pod_subnet()` reads podSubnet from the same file. advertise_instances() becomes MC-only (`mc-service/mc-[0-9]*`).
- `render_env_overlay`: always (both modes) emits one generator per slot `mh-slot-<k>-config` (labels `dark-tower.io/mh-slot=<k>`, app/component; literals MH_WEBTRANSPORT_ADVERTISE_ADDRESS, MH_HOST_PORT). Values come only from the slot table (S6).
- `apply_env_root`: render wrapper once → JSON via offline `kubectl annotate --local -o json` (stripped) → jq split: exactly one `Deployment/mh-template` (+ `mh-template-input` docs) are template inputs; everything else applied as a List (no `--prune`, so `mh-s<k>` and live-referenced generations are never deleted — ops 4c). Secrets stay in pipes/vars, never files/stdout.
- Slot-CM retirement: a slot ConfigMap labelled mh-slot but not in this render's set loses the `mh-slot` label (never deleted: live pods may reference it) — else mh.sh sees a duplicate (`no-template`).
- `mh-template` ConfigMap: `deployment.json` (template, content-tagged image + hash refs), `revision` (first 16 hex of sha256 of canonical `jq -S -c` JSON — one place), `baseline` (from mh-fleet), additive `previous_images` (images of the template being replaced when revision changes; carried forward otherwise). Comment: write access to mh-template/slot CMs = "run a hostPort pod spec"; acceptable under today's Kind RBAC (S6).
- `env_root_workloads()`/`wait_for_env_root()` exclude `deployment/mh-template`.
- New step `converge_mh_pool` after `wait_for_env_root`: headroom check K ≥ B+1 (else `mh-no-free-slot` before touching anything — ops 4e) → `mh.sh upgrade --json` → top up with `mh.sh up --owner baseline --json` until baseline-owned non-terminating MHs == B; excess baseline MHs (B lowered) drained one at a time → final `list` asserts B serving. mh.sh is branched on its `--json` `.reason` token, never stderr text or exit literals.
- Failure mapping (one line from deploy_failed, existing shape + mh.sh detail): `mh-no-free-slot`→`REASON=mh-no-free-slot`; `mh-rollout-failed` (upgrade) and `not-available-timeout` (top-up) → `REASON=mh-rollout-failed`; every other mh.sh token (`kubectl-failed, mh-pool-busy, no-template, drain-timeout, stuck-terminating, last-serving-mh, foreign-owner, no-kubeconfig, usage, prerequisite-missing, internal-error`) → `REASON=mh-pool-failed`. All carry `MH_TOKEN=<token> SLOT=<k|->`. Header + devloop-validation.md §6.7 updated; setup.test.sh cases per reason (token + exactly one line), with stub mh.sh. Old MH keeps serving on failure (mh.sh guarantees). After `mh-rollout-failed` the old MHs show `stale` — runbook says so (ops 4b).
- `prune_superseded_images`: MH repo keep set = this converge's ref ∪ images of live `app=mh-service` pods ∪ mh-template `previous_images`; unreadable → skip MH prune (PRUNE_WARN). Comment: ConfigMap generations referenced by a live MH pod are never pruned (none are).
- Old cluster/old helper: kind-config is a blueprint input, so a pre-I-2 cluster fails at `check_blueprint` (`blueprint-changed`, "provision it first"); an old helper binary leaves `${MH_SLOT_PORT_MAPPINGS}` unrendered → `mh_slot_table` fails naming the helper rebuild (ops 3).

### 4. mh.sh / layer7
- mh.sh and mh.test.sh are UNCHANGED (the interface produced matches the I-5 header exactly). DRY-5: setup.test.sh runs deploy.sh's produced mh-template + slot ConfigMaps through **mh.sh's own `template_jq`/`slots_jq`** (sourced) — proves the contract, not the spelling.
- layer7.sh exports `DT_MH_SH` (absolute); layer7.test.sh asserts exported, absolute, existing.

### 5. Consumers
- **env-tests (paired with @paired-test, who I propose writes them; I review the infra facts):** R-28 test `42_mh_address_invariant.rs` per paired-test's (a)-(e) — non-tautological: pod set == live `app=mh-service` pods, per pod kubectl-read slot label → ONE slot CM, hostPort == MH_HOST_PORT == address port, process-observed address from the MH startup log line == slot CM, uniqueness of addresses and (hostIP,hostPort), pod labels ⊇ live NP podSelector (S1), positive controls; DT_MH_SH unset → loud panic. R-27 `43_mh_slot_publish_udp.rs` own binary: QUIC/WebTransport connect from the runner to each serving slot's `address` (host gateway:host port, never node-direct — S8), MH-side positive evidence `mh_webtransport_connections_total{status="accepted"}` delta > 0 on `instance=<pod_ip>:8083`, others unmoved; failure = `R27 slot-design-failure` + escalate. Uses existing series only (O-6). Tests 01/02/28/29, `kube.rs` (`MH_INSTANCES`/`PROFILE_READERS` removed; MH reads resolved from live labelled pods), `deployment_config.rs` (template + slot CMs; KIND_SIZING_RESTORE_CMD checked), gate_zero/canary fixture names + comments. One `mh_pool.rs::list()` reader for both new tests (T-1 hoists).
- **dev-web.sh (client):** per client 1-8: `"${DT_MH_SH:-infra/kind/scripts/mh.sh}" list --json`, jq-only parse (S7), exit status + `.ok/.schema_version/.baseline/.slots` validated; new CANNOT-VERIFY causes (mh.sh absent, jq absent, mh.sh non-zero (print REASON as data), bad JSON/schema, zero slots) with header count + test kept in sync; serving count (listener + family pass) < max(baseline,1) → hard fail; per-occupied-slot check via ONE refactored `check_wt_endpoint` path shared with MC (file vs URL input); starting/draining/terminating → NOTICE, not counted; free → silent. MC glob narrowed to `mc-service/mc-[0-9]*`. Static slot addresses are `https://127.0.0.1:444x` (loopback literal), so the family branch is unchanged; a test case uses an address taken from `mh_slot_table` on the real static kind-config. dev-web.test.sh stubs mh.sh with keys from `MH_JQ_PUBLIC`.
- **MH crate (media-handler):** config.rs memory test → single `include_str!` of the template (loop dropped); doc comments ~1819/1947/3327 repointed; otel_capture.rs → template path + comment. No MH Rust/env change (topology-blind).
- **dt-guard:** kustomize.rs real-tree test expects `Deployment/mh-template` (one); env_config/kustomize_resource_field_refs fixtures renamed where they model real files.
- **Runbooks (operations):** every site in ops' inventory incl. the ~20 pre-existing `deployment/mh-service` commands (same one-line replacements: `pod/<pod from mh.sh list --json>`, `kubectl logs -l app=mh-service --prefix`, `mh.sh up`/`drain`, rollback = previous checkout + deploy.sh → `mh.sh upgrade`; never `rollout restart` an `mh-s<k>`), `pod=~"mh-service-.*"` → `container="mh-service"` (O-3), K and B Configuration Reference rows (misset behaviour), a prominent recreate-across-this-commit note with its symptom, `mh.sh list` as first triage. client-dev-local.md lines per client 8.
- INDEX ×4, TODO.md pointers, comments elsewhere.

**No new metric series, no pod-label promotion to metrics/Loki labels (O-2/O-6, obligation c/e).** No GSA path is touched.

**Gate-1 conditions carried into implementation:**
- (security) setup.test.sh asserts, from the rendered template, that the advertise ref is NOT `optional: true` and that the placeholder ConfigMap is absent from the applied set. R-28 checks each live pod's ref name == its slot CM name.
- (operations) An MH prune skip prints `PRUNE_SKIPPED SERVICE=mh REASON=<...>` on stderr. `mh-pool-failed` (with MH_REASON/SLOT) and the next oncall step for each of the three MH reasons go into the runbook DEPLOY_FAILED catalogue.
- (observability, for paired-test) The R-27 delta is measured against a same-instance pre-connect baseline with the poll helpers and never a fixed sleep. A missing R-28 startup line gets its own fail token.
- (code-reviewer) mh.sh-token → deploy-reason mapping is ONE assoc table in deploy.sh. `mh-pool-failed` is kept because operations branches on it differently from `step-failed` (it is a pool/tool fault, not a tree step). It is added to the header REASON enumeration and devloop-validation.md §6.7. The extra field is `MH_TOKEN=<token> SLOT=<k|->`, so no `REASON=` substring; it sits before `WORKLOADS=`. A setup.test.sh case parses the line with layer7's real relay regex. The slot marker is spelled once in each language: the Rust const in ports.rs (emitter) and `MH_SLOT_MARKER` in deploy.sh (parser). A setup.test.sh drift case asserts the two are equal, and the helper test pins the static file. Every mh_slot_table failure mode gets its own case and distinct message, plus a positive control.

**Layer 7 needs:** helper rebuilt on the host + `dev-cluster recreate` (new kind mappings; old cluster fails `check_blueprint`).

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (S1-S8 + placeholder-ref condition) |
| Test (paired) | confirmed |
| Observability | confirmed (O-1..O-6) |
| Code Quality | confirmed (mapping table, MH_TOKEN field, marker single-home) |
| DRY | confirmed |
| Operations | confirmed (PRUNE_SKIPPED line, mh-pool-failed catalogue) |
| Client | confirmed |
| Media Handler | confirmed |

Classification-sanity guard: `STATUS=OK`. Lead accepted implementer decisions (1)-(5): one port value per slot (static 4440-4445), slot table from blueprint-verified kind-config, strategy Recreate, `mh-pool-failed` reason, `previous_images` key.

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

Implemented as planned (Planning section), with one deviation: **B is not a ConfigMap generator**. A generator in
the Kind overlay that no pod reads trips dt-guard R-21 `configmap_unreferenced` (no exemption by design), so
`mh-fleet.env` stays B's one home and `deploy.sh:write_mh_template` reads it directly into `mh-template.baseline`
(every other reader takes `.baseline` back from there).

- **Slot pool**: `ports.rs` K=6 at offsets 20..25 (`MH_SLOT_BASE/MH_SLOT_COUNT`, const-asserted disjoint), `MH_HEALTH`=31;
  kind-config `.tmpl` MH block rendered from K (`${MH_SLOT_PORT_MAPPINGS}`, `# dt:mh-slot=<k>` markers); static
  `kind-config.yaml` slots 4440..4445 on 127.0.0.1, pinned by a helper unit test. No MH lines in `port-map.env`.
- **Template**: `infra/services/mh-service/deployment.yaml` (`mh-template`, Recreate), placeholder advertise generator
  `slot-template.env` (never applied), ONE `replacements:` entry, per-instance NodePort Services deleted; Kind-only
  `shareProcessNamespace` patch; ADR-0038 parity-allowlist entry.
- **deploy.sh**: `mh_slot_table()` (from the blueprint-verified kind-config) → per-slot generators in
  `render_env_overlay`; one render split by `render_env_root_json` + `MH_TEMPLATE_PART_JQ`; `write_mh_template`
  (revision, baseline, `previous_images`); `retire_stale_slot_configmaps`; `converge_mh_pool` (headroom, `mh.sh upgrade`,
  top-up/drain to B) with the ONE `MH_DEPLOY_REASONS` map; MH prune keep set + `PRUNE_SKIPPED`. `jq` added to prerequisites.
- **layer7.sh** exports `DT_MH_SH` + `DT_CLUSTER_NAME` (Phase 1h). Relay regex carries `MH_TOKEN`/`SLOT`.
- **Consumers**: env-tests (paired-test), dev-web.sh `check_mh_pool` + shared `check_wt_url`, MH crate tests, dt-guard
  fixtures, runbooks/INDEX/TODO/dashboard/metrics-doc pointer fixes.

**Layer 7 needs** (Lead): rebuild devloop-helper on the HOST (ports.rs + kind-config template changed), then
`dev-cluster recreate` — the existing cluster fails `check_blueprint` (`blueprint-changed`). Env-tests 42 (R-28, feature
`smoke`) and 43 (R-27, feature `flows`, own binary) run for the first time there; a 43 failure is a slot-design
failure → escalate.

**Remaining host-side actions**: rebuild devloop-helper on the host, then recreate every existing Kind cluster; the
first deploy and any rollback across this commit needs a recreate (R-30).

---

## Files Modified

```
{Output of: git diff --stat HEAD}
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

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
**Status**: PASS (attempt 3)
**Duration**: ~1361s
**Output**: env-tests + browser E2E green; 42 (R-28) and 43 (R-27) passed. Log: `/tmp/devloop/layer-7-env-test.log`

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

## Gate 3 — Verdicts (Lead record)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | S1-S8 + placeholder-ref condition verified |
| Test (paired) | RESOLVED-FIXED (it2: CLEAR) | 2 | 2 | 0 | prune keep-set live-pod arm; slot-CM retirement both sides |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | runbook describe/events; ordinal wording; mh_run env classification |
| Code Quality | RESOLVED-FIXED | 7+2 | 9 | 0 | baseline≥1 guard; check_pod; address_authority; it2 D1/D2 |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | |
| Operations | RESOLVED-FIXED | 6+1 | 7 | 0 | --owner on recipes; 8083 port; recreate-order remedy (it2 F7) |
| Client | RESOLVED-FIXED | 4 | 4 | 0 | dev-web.sh |
| Media Handler | RESOLVED-FIXED | 1 | 1 | 0 | |
| Semantic Guard | — | — | — | — | not spawned |

## Gate 2 — Validation Log

- Attempt 1 (`layer-all.sh`): L1-6 OK; L7 `PRECONDITION_FAILURE cluster-provision-failed` — `PROVISION_FAILED REASON=runtime-incapable STEP=create_cluster`. Root cause: the HOST devloop-helper is the pre-I-2 binary, so `/tmp/devloop/kind-config.yaml` kept a literal `${MH_SLOT_PORT_MAPPINGS}`; provision deleted the old cluster on blueprint-changed and `kind create` then failed on YAML. Diff-caused misclassification → routed to implementer (iteration 2): new `check_kind_config_rendered` step refuses an unrendered kind config BEFORE blueprint/delete with `REASON=kind-config-unrendered` (operator lane). Re-reviewed by test/ops/code-reviewer.
- Attempt 2 (`layer-all.sh`): L1-6 OK; L7 `PRECONDITION_FAILURE` — `PROVISION_FAILED REASON=kind-config-unrendered STEP=check_kind_config_rendered` (correct, nothing deleted). Deterministic host precondition: the container cannot rebuild the host helper. **Escalated `host-op-needed`; NOT committed.** Env-tests 42 (R-28) and 43 (R-27) have never run.

- Resume (2026-11-08, `--continue`): operator rebuilt the host helper and re-provisioned; `/tmp/devloop/kind-config.yaml` rendered with 6 `dt:mh-slot` markers; `dev-cluster status` healthy, MH pool B=2/K=6.
- Attempt 3 (`layer-all.sh`): `TOTAL_RESULT=OK` — L1-6 OK, L7 OK (`env-tests-passed`, `browser-e2e-passed`). Test 42 `every_live_mh_pod_advertises_its_own_unique_slot_address` and test 43 `udp_to_each_serving_slot_reaches_that_slots_mh` ran and passed. Compiler surfaced an unused `kubectl_get_json` import in test 43 → removed (Lead, trivial fix), which changes the tree.
- Attempt 4 (`layer-all.sh`) on the final tree incl. this main.md — the attesting Gate-2 verdict (`/tmp/devloop/gate2-verdict`).

**Host actions (done before resume):** (1) rebuild devloop-helper on the HOST from this tree (re-run `infra/devloop/devloop.sh`); (2) `dev-cluster provision` / `recreate` (the old cluster `devloop-mh-failure-transfer` was already deleted by attempt 1); (3) resume this devloop (`--continue=2026-11-07-mh-hostport-slot-pool`) to run Gate 2 incl. Layer 7. A test-43 `R27 slot-design-failure` is a slot-design escalation, not a work-around.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `{the whole commit | <unit + safe direction>}` (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

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
