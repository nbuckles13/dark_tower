#!/usr/bin/env bash
# dev-web.sh — preflight-check the local topology, then launch the web-app demo.
#
# The executable companion to the R-49 dev runbook
# (docs/runbooks/client-dev-local.md, story task #20): it automates the
# bring-up checks that runbook documents by hand, then starts the Vite dev
# server. The runbook stays the source of prose/diagnosis; this script is the
# fast path for "just launch the demo, and tell me loudly if something's off."
#
# What it verifies (SSoT-driven — versions are read from the repo, not hardcoded):
#   - nvm present (prereq for the Node fix commands)          [WARN]
#   - Node major version matches .nvmrc                       [HARD FAIL]
#   - pnpm version matches package.json `packageManager` pin  [HARD FAIL]
#   - AC (8443) + GC (8444) host ports answer                 [HARD FAIL]
#   - MC/MH WebTransport cert fingerprints present            [WARN — join only]
#   - MC/MH WebTransport ports have a right-family listener    [WARN — join only]
#   - demo.localhost resolves (/etc/hosts)                    [WARN — browser only]
#
# HARD FAIL = nothing works without it, so we stop. WARN = only part of the
# demo breaks (documented inline), so we start the server and tell you.
#
# Usage:
#   scripts/dev-web.sh              # preflight + install (if needed) + dev server
#   scripts/dev-web.sh --check      # preflight only, do not install or launch
#   scripts/dev-web.sh --no-install # skip `pnpm install` (deps already present)
#
# Ports mirror the Vite dev-proxy defaults (VITE_AC_PROXY_TARGET /
# VITE_GC_PROXY_TARGET) and #4's kind-config extraPortMappings; override via
# AC_PORT / GC_PORT env if your overlay differs.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ─── Args ───────────────────────────────────────────────────────
CHECK_ONLY=false
DO_INSTALL=true
for arg in "$@"; do
    case "$arg" in
        --check)      CHECK_ONLY=true ;;
        --no-install) DO_INSTALL=false ;;
        -h|--help)    sed -n '2,29p' "$0"; exit 0 ;;
        *) echo "Unknown arg: $arg (see --help)" >&2; exit 1 ;;
    esac
done

# ─── Output helpers ─────────────────────────────────────────────
if [[ -t 1 ]]; then
    RED=$'\033[0;31m'; YEL=$'\033[1;33m'; GRN=$'\033[0;32m'; NC=$'\033[0m'
else
    RED=''; YEL=''; GRN=''; NC=''
fi
HARD_FAIL=0
pass() { echo "  ${GRN}✓${NC} $*"; }
warn() { echo "  ${YEL}!${NC} $*"; }
fail() { echo "  ${RED}✗${NC} $*"; HARD_FAIL=1; }

# ─── SSoT: expected versions read from the repo ─────────────────
EXPECTED_NODE="$(<.nvmrc)"                       # e.g. 22.11.0
EXPECTED_NODE_MAJOR="${EXPECTED_NODE%%.*}"       # e.g. 22
# packageManager pin, e.g. "pnpm@10.33.2" → 10.33.2 (no node needed to parse)
EXPECTED_PNPM="$(grep -oE '"packageManager"[[:space:]]*:[[:space:]]*"pnpm@[^"]+"' package.json \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"

# Ports (Vite proxy defaults / #4 kind-config); env-overridable.
AC_PORT="${AC_PORT:-8443}"
GC_PORT="${GC_PORT:-8444}"
FINGERPRINTS_JSON="infra/docker/certs/fingerprints.json"
DEMO_HOST="demo.localhost"

echo "== Preflight (expecting Node ${EXPECTED_NODE_MAJOR}.x, pnpm ${EXPECTED_PNPM}) =="

# ─── nvm (prerequisite for the Node fix commands below) ─────────
# nvm is a shell function sourced from $NVM_DIR/nvm.sh, NOT a binary on PATH —
# `command -v nvm` never sees it from a script, so probe the file on disk.
# Absence is a WARN, not a hard fail: Node could be managed another way. But
# the `nvm install/use` remediation the Node check prints assumes it, so if
# Node is ALSO missing this is really the first thing to fix.
NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
if [[ -s "$NVM_DIR/nvm.sh" ]]; then
    # On disk ≠ loaded. nvm is only a command in shells that have sourced
    # nvm.sh (normally via ~/.bashrc). So prefix the Node fix commands with an
    # explicit `source` — idempotent if already loaded, and it makes the
    # recommendation work even in a shell where nvm isn't yet active.
    NVM_LOAD="source \"$NVM_DIR/nvm.sh\" && "
    pass "nvm installed ($NVM_DIR)"
else
    NVM_LOAD=""
    warn "nvm not found at $NVM_DIR — the recommended way to install the pinned Node."
    echo "      Install: see https://github.com/nvm-sh/nvm#installing-and-updating"
    echo "      (then restart your shell so nvm is sourced, and re-run this script)"
fi

# ─── Node ───────────────────────────────────────────────────────
if ! command -v node >/dev/null 2>&1; then
    fail "node not found. Install + set the pinned version as default (run in your shell):"
    echo "      ${NVM_LOAD}nvm install ${EXPECTED_NODE} && nvm alias default ${EXPECTED_NODE} && nvm use ${EXPECTED_NODE}"
    echo "      (alias default persists across shells; 'nvm use' alone is current-shell-only)"
else
    NODE_VER="$(node --version)"; NODE_VER="${NODE_VER#v}"
    if [[ "${NODE_VER%%.*}" != "$EXPECTED_NODE_MAJOR" ]]; then
        fail "node ${NODE_VER} — .nvmrc pins ${EXPECTED_NODE}. Set it as default so new shells keep it:"
        echo "      ${NVM_LOAD}nvm alias default ${EXPECTED_NODE} && nvm use ${EXPECTED_NODE}"
    elif [[ "$NODE_VER" != "$EXPECTED_NODE" ]]; then
        warn "node ${NODE_VER} (pin is ${EXPECTED_NODE}; same major, likely fine)"
    else
        pass "node ${NODE_VER}"
    fi
fi

# ─── pnpm (via corepack, derived from the packageManager pin) ───
# The corepack BUNDLED with an older pinned Node (e.g. 22.11.0) carries stale
# pnpm signing keys and fails `corepack prepare` on newer pnpm with
# "Cannot find matching keyid". So the fix leads with `npm i -g corepack@latest`
# (current keys) before enable/prepare — robust regardless of Node's vintage.
COREPACK_FIX="npm install -g corepack@latest && corepack enable && corepack prepare pnpm@${EXPECTED_PNPM} --activate"
if ! command -v pnpm >/dev/null 2>&1; then
    fail "pnpm not found. Activate the pinned version via corepack:"
    echo "      ${COREPACK_FIX}"
else
    # `pnpm ?` here usually means a corepack shim IS on PATH but the lazy
    # download/verify failed (same stale-key bug) — the fix is identical.
    PNPM_VER="$(pnpm --version 2>/dev/null || echo '?')"
    if [[ "$PNPM_VER" != "$EXPECTED_PNPM" ]]; then
        fail "pnpm ${PNPM_VER} — packageManager pins ${EXPECTED_PNPM}. Run:"
        echo "      ${COREPACK_FIX}"
    else
        pass "pnpm ${PNPM_VER}"
    fi
fi

# ─── AC / GC host ports ─────────────────────────────────────────
check_port() {
    local name="$1" port="$2"
    # A refused connection => cluster/NodePort down. Any HTTP reply (even 4xx)
    # means the listener is up, which is all we need to confirm here.
    if curl -sS -o /dev/null --max-time 3 "http://127.0.0.1:${port}/" 2>/dev/null \
       || curl -sS -o /dev/null --max-time 3 "http://127.0.0.1:${port}/" 2>&1 | grep -qv 'Connection refused'; then
        pass "${name} reachable on 127.0.0.1:${port}"
    else
        fail "${name} not answering on 127.0.0.1:${port} — is the Kind cluster up? (./infra/kind/scripts/setup.sh)"
    fi
}
check_port "AC" "$AC_PORT"
check_port "GC" "$GC_PORT"

# ─── Cert fingerprints (WARN — only the WebTransport join needs these) ──
if [[ -s "$FINGERPRINTS_JSON" ]]; then
    pass "cert fingerprints present ($FINGERPRINTS_JSON)"
else
    warn "fingerprints missing ($FINGERPRINTS_JSON) — sign-up/create work, but WebTransport JOIN will fail."
    echo "      Fix: scripts/generate-dev-certs.sh  (then restart this script — Vite reads them at config time)"
fi

# ─── MC / MH WebTransport reachability (WARN — only the join step needs these) ──
# The join step dials MC (signaling) then MH (media) over QUIC/WebTransport at
# the addresses GC advertises to the browser — read here from the service
# configmaps, the SSoT for those values. Two failure modes this catches that the
# AC/GC TCP probes above cannot: nothing published on the MC/MH WT port, and an
# IPv4/IPv6 loopback family mismatch.
#
# The "why" for both — the WSL2 mirrored-networking address-family trap, and why
# AC/GC keep working while only the join dies — is owned by
# docs/runbooks/client-dev-local.md (§5 F1). Deliberately a pointer, not a copy:
# this script is the fast path, the runbook is the source of prose/diagnosis.
#
# NOTE: the advertise addresses are read from the committed configmap files on
# disk, which is correct for the static host topology this script targets. On a
# devloop cluster the live ConfigMap is patched and the on-disk value is stale,
# so a green result there does not mean the join will work (runbook §1, F8).
WT_CONFIGMAPS=(
    infra/services/mc-service/mc-0-configmap.yaml
    infra/services/mc-service/mc-1-configmap.yaml
    infra/services/mh-service/mh-0-configmap.yaml
    infra/services/mh-service/mh-1-configmap.yaml
)

# Snapshot UDP listeners once; note whether "localhost" prefers IPv6 (mirrored).
if command -v ss >/dev/null 2>&1; then
    WT_LISTENERS="$(ss -uln 2>/dev/null || true)"
    WT_HAVE_SS=1
else
    WT_LISTENERS=""
    WT_HAVE_SS=0
fi
LOCALHOST_FIRST="$(getent ahosts localhost 2>/dev/null | awk 'NR==1 {print $1}')"

# 0 if a UDP listener for $port exists in family $fam (4|6). rootlessport publishes
# on 127.0.0.1 (v4) or [::1] (v6); 0.0.0.0/[::]/`*` wildcards count too.
wt_listener_on() {
    local port="$1" fam="$2"
    if [[ "$fam" == 4 ]]; then
        printf '%s\n' "$WT_LISTENERS" | grep -qE "(127\.0\.0\.1|0\.0\.0\.0):${port}([[:space:]]|\$)"
    else
        printf '%s\n' "$WT_LISTENERS" | grep -qE "(\[::1?\]|\*):${port}([[:space:]]|\$)"
    fi
}

check_wt_endpoint() {
    local file="$1" label="$2" url hostport host port
    url="$(grep -E 'WEBTRANSPORT_ADVERTISE_ADDRESS' "$file" 2>/dev/null | grep -oE 'https://[^"]+' | head -1)"
    if [[ -z "$url" ]]; then
        warn "${label}: no advertise address in $file — skipping reachability check"
        return
    fi
    hostport="${url#https://}"; port="${hostport##*:}"; host="${hostport%:*}"
    if [[ "$WT_HAVE_SS" -eq 0 ]]; then
        warn "${label} advertises ${host}:${port} — install 'ss' (iproute2) to verify a listener"
        return
    fi
    case "$host" in
        127.0.0.1|0.0.0.0)                       # IPv4 literal — browser dials IPv4
            if wt_listener_on "$port" 4; then
                pass "${label} WebTransport listener on ${host}:${port} (IPv4)"
            else
                warn "${label}: nothing listening on ${host}:${port} — join's media step will fail (cluster up? MC/MH published?)"
            fi ;;
        ::1|::)                                  # IPv6 literal
            if wt_listener_on "$port" 6; then
                pass "${label} WebTransport listener on [${host}]:${port} (IPv6)"
            else
                warn "${label}: nothing listening on [${host}]:${port} (IPv6) — join's media step will fail"
            fi ;;
        *)                                       # hostname — name resolution picks the family
            local has4=1 has6=1
            wt_listener_on "$port" 4 || has4=0
            wt_listener_on "$port" 6 || has6=0
            if [[ "$LOCALHOST_FIRST" == ::* && "$has6" -eq 0 && "$has4" -eq 1 ]]; then
                warn "${label} advertises hostname '${host}:${port}', which resolves to IPv6 (${LOCALHOST_FIRST}) under WSL2 mirrored mode, but only an IPv4 listener exists → QUIC will time out."
                echo "      Fix: set ${host} → the IPv4 literal 127.0.0.1 in $file (see the comment there), then re-run the cluster setup."
            elif [[ "$has4" -eq 1 || "$has6" -eq 1 ]]; then
                pass "${label} WebTransport listener on ${host}:${port}"
            else
                warn "${label}: nothing listening on ${host}:${port} — join's media step will fail"
            fi ;;
    esac
}

for cm in "${WT_CONFIGMAPS[@]}"; do
    label="${cm##*/}"; label="${label%-configmap.yaml}"
    check_wt_endpoint "$cm" "$label"
done

# ─── demo.localhost resolution (WARN — WSL2-side tooling only) ───
# This checks THIS machine's resolver (/etc/hosts + glibc). It is NOT the browser's:
# a host browser reads its own hosts file, and Chromium resolves *.localhost to
# loopback natively per RFC 6761 §6.3, so it typically needs no entry at all.
# What breaks without the entry here is WSL2-side tooling — this preflight, and any
# local curl/browser hitting ${DEMO_HOST}. See docs/runbooks/client-dev-local.md F6.
if getent hosts "$DEMO_HOST" >/dev/null 2>&1 || grep -qE "[[:space:]]${DEMO_HOST}(\$|[[:space:]])" /etc/hosts 2>/dev/null; then
    pass "${DEMO_HOST} resolves"
else
    warn "${DEMO_HOST} does not resolve here — WSL2-side tooling (curl, this check) can't reach it."
    echo "      Fix: echo '127.0.0.1  ${DEMO_HOST}' | sudo tee -a /etc/hosts"
fi

echo
if [[ "$HARD_FAIL" -ne 0 ]]; then
    echo "${RED}Preflight failed.${NC} Resolve the ✗ items above, then re-run." >&2
    exit 1
fi
echo "${GRN}Preflight OK.${NC}"

if $CHECK_ONLY; then
    exit 0
fi

# ─── Install + launch ───────────────────────────────────────────
if $DO_INSTALL; then
    echo; echo "== pnpm install =="
    pnpm install
fi

echo; echo "== Starting web-app dev server =="
echo "   Open Chrome at: http://${DEMO_HOST}:5173"
echo
# Launch through the Nx `dev` target, NOT `pnpm --filter … dev`. The web-app
# `dev` target declares `dependsOn: proto-gen:codegen`, so Nx generates the
# gitignored protobuf-es `*_pb.ts` first. Running vite directly (via --filter)
# bypasses the task graph, so those generated imports don't exist yet and vite
# fails to resolve `signaling_pb.js`. Nx caches codegen, so this is ~free once
# the proto is unchanged.
exec pnpm nx run web-app:dev
