# User Story: Migrate K8s Manifests to Kustomize

**Date**: 2026-03-30
**Status**: Planning
**Participants**: infrastructure, operations, test, security, observability, auth-controller, global-controller, meeting-controller, media-handler, database, protocol

## Story

As a **platform engineer**, I want **K8s manifest management to use Kustomize bases and overlays instead of glob-apply bash loops** so that **adding or removing manifests is explicit, deployment ordering bugs are eliminated, and the local dev setup aligns structurally with future production deployments**.

## Requirements

- [ ] R-1: Each service directory gets a `kustomization.yaml` that explicitly lists its resources (from: infrastructure)
- [ ] R-2: A Kind overlay excludes resources not applicable to local dev — service-monitor.yaml, tls-secret placeholder (from: infrastructure, security)
- [ ] R-3: Inline PostgreSQL manifests extracted from setup.sh heredocs to standalone YAML files with own kustomization.yaml (from: infrastructure)
- [ ] R-4: Inline Grafana RBAC/deployment manifests extracted from setup.sh to standalone YAML files (from: infrastructure, observability)
- [ ] R-5: Grafana dashboard ConfigMaps use Kustomize `configMapGenerator` with explicit file listing — replaces bash generation script (from: infrastructure, observability)
- [ ] R-6: setup.sh uses sequential `kubectl apply -k` calls instead of glob-apply loops, preserving deployment order (from: infrastructure)
- [ ] R-7: `tls-secret.yaml` placeholder deleted from repo entirely — documentation moved to setup.sh comment or README (from: security, meeting-controller)
- [ ] R-8: Secret values never inlined via Kustomize `secretGenerator` — secrets remain imperatively created (from: security)
- [ ] R-9: Security contexts and volume mount permissions preserved through migration (from: security)
- [ ] R-10: NetworkPolicy manifests included in all service bases (from: infrastructure, security)
- [ ] R-11: Observability stack switches from individual `kubectl apply -f` to `kubectl apply -k` using existing kustomization.yaml (from: observability)
- [ ] R-12: `environment` label applied via overlay (environment-specific), `managed-by` label in base (from: observability)
- [ ] R-13: Deployment runbooks updated with Kustomize commands and manifest structure documentation (from: operations)
- [ ] R-14: All existing env-tests pass identically after migration (from: infrastructure, test, operations)
- [ ] R-15: CI guard validates `kustomize build` succeeds for all bases and overlays (from: test, observability)
- [ ] R-16: CI guard validates no orphan manifests — every `.yaml` in service dirs is listed in kustomization.yaml or on explicit exclusion list (from: operations, test)
- [ ] R-17: CI guard validates kustomize build output against K8s API schemas via kubeconform (from: test)
- [ ] R-18: CI guard validates security contexts preserved in kustomize build output (from: security)
- [ ] R-19: CI guard validates no empty secret values in kustomize build output (from: security)
- [ ] R-20: CI guard validates all dashboard JSON files in `infra/grafana/dashboards/` are listed in the configMapGenerator (from: infrastructure, observability)

---

## Architecture Validation

**Result**: PASS — all 11 specialists confirmed. Pure infrastructure reorganization within existing patterns. Observability stack already uses Kustomize successfully.

**Opt-outs**: auth-controller (no app changes), global-controller (no app changes), meeting-controller (no app changes, confirmed TLS exclusion), media-handler (no K8s manifests), database (no schema changes), protocol (no protocol changes).

---

## Design

### Infrastructure

#### Directory Structure (After Migration)

```
infra/
├── services/
│   ├── ac-service/
│   │   ├── kustomization.yaml          # Base — explicit resource list
│   │   ├── configmap.yaml
│   │   ├── statefulset.yaml
│   │   ├── service.yaml
│   │   ├── pdb.yaml
│   │   ├── network-policy.yaml
│   │   └── service-monitor.yaml        # NOT in kustomization.yaml (needs Prometheus Operator)
│   ├── gc-service/
│   │   ├── kustomization.yaml
│   │   ├── configmap.yaml, deployment.yaml, service.yaml, secret.yaml, pdb.yaml, network-policy.yaml
│   │   └── service-monitor.yaml        # NOT in kustomization.yaml
│   ├── mc-service/
│   │   ├── kustomization.yaml
│   │   ├── configmap.yaml, deployment.yaml, service.yaml, secret.yaml, pdb.yaml, network-policy.yaml
│   │   └── service-monitor.yaml        # NOT in kustomization.yaml
│   │   # tls-secret.yaml DELETED (R-7)
│   ├── postgres/                       # NEW — extracted from setup.sh (R-3)
│   │   ├── kustomization.yaml
│   │   ├── secret.yaml, pvc.yaml, statefulset.yaml, service.yaml
│   └── redis/
│       ├── kustomization.yaml
│       ├── configmap.yaml, statefulset.yaml, service.yaml, secret.yaml, network-policy.yaml
├── kubernetes/
│   ├── observability/
│   │   ├── kustomization.yaml          # UPDATED — includes grafana/, migrates to labels
│   │   ├── grafana/                    # NEW — extracted from setup.sh (R-4)
│   │   │   ├── kustomization.yaml
│   │   │   ├── rbac.yaml, deployment.yaml, service.yaml
│   │   ├── prometheus-config.yaml, loki-config.yaml, promtail-config.yaml
│   │   ├── kube-state-metrics.yaml, node-exporter.yaml
│   └── overlays/
│       └── kind/                       # NEW (R-2)
│           ├── kustomization.yaml      # Top-level (for CI validation)
│           ├── services/
│           │   ├── kustomization.yaml  # Aggregates all service overlays
│           │   ├── {ac,gc,mc}-service/kustomization.yaml  # Refs base
│           │   ├── postgres/kustomization.yaml
│           │   └── redis/kustomization.yaml
│           └── observability/
│               └── kustomization.yaml  # Adds environment: kind label
```

#### Kustomization.yaml Pattern

Service bases use `labels` with `includeSelectors: false` (NOT `commonLabels`) to avoid mutating `selector.matchLabels` on Deployments/StatefulSets:

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - configmap.yaml
  - deployment.yaml
  - service.yaml
  - pdb.yaml
  - network-policy.yaml
  # service-monitor.yaml NOT listed — requires Prometheus Operator CRD
labels:
  - pairs:
      managed-by: dark-tower
    includeSelectors: false
```

`service-monitor.yaml` excluded from base by omission (not by overlay). Kind overlay is thin — just references base, no exclusions needed. Future production overlay adds monitoring resources.

#### Observability Kustomization Migration

Existing `commonLabels` migrated to `labels` with `includeSelectors: false` to avoid selector mutation when adding the new Grafana sub-kustomization. Grafana sub-kustomization has no labels (inherits from parent).

#### setup.sh Transformation

```bash
main() {
    check_prerequisites
    create_cluster
    install_calico
    create_namespaces

    # Data stores
    deploy_postgres         # kubectl apply -k overlays/kind/services/postgres/ + wait
    deploy_redis            # kubectl apply -k overlays/kind/services/redis/ + wait

    # Observability (dashboards included via configMapGenerator)
    deploy_observability    # kubectl apply -k overlays/kind/observability/ + wait

    # Post-deploy steps (remain scripted)
    run_migrations
    seed_test_data
    create_ac_secrets

    # Application services
    build_and_load_ac_image
    deploy_ac_service       # kubectl apply -k overlays/kind/services/ac-service/ + wait
    build_and_load_gc_image
    deploy_gc_service       # kubectl apply -k overlays/kind/services/gc-service/ + wait
    create_mc_tls_secret    # Must happen BEFORE mc-service deploy
    build_and_load_mc_image
    deploy_mc_service       # kubectl apply -k overlays/kind/services/mc-service/ + wait

    install_telepresence
    setup_port_forwards
    print_access_info
}
```

---

## Cross-Cutting Requirements

### Security

- **R-7**: Delete `tls-secret.yaml` — empty placeholder is a footgun. TLS secret creation documented in `create_mc_tls_secret()`.
- **R-8**: No `secretGenerator` with literal values. All secrets created imperatively in setup.sh.
- **R-9**: Existing security contexts (`runAsNonRoot`, `readOnlyRootFilesystem`, `allowPrivilegeEscalation: false`, `capabilities.drop: ALL`) and volume mount permissions (`defaultMode: 0400` on mc-tls) must be preserved. Validated by CI guard (R-18).
- **R-10**: NetworkPolicy manifests in all service bases. Missing NetworkPolicy degrades silently to allow-all.
- **R-18/R-19**: Security checks in unified CI guard. Check criteria provided by security specialist, implementation by test specialist.

### Observability

- **R-11**: Replace 5 individual `kubectl apply -f` calls with single `kubectl apply -k infra/kubernetes/observability/`.
- **R-12**: `managed-by: dark-tower` in base, `environment: kind` in Kind overlay. Use `labels` with `includeSelectors: false` throughout.
- **R-5**: Grafana dashboard ConfigMaps use Kustomize `configMapGenerator` with explicit file listing. Replaces the bash generation script, keeping dev and production in sync.
- No new application metrics, dashboards, logs, or alerts needed.
- Existing observability guards (`validate-application-metrics.sh`) unaffected.

### Test

- **R-14**: All existing env-tests are the acceptance criteria. No new env-test scenarios needed.
- **R-15–R-19**: Single unified guard script `scripts/guards/simple/validate-kustomize.sh` with 5 check sections. Auto-discovered by `run-guards.sh`. Uses changed-file scoping (skip if no `infra/` changes). `kubeconform` optional (warn-skip if not installed).
- **Risk**: `commonLabels` selector mutation — mitigated by using `labels` with `includeSelectors: false` everywhere.

### Deployment

- Kustomize bases created for all services + postgres + grafana (R-1, R-3, R-4).
- Kind overlay provides environment-specific labels (R-2, R-12).
- setup.sh uses sequential `kubectl apply -k` for ordering (R-6).

### Operations

- **R-13**: Update 3 deployment runbooks: replace `kubectl apply -f` with `kubectl apply -k`, add "Manifest Structure" section, update References.
- Rollback procedures unchanged (`kubectl rollout undo` is apply-method agnostic).
- No new incident response scenarios — failure modes are all development-time (Kustomize build errors, not production incidents).
- setup.sh idempotency preserved (`kubectl apply -k` is idempotent).

---

## Assumptions

| # | Assumption | Made By | Reason Not Blocked |
|---|-----------|---------|-------------------|
| 1 | `environment` label applied at top-level Kind overlay, not per-service | infrastructure | Consistent labeling, simpler structure |
| 2 | Observability `environment` value changes from `dev` to `kind` to match Kind overlay | infrastructure | This IS the Kind environment; `dev` was inaccurate |
| 3 | Redis `network-policy.yaml` included in base (currently skipped by setup.sh) | infrastructure | Skip appears to be oversight; env-tests expect NetworkPolicy enforcement |
| 4 | Kustomize v5.4+ available (for `labels` with `includeSelectors`) | infrastructure | kubectl v1.27+ bundles this; verify in `check_prerequisites` |
| 5 | `kubeconform` is optional in CI — guard warns and skips if not installed | test | Avoids blocking CI until tool is added |

## Clarification Questions

| # | Question | Asked By | Status | Answer |
|---|---------|----------|--------|--------|

---

## Implementation Plan

| # | Task | Specialist | Dependencies | Covers | Status |
|---|------|-----------|--------------|--------|--------|
| 1 | Create per-service Kustomize bases, extract PostgreSQL/Grafana inline manifests, add dashboard configMapGenerator, delete tls-secret.yaml, migrate observability kustomization | infrastructure | — | R-1, R-3, R-4, R-5, R-7, R-8, R-9, R-10, R-11, R-12 | Pending |
| 2 | Create Kind overlay structure and rewrite setup.sh to use kubectl apply -k | infrastructure | 1 | R-2, R-6, R-14 | Pending |
| 3 | Add validate-kustomize CI guard (R-15 through R-20) | test | 1 | R-15, R-16, R-17, R-18, R-19, R-20 | Pending |
| 4 | Update deployment runbooks for Kustomize manifest management | operations | 2 | R-13 | Pending |

### Requirements Coverage

| Req | Covered By Tasks |
|-----|-----------------|
| R-1 | 1 |
| R-2 | 2 |
| R-3 | 1 |
| R-4 | 1 |
| R-5 | 2 |
| R-6 | 2 |
| R-7 | 1 |
| R-8 | 1 |
| R-9 | 1, 3 |
| R-10 | 1 |
| R-11 | 1 |
| R-12 | 1, 2 |
| R-13 | 4 |
| R-14 | 2 |
| R-15 | 3 |
| R-16 | 3 |
| R-17 | 3 |
| R-18 | 3 |
| R-19 | 3 |
| R-20 | 3 |

### Aspect Coverage

| Aspect | Covered By Tasks | N/A? |
|--------|-----------------|------|
| Code | 1, 2, 3 | |
| Database | | N/A — no schema changes |
| Tests | 3 (CI guard), 2 (env-tests as acceptance) | |
| Observability | 1 (kustomization migration) | |
| Deployment | 1, 2 | |
| Operations | 4 | |

---

## Devloop Tracking

| # | Task | Devloop Output | PR | Status |
|---|------|---------------|-----|--------|
| 1 | Create Kustomize bases + extract manifests | `docs/devloop-outputs/2026-03-31-kustomize-bases/` | | Completed |
| 2 | Create Kind overlay + rewrite setup.sh | | | Pending |
| 3 | Add validate-kustomize CI guard | | | Pending |
| 4 | Update deployment runbooks | | | Pending |

---

## Revisions

