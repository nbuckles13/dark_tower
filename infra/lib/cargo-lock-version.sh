# cargo-lock-version.sh — sourced library; the ONE reader of a crate's resolved
# version from Cargo.lock.
#
# Consumers: infra/kind/scripts/deploy.sh (the db-migrate image's sqlx-cli) and
# infra/devloop/devloop.sh (the devloop image's sqlx-cli). Both pin sqlx-cli to
# the workspace's `sqlx`, so the CLI that writes `_sqlx_migrations` and the
# library that reads it (and `#[sqlx::test]`) agree on its shape and checksums.
# Do not re-derive the version anywhere else.
#
# Usage: cargo_lock_version <Cargo.lock> <crate>
# Prints the version on stdout. Fails (non-zero, message on stderr) when the
# lockfile is unreadable, the crate has no entry, or it resolves to more than
# one version — an ambiguous pin must never silently pick one. The match is on
# the exact `name = "<crate>"` line, so `sqlx` never matches `sqlx-core`.

cargo_lock_version() {
    local lock="$1" crate="$2" versions count
    if [[ ! -r "${lock}" ]]; then
        echo "ERROR: cargo_lock_version: cannot read ${lock}" >&2
        return 1
    fi
    versions="$(awk -v want="name = \"${crate}\"" '
        $0 == want { hit = 1; next }
        hit && /^version = "/ { v = $0; sub(/^version = "/, "", v); sub(/"$/, "", v); print v; hit = 0; next }
        /^\[\[package\]\]/ { hit = 0 }
    ' "${lock}" | sort -u)"
    count="$(grep -c . <<< "${versions}" || true)"
    if [[ "${count}" -eq 0 ]]; then
        echo "ERROR: cargo_lock_version: no \`${crate}\` package in ${lock}" >&2
        return 1
    fi
    if [[ "${count}" -gt 1 ]]; then
        echo "ERROR: cargo_lock_version: \`${crate}\` resolves to more than one version in ${lock}: $(tr '\n' ' ' <<< "${versions}")" >&2
        return 1
    fi
    printf '%s\n' "${versions}"
}
