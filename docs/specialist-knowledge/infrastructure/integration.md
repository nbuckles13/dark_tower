# Infrastructure Specialist - Integration Notes

Notes on working with other services and specialists in Dark Tower.

---

## Integration: env-tests Cluster Requirements
**Added**: 2026-01-13
**Updated**: 2026-02-11
**Related services**: env-tests, ac-service, gc-service, mc-service
**Coordination with**: Test Specialist, Security Specialist

The env-tests crate requires a running Kubernetes cluster with specific infrastructure. Prerequisites: Kind cluster (./infra/kind/scripts/setup.sh), services deployed in dark-tower namespace, port-forwards established (AC 8082, Prometheus 9090, Grafana 3000, Loki 3100), kubectl configured for target cluster. NetworkPolicy tests require AC NetworkPolicy deployed, ability to create/delete pods in arbitrary namespaces. Feature gates: smoke (cluster health 30s), flows (service flows 2-3min), observability (metrics/logs validation), resilience (NetworkPolicy, pod restart 5min+).

---

## Integration: Test Specialist - CanaryPod Design
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`
**Coordination with**: Test Specialist

CanaryPod API designed for Test Specialist to validate NetworkPolicy without deep Kubernetes knowledge. Simple case: CanaryPod::deploy(namespace), can_reach(url), cleanup(). Advanced case with custom labels for NetworkPolicy matching: CanaryConfig::builder().label(key, value).build(), deploy_with_config(namespace, config). All errors are CanaryError variants with descriptive messages.

---

## Integration: Security Specialist - NetworkPolicy Validation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`, `infra/services/ac-service/network-policy.yaml`
**Coordination with**: Security Specialist

NetworkPolicy tests validate security boundaries at the network layer (ADR-0012). Infrastructure validates: same-namespace pods CAN reach services (positive test), cross-namespace pods CANNOT reach services (negative test). Security validates: JWT validation, mTLS, rate limiting, authentication bypass. Defense-in-depth relationship: NetworkPolicy = Layer 3/4 (Infrastructure owns), Service authentication = Layer 7 (Security owns).

---

## Integration: Observability Specialist - Metrics Infrastructure
**Added**: 2026-01-13
**Updated**: 2026-02-11
**Related files**: `crates/env-tests/tests/30_observability.rs`, `infra/kind/scripts/setup.sh`, `infra/services/*/service-monitor.yaml`
**Coordination with**: Observability Specialist

Infrastructure provides: Prometheus deployed in dark-tower-observability namespace, ServiceMonitor CRDs for each service (commented out until /metrics implemented), Kubernetes service discovery with pod role and relabel_configs (filters by app label and port number), port-forward on 9090 for local access. Infrastructure validates: Prometheus reachable, scrape targets up, basic metric existence. Observability validates: metric semantics, label correctness, dashboard queries, SLO compliance.

---

## Integration: Operations Specialist - Local Development Workflow
**Added**: 2026-02-11
**Related files**: `infra/kind/scripts/iterate.sh`, `infra/kind/scripts/teardown.sh`
**Coordination with**: Operations Specialist

Infrastructure provides iterate.sh script for local development with Telepresence. Script scales down in-cluster service (kubectl scale --replicas=0), force-deletes pods for faster iteration, connects Telepresence to cluster, runs service locally with proper environment variables (DATABASE_URL, AC_MASTER_KEY, BIND_ADDRESS, RUST_LOG). Cleanup trap automatically scales service back up and leaves Telepresence intercept on exit. Operations owns runbooks for manual recovery if script fails.

---

## Integration: Database Specialist - PostgreSQL in Kind
**Added**: 2026-02-11
**Related files**: `infra/kind/scripts/setup.sh` (lines 154-240)
**Coordination with**: Database Specialist

Infrastructure provides: PostgreSQL 16-alpine StatefulSet with 1 replica, PVC for persistent storage, secret with credentials (POSTGRES_USER=darktower, POSTGRES_PASSWORD=dev_password_change_in_production, POSTGRES_DB=dark_tower), headless Service (clusterIP: None) for stable DNS, readinessProbe using pg_isready. Setup script runs migrations after deployment (sqlx migrate run via port-forward). Database specialist owns: schema design, migration content, query optimization, backup/restore procedures.

---

## Integration: Service Specialists - Manifest Ownership
**Added**: 2026-02-11
**Related files**: `infra/services/ac-service/`, `infra/services/gc-service/`, `infra/services/mc-service/`
**Coordination with**: AC, GC, MC service specialists

Infrastructure owns: K8s manifest structure (Deployment/StatefulSet, Service, ConfigMap, Secret templates, NetworkPolicy, PDB, ServiceMonitor), resource requests/limits, security contexts, probe configurations. Service specialists own: application configuration values (environment variables in ConfigMap), service-specific port numbers, health endpoint paths, replica counts (in coordination with Operations for capacity planning). Changes to manifests require coordination between Infrastructure and respective service specialist.

---

## Quick Reference: Which Specialist for What

| Concern | Primary Specialist | Infrastructure Role |
|---------|-------------------|---------------------|
| NetworkPolicy rules | Infrastructure | Owner |
| NetworkPolicy test code | Test | Owner |
| Security validation | Security | Owner |
| Prometheus deployment | Infrastructure | Owner |
| Metric semantics | Observability | Owner |
| Grafana dashboards | Observability | Owner |
| Pod cleanup runbook | Operations | Owner |
| kubectl access | Infrastructure | Owner |
| Manifest structure | Infrastructure | Owner |
| ConfigMap values | Service Specialists | Owner |
| Database schema | Database | Owner |
| PostgreSQL StatefulSet | Infrastructure | Owner |
