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
        -h|--help)    sed -n '2,33p' "$0"; exit 0 ;;
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
        fail "${name} not answering on 127.0.0.1:${port} — is the Kind cluster up? (infra/devloop/dev-cluster setup)"
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

# ─── demo.localhost resolution (WARN — server starts regardless) ───
if getent hosts "$DEMO_HOST" >/dev/null 2>&1 || grep -qE "[[:space:]]${DEMO_HOST}(\$|[[:space:]])" /etc/hosts 2>/dev/null; then
    pass "${DEMO_HOST} resolves"
else
    warn "${DEMO_HOST} does not resolve — the browser can't reach the app until you add it."
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
