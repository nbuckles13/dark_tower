#!/usr/bin/env bash
# claude-token.sh — the long-lived Claude token that devloop.sh uses in TOKEN MODE.
#
# Run it directly to mint (or rotate) the token:
#   infra/devloop/claude-token.sh
# It runs `claude setup-token` (browser sign-in), asks you to paste the token it
# printed (input hidden, so it never reaches shell history or scrollback) and to
# confirm its expiry, then writes the token file atomically with mode 600 and
# validates it with the same reader devloop.sh uses. After ROTATING, recreate any
# live devloop container so it picks the new token up:
#   infra/devloop/devloop.sh --recreate <slug>
#
# devloop.sh SOURCES this file, which only defines variables and functions. This
# is the single home of the token file's path, format, reader and writer, so the
# format devloop.sh reads can never drift from the format written here.
#
# Why token mode exists: credentials mode COPIES the host's
# ~/.claude/.credentials.json into the container, so host and container hold two
# copies of ONE OAuth session. Whichever refreshes second finds its refresh token
# already rotated away — which is what expired containers overnight and stopped
# unattended story runs. Token mode gives the container its own long-lived
# credential via CLAUDE_CODE_OAUTH_TOKEN, set at container creation and therefore
# inherited by EVERY `podman exec`: the interactive attach and run-story's
# headless `claude -p` alike. In token mode .credentials.json is neither mounted
# nor copied, so exactly one credential is present.
#
# File format (mode 600), parsed and never sourced:
#   token=sk-ant-oat01-...
#   expires=YYYY-MM-DD

CLAUDE_TOKEN_FILE="${DEVLOOP_CLAUDE_TOKEN_FILE:-${HOME}/.config/dark-tower/claude-oauth-token}"
TOKEN_WARN_DAYS="${DEVLOOP_TOKEN_WARN_DAYS:-30}"
# Refuse below this many days left: a multi-day story run started on a nearly
# expired token would fail mid-run, which is the failure token mode exists to end.
TOKEN_FAIL_DAYS="${DEVLOOP_TOKEN_FAIL_DAYS:-7}"
# TOKEN_MODE is read by devloop.sh, which sources this file.
# shellcheck disable=SC2034
TOKEN_MODE=false
CLAUDE_TOKEN_ROTATE_HINT="Rotate: run infra/devloop/claude-token.sh, then 'devloop.sh --recreate <slug>' for any live container."

# Validate an expiry string; on success print its epoch seconds. Strict format
# first: GNU date also accepts loose strings ("tomorrow", "next year") that would
# silently yield a plausible-looking expiry.
claude_token_expiry_epoch() {
    local expires="$1"
    [[ "$expires" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || return 1
    date -d "$expires" +%s 2>/dev/null
}

# Reader. No-op if the token file is absent (credentials mode). Otherwise
# validates it, exits loudly on any problem, and exports CLAUDE_CODE_OAUTH_TOKEN.
load_oauth_token() {
    [ -f "$CLAUDE_TOKEN_FILE" ] || return 0
    local d
    for d in "$TOKEN_WARN_DAYS" "$TOKEN_FAIL_DAYS"; do
        [[ "$d" =~ ^[0-9]+$ ]] || { echo "ERROR: DEVLOOP_TOKEN_WARN_DAYS / DEVLOOP_TOKEN_FAIL_DAYS must be non-negative integers (got '${d}')" >&2; exit 1; }
    done
    local perms
    perms="$(stat -c '%a' "$CLAUDE_TOKEN_FILE")"
    if [ "$perms" != "600" ] && [ "$perms" != "400" ]; then
        echo "ERROR: ${CLAUDE_TOKEN_FILE} has mode ${perms} but holds a long-lived credential." >&2
        echo "  Fix: chmod 600 ${CLAUDE_TOKEN_FILE}" >&2
        exit 1
    fi
    local token expires
    token="$(sed -n 's/^token=//p' "$CLAUDE_TOKEN_FILE" | head -n1)"
    expires="$(sed -n 's/^expires=//p' "$CLAUDE_TOKEN_FILE" | head -n1)"
    if [ -z "$token" ] || [[ "$token" =~ [[:space:]] ]]; then
        echo "ERROR: ${CLAUDE_TOKEN_FILE} has no valid 'token=' line (expected token=sk-ant-oat01-..., no whitespace)." >&2
        exit 1
    fi
    if [ -z "$expires" ]; then
        # The token is opaque (not a JWT), so its expiry cannot be read from it.
        # An unknown expiry is not treated as valid.
        echo "ERROR: ${CLAUDE_TOKEN_FILE} has no 'expires=YYYY-MM-DD' line." >&2
        echo "  ${CLAUDE_TOKEN_ROTATE_HINT}" >&2
        exit 1
    fi
    local exp_s now_s days_left
    if ! exp_s="$(claude_token_expiry_epoch "$expires")"; then
        echo "ERROR: ${CLAUDE_TOKEN_FILE}: cannot parse expires='${expires}' (use YYYY-MM-DD)." >&2
        exit 1
    fi
    now_s="$(date +%s)"
    days_left=$(( (exp_s - now_s) / 86400 ))
    if (( exp_s <= now_s )); then
        echo "ERROR: the Claude token in ${CLAUDE_TOKEN_FILE} EXPIRED on ${expires}." >&2
        echo "  ${CLAUDE_TOKEN_ROTATE_HINT}" >&2
        exit 1
    fi
    if (( days_left < TOKEN_FAIL_DAYS )); then
        echo "ERROR: the Claude token in ${CLAUDE_TOKEN_FILE} expires ${expires} (${days_left} days left)," >&2
        echo "  below the ${TOKEN_FAIL_DAYS}-day floor — a story run could expire mid-run." >&2
        echo "  ${CLAUDE_TOKEN_ROTATE_HINT}" >&2
        exit 1
    fi
    if (( days_left < TOKEN_WARN_DAYS )); then
        echo "WARNING: the Claude token in ${CLAUDE_TOKEN_FILE} expires ${expires} (${days_left} days left). Rotate soon." >&2
    fi
    export CLAUDE_CODE_OAUTH_TOKEN="$token"
    # shellcheck disable=SC2034  # read by devloop.sh
    TOKEN_MODE=true
    echo "Auth: token mode (${CLAUDE_TOKEN_FILE}; expires ${expires}, ${days_left} days left)"
}

# Writer. Atomic (temp file in the same directory, then rename) and mode 600 from
# creation, so there is never a moment where a partial or world-readable file
# exists. printf is a builtin, so the token never appears in a process's argv.
write_claude_token_file() {
    local token="$1" expires="$2" dir tmp
    dir="$(dirname "$CLAUDE_TOKEN_FILE")"
    (umask 077 && mkdir -p "$dir")
    tmp="$(mktemp "${dir}/.claude-oauth-token.XXXXXX")"
    chmod 600 "$tmp"
    printf 'token=%s\nexpires=%s\n' "$token" "$expires" > "$tmp"
    mv -f "$tmp" "$CLAUDE_TOKEN_FILE"
}

claude_token_mint() {
    command -v claude >/dev/null || { echo "ERROR: the claude CLI is not on PATH." >&2; exit 1; }
    [ -t 0 ] || { echo "ERROR: run this from an interactive terminal (it prompts for the token)." >&2; exit 1; }

    echo "Step 1/3: running 'claude setup-token' (opens a browser sign-in)..."
    claude setup-token
    echo

    local token
    printf 'Step 2/3: paste the token it printed (input hidden), then Enter: '
    IFS= read -rs token
    echo
    # Trim only leading/trailing whitespace (a paste artifact). Internal whitespace
    # means a broken paste and is rejected by the reader below, not silently fixed.
    token="${token#"${token%%[![:space:]]*}"}"
    token="${token%"${token##*[![:space:]]}"}"
    [ -n "$token" ] || { echo "ERROR: no token entered; nothing written." >&2; exit 1; }
    if [[ "$token" != sk-ant-oat01-* ]]; then
        echo "WARNING: the token does not start with 'sk-ant-oat01-'. Check you pasted all of it." >&2
    fi

    local default_exp expires
    default_exp="$(date -d '+365 days' +%F)"
    printf 'Step 3/3: expiry date setup-token stated, YYYY-MM-DD [%s]: ' "$default_exp"
    IFS= read -r expires
    expires="${expires:-$default_exp}"
    if ! claude_token_expiry_epoch "$expires" >/dev/null; then
        echo "ERROR: '${expires}' is not a valid YYYY-MM-DD date; nothing written." >&2
        exit 1
    fi

    local existed=false
    [ -f "$CLAUDE_TOKEN_FILE" ] && existed=true
    write_claude_token_file "$token" "$expires"
    # Round-trip through the SAME reader devloop.sh uses: what was written is what
    # will be accepted.
    load_oauth_token
    echo "Wrote ${CLAUDE_TOKEN_FILE}."
    if $existed; then
        echo "This REPLACED an existing token. The old one stays valid until it expires or you revoke"
        echo "it in your claude.ai account settings (do revoke it if you rotated because of exposure)."
    fi
    echo "Recreate any live devloop container to use it: infra/devloop/devloop.sh --recreate <slug>"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    set -euo pipefail
    if (( $# )); then echo "ERROR: claude-token.sh takes no arguments (path override: DEVLOOP_CLAUDE_TOKEN_FILE)." >&2; exit 2; fi
    claude_token_mint
fi
