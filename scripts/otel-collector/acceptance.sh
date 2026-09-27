#!/usr/bin/env bash
# Collector metrics-pipeline acceptance harness (R-27).
#
# WHAT THIS IS FOR. The collector's client-metrics pipeline has properties that
# are VERSION-DEPENDENT and cannot be read off the config or the release notes:
# whether the pinned image compiles in `delta_to_cumulative` at all, whether it
# sums N same-identity delta writers, and whether it survives browser clock
# skew. Each of those was empirically FALSE at some point while this feature was
# built — the previously pinned 0.103.1 lacked the processor entirely, and on
# the current image the processor ALONE still dropped a second writer's first
# datapoint. So this is the pre-upgrade check: run it against a candidate image
# BEFORE bumping the tag in `deployment.yaml`.
#
# IT READS THE COMMITTED CONFIG. It copies
# `infra/services/otel-collector/collector.yaml` verbatim, so this exercises what
# actually ships — with ONE exception: the `stale` scenario shortens `max_stale`
# (see the note at that `sed`; the only permitted divergence). A harness carrying
# its own copy of the config would only prove something about the copy.
#
# It runs in a scratch namespace and deletes it on exit; it touches nothing the
# cluster serves.
#
# Usage:
#   scripts/otel-collector/acceptance.sh                    # committed image + config
#   scripts/otel-collector/acceptance.sh <image>            # candidate image
#   scripts/otel-collector/acceptance.sh <image> <scenario> [idle-seconds]
#
# Scenarios: all (default) | two | idle | skew | filter | sdk | stale
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
CONFIG="$REPO_ROOT/infra/services/otel-collector/collector.yaml"
DEPLOYMENT="$REPO_ROOT/infra/services/otel-collector/deployment.yaml"
HERE="$REPO_ROOT/scripts/otel-collector"
NS=otel-acceptance
export KUBECONFIG=${KUBECONFIG:-/tmp/devloop/kubeconfig}

IMAGE=${1:-}
SCEN=${2:-all}
IDLE=${3:-20}
if [[ -z "$IMAGE" ]]; then
  IMAGE=$(grep -oP '^\s*image:\s*\K\S+' "$DEPLOYMENT" | head -1)
fi
[[ -n "$IMAGE" ]] || { echo "could not determine collector image from $DEPLOYMENT"; exit 2; }

WORK=$(mktemp -d)
cleanup() {
  kill "${PF:-0}" 2>/dev/null || true
  kubectl delete ns "$NS" --wait=false >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# The committed collector config IS a plain file since ADR-0038 §2 (the ConfigMap is
# generated from it), so the harness copies it verbatim — no extraction step.
cp "$CONFIG" "$WORK/config.yaml"
[[ -s "$WORK/config.yaml" ]] || { echo "FAILED: $CONFIG is empty or unreadable"; exit 2; }

# The stale-reset scenario needs a short `max_stale` to be observable inside a
# test's lifetime. This is the ONLY permitted divergence from the committed
# config, and it is a scenario INPUT rather than a different pipeline: the
# relation being demonstrated (a forgotten stream restarts visibly instead of
# being summed through) is scale-free.
if [[ "$SCEN" == stale ]]; then
  sed -i 's/max_stale: .*/max_stale: 20s/' "$WORK/config.yaml"
fi

echo "image:  $IMAGE"
echo "config: $CONFIG (committed)"

kubectl delete ns "$NS" --ignore-not-found --wait=true >/dev/null
kubectl create ns "$NS" >/dev/null
kubectl -n "$NS" create configmap col --from-file=config.yaml="$WORK/config.yaml" >/dev/null
kubectl -n "$NS" apply -f - >/dev/null <<EOF
apiVersion: v1
kind: Pod
metadata: {name: col, labels: {app: col}}
spec:
  securityContext: {runAsNonRoot: true, runAsUser: 10001}
  containers:
  - name: col
    image: $IMAGE
    args: ["--config=/etc/col/config.yaml"]
    securityContext: {readOnlyRootFilesystem: true, allowPrivilegeEscalation: false, capabilities: {drop: [ALL]}}
    readinessProbe: {httpGet: {path: /, port: 13133}, periodSeconds: 2}
    volumeMounts: [{name: c, mountPath: /etc/col}, {name: tmp, mountPath: /tmp}]
  volumes: [{name: c, configMap: {name: col}}, {name: tmp, emptyDir: {}}]
EOF

if ! kubectl -n "$NS" wait pod/col --for=condition=Ready --timeout=300s >/dev/null; then
  echo "COLLECTOR DID NOT BECOME READY ($IMAGE)"
  echo "For a candidate image a MISSING PROCESSOR is the likeliest cause — run"
  echo "  kubectl run c --rm -it --restart=Never --image=$IMAGE -- components"
  echo "and compare the listed processors against the committed config."
  kubectl -n "$NS" logs col --tail=40 || true
  exit 2
fi
echo "imageID: $(kubectl -n "$NS" get pod col -o jsonpath='{.status.containerStatuses[0].imageID}')"

kubectl -n "$NS" port-forward pod/col 14318:4318 18889:8889 18888:8888 >/dev/null 2>&1 &
PF=$!
for _ in $(seq 40); do curl -s -o /dev/null http://127.0.0.1:18889/metrics && break; sleep 0.5; done

rc=0
if [[ "$SCEN" == sdk ]]; then
  # Real OTel JS MeterProviders built from sdk-core's OWN pinned packages, so the
  # wire shape is the SDK's rather than this harness's idea of it. Run from the
  # package dir so module resolution finds them.
  for mode in two idle restart cumulative; do
    (
      cd "$REPO_ROOT/packages/sdk-core"
      cp "$HERE/sdk_writers.mjs" ./.dt-sdk-writers.tmp.mjs
      node ./.dt-sdk-writers.tmp.mjs http://127.0.0.1:14318 http://127.0.0.1:18889 "$mode"
      r=$?
      rm -f ./.dt-sdk-writers.tmp.mjs
      exit $r
    ) || rc=$?
  done
else
  python3 "$HERE/acceptance_driver.py" \
    http://127.0.0.1:14318 http://127.0.0.1:18889 4 "$SCEN" "$IDLE" || rc=$?
fi

echo "--- collector self-telemetry (:8888)"
curl -s http://127.0.0.1:18888/metrics \
  | grep -E '^otelcol_(process_memory_rss|deltatocumulative|delta_to_cumulative|processor_(incoming|outgoing)_items|receiver_refused|processor_refused|exporter_send_failed)' \
  | cut -c1-240 || true
echo "--- collector warnings/errors"
kubectl -n "$NS" logs col | grep -Ei '"?(warn|error)' | cut -c1-260 | tail -12 || true
exit $rc
