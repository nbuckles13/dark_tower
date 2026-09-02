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
#   - MC/MH WebTransport cert fingerprints present            [HARD FAIL]
#   - MC/MH WebTransport ports have a right-family listener   [HARD FAIL]
#   - ...including when that check CANNOT RUN. All FOUR causes are  [HARD FAIL]
#     hard fails: `ss` missing, `ss` failed, no advertise address,
#     no resolver (getent). Enumerative, not illustrative — the
#     runbook cites this block as the severity SSoT, so it must not
#     under-list. See the contract below.
#   - demo.localhost resolves (/etc/hosts)                    [WARN — browser only]
#
# THE CONTRACT (rewritten, not just retagged — the old rule stopped explaining
# its own rows once the WebTransport checks became hard fails):
#
#   HARD FAIL = without this, the demonstrated path SILENTLY produces no audio.
#               "Silently" is the operative word. The script stops and does NOT
#               start the dev server, and every hard fail prints the next
#               command to run.
#   WARN      = a convenience or a genuinely side path is degraded. The script
#               names which, then starts the server anyway.
#
# Why the WebTransport rows moved: they were written when the demo's success
# criterion was sign-up and create-meeting, which run over TCP through the Vite
# proxy and are unaffected by any WebTransport problem. That is no longer the
# criterion — the demo is now hearing your own audio back through MH
# (docs/decisions/adr-0036-media-flow.md), so MC and MH reachability IS the
# demonstrated path. A warning there produced the worst available outcome: a
# demo that starts, appears to join, and returns no audio.
#
# A check that CANNOT RUN is a hard fail too, not a pass. There are only two
# outcomes on a critical-path check, and "I could not tell" is not one of them —
# otherwise not having iproute2 installed reproduces the exact silent failure
# this script exists to stop. Those messages say CANNOT VERIFY explicitly so
# the remedy is distinguishable. There is deliberately no bypass environment
# variable: an escape hatch here would be re-opened by the first person to hit
# it and never closed again.
#
# SECURE CONTEXT — the failure this script cannot see, and the one most likely
# to waste your afternoon. getUserMedia, WebCodecs, WebCrypto and WebTransport
# are ALL secure-context gated, so the whole media pipeline is either present or
# entirely absent. `.localhost` names and loopback literals (127.0.0.1, [::1])
# ARE potentially trustworthy, which is why the demo works over plain http://.
# A NON-LOOPBACK http:// origin — reaching Vite at http://<lan-ip>:5173 or by
# hostname from another machine — silently disables all four and presents as
# "joined, no audio". Use a .localhost name or a loopback literal, or terminate
# real TLS. Details: docs/runbooks/client-dev-local.md#secure-context-and-media-setup
#
# That runbook section is the CANONICAL prose and is authored by story task 20;
# the paragraph above is a deliberate offline copy, kept inline because a reader
# who follows a not-yet-existing anchor is by construction someone who just hit
# a hard fail — the moment a bad "fix" gets pasted in. Reconcile the two when
# task 20 lands (tracked in docs/TODO.md §Documentation Hygiene).
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
        # Print the header block by DERIVATION, not by a hardcoded line range.
        # `sed -n '2,29p'` printed the header correctly as of this commit — it
        # was fragile, not broken. THIS diff is what breaks it: the
        # secure-context pointer and the rewritten severity contract below push
        # the header past line 29, at which point a literal range silently drops
        # the tail — including part of the severity contract that
        # docs/runbooks/client-dev-local.md §3 Step 0 now cites as the SSoT.
        #
        # awk, not `sed -n '2,/^$/p'`: awk defines the block by what it IS (the
        # contiguous run of `#` lines after the shebang), so it is bounded by
        # construction and its worst case is truncation. The sed form's worst
        # case is UNBOUNDED — with no blank line after the header it prints to
        # EOF and dumps the whole script. Not reachable today, but "not
        # reachable today" is a property of the file this diff is editing.
        #
        # Both forms depend on the block staying contiguous, so a stray BARE
        # empty line mid-header would silently shorten --help. That is covered
        # mechanically, not by review: scripts/dev-web.test.sh asserts --help
        # still contains a sentinel from the header's LAST line.
        -h|--help)    awk 'NR==1{next} /^#/{print; next} {exit}' "$0"; exit 0 ;;
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

# ─── Bundler load probe (the native-binding silent-skip class) ──
# Vite 8 / rolldown load a native binding (@rolldown/binding-linux-x64-gnu) at import
# time. When the running Node is below the workspace engines floor, pnpm SILENTLY skips
# that engines-mismatched optional binding at install; `pnpm install` succeeds and vite
# then crashes at LAUNCH with "cannot find native binding". engine-strict now fails a
# below-floor install loudly (see .npmrc + root package.json engines) — but a
# node_modules tree installed EARLIER under a below-floor Node keeps the gap until it is
# reinstalled, and that residual case is exactly what this probe catches.
#
# Guarded on node_modules presence: only meaningful when a tree exists (so --check and
# --no-install catch the stale-tree case). A fresh clone has nothing to probe — skip with
# a note; the fresh case is caught loudly by engine-strict at `pnpm install`.
#
# It does a REAL dynamic import() of rolldown's entry (resolved from vite) — the exact
# operation that loads the native binding; a resolve-only check would false-pass. Two
# distinct causes get two remedies (exit 3 = tree unresolvable → reinstall; exit 4 =
# binding load threw → engine/binding remedy), so the binding-mismatch message is not
# emitted for a merely-incomplete tree. No version literal — the floor is derived from
# package.json engines.node (ENGINES_NODE), the in-repo SSoT.
probe_bundler() {
    if [[ ! -d node_modules ]]; then
        warn "bundler load probe skipped — no node_modules yet (fresh clone). 'pnpm install' below is the loud gate; engine-strict fails a below-floor Node install."
        return
    fi
    if ! command -v node >/dev/null 2>&1; then
        return   # node absence already hard-failed above; nothing to add here
    fi
    local probe_js probe_out rc=0
    # Resolve vite from the web-app package, resolve rolldown from vite, then import it.
    probe_js='
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
const req = createRequire(process.cwd() + "/packages/web-app/");
let vitePath;
try { vitePath = req.resolve("vite"); } catch (e) { console.error("resolve vite: " + e.message); process.exit(3); }
let entry;
try { entry = createRequire(vitePath).resolve("rolldown"); } catch (e) { console.error("resolve rolldown: " + e.message); process.exit(3); }
try { await import(pathToFileURL(entry).href); } catch (e) { console.error("load rolldown: " + (e && e.message || String(e))); process.exit(4); }
'
    # `|| rc=$?` CAPTURES the exit code for explicit handling below — it is not `|| true`;
    # a nonzero code flips HARD_FAIL via fail(), so the failure is surfaced, never masked.
    probe_out="$(node --input-type=module -e "$probe_js" 2>&1)" || rc=$?
    if [[ "$rc" -eq 0 ]]; then
        pass "bundler loads (vite + rolldown native binding present)"
        return
    fi
    if [[ "$rc" -eq 3 ]]; then
        fail "bundler probe: vite/rolldown not resolvable — node_modules looks incomplete."
        echo "      Detail: ${probe_out}"
        echo "      Fix (reinstall; Node is not the issue): rm -rf node_modules && pnpm install"
    else
        fail "bundler probe: rolldown could not load its native binding (@rolldown/binding-linux-x64-gnu)."
        echo "      Detail: ${probe_out}"
        echo "      Cause: node_modules was installed under a Node BELOW the workspace engines floor"
        echo "             (package.json engines.node = ${ENGINES_NODE:-see package.json}), so pnpm SILENTLY"
        echo "             skipped the engines-mismatched optional binding."
        echo "      This SUPERSEDES any 'same major, likely fine' Node note above — same major is not"
        echo "             sufficient; satisfying the engines floor is."
        echo "      Fix: ${NVM_LOAD}nvm install \"\$(cat .nvmrc)\" && rm -rf node_modules && pnpm install"
        echo "      (If 'node --version' already satisfies the floor, the binding was skipped by an earlier"
        echo "       below-floor install — 'rm -rf node_modules && pnpm install' alone is enough.)"
    fi
}

# ─── SSoT: expected versions read from the repo ─────────────────
EXPECTED_NODE="$(<.nvmrc)"                       # from .nvmrc (SSoT)
EXPECTED_NODE_MAJOR="${EXPECTED_NODE%%.*}"       # e.g. 22
# packageManager pin, e.g. "pnpm@10.33.2" → 10.33.2 (no node needed to parse)
EXPECTED_PNPM="$(grep -oE '"packageManager"[[:space:]]*:[[:space:]]*"pnpm@[^"]+"' package.json \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
# Root engines.node range — the SSoT Node floor the bundler's native binding needs
# (e.g. ">=22.13.0 <23"). Used ONLY in the bundler-probe remedy message, never as a
# version literal compared against here (that would fork the SSoT the drift-guard owns).
ENGINES_NODE="$(grep -oE '"node"[[:space:]]*:[[:space:]]*"[^"]+"' package.json \
    | grep -oE '"[^"]+"$' | tr -d '"' | head -1)"

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

# ─── Bundler load probe (AFTER the Node/pnpm version checks) ────
# Catches the engines-skipped native-binding class the major-only Node check above only
# WARNs on. node_modules-gated so a fresh clone is not false-failed. See F11 in
# docs/runbooks/client-dev-local.md.
probe_bundler

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

# ─── Cert fingerprints (HARD FAIL — the demonstrated audio path needs these) ──
# Was a WARN when the demo's success criterion was sign-up/create. It is now
# hearing your own audio back through MH, and the browser cannot open the
# WebTransport session to MC or MH without these fingerprints — so a warning
# here yields a demo that starts and silently returns no audio.
if [[ -s "$FINGERPRINTS_JSON" ]]; then
    pass "cert fingerprints present ($FINGERPRINTS_JSON)"
else
    fail "fingerprints missing ($FINGERPRINTS_JSON) — the browser cannot open the WebTransport session to MC or MH, so the demo would join and you would hear nothing. Not starting the dev server."
    echo "      Fix: scripts/generate-dev-certs.sh"
    echo "      Then re-run THIS script — Vite reads the fingerprints at config time, so a"
    echo "      dev server that is already running will never pick up regenerated certs."
    echo "      (Sign-up and create-meeting would still work: they are TCP through the Vite"
    echo "       proxy. That is exactly why this used to warn, and exactly why it no longer can.)"
fi

# ─── MC / MH WebTransport reachability (HARD FAIL — this IS the demo) ──
# The join step dials MC (signaling) then MH (media) over QUIC/WebTransport at
# the addresses GC advertises to the browser — read here from the service
# configmaps, the SSoT for those values. Two failure modes this catches that the
# AC/GC TCP probes above cannot: nothing published on the MC/MH WT port, and an
# IPv4/IPv6 loopback family mismatch.
#
# ALL FOUR instances hard-fail, MC as well as MH. The join dials MC for
# signaling BEFORE it dials MH for media, so an MC listener gap produces the
# same silent outcome the MH gap does — in fact "never joined" — behind a yellow
# `!` that scrolls past. Warning on MC while failing on MH would encode a
# distinction the failure mode does not have.
#
# The "why" for both — the WSL2 mirrored-networking address-family trap, and why
# AC/GC keep working while only the join dies — is owned by
# docs/runbooks/client-dev-local.md (§5 F1). Deliberately a pointer, not a copy:
# this script is the fast path, the runbook is the source of prose/diagnosis.
#
# KNOWN FALSE POSITIVE, accepted with a named diagnosis rather than softened:
# the advertise addresses are read from the committed configmap files on disk,
# which is correct for the static host topology this script targets. On a
# devloop cluster the live ConfigMap is patched and the on-disk value is stale,
# so this check can go RED against a perfectly healthy cluster — and now that is
# a blocker, not a misleading green. Every listener failure below therefore
# prints the ground-truth `kubectl` command for the live value. The honest fix
# if this ever bites in practice is to read the live ConfigMap, not to soften
# the check; that is recorded in docs/TODO.md. See runbook §1 and F8.
WT_CONFIGMAPS=(
    infra/services/mc-service/mc-0-configmap.yaml
    infra/services/mc-service/mc-1-configmap.yaml
    infra/services/mh-service/mh-0-configmap.yaml
    infra/services/mh-service/mh-1-configmap.yaml
)

# Snapshot UDP listeners once; note whether "localhost" prefers IPv6 (mirrored).
# WT_SS_OK distinguishes "ss ran and found no listener" from "ss failed to run".
# Without it a failing `ss` (restricted container, missing /proc/net) yields an
# empty snapshot, every port reads as unlistened, and all four instances report
# "nothing listening — is the cluster up?" — a HARD FAIL, so it does not pass
# silently, but it sends the reader to restart a healthy cluster. Same
# misdirecting-remedy class as the unprefixed jsonpath @operations caught in
# OPS-3: not wrong-and-quiet, but wrong-and-confident.
if command -v ss >/dev/null 2>&1; then
    WT_HAVE_SS=1
    if WT_LISTENERS="$(ss -uln 2>/dev/null)"; then
        WT_SS_OK=1
    else
        WT_SS_OK=0
        WT_LISTENERS=""
    fi
else
    WT_LISTENERS=""
    WT_HAVE_SS=0
    WT_SS_OK=0
fi
# `|| true` is load-bearing, not defensive clutter. This value is only a HINT
# used to explain the WSL2 IPv6-first mismatch below. Without the guard, a
# `getent` that is absent or fails makes the pipeline nonzero, `pipefail`
# propagates it, and `set -e` aborts the ENTIRE preflight right here — silently,
# with no message, in the middle of a script whose whole job is to fail loudly.
# The remaining checks would simply never run and the reader would see a
# truncated report that looks like it finished.
LOCALHOST_FIRST="$(getent ahosts localhost 2>/dev/null | awk 'NR==1 {print $1}' || true)"

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

# Ground-truth command for the live ConfigMap value, derived from the failing
# label so the operator never hand-edits it at 3am.
#
# The key is SERVICE-PREFIXED (`MH_WEBTRANSPORT_ADVERTISE_ADDRESS` /
# `MC_WEBTRANSPORT_ADVERTISE_ADDRESS`, see the configmap files). An unprefixed
# jsonpath does not error — it returns EMPTY, which reads as "the live
# ConfigMap has no advertise address either" and manufactures exactly the false
# conclusion this remedy exists to prevent. A remedy command that fails
# silently is worse than no remedy.
wt_live_value_cmd() {
    local label="$1" prefix
    prefix="$(printf '%s' "${label%%-*}" | tr '[:lower:]' '[:upper:]')"
    printf "kubectl get cm %s-config -n dark-tower -o jsonpath='{.data.%s_WEBTRANSPORT_ADVERTISE_ADDRESS}'" \
        "$label" "$prefix"
}

# Shared remedy block for "nothing is listening". Printed by every listener
# hard-fail: what broke, why it matters in the user's terms, and the next
# command. A hard fail with no remedy is the 3am failure mode.
wt_listener_remedy() {
    local label="$1"
    echo "      Fix: ./infra/kind/scripts/setup.sh          # bring the cluster up / republish"
    echo "      Check it really is up:  kubectl get pods -n dark-tower"
    echo "      On a DEVLOOP cluster this check reads a stale on-disk value — confirm against the live one:"
    echo "        $(wt_live_value_cmd "$label")"
    echo "      If that disagrees with the configmap file, the preflight is reporting stale data and"
    echo "      the cluster may be healthy (runbook §1, F8). This script targets the static host topology."
}

check_wt_endpoint() {
    local file="$1" label="$2" url hostport host port
    # `|| true` makes the not-found branch REACHABLE. Without it, `grep`
    # exiting 1 on no-match propagates through `pipefail` and `set -e` kills
    # the whole preflight at this assignment — so the "no advertise address"
    # case below could never run, and the script died silently mid-report
    # instead of failing loudly. Found by scripts/dev-web.test.sh; the branch
    # was unreachable before this change, which is why nobody noticed.
    #
    # The grep pattern is deliberately UNANCHORED: the real keys are
    # service-prefixed (MC_/MH_WEBTRANSPORT_ADVERTISE_ADDRESS), and an anchored
    # match would silently stop finding them.
    url="$(grep -E 'WEBTRANSPORT_ADVERTISE_ADDRESS' "$file" 2>/dev/null | grep -oE 'https://[^"]+' | head -1 || true)"
    if [[ -z "$url" ]]; then
        # DIFFERENT LANE from every other failure here, and it must say so.
        # Everything else means "your machine or your cluster is wrong" and is
        # operator-fixable. This means the configmap or this script's grep
        # pattern drifted — there is nothing to fix locally, and a reader who
        # gets the generic message spends an hour restarting a healthy cluster.
        fail "${label}: CANNOT VERIFY — no WEBTRANSPORT_ADVERTISE_ADDRESS found in ${file}."
        echo "      This is REPO/CONFIG DRIFT (the configmap key, or this script's parse), not your"
        echo "      environment. Nothing to fix locally: report it. Not starting the dev server,"
        echo "      because a check that did not run must never read as a check that passed."
        return
    fi
    hostport="${url#https://}"; port="${hostport##*:}"; host="${hostport%:*}"
    if [[ "$WT_HAVE_SS" -eq 1 && "$WT_SS_OK" -eq 0 ]]; then
        fail "${label}: CANNOT VERIFY — ${host}:${port} is advertised but 'ss' failed to run, so the listener snapshot is empty and every port would falsely read as unlistened."
        echo "      This is NOT 'the cluster is down' — do not restart it on this signal. Check 'ss -uln' by hand;"
        echo "      a restricted container or missing /proc/net is the usual cause."
        return
    fi
    if [[ "$WT_HAVE_SS" -eq 0 ]]; then
        # "Could not check" is a hard fail, not a pass. Otherwise not having a
        # package installed reproduces the exact silent no-audio failure this
        # escalation exists to close.
        fail "${label}: CANNOT VERIFY — ${host}:${port} is advertised but 'ss' is not installed, so no listener check ran."
        echo "      Fix: sudo apt-get install -y iproute2"
        echo "      (Hard fail by policy: an unverifiable critical-path check is not a passing one.)"
        return
    fi
    case "$host" in
        127.0.0.1|0.0.0.0)                       # IPv4 literal — browser dials IPv4
            if wt_listener_on "$port" 4; then
                pass "${label} WebTransport listener on ${host}:${port} (IPv4)"
            else
                fail "${label}: nothing listening on ${host}:${port} — the demo would join and you would hear nothing. Not starting the dev server."
                wt_listener_remedy "$label"
            fi ;;
        ::1|::)                                  # IPv6 literal
            if wt_listener_on "$port" 6; then
                pass "${label} WebTransport listener on [${host}]:${port} (IPv6)"
            else
                fail "${label}: nothing listening on [${host}]:${port} (IPv6) — the demo would join and you would hear nothing. Not starting the dev server."
                wt_listener_remedy "$label"
            fi ;;
        *)                                       # hostname — name resolution picks the family
            # SILENT-PASS FIX (found by sweeping @dry-reviewer's F-DRY-F method
            # into the bash dialect its Rust sweep could not reach).
            #
            # `LOCALHOST_FIRST` is empty when `getent` is absent or fails. The
            # IPv6-mismatch test below is `[[ "$LOCALHOST_FIRST" == ::* && ... ]]`,
            # which is simply FALSE when the value is empty — so the check could
            # not fire, control fell through to the `elif has4 || has6` arm, and
            # a host whose hostname resolves IPv6-first with only an IPv4
            # listener — a guaranteed QUIC timeout and silent no-audio — reported
            # PASS. "Could not evaluate" converted into a pass, which is exactly
            # the class F-DRY-F named, one dialect over.
            #
            # Not reachable on today's tree (every configmap advertises the IPv4
            # literal 127.0.0.1, so this branch is dead), which is precisely why
            # it would have sat here unnoticed until someone switched to a
            # hostname — the point at which it matters most.
            if [[ -z "$LOCALHOST_FIRST" ]]; then
                fail "${label}: CANNOT VERIFY — ${host}:${port} advertises a HOSTNAME, but the resolver could not be queried (getent unavailable), so the IPv4/IPv6 family mismatch that silently kills the join cannot be ruled out."
                echo "      Fix: install glibc's getent (package 'libc-bin'), or set ${host} → the IPv4 literal 127.0.0.1 in $file."
                return
            fi
            local has4=1 has6=1
            wt_listener_on "$port" 4 || has4=0
            wt_listener_on "$port" 6 || has6=0
            if [[ "$LOCALHOST_FIRST" == ::* && "$has6" -eq 0 && "$has4" -eq 1 ]]; then
                fail "${label} advertises hostname '${host}:${port}', which resolves to IPv6 (${LOCALHOST_FIRST}) under WSL2 mirrored mode, but only an IPv4 listener exists → QUIC times out and you hear nothing."
                echo "      Fix: set ${host} → the IPv4 literal 127.0.0.1 in $file (see the comment there), then re-run the cluster setup."
            elif [[ "$has4" -eq 1 || "$has6" -eq 1 ]]; then
                pass "${label} WebTransport listener on ${host}:${port}"
            else
                fail "${label}: nothing listening on ${host}:${port} — the demo would join and you would hear nothing. Not starting the dev server."
                wt_listener_remedy "$label"
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
