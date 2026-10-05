# rust-toolchain.sh — sourced library; the ONE reader of the pinned Rust version
# from the repo-root rust-toolchain.toml.
#
# Consumers: infra/devloop/devloop.sh (the devloop image's `FROM rust:<v>-…`) and
# infra/kind/scripts/deploy.sh (every first-party image's `ARG RUST_VERSION`). CI does
# not call it: rustup reads the file itself (`rustup toolchain install` with no
# argument). Do not re-derive the version anywhere else.
#
# Usage: rust_toolchain_version <rust-toolchain.toml>
# Prints the channel (X.Y.Z) on stdout. Fails (non-zero, message on stderr) when the
# file is unreadable or is not EXACTLY the shape this repo uses:
#   - one `[toolchain]` table and no other table;
#   - keys from the allow-list {channel, profile, components} only, each at most once.
#     An allow-list, not a deny-list: rustup honours more keys than this reader
#     looks at, and `path = "…"` makes rustup run binaries from a local directory as
#     the toolchain — a file carrying it would bypass the pin while still reporting a
#     clean channel;
#   - `channel = "X.Y.Z"` exactly (whole-line match): `stable`, `nightly`, `1.99`,
#     single quotes and trailing comments are all rejected. X.Y.Z is what makes the
#     value usable as the `rust:<v>-slim-bookworm` image tag without floating.

rust_toolchain_version() {
    local file="$1" line key tables=0 channel="" n=0
    local -A seen=()
    if [[ ! -r "${file}" ]]; then
        echo "ERROR: rust_toolchain_version: cannot read ${file}" >&2
        return 1
    fi
    while IFS= read -r line || [[ -n "${line}" ]]; do
        n=$((n + 1))
        [[ "${line}" =~ ^[[:space:]]*(#.*)?$ ]] && continue
        if [[ "${line}" =~ ^\[.*\]$ ]]; then
            if [[ "${line}" != "[toolchain]" || "${tables}" -ne 0 ]]; then
                echo "ERROR: rust_toolchain_version: ${file}:${n}: only one [toolchain] table is allowed (got ${line@Q})" >&2
                return 1
            fi
            tables=1
            continue
        fi
        if [[ "${tables}" -ne 1 ]]; then
            echo "ERROR: rust_toolchain_version: ${file}:${n}: key outside [toolchain]: ${line@Q}" >&2
            return 1
        fi
        key="${line%%[[:space:]=]*}"
        case "${key}" in
            channel|profile|components) ;;
            *)
                echo "ERROR: rust_toolchain_version: ${file}:${n}: key ${key@Q} is not allowed (allow-list: channel, profile, components)" >&2
                return 1
                ;;
        esac
        if [[ -n "${seen[${key}]:-}" ]]; then
            echo "ERROR: rust_toolchain_version: ${file}:${n}: duplicate key ${key@Q}" >&2
            return 1
        fi
        seen[${key}]=1
        if [[ "${key}" == channel ]]; then
            if [[ ! "${line}" =~ ^channel\ =\ \"([0-9]+\.[0-9]+\.[0-9]+)\"$ ]]; then
                echo "ERROR: rust_toolchain_version: ${file}:${n}: expected channel = \"X.Y.Z\" exactly (got ${line@Q})" >&2
                return 1
            fi
            channel="${BASH_REMATCH[1]}"
        fi
    done < "${file}"
    if [[ -z "${channel}" ]]; then
        echo "ERROR: rust_toolchain_version: no \`channel\` in ${file}" >&2
        return 1
    fi
    printf '%s\n' "${channel}"
}
