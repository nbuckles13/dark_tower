#!/usr/bin/env bash
# devloop-pr.test.sh — self-test for infra/devloop/devloop.sh argument handling and
# `--pr <n>` mode (fix an existing same-repo GitHub PR in place).
#
# Runs the REAL devloop.sh from a throwaway "host" repo whose `origin` is a bare fixture
# repo standing in for GitHub, so the clone lands in $WORK/<case>/worktrees/<slug>.
# Real git. PATH-stubbed `gh` (serves `pr view` / `pr list` from JSON fixtures through
# real jq, so devloop.sh's own --jq expressions are what is tested) and `podman` (models
# "image exists" + "container already running"; no-op exec/cp). Both stubs log argv and
# exit 91 on any call they do not model. PATH is the stubs plus a symlink farm of
# /usr/bin and /bin WITHOUT gh, kind, podman or cargo: no Kind means no helper build and
# no cluster — today's no-kind path — so the suite runs in seconds.
#
# Covers: every PR-mode refusal fires BEFORE any clone or podman call (each lookup-stage
# case also proves gh WAS asked, so it died at the intended check); the happy path (clone
# on a slash-containing head branch at the PR head, menu detects the PR, pushes to it,
# never offers create; re-attach; [r] re-exec keeps --pr; PR closed or gh failing
# mid-session); a rejected push PRINTS the absorb recovery with this run's values, runs nothing
# and runs no git with the clone as repo/cwd (git shim) — and the printed steps, followed
# verbatim against a Dependabot-style rewritten head, land the fix; the clone-branch mismatch refused in both modes (detached HEAD
# included) and a matching re-attach; and the commits-since-base listing in a clone with
# no local base branch (masked as "No new commits" before).
#
# Wired into scripts/layer3.sh. Consumes scripts/lang/_test_helpers.sh.
set -Eeuo pipefail
IFS=$'\n\t'
# CI shows only a self-test's STDOUT: an abort under set -e must say where, there.
# (Only under errexit: dl and friends run expected failures inside `set +e`.)
trap '__rc=$?; [[ $- == *e* ]] && echo "devloop-pr.test.sh: ABORTED at line ${LINENO} (rc=${__rc}): ${BASH_COMMAND}"' ERR

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/../.." && pwd)"
DEVLOOP="${__here}/devloop.sh"
# shellcheck source=../../scripts/lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

WORK="$(mktemp -d)"
# The PR-mode slug is unique per run: devloop.sh's per-slug runtime dir is the REAL
# /tmp/devloop-<slug> (shared with the helper binary, not relocatable), which --destroy
# removes — it must never be a live devloop's.
PS="dlprtest${$}${RANDOM}"
trap 'rm -rf "$WORK" "/tmp/devloop-${PS}"' EXIT
# devloop.sh creates it (0700) only with the Kind helper; made here so --destroy's removal is observable.
(umask 077 && mkdir -p "/tmp/devloop-${PS}")
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid
export GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid

# ── Tool farm: /usr/bin + /bin minus the tools this suite stubs or must not reach ──
SYS="${WORK}/sys"; mkdir -p "$SYS"
for f in /usr/bin/* /bin/*; do
  n="${f##*/}"
  case "$n" in gh|kind|podman|cargo) continue ;; esac
  # -L too: a dangling link (seen on CI runners) fails -e, and /bin/* repeats /usr/bin/* names.
  [[ -e "${SYS}/${n}" || -L "${SYS}/${n}" ]] || ln -s "$f" "${SYS}/${n}"
done
# Required tools not under /usr/bin or /bin (e.g. a runner image that installs jq elsewhere):
# link the one on PATH. Never a stubbed name. Loud (stdout, which CI shows) if still missing.
for t in git jq awk bash; do
  if [[ ! -x "${SYS}/${t}" ]] && p="$(command -v "$t" 2>/dev/null)" && [[ -n "$p" ]]; then ln -s "$p" "${SYS}/${t}"; fi
  [[ -x "${SYS}/${t}" ]] || { echo "FATAL: ${t} not found (looked in /usr/bin, /bin and PATH) — cannot run hermetically"; exit 1; }
done

BIN="${WORK}/bin"; NOGH="${WORK}/bin-nogh"; mkdir -p "$BIN" "$NOGH"
STUBLOG="${WORK}/log"; FIX="${WORK}/fixtures"; mkdir -p "$STUBLOG" "$FIX"

cat > "${BIN}/podman" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${STUBLOG}/podman.log"
case "\$1" in
  image) [[ "\$2" == exists ]] && exit 0 ;;
  ps) printf '%s\n' "\${STUB_RUNNING:-}"; exit 0 ;;
  container) [[ "\$2" == exists ]] && exit 1 ;;
  exec)
    # The interactive attach: run the case's "session" (commit, flip a fixture, ...).
    # Without the git shim: the session is the CONTAINER's git, not the host's.
    if [[ " \$* " == *" claude "* && -n "\${STUB_SESSION:-}" ]]; then PATH="${SYS}" bash "\$STUB_SESSION" >&2; fi
    # A \`-- <command>\` exec (not the claude attach) exits with STUB_EXEC_RC (default 0).
    if [[ " \$* " == *" -w /work "* && " \$* " != *" claude "* ]]; then exit "\${STUB_EXEC_RC:-0}"; fi
    exit 0 ;;
  inspect) printf '%s\n' "\${STUB_LABEL:-}"; exit 0 ;;
  cp) exit 0 ;;
esac
echo "podman stub: unmodelled: \$*" >&2; exit 91
EOF
cp "${BIN}/podman" "${NOGH}/podman"

cat > "${BIN}/gh" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${STUBLOG}/gh.log"
jqx=""; head=""; args=("\$@")
for ((i = 0; i < \${#args[@]}; i++)); do
  case "\${args[i]}" in --jq|-q) jqx="\${args[i+1]}" ;; --head) head="\${args[i+1]}" ;; esac
done
if [[ "\$1 \$2" == "pr view" ]]; then
  n="\$3"
  if [[ -f "${FIX}/pr-\${n}.fail" ]]; then cat "${FIX}/pr-\${n}.fail" >&2; exit 1; fi
  [[ -f "${FIX}/pr-\${n}.json" ]] || { echo "no pull requests found for #\${n}" >&2; exit 1; }
  jq -r "\$jqx" "${FIX}/pr-\${n}.json"; exit
fi
if [[ "\$1 \$2" == "pr list" ]]; then
  if [[ -f "${FIX}/list.fail" ]]; then cat "${FIX}/list.fail" >&2; exit 1; fi
  shopt -s nullglob; files=("${FIX}"/pr-*.json)
  ((\${#files[@]})) || { echo '[]' | jq -r "\$jqx"; exit; }
  jq -s --arg h "\$head" '[.[] | select(.headRefName == \$h and .state == "OPEN") | {number, url}]' "\${files[@]}" \
    | jq -r "\$jqx"; exit
fi
if [[ "\$1 \$2" == "repo view" ]]; then
  if [[ -f "${FIX}/visibility.fail" ]]; then echo "repo view failed" >&2; exit 1; fi
  cat "${FIX}/visibility" 2>/dev/null || echo PUBLIC; exit 0
fi
if [[ "\$1 \$2" == "pr create" ]]; then echo "https://github.com/acme/tower/pull/99"; exit 0; fi
echo "gh stub: unmodelled: \$*" >&2; exit 91
EOF
# git shim: every host git call devloop.sh makes is logged (cwd, GIT_DIR, GIT_WORK_TREE, argv),
# then the real git runs. clone_git_calls reads the log.
cat > "${BIN}/git" <<EOF
#!/usr/bin/env bash
printf 'cwd=%s|GIT_DIR=%s|GIT_WORK_TREE=%s|argv= %s\n' "\$PWD" "\${GIT_DIR:-}" "\${GIT_WORK_TREE:-}" "\$*" >> "${STUBLOG}/git.log"
exec "${SYS}/git" "\$@"
EOF
chmod +x "${BIN}/podman" "${BIN}/gh" "${NOGH}/podman" "${BIN}/git"
# clone_git_calls <slug> <log-text>: the logged git calls that had the clone as repo or cwd (cwd,
# GIT_DIR, GIT_WORK_TREE, -C, --git-dir, --work-tree). The clone path as a fetch SOURCE (from
# the host repo — fetch_from_clone) is not one of them.
clone_git_calls() {
  local w="worktrees/${1}"
  grep -E "^cwd=[^|]*${w}(/[^|]*)?\||\|GIT_DIR=[^|]*${w}|\|GIT_WORK_TREE=[^|]*${w}| -C [^ ]*${w}( |/|\$)|--(git-dir|work-tree)[= ][^ ]*${w}" <<< "$2" || true
}

HEADREF="dependabot/cargo/x-1.2.3"
C=""; ORIGIN=""; H=""; OUT=""; RC=0; HEAD_OID=""; MAIN_OID=""

# fixture <case>: bare origin (main + $HEADREF one commit ahead), host = clone of origin
# with only main local. Resets fixtures and stub logs.
fixture() {
  C="${WORK}/$1"; ORIGIN="${C}/origin.git"; H="${C}/host"
  rm -f "${FIX}"/* "${STUBLOG}"/*
  git init -q --bare --initial-branch=main "$ORIGIN"
  git -c init.defaultBranch=main init -q "${C}/seed"
  printf 'base\n' > "${C}/seed/f"; git -C "${C}/seed" add f; git -C "${C}/seed" commit -qm base
  git -C "${C}/seed" checkout -qb "$HEADREF"
  printf 'bump\n' > "${C}/seed/g"; git -C "${C}/seed" add g; git -C "${C}/seed" commit -qm "bump x to 1.2.3"
  git -C "${C}/seed" push -q "$ORIGIN" main "$HEADREF"
  HEAD_OID="$(git -C "${C}/seed" rev-parse HEAD)"; MAIN_OID="$(git -C "${C}/seed" rev-parse main)"
  git clone -q "$ORIGIN" "$H"
}
# mkpr <n> [jq assignments...]: an OPEN same-repo PR on $HEADREF -> main, then overrides.
mkpr() {
  local n="$1"; shift
  jq -n --argjson n "$n" --arg h "$HEADREF" --arg oid "$HEAD_OID" '{
    number: $n, state: "OPEN", isCrossRepository: false,
    headRepositoryOwner: {login: "acme"}, headRepository: {name: "tower"},
    headRefName: $h, headRefOid: $oid, baseRefName: "main",
    url: "https://github.com/acme/tower/pull/\($n)"}' > "${FIX}/pr-${n}.json"
  local a
  for a in "$@"; do jq "$a" "${FIX}/pr-${n}.json" > "${FIX}/tmp" && mv "${FIX}/tmp" "${FIX}/pr-${n}.json"; done
}
# dl <stdin> <path-bin> <slug-for-ps> args...: run the real devloop.sh from the host repo.
dl() {
  local input="$1" bin="$2" running="$3"; shift 3
  rm -f "${STUBLOG}"/*
  set +e
  OUT="$(cd "$H" && printf '%s' "$input" | env -i HOME="${C}/home" PATH="${bin}:${SYS}" \
      GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
      GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid \
      ANTHROPIC_API_KEY=dummy DEVLOOP_CONTAINER_MEMORY=1g STUB_RUNNING="devloop-${running}-dev" \
      STUB_SESSION="${STUB_SESSION:-}" STUB_EXEC_RC="${STUB_EXEC_RC:-0}" STUB_LABEL="${STUB_LABEL:-}" \
      bash "$DEVLOOP" "$@" 2>&1)"
  RC=$?
  set -e
}
log() { cat "${STUBLOG}/$1.log" 2>/dev/null || true; }

# Hostile clone (security S-R1): the container can write the clone's .git/config and hooks,
# and host git run WITH THE CLONE AS ITS REPO would execute them. hostile_session <clone>
# <subject> writes ${C}/session.sh, run as the "session": a commit, then a (non-empty) commit carrying a
# gpgsig header on top, then (last, so the session's own git calls don't trip them) hooks and
# config that would fire on a host-side push/log/status in the clone. Markers land in ${C}/markers.
MARKBIN="${WORK}/markbin"; mkdir -p "$MARKBIN"
for n in sshcmd gpg fsmon; do
  printf '#!/bin/sh\ntouch "$MARKERS/%s"\ncat >/dev/null 2>&1 || true\nexit 1\n' "$n" > "${MARKBIN}/${n}"; chmod +x "${MARKBIN}/${n}"
done
printf '#!/bin/sh\ntouch "$MARKERS/hook-$(basename "$0")"\n' > "${MARKBIN}/hook"; chmod +x "${MARKBIN}/hook"
hostile_session() {
  local cl="$1" subj="$2"
  export MARKERS="${C}/markers"; mkdir -p "$MARKERS"
  DECOY="${C}/decoy.git"; [[ -d "$DECOY" ]] || git init -q --bare "$DECOY"
  cat > "${C}/session.sh" <<EOF
set -e
cd $(printf '%q' "$cl")
printf '%s\\n' $(printf '%q' "$subj") >> work; git add work; git commit -qm $(printf '%q' "$subj")
printf '%s signed\\n' $(printf '%q' "$subj") >> work; git add work; tree=\$(git write-tree); parent=\$(git rev-parse HEAD)
sha=\$(printf 'tree %s\\nparent %s\\nauthor t <t@e> 1700000000 +0000\\ncommitter t <t@e> 1700000000 +0000\\ngpgsig -----BEGIN PGP SIGNATURE-----\\n \\n -----END PGP SIGNATURE-----\\n\\n%s signed\\n' "\$tree" "\$parent" $(printf '%q' "$subj") | git hash-object -t commit -w --stdin)
git update-ref "refs/heads/\$(git symbolic-ref --short HEAD)" "\$sha"
for h in pre-push post-checkout pre-commit; do cp $(printf '%q' "${MARKBIN}/hook") ".git/hooks/\$h"; done
git config remote.origin.pushurl $(printf '%q' "$DECOY")
git config core.sshCommand $(printf '%q' "${MARKBIN}/sshcmd")
git config log.showSignature true
git config gpg.program $(printf '%q' "${MARKBIN}/gpg")
git config core.fsmonitor $(printf '%q' "${MARKBIN}/fsmon")
EOF
}
# assert_not_hostile <label> <clone>: nothing planted fired, the decoy got nothing; then a
# POSITIVE CONTROL proves the plants are live (host git in the clone DOES fire them), and the
# clone is disarmed for later steps.
assert_not_hostile() {
  local label="$1" cl="$2"
  assert_no_marker "${label}-no-planted-hook-or-config-ran" "$MARKERS" "*"
  assert_status    "${label}-decoy-untouched" "0" "$(git -C "$DECOY" for-each-ref | wc -l | tr -d ' ')"
  MARKERS="$MARKERS" git -C "$cl" log -1 >/dev/null 2>&1 || true
  assert_marker    "${label}-control-plant-is-live" "$MARKERS" "gpg"
  rm -f "$MARKERS"/* "$cl"/.git/hooks/pre-push "$cl"/.git/hooks/post-checkout "$cl"/.git/hooks/pre-commit
  local k; for k in remote.origin.pushurl core.sshCommand log.showSignature gpg.program core.fsmonitor; do
    git -C "$cl" config --unset "$k" || true
  done
}
clone() { printf '%s' "${C}/worktrees/$1"; }

assert_rc "precondition-kind-not-resolvable" 1 "$(PATH="${BIN}:${SYS}" command -v kind >/dev/null && echo 0 || echo 1)"

# Usage text (the ONE USAGE_BASE/USAGE_PR pair) on a bare invocation.
fixture usage
dl "" "$BIN" x
assert_rc     "usage-rc" 1 "$RC"
assert_status "usage-both-forms" "<task-slug> [base-branch] [-- <command...>] | devloop.sh [--rebuild] [--recreate] --pr <number> <task-slug>" "$OUT"

# ── Refused at argument parsing: gh never asked, nothing created ────────────────
refused_at_args() {  # <label> <reason> args...
  local label="$1" reason="$2"; shift 2
  fixture "$label"
  dl "" "$BIN" x "$@"
  assert_rc     "${label}-rc" 1 "$RC"
  assert_status "${label}-token" "PR_MODE_REFUSED REASON=${reason}" "$OUT"
  assert_rc     "${label}-gh-not-called" 0 "$([[ -z "$(log gh)" ]] && echo 0 || echo 1)"
  assert_rc     "${label}-podman-not-called" 0 "$([[ -z "$(log podman)" ]] && echo 0 || echo 1)"
  assert_rc     "${label}-no-clone" 1 "$([[ -e "${C}/worktrees" ]] && echo 0 || echo 1)"
}
refused_at_args arg-no-value      bad-pr-number --pr
refused_at_args arg-non-numeric   bad-pr-number --pr abc ${PS}
refused_at_args arg-zero          bad-pr-number --pr 0 ${PS}
refused_at_args arg-gh-option     bad-pr-number --pr -R ${PS}
refused_at_args arg-twice         pr-given-twice --pr 7 --pr 8 ${PS}
refused_at_args arg-with-base     with-base-branch --pr 7 ${PS} main
refused_at_args arg-with-destroy  with-destroy --pr 7 --destroy ${PS}
refused_at_args arg-with-refresh  with-refresh-creds --pr 7 --refresh-creds ${PS}
refused_at_args arg-no-slug       no-slug --pr 7
refused_at_args arg-rebuild-noslug no-slug --rebuild --pr 7

# ── Refused after the lookup: gh asked (pinned to origin), nothing created ──────
refused_at_lookup() {  # <label> <reason> <expected-substring> [mkpr overrides...]  ($PRE runs after fixture)
  local label="$1" reason="$2" want="$3"; shift 3
  fixture "$label"
  [[ "${NOPR:-}" == 1 ]] || mkpr 7 "$@"
  [[ -z "${PRE:-}" ]] || eval "$PRE"
  dl "" "${BINSEL:-$BIN}" ${PS} --pr 7 ${PS}
  assert_rc     "${label}-rc" 1 "$RC"
  assert_status "${label}-token" "PR_MODE_REFUSED REASON=${reason} PR=7" "$OUT"
  assert_status "${label}-detail" "$want" "$OUT"
  [[ "$reason" == gh-missing ]] || assert_status "${label}-gh-asked-pinned" "pr view 7 -R ${ORIGIN} --json" "$(log gh)"
  assert_rc     "${label}-podman-not-called" 0 "$([[ -z "$(log podman)" ]] && echo 0 || echo 1)"
  assert_rc     "${label}-no-new-clone" 1 "$([[ -e "$(clone ${PS})/.git" && -z "${KEEP_CLONE:-}" ]] && echo 0 || echo 1)"
}
NOPR=1 refused_at_lookup lk-not-found lookup-failed "no pull requests found for #7"
PRE='printf "HTTP 401: Bad credentials\n" > "${FIX}/pr-7.fail"' \
  refused_at_lookup lk-auth lookup-failed "HTTP 401: Bad credentials"
BINSEL="$NOGH" refused_at_lookup lk-gh-missing gh-missing "needs the GitHub CLI"
refused_at_lookup lk-closed  not-open "PR #7 is CLOSED" '.state = "CLOSED"'
refused_at_lookup lk-merged  not-open "PR #7 is MERGED" '.state = "MERGED"'
refused_at_lookup lk-fork-true fork "isCrossRepository=true" '.isCrossRepository = true'
refused_at_lookup lk-fork-null fork "isCrossRepository=null" '.isCrossRepository = null'
refused_at_lookup lk-fork-absent fork "isCrossRepository=null" 'del(.isCrossRepository)'
refused_at_lookup lk-fork-owner fork "head is mallory/tower" '.headRepositoryOwner.login = "mallory"'
refused_at_lookup lk-bad-url unexpected-url "expected https://" '.url = "https://github.com/acme/tower/pull/77"'
refused_at_lookup lk-bad-oid unexpected-head-oid "head commit 'abc'" '.headRefOid = "abc"'
i=0
for bad in '+evil' '-x' '@' 'a..b' 'x$(id)' 'x`id`' 'x.lock' 'x/' '.hidden'; do
  i=$((i + 1))
  refused_at_lookup "lk-unsafe-head-${i}" unsafe-ref "head branch '${bad}'" ".headRefName = \"${bad//\"/\\\"}\""
done
refused_at_lookup lk-unsafe-base unsafe-ref "base branch '+main'" '.baseRefName = "+main"'
PRE='git clone -q "$H" "$(clone ${PS})"' KEEP_CLONE=1 \
  refused_at_lookup lk-clone-other-branch clone-branch-mismatch "ON=main EXPECTED=${HEADREF}"
assert_status "lk-clone-other-branch-switch-hint" "podman exec -it -w /work devloop-${PS}-dev git switch ${HEADREF}" "$OUT"
assert_status "lk-clone-other-branch-start-hint" "start them first: podman start devloop-${PS}-dev devloop-${PS}-db" "$OUT"
assert_absent "lk-clone-other-branch-no-host-git-hint" "git -C $(clone ${PS}) switch" "$OUT"
assert_status "lk-clone-other-branch-destroy-hint" "--destroy ${PS}" "$OUT"
assert_status "lk-clone-other-branch-clone-untouched" "main" "$(git -C "$(clone ${PS})" symbolic-ref --short HEAD)"
PRE='git clone -q "$H" "$(clone ${PS})"; git -C "$(clone ${PS})" checkout -q --detach' KEEP_CLONE=1 \
  refused_at_lookup lk-clone-detached clone-branch-mismatch "detached HEAD"
PRE='git -C "$H" branch -q "$HEADREF" main' \
  refused_at_lookup lk-stale-local local-branch-stale "local ${HEADREF} is "
# (OIDs are asserted after the call: the arguments above expand BEFORE the case's fixture exists.)
assert_status "lk-stale-local-names-both-shas" "is ${MAIN_OID}; PR #7 head is ${HEAD_OID}" "$OUT"
assert_status "lk-stale-local-not-overwritten" "$MAIN_OID" "$(git -C "$H" rev-parse "$HEADREF")"
assert_status "lk-stale-local-inspect" "Inspect: git -C ${H} fetch origin && git -C ${H} log --oneline origin/${HEADREF}..${HEADREF}" "$OUT"
assert_status "lk-stale-local-delete-if-empty" "If that prints nothing: git -C ${H} branch -D ${HEADREF}" "$OUT"
assert_status "lk-stale-local-just-behind" "If it is just behind: git -C ${H} fetch origin refs/heads/${HEADREF}:refs/heads/${HEADREF}" "$OUT"
PRE='git -C "$H" remote set-url origin "${C}/nowhere.git"; ORIGIN="${C}/nowhere.git"' \
  refused_at_lookup lk-origin-unreachable origin-unreachable "git ls-remote "
assert_absent "lk-origin-unreachable-not-absent" "head-not-on-origin" "$OUT"
refused_at_lookup lk-head-not-on-origin head-not-on-origin "dependabot/cargo/gone is not on" \
  '.headRefName = "dependabot/cargo/gone"'
PRE='mkpr 7 ".headRefOid = \"$MAIN_OID\""' \
  refused_at_lookup lk-head-moved head-moved "at lookup; origin's ${HEADREF} is now"
assert_status "lk-head-moved-names-both-shas" "head was ${MAIN_OID} at lookup; origin's ${HEADREF} is now ${HEAD_OID}" "$OUT"
PRE='git clone -q "$H" "$(clone ${PS})"' KEEP_CLONE=1 \
  refused_at_lookup lk-closed-with-clone not-open "push" '.state = "CLOSED"'
assert_status "lk-closed-with-clone-host-side" "fetch -q --no-tags --no-write-fetch-head ${H}/../worktrees/${PS} refs/heads/${HEADREF}:refs/devloop/${PS}/head && git -C ${H} push -q ${ORIGIN} refs/devloop/${PS}/head:refs/heads/${HEADREF}" "$OUT"
assert_status "lk-closed-with-clone-fsck" "-c fetch.fsckObjects=true fetch" "$OUT"
assert_status "lk-closed-with-clone-branch-only-note" "updates the branch only, not the CLOSED PR" "$OUT"
PRE='git clone -q "$H" "$(clone ${PS})"' KEEP_CLONE=1 \
  refused_at_lookup lk-closed-unsafe-head not-open "refs/heads/<branch>:refs/devloop/${PS}/head" \
  '.state = "CLOSED"' '.headRefName = "+evil"'

# ── Happy path (positive control: gets past every check, podman IS reached) ─────
fixture happy
mkpr 1 '.isCrossRepository = true' '.headRepositoryOwner.login = "mallory"'   # same head name, a fork, listed first
mkpr 7
dl "q" "$BIN" ${PS} --pr 7 ${PS}
CL="$(clone ${PS})"
assert_rc     "happy-rc" 0 "$RC"
assert_status "happy-clone-on-head" "$HEADREF" "$(git -C "$CL" symbolic-ref --short HEAD)"
assert_status "happy-clone-at-pr-head" "$HEAD_OID" "$(git -C "$CL" rev-parse HEAD)"
assert_status "happy-origin-is-github-stand-in" "$ORIGIN" "$(git -C "$CL" remote get-url origin)"
assert_status "happy-banner" "=== PR mode: #7 https://github.com/acme/tower/pull/7 ===" "$OUT"
assert_status "happy-banner-head" "head:  ${HEADREF} @ ${HEAD_OID:0:12}" "$OUT"
assert_status "happy-banner-new" "(new)" "$OUT"
# The Dependabot caveat belongs at the push decision (exit menu), not in the launch output.
assert_absent "happy-no-caveat-at-launch" "is a Dependabot branch" "$(sed '/^=== PR exists/,$d' <<< "$OUT")"
assert_status "happy-podman-reached" "exec -it devloop-${PS}-dev claude" "$(log podman)"
assert_status "happy-menu-picks-pr-by-number" "=== PR exists: https://github.com/acme/tower/pull/7 ===" "$OUT"
assert_absent "happy-no-create" "[c] Push and create" "$OUT"
assert_status "happy-reenter-hint-keeps-pr" "Re-enter with: ${DEVLOOP} --pr 7 ${PS}" "$OUT"

# Re-attach with a commit made in the session: menu lists it and [p] pushes it to the PR head.
hostile_session "$CL" "fix build"
STUB_SESSION="${C}/session.sh" dl "pq" "$BIN" ${PS} --pr 7 ${PS}
assert_rc     "push-rc" 0 "$RC"
assert_status "push-reattached" "(existing, on ${HEADREF})" "$OUT"
assert_absent "push-no-mismatch" "CLONE_BRANCH_MISMATCH" "$OUT"
assert_status "push-unpushed-listed" "fix build" "$OUT"
assert_status "push-done" "Pushed. PR updated." "$OUT"
assert_status "push-caveat-at-menu" "is a Dependabot branch" "$OUT"
assert_status "push-landed-on-pr-head" "$(git -C "$CL" rev-parse HEAD)" "$(git -C "$ORIGIN" rev-parse "refs/heads/${HEADREF}")"
assert_absent "push-no-create" "[c] Push and create" "$OUT"
assert_status "push-signed-commit-listed" "fix build signed" "$OUT"
assert_status "push-gh-pinned" "pr list -R ${ORIGIN} --head ${HEADREF}" "$(log gh)"
assert_absent "push-host-ref-left-unforced" "forced" "$OUT"
assert_not_hostile "push-pr" "$CL"

# [r] re-execs with --pr <n> <slug> (no positional base, which --pr would refuse).
dl "rq" "$BIN" ${PS} --pr 7 ${PS}
assert_rc     "rerun-rc" 0 "$RC"
assert_status "rerun-banner-twice" "2" "$(grep -c '^=== PR mode: #7' <<< "$OUT")"
assert_absent "rerun-not-refused" "PR_MODE_REFUSED" "$OUT"

# `--` exec form in PR mode ends with the normal exit menu (PR mode is never unattended).
# Everything is pushed (above): the menu sees the PR, nothing unpushed, offers no push.
dl "q" "$BIN" ${PS} --pr 7 ${PS} -- true
assert_rc     "exec-rc" 0 "$RC"
assert_status "exec-ran-command" "Running in devloop-${PS}-dev: true" "$OUT"
assert_status "exec-rc-reported" "=== exited rc=0 ===" "$OUT"
assert_status "exec-menu-sees-pr" "=== PR exists:" "$OUT"
assert_absent "exec-nothing-unpushed" "Unpushed commits:" "$OUT"
assert_absent "exec-no-claude" "exec -it devloop-${PS}-dev claude" "$(log podman)"
assert_status "exec-reenter-keeps-pr" "Re-enter with: ${DEVLOOP} --pr 7 ${PS}" "$OUT"

# A new commit in the clone (as a `-- bash` hand fix makes): the menu lists only it as unpushed
# and [p] lands it on the PR head from the host repo, never forced.
printf 'exec fix\n' > "${CL}/exec-fix"; git -C "$CL" add exec-fix; git -C "$CL" commit -qm "exec fix"
dl "pq" "$BIN" ${PS} --pr 7 ${PS} -- true
assert_rc     "exec-push-rc" 0 "$RC"
assert_status "exec-unpushed-listed" "exec fix" "$(grep -A2 '^Unpushed commits:' <<< "$OUT")"
assert_absent "exec-unpushed-only-new" "fix build" "$(grep -A2 '^Unpushed commits:' <<< "$OUT")"
assert_status "exec-pushed" "Pushed. PR updated." "$OUT"
assert_status "exec-push-landed" "$(git -C "$CL" rev-parse HEAD)" "$(git -C "$ORIGIN" rev-parse "refs/heads/${HEADREF}")"
assert_absent "exec-push-unforced" "forced update" "$OUT"

# A FAILING command (a red layer-all, a rebase conflict) still ends in the menu, rc reported.
STUB_EXEC_RC=3 dl "q" "$BIN" ${PS} --pr 7 ${PS} -- false
assert_status "exec-fail-rc-reported" "=== exited rc=3 ===" "$OUT"
assert_status "exec-fail-menu-reached" "=== PR exists:" "$OUT"

# [r] from the exec-form menu re-runs the same command, not Claude.
dl "rq" "$BIN" ${PS} --pr 7 ${PS} -- true
assert_status "exec-rerun-same-command" "2" "$(grep -c "^Running in devloop-${PS}-dev: true" <<< "$OUT")"
assert_absent "exec-rerun-no-claude" "exec -it devloop-${PS}-dev claude" "$(log podman)"

# Without --pr the `--` form still reports and exits, never prompting (run-story's unattended
# contract). The podman stub's exec is a no-op returning 0, so rc propagation is not modelled.
STUB_EXEC_RC=2 dl "" "$BIN" plainexec plainexec -- false
assert_rc     "plain-exec-rc-propagated" 2 "$RC"
assert_status "plain-exec-reported" "=== exited rc=2 ===" "$OUT"
assert_status "plain-exec-still-running" "Still running: devloop-plainexec-dev" "$OUT"
assert_status "plain-exec-reenter" "Re-enter:  ${DEVLOOP} plainexec" "$OUT"
assert_absent "plain-exec-no-menu" "Choice:" "$OUT"

# Push rejected: Dependabot REWROTE the PR head (bump re-made, with a different patch, on a moved
# main; force-pushed). The menu only PRINTS the recovery: this run's values, nothing run, no git
# with the clone as repo/cwd, nothing planted in the clone fires.
git clone -q "$ORIGIN" "${C}/other"
printf 'main moved\n' > "${C}/other/m"; git -C "${C}/other" add m; git -C "${C}/other" commit -qm "main moves"
git -C "${C}/other" push -q origin HEAD:main
git -C "${C}/other" checkout -qb rewritten
printf 'bump v2\n' > "${C}/other/g"; git -C "${C}/other" add g; git -C "${C}/other" commit -qm "bump x to 1.2.3"
git -C "${C}/other" push -q -f origin "rewritten:refs/heads/${HEADREF}"
moved="$(git -C "$ORIGIN" rev-parse "refs/heads/${HEADREF}")"
hostile_session "$CL" "more fixes"
# Relaunch after the rebase: the PR lookup now reports the rewritten head, so the host's local
# <head> (the launch head) is no longer the PR head — but the clone still contains it.
mkpr 7 ".headRefOid = \"$moved\""
# Public repo: ONLY the container-rebase command — one devloop.sh line that fetches anonymously
# from the PR's repo (derived from the PR url) and ends in the menu for [p]. No host fallback.
echo PUBLIC > "${FIX}/visibility"
dl "pq" "$BIN" ${PS} --pr 7 ${PS}
assert_status "pub-rejected" "was rejected" "$OUT"
assert_status "pub-container-rebase-cmd" "${DEVLOOP} --pr 7 ${PS} -- bash -c 'GIT_TERMINAL_PROMPT=0 git fetch -q https://github.com/acme/tower.git refs/heads/${HEADREF} && git -c advice.skippedCherryPicks=false rebase --onto FETCH_HEAD ${HEAD_OID}'" "$OUT"
# Run the printed in-container script (url -> the fixture origin) in a fresh copy of the clone's
# branch: it lands the clone's own commits on the REWRITTEN head and never replays the original
# bump (a plain rebase would conflict: both bumps add g with different content).
INNER="$(grep -o "bash -c '[^']*'" <<< "$OUT" | head -1 | sed "s/^bash -c '//; s/'\$//; s#https://github.com/acme/tower.git#${ORIGIN}#")"
RB="${C}/rebase-run"; git init -q "$RB"
git -C "$RB" fetch -q "$CL" "refs/heads/${HEADREF}:refs/heads/${HEADREF}"; git -C "$RB" checkout -q "$HEADREF"
RB_RC=0; (cd "$RB" && bash -c "$INNER") >/dev/null 2>&1 || RB_RC=$?
assert_rc     "pub-rebase-runs" 0 "$RB_RC"
assert_status "pub-rebase-on-rewritten" "bump v2" "$(cat "${RB}/g" 2>&1)"
assert_status "pub-rebase-bump-once" "1" "$(git -C "$RB" log --format=%s "$MAIN_OID..HEAD" 2>/dev/null | grep -c '^bump x to 1.2.3$' || true)"
assert_status "pub-rebase-keeps-fixes" "exec fix" "$(git -C "$RB" log --format=%s "${moved}..HEAD" 2>&1)"
assert_status "pub-conflict-note" "re-enter with -- bash, resolve," "$OUT"
assert_absent "pub-no-host-fallback" "worktree add" "$OUT"
assert_status "pub-visibility-asked" "repo view https://github.com/acme/tower --json visibility" "$(log gh)"
# Visibility unknown: both, with a WARNING (never silently the wrong one).
touch "${FIX}/visibility.fail"
dl "pq" "$BIN" ${PS} --pr 7 ${PS}
assert_status "unk-warning" "WARNING: could not read the repo's visibility" "$OUT"
assert_status "unk-container-cmd" "-- bash -c 'GIT_TERMINAL_PROMPT=0 git fetch" "$OUT"
assert_status "unk-fallback" "worktree add -b ${HEADREF}" "$OUT"
rm -f "${FIX}/visibility.fail"
# Private repo: the host-side fallback only (the scenario below runs its steps).
echo PRIVATE > "${FIX}/visibility"
STUB_SESSION="${C}/session.sh" dl "pq" "$BIN" ${PS} --pr 7 ${PS}
REB="${C}/${PS}-rebase"; ABSORB="${__here}/absorb-devloop.sh"
STEPS_TEXT="$(grep '^    \$ ' <<< "$OUT" || true)"
assert_rc     "reject-rc" 0 "$RC"
assert_status "reject-error" "ERROR: push of ${HEADREF} was rejected" "$OUT"
assert_status "reject-origin-untouched" "$moved" "$(git -C "$ORIGIN" rev-parse "refs/heads/${HEADREF}")"
assert_status "reject-never-force" "Never force-push" "$OUT"
assert_status "reject-pr-wording" "PR #7's branch ${HEADREF} probably moved" "$OUT"
assert_absent "reject-private-no-container-cmd" "-- bash -c" "$OUT"
assert_status "reject-private-labelled" "The repo is PRIVATE, so the container cannot fetch it anonymously; recover on the host:" "$OUT"
assert_status "reject-header-once" "1" "$(grep -c 'probably moved on origin' <<< "$OUT")"
# The local <head> (ensure_branch_ref made it at launch) blocks the worktree add: its remedy comes
# FIRST, and since it is still the launch head (-D loses nothing) it is step 1 of the block.
assert_status "reject-first-section" "First, or the worktree add below refuses:" "$OUT"
assert_status "reject-local-head-in-clone" "It is at ${HEAD_OID:0:12}, which the clone's ${HEADREF} contains" "$OUT"
assert_status "reject-local-head-loses-nothing" "so deleting it loses nothing while the clone exists (first step below) — do not --destroy before the push." "$OUT"
assert_absent "reject-local-head-not-may-hold" "may hold your commits" "$OUT"
assert_status "reject-steps-in-order" "\
    \$ git -C ${H} branch -D ${HEADREF}
    \$ git -C ${H} fetch origin
    \$ git -C ${H} worktree add -b ${HEADREF} ${REB} origin/${HEADREF}
    \$ cd ${REB} && ${ABSORB} ${H}/../worktrees/${PS}
    \$ git -C ${H} push -q ${ORIGIN} refs/heads/${HEADREF}:refs/heads/${HEADREF}  # never force" "$STEPS_TEXT"
assert_status "reject-five-steps" "5" "$(grep -c . <<< "$STEPS_TEXT")"
assert_absent "reject-no-pr-create" "gh pr create" "$OUT"
assert_status "reject-skill-form" "/absorb-devloop ${PS} run from that worktree" "$OUT"
assert_status "reject-dry-run" "${ABSORB} --dry-run ${H}/../worktrees/${PS}" "$OUT"
assert_status "reject-skip-note" "git cherry-pick --skip   — it goes on with your remaining commits" "$OUT"
assert_status "reject-no-rerun" "Do NOT rerun absorb after a skip" "$OUT"
assert_status "reject-destroy-only-after" "ONLY after the push succeeded" "$OUT"
assert_status "reject-destroy-abs" "cd ${H} && git -C ${H} worktree remove ${REB} && ${__here}/devloop.sh --destroy ${PS}" "$OUT"
# Nothing was run.
assert_rc     "reject-no-rebase-dir" 1 "$([[ -e "$REB" ]] && echo 0 || echo 1)"
assert_rc     "reject-no-host-worktree" 1 "$(git -C "$H" worktree list | grep -q rebase && echo 0 || echo 1)"
assert_status "reject-local-head-untouched" "$HEAD_OID" "$(git -C "$H" rev-parse "refs/heads/${HEADREF}")"
GL="$(log git)"
assert_status "reject-shim-saw-host-push" "argv= -C ${H} push -q ${ORIGIN} " "$GL"
assert_absent "reject-shim-no-worktree-add" "worktree add" "$GL"
assert_absent "reject-shim-no-fetch-origin" " fetch origin" "$GL"
assert_absent "reject-shim-no-branch-delete" "branch -D" "$GL"
assert_status "reject-no-git-in-clone" "none" "$(c="$(clone_git_calls "$PS" "$GL")"; [[ -z "$c" ]] && echo none || echo "$c")"
assert_status "reject-matcher-control-cwd" "cwd=${CL}|" "$(clone_git_calls "$PS" "cwd=${CL}|GIT_DIR=|GIT_WORK_TREE=|argv= rev-parse HEAD")"
assert_status "reject-matcher-control-C" "rev-parse" "$(clone_git_calls "$PS" "cwd=/x|GIT_DIR=|GIT_WORK_TREE=|argv= -C ${H}/../worktrees/${PS} rev-parse HEAD")"
assert_status "reject-matcher-control-gitdir" "rev-parse" "$(clone_git_calls "$PS" "cwd=/x|GIT_DIR=${CL}/.git|GIT_WORK_TREE=|argv= rev-parse HEAD")"
# The recovery inspects no container (the bundle era checked mounts); the only inspect is the
# re-attach's cluster-label read.
assert_absent "reject-no-mount-inspect" ".Mounts" "$(log podman)"
assert_status "reject-only-label-inspect" "0" "$(grep inspect <<< "$(log podman)" | grep -vc 'devloop.cluster' || true)"
assert_absent "reject-no-unmodelled" "unmodelled" "$OUT"
assert_not_hostile "reject" "$CL"

# Positive control: run the printed `$ ` lines IN PRINTED ORDER on the host (-D, fetch, add);
# absorb stops on the PR's original bump (its patch changed upstream); the printed skip; the
# printed push. Every host git call is guarded, so a regression fails a named assertion and the
# suite still runs to its summary.
mapfile -t STEPS <<< "$(sed 's/^    \$ //' <<< "$STEPS_TEXT")"
run_host() { (cd "$H" && eval "$1") >/dev/null 2>&1; }   # one printed step, from the host repo
set +e
run_host "${STEPS[0]}" && run_host "${STEPS[1]}" && run_host "${STEPS[2]}"; ctl_rc=$?
set -e
assert_rc     "ctl-delete-fetch-add-in-order" 0 "$ctl_rc"
dry="$(cd "$REB" 2>/dev/null && "$ABSORB" --dry-run "$PS" 2>&1 || true)"
assert_status "ctl-slug-form-same-clone" "Source: ${HEADREF} @ ${REB}/../worktrees/${PS}" "$dry"
set +e; abs_out="$(cd "$H" && eval "${STEPS[3]}" 2>&1)"; abs_rc=$?; set -e
assert_rc     "ctl-absorb-stops-on-bump" 3 "$abs_rc"
assert_status "ctl-absorb-stopped-on-original" "could not apply ${HEAD_OID:0:7}" "$abs_out"
set +e; git -C "$REB" cherry-pick --skip >/dev/null 2>&1; skip_rc=$?; set -e
assert_rc     "ctl-skip-finishes" 0 "$skip_rc"
# The printed warning is real: an absorb rerun after the skip queues the bump again and stops.
set +e; run_host "${STEPS[3]}"; rerun_rc=$?; git -C "$REB" cherry-pick --abort >/dev/null 2>&1; abort_rc=$?; set -e
assert_rc     "ctl-rerun-after-skip-loops" 3 "$rerun_rc"
assert_rc     "ctl-rerun-aborted" 0 "$abort_rc"
set +e; run_host "${STEPS[4]}"; push_rc=$?; set -e
assert_rc     "ctl-push" 0 "$push_rc"
assert_status "ctl-on-top-of-rewritten" "$moved" "$(git -C "$ORIGIN" rev-list "refs/heads/${HEADREF}" | grep -x "$moved" || true)"
assert_status "ctl-fix-once" "1" "$(git -C "$ORIGIN" log --format=%s "${moved}..refs/heads/${HEADREF}" | grep -c '^more fixes$' || true)"
assert_status "ctl-bump-once" "1" "$(git -C "$ORIGIN" log --format=%s "main..refs/heads/${HEADREF}" | grep -c '^bump x to 1.2.3$' || true)"
assert_status "ctl-bump-is-rewritten" "bump v2" "$(git -C "$ORIGIN" show "refs/heads/${HEADREF}:g" 2>&1)"
set +e; git -C "$H" worktree remove "$REB" >/dev/null 2>&1; rc_=$?; set -e
assert_rc     "ctl-worktree-removed" 0 "$rc_"

# Rejected again, now with the host's local <head> MOVED since launch (it holds the absorbed
# result): -D is offered only after an inspect, never as "loses nothing", never as a `$ ` step.
printf 'cd %q && printf again >> h && git add h && git commit -qm "again"\n' "$CL" > "${C}/session.sh"
STUB_SESSION="${C}/session.sh" dl "pq" "$BIN" ${PS} --pr 7 ${PS}
assert_status "reject2-rejected" "was rejected" "$OUT"
assert_status "reject2-may-hold" "The host repo's local ${HEADREF} is at $(git -C "$H" rev-parse "refs/heads/${HEADREF}"); it may hold your commits" "$OUT"
assert_status "reject2-inspect" "Inspect: git -C ${H} fetch origin && git -C ${H} log --oneline origin/${HEADREF}..${HEADREF}" "$OUT"
assert_status "reject2-delete-if-empty" "If that prints nothing: git -C ${H} branch -D ${HEADREF}" "$OUT"
assert_absent "reject2-not-loses-nothing" "loses nothing" "$OUT"
assert_absent "reject2-delete-not-a-step" "    \$ git -C ${H} branch -D" "$OUT"
# A leftover <slug>-rebase worktree on <head>: named, plain remove (no --force).
set +e; git -C "$H" worktree add -q "$REB" "$HEADREF" >/dev/null 2>&1; rc_=$?; set -e
assert_rc     "reject3-setup" 0 "$rc_"
dl "pq" "$BIN" ${PS} --pr 7 ${PS}
assert_status "reject3-checked-out-elsewhere" "${HEADREF} is checked out in the worktree ${REB}" "$OUT"
assert_status "reject3-remove" "has uncommitted work): git -C ${H} worktree remove ${REB}" "$OUT"
assert_absent "reject3-no-force" "--force" "$OUT"
set +e; git -C "$H" worktree remove "$REB" >/dev/null 2>&1; rc_=$?; set -e
assert_rc     "reject3-teardown" 0 "$rc_"
# A leftover <slug>-rebase that is NOT a registered worktree: inspect, then rm -r (printed only).
mkdir -p "${REB}/junk"
dl "pq" "$BIN" ${PS} --pr 7 ${PS}
assert_status "reject5-not-a-worktree" "${REB} exists and is not a git worktree: inspect it, then remove it: rm -r ${REB}" "$OUT"
assert_absent "reject5-no-worktree-remove-remedy" "is a leftover worktree" "$OUT"
assert_rc     "reject5-dir-kept" 0 "$([[ -d "${REB}/junk" ]] && echo 0 || echo 1)"
rm -rf "$REB"
# <head> checked out in REPO_ROOT itself: switch, not remove.
set +e; git -C "$H" checkout -q "$HEADREF" >/dev/null 2>&1; rc_=$?; set -e
assert_rc     "reject4-setup" 0 "$rc_"
dl "pq" "$BIN" ${PS} --pr 7 ${PS}
assert_status "reject4-in-repo-root" "${HEADREF} is checked out in ${H} itself. Switch it: git -C ${H} switch <another-branch>" "$OUT"
set +e; git -C "$H" checkout -q main >/dev/null 2>&1; rc_=$?; set -e
assert_rc     "reject4-teardown" 0 "$rc_"

# PR closed during the session: warning + manual push hint, no push/create arm.
printf 'jq %q %q > %q.t && mv %q.t %q\n' '.state = "CLOSED"' "${FIX}/pr-7.json" "${FIX}/pr-7.json" "${FIX}/pr-7.json" "${FIX}/pr-7.json" > "${C}/session.sh"
STUB_SESSION="${C}/session.sh" dl "q" "$BIN" ${PS} --pr 7 ${PS}
assert_status "closed-mid-warning" "WARNING: PR #7 is no longer open" "$OUT"
assert_status "closed-mid-manual-push" "push -q ${ORIGIN} refs/devloop/${PS}/head:refs/heads/${HEADREF}" "$OUT"
assert_absent "closed-mid-no-push-arm" "[p] Push" "$OUT"
assert_absent "closed-mid-no-create" "[c] Push and create" "$OUT"

# gh failing at the menu: named as a query failure (not "PR closed"), still no create/push.
mkpr 1 '.isCrossRepository = true' '.headRepositoryOwner.login = "mallory"'; mkpr 7
printf 'echo "HTTP 502: Bad Gateway" > %q\n' "${FIX}/list.fail" > "${C}/session.sh"
STUB_SESSION="${C}/session.sh" dl "q" "$BIN" ${PS} --pr 7 ${PS}
assert_status "ghfail-mid-warning" "could not query open PRs for ${HEADREF} (gh rc=1" "$OUT"
assert_status "ghfail-mid-gh-stderr" "HTTP 502: Bad Gateway" "$OUT"
assert_absent "ghfail-mid-not-called-closed" "is no longer open" "$OUT"
assert_absent "ghfail-mid-no-push-arm" "[p] Push" "$OUT"
assert_absent "ghfail-mid-no-create" "[c] Push and create" "$OUT"
rm -f "${FIX}/list.fail"

# origin's head branch gone: what is unpushed is UNKNOWN (warned), and the push is still offered.
printf 'git -C %q update-ref -d %q\n' "$ORIGIN" "refs/heads/${HEADREF}" > "${C}/session.sh"
STUB_SESSION="${C}/session.sh" dl "q" "$BIN" ${PS} --pr 7 ${PS}
assert_status "unpushed-unknown-warning" "cannot tell what is unpushed: no ${HEADREF} found on ${ORIGIN}" "$OUT"
assert_status "unpushed-unknown-listed" "(unknown — see WARNING above)" "$OUT"
assert_status "unpushed-unknown-push-offered" "[p] Push to update PR" "$OUT"

# --destroy: clone, runtime dir and the host-repo refs/devloop/<slug>/* refs are gone.
assert_rc     "destroy-precondition-refs-exist" 0 "$(git -C "$H" for-each-ref "refs/devloop/${PS}" | grep -q . && echo 0 || echo 1)"
dl "" "$BIN" "${PS}" --destroy "${PS}"
assert_rc     "destroy-rc" 0 "$RC"
assert_rc     "destroy-drops-host-refs" 1 "$(git -C "$H" for-each-ref "refs/devloop/${PS}" | grep -q . && echo 0 || echo 1)"
assert_rc     "destroy-drops-clone" 1 "$([[ -e "$CL" ]] && echo 0 || echo 1)"
assert_rc     "destroy-drops-runtime-dir" 1 "$([[ -e "/tmp/devloop-${PS}" ]] && echo 0 || echo 1)"
# The local <head> devloop.sh created was moved since (the reject recovery rebuilt it): kept, loudly.
assert_status "destroy-keeps-moved-branch" "Kept local branch ${HEADREF}: it moved since devloop.sh created it" "$OUT"
assert_rc     "destroy-moved-branch-still-there" 0 "$(git -C "$H" rev-parse -q --verify "refs/heads/${HEADREF}" >/dev/null && echo 0 || echo 1)"
assert_rc     "destroy-drops-branch-record" 1 "$(git -C "$H" config --get-regexp "^devloop\\.${PS}\\." >/dev/null && echo 0 || echo 1)"

# ── --no-cluster, and --destroy deleting the unchanged local <head> it created ──
fixture nocluster
mkpr 7
dl "q" "$BIN" ${PS} --no-cluster --pr 7 ${PS}
assert_rc     "nocl-rc" 0 "$RC"
assert_status "nocl-infra-skipped" "cluster=skipped (--no-cluster; Layer 7 needs it: run scripts/layer-all.sh --max-layer 6)" "$OUT"
assert_status "nocl-reenter-keeps-flag" "Re-enter with: ${DEVLOOP} --no-cluster --pr 7 ${PS}" "$OUT"
assert_status "nocl-branch-recorded" "$HEADREF" "$(git -C "$H" config --get "devloop.${PS}.createdBranch")"
STUB_LABEL=on dl "q" "$BIN" ${PS} --no-cluster --pr 7 ${PS}
assert_status "nocl-label-mismatch-warned" "was created with a cluster; its helper and Kind cluster from earlier" "$OUT"
STUB_LABEL=off dl "q" "$BIN" ${PS} --no-cluster --pr 7 ${PS}
assert_absent "nocl-label-match-quiet" "was created with" "$OUT"
# Two upstream rewrites in a row (e.g. Dependabot rebases twice): each printed recovery replays
# ONLY the clone's own commits. The second must rebase from the head the first rebased onto
# (devloop.<slug>.lastOnto), not the launch head — that range would replay the re-made bump.
CL2="$(clone "$PS")"
printf 'fix1\n' > "${CL2}/fix1"; git -C "$CL2" add fix1; git -C "$CL2" commit -qm "fix one"
rewrite() {  # <bump content>: force-push a re-made bump on a moved main; prints the new head
  local o="${C}/rw"; rm -rf "$o"; git clone -q "$ORIGIN" "$o"
  printf '%s\n' "$1" > "${o}/m"; git -C "$o" add m; git -C "$o" commit -qm "main moves"; git -C "$o" push -q origin HEAD:main
  git -C "$o" checkout -qb rw origin/main 2>/dev/null || git -C "$o" checkout -qb rw
  printf '%s\n' "$1" > "${o}/g"; git -C "$o" add g; git -C "$o" commit -qm "bump x to 1.2.3"
  git -C "$o" push -q -f origin "rw:refs/heads/${HEADREF}"; git -C "$ORIGIN" rev-parse "refs/heads/${HEADREF}"
}
run_inner() {  # run the printed in-container script in CL2 (url -> fixture origin)
  local inner; inner="$(grep -o "bash -c '[^']*'" <<< "$OUT" | head -1 | sed "s/^bash -c '//; s/'\$//; s#https://github.com/acme/tower.git#${ORIGIN}#")"
  (cd "$CL2" && bash -c "$inner") >/dev/null 2>&1
}
echo PUBLIC > "${FIX}/visibility"
R1="$(rewrite 'bump r1')"; mkpr 7 ".headRefOid = \"$R1\""
dl "pq" "$BIN" ${PS} --no-cluster --pr 7 ${PS}
assert_status "twice1-rejected" "was rejected" "$OUT"
assert_status "twice1-base-is-launch-head" "rebase --onto FETCH_HEAD ${HEAD_OID}'" "$OUT"
assert_status "twice1-keeps-no-cluster" "${DEVLOOP} --no-cluster --pr 7 ${PS} -- bash -c" "$OUT"
assert_status "twice1-lastonto-recorded" "$R1" "$(git -C "$H" config --get "devloop.${PS}.lastOnto")"
T_RC=0; run_inner || T_RC=$?
assert_rc     "twice1-rebase-runs" 0 "$T_RC"
R2="$(rewrite 'bump r2')"; mkpr 7 ".headRefOid = \"$R2\""
dl "pq" "$BIN" ${PS} --no-cluster --pr 7 ${PS}
assert_status "twice2-rejected" "was rejected" "$OUT"
assert_status "twice2-base-is-first-onto" "rebase --onto FETCH_HEAD ${R1}'" "$OUT"
T_RC=0; run_inner || T_RC=$?
assert_rc     "twice2-rebase-runs" 0 "$T_RC"
assert_status "twice2-on-second-rewrite" "bump r2" "$(cat "${CL2}/g")"
assert_status "twice2-bump-once" "1" "$(git -C "$CL2" log --format=%s "${R2}~1..HEAD" | grep -c '^bump x to 1.2.3$' || true)"
assert_status "twice2-fix-kept" "fix one" "$(git -C "$CL2" log --format=%s "${R2}..HEAD")"
dl "" "$BIN" "${PS}" --no-cluster --destroy "${PS}"
assert_rc     "nocl-with-destroy-refused" 1 "$RC"
assert_status "nocl-with-destroy-msg" "--no-cluster only applies to a launch/re-attach" "$OUT"
dl "" "$BIN" "${PS}" --destroy "${PS}"
assert_rc     "destroy-unchanged-rc" 0 "$RC"
assert_status "destroy-deletes-unchanged-branch" "Deleted local branch ${HEADREF} (created by devloop.sh from origin, unchanged since)." "$OUT"
assert_rc     "destroy-unchanged-branch-gone" 1 "$(git -C "$H" rev-parse -q --verify "refs/heads/${HEADREF}" >/dev/null && echo 0 || echo 1)"
assert_rc     "destroy-unchanged-record-gone" 1 "$(git -C "$H" config --get-regexp "^devloop\\.${PS}\\." >/dev/null && echo 0 || echo 1)"
assert_rc     "destroy-main-untouched" 0 "$(git -C "$H" rev-parse -q --verify refs/heads/main >/dev/null && echo 0 || echo 1)"

# ── Without --pr ───────────────────────────────────────────────────────────────
fixture nopr
hostile_session "$(clone s1)" "feature work"
STUB_SESSION="${C}/session.sh" dl "cq" "$BIN" s1 s1 main
assert_rc     "nopr-rc" 0 "$RC"
assert_status "nopr-clone-feature-branch" "feature/s1" "$(git -C "$(clone s1)" symbolic-ref --short HEAD)"
assert_absent "nopr-no-pr-view" "pr view" "$(log gh)"
assert_status "nopr-menu-queried" "pr list -R ${ORIGIN} --head feature/s1" "$(log gh)"
assert_status "nopr-commits-label" "=== Commits on feature/s1 since main (no open PR) ===" "$OUT"
assert_status "nopr-create-offered" "[c] Push and create PR into main" "$OUT"
assert_status "nopr-c-pushed-from-host" "$(git -C "$(clone s1)" rev-parse HEAD)" "$(git -C "$ORIGIN" rev-parse refs/heads/feature/s1 2>&1)"
assert_status "nopr-c-pr-created-pinned" "pr create -R ${ORIGIN} --base main --head feature/s1 --title feature/s1 --body - " "$(log gh)"
assert_status "nopr-c-body-lists-commits" "feature work signed" "$(log gh)"
assert_not_hostile "nopr-c" "$(clone s1)"
assert_status "nopr-reenter-hint" "Re-enter with: ${DEVLOOP} s1" "$OUT"

# Non-PR [p]: pushed from the host repo, nothing planted in the clone fires.
hostile_session "$(clone s3)" "plain push"
STUB_SESSION="${C}/session.sh" dl "pq" "$BIN" s3 s3 main
assert_rc     "nopr-p-rc" 0 "$RC"
assert_status "nopr-p-pushed" "Pushed feature/s3." "$OUT"
assert_status "nopr-p-landed" "$(git -C "$(clone s3)" rev-parse HEAD)" "$(git -C "$ORIGIN" rev-parse refs/heads/feature/s3 2>&1)"
assert_not_hostile "nopr-p" "$(clone s3)"

# Non-PR [c] rejected (feature/s3 moved on origin): branch wording, no PR wording, the recovery
# ends with gh pr create, no local-branch remedy (the host has no local feature/s3).
git clone -q -b feature/s3 "$ORIGIN" "${C}/o3"
printf 'x\n' > "${C}/o3/x"; git -C "${C}/o3" add x; git -C "${C}/o3" commit -qm "moved"; git -C "${C}/o3" push -q origin feature/s3
printf 'cd %q && printf y >> y && git add y && git commit -qm "nopr more"\n' "$(clone s3)" > "${C}/session.sh"
STUB_SESSION="${C}/session.sh" dl "cq" "$BIN" s3 s3 main
assert_status "nopr-c-reject-error" "ERROR: push of feature/s3 was rejected" "$OUT"
assert_status "nopr-c-reject-branch-wording" "The branch feature/s3 probably moved" "$OUT"
assert_absent "nopr-c-reject-no-pr-wording" "PR #" "$OUT"
assert_absent "nopr-c-reject-no-pr-original" "the PR's original commit" "$OUT"
assert_status "nopr-c-reject-step-add" "    \$ git -C ${H} worktree add -b feature/s3 ${C}/s3-rebase origin/feature/s3" "$OUT"
assert_status "nopr-c-reject-create-last" "    \$ gh pr create -R ${ORIGIN} --base main --head feature/s3" "$(grep '^    \$ ' <<< "$OUT" | tail -1)"
assert_absent "nopr-c-reject-no-local-remedy" "branch -D" "$OUT"
assert_absent "nopr-c-reject-no-pr-created" "pr create" "$(log gh)"
assert_status "nopr-c-reject-no-git-in-clone" "none" "$(c="$(clone_git_calls s3 "$(log git)")"; [[ -z "$c" ]] && echo none || echo "$c")"

# Non-PR, origin unreachable at start: loud WARNING (git's stderr + divergence risk), not refused.
git -C "$H" remote set-url origin "${C}/nowhere.git"
dl "q" "$BIN" s9 s9 main
assert_rc     "nopr-offline-rc" 0 "$RC"
assert_status "nopr-offline-warning" "WARNING: git ls-remote ${C}/nowhere.git failed (rc=" "$OUT"
assert_status "nopr-offline-git-stderr" "nowhere.git" "$OUT"
assert_status "nopr-offline-diverge" "this DIVERGES from it" "$OUT"
assert_status "nopr-offline-fresh-clone" "feature/s9" "$(git -C "$(clone s9)" symbolic-ref --short HEAD)"
git -C "$H" remote set-url origin "$ORIGIN"
assert_absent "nopr-no-banner" "=== PR mode" "$OUT"

# Re-attach to a clone on feature/<slug>: no mismatch, same clone, commit kept.
dl "q" "$BIN" s1 s1
assert_rc     "nopr-reattach-rc" 0 "$RC"
assert_absent "nopr-reattach-no-mismatch" "CLONE_BRANCH_MISMATCH" "$OUT"
assert_status "nopr-reattach-commit-kept" "feature work" "$(git -C "$(clone s1)" log --format=%s -1)"

# Clone on another branch: refused before anything starts (the one no---pr behaviour change).
git -C "$(clone s1)" checkout -qb "$HEADREF"
dl "q" "$BIN" s1 s1
assert_rc     "nopr-mismatch-rc" 1 "$RC"
assert_status "nopr-mismatch-token" "ERROR: CLONE_BRANCH_MISMATCH CLONE=" "$OUT"
assert_status "nopr-mismatch-names-both" "worktrees/s1 ON=${HEADREF} EXPECTED=feature/s1" "$OUT"
assert_status "nopr-mismatch-switch-hint" "podman exec -it -w /work devloop-s1-dev git switch feature/s1" "$OUT"
assert_status "nopr-mismatch-start-hint" "start them first: podman start devloop-s1-dev devloop-s1-db" "$OUT"
assert_status "nopr-mismatch-pr-hint" "re-run with: ${DEVLOOP} --pr <number> s1" "$OUT"
assert_status "nopr-mismatch-destroy-hint" "--destroy s1" "$OUT"
assert_absent "nopr-mismatch-no-pr-token" "PR_MODE_REFUSED" "$OUT"
assert_rc     "nopr-mismatch-podman-not-called" 0 "$([[ -z "$(log podman)" ]] && echo 0 || echo 1)"
assert_rc     "nopr-mismatch-gh-not-called" 0 "$([[ -z "$(log gh)" ]] && echo 0 || echo 1)"
assert_status "nopr-mismatch-clone-untouched" "$HEADREF" "$(git -C "$(clone s1)" symbolic-ref --short HEAD)"
git -C "$(clone s1)" checkout -q --detach
dl "q" "$BIN" s1 s1
assert_rc     "nopr-detached-rc" 1 "$RC"
assert_status "nopr-detached-token" "ON=(no branch: detached HEAD" "$OUT"
# The remedy works: switched back, it re-attaches.
git -C "$(clone s1)" switch -q feature/s1
dl "q" "$BIN" s1 s1
assert_rc     "nopr-switched-back-rc" 0 "$RC"
assert_absent "nopr-switched-back-no-mismatch" "CLONE_BRANCH_MISMATCH" "$OUT"

# Existing feature/<slug> (PR iteration): the clone has no local base branch. Before, the
# menu's `git log main..HEAD` errored, was masked, and said "No new commits".
git -C "$H" checkout -qb feature/s2
printf 'iter\n' > "${H}/i"; git -C "$H" add i; git -C "$H" commit -qm "iteration commit"
git -C "$H" checkout -q main
dl "q" "$BIN" s2 s2 main
assert_rc     "nobase-rc" 0 "$RC"
assert_rc     "nobase-precondition-no-local-main" 1 "$(git -C "$(clone s2)" show-ref -q --verify refs/heads/main && echo 0 || echo 1)"
assert_status "nobase-label" "Commits on feature/s2 since main (no open PR)" "$OUT"
assert_status "nobase-commit-listed" "iteration commit" "$OUT"
assert_absent "nobase-not-masked" "No new commits" "$OUT"

# Base branch nowhere in the host repo: commits since base UNKNOWN (warned), push/create still offered.
git -C "$H" branch -q feature/s4 feature/s2
dl "q" "$BIN" s4 s4 release
assert_rc     "base-unknown-rc" 0 "$RC"
assert_status "base-unknown-warning" "cannot list commits since release: the host repo has neither" "$OUT"
assert_status "base-unknown-header" "commits since release unknown" "$OUT"
assert_status "base-unknown-create-offered" "[c] Push and create PR into release" "$OUT"
assert_absent "base-unknown-not-masked" "No new commits" "$OUT"

# The per-slug /tmp/devloop mount exists ONLY with the Kind helper (no case models `podman run`,
# so this is a structural check): exactly one such mount, inside devloop.sh's `if launch_helper` arm.
helper_arm="$(awk '/^        if launch_helper; then$/ {on=1; next} on && /^        else$/ {exit} on' "$DEVLOOP")"
assert_status "mount-only-with-helper" '-v "$HELPER_RUNTIME_DIR:/tmp/devloop:Z"' "$helper_arm"
assert_status "mount-exactly-once" "1" "$(grep -c -- ':/tmp/devloop:Z' "$DEVLOOP")"

report_results "infra/devloop/devloop-pr.test.sh"
