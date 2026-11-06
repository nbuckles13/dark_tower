# clone-head.sh — sourced library; the ONE way host-side scripts learn which branch a
# devloop clone has checked out.
#
# A devloop clone is bind-mounted read-write into its container, whose agent runs with
# --dangerously-skip-permissions, so the clone's .git/config and .git/hooks are
# container-writable. Host git run WITH THE CLONE AS ITS REPO trusts that config: a planted
# pre-push hook, core.sshCommand, remote.*.pushurl or log.showSignature + gpg.program then
# executes on the host or redirects a push. So host scripts never run git against a clone
# once its container exists: they read HEAD as a plain FILE here, and take commits out with
# a fetch FROM the clone into the host repo (upload-pack runs no hooks and honours pack hooks
# only from protected config).
#
# Consumers: infra/devloop/devloop.sh (check_clone_branch, fetch_clone_branch, manual_push_hint),
# infra/devloop/absorb-devloop.sh.
#
# Usage: clone_head_branch <clone-dir>   prints the branch name HEAD points at.
# Returns 1 (prints nothing) on a detached HEAD, a missing/unreadable HEAD, or a name that is
# not a valid branch name.
#
#        fetch_from_clone <host-repo> <clone-dir> <branch> <dest-ref>
# The ONE way commits leave a clone: a fetch run in the HOST repo, FROM the clone's
# refs/heads/<branch> (fully qualified), into <dest-ref>, which is deleted first so the fetch only
# ever CREATES it (never a forced update). CLONE_FETCH_ARGS is the flag set, also used to print
# the copy-paste form: fsckObjects refuses malformed container-authored objects at the boundary;
# --no-tags stops tags the container planted (e.g. one named `main`, which would shadow the
# branch in the host repo) from being auto-followed in; --no-write-fetch-head keeps the shared
# FETCH_HEAD out of it.

CLONE_FETCH_ARGS=(-c fetch.fsckObjects=true fetch -q --no-tags --no-write-fetch-head)

fetch_from_clone() {
    local repo="$1" clone="$2" branch="$3" dest="$4"
    git -C "$repo" update-ref -d "$dest" 2>/dev/null || true
    git -C "$repo" "${CLONE_FETCH_ARGS[@]}" "$clone" "refs/heads/${branch}:${dest}"
}

clone_head_branch() {
    local dir="$1" gitdir="${1}/.git" line="" head=""
    if [[ -f "$gitdir" ]]; then
        # A linked worktree's `.git` is a file: `gitdir: <path>`.
        IFS= read -r line < "$gitdir" || [[ -n "$line" ]] || return 1
        [[ "$line" == "gitdir: "* ]] || return 1
        gitdir="${line#gitdir: }"
        [[ "$gitdir" == /* ]] || gitdir="${dir}/${gitdir}"
    fi
    [[ -f "${gitdir}/HEAD" ]] || return 1
    IFS= read -r head < "${gitdir}/HEAD" || [[ -n "$head" ]] || return 1
    [[ "$head" == "ref: refs/heads/"* ]] || return 1
    head="${head#ref: refs/heads/}"
    # Plain refname check (no --branch: that form can consult a repository).
    git check-ref-format "refs/heads/${head}" >/dev/null 2>&1 || return 1
    printf '%s\n' "$head"
}
