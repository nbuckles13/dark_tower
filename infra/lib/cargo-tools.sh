# cargo-tools.sh — sourced library; the ONE reader of infra/cargo-tools.versions,
# the pin for every cargo tool a workflow or the devloop image installs.
#
# Consumers: infra/devloop/devloop.sh (the image's build args), infra/kind/scripts/
# deploy.sh (the service images' CARGO_CHEF_VERSION), .github/workflows/
# {ci,audit-scheduled,fuzz-nightly}.yml (taiki-e/install-action `tool@version`, or
# `cargo install --version`) and scripts/lang/rust/test.sh (the nextest drift check).
# Do not re-derive a version anywhere else.
#
# Usage: cargo_tool_version <cargo-tools.versions> <crate>
# Prints the version on stdout. Fails (non-zero, message on stderr) when the file
# is unreadable, a non-comment line is not exactly `<crate> X.Y.Z`, the crate has
# no entry, or it has more than one — a malformed or ambiguous pin never resolves.

cargo_tool_version() {
    local file="$1" want="$2" line n=0 found=""
    if [[ ! -r "${file}" ]]; then
        echo "ERROR: cargo_tool_version: cannot read ${file}" >&2
        return 1
    fi
    while IFS= read -r line || [[ -n "${line}" ]]; do
        n=$((n + 1))
        [[ "${line}" =~ ^[[:space:]]*(#.*)?$ ]] && continue
        if [[ ! "${line}" =~ ^([a-z0-9][a-z0-9_-]*)\ ([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
            echo "ERROR: cargo_tool_version: ${file}:${n}: expected \`<crate> X.Y.Z\` (got ${line@Q})" >&2
            return 1
        fi
        [[ "${BASH_REMATCH[1]}" == "${want}" ]] || continue
        if [[ -n "${found}" ]]; then
            echo "ERROR: cargo_tool_version: \`${want}\` is pinned more than once in ${file}" >&2
            return 1
        fi
        found="${BASH_REMATCH[2]}"
    done < "${file}"
    if [[ -z "${found}" ]]; then
        echo "ERROR: cargo_tool_version: no \`${want}\` pin in ${file}" >&2
        return 1
    fi
    printf '%s\n' "${found}"
}
