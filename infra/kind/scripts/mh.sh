#!/usr/bin/env bash
# mh.sh — the Kind MH pool tool (ADR-0039 §7 "On demand" / "Callable by automated tests").
#
#   mh.sh list    [--json]
#   mh.sh up      [--slot K] [--owner ID] [--timeout S] [--json]
#   mh.sh drain   K | --all --owner ID  [--owner ID] [--no-wait] [--timeout S] [--force] [--json]
#   mh.sh upgrade [--timeout S] [--json]
#
# One script, used by humans, env-tests (crates/env-tests/src/fixtures/mh_pool.rs) and
# deploy.sh, to provision, drain, upgrade and list MH incarnations on hostPort slots.
# THE MACHINE CONTRACT is docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt;
# this header restates the parts a caller branches on. Self-test: infra/kind/scripts/mh.test.sh
# (Layer 3), which pins this script's token array == the contract's §3 list in both directions.
#
# INVOCATION. Host or devloop container, with the caller's kubeconfig. The cluster is ALWAYS
# `kind-<DT_CLUSTER_NAME>` (lib/common.sh dt_init_cluster_env), never the current context:
# every kubectl call goes through kc(), which passes --context. Dependencies: kubectl + jq.
# mh.sh never talks to GC or its database, and is Prometheus-free.
#
# STREAMS. stdout: exactly one JSON object, and only with --json (every object carries "ok";
# a failure prints {"ok":false,"reason","slot","detail"}). stderr: human text, then exactly ONE
# final line, on every exit path (including signals and unexpected errors):
#     MH_RESULT STATUS=OK|FAIL REASON=<token> [SLOT=<k>] [DETAIL=<text>]
# DETAIL is always the LAST field and runs to end of line (it may contain spaces); it is built
# only from validated values, never from raw kubectl output.
#
# EXIT CODES + TOKENS: the MH_TOKENS array below is their ONE home (exit codes are looked up
# from it, never typed at a call site). STATUS=OK always carries REASON=ok.
#   not-available-timeout  `up`: the Deployment never went Available within the derived bound.
#                          The pod is LEFT IN PLACE for diagnosis; DETAIL carries the drain command.
#   mh-rollout-failed      `upgrade`: a replacement never went Available. The old MH is untouched
#                          and still serving; DETAIL names both slots.
#   drain-timeout          the pod/Deployment were not gone within the pod's own
#                          terminationGracePeriodSeconds + MH_DRAIN_MARGIN_S. No escalation.
#   kubectl-failed         an unexpected API/kubectl error after startup (never masquerades as
#                          a rollout or timeout), or a malformed value read back from the cluster.
#   internal-error         the EXIT-trap fallback: a signal, or an exit that bypassed the emitter.
#   usage / no-kubeconfig / no-template / prerequisite-missing   preconditions (exit 2).
#   mh-no-free-slot        capacity (exit 3).
#   last-serving-mh / stuck-terminating / foreign-owner          refusals (exit 4).
#   mh-pool-busy           the mh-pool Lease is held (exit 5), DETAIL=holder=<identity>.
#
# CLUSTER-SIDE INTERFACE (produced by deploy.sh, story 4 task I-2; mh.sh only consumes it):
#   - ConfigMap `mh-template` in namespace dark-tower, data keys:
#       deployment.json  the fully rendered MH Deployment, as JSON (hash-named ConfigMap refs
#                        and the content-tagged image already resolved by the root render);
#       revision         the template revision (hash; a label value);
#       baseline         B, the baseline MH count (integer).
#     The Deployment must have a non-empty spec.selector.matchLabels that its pod-template
#     labels contain, a container port named `webtransport`, and on that container an env
#     MH_WEBTRANSPORT_ADVERTISE_ADDRESS with valueFrom.configMapKeyRef. Else: no-template.
#   - Per-slot ConfigMaps labelled dark-tower.io/mh-slot=<k> (k a decimal integer), keys
#     MH_WEBTRANSPORT_ADVERTISE_ADDRESS and MH_HOST_PORT. Found BY LABEL only (no name list,
#     no count: K = the discovered set). Exactly one ConfigMap per slot value; a duplicate
#     (e.g. a previous hash-named generation left labelled) is no-template.
#   mh.sh instantiates the template as Deployment mh-s<k> (replicas 1) and sets exactly:
#     - labels dark-tower.io/mh-slot=<k>, dark-tower.io/mh-revision=<rev>,
#       dark-tower.io/mh-owner=<owner>, MERGED into Deployment and pod-template labels (every
#       template label survives; the selector gains only mh-slot);
#     - the webtransport container port's hostPort = the slot's MH_HOST_PORT;
#     - the MH_WEBTRANSPORT_ADVERTISE_ADDRESS configMapKeyRef name = slot k's ConfigMap;
#     - annotation dark-tower.io/mh-replaces=<old slot>, during upgrade only.
#   It never computes an address: addresses come only from the slot ConfigMaps.
#
# SLOT STATES (list/up/drain/upgrade all read the ONE jq snapshot, mh_snapshot):
#   free         no Deployment and no pod;
#   starting     live Deployment, pod absent or not Ready;
#   serving      live Deployment, pod Ready, nothing deleting;
#   draining     deletion requested, the pod exists and is within its grace (SIGTERM delivered
#                or imminent; MH is draining its meetings);
#   terminating  deletion requested and the pod is past its grace (terminating_past_grace=true;
#                pod.deletionTimestamp is request time + grace), or the pod is gone and the
#                Deployment is still finalizing.
#
# OWNERS: `baseline` (deploy.sh), `manual` (default), or the Layer-7 run token; one valid shape,
# MH_OWNER_RE (a subset of the label-value rule). `drain` refuses another owner's MH
# (foreign-owner) unless --force. `upgrade` is OWNER-PRESERVING across ALL owners, by design:
# each replacement inherits the old MH's owner, so it skips the owner check.
#
# --force overrides ONLY last-serving-mh and foreign-owner. It never overrides stuck-terminating
# ("investigate, don't force") and never reaches kubectl. mh.sh NEVER deletes a pod and never
# passes --force/--grace-period to kubectl: `delete pod --force --grace-period=0` is the one way
# to defeat the hostPort slot lock. The only delete it issues is a foreground Deployment delete.
#
# LOCK: a coordination.k8s.io Lease `mh-pool` (namespace dark-tower) serialises ONLY the
# decide-plus-create/delete step of up, drain and each upgrade step; the Ready/gone waits run
# outside it, and `list` takes none. holderIdentity = <owner>@<host>:<pid>, unique per
# invocation; leaseDurationSeconds = MH_LEASE_S. Acquire = atomic create; an existing Lease is
# reclaimed ONLY when released (no holder) or expired (renewTime + duration < host now), via a
# replace carrying the observed resourceVersion (a Conflict is BUSY). Release (every exit path)
# clears the holder only if it is still ours, again by resourceVersion; it never deletes.
# RESIDUAL RISK (clock skew): expiry is judged on the HOST clock. A host clock running more than
# MH_LEASE_S ahead of the writer's (WSL/podman after suspend) can reclaim a LIVE Lease and reopen
# the concurrent-drain window that could remove the last serving MH (up is still backstopped by
# AlreadyExists + the hostPort; drain is not). Mitigations: every reclaim warns loudly with the
# prior holder and age, and a renewTime more than MH_LEASE_FUTURE_SKEW_S in the future of host
# now is treated as BUSY (never reclaimed) with a skew warning.
#
# CONFIG (environment; validated, a bad value is `usage`):
#   MH_POLL_INTERVAL_S  poll interval for the waits (default 2; seconds, may be fractional);
#   MH_UP_MARGIN_S      added to the probe-derived `up` bound (default 30; integer >= 0);
#   MH_DRAIN_MARGIN_S   added to the pod's grace for the drain bound (default 30; integer >= 0);
#   MH_KUBECTL_TIMEOUT_S  --request-timeout on every kubectl call (default 5; 1..MH_LEASE_S-1).
# LEASE INVARIANT: no mutation is issued unless (time since our acquire call) + MH_KUBECTL_TIMEOUT_S
# < MH_LEASE_S (lease_guard), so a slow apiserver can never let the exclusion lapse silently: it
# fails kubectl-failed DETAIL=lease-overrun with nothing mutated.
# The default `up` bound is max over the webtransport container's startup/readiness/liveness
# probes of (initialDelaySeconds + periodSeconds x failureThreshold + timeoutSeconds), with
# Kubernetes' probe defaults, plus spec.minReadySeconds (Available needs Ready that long), plus
# MH_UP_MARGIN_S. Scheduling and image pull are NOT in the derivation: on Kind the image is
# preloaded (imagePullPolicy Never) and scheduling is immediate; MH_UP_MARGIN_S is their slack. --timeout S (1..MH_TIMEOUT_MAX_S) overrides.
#
# SOURCEABLE: sourcing this file defines constants and functions only (no trap, no cluster
# call, no temp file); main runs only when executed. mh.test.sh and T-1 read MH_TOKENS and
# MH_OWNER_RE from it.

# shellcheck source=lib/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

# --- Constants ----------------------------------------------------------------

readonly MH_NAMESPACE="dark-tower"
readonly MH_TEMPLATE_CM="mh-template"
readonly MH_LEASE_NAME="mh-pool"
readonly MH_LABEL_SLOT="dark-tower.io/mh-slot"
readonly MH_LABEL_REVISION="dark-tower.io/mh-revision"
readonly MH_LABEL_OWNER="dark-tower.io/mh-owner"
readonly MH_ANNOTATION_REPLACES="dark-tower.io/mh-replaces"
readonly MH_ADDRESS_KEY="MH_WEBTRANSPORT_ADVERTISE_ADDRESS"
readonly MH_HOST_PORT_KEY="MH_HOST_PORT"
readonly MH_WT_PORT_NAME="webtransport"
readonly MH_SCHEMA_VERSION=1
readonly MH_DEFAULT_OWNER="manual"
readonly MH_LEASE_S=30
readonly MH_LEASE_FUTURE_SKEW_S=5
readonly MH_TIMEOUT_MAX_S=3600
readonly MH_KUBECTL_TIMEOUT_DEFAULT_S=5

# THE one definition of a valid owner (a subset of the K8s label-value rule). It is also a
# selector-injection control: an owner is compared, never spliced into a `-l` selector.
# ANCHOR: the org-subdomain rule (migrations/20250118000001_initial_schema.sql subdomain_format),
# because Layer 7's owner token is the per-run org subdomain. Drift-guarded by
# scripts/guards/simple/validate-subdomain-regex-sync.sh.
readonly MH_OWNER_RE='^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'
readonly MH_SLOT_RE='^(0|[1-9][0-9]{0,3})$'
# The Lease holder is <owner>@<host>:<pid>; its owner part is DERIVED from MH_OWNER_RE.
readonly MH_HOLDER_RE="${MH_OWNER_RE%\$}@[A-Za-z0-9.-]{1,63}:[0-9]{1,10}\$"
readonly MH_LABEL_VALUE_RE='^[A-Za-z0-9]([A-Za-z0-9._-]{0,61}[A-Za-z0-9])?$'
readonly MH_POD_NAME_RE='^[a-z0-9]([a-z0-9.-]{0,251}[a-z0-9])?$'
# The per-slot Deployment is <prefix><k> (mh-s<k>); the ONE spelling of the prefix.
readonly MH_DEPLOY_PREFIX="mh-s"
# The ONE jq parser for API timestamps (Lease MicroTime and object timestamps).
readonly MH_JQ_TS='def ts: sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601;'

# THE token list: "token:exit-code", one per line. Its ONE home; mh-sh-contract.txt §3 is pinned
# to it (both directions) by mh.test.sh.
readonly -a MH_TOKENS=(
    "ok:0"
    "mh-rollout-failed:1"
    "drain-timeout:1"
    "not-available-timeout:1"
    "kubectl-failed:1"
    "internal-error:1"
    "usage:2"
    "no-kubeconfig:2"
    "no-template:2"
    "prerequisite-missing:2"
    "mh-no-free-slot:3"
    "last-serving-mh:4"
    "stuck-terminating:4"
    "foreign-owner:4"
    "mh-pool-busy:5"
)

# --- Run state (set by main) ---------------------------------------------------

MH_JSON=0
MH_TMP=""
MH_DONE=0
MH_OUT_JSON=""        # the success object (built with jq), printed by mh_finish
MH_FAIL_EXTRA="{}"    # extra keys merged into a failure object (e.g. drained:[...])
MH_LEASE_HELD=0
MH_LEASE_T0=0
MH_HOLDER=""
MH_VERB=""
MH_CREATED_SLOT=""    # set once create_mh succeeds, for the fallback's reclaim hint
MH_CREATED_OWNER=""
MH_FAIL_SUFFIX=""     # appended to a failure DETAIL (upgrade: steps done so far)
MH_OWNER=""
ARG_SLOT=""
ARG_TIMEOUT=""
ARG_ALL=0
ARG_NOWAIT=0
ARG_FORCE=0
UPGRADE_NEW=""
UPGRADE_OWNER=""

# --- Output ----------------------------------------------------------------------

mh_log()  { printf '[mh.sh] %s\n' "$*" >&2; }
mh_warn() { printf '[mh.sh] WARN: %s\n' "$*" >&2; }

# Exit code for a token, from MH_TOKENS only. Unknown token: rc 1, prints nothing.
mh_token_exit() {
    local entry
    for entry in "${MH_TOKENS[@]}"; do
        if [[ "${entry%%:*}" == "$1" ]]; then printf '%s' "${entry##*:}"; return 0; fi
    done
    return 1
}

# THE ONE emitter. mh_finish <token> [slot] [detail]. Order is load-bearing: stdout JSON, then
# Lease release + temp cleanup (whose warnings go to stderr), and only then the MH_RESULT line,
# so MH_RESULT is always the LAST stderr line. An unknown token is itself a bug: internal-error.
mh_finish() {
    # Once finishing has started it must complete: a signal now (e.g. during the Lease release's
    # kubectl calls) would otherwise re-enter and print a second, contradicting result.
    trap '' INT TERM
    local token="$1" slot="${2:-}" detail="${3:-}" code status
    if ! code="$(mh_token_exit "$token")"; then
        detail="unknown-token"
        token="internal-error"
        code="$(mh_token_exit "$token")"
    fi
    [[ "$code" != "0" && -n "$MH_FAIL_SUFFIX" ]] && detail+="${detail:+; }${MH_FAIL_SUFFIX}"
    detail="${detail//$'\n'/ }"
    detail="${detail//$'\r'/ }"
    [[ "$slot" =~ $MH_SLOT_RE ]] || slot=""
    status="FAIL"; [[ "$code" == "0" ]] && status="OK"
    MH_DONE=1
    if (( MH_JSON )); then
        if [[ "$status" == "OK" && -n "$MH_OUT_JSON" ]]; then
            printf '%s\n' "$MH_OUT_JSON"
        elif [[ "$status" == "OK" ]]; then
            jq -cn '{ok:true}'
        else
            jq -cn --arg r "$token" --arg s "$slot" --arg d "$detail" --argjson x "$MH_FAIL_EXTRA" \
                '{ok:false, reason:$r, slot:(if $s=="" then null else ($s|tonumber) end),
                  detail:(if $d=="" then null else $d end)} + $x'
        fi
    fi
    mh_lease_release
    [[ -n "$MH_TMP" && -d "$MH_TMP" ]] && rm -rf "$MH_TMP"
    local line="MH_RESULT STATUS=${status} REASON=${token}"
    [[ -n "$slot" ]] && line+=" SLOT=${slot}"
    [[ -n "$detail" ]] && line+=" DETAIL=${detail}"
    printf '%s\n' "$line" >&2
    exit "$code"
}

# EXIT-trap fallback: an exit that bypassed mh_finish still ends with one MH_RESULT line.
# Both handlers first restore the ORIGINAL stdout/stderr (saved as fds 3/4 in main): a signal
# can land while a function runs under a redirection (kc_call's `>out 2>err`), and the result
# line must not vanish into a temp file.
mh_on_exit() {
    local rc=$?
    (( MH_DONE )) && return 0
    exec 1>&3 2>&4
    mh_fallback "unexpected exit-path rc=${rc}"
}
mh_on_signal() {
    exec 1>&3 2>&4
    mh_fallback "signal=$1"
}
# The fallback names what may have been left behind: a created MH is never silently orphaned.
mh_fallback() {
    if [[ -n "$MH_CREATED_SLOT" && "$MH_VERB" == "upgrade" ]]; then
        mh_finish internal-error "$MH_CREATED_SLOT" "$1; $(mh_dep_name "$MH_CREATED_SLOT") may be left in place; rerun mh.sh upgrade to resume"
    elif [[ -n "$MH_CREATED_SLOT" ]]; then
        mh_finish internal-error "$MH_CREATED_SLOT" "$1; $(mh_dep_name "$MH_CREATED_SLOT") may be left in place; reclaim with: mh.sh drain ${MH_CREATED_SLOT} --owner ${MH_CREATED_OWNER}"
    fi
    mh_finish internal-error "" "$1"
}

usage_text() {
    sed -n '2,7p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2
}

# --- kubectl -----------------------------------------------------------------------

# THE kubectl wrapper: every call is pinned to the cluster context and namespace. KUBE_CONTEXT
# is dt_init_cluster_env's ONE encoding of `kind-<cluster>`; ${KUBECTL} is not used because it is
# a space-joined string.
mh_dep_name() { printf '%s%s' "$MH_DEPLOY_PREFIX" "$1"; }

# --request-timeout bounds every call, so a hung apiserver cannot stretch the Lease's critical
# section unnoticed (see lease_guard).
kc() { kubectl --context "${KUBE_CONTEXT}" -n "${MH_NAMESPACE}" --request-timeout="${MH_KUBECTL_TIMEOUT_S}s" "$@"; }

# kc_call <op-name> <args...>: stdout -> $MH_TMP/out, stderr -> $MH_TMP/err; returns kubectl's rc.
kc_call() {
    local op="$1"; shift
    kc "$@" >"$MH_TMP/out" 2>"$MH_TMP/err"
    local rc=$?
    (( rc == 0 )) || printf '%s' "$op" >"$MH_TMP/failed-op"
    return "$rc"
}

# A failed kubectl call that has no expected-failure meaning: relay kubectl's own words as
# human lines (bounded), then kubectl-failed with only the op name in DETAIL.
kc_fail() {
    local op
    op="$(cat "$MH_TMP/failed-op" 2>/dev/null)"
    mh_warn "kubectl ${op:-call} failed:"
    head -n 5 "$MH_TMP/err" | sed 's/^/[mh.sh]   kubectl: /' >&2
    mh_finish kubectl-failed "${1:-}" "op=${op:-unknown}"
}

# kc_must <op> <args...>: the call must succeed (else kubectl-failed); stdout stays in $MH_TMP/out.
kc_must() { kc_call "$@" || kc_fail; }

err_has() { grep -qF "$1" "$MH_TMP/err"; }

# --- Validation ---------------------------------------------------------------------

is_uint() { [[ "$1" =~ ^[0-9]+$ ]]; }

validate_config() {
    MH_POLL_INTERVAL_S="${MH_POLL_INTERVAL_S:-2}"
    MH_UP_MARGIN_S="${MH_UP_MARGIN_S:-30}"
    MH_DRAIN_MARGIN_S="${MH_DRAIN_MARGIN_S:-30}"
    [[ "$MH_POLL_INTERVAL_S" =~ ^[0-9]{1,3}(\.[0-9]{1,3})?$ && ! "$MH_POLL_INTERVAL_S" =~ ^0+(\.0+)?$ ]] \
        || mh_finish usage "" "MH_POLL_INTERVAL_S must be a number of seconds > 0"
    is_uint "$MH_UP_MARGIN_S" && (( ${#MH_UP_MARGIN_S} <= 4 )) \
        || mh_finish usage "" "MH_UP_MARGIN_S must be an integer >= 0"
    is_uint "$MH_DRAIN_MARGIN_S" && (( ${#MH_DRAIN_MARGIN_S} <= 4 )) \
        || mh_finish usage "" "MH_DRAIN_MARGIN_S must be an integer >= 0"
    MH_KUBECTL_TIMEOUT_S="${MH_KUBECTL_TIMEOUT_S:-$MH_KUBECTL_TIMEOUT_DEFAULT_S}"
    [[ "$MH_KUBECTL_TIMEOUT_S" =~ ^[1-9][0-9]?$ ]] && (( MH_KUBECTL_TIMEOUT_S < MH_LEASE_S )) \
        || mh_finish usage "" "MH_KUBECTL_TIMEOUT_S must be an integer in 1..$(( MH_LEASE_S - 1 ))"
}

validate_owner() {
    [[ "$1" =~ $MH_OWNER_RE ]] || mh_finish usage "" "--owner must match ${MH_OWNER_RE}"
}

validate_timeout() {
    [[ "$1" =~ ^[1-9][0-9]{0,3}$ ]] && (( $1 <= MH_TIMEOUT_MAX_S )) \
        || mh_finish usage "" "--timeout must be an integer in 1..${MH_TIMEOUT_MAX_S}"
}

validate_slot_arg() {
    [[ "$1" =~ $MH_SLOT_RE ]] || mh_finish usage "" "slot must be a decimal integer"
}

# After the slot ConfigMaps are loaded: the slot must be one of them.
require_known_slot() {
    jq -e --argjson k "$1" 'any(.[]; .slot == $k)' "$MH_TMP/slots.json" >/dev/null \
        || mh_finish usage "$1" "slot $1 is not in the discovered slot set $(jq -c '[.[].slot]' "$MH_TMP/slots.json")"
}

# --- Preflight + interface load -------------------------------------------------------

preflight() {
    local tool
    for tool in kubectl jq; do
        command -v "$tool" >/dev/null 2>&1 || mh_finish prerequisite-missing "" "missing=${tool}"
    done
    # dt_init_cluster_env is called TWICE on purpose: it `exit 1`s with its own text on a bad
    # DT_CLUSTER_NAME / DT_PORT_MAP / DT_HOST_GATEWAY_IP, and exit 1 is FAIL in this contract.
    # The subshell probe maps that to no-kubeconfig (exit 2) with one MH_RESULT line; the real
    # call then sets KUBE_CONTEXT in this shell. Do not "simplify" this to one call.
    if ! ( dt_init_cluster_env ) >/dev/null 2>"$MH_TMP/env-err"; then
        sed 's/^/[mh.sh]   /' "$MH_TMP/env-err" >&2
        mh_finish no-kubeconfig "" "cluster environment invalid (DT_CLUSTER_NAME/DT_PORT_MAP/DT_HOST_GATEWAY_IP)"
    fi
    dt_init_cluster_env >&2
    if ! kc_call config-get-contexts config get-contexts -o name \
        || ! grep -qxF "${KUBE_CONTEXT}" "$MH_TMP/out"; then
        mh_finish no-kubeconfig "" "context ${KUBE_CONTEXT} not in the kubeconfig"
    fi
}

# Load + validate mh-template into $MH_TMP/tpl.json and the slot set into $MH_TMP/slots.json.
load_interface() {
    if ! kc_call get-template get configmap "$MH_TEMPLATE_CM" -o json; then
        if err_has "(NotFound)"; then
            mh_finish no-template "" "ConfigMap ${MH_TEMPLATE_CM} not found in ${MH_NAMESPACE} (deploy.sh produces it)"
        fi
        head -n 3 "$MH_TMP/err" | sed 's/^/[mh.sh]   kubectl: /' >&2
        mh_finish no-kubeconfig "" "apiserver unreachable or refused via ${KUBE_CONTEXT}"
    fi
    local verdict
    verdict="$(jq -c --arg port "$MH_WT_PORT_NAME" --arg env "$MH_ADDRESS_KEY" --arg lre "$MH_LABEL_VALUE_RE" "$(template_jq)" "$MH_TMP/out" 2>/dev/null)" || verdict='{"error":"template is not valid JSON"}'
    if [[ "$(jq -r '.error // empty' <<<"$verdict")" != "" ]]; then
        mh_finish no-template "" "$(jq -r '.error' <<<"$verdict")"
    fi
    printf '%s' "$verdict" >"$MH_TMP/tpl.json"

    kc_must get-slot-configmaps get configmaps -l "$MH_LABEL_SLOT" -o json
    verdict="$(jq -c --arg ls "$MH_LABEL_SLOT" --arg ak "$MH_ADDRESS_KEY" --arg pk "$MH_HOST_PORT_KEY" \
        --arg sre "$MH_SLOT_RE" "$(slots_jq)" "$MH_TMP/out" 2>/dev/null)" \
        || verdict='{"error":"slot ConfigMap list is not valid JSON"}'
    if [[ "$(jq -r 'if type=="object" then .error // empty else empty end' <<<"$verdict")" != "" ]]; then
        mh_finish no-template "" "$(jq -r '.error' <<<"$verdict")"
    fi
    printf '%s' "$verdict" >"$MH_TMP/slots.json"
}

# jq: the template ConfigMap -> {revision, baseline, deployment, cname, grace, probe_window}
# or {error}. Probe defaults are Kubernetes' (period 10, failureThreshold 3, timeout 1).
template_jq() {
    cat <<'JQ'
def err($m): {error: $m};
(.data // {}) as $d
| if ($d.revision // "") == "" then err("mh-template has no revision")
  elif (($d.revision | test($lre)) | not) then err("mh-template revision is not a label value")
  elif (($d.baseline // "") | test("^[0-9]{1,3}$") | not) then err("mh-template baseline is not an integer")
  elif ($d["deployment.json"] // "") == "" then err("mh-template has no deployment.json")
  else
    (try ($d["deployment.json"] | fromjson) catch null) as $t
    | if ($t | type) != "object" or $t.kind != "Deployment" then err("deployment.json is not a Deployment")
      else
        ($t.spec.selector.matchLabels // {}) as $sel
        | ($t.spec.template.metadata.labels // {}) as $pl
        | ([$t.spec.template.spec.containers[]? | select(any(.ports[]?; .name == $port))] | .[0]) as $c
        | if ($sel | length) == 0 then err("template has no spec.selector.matchLabels")
          elif ([$sel | to_entries[] | select($pl[.key] != .value)] | length) > 0 then err("template pod labels do not contain its selector labels")
          elif $c == null then err("template has no container port named " + $port)
          elif ([$c.env[]? | select(.name == $env and .valueFrom.configMapKeyRef != null)] | length) == 0 then err("template webtransport container has no " + $env + " configMapKeyRef")
          else
            {revision: $d.revision, baseline: ($d.baseline | tonumber), deployment: $t, cname: $c.name,
             grace: ($t.spec.template.spec.terminationGracePeriodSeconds // 30),
             probe_window: (([$c.startupProbe, $c.readinessProbe, $c.livenessProbe | select(. != null)
                             | (.initialDelaySeconds // 0) + (.periodSeconds // 10) * (.failureThreshold // 3) + (.timeoutSeconds // 1)]
                            | max // 0) + ($t.spec.minReadySeconds // 0))}
          end
      end
  end
JQ
}

# jq: the slot ConfigMap list -> sorted [{slot, cm, host_port, address}] or {error}.
slots_jq() {
    cat <<'JQ'
def err($m): {error: $m};
[.items[]? | {lbl: (.metadata.labels[$ls] // ""), cm: .metadata.name, data: (.data // {})}] as $raw
| if ($raw | length) == 0 then err("no ConfigMaps labelled " + $ls)
  elif any($raw[]; .lbl | test($sre) | not) then err("a slot ConfigMap label is not a decimal slot number")
  elif ($raw | group_by(.lbl) | any(.[]; length > 1)) then err("duplicate-slot-configmap: more than one ConfigMap for one slot")
  elif any($raw[]; (.data[$pk] // "") | test("^[1-9][0-9]{0,4}$") | not) then err("a slot ConfigMap has no valid " + $pk)
  elif any($raw[]; (.data[$ak] // "") | test("^[^\\s]{1,253}$") | not) then err("a slot ConfigMap has no valid " + $ak)
  else [$raw[] | {slot: (.lbl | tonumber), cm, host_port: (.data[$pk] | tonumber), address: .data[$ak]}] | sort_by(.slot)
  end
JQ
}

# --- The snapshot (THE one state derivation) ----------------------------------------------

# mh_snapshot: fresh Deployments + pods -> $MH_TMP/snap.json:
#   {template_revision, baseline, slots:[entry + internal _fields], malformed:[...]}
mh_snapshot() {
    kc_must get-deployments get deployments -l "$MH_LABEL_SLOT" -o json
    mv "$MH_TMP/out" "$MH_TMP/deps.json"
    kc_must get-pods get pods -l "$MH_LABEL_SLOT" -o json
    mv "$MH_TMP/out" "$MH_TMP/pods.json"
    if ! jq -n --slurpfile tpl "$MH_TMP/tpl.json" --slurpfile slots "$MH_TMP/slots.json" \
        --slurpfile deps "$MH_TMP/deps.json" --slurpfile pods "$MH_TMP/pods.json" \
        --argjson now "$(date -u +%s)" --arg ls "$MH_LABEL_SLOT" --arg lr "$MH_LABEL_REVISION" \
        --arg lo "$MH_LABEL_OWNER" --arg ar "$MH_ANNOTATION_REPLACES" --arg ore "$MH_OWNER_RE" \
        --arg sre "$MH_SLOT_RE" --arg lre "$MH_LABEL_VALUE_RE" --arg pre "$MH_POD_NAME_RE" \
        --arg dp "$MH_DEPLOY_PREFIX" "${MH_JQ_TS} $(snapshot_jq)" >"$MH_TMP/snap.json" 2>"$MH_TMP/err"; then
        printf 'snapshot' >"$MH_TMP/failed-op"
        kc_fail
    fi
    local bad
    bad="$(jq -r '.malformed | join(",")' "$MH_TMP/snap.json")"
    [[ -z "$bad" ]] || mh_finish kubectl-failed "" "malformed cluster value(s): ${bad}"
}

snapshot_jq() {
    cat <<'JQ'
def lbl($k): (.metadata.labels // {})[$k];
def isready: any((.status.conditions // [])[]; .type == "Ready" and .status == "True");
$tpl[0] as $t | $deps[0].items as $D | $pods[0].items as $P
| [ $slots[0][] as $s | ($s.slot | tostring) as $k
    | ([$D[] | select(lbl($ls) == $k)] | sort_by(.metadata.name != ($dp + $k)) | .[0]) as $d
    | ([$P[] | select(lbl($ls) == $k)] | sort_by(.metadata.creationTimestamp // "")) as $ps
    | $ps[0] as $p
    | [$ps[] | select(.metadata.deletionTimestamp != null and ($now > (.metadata.deletionTimestamp | ts)))] as $past
    | ($d != null and $d.metadata.deletionTimestamp != null) as $ddel
    | ($p != null and $p.metadata.deletionTimestamp != null) as $pdel
    | (($d | lbl($lr)) // ($p | lbl($lr))) as $rev
    | { slot: $s.slot, host_port: $s.host_port, address: $s.address,
        pod_ip: ($p.status.podIP // null), pod_uid: ($p.metadata.uid // null),
        state: (if $d == null and $p == null then "free"
                elif $ddel or $pdel or $d == null then
                  (if $p == null or ($past | length) > 0 then "terminating" else "draining" end)
                elif ($p != null and ($p | isready)) then "serving"
                else "starting" end),
        terminating_past_grace: (($past | length) > 0),
        deployment: ($d.metadata.name // null), pod: ($p.metadata.name // null),
        revision: $rev, stale: ($rev != null and $rev != $t.revision),
        owner: (($d | lbl($lo)) // ($p | lbl($lo))),
        restart_count: (if $p == null then null
                        else ([($p.status.containerStatuses // [])[] | select(.name == $t.cname) | .restartCount] | .[0]) end),
        _replaces: ((($d.metadata.annotations // {})[$ar]) // null),
        _grace: ($p.spec.terminationGracePeriodSeconds // $d.spec.template.spec.terminationGracePeriodSeconds // $t.grace),
        _stuck_pod: ($past[0].metadata.name // null),
        _signalled: (($ps | length) == 0 or all($ps[]; .metadata.deletionTimestamp != null)) } ] as $entries
| { template_revision: $t.revision, baseline: $t.baseline, slots: $entries,
    malformed: ([ $entries[]
      | (select(.owner != null and (.owner | test($ore) | not)) | "owner@slot\(.slot)"),
        (select(.revision != null and (.revision | test($lre) | not)) | "revision@slot\(.slot)"),
        (select(._replaces != null and (._replaces | test($sre) | not)) | "replaces@slot\(.slot)"),
        (select(.pod != null and (.pod | test($pre) | not)) | "pod@slot\(.slot)"),
        (select(._stuck_pod != null and (._stuck_pod | test($pre) | not)) | "pod@slot\(.slot)"),
        (select((._grace | type) != "number") | "grace@slot\(.slot)") ]) }
JQ
}

# The public per-slot entry: the list --json schema (v1, additive-only). The ONE key list.
readonly MH_JQ_PUBLIC='def public: {slot, host_port, address, pod_ip, pod_uid, state, terminating_past_grace, deployment, pod, revision, stale, owner, restart_count};'

snap_q() { jq "$@" "$MH_TMP/snap.json"; }
# entry_field <slot> <field>: one field of a slot's entry; null prints nothing, false prints false.
entry_field() { snap_q -r --argjson k "$1" --arg f "$2" '.slots[] | select(.slot == $k) | .[$f] | if . == null then empty else . end'; }
public_entry() { snap_q -c --argjson k "$1" "${MH_JQ_PUBLIC} .slots[] | select(.slot == \$k) | public"; }
serving_count() { snap_q -r '[.slots[] | select(.state == "serving")] | length'; }
stuck_slot() { snap_q -r '[.slots[] | select(.terminating_past_grace)] | .[0].slot // empty'; }

# Next-fit: the free slots after the highest occupied one, wrapping (ADR-0039 §7). Empty pool:
# from the lowest. NOT lowest-free.
nextfit_slots() {
    snap_q -r '.slots as $s
        | ([$s | to_entries[] | select(.value.state != "free") | .key] | max) as $h
        | (if $h == null then $s else $s[($h + 1):] + $s[:($h + 1)] end)
        | .[] | select(.state == "free") | .slot'
}

# Refuse (stuck-terminating) when any MH pod is Terminating past its grace. Never forceable.
refuse_if_stuck() {
    local s pod
    s="$(stuck_slot)"
    [[ -z "$s" ]] && return 0
    pod="$(entry_field "$s" _stuck_pod)"
    mh_finish stuck-terminating "$s" "pod ${pod} is Terminating past its grace; investigate, don't force: kubectl --context ${KUBE_CONTEXT} -n ${MH_NAMESPACE} describe pod ${pod}"
}

# --- Lease --------------------------------------------------------------------------

lease_now_iso() { date -u +%Y-%m-%dT%H:%M:%S.000000Z; }

lease_body() { # lease_body [base-json-file]: a Lease held by us, keeping the base's metadata.
    local base="${1:-}"
    local src='{"apiVersion":"coordination.k8s.io/v1","kind":"Lease","metadata":{}}'
    [[ -n "$base" ]] && src="$(cat "$base")"
    jq -c --arg n "$MH_LEASE_NAME" --arg ns "$MH_NAMESPACE" --arg h "$MH_HOLDER" \
        --arg t "$(lease_now_iso)" --argjson d "$MH_LEASE_S" \
        '.metadata.name = $n | .metadata.namespace = $ns
         | .spec = ((.spec // {}) + {holderIdentity: $h, leaseDurationSeconds: $d, acquireTime: $t, renewTime: $t})' <<<"$src"
}

holder_for_detail() {
    if [[ "$1" =~ $MH_HOLDER_RE ]]; then printf '%s' "$1"; else printf 'malformed'; fi
}

# Acquire the mh-pool Lease, or finish mh-pool-busy at once.
mh_lease_acquire() {
    local host
    host="$(hostname 2>/dev/null || uname -n)"
    host="$(printf '%s' "$host" | LC_ALL=C tr -c 'A-Za-z0-9.-' '-' | cut -c1-63)"
    MH_HOLDER="${MH_OWNER}@${host:-host}:$$"
    lease_body >"$MH_TMP/lease-new.json"
    MH_LEASE_T0="$(now_ms)"
    if kc_call create-lease create -f "$MH_TMP/lease-new.json" -o json; then
        MH_LEASE_HELD=1; return 0
    fi
    # "(AlreadyExists)" is kubectl's wording. Matching it fails CLOSED: any other text is
    # kubectl-failed, never "busy" and never a proceed.
    err_has "(AlreadyExists)" || kc_fail
    kc_must get-lease get lease "$MH_LEASE_NAME" -o json
    cp "$MH_TMP/out" "$MH_TMP/lease-seen.json"
    local verdict holder
    verdict="$(jq -r --argjson now "$(date -u +%s)" --argjson skew "$MH_LEASE_FUTURE_SKEW_S" "${MH_JQ_TS}"'
        (.spec.holderIdentity // "") as $h
        | if $h == "" then "released 0"
          else ((.spec.renewTime // .spec.acquireTime) | if . == null then null else ts end) as $r
          | (.spec.leaseDurationSeconds // 0) as $d
          | if $r == null then "expired 0"
            elif $r > $now + $skew then "future \($r - $now)"
            elif $r + $d < $now then "expired \($now - $r)"
            else "busy 0" end
          end' "$MH_TMP/lease-seen.json" 2>/dev/null)" \
        || mh_finish kubectl-failed "" "malformed cluster value(s): lease"
    holder="$(holder_for_detail "$(jq -r '.spec.holderIdentity // ""' "$MH_TMP/lease-seen.json")")"
    case "${verdict%% *}" in
        released) ;;
        expired) mh_warn "RECLAIMING the expired ${MH_LEASE_NAME} Lease from holder=${holder} (renewed ${verdict#* }s ago, duration ${MH_LEASE_S}s)" ;;
        future)
            mh_warn "CLOCK SKEW: ${MH_LEASE_NAME} renewTime is ${verdict#* }s AHEAD of this host's clock; treating it as held (never reclaimed)"
            mh_finish mh-pool-busy "" "holder=${holder}" ;;
        *) mh_finish mh-pool-busy "" "holder=${holder}" ;;
    esac
    lease_body "$MH_TMP/lease-seen.json" >"$MH_TMP/lease-new.json"
    MH_LEASE_T0="$(now_ms)"
    if kc_call replace-lease replace -f "$MH_TMP/lease-new.json" -o json; then
        MH_LEASE_HELD=1; return 0
    fi
    # Someone else reclaimed between our get and replace: they hold it.
    err_has "(Conflict)" && mh_finish mh-pool-busy "" "holder=${holder}"
    kc_fail
}

# lease_guard [slot]: before EVERY mutation made under the Lease. The mutation itself may take up
# to MH_KUBECTL_TIMEOUT_S; if that could end past the Lease's expiry (measured from just before
# our acquire call, so conservatively), another invoker may legitimately reclaim it and the
# mutual exclusion would have lapsed silently. Refuse instead, with no mutation issued.
lease_guard() {
    # Fail CLOSED: a mutation outside the Lease is a bug, never a pass.
    (( MH_LEASE_HELD )) || mh_finish internal-error "${1:-}" "mutation without the ${MH_LEASE_NAME} Lease"
    local elapsed=$(( $(now_ms) - MH_LEASE_T0 ))
    (( elapsed + MH_KUBECTL_TIMEOUT_S * 1000 < MH_LEASE_S * 1000 )) \
        || mh_finish kubectl-failed "${1:-}" "lease-overrun: ${elapsed}ms into the ${MH_LEASE_S}s ${MH_LEASE_NAME} Lease (kubectl timeout ${MH_KUBECTL_TIMEOUT_S}s); no mutation issued"
}

# Release the Lease if (and only if) we still hold it: clear the holder by resourceVersion.
# Never deletes; never changes the result token (warnings only).
mh_lease_release() {
    (( MH_LEASE_HELD )) || return 0
    MH_LEASE_HELD=0
    if ! kc_call get-lease get lease "$MH_LEASE_NAME" -o json; then
        mh_warn "could not read the ${MH_LEASE_NAME} Lease to release it; it expires in ${MH_LEASE_S}s"
        return 0
    fi
    local cur
    cur="$(jq -r '.spec.holderIdentity // ""' "$MH_TMP/out")"
    if [[ "$cur" != "$MH_HOLDER" ]]; then
        mh_warn "${MH_LEASE_NAME} Lease is no longer ours (holder=$(holder_for_detail "$cur")); not clearing it"
        return 0
    fi
    jq -c 'del(.spec.holderIdentity)' "$MH_TMP/out" >"$MH_TMP/lease-rel.json"
    if ! kc_call release-lease replace -f "$MH_TMP/lease-rel.json" -o json; then
        mh_warn "releasing the ${MH_LEASE_NAME} Lease failed (it changed underneath us); it expires in ${MH_LEASE_S}s"
    fi
}

# --- Mutations ------------------------------------------------------------------------

# create_mh <slot> <owner> [replaces]: rc 0 created, rc 3 AlreadyExists; else kubectl-failed.
create_mh() {
    local k="$1" owner="$2" replaces="${3:-}"
    jq -c --slurpfile tpl "$MH_TMP/tpl.json" --argjson k "$k" --arg owner "$owner" --arg rep "$replaces" \
        --arg ls "$MH_LABEL_SLOT" --arg lr "$MH_LABEL_REVISION" --arg lo "$MH_LABEL_OWNER" \
        --arg ar "$MH_ANNOTATION_REPLACES" --arg port "$MH_WT_PORT_NAME" --arg env "$MH_ADDRESS_KEY" --arg ns "$MH_NAMESPACE" \
        --arg dp "$MH_DEPLOY_PREFIX" '
        $tpl[0] as $t | (.[] | select(.slot == $k)) as $s
        | ({($ls): ($k | tostring), ($lr): $t.revision, ($lo): $owner}) as $lab
        | $t.deployment
        | .metadata = {name: ($dp + ($k | tostring)), namespace: $ns,
                       labels: ((.metadata.labels // {}) + $lab),
                       annotations: ((.metadata.annotations // {}) + (if $rep == "" then {} else {($ar): $rep} end))}
        | .spec.replicas = 1
        | .spec.selector.matchLabels += {($ls): ($k | tostring)}
        | .spec.template.metadata.labels += $lab
        | .spec.template.spec.containers |= map(
            if any(.ports[]?; .name == $port) then
              (.ports |= map(if .name == $port then .hostPort = $s.host_port else . end))
              | (.env |= map(if .name == $env and .valueFrom.configMapKeyRef != null
                             then .valueFrom.configMapKeyRef.name = $s.cm else . end))
            else . end)
        | del(.status)' "$MH_TMP/slots.json" >"$MH_TMP/create.json"
    lease_guard "$k"
    if kc_call create-deployment create -f "$MH_TMP/create.json"; then
        MH_CREATED_SLOT="$k"; MH_CREATED_OWNER="$owner"
        mh_log "created Deployment $(mh_dep_name "$k") (owner=${owner}${replaces:+, replaces slot ${replaces}})"
        return 0
    fi
    # kubectl's wording; fails CLOSED (any other failure is kubectl-failed, never "next slot").
    if err_has "(AlreadyExists)"; then
        mh_warn "slot ${k}: $(mh_dep_name "$k") already exists (lost a race); trying the next slot"
        return 3
    fi
    kc_fail "$k"
}

# delete_mh <slot>: the ONE delete mh.sh issues (foreground Deployment delete; SIGTERM via the
# cascade). Never a pod delete, never --force/--grace-period.
delete_mh() {
    lease_guard "$1"
    if kc_call delete-deployment delete deployment "$(mh_dep_name "$1")" --cascade=foreground --wait=false; then
        mh_log "deleting Deployment $(mh_dep_name "$1") (foreground; the pod gets SIGTERM and drains)"
        return 0
    fi
    if err_has "(NotFound)"; then mh_warn "$(mh_dep_name "$1") was already gone"; return 0; fi
    kc_fail "$1"
}

# --- Waits (always OUTSIDE the Lease) ------------------------------------------------------

# Wall clock in milliseconds (bash >= 5: EPOCHREALTIME; else whole seconds from date).
now_ms() {
    if [[ -n "${EPOCHREALTIME:-}" ]]; then
        local t="${EPOCHREALTIME/[.,]/}"
        printf '%s' "$(( t / 1000 ))"
    else
        printf '%s' "$(( $(date +%s) * 1000 + 999 ))"
    fi
}

# wait_until <timeout-s> <predicate-fn> <args...>: rc 0 when the predicate holds, 1 once at least
# <timeout-s> seconds of wall clock have passed without it. A predicate rc >= 2 means "can never
# hold" (e.g. the object was deleted) and is returned at once.
wait_until() {
    local timeout="$1" deadline rc; shift
    deadline=$(( $(now_ms) + timeout * 1000 ))
    while :; do
        "$@"; rc=$?
        (( rc == 0 )) && return 0
        (( rc >= 2 )) && return "$rc"
        (( $(now_ms) >= deadline )) && return 1
        sleep "$MH_POLL_INTERVAL_S"
    done
}

is_available() {
    if ! kc_call get-deployment get deployment "$(mh_dep_name "$1")" -o json; then
        err_has "(NotFound)" && return 2   # deleted during the wait: it can never go Available
        kc_fail "$1"
    fi
    jq -e '(.status.availableReplicas // 0) >= 1
           and any((.status.conditions // [])[]; .type == "Available" and .status == "True")
           and ((.status.observedGeneration // 0) >= (.metadata.generation // 0))' "$MH_TMP/out" >/dev/null
}

is_gone() { mh_snapshot; [[ "$(entry_field "$1" state)" == "free" ]]; }

is_signalled() { # SIGTERM delivered: every slot pod carries a deletionTimestamp, or none remains.
    mh_snapshot
    [[ "$(entry_field "$1" _signalled)" == "true" ]]
}

up_timeout() { # the derived default `up` bound
    local w; w="$(jq -r '.probe_window' "$MH_TMP/tpl.json")"
    local t=$(( w + MH_UP_MARGIN_S )); (( t < 1 )) && t=1
    printf '%s' "$t"
}

# --- Verbs ----------------------------------------------------------------------------

parse_list() { (( $# == 0 )) || mh_finish usage "" "list takes only --json"; }

run_list() {
    mh_snapshot
    MH_OUT_JSON="$(snap_q -c --argjson v "$MH_SCHEMA_VERSION" "${MH_JQ_PUBLIC}
        {ok: true, schema_version: \$v, template_revision, baseline, slots: [.slots[] | public]}")"
    snap_q -r '"template_revision=\(.template_revision) baseline=\(.baseline)",
        (.slots[] | "  slot \(.slot)  \(.state)\(if .terminating_past_grace then " PAST-GRACE" else "" end)  port=\(.host_port)  owner=\(.owner // "-")  rev=\(.revision // "-")\(if .stale then " STALE" else "" end)  pod=\(.pod // "-")  restarts=\(.restart_count // "-")")' >&2
    mh_finish ok "" "listed"
}

parse_up() {
    while (( $# )); do
        case "$1" in
            --slot) (( $# >= 2 )) || mh_finish usage "" "--slot needs a value"; validate_slot_arg "$2"; ARG_SLOT="$2"; shift 2 ;;
            --owner) (( $# >= 2 )) || mh_finish usage "" "--owner needs a value"; validate_owner "$2"; MH_OWNER="$2"; shift 2 ;;
            --timeout) (( $# >= 2 )) || mh_finish usage "" "--timeout needs a value"; validate_timeout "$2"; ARG_TIMEOUT="$2"; shift 2 ;;
            *) mh_finish usage "" "up: unknown argument" ;;
        esac
    done
}

run_up() {
    local slot="$ARG_SLOT" timeout="$ARG_TIMEOUT"
    [[ -n "$slot" ]] && require_known_slot "$slot"
    [[ -n "$timeout" ]] || timeout="$(up_timeout)"
    local rev; rev="$(jq -r '.revision' "$MH_TMP/tpl.json")"

    mh_lease_acquire
    mh_snapshot
    local -a cands=()
    if [[ -n "$slot" ]]; then
        local st; st="$(entry_field "$slot" state)"
        if [[ ( "$st" == "starting" || "$st" == "serving" ) && "$(entry_field "$slot" revision)" == "$rev" \
              && "$(entry_field "$slot" owner)" == "$MH_OWNER" ]]; then
            mh_lease_release
            up_await "$slot" "$timeout" false "existing"
        fi
        [[ "$st" == "free" ]] || mh_finish mh-no-free-slot "$slot" "slot-occupied (state=${st})"
        cands=("$slot")
    else
        mapfile -t cands < <(nextfit_slots)
    fi
    (( ${#cands[@]} )) || mh_finish mh-no-free-slot "" "all $(snap_q -r '.slots | length') slots are occupied"
    local c made=""
    for c in "${cands[@]}"; do
        if create_mh "$c" "$MH_OWNER"; then made="$c"; break; fi
    done
    [[ -n "$made" ]] || mh_finish mh-no-free-slot "$slot" "every candidate slot already exists (lost the race)"
    mh_lease_release
    up_await "$made" "$timeout" true "created"
}

# up_await <slot> <timeout> <created> <detail>: wait Available, print the entry, finish.
up_await() {
    local k="$1" timeout="$2" rc
    wait_until "$timeout" is_available "$k"; rc=$?
    if (( rc >= 2 )); then
        mh_finish not-available-timeout "$k" "$(mh_dep_name "$k") was deleted during the wait for Available"
    elif (( rc )); then
        mh_finish not-available-timeout "$k" "timeout=${timeout}s; $(mh_dep_name "$k") left in place for diagnosis (kubectl --context ${KUBE_CONTEXT} -n ${MH_NAMESPACE} describe deployment $(mh_dep_name "$k")); reclaim it with: mh.sh drain ${k} --owner ${MH_OWNER}"
    fi
    mh_snapshot
    MH_OUT_JSON="$(snap_q -c --argjson k "$k" --argjson c "$3" "${MH_JQ_PUBLIC}
        .slots[] | select(.slot == \$k) | public + {ok: true, created: \$c}")"
    mh_finish ok "$k" "$4 timeout=${timeout}s"
}

parse_drain() {
    local owner_set=0
    while (( $# )); do
        case "$1" in
            --all) ARG_ALL=1; shift ;;
            --no-wait) ARG_NOWAIT=1; shift ;;
            --force) ARG_FORCE=1; shift ;;
            --owner) (( $# >= 2 )) || mh_finish usage "" "--owner needs a value"; validate_owner "$2"; MH_OWNER="$2"; owner_set=1; shift 2 ;;
            --timeout) (( $# >= 2 )) || mh_finish usage "" "--timeout needs a value"; validate_timeout "$2"; ARG_TIMEOUT="$2"; shift 2 ;;
            -*) mh_finish usage "" "drain: unknown flag" ;;
            *) [[ -z "$ARG_SLOT" ]] || mh_finish usage "" "drain takes one slot"; validate_slot_arg "$1"; ARG_SLOT="$1"; shift ;;
        esac
    done
    if (( ARG_ALL )); then
        [[ -z "$ARG_SLOT" ]] || mh_finish usage "" "drain: a slot and --all are exclusive"
        (( owner_set )) || mh_finish usage "" "drain --all requires --owner"
    else
        [[ -n "$ARG_SLOT" ]] || mh_finish usage "" "drain needs a slot (or --all --owner ID)"
    fi
}

run_drain() {
    local slot="$ARG_SLOT" nowait="$ARG_NOWAIT" force="$ARG_FORCE" timeout="$ARG_TIMEOUT"
    (( ARG_ALL )) && drain_all "$nowait" "$force" "$timeout"
    require_known_slot "$slot"

    mh_lease_acquire
    mh_snapshot
    local pre st detail="drained"
    pre="$(public_entry "$slot")"
    st="$(entry_field "$slot" state)"
    if [[ "$st" == "free" ]]; then
        MH_OUT_JSON="$(jq -c '. + {ok: true, drained: false}' <<<"$pre")"
        mh_finish ok "$slot" "empty"
    fi
    refuse_if_stuck
    if [[ "$st" == "draining" || "$st" == "terminating" ]]; then
        detail="joined"
        mh_log "slot ${slot} is already ${st}; joining the wait (no second delete)"
    else
        drain_guard "$slot" "$force"
        delete_mh "$slot"
    fi
    mh_lease_release
    drain_await "$slot" "$nowait" "$timeout"
    (( nowait )) && detail="signalled"
    MH_OUT_JSON="$(jq -c '. + {ok: true, drained: true}' <<<"$pre")"
    mh_finish ok "$slot" "$detail"
}

# drain_guard <slot> <force>: foreign-owner, then last-serving-mh (both --force-able), on the
# fresh snapshot taken under the Lease.
drain_guard() {
    local k="$1" force="$2" owner
    owner="$(entry_field "$k" owner)"
    if [[ "$owner" != "$MH_OWNER" ]] && (( ! force )); then
        mh_finish foreign-owner "$k" "slot ${k} is owned by ${owner:-nobody}, not ${MH_OWNER}; use --owner ${owner:-<owner>} (or --force)"
    fi
    if [[ "$(entry_field "$k" state)" == "serving" ]] && (( $(serving_count) <= 1 )) && (( ! force )); then
        mh_finish last-serving-mh "$k" "slot ${k} is the only serving MH; run mh.sh up first (or --force)"
    fi
}

# drain_await <slot> <nowait> <timeout>: the gone (or SIGTERM-delivered) wait, outside the Lease.
drain_await() {
    local k="$1" bound="$3" pod
    [[ -n "$bound" ]] || bound=$(( $(entry_field "$k" _grace) + MH_DRAIN_MARGIN_S ))
    (( bound < 1 )) && bound=1
    pod="$(entry_field "$k" pod)"
    local pred=is_gone
    (( $2 )) && pred=is_signalled
    if ! wait_until "$bound" "$pred" "$k"; then
        mh_finish drain-timeout "$k" "bound=${bound}s; pod=${pod:-none} still present; inspect: kubectl --context ${KUBE_CONTEXT} -n ${MH_NAMESPACE} describe pod ${pod:-<pod>}; mh.sh list (terminating_past_grace)"
    fi
}

drain_all() {
    local nowait="$1" force="$2" timeout="$3"
    mh_lease_acquire
    mh_snapshot
    refuse_if_stuck
    local -a targets=() waits=()
    mapfile -t targets < <(snap_q -r --arg o "$MH_OWNER" '.slots[] | select(.owner == $o and .state != "free") | .slot')
    local remaining refused="" k st
    remaining="$(serving_count)"
    for k in "${targets[@]}"; do
        st="$(entry_field "$k" state)"
        if [[ "$st" == "serving" ]] && (( remaining <= 1 )) && (( ! force )); then
            refused="$k"; continue
        fi
        if [[ "$st" != "draining" && "$st" != "terminating" ]]; then delete_mh "$k"; fi
        [[ "$st" == "serving" ]] && remaining=$(( remaining - 1 ))
        waits+=("$k")
    done
    mh_lease_release
    for k in "${waits[@]}"; do drain_await "$k" "$nowait" "$timeout"; done
    local drained
    drained="$(printf '%s\n' "${waits[@]}" | jq -cRs '[split("\n")[] | select(. != "") | tonumber]')"
    if [[ -n "$refused" ]]; then
        MH_FAIL_EXTRA="$(jq -cn --argjson d "$drained" '{drained: $d}')"
        mh_finish last-serving-mh "$refused" "slot ${refused} is the only serving MH; drained the rest of owner ${MH_OWNER}: ${drained}"
    fi
    MH_OUT_JSON="$(jq -cn --argjson d "$drained" '{ok: true, drained: $d}')"
    mh_finish ok "" "drained=${drained}"
}

parse_upgrade() {
    while (( $# )); do
        case "$1" in
            --timeout) (( $# >= 2 )) || mh_finish usage "" "--timeout needs a value"; validate_timeout "$2"; ARG_TIMEOUT="$2"; shift 2 ;;
            *) mh_finish usage "" "upgrade: unknown argument" ;;
        esac
    done
}

run_upgrade() {
    local timeout="$ARG_TIMEOUT"
    [[ -n "$timeout" ]] || timeout="$(up_timeout)"
    local replaced="[]" step=0 max_steps
    max_steps=$(( 2 * $(jq 'length' "$MH_TMP/slots.json") + 2 ))
    while :; do
        (( step++ < max_steps )) || mh_finish internal-error "" "upgrade did not converge in ${max_steps} steps"
        # Any failure from here on reports the steps already done (deploy.sh / runbook resume).
        MH_FAIL_EXTRA="$(jq -cn --argjson r "$replaced" '{replaced: $r}')"
        MH_FAIL_SUFFIX="replaced so far=${replaced}; rerun mh.sh upgrade to resume"
        mh_lease_acquire
        mh_snapshot
        refuse_if_stuck
        local pair old new
        # A pending resume pair: a current-revision live replacement annotated replaces=k,
        # whose k still holds a stale MH. It needs no new slot.
        pair="$(snap_q -r '. as $s | [ $s.slots[] | select((.state == "starting" or .state == "serving") and (.stale | not) and .revision != null and ._replaces != null)
                | (._replaces | tonumber) as $k | {old: $k, new: .slot}
                | select(any($s.slots[]; .slot == $k and .state != "free" and .stale)) ] | .[0] // empty | "\(.old) \(.new)"')"
        if [[ -n "$pair" ]]; then
            old="${pair% *}"; new="${pair#* }"
            UPGRADE_OWNER="$(entry_field "$new" owner)"
            mh_log "resuming: slot ${new} replaces stale slot ${old}"
        else
            old="$(snap_q -r '[.slots[] | select((.state == "starting" or .state == "serving") and .stale) | .slot] | .[0] // empty')"
            if [[ -z "$old" ]]; then
                mh_lease_release
                MH_FAIL_SUFFIX=""
                MH_OUT_JSON="$(jq -cn --argjson r "$replaced" '{ok: true, replaced: $r}')"
                mh_finish ok "" "$([[ "$replaced" == "[]" ]] && echo noop || echo "replaced=${replaced}")"
            fi
            upgrade_create "$old"
            new="$UPGRADE_NEW"
        fi
        mh_lease_release
        local rc
        wait_until "$timeout" is_available "$new"; rc=$?
        if (( rc >= 2 )); then
            mh_finish mh-rollout-failed "$new" "old=${old} new=${new}: $(mh_dep_name "$new") was deleted during the wait for Available; slot ${old} untouched and still serving"
        elif (( rc )); then
            mh_finish mh-rollout-failed "$new" "old=${old} new=${new}: the replacement never went Available (timeout=${timeout}s); slot ${old} untouched and still serving; reclaim the replacement with: mh.sh drain ${new} --owner ${UPGRADE_OWNER:-$MH_DEFAULT_OWNER}"
        fi
        upgrade_drain_old "$old"
        replaced="$(jq -c --argjson o "$old" --argjson n "$new" '. + [{old: $o, new: $n}]' <<<"$replaced")"
        MH_CREATED_SLOT=""
    done
}

# Under the Lease: create the replacement for stale slot $1 in a next-fit slot (owner
# inherited), or refuse mh-no-free-slot before touching the old MH. Sets UPGRADE_NEW.
upgrade_create() {
    local old="$1" owner c
    owner="$(entry_field "$old" owner)"
    owner="${owner:-$MH_DEFAULT_OWNER}"
    UPGRADE_OWNER="$owner"
    local -a cands=()
    mapfile -t cands < <(nextfit_slots)
    (( ${#cands[@]} )) || mh_finish mh-no-free-slot "$old" "no free slot for the replacement of slot ${old}; the old MH is untouched and still serving"
    UPGRADE_NEW=""
    for c in "${cands[@]}"; do
        if create_mh "$c" "$owner" "$old"; then UPGRADE_NEW="$c"; break; fi
    done
    [[ -n "$UPGRADE_NEW" ]] || mh_finish mh-no-free-slot "$old" "every candidate slot already exists (lost the race); the old MH is untouched"
}

# One upgrade drain step: Lease, fresh snapshot, stuck + last-serving re-evaluated (owner is
# skipped: upgrade is owner-preserving), delete (or join), release, wait gone.
upgrade_drain_old() {
    local old="$1" st
    mh_lease_acquire
    mh_snapshot
    refuse_if_stuck
    st="$(entry_field "$old" state)"
    if [[ "$st" == "free" ]]; then mh_lease_release; return 0; fi
    if [[ "$st" != "draining" && "$st" != "terminating" ]]; then
        if [[ "$st" == "serving" ]] && (( $(serving_count) <= 1 )); then
            mh_finish last-serving-mh "$old" "slot ${old} is the only serving MH; its replacement is not serving"
        fi
        delete_mh "$old"
    fi
    mh_lease_release
    drain_await "$old" 0 ""
}

# --- main ---------------------------------------------------------------------------

main() {
    # No errexit, deliberately: every rc is handled explicitly, and -e would abort before the
    # emitter could map a failure to its token (the EXIT trap would then say internal-error).
    set -uo pipefail
    IFS=$'\n\t'
    exec 3>&1 4>&2
    trap mh_on_exit EXIT
    MH_TMP="$(mktemp -d "${TMPDIR:-/tmp}/mh-sh.XXXXXX")" || { MH_TMP=""; mh_finish internal-error "" "mktemp failed"; }
    trap 'mh_on_signal INT' INT
    trap 'mh_on_signal TERM' TERM
    MH_OWNER="$MH_DEFAULT_OWNER"

    local -a args=()
    local a
    for a in "$@"; do
        if [[ "$a" == "--json" ]]; then MH_JSON=1; else args+=("$a"); fi
    done
    local verb="${args[0]:-}"
    (( ${#args[@]} )) && args=("${args[@]:1}")
    case "$verb" in
        -h|--help|help) usage_text; mh_finish ok "" "help" ;;
        list|up|drain|upgrade) ;;
        "") usage_text; mh_finish usage "" "no verb given" ;;
        *) usage_text; mh_finish usage "" "unknown verb" ;;
    esac
    # Every argument and config value is validated BEFORE the first kubectl call.
    MH_VERB="$verb"
    "parse_${verb}" "${args[@]}"
    validate_config
    preflight
    load_interface
    "run_${verb}"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
