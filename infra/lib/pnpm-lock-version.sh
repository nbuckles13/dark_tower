# pnpm-lock-version.sh — sourced library; the ONE reader of a package's resolved version
# from pnpm-lock.yaml (the pnpm counterpart of cargo-lock-version.sh).
#
# Consumers: infra/devloop/devloop.sh (the devloop image's baked Playwright Chromium must
# match the `playwright` package the tests run, or Playwright refuses to launch it). CI
# needs no copy: it installs browsers through `pnpm exec playwright`, which resolves the
# same lockfile. Do not re-derive the version anywhere else.
#
# Usage: pnpm_lock_version <pnpm-lock.yaml> <package>
# Prints the version on stdout. Reads ONLY the keys of the top-level `packages:` section —
# `  <package>@<version>:` (or the quoted `  '<package>@<version>':` form; a `(peer…)`
# suffix is stripped) — so `playwright-core@…`, `'@vitest/browser-playwright@…(playwright@…)'`
# keys, `snapshots:` entries and importer `version:` lines never match. Fails (non-zero,
# message on stderr) when the lockfile is unreadable, the package has no entry, it resolves
# to more than one version (an ambiguous pin must never silently pick one), or the version
# is not `X.Y.Z[-pre]` (the value reaches a --build-arg and a RUN shell).

# ANCHOR (DRY): byte-identical copy in infra/devloop/playwright-install.sh (its build context
# cannot source infra/lib); scripts/setup.test.sh group (F) fails on drift.
__PNPM_LOCK_VERSION_RE='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'

pnpm_lock_version() {
    local lock="$1" pkg="$2" versions count v
    if [[ ! -r "${lock}" ]]; then
        echo "ERROR: pnpm_lock_version: cannot read ${lock}" >&2
        return 1
    fi
    versions="$(awk -v want="${pkg}@" '
        /^[^[:space:]#][^:]*:/ { section = $0; sub(/:.*/, "", section); next }
        section != "packages" { next }
        /^  [^ ]/ {
            key = substr($0, 3)
            sub(/:[[:space:]]*(\{\})?[[:space:]]*$/, "", key)
            gsub(/^'"'"'|'"'"'$/, "", key)
            if (substr(key, 1, length(want)) != want) next
            v = substr(key, length(want) + 1)
            sub(/\(.*/, "", v)
            print v
        }
    ' "${lock}" | sort -u)"
    count="$(grep -c . <<< "${versions}" || true)"
    if [[ "${count}" -eq 0 ]]; then
        echo "ERROR: pnpm_lock_version: no \`${pkg}\` package in the packages: section of ${lock}" >&2
        return 1
    fi
    if [[ "${count}" -gt 1 ]]; then
        echo "ERROR: pnpm_lock_version: \`${pkg}\` resolves to more than one version in ${lock}: $(tr '\n' ' ' <<< "${versions}")" >&2
        return 1
    fi
    v="${versions}"
    if [[ ! "${v}" =~ ${__PNPM_LOCK_VERSION_RE} ]]; then
        echo "ERROR: pnpm_lock_version: \`${pkg}\` version '${v}' in ${lock} is not X.Y.Z[-pre]" >&2
        return 1
    fi
    printf '%s\n' "${v}"
}
