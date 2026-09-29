#!/usr/bin/env bash
# memory-harvest.test.sh — self-test for infra/devloop/memory-harvest.sh, plus a
# static check that devloop.sh harvests before every removal of the dev container.
#
# A PATH `podman` stub models a container filesystem as a directory tree
# ($FAKE/<container>/...), so the real function runs unchanged.
#
# Wired into scripts/layer3.sh. Consumes scripts/lang/_test_helpers.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/../.." && pwd)"
# shellcheck source=../../scripts/lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FAKE="${WORK}/containers"; BIN="${WORK}/bin"; mkdir -p "$FAKE" "$BIN"
export DEVLOOP_MEMORY_INBOX="${WORK}/inbox"
cat > "${BIN}/podman" <<EOF
#!/usr/bin/env bash
case "\$1 \${2:-}" in
  "container exists") [[ -d "${FAKE}/\$3" ]]; exit ;;
esac
if [[ "\$1" == exec && "\$3" == mkdir ]]; then mkdir -p "${FAKE}/\$2\$5"; exit; fi
if [[ "\$1" == cp && "\$3" == *:* ]]; then
  dst="\${3%%:*}"; path="\${3#*:}"; cp -a "\$2" "${FAKE}/\${dst}\${path}"; exit
fi
if [[ "\$1" == cp ]]; then
  [[ -f "${FAKE}/cp-broken" ]] && { echo "Error: storage driver exploded" >&2; exit 125; }
  src="\${2%%:*}"; path="\${2#*:}"; path="\${path%/.}"
  [[ -d "${FAKE}/\${src}\${path}" ]] || { echo "Error: could not find \${path}: no such file or directory" >&2; exit 125; }
  cp -a "${FAKE}/\${src}\${path}/." "\$3"; exit
fi
echo "podman stub: unmodelled: \$*" >&2; exit 91
EOF
chmod +x "${BIN}/podman"
export PATH="${BIN}:${PATH}"
# shellcheck source=memory-harvest.sh
source "${__here}/memory-harvest.sh"
set +e

MEM="${DEVLOOP_CONTAINER_MEMORY_DIR}"
inbox() { find "${DEVLOOP_MEMORY_INBOX}" -type f 2>/dev/null | sed "s#${DEVLOOP_MEMORY_INBOX}/##" | sort | tr '\n' ' '; }

# No container: nothing to lose, success, nothing written.
harvest_container_memory devloop-gone-dev gone; assert_rc "absent-container-ok" 0 $?
assert_rc "absent-container-writes-nothing" 0 "$([[ -z "$(inbox)" ]] && echo 0 || echo "1 ($(inbox))")"

# Container without a memory directory: success, nothing written.
mkdir -p "${FAKE}/devloop-empty-dev/home/dev"
harvest_container_memory devloop-empty-dev empty; assert_rc "no-memory-dir-ok" 0 $?
assert_absent "no-memory-dir-writes-nothing" "empty/" "$(inbox)"

# Memories are copied under the slug; a second harvest merges and updates.
mkdir -p "${FAKE}/devloop-a-dev${MEM}"
printf -- '- [x](x.md) — x\n' > "${FAKE}/devloop-a-dev${MEM}/MEMORY.md"
printf 'first\n' > "${FAKE}/devloop-a-dev${MEM}/x.md"
out="$(harvest_container_memory devloop-a-dev a)"; assert_rc "copy-ok" 0 $?
assert_status "copy-reports-count" "Harvested 2 memory file(s)" "$out"
assert_status "copy-lands-under-slug" "a/MEMORY.md a/x.md" "$(inbox)"
printf 'second\n' > "${FAKE}/devloop-a-dev${MEM}/x.md"
printf 'y\n' > "${FAKE}/devloop-a-dev${MEM}/y.md"
harvest_container_memory devloop-a-dev a >/dev/null
assert_status "reharvest-adds-new" "a/y.md" "$(inbox)"
assert_status "reharvest-updates" "second" "$(cat "${DEVLOOP_MEMORY_INBOX}/a/x.md")"

# A copy failure other than "no such file" is a failure, and gates removal.
touch "${FAKE}/cp-broken"
harvest_container_memory devloop-a-dev a 2>/dev/null; assert_rc "copy-error-fails" 1 $?
out="$(harvest_before_removal devloop-a-dev a 2>&1)"; assert_rc "gate-refuses-on-error" 1 $?
assert_status "gate-says-refusing" "Refusing to remove devloop-a-dev" "$out"
out="$(DEVLOOP_SKIP_MEMORY_HARVEST=1 harvest_before_removal devloop-a-dev a 2>&1)"; assert_rc "gate-override-ok" 0 $?
assert_status "gate-override-warns" "without its Claude memories" "$out"
rm -f "${FAKE}/cp-broken"
harvest_before_removal devloop-a-dev a >/dev/null; assert_rc "gate-passes-on-success" 0 $?

# Seed: the host memory dir is copied into a new container; key derivation.
assert_status "host-dir-key" "/.claude/projects/-home-nathan-code-dark-tower/memory" "$(DEVLOOP_MEMORY_SEED_DIR='' host_memory_dir /home/nathan/code/dark_tower)"
SEED="${WORK}/seed"; mkdir -p "$SEED"
printf -- '- [k](k.md) — k\n' > "${SEED}/MEMORY.md"; printf 'keep\n' > "${SEED}/k.md"
mkdir -p "${FAKE}/devloop-s-dev/home/dev"
out="$(DEVLOOP_MEMORY_SEED_DIR="$SEED" seed_container_memory devloop-s-dev /repo)"; assert_rc "seed-ok" 0 $?
assert_status "seed-reports" "Seeded 2 memory file(s)" "$out"
assert_status "seed-lands-in-container" "keep" "$(cat "${FAKE}/devloop-s-dev${MEM}/k.md" 2>/dev/null)"
out="$(DEVLOOP_MEMORY_SEED_DIR="${WORK}/nope" seed_container_memory devloop-s-dev /repo 2>&1)"; assert_rc "seed-missing-host-dir-ok" 0 $?
assert_status "seed-missing-host-dir-warns" "no host Claude memory" "$out"

# Harvest with a repo root skips unchanged seeds; new and changed files land.
printf 'new\n' > "${FAKE}/devloop-s-dev${MEM}/n.md"
DEVLOOP_MEMORY_SEED_DIR="$SEED" harvest_container_memory devloop-s-dev s /repo >/dev/null
assert_absent "harvest-skips-unchanged-seed" "s/k.md" "$(inbox)"
assert_status "harvest-keeps-new" "s/n.md" "$(inbox)"
printf 'keep, edited\n' > "${FAKE}/devloop-s-dev${MEM}/k.md"
DEVLOOP_MEMORY_SEED_DIR="$SEED" harvest_container_memory devloop-s-dev s /repo >/dev/null
assert_status "harvest-keeps-changed-seed" "s/k.md" "$(inbox)"

# Static: every removal of the dev container in devloop.sh is preceded by the gate.
DL="${__here}/devloop.sh"
ungated="$(awk '
  /^[[:space:]]*#/ { next }
  /harvest_before_removal "\$DEV_CONTAINER"/ { gated = NR }
  /podman rm -f "\$DEV_CONTAINER"/ { if (NR - gated > 3) print NR }
' "$DL")"
assert_rc "devloop-every-dev-removal-gated" 0 "$([[ -z "$ungated" ]] && echo 0 || echo "1 (ungated removal at devloop.sh line(s): ${ungated//$'\n'/ })")"
removals="$(grep -c 'podman rm -f "\$DEV_CONTAINER"' "$DL")"
assert_rc "devloop-removals-found" 0 "$([[ "$removals" -ge 3 ]] && echo 0 || echo "1 (${removals})")"
# ...and a new container is seeded before it is used.
assert_rc "devloop-seeds-new-container" 0 "$(grep -q '^[[:space:]]*seed_container_memory "\$DEV_CONTAINER"' "$DL" && echo 0 || echo 1)"

report_results "infra/devloop/memory-harvest.test.sh"
