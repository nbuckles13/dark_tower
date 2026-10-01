#!/usr/bin/env bash
# coverage-exempt: drives validate-dev-cluster-verbs.sh (a shell guard) against fixture trees; it never runs the dt-guard binary via $DT_GUARD
# validate-dev-cluster-verbs.test.sh — self-test for scripts/guards/simple/validate-dev-cluster-verbs.sh.
#
# The guard passing on the real tree proves nothing about its FAILURE branches, so each rule
# is driven red against a synthetic git tree (through the DEVLOOP_TEST-gated
# DT_VERBS_GUARD_ROOT seam), next to a pristine tree that must pass — the positive control
# that a red is caused by the planted defect and nothing else. Wired into scripts/layer3.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # scripts/guards
REPO_ROOT="$(cd "${__here}/../.." && pwd)"
GUARD="${__here}/simple/validate-dev-cluster-verbs.sh"
# shellcheck source=../lang/_test_helpers.sh
source "${__here}/../lang/_test_helpers.sh"
set +e

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A minimal tree carrying every scan root, both allowlist copies, the three Kind scripts'
# flag arms and every enumerated remedy site.
make_tree() {
  local t="$1"
  rm -rf "$t"; mkdir -p "$t"
  mkdir -p "$t/crates/devloop-helper/src" "$t/crates/env-tests/src/fixtures" "$t/infra/devloop" \
    "$t/infra/kind/scripts" "$t/scripts" "$t/packages" "$t/.claude" "$t/docs/runbooks" \
    "$t/docs/specialist-knowledge" "$t/docs/decisions" "$t/docs/devloop-outputs"
  printf 'pub const VERBS: &[&str] = &["provision", "deploy", "teardown", "status"];\n' \
    > "$t/crates/devloop-helper/src/protocol.rs"
  printf '#!/bin/bash\nVERBS="provision deploy teardown status"\n' > "$t/infra/devloop/dev-cluster"
  for s in setup provision deploy; do
    printf 'while :; do\n  case "$1" in\n    --yes) shift ;;\n    --help) exit 0 ;;\n  esac\ndone\n' \
      > "$t/infra/kind/scripts/${s}.sh"
  done
  printf '    --provision-org) shift 2 ;;\n' >> "$t/infra/kind/scripts/setup.sh"
  printf 'echo "Provision it first: '"'"'dev-cluster provision'"'"'"\n' >> "$t/infra/kind/scripts/deploy.sh"
  printf 'pub const REDEPLOY_HINT: &str = "`dev-cluster deploy` or `./infra/kind/scripts/deploy.sh`";\n' \
    > "$t/crates/env-tests/src/fixtures/kube.rs"
  printf '| cluster-deploy-failed | `dev-cluster provision` then `dev-cluster deploy` |\n' \
    > "$t/docs/runbooks/devloop-validation.md"
  printf 'fix="re-run '"'"'dev-cluster provision'"'"'"\n' > "$t/scripts/layer7.sh"
  printf 'Host: `setup.sh --provision-org <sub>`, `deploy.sh --yes`.\n' > "$t/docs/LOCAL_DEVELOPMENT.md"
  printf '| `provision` | write |\n' > "$t/docs/decisions/adr-0030-host-side-cluster-helper.md"
  mkdir -p "$t/scripts/guards/simple"
  printf '# retired-spelling allowlist (fixture)\n' > "$t/scripts/guards/simple/dev-cluster-verbs-allow.txt"
  ( cd "$t" && git init -q && git add -A && git -c user.email=t@t -c user.name=t commit -qm t )
}

run_guard() {  # $1 = tree. Sets G_RC / G_OUT.
  G_OUT="$(DEVLOOP_TEST=1 DT_VERBS_GUARD_ROOT="$1" bash "$GUARD" 2>&1)"; G_RC=$?
}

T="${WORK}/tree"

# (1) Pristine: passes, and actually checked something.
make_tree "$T"; run_guard "$T"
assert_rc "pristine-passes" 0 "$G_RC"
assert_status "pristine-status-ok" "STATUS=OK REASON=dev-cluster-vocabulary-live" "$G_OUT"

# (2) A retired verb in a runbook's inline code reds, naming the file and the verb.
make_tree "$T"; printf 'Run `dev-cluster rebuild-all` to converge.\n' >> "$T/docs/runbooks/devloop-validation.md"
run_guard "$T"
assert_rc "retired-verb-md-reds" 1 "$G_RC"
assert_status "retired-verb-md-named" "docs/runbooks/devloop-validation.md: 'dev-cluster rebuild-all'" "$G_OUT"

# (2b) ...and in a shell string, and after a chain operator in a fenced block.
make_tree "$T"; printf 'echo "then dev-cluster teardown && dev-cluster setup"\n' >> "$T/scripts/layer7.sh"
run_guard "$T"
assert_rc "retired-verb-chained-code-reds" 1 "$G_RC"
assert_status "retired-verb-chained-code-named" "'dev-cluster setup'" "$G_OUT"
make_tree "$T"; printf '```bash\ndev-cluster teardown && dev-cluster rebuild <svc>\n```\n' >> "$T/docs/runbooks/devloop-validation.md"
run_guard "$T"
assert_rc "retired-verb-fence-reds" 1 "$G_RC"
assert_status "retired-verb-fence-named" "'dev-cluster rebuild'" "$G_OUT"

# (3) Prose and diagrams are NOT commands (no false positive).
make_tree "$T"
printf 'The dev-cluster helper runs on the host.\n```\n└── dev-cluster client -> socket\n```\n' \
  >> "$T/docs/runbooks/devloop-validation.md"
printf '# the dev-cluster client (infra/devloop/dev-cluster) even though the helper\n' >> "$T/scripts/layer7.sh"
run_guard "$T"
assert_rc "prose-is-not-a-command" 0 "$G_RC"

# (4) A retired Kind-script flag reds; a live one does not.
make_tree "$T"; printf 'Rebuild one: `./infra/kind/scripts/setup.sh --only gc`.\n' >> "$T/docs/runbooks/devloop-validation.md"
run_guard "$T"
assert_rc "retired-flag-reds" 1 "$G_RC"
assert_status "retired-flag-named" "'setup.sh --only' is not a flag" "$G_OUT"
make_tree "$T"; printf 'Deploy: `deploy.sh --provision-org x`.\n' >> "$T/docs/runbooks/devloop-validation.md"
run_guard "$T"
assert_rc "flag-of-another-script-reds" 1 "$G_RC"
assert_status "flag-of-another-script-named" "'deploy.sh --provision-org'" "$G_OUT"

# (5) The client's allowlist copy drifting from the helper's reds.
make_tree "$T"; printf '#!/bin/bash\nVERBS="provision deploy teardown status setup"\n' > "$T/infra/devloop/dev-cluster"
( cd "$T" && git add -A && git -c user.email=t@t -c user.name=t commit -qm drift )
run_guard "$T"
assert_rc "client-helper-mismatch-reds" 1 "$G_RC"
assert_status "client-helper-mismatch-named" "differ from the helper's" "$G_OUT"
# ...and an unreadable SSoT is itself a failure, never a vacuous pass.
make_tree "$T"; printf '// no verbs here\n' > "$T/crates/devloop-helper/src/protocol.rs"
run_guard "$T"
assert_rc "ssot-unreadable-reds" 1 "$G_RC"
assert_status "ssot-unreadable-named" "the SSoT could not be read" "$G_OUT"

# (6) A remedy site that stops spelling a live command reds (>= 1 per site).
make_tree "$T"; printf 'pub const REDEPLOY_HINT: &str = "converge the cluster";\n' > "$T/crates/env-tests/src/fixtures/kube.rs"
run_guard "$T"
assert_rc "empty-remedy-site-reds" 1 "$G_RC"
assert_status "empty-remedy-site-named" "remedy site crates/env-tests/src/fixtures/kube.rs" "$G_OUT"

# (7) A missing scan root is a FAILURE, never "0 hits".
make_tree "$T"; rm -rf "$T/packages"
run_guard "$T"
assert_rc "missing-root-reds" 1 "$G_RC"
assert_status "missing-root-named" "scan root 'packages' does not exist" "$G_OUT"

# (8) History is out of scope: a dated record quoting a retired verb stays green.
make_tree "$T"; printf 'We ran `dev-cluster rebuild-all`.\n' > "$T/docs/devloop-outputs/old.md"
( cd "$T" && git add -A && git -c user.email=t@t -c user.name=t commit -qm hist )
run_guard "$T"
assert_rc "history-out-of-scope" 0 "$G_RC"

# (11) Retired spellings in bare prose (rule 5): reds, names path:line and token.
commit_tree() { ( cd "$1" && git add -A && git -c user.email=t@t -c user.name=t commit -qm x ); }
make_tree "$T"; printf 'Usually a rollout still settling after `rebuild-all`.\n' >> "$T/docs/runbooks/devloop-validation.md"
commit_tree "$T"; run_guard "$T"
assert_rc "retired-prose-reds" 1 "$G_RC"
assert_status "retired-prose-named" "docs/runbooks/devloop-validation.md:2: retired spelling 'rebuild-all'" "$G_OUT"
make_tree "$T"; printf '# (reason=setup-poll-timeout)\n' >> "$T/infra/kind/scripts/deploy.sh"
commit_tree "$T"; run_guard "$T"
assert_rc "retired-token-in-code-reds" 1 "$G_RC"
assert_status "retired-token-in-code-named" "retired spelling 'setup-poll-timeout'" "$G_OUT"
# (11b) An exact allow entry admits it; the same token on a line WITHOUT the context does not.
make_tree "$T"; printf 'echo "the retired rebuild-all verb is rejected"\n' >> "$T/scripts/layer7.sh"
printf 'scripts/layer7.sh|rebuild-all|the retired rebuild-all verb|fixture: a rejection message\n' >> "$T/scripts/guards/simple/dev-cluster-verbs-allow.txt"
commit_tree "$T"; run_guard "$T"
assert_rc "allowlisted-retired-spelling-passes" 0 "$G_RC"
printf 'echo "run rebuild-all"\n' >> "$T/scripts/layer7.sh"; commit_tree "$T"; run_guard "$T"
assert_rc "allow-entry-is-not-a-path-wide-suppression" 1 "$G_RC"
assert_status "allow-entry-context-miss-named" "scripts/layer7.sh:3: retired spelling 'rebuild-all'" "$G_OUT"
# (11c) A stale allow entry (matches no line) reds; so does a malformed one and a missing file.
make_tree "$T"; printf 'scripts/layer7.sh|rebuild-all|no such line|fixture\n' >> "$T/scripts/guards/simple/dev-cluster-verbs-allow.txt"
commit_tree "$T"; run_guard "$T"
assert_rc "stale-allow-entry-reds" 1 "$G_RC"
assert_status "stale-allow-entry-named" "stale ALLOW entry" "$G_OUT"
make_tree "$T"; printf 'scripts/layer7.sh|rebuild-all\n' >> "$T/scripts/guards/simple/dev-cluster-verbs-allow.txt"
commit_tree "$T"; run_guard "$T"
assert_rc "malformed-allow-entry-reds" 1 "$G_RC"
assert_status "malformed-allow-entry-named" "malformed ALLOW entry" "$G_OUT"
make_tree "$T"; rm "$T/scripts/guards/simple/dev-cluster-verbs-allow.txt"; commit_tree "$T"; run_guard "$T"
assert_rc "missing-allow-file-reds" 1 "$G_RC"
assert_status "missing-allow-file-named" "dev-cluster-verbs-allow.txt is missing" "$G_OUT"

# (12) guard_seam_root (scripts/guards/common.sh): the override needs DEVLOOP_TEST EXACTLY "1"
# and a non-empty variable; everything else yields the real root.
seam() { bash -c 'source "$1"; guard_seam_root DT_SEAM_FIXTURE_ROOT /real' _ "${__here}/common.sh"; }
assert_rc "seam-sentinel-1-var-set-overrides" 0 "$([[ "$(DEVLOOP_TEST=1 DT_SEAM_FIXTURE_ROOT=/fake seam)" == /fake ]] && echo 0 || echo 1)"
assert_rc "seam-sentinel-0-stays-real" 0 "$([[ "$(DEVLOOP_TEST=0 DT_SEAM_FIXTURE_ROOT=/fake seam)" == /real ]] && echo 0 || echo 1)"
assert_rc "seam-sentinel-unset-stays-real" 0 "$([[ "$(env -u DEVLOOP_TEST DT_SEAM_FIXTURE_ROOT=/fake bash -c 'source "$1"; guard_seam_root DT_SEAM_FIXTURE_ROOT /real' _ "${__here}/common.sh")" == /real ]] && echo 0 || echo 1)"
assert_rc "seam-sentinel-1-empty-var-stays-real" 0 "$([[ "$(DEVLOOP_TEST=1 DT_SEAM_FIXTURE_ROOT= seam)" == /real ]] && echo 0 || echo 1)"
assert_rc "seam-sentinel-1-unset-var-stays-real" 0 "$([[ "$(DEVLOOP_TEST=1 env -u DT_SEAM_FIXTURE_ROOT bash -c 'source "$1"; guard_seam_root DT_SEAM_FIXTURE_ROOT /real' _ "${__here}/common.sh")" == /real ]] && echo 0 || echo 1)"

# (9) Positive control against the REAL files: both extractions produce the exact verb set.
want="cancel deploy provision recreate restore-kubeconfig status teardown"
helper_real="$(grep -m1 -E '^pub const VERBS: &\[&str\] = &\[' "${REPO_ROOT}/crates/devloop-helper/src/protocol.rs" | grep -oE '"[a-z][a-z-]*"' | tr -d '"' | sort | tr '\n' ' ')"
client_real="$(grep -m1 -E '^VERBS="[a-z -]+"$' "${REPO_ROOT}/infra/devloop/dev-cluster" | sed -E 's/^VERBS="(.*)"$/\1/' | tr ' ' '\n' | sort | tr '\n' ' ')"
assert_rc "real-helper-verbs-extracted-exactly" 0 "$([[ "$helper_real" == "${want} " ]] && echo 0 || echo "1 (${helper_real})")"
assert_rc "real-client-verbs-extracted-exactly" 0 "$([[ "$client_real" == "${want} " ]] && echo 0 || echo "1 (${client_real})")"

# (10) The root seam is INERT without the test sentinel.
G_OUT="$(env -u DEVLOOP_TEST DT_VERBS_GUARD_ROOT="${WORK}/nowhere" bash "$GUARD" 2>&1)"
assert_absent "seam-inert-without-devloop-test" "${WORK}/nowhere" "$G_OUT"

report_results "scripts/guards/validate-dev-cluster-verbs.test.sh"
