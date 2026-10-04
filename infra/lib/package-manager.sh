# package-manager.sh — sourced library; the ONE reader of the pnpm pin in the repo-root
# package.json `packageManager` field (`pnpm@X.Y.Z+sha512.<128 hex>`).
#
# Consumers: infra/devloop/devloop.sh (the devloop image's pnpm, verified by corepack against
# the hash) and scripts/dev-web.sh (the host preflight). CI reads the same field through
# pnpm/action-setup, which errors if a workflow also sets `version:`. Do not re-parse the field
# anywhere else; this is also the only place the `+sha512.` suffix is stripped.
#
# The hash is REQUIRED: a pin without it would let corepack fetch an unverified pnpm.
# Parsed with an anchored regex (no node/jq needed on the host); exactly one
# `"packageManager": "<value>"` occurrence must exist in the file.
#
# Usage: pnpm_package_manager_spec <package.json>   prints pnpm@X.Y.Z+sha512.<hex>
#        pnpm_version <package.json>                prints X.Y.Z
# Both fail (non-zero, message on stderr) on an unreadable file, a missing, duplicated or
# malformed field, or a missing hash.

__PNPM_SPEC_RE='^pnpm@([0-9]+\.[0-9]+\.[0-9]+)\+sha512\.[0-9a-f]{128}$'

pnpm_package_manager_spec() {
    local pkg="$1" lines count spec
    if [[ ! -r "${pkg}" ]]; then
        echo "ERROR: pnpm_package_manager_spec: cannot read ${pkg}" >&2
        return 1
    fi
    lines="$(grep -oE '"packageManager"[[:space:]]*:[[:space:]]*"[^"]*"' "${pkg}" || true)"
    count="$(grep -c . <<< "${lines}" || true)"
    if [[ "${count}" -ne 1 ]]; then
        echo "ERROR: pnpm_package_manager_spec: expected exactly one \"packageManager\" field in ${pkg}, found ${count}" >&2
        return 1
    fi
    spec="$(sed -E 's/^"packageManager"[[:space:]]*:[[:space:]]*"([^"]*)"$/\1/' <<< "${lines}")"
    if [[ ! "${spec}" =~ ${__PNPM_SPEC_RE} ]]; then
        echo "ERROR: pnpm_package_manager_spec: \"packageManager\" in ${pkg} is '${spec}', expected pnpm@X.Y.Z+sha512.<128 hex>" >&2
        return 1
    fi
    printf '%s\n' "${spec}"
}

pnpm_version() {
    local spec
    spec="$(pnpm_package_manager_spec "$1")" || return 1
    [[ "${spec}" =~ ${__PNPM_SPEC_RE} ]]
    printf '%s\n' "${BASH_REMATCH[1]}"
}
