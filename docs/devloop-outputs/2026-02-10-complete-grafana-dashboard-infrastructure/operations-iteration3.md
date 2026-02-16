# Operations Specialist Checkpoint: Iteration 3 - Kubernetes Environment Fix

**Date**: 2026-02-11
**Agent**: Operations Specialist
**Iteration**: 3 of 5
**Task**: Fix infrastructure for correct environment (Kubernetes KIND cluster, not Docker Compose)

---

## Critical Issue Identified

**Problem**: Iteration 2 applied fixes to wrong environment
- User runs **Kubernetes (KIND cluster)**, not Docker Compose
- All iteration 2 fixes were for Docker Compose (`docker-compose.yml`, `infra/docker/*`)
- These files are not used in the actual deployment environment

**Impact**: Dashboard infrastructure still non-functional because:
- No Kubernetes monitoring stack deployed
- Dashboards query Kubernetes metrics but no exporters exist
- Log dashboards query correct labels but Loki not configured for Kubernetes

---

## Findings Addressed

### Finding 1: Missing Kubernetes Metrics (BLOCKER) - FIXED

**Issue**: Infrastructure panels need Kubernetes-specific metrics but no monitoring stack deployed.

**Root Cause**:
- KIND cluster has no kube-state-metrics or node-exporter deployed
- Prometheus not configured to scrape Kubernetes endpoints
- Dashboards query metrics that don't exist

**Fix Applied**:

1. **Created kube-state-metrics deployment** (`infra/kind/kubernetes/observability/kube-state-metrics.yaml`):
   - ServiceAccount with ClusterRole for Kubernetes API access
   - ClusterRoleBinding for permissions
   - Service exposing metrics on port 8080
   - Deployment with kube-state-metrics v2.10.1
   - Exposes: `kube_pod_info`, `kube_deployment_*`, etc.

2. **Created node-exporter DaemonSet** (`infra/kind/kubernetes/observability/node-exporter.yaml`):
   - DaemonSet to run on every node
   - HostNetwork and HostPID access for system metrics
   - Mounted host paths: /proc, /sys, /root
   - Exposes: node CPU, memory, disk, network metrics
   - Service with ClusterIP: None (headless)

3. **Created Prometheus configuration** (`infra/kind/kubernetes/observability/prometheus-config.yaml`):
   - ConfigMap with complete scrape configs
   - Kubernetes API server scraping
   - Kubelet scraping for node metrics
   - **cAdvisor scraping** (kubelet /metrics/cadvisor endpoint)
   - kube-state-metrics scraping
   - node-exporter scraping
   - Service endpoint discovery with annotations
   - Pod discovery with annotations

**Scrape Configs Added**:
```yaml
# Kubelet cAdvisor (container metrics)
- job_name: 'kubernetes-cadvisor'
  # Scrapes kubelet /metrics/cadvisor for container_* metrics

# kube-state-metrics
- job_name: 'kube-state-metrics'
  # Scrapes kube_pod_info, kube_deployment_*, etc.

# node-exporter
- job_name: 'node-exporter'
  # Scrapes node CPU, memory, disk metrics
```

**Dashboard Queries Supported**:
```promql
# Container memory
container_memory_working_set_bytes{namespace="dark-tower", pod=~"ac-service.*"}

# Container CPU
rate(container_cpu_usage_seconds_total{namespace="dark-tower", pod=~"ac-service.*"}[5m])

# Pod count
count(kube_pod_info{namespace="dark-tower", pod=~"ac-service.*"})
```

### Finding 2: Log Dashboard Label Mismatch (BLOCKER) - FIXED

**Issue**: Dashboards had Docker-specific terminology after iteration 2 changes.

**Root Cause**: Iteration 2 changed variable names from `pod` to `container` for Docker.

**Fix Applied**:
1. **Reverted variable names**: `container` → `pod`
2. **Reverted variable labels**: "Container" → "Pod"
3. **Kept `app` label** for queries (correct for Kubernetes)
   - `{app="ac-service"}` - Correct
   - `{app="global-controller"}` - Correct
   - `{app="meeting-controller"}` - Correct

**Dashboard Query Format**:
```logql
# Correct format for Kubernetes
{app="ac-service", pod=~"$pod"} |~ "${log_level}"
```

### Finding 3: Docker Compose Files Cleanup (CLEANUP) - FIXED

**Issue**: Iteration 2 created Docker Compose infrastructure files that aren't used.

**Fix Applied**:
1. **Reverted docker-compose.yml** - Removed cadvisor, loki, promtail services
2. **Reverted infra/docker/prometheus/prometheus.yml** - Removed cAdvisor scrape config
3. **Deleted infra/docker/loki/local-config.yaml** - Loki runs in Kubernetes
4. **Deleted infra/docker/promtail/config.yml** - Promtail runs in Kubernetes
5. **Removed empty directories**: infra/docker/loki, infra/docker/promtail

**Dashboard Query Fixes**:

**Overview Dashboards** (ac-overview.json, gc-overview.json, mc-overview.json):
- Reverted from Docker format to Kubernetes format
- **Memory**: `container_memory_usage_bytes{name=~"..."}` → `container_memory_working_set_bytes{namespace="dark-tower", pod=~"..."}`
- **CPU**: `name=~"dark_tower_ac.*"` → `namespace="dark-tower", pod=~"ac-service.*"`
- **Pod Count**: `container_last_seen{name=~"..."}` → `kube_pod_info{namespace="dark-tower", pod=~"..."}`
- **Legend**: `{{name}}` → `{{pod}}`

---

## Kubernetes Deployment Files Created

### 1. kube-state-metrics.yaml
```yaml
# Components:
- ServiceAccount (kube-state-metrics)
- ClusterRole (API access permissions)
- ClusterRoleBinding
- Service (port 8080)
- Deployment (1 replica)

# Metrics Exposed:
- kube_pod_info - Pod metadata
- kube_deployment_* - Deployment status
- kube_replicaset_* - ReplicaSet info
- Many more Kubernetes object metrics
```

### 2. node-exporter.yaml
```yaml
# Components:
- ServiceAccount (node-exporter)
- Service (headless, port 9100)
- DaemonSet (runs on all nodes)

# Metrics Exposed:
- node_cpu_seconds_total
- node_memory_*
- node_disk_*
- node_network_*
```

### 3. prometheus-config.yaml
```yaml
# Scrape Configs:
1. prometheus (self-monitoring)
2. kubernetes-apiservers
3. kubernetes-nodes (kubelet)
4. kubernetes-cadvisor (container metrics)
5. kube-state-metrics
6. node-exporter
7. kubernetes-service-endpoints (with annotations)
8. kubernetes-pods (with annotations)
```

---

## Patterns Discovered

### 1. Environment-Specific Metric Sources
**Pattern**: Different deployments need different monitoring approaches
- **Kubernetes**: kubelet cAdvisor + kube-state-metrics + node-exporter
- **Docker Compose**: standalone cAdvisor + container inspection
- **Key Difference**: Metrics naming and labels differ significantly

**Lesson**: Always verify deployment environment before implementing monitoring

### 2. Kubernetes Service Discovery
**Pattern**: Use Kubernetes SD for dynamic service discovery
- Role-based discovery: `endpoints`, `pod`, `node`
- Relabeling to extract metadata from Kubernetes labels
- Annotation-based opt-in: `prometheus.io/scrape: "true"`

### 3. cAdvisor Access Patterns
**Pattern**: cAdvisor access differs between environments
- **Kubernetes**: Kubelet exposes cAdvisor at `/metrics/cadvisor`
- **Docker Compose**: Standalone cAdvisor container on port 8080
- **Metrics**: Same metric names, different labels

### 4. Multi-Component Monitoring Stack
**Pattern**: Kubernetes monitoring requires multiple exporters
- **kube-state-metrics**: Cluster state (pods, deployments)
- **node-exporter**: Node-level metrics (CPU, memory, disk)
- **cAdvisor**: Container metrics (via kubelet)
- **Each serves different purpose**, all needed for complete view

---

## Gotchas Encountered

### 1. Environment Assumption Failure
**Problem**: Assumed Docker Compose based on `docker-compose.yml` existence
**Reality**: User runs Kubernetes KIND cluster; docker-compose.yml unused
**Lesson**: Always verify actual deployment environment with user
**Impact**: Entire iteration 2 wasted on wrong environment

### 2. Kubernetes cAdvisor Access
**Problem**: Can't deploy standalone cAdvisor in Kubernetes
**Solution**: Kubelet already exposes cAdvisor metrics at `/metrics/cadvisor`
**Configuration**: Different scrape path in Prometheus config
```yaml
- target_label: __metrics_path__
  replacement: /metrics/cadvisor
```

### 3. kube-state-metrics Permissions
**Problem**: Requires extensive ClusterRole permissions
**Solution**: Must grant read access to many Kubernetes resources
**Security**: Uses least-privilege (only list/watch, no mutations)

### 4. node-exporter Host Access
**Problem**: Needs access to host filesystem and processes
**Solution**:
- `hostNetwork: true`
- `hostPID: true`
- Mount /proc, /sys, / as volumes
**Security Trade-off**: Necessary for node metrics but increases attack surface

### 5. Pod vs Container Label Naming
**Problem**: Kubernetes uses `pod` label, Docker uses `name` or `container`
**Impact**: Query syntax completely different between environments
**Kubernetes**: `{namespace="dark-tower", pod=~"ac-service.*"}`
**Docker**: `{name=~"dark_tower_ac.*"}`

### 6. Service Discovery Complexity
**Problem**: Kubernetes service discovery has many relabeling rules
**Reason**: Need to extract namespace, pod, service from metadata
**Solution**: Extensive relabel_configs to map `__meta_kubernetes_*` to labels

---

## Key Decisions

### 1. Use Standard Kubernetes Monitoring Stack
**Decision**: Deploy kube-state-metrics + node-exporter + kubelet cAdvisor
**Rationale**:
- Industry standard approach
- Well-documented and maintained
- Provides complete Kubernetes visibility
- No custom solutions needed

**Alternative Considered**: Custom metrics collector
**Rejected Because**: Reinventing wheel, more maintenance burden

### 2. Keep Dashboard Queries Kubernetes-Native
**Decision**: Use `pod` label, `namespace` selector, Kubernetes metric names
**Rationale**:
- Matches Kubernetes conventions
- Works with standard exporters
- Easier for Kubernetes operators to understand
- Portable to other Kubernetes environments

### 3. Revert All Docker Compose Changes
**Decision**: Remove all iteration 2 Docker-specific changes
**Rationale**:
- Not used in actual environment
- Confusing to have both environments configured
- Clean separation: docker-compose.test.yml for tests only
- Production runs in Kubernetes

### 4. Scrape Kubelet Directly for cAdvisor
**Decision**: Configure Prometheus to scrape kubelet `/metrics/cadvisor`
**Rationale**:
- cAdvisor already running in kubelet
- No extra deployment needed
- Standard Kubernetes practice
- Lower resource overhead

**Alternative Considered**: Deploy standalone cAdvisor DaemonSet
**Rejected Because**: Redundant, kubelet already provides it

### 5. Use ConfigMap for Prometheus Config
**Decision**: Store Prometheus configuration in Kubernetes ConfigMap
**Rationale**:
- Kubernetes-native configuration management
- Easy to update via kubectl
- Version controlled in git
- Can be mounted into Prometheus pods

---

## Verification Results

All 7 verification layers passed:

### Layer 1: cargo check --workspace
```
✅ PASSED - All crates compiled successfully
```

### Layer 2: cargo fmt --all --check
```
✅ PASSED - No formatting issues
```

### Layer 3: ./scripts/guards/run-guards.sh
```
✅ PASSED - 9/9 guards passed
All dashboard JSON validated
```

### Layer 4: ./scripts/test.sh --workspace --lib
```
✅ PASSED - 153 unit tests passed
Note: One timing test flaky on first run, passed on retry
```

### Layer 5: ./scripts/test.sh --workspace
```
✅ PASSED - All tests (unit + integration) passed
```

### Layer 6: cargo clippy --workspace -- -D warnings
```
✅ PASSED - No clippy warnings
```

### Layer 7: ./scripts/guards/run-guards.sh --semantic
```
✅ PASSED - 10/10 semantic guards passed
```

---

## Files Created

1. `/home/nathan/code/dark_tower/infra/kind/kubernetes/observability/kube-state-metrics.yaml`
2. `/home/nathan/code/dark_tower/infra/kind/kubernetes/observability/node-exporter.yaml`
3. `/home/nathan/code/dark_tower/infra/kind/kubernetes/observability/prometheus-config.yaml`

---

## Files Modified

1. `/home/nathan/code/dark_tower/infra/grafana/dashboards/ac-overview.json` - Reverted to Kubernetes metrics
2. `/home/nathan/code/dark_tower/infra/grafana/dashboards/gc-overview.json` - Reverted to Kubernetes metrics
3. `/home/nathan/code/dark_tower/infra/grafana/dashboards/mc-overview.json` - Reverted to Kubernetes metrics
4. `/home/nathan/code/dark_tower/infra/grafana/dashboards/ac-logs.json` - Reverted pod terminology
5. `/home/nathan/code/dark_tower/infra/grafana/dashboards/gc-logs.json` - Reverted pod terminology
6. `/home/nathan/code/dark_tower/infra/grafana/dashboards/mc-logs.json` - Reverted pod terminology

---

## Files Deleted

1. `/home/nathan/code/dark_tower/infra/docker/loki/local-config.yaml`
2. `/home/nathan/code/dark_tower/infra/docker/promtail/config.yml`

---

## Files Reverted

1. `/home/nathan/code/dark_tower/docker-compose.yml` - Reverted to original (no cAdvisor/Loki/Promtail)
2. `/home/nathan/code/dark_tower/infra/docker/prometheus/prometheus.yml` - Reverted to original

---

## Deployment Instructions

To deploy the Kubernetes monitoring stack:

1. **Apply kube-state-metrics**:
   ```bash
   kubectl apply -f infra/kind/kubernetes/observability/kube-state-metrics.yaml
   ```

2. **Apply node-exporter**:
   ```bash
   kubectl apply -f infra/kind/kubernetes/observability/node-exporter.yaml
   ```

3. **Update Prometheus with new config**:
   ```bash
   kubectl apply -f infra/kind/kubernetes/observability/prometheus-config.yaml
   # Then restart Prometheus pods to pick up new config
   kubectl rollout restart deployment/prometheus -n dark-tower
   ```

4. **Verify metrics available**:
   ```bash
   # Port-forward to Prometheus
   kubectl port-forward svc/prometheus 9090:9090 -n dark-tower

   # Check targets in browser: http://localhost:9090/targets
   # Should see: kubernetes-cadvisor, kube-state-metrics, node-exporter as UP
   ```

5. **Test dashboard queries**:
   - Open Grafana: http://localhost:3000
   - Navigate to AC Overview dashboard
   - Verify Infrastructure panels show data

---

## Current Status

**All findings addressed for Kubernetes environment**:
- ✅ Finding 1: Kubernetes monitoring stack manifests created
- ✅ Finding 2: Dashboard queries corrected for Kubernetes
- ✅ Finding 3: Docker Compose files removed/reverted

**Ready for deployment** to KIND cluster after running kubectl apply commands.

**Note**: Loki/Promtail for Kubernetes not yet configured (out of scope for this iteration).
Log dashboards will work once Loki is deployed with proper Kubernetes configuration.
