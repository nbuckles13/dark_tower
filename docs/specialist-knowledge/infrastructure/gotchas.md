# Infrastructure Specialist - Gotchas

Mistakes to avoid, learned from experience in Dark Tower infrastructure work.

---

## Gotcha: NetworkPolicy Tests Require Matching Pod Labels
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`, `infra/services/ac-service/network-policy.yaml`

NetworkPolicy selects pods by label, not by namespace alone. A canary pod with app=canary label will be blocked by a NetworkPolicy that only allows app=global-controller in podSelector.matchLabels. Symptom: positive test fails (same-namespace connectivity blocked). Solution: configure canary pod labels to match allowed ingress. Lesson: review actual NetworkPolicy rules before writing connectivity tests.

---

## Gotcha: Redis Probes Expose Password in Process List
**Added**: 2026-01-31
**Related files**: `infra/services/redis/statefulset.yaml`

Using redis-cli -a $REDIS_PASSWORD in liveness/readiness probes exposes the password in the process list (visible via ps aux). This is a security issue even in development clusters. Symptom: password visible in process listings, security scanners flag credential exposure. Solution: use REDISCLI_AUTH environment variable instead. Redis-cli reads this automatically for authentication without exposing password in command line. Lesson: always prefer environment-based authentication over command-line flags for any probe or init container.

---

## Gotcha: UDP Services Require Explicit Protocol in K8s Service
**Added**: 2026-01-31
**Related files**: `infra/services/meeting-controller/service.yaml`

Kubernetes Services default to TCP protocol. For UDP-based services like WebTransport (QUIC), you must explicitly specify protocol: UDP in the Service port definition, or traffic will not route correctly. Symptom: UDP clients cannot connect via ClusterIP; works with hostNetwork but not via Service. Solution: explicitly declare UDP protocol in ports section. Lesson: when adding non-HTTP services, always verify the protocol field matches the actual transport.

---

## Gotcha: Calico Must Be Installed Before Nodes Become Ready
**Added**: 2026-02-11
**Related files**: `infra/kind/scripts/setup.sh`, `infra/kind/kind-config.yaml`

When using disableDefaultCNI: true in Kind cluster config, nodes will NOT become Ready until a CNI plugin is installed. Waiting for nodes Ready immediately after kind create cluster will timeout. Symptom: kubectl wait --for=condition=Ready nodes hangs indefinitely after cluster creation. Solution: install Calico CNI first (kubectl create -f calico.yaml), wait for Calico pods to be Ready, THEN wait for nodes Ready. Lesson: CNI must be installed before nodes can transition to Ready state.

---

## Gotcha: Kind Image Loading Differs for Podman vs Docker
**Added**: 2026-02-11
**Related files**: `infra/kind/scripts/setup.sh` (lines 864-873)

kind load docker-image has issues with Podman runtime. Images built with Podman may not load correctly using the standard kind load docker-image command. Symptom: image not found errors in cluster even after kind load appears successful. Solution: use save/load workaround for Podman - save image to tar file (podman save -o tmpfile), load archive (kind load image-archive tmpfile), then delete tmpfile. For Docker, use standard kind load docker-image. Lesson: detect container runtime and use appropriate image loading method.

---

## Gotcha: Port-Forward Conflicts Require Cleanup
**Added**: 2026-02-11
**Related files**: `infra/kind/scripts/iterate.sh`

When running local development with Telepresence or direct port-forwards, old port-forward processes may still be running on the same port, causing "address already in use" errors. Symptom: cannot bind to port 8082, 8080, etc. when starting local service. Solution: kill existing port-forwards before starting new ones (pkill -f "kubectl port-forward" or fuser -k PORT/tcp). Lesson: always cleanup port-forwards before establishing new ones in development scripts.

---

## Gotcha: Service Readiness Probes Must Match Actual Endpoints
**Added**: 2026-02-11
**Related files**: `infra/services/global-controller/deployment.yaml`

Kubernetes readiness probes fail if the path/port doesn't exist, preventing pod from receiving traffic. Symptom: pods Running but not Ready, service has no endpoints. Solution: ensure readinessProbe httpGet path matches actual health endpoint (e.g., /health on correct port), and service starts health endpoint before probe initialDelaySeconds expires. Lesson: verify probe configuration matches actual application endpoints, use adequate initialDelaySeconds to allow service startup.
