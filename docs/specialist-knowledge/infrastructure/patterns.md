# Infrastructure Specialist - Patterns

Infrastructure patterns worth documenting for Dark Tower codebase.

---

## Pattern: CanaryPod for NetworkPolicy Testing
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Deploy minimal test pods to validate NetworkPolicy enforcement from within the cluster:

```rust
pub struct CanaryPod {
    name: String,
    namespace: String,
    cleaned_up: AtomicBool,
}

impl CanaryPod {
    pub async fn deploy(namespace: &str) -> Result<Self, CanaryError> {
        // Generate unique name: canary-{uuid8}
        // kubectl run with busybox:1.36, sleep 3600
        // Wait for pod Running status
    }

    pub async fn can_reach(&self, target_url: &str) -> bool {
        // kubectl exec -- wget --spider -T 5 <url>
    }
}
```

Key design decisions:
- **busybox:1.36**: Minimal image with wget for HTTP probes
- **sleep 3600**: Keep pod alive for testing duration (1hr max)
- **--restart=Never**: Create bare pod, not Deployment
- **AtomicBool cleanup tracking**: Prevent double-delete on Drop + explicit cleanup()

---

## Pattern: Idempotent Namespace Creation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Handle namespace creation race conditions in parallel test runs:

```rust
fn ensure_namespace(namespace: &str) -> Result<(), Error> {
    let check = Command::new("kubectl")
        .args(["get", "namespace", namespace])
        .output()?;

    if check.status.success() {
        return Ok(());
    }

    let create = Command::new("kubectl")
        .args(["create", "namespace", namespace])
        .output()?;

    if !create.status.success() {
        let stderr = String::from_utf8_lossy(&create.stderr);
        if !stderr.contains("already exists") {
            return Err(Error::NamespaceFailed(stderr.to_string()));
        }
    }
    Ok(())
}
```

This handles the race condition when parallel tests create the same namespace simultaneously.

---

## Pattern: Test Namespace Cleanup Guard
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Only cleanup namespaces that were created for testing:

```rust
fn cleanup_test_namespace(namespace: &str) {
    // Safety check: only delete test namespaces
    if !namespace.starts_with("canary-test") {
        return;
    }
    let _ = Command::new("kubectl")
        .args(["delete", "namespace", namespace, "--ignore-not-found=true"])
        .output();
}
```

Prevents accidental deletion of `dark-tower` or `kube-system` namespaces. Use distinctive prefix like `canary-test-` for all test-created namespaces.

---

## Pattern: Multi-Document Observability Manifests
**Added**: 2026-02-12
**Related files**: `infra/kubernetes/observability/`

Group related Kubernetes resources for each observability component in a single YAML file using `---` separators:

```yaml
# promtail-config.yaml
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: promtail-config
  namespace: dark-tower-observability
data:
  promtail.yaml: |
    # Promtail configuration...
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: promtail
  namespace: dark-tower-observability
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
# RBAC...
---
apiVersion: apps/v1
kind: DaemonSet
# Workload...
```

Benefits:
- **Single source of truth**: One file per component
- **Atomic deployment**: All resources deploy together
- **Easier review**: Related configs visible in one place
- **Kustomize-friendly**: Can overlay individual files

---

## Pattern: Guard-Driven Config Validation
**Added**: 2026-02-12
**Related files**: `scripts/guards/simple/grafana-datasources.sh`

Use guards to validate configuration consistency between related systems. Example: Ensure Grafana dashboard queries only use labels that Promtail actually exports to Loki.

```bash
# Extract valid labels from Promtail config
valid_loki_labels=$(python3 -c "
    # Parse ConfigMap -> promtail.yaml -> scrape_configs -> relabel_configs
    # Extract target_label values where action == 'replace'
")

# Parse dashboard LogQL queries
for dashboard in infra/grafana/dashboards/*.json; do
    loki_exprs=$(jq -r '.. | select(.datasource?.uid == "loki") | .expr' "$dashboard")
    used_labels=$(echo "$loki_exprs" | grep -oE '{[^}]+}' | extract_label_names)

    # Validate each used label exists in valid_loki_labels
done
```

This catches mismatches like using `job=` when Promtail exports `app=`.

---

## Pattern: Dynamic Config Parsing in Bash Guards
**Added**: 2026-02-13
**Related files**: `scripts/guards/simple/grafana-datasources.sh`

Extract valid configuration values dynamically from source configs using Python embedded in bash. This prevents hardcoded assumptions and ensures guards stay synchronized with actual config.

**Architecture**:
```bash
# Parse YAML config to extract valid labels
valid_labels=$(python3 -c "
import yaml, sys

with open('$CONFIG_FILE', 'r') as f:
    docs = list(yaml.safe_load_all(f.read()))

labels = set()
for doc in docs:
    if doc.get('kind') == 'ConfigMap':
        config = yaml.safe_load(doc['data']['config.yaml'])
        # Extract labels from relabel_configs
        for relabel in config['scrape_configs'][0]['relabel_configs']:
            if relabel.get('action') == 'replace':
                labels.add(relabel['target_label'])

        # Also extract from pipeline_stages
        for stage in config['scrape_configs'][0]['pipeline_stages']:
            if 'labels' in stage:
                labels.update(stage['labels'].keys())

for label in sorted(labels):
    print(label)
")

# Validate against extracted labels
for dashboard in dashboards/*.json; do
    used_labels=$(jq -r '.panels[].targets[].expr' "$dashboard" | extract_labels)
    for label in $used_labels; do
        if ! echo "$valid_labels" | grep -q "^${label}$"; then
            error "Dashboard uses invalid label: $label"
        fi
    done
done
```

**Key benefits**:
- **No hardcoding**: Labels extracted from actual config, not assumptions
- **Automatic updates**: Guard adapts when config changes
- **Multiple sources**: Can parse both relabel_configs AND pipeline_stages
- **Multi-document YAML**: Handles `---` separated Kubernetes manifests

**When to use**: Whenever validating that one config (dashboards) uses only values defined in another config (Promtail, Prometheus).
