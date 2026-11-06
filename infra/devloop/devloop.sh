#!/bin/bash
# devloop.sh — Isolated devloop execution with persistent containers.
#
# Creates a local git clone, spins up a dev container + postgres
# (sharing a network namespace), and drops you into Claude Code with
# --dangerously-skip-permissions.
#
# Containers persist between sessions — Ctrl-D exits Claude but
# they stay running. Re-run this script to re-attach.
#
# Usage:
#   ./devloop.sh [--rebuild] [--recreate] <task-slug> [base-branch] [-- <command...>]
#   ./devloop.sh [--rebuild] [--recreate] --pr <number> <task-slug> [-- <command...>]
#   ./devloop.sh --rebuild                              # rebuild image only
#   ./devloop.sh --refresh-creds <task-slug>
#   ./devloop.sh --destroy <task-slug>
#
# Options:
#   --rebuild         Force rebuild the dev container image (exits if no task-slug)
#   --recreate        Destroy and recreate containers, preserving the local clone
#   --refresh-creds   Copy fresh OAuth credentials into a running container
#                     (credentials mode only; token mode redirects to --recreate)
#   --destroy         Tear down containers, helper, Kind cluster and the clone
#   --pr <number>     Fix an existing open GitHub PR in place (see "PR mode" below)
#   --no-cluster      Skip the devloop helper + Kind cluster for this run (default: bring it up).
#                     For fixes that don't need Layer 7: validate with `scripts/layer-all.sh
#                     --max-layer 6` (Layer 7 fails loudly without a cluster, by design), and
#                     CI runs Layer 7. A /devloop needs the cluster (its Gate 2 runs all layers).
#   -- <command...>   Run <command> in the container instead of the interactive attach
#
# Examples:
#   ./devloop.sh td-42-rate-limiting                     # branches from current branch
#   ./devloop.sh td-42-rate-limiting main                # branches from main
#   ./devloop.sh --pr 117 fix-cargo-bump                # work on PR #117's own branch
#   ./devloop.sh --rebuild                              # just rebuild the image
#   ./devloop.sh --rebuild td-42-rate-limiting
#   ./devloop.sh --recreate td-42-rate-limiting         # new containers, same clone
#   ./devloop.sh --rebuild --recreate td-42-rate-limiting  # rebuild image + recreate
#   ./devloop.sh --refresh-creds td-42-rate-limiting   # from another terminal
#   ./devloop.sh --destroy td-42-rate-limiting         # tear down (push first)
#
# PR mode (--pr <number>):
#   The clone is checked out on the PR's head branch at the PR's current head commit, with
#   the PR's base branch as the base. Slug, containers and cluster are named by <task-slug>
#   as usual; re-running the same command re-attaches. The exit menu pushes to the PR's head
#   branch (never force) and never offers to create a branch or PR. The PR is looked up in
#   the repo `origin` points at (gh -R), must be OPEN, and must not come from a fork. Every
#   refusal (ERROR: PR_MODE_REFUSED REASON=...) happens before any clone or container exists.
#   Not combinable with a positional base-branch, --destroy or --refresh-creds.
#   With `-- <command>` (e.g. `-- bash` for a hand fix), PR mode still ends with the exit menu
#   ([p] push, rejection recovery) instead of report-and-exit; [r] re-runs the same command.
#   - Return path is the exit menu's push. When that push is rejected, it is /absorb-devloop —
#     into a fresh host worktree on the PR's branch, never your current branch (absorbed there it
#     would cherry-pick the PR's commits, e.g. a dependency bump, onto it).
#   - Dependabot PRs: once you push to a dependabot/* branch, Dependabot stops rebasing that
#     PR, and `@dependabot rebase` / `@dependabot recreate` force-push over the branch and
#     DISCARD your commits.
#   - Push rejected (someone, e.g. Dependabot, pushed meanwhile): the menu PRINTS, never runs, the
#     recovery. PR mode: one `devloop.sh --pr <n> <slug> -- bash -c 'git fetch <https-url> ... &&
#     git rebase FETCH_HEAD'` command — the rebase runs INSIDE the container, fetching anonymously
#     from the PR's (public) repo, and ends in the menu for [p] (print_push_recovery).
#     Fallback (private repo / no network, or outside PR mode): a host-side recovery with this run's values (print_absorb_recovery): fetch
#     origin; `git worktree add -b <head> <repo>/../<slug>-rebase origin/<head>`; absorb the clone
#     there; push from the host, never forced (+ `gh pr create` after a rejected [c]); only then
#     remove the worktree and --destroy. It also prints what to do if absorb stops on the PR's
#     original commit, and the remedy (never applied) when a local <head> branch, a worktree on
#     <head> or a leftover <slug>-rebase makes the worktree add refuse.
#     Trust boundary: absorb's cherry-pick runs the repo's pre-commit hook on container-authored
#     commits on the host — the crossing every /absorb-devloop already makes, and accepted. What
#     holds is that the host never runs git with the clone as its repo or cwd; the container
#     fetches what it needs itself (anonymous HTTPS), so do not add a host-made bundle or an
#     always-on /tmp/devloop mount to hand it commits.
#   - A local branch this script fetched into the host repo (any mode; in PR mode, <head>) is
#     deleted by --destroy when it is still that unchanged copy of origin; if it moved, it is kept
#     and the command printed.
#     A kept or older (pre-tracking) local <head>: if the PR moves later, the
#     next --pr run refuses it as stale (REASON=local-branch-stale) and prints the remedy (inspect, then
#     `git branch -D <head>` only if it holds nothing you need).
#
# Host-side rule: once its container exists, the clone's .git (config, hooks) is writable from
# inside it, so this script never runs git or gh with the clone as the repo or cwd. The exit
# menu fetches the clone's branch into the host repo (refs/devloop/<slug>/head) and lists and
# pushes from there, to origin's URL, never forced. Do not run git against the clone on the host
# yourself either (see infra/lib/clone-head.sh).
#
# Clone on the wrong branch: if an existing clone's checked-out branch (detached HEAD
# included) is not the one this run expects (feature/<slug>, or the PR head with --pr), the
# run refuses before anything starts (ERROR: CLONE_BRANCH_MISMATCH). This is the one
# deliberate change to invocations without --pr: before, such a clone re-attached and the
# exit menu listed HEAD's commits but pushed / opened a PR for feature/<slug> — a different
# branch. Fix: switch back INSIDE the container (`podman exec -it -w /work devloop-<slug>-dev
# git switch <branch>` — never `git -C <clone>` on the host; if the containers are stopped, start
# them first: `podman start devloop-<slug>-dev devloop-<slug>-db`), or --pr <n>, or --destroy.
#
# Prerequisites:
#   - podman installed
#   - Authenticated with Claude Code, EITHER:
#       token mode (preferred for unattended story runs): run infra/devloop/claude-token.sh
#         once — it runs `claude setup-token` and writes the token file
#         (~/.config/dark-tower/claude-oauth-token, override DEVLOOP_CLAUDE_TOKEN_FILE).
#         Rotate before expiry: rerun it, then --recreate any live slug.
#       credentials mode: run 'claude' once to log in (~/.claude/.credentials.json)
#
# See: docs/decisions/adr-0025-containerized-devloop.md

# Parse the WHOLE script before running any of it. Bash reads a script from disk
# as it executes. Git checkouts replace the file (new inode, so a running bash keeps
# the old bytes), but an IN-PLACE write (an editor, a tool's file write) changes
# the bytes under bash's read offset: it resumes mid-statement in the new content,
# which is a syntax error or, worse, the wrong command. The braces force one parse
# up front; the `exit` before the closing brace keeps bash from reading past it.
# Checked by scripts/parse-whole-script.test.sh.
{
set -euo pipefail

# ─── Node pin (SSoT = repo-root .nvmrc) ─────────────────────────
# The Dockerfile pins Node via `ARG NODE_VERSION` (no default; fails loud if unset).
# The build context is infra/devloop/ (SCRIPT_DIR), which cannot COPY the repo-root
# .nvmrc, so we read .nvmrc here and pass it as --build-arg at the one `podman build`
# site (build_devloop_image). This keeps .nvmrc the single source and prevents the
# .nvmrc/Dockerfile/lockfile pin drift behind the 2026-08-05 host dev-env failure. Fails loud on a missing/empty
# .nvmrc rather than defaulting to a floating version.
read_node_version() {
    local script_dir="$1" nvmrc ver
    nvmrc="${script_dir}/../../.nvmrc"
    [[ -r "$nvmrc" ]] || { echo "ERROR: cannot read ${nvmrc} (SSoT for the container Node pin)." >&2; exit 1; }
    ver="$(tr -d '[:space:]' < "$nvmrc")"
    [[ -n "$ver" ]] || { echo "ERROR: ${nvmrc} is empty — cannot derive NODE_VERSION." >&2; exit 1; }
    printf '%s\n' "$ver"
}

# ─── sqlx-cli pin (SSoT = Cargo.lock's `sqlx`) ──────────────────
# The devloop image's sqlx-cli migrates the unit-test DB
# (scripts/lang/rust/test.sh). It must match the workspace's sqlx library so the
# `_sqlx_migrations` table it writes is the one `#[sqlx::test]` reads. The
# Dockerfile takes it as `ARG SQLX_CLI_VERSION` (no default; fails loud if
# unset); the value comes from the ONE Cargo.lock reader, shared with
# infra/kind/scripts/deploy.sh (the db-migrate image). Fails loud, never floats.
# shellcheck source=../lib/cargo-lock-version.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/cargo-lock-version.sh"
# The Kind scripts' shared prelude (definitions only): cluster_exists is the ONE Kind-cluster
# existence check, with a failed listing (rc 2) kept distinct from "absent".
# shellcheck source=../kind/scripts/lib/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../kind/scripts/lib/common.sh"
read_sqlx_cli_version() {
    local script_dir="$1"
    cargo_lock_version "${script_dir}/../../Cargo.lock" sqlx || exit 1
}

# ─── Rust pin (SSoT = repo-root rust-toolchain.toml) ────────────
# The Dockerfile takes `ARG RUST_VERSION` (no default) for its `FROM rust:<v>-slim-bookworm`.
# The build context is infra/devloop/, which cannot COPY the repo-root file, so the value
# comes from the ONE reader, shared with infra/kind/scripts/deploy.sh. Fails loud.
# shellcheck source=../lib/rust-toolchain.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/rust-toolchain.sh"
read_rust_version() {
    local script_dir="$1"
    rust_toolchain_version "${script_dir}/../../rust-toolchain.toml" || exit 1
}

# ─── pnpm pin (SSoT = repo-root package.json `packageManager`) ──
# The Dockerfile takes the full `pnpm@X.Y.Z+sha512.<hex>` spec as `ARG PNPM_PACKAGE_MANAGER`
# (no default; fails loud if unset) and hands it to `corepack prepare`, which verifies the
# hash. The value comes from the ONE packageManager reader, shared with scripts/dev-web.sh.
# shellcheck source=../lib/package-manager.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/package-manager.sh"
read_pnpm_package_manager() {
    local script_dir="$1"
    pnpm_package_manager_spec "${script_dir}/../../package.json" || exit 1
}

# ─── Playwright pin (SSoT = pnpm-lock.yaml's `playwright`) ───────
# The image bakes Chromium for ONE Playwright version, and Playwright refuses to launch a
# browser revision its package doesn't record, so the image must follow the lockfile the
# tests run against. The Dockerfile takes `ARG PLAYWRIGHT_VERSION` (no default; fails loud
# if unset); the value comes from the ONE pnpm-lock.yaml reader, which fails loudly if the
# workspace resolves more than one playwright version.
# shellcheck source=../lib/pnpm-lock-version.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/pnpm-lock-version.sh"
read_playwright_version() {
    local script_dir="$1"
    pnpm_lock_version "${script_dir}/../../pnpm-lock.yaml" playwright || exit 1
}

# ─── cargo tool pins (SSoT = infra/cargo-tools.versions) ────────
# cargo-nextest (Layer 4's runner), cargo-llvm-cov and cargo-audit: the same pins CI
# installs, read through the ONE reader. The Dockerfile ARGs have no defaults.
# shellcheck source=../lib/cargo-tools.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/cargo-tools.sh"
# clone_head_branch: the clone's branch read from its HEAD file. Host-side rule (see the lib):
# once a container exists, the host never runs git with the clone as its repo — the clone's
# .git/config and hooks are container-writable. Commits leave the clone only via
# fetch_clone_branch (a host-repo fetch FROM it); every push is from the host repo.
# shellcheck source=../lib/clone-head.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/clone-head.sh"
read_cargo_tool_version() {
    local script_dir="$1" tool="$2"
    cargo_tool_version "${script_dir}/../../infra/cargo-tools.versions" "$tool" || exit 1
}

# ─── Image build (the ONE build site) ───────────────────────────
# Every pin is read here, once per build, and passed as a build arg; the previous image
# is removed only after a successful build produced a different one.
build_devloop_image() {
    local script_dir="$1" image="$2" old_image_id
    local RUST_VERSION NODE_VERSION SQLX_CLI_VERSION PNPM_PACKAGE_MANAGER PLAYWRIGHT_VERSION
    local CARGO_NEXTEST_VERSION CARGO_LLVM_COV_VERSION CARGO_AUDIT_VERSION
    echo "Building dev container image..."
    old_image_id=$(podman images -q "$image" 2>/dev/null || true)
    NODE_VERSION="$(read_node_version "$script_dir")"
    SQLX_CLI_VERSION="$(read_sqlx_cli_version "$script_dir")"
    PNPM_PACKAGE_MANAGER="$(read_pnpm_package_manager "$script_dir")"
    RUST_VERSION="$(read_rust_version "$script_dir")"
    PLAYWRIGHT_VERSION="$(read_playwright_version "$script_dir")"
    CARGO_NEXTEST_VERSION="$(read_cargo_tool_version "$script_dir" cargo-nextest)"
    CARGO_LLVM_COV_VERSION="$(read_cargo_tool_version "$script_dir" cargo-llvm-cov)"
    CARGO_AUDIT_VERSION="$(read_cargo_tool_version "$script_dir" cargo-audit)"
    podman build --build-arg "RUST_VERSION=${RUST_VERSION}" \
        --build-arg "NODE_VERSION=${NODE_VERSION}" \
        --build-arg "SQLX_CLI_VERSION=${SQLX_CLI_VERSION}" \
        --build-arg "PNPM_PACKAGE_MANAGER=${PNPM_PACKAGE_MANAGER}" \
        --build-arg "PLAYWRIGHT_VERSION=${PLAYWRIGHT_VERSION}" \
        --build-arg "CARGO_NEXTEST_VERSION=${CARGO_NEXTEST_VERSION}" \
        --build-arg "CARGO_LLVM_COV_VERSION=${CARGO_LLVM_COV_VERSION}" \
        --build-arg "CARGO_AUDIT_VERSION=${CARGO_AUDIT_VERSION}" -t "$image" "$script_dir"
    if [ -n "$old_image_id" ] && [ "$old_image_id" != "$(podman images -q "$image")" ]; then
        podman rmi "$old_image_id" 2>/dev/null || true
    fi
}

# ─── Configuration ──────────────────────────────────────────────

REBUILD_IMAGE=false
REFRESH_CREDS=false
RECREATE=false
DESTROY=false
# --no-cluster: skip the devloop helper + Kind cluster (bring-up and health check) for this run.
CLUSTER_ENABLED=true
# Model for the interactive session, which is the Lead when /devloop runs.
# Teammates take their model from .claude/agents/*.md; the Lead has no agent
# file, so this is the only place its model can be set.
# The model's single home is "model" in .claude/settings.json, which the in-container
# session reads from the clone; every teammate inherits it. DEVLOOP_LEAD_MODEL is an
# optional one-off override (passed as --model only when set), not a second default.
DEVLOOP_LEAD_MODEL="${DEVLOOP_LEAD_MODEL:-}"
LEAD_MODEL_ARGS=()
[[ -n "$DEVLOOP_LEAD_MODEL" ]] && LEAD_MODEL_ARGS=(--model "$DEVLOOP_LEAD_MODEL")
# Hard memory cap on the dev container (2026-10-22). Without one, a single
# runaway test (story 3 task 12: an exhaustive model check at 12 GB) took the
# whole WSL VM into swap, and the Kind node, Prometheus and the shell froze with
# it. Capped and with swap equal to the limit (no container swap), the runaway is
# OOM-killed inside the container and fails its layer loudly instead.
# Default is derived from the host, not hardcoded: 70% of MemTotal, leaving the
# rest for the Kind node, the observability stack and the OS. Override with
# DEVLOOP_CONTAINER_MEMORY (a podman size, e.g. 12g). Like every `podman run`
# flag it takes effect only on container (re)create (--recreate).
DEVLOOP_CONTAINER_MEMORY="${DEVLOOP_CONTAINER_MEMORY:-}"
if [[ -z "$DEVLOOP_CONTAINER_MEMORY" ]]; then
    __mem_total_kib="$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || true)"
    [[ "$__mem_total_kib" =~ ^[0-9]+$ ]] || { echo "ERROR: cannot read MemTotal from /proc/meminfo to size the dev-container memory cap; set DEVLOOP_CONTAINER_MEMORY (e.g. 10g)." >&2; exit 1; }
    DEVLOOP_CONTAINER_MEMORY="$(( __mem_total_kib * 70 / 100 / 1024 ))m"
fi
[[ "$DEVLOOP_CONTAINER_MEMORY" =~ ^[1-9][0-9]*[kmg]$ ]] || { echo "ERROR: DEVLOOP_CONTAINER_MEMORY='${DEVLOOP_CONTAINER_MEMORY}' is not a podman size like 10g or 9500m." >&2; exit 1; }
# PR mode (--pr <n>): empty = off. Every PR-mode refusal goes through pr_refuse so the
# operator (and devloop-pr.test.sh) gets ONE stable token line, then the remedy lines.
USAGE_BASE="devloop.sh [--rebuild|--refresh-creds|--recreate|--destroy] <task-slug> [base-branch] [-- <command...>]"
USAGE_PR="devloop.sh [--rebuild] [--recreate] --pr <number> <task-slug> [-- <command...>]"
# The ONE push: from the HOST repo (its own trusted config and hooks — never the clone's),
# to origin's URL, fully-qualified destination, no `+`, so never a forced update.
# host_push_cmd <source-commit> <branch> sets HOST_PUSH (argv); every push site and every
# manual-push hint is built from it.
host_push_cmd() { HOST_PUSH=(git -C "$REPO_ROOT" push -q "$GITHUB_REMOTE" "${1}:refs/heads/${2}"); }
# record_created_branch / release_created_branch: a host-repo local branch this script created
# (a copy of origin's branch, for the clone to start from) is recorded in the HOST repo's config
# (devloop.<slug>.createdBranch/createdOid — host-written, never the clone's). --destroy deletes it
# iff it is still exactly that copy and not checked out anywhere; anything else is kept, loudly.
record_created_branch() {
    git -C "$REPO_ROOT" config "devloop.${TASK_SLUG}.createdBranch" "$BRANCH_NAME"
    git -C "$REPO_ROOT" config "devloop.${TASK_SLUG}.createdOid" \
        "$(git -C "$REPO_ROOT" rev-parse --verify "refs/heads/${BRANCH_NAME}^{commit}")"
}
release_created_branch() {
    local b oid have
    b="$(git -C "$REPO_ROOT" config --get "devloop.${TASK_SLUG}.createdBranch")" || {
        git -C "$REPO_ROOT" config --remove-section "devloop.${TASK_SLUG}" 2>/dev/null || true
        return 0
    }
    oid="$(git -C "$REPO_ROOT" config --get "devloop.${TASK_SLUG}.createdOid" || true)"
    have="$(git -C "$REPO_ROOT" rev-parse -q --verify "refs/heads/${b}^{commit}" || true)"
    if [[ -z "$have" ]]; then
        :   # already gone
    elif [[ -n "$oid" && "$have" == "$oid" ]] && git -C "$REPO_ROOT" branch -q -D "$b" 2>/dev/null; then
        echo "Deleted local branch ${b} (created by devloop.sh from origin, unchanged since)."
    else
        echo "Kept local branch ${b}: it moved since devloop.sh created it at ${oid:0:12}, or it is checked out." >&2
        echo "  Delete it by hand once nothing on it is needed: $(printf 'git -C %q branch -D %q' "$REPO_ROOT" "$b")" >&2
    fi
    git -C "$REPO_ROOT" config --remove-section "devloop.${TASK_SLUG}" 2>/dev/null || true
}

# quiet_unless_fails <cmd...>: run it with its output captured; print the output (to stderr)
# only when it fails, and return its exit code. For routine steps whose success is noise.
quiet_unless_fails() {
    local out rc=0
    out="$("$@" 2>&1)" || rc=$?
    [ "$rc" -eq 0 ] || printf '%s\n' "$out" >&2
    return "$rc"
}
# local_branch_delete_hint <branch> <sha>: the ONE remedy text for a host-repo local branch
# that may hold someone's commits (pr_refuse local-branch-stale, print_absorb_recovery). Never
# deleted or overwritten by this script; the operator inspects first.
local_branch_delete_hint() {
    local r b; r="$(printf '%q' "$REPO_ROOT")"; b="$(printf '%q' "$1")"
    echo "The host repo's local ${1} is at ${2}; it may hold your commits, so it is never overwritten."
    echo "  Inspect: git -C ${r} fetch origin && git -C ${r} log --oneline origin/${b}..${b}"
    echo "  If that prints nothing: git -C ${r} branch -D ${b}"
}
# manual_pr_create_hint: the ONE copy-pasteable PR creation for BRANCH_NAME into BASE_BRANCH
# (the [c] arm's own gh call adds title/body; this is what an operator runs by hand).
manual_pr_create_hint() {
    printf 'gh pr create -R %q --base %q --head %q' "$GITHUB_REMOTE" "$BASE_BRANCH" "$BRANCH_NAME"
}
# manual_push_hint <branch>: the copy-pasteable host-side push of the clone's <branch> — the
# same fetch_from_clone (flag set CLONE_FETCH_ARGS) and host_push_cmd the menu runs.
manual_push_hint() {
    host_push_cmd "$DEVLOOP_REF" "$1"
    printf 'git -C %s update-ref -d %s; git -C %s %s %s refs/heads/%s:%s && %s' \
        "$REPO_ROOT" "$DEVLOOP_REF" "$REPO_ROOT" "${CLONE_FETCH_ARGS[*]}" "$CLONE_DIR" "$1" "$DEVLOOP_REF" "${HOST_PUSH[*]}"
}
PR_NUMBER=""
pr_refuse() {
    local reason="$1"; shift
    echo "ERROR: PR_MODE_REFUSED REASON=${reason} PR=${PR_NUMBER:-?}" >&2
    local line
    for line in "$@"; do echo "  ${line}" >&2; done
    exit 1
}
while [[ "${1:-}" == --* ]]; do
    case "$1" in
        --rebuild) REBUILD_IMAGE=true; shift ;;
        --refresh-creds) REFRESH_CREDS=true; shift ;;
        --recreate) RECREATE=true; shift ;;
        --destroy) DESTROY=true; shift ;;
        --no-cluster) CLUSTER_ENABLED=false; shift ;;
        --pr)
            [[ -z "$PR_NUMBER" ]] || pr_refuse pr-given-twice "--pr may be given once."
            # Validated BEFORE it reaches gh: a value like `-R…` or `--web` would be read as a gh option.
            [[ "${2:-}" =~ ^[1-9][0-9]*$ ]] || pr_refuse bad-pr-number "--pr takes a PR number (got '${2:-}')."
            PR_NUMBER="$2"; shift 2 ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done
if ! $CLUSTER_ENABLED && { $DESTROY || $REFRESH_CREDS; }; then
    # --destroy always tears the cluster down and --refresh-creds never starts one: --no-cluster
    # there would be silently meaningless, so refuse rather than ignore it.
    echo "ERROR: --no-cluster only applies to a launch/re-attach, not to --destroy or --refresh-creds." >&2
    exit 1
fi
if [[ -n "$PR_NUMBER" ]]; then
    # Teardown and credential refresh must never depend on a PR lookup (the PR may be merged,
    # gh may be down) — and silently ignoring --pr there would hide a mistyped command.
    $DESTROY && pr_refuse with-destroy "--destroy needs no PR: devloop.sh --destroy <task-slug>"
    $REFRESH_CREDS && pr_refuse with-refresh-creds "--refresh-creds needs no PR: devloop.sh --refresh-creds <task-slug>"
    [[ -n "${1:-}" && "$1" != "--" ]] || pr_refuse no-slug "Usage: ${USAGE_PR}"
fi

# --rebuild with no task-slug: just build the image and exit
if $REBUILD_IMAGE && [ -z "${1:-}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    IMAGE="darktower-dev:latest"
    build_devloop_image "$SCRIPT_DIR" "$IMAGE"
    echo "Image rebuilt: ${IMAGE}"
    exit 0
fi

TASK_SLUG="${1:?Usage: ${USAGE_BASE} | ${USAGE_PR}}"
shift
# Kind cluster names must be DNS labels (a-z, 0-9, hyphens only).
if [[ ! "$TASK_SLUG" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]]; then
    echo "ERROR: Invalid task slug: '${TASK_SLUG}'" >&2
    echo "  Must be lowercase alphanumeric with hyphens (e.g., 'my-task-42')" >&2
    exit 1
fi

# --- Cluster name-length bound (#1), DERIVED from 63 — reject LOUDLY at launch,
#     before any container or cluster is created. ---
# KIND names the control-plane node `<cluster>-control-plane`, a Kubernetes DNS
# label capped at 63 chars. Exactly one control-plane node is declared in
# infra/kind/kind-config.yaml.tmpl (the config the helper renders for a devloop
# cluster), so `-control-plane` is the longest node-name suffix. The cluster name
# here is `${CLUSTER_PREFIX}${TASK_SLUG}`, so the operative bound is on the SLUG.
# Derive the caps from the two string constants (never type 49/41) so the bound
# stays honest if a prefix/suffix ever changes. CLUSTER_PREFIX is the SAME constant
# that builds CLUSTER_NAME below (single source of truth). NO truncation/hash — that
# would silently collide two devloops onto one cluster.
CLUSTER_PREFIX="devloop-"
NODE_SUFFIX="-control-plane"       # cross-ref: infra/kind/kind-config.yaml.tmpl (single control-plane node; see the NOTE above `nodes:`)
DNS_LABEL_MAX=63
CLUSTER_NAME_MAX=$(( DNS_LABEL_MAX - ${#NODE_SUFFIX} ))   # 49
SLUG_MAX=$(( CLUSTER_NAME_MAX - ${#CLUSTER_PREFIX} ))     # 41
if (( ${#TASK_SLUG} > SLUG_MAX )); then
    # SUBJECT= disambiguates which entity NAME=/LEN=/MAX= describe (this shared token
    # also fires from the Kind scripts (lib/common.sh) with SUBJECT=cluster-name). Here it is the
    # SLUG the operator typed; MAX is the computed cap, never a literal. A
    # SUBJECT=cluster-name line reaching an operator who launched via devloop.sh means
    # this launcher check was bypassed — itself a finding.
    echo "ERROR: CLUSTER_NAME_TOO_LONG SUBJECT=slug NAME=${TASK_SLUG} LEN=${#TASK_SLUG} MAX=${SLUG_MAX}" >&2
    echo "  Task slug '${TASK_SLUG}' is ${#TASK_SLUG} chars; shorten it to <= ${SLUG_MAX}." >&2
    echo "  (Cluster name is '${CLUSTER_PREFIX}<slug>', whose '${NODE_SUFFIX}' node name must fit the ${DNS_LABEL_MAX}-char DNS label.)" >&2
    exit 1
fi

# Optional base-branch, then an optional `-- <command...>`: everything after the
# separator runs in the container instead of the interactive claude attach. Kept
# deliberately general — devloop.sh sets up an environment, and encoding any
# particular tool's flags here would be a second copy of that tool's interface,
# drifting from the one it describes. Unattended story runs are just:
#   devloop.sh <slug> -- scripts/workflow/run-story.sh <story> --stop-after=1
if [[ -n "${1:-}" && "$1" != "--" ]]; then
    [[ -z "$PR_NUMBER" ]] || pr_refuse with-base-branch \
        "--pr takes its base branch from the PR (got positional base '$1')." \
        "Usage: devloop.sh --pr ${PR_NUMBER} ${TASK_SLUG} [-- <command...>]"
    BASE_BRANCH="$1"
    shift
else
    BASE_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
fi
EXEC_CMD=()
if [[ "${1:-}" == "--" ]]; then
    shift
    EXEC_CMD=("$@")
    [ ${#EXEC_CMD[@]} -gt 0 ] || { echo "ERROR: -- given with no command to run" >&2; exit 1; }
fi

# Resolve paths relative to the repo root
REPO_ROOT="$(git rev-parse --show-toplevel)"
CLONE_DIR="${REPO_ROOT}/../worktrees/${TASK_SLUG}"
DEV_CONTAINER="devloop-${TASK_SLUG}-dev"
DB_CONTAINER="devloop-${TASK_SLUG}-db"
IMAGE="darktower-dev:latest"
# Default branch/base; PR mode (--pr) replaces both with the PR's head/base in resolve_pr.
BRANCH_NAME="feature/${TASK_SLUG}"
# Host-repo ref the exit menu fetches the clone's branch into (fetch_clone_branch); removed by cleanup.
DEVLOOP_REF="refs/devloop/${TASK_SLUG}/head"
GITHUB_REMOTE="$(git -C "$REPO_ROOT" remote get-url origin)"

# Git identity — defaults to your git config
GIT_AUTHOR_NAME="${GIT_AUTHOR_NAME:-$(git config user.name)}"
GIT_AUTHOR_EMAIL="${GIT_AUTHOR_EMAIL:-$(git config user.email)}"

# Helper binary configuration (ADR-0030: host-side cluster helper)
# Separate --target-dir avoids cargo lock conflicts with container builds.
# Built from $REPO_ROOT (host source), NOT $CLONE_DIR — security invariant.
HELPER_TARGET_DIR="${REPO_ROOT}/target/devloop-helper"
HELPER_BINARY="${HELPER_TARGET_DIR}/release/devloop-helper"
HELPER_RUNTIME_DIR="/tmp/devloop-${TASK_SLUG}"

# Story-runner ledger (ADR-0037 D5). TWO-KEY DECOMPOSITION — read before touching:
# the story-runner keeps TWO run-artifact bases with different lifetimes.
#   - The in-flight MARKER + substrate-probe cache are per-CONTAINER and ephemeral;
#     they stay on HELPER_RUNTIME_DIR (the /tmp/devloop mount) — INFLIGHT_MARKER
#     below still reads them there, UNCHANGED.
#   - The cost LEDGER + durable per-task evidence are per-STORY and MUST survive
#     container teardown, so they live on this host-persistent, git-external, FLAT
#     dir — NOT under HELPER_RUNTIME_DIR, which cleanup() deletes.
# STORY_RUNS_HOST is flat (shared across per-slug containers); run-story keys it
# per-story under story-runner/<story>. Do NOT move the marker onto this mount and
# do NOT make this per-slug — see run-story.sh's resolve_run_base + the seam block.
STORY_RUNS_HOST="${HOME}/.cache/devloop/story-runs"
# SINGLE SOURCE of the in-container ledger path (DRY): used BOTH as the bind-mount
# target AND passed to run-story via -e DEVLOOP_STORY_RUN_BASE, so the two sides
# cannot drift (they agreed before only because the image user happens to be `dev`).
# validate-run-dir-path-sync.sh statically binds this to run-story's default.
CONTAINER_LEDGER_BASE="/home/dev/.cache/devloop/story-runs"

# ─── Quick actions (early exit) ───────────────────────────────────

# Token mode: path, file format, reader (load_oauth_token) and writer all live in
# claude-token.sh so the format has one home. Sourcing it defines functions only.
# shellcheck source-path=SCRIPTDIR source=claude-token.sh
source "$(dirname "${BASH_SOURCE[0]}")/claude-token.sh"
# Claude memories saved inside the dev container are copied out before it is
# removed (and after each session); see memory-harvest.sh.
# shellcheck source-path=SCRIPTDIR source=memory-harvest.sh
source "$(dirname "${BASH_SOURCE[0]}")/memory-harvest.sh"

refresh_credentials() {
    if $TOKEN_MODE; then
        echo "Token mode (${CLAUDE_TOKEN_FILE}): the token is set when the container is created," >&2
        echo "  so there is nothing to refresh in place. To pick up a rotated token (or to move a" >&2
        echo "  container created in credentials mode onto the token), recreate its containers —" >&2
        echo "  the clone, story-run ledgers and Kind cluster are preserved:" >&2
        echo "    devloop.sh --recreate ${TASK_SLUG}   (add --pr <number> if this slug was started with --pr)" >&2
        exit 1
    fi
    if [ ! -f "${HOME}/.claude/.credentials.json" ]; then
        echo "No credentials file found at ${HOME}/.claude/.credentials.json" >&2
        exit 1
    fi
    if ! podman container exists "$DEV_CONTAINER" 2>/dev/null; then
        echo "Container not running: ${DEV_CONTAINER}" >&2
        exit 1
    fi
    podman cp "${HOME}/.claude/.credentials.json" "${DEV_CONTAINER}:/home/dev/.claude/.credentials.json"
    echo "Credentials refreshed in ${DEV_CONTAINER}"
}

load_oauth_token

if $REFRESH_CREDS; then
    refresh_credentials
    exit 0
fi

# ─── Helper Functions ────────────────────────────────────────────

# Check if a PID belongs to a running devloop-helper process.
# Guards against PID recycling by checking /proc/$pid/cmdline.
is_helper_process_alive() {
    local pid="$1"
    kill -0 "$pid" 2>/dev/null && grep -q "devloop-helper" "/proc/$pid/cmdline" 2>/dev/null
}

cleanup() {
    harvest_before_removal "$DEV_CONTAINER" "$TASK_SLUG" "$REPO_ROOT" || exit 1
    echo "Destroying containers..."
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true

    # Remove named network (ADR-0030)
    podman network rm "devloop-${TASK_SLUG}-net" 2>/dev/null || true

    # Kill helper process (graceful SIGTERM, then force SIGKILL)
    if [ -f "$HELPER_RUNTIME_DIR/helper.pid" ]; then
        local pid
        pid=$(cat "$HELPER_RUNTIME_DIR/helper.pid")
        if is_helper_process_alive "$pid"; then
            echo "Stopping helper (PID $pid)..."
            kill "$pid" 2>/dev/null || true
            for _ in $(seq 1 10); do
                kill -0 "$pid" 2>/dev/null || break
                sleep 0.5
            done
            kill -9 "$pid" 2>/dev/null || true
        fi
    fi

    # Delete Kind cluster if kind is available
    if command -v kind &>/dev/null; then
        echo "Deleting Kind cluster: ${CLUSTER_PREFIX}${TASK_SLUG}..."
        kind delete cluster --name "${CLUSTER_PREFIX}${TASK_SLUG}" 2>/dev/null || true
    fi

    # Reclaim the host build cruft this devloop produced — the service images and build
    # cache nothing else GCs (the multi-hundred-GB leak that exhausts container storage
    # over time; see docs/TODO.md §Devloop Container Resource Hygiene item A). CONSERVATIVE,
    # dangling/unused-ONLY prune — NOT -af: first-party images are tagged by CONTENT
    # (`localhost/<name>:sha-<image id>`, ADR-0038 §2), so two slugs building identical code
    # share a tag, and `-af` could yank an image a *parallel* slug is between building and
    # loading. Dangling-only is cross-slug-safe: orphaned layers + unused build cache only,
    # never a tagged image a concurrent devloop still references. deploy.sh already prunes
    # each cluster's superseded refs after every converge; the precise per-slug rmi of the
    # LAST generation at cleanup needs a record of the deployed refs that outlives the
    # cluster (docs/TODO.md item A). podman-only by design — consistent with every other
    # container op in this script; a docker-built host (KIND_EXPERIMENTAL_PROVIDER=docker)
    # wouldn't be reclaimed here, acceptable under the podman-primary assumption.
    if command -v podman &>/dev/null; then
        echo "Reclaiming dangling images + unused build cache..."
        podman image prune -f 2>/dev/null || true
        podman builder prune -f 2>/dev/null || true
    fi

    # Remove helper runtime directory.
    # DO NOT add "$STORY_RUNS_HOST" (~/.cache/devloop/story-runs) to any rm here:
    # it is the host-persistent cost ledger + escalation/incident evidence (ADR-0037
    # D5), the whole point of which is to survive --destroy, and it is FLAT/shared
    # across slugs — deleting it on one slug's teardown would nuke other slugs'
    # stories. The two rm's below are per-slug scoped (HELPER_RUNTIME_DIR and
    # devloop-${TASK_SLUG}), so they are safe; do NOT widen either to
    # "$HOME/.cache/devloop", which would take story-runs with it.
    rm -rf "$HELPER_RUNTIME_DIR"

    # Release port reservation (per-slug — NOT $HOME/.cache/devloop; see above)
    rm -rf "$HOME/.cache/devloop/devloop-${TASK_SLUG}"
    local registry="$HOME/.cache/devloop/port-registry.json"
    if [ -f "$registry" ] && command -v jq &>/dev/null; then
        jq --arg slug "$TASK_SLUG" '.entries |= map(select(.slug != $slug))' "$registry" > "${registry}.tmp" \
            && mv "${registry}.tmp" "$registry"
    fi

    echo "Removing clone: ${CLONE_DIR}"
    rm -rf "$CLONE_DIR"
    git -C "$REPO_ROOT" update-ref -d "$DEVLOOP_REF" 2>/dev/null || true
    release_created_branch
    echo "Story-runner ledger retained (survives --destroy by design): ${STORY_RUNS_HOST}"
    echo "Cleaned up."
}

# --destroy: tear down everything for this slug and exit. Placed after
# cleanup() definition because bash resolves function calls against the
# parsed body, not just declared names.
if $DESTROY; then
    cleanup
    exit 0
fi

# ─── Branch resolution + pre-flight (before ANY image build, helper, clone or container) ───

# A branch name from GitHub (PR mode) must be safe as ONE plain argv element in every git call.
# check-ref-format alone is not enough: it accepts `+foo` (a leading `+` makes a fetch/push
# refspec FORCED), `@` (= HEAD), `$(…)` and backticks. The allowlist closes those; `..`,
# a trailing `/` or `.lock` are refused explicitly. dependabot/cargo/x-1.2.3 passes.
safe_branch_name() {
    local b="$1"
    [[ "$b" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || return 1
    [[ "$b" != *..* && "$b" != */ && "$b" != *.lock && "$b" != *//* ]] || return 1
    git check-ref-format --branch "$b" >/dev/null 2>&1
}

# PR mode: ONE gh lookup, pinned with -R to the repo `origin` points at — the repo
# ensure_branch_ref fetches from and the clone pushes to — so an `upstream` remote or
# `gh repo set-default` cannot resolve PR <n> in a different repo. gh's stderr is left
# visible: on failure it is what says not-found vs auth vs network (one token, decided by
# exit status, never by gh's wording). Sets BRANCH_NAME/BASE_BRANCH and PR_HEAD_OID/PR_URL.
resolve_pr() {
    command -v gh >/dev/null 2>&1 \
        || pr_refuse gh-missing "PR mode needs the GitHub CLI (gh) on the host, authenticated for ${GITHUB_REMOTE}."
    local out state cross head_owner head_repo head_ref head_oid base_ref url
    # \u001f (unit separator), not a tab: tab is IFS whitespace, so `read` would collapse an
    # empty field and shift every later one. Every field is stringified, so null -> "null".
    if ! out="$(gh pr view "$PR_NUMBER" -R "$GITHUB_REMOTE" \
            --json state,isCrossRepository,headRepositoryOwner,headRepository,headRefName,headRefOid,baseRefName,url \
            --jq '[.state, .isCrossRepository, .headRepositoryOwner.login, .headRepository.name, .headRefName, .headRefOid, .baseRefName, .url] | map(tostring) | join("\u001f")')"; then
        pr_refuse lookup-failed "gh pr view ${PR_NUMBER} -R ${GITHUB_REMOTE} failed (gh's message is above: not found, auth or network)."
    fi
    IFS=$'\x1f' read -r state cross head_owner head_repo head_ref head_oid base_ref url <<< "$out"
    if [[ "$state" != "OPEN" ]]; then
        local hints=("PR #${PR_NUMBER} is ${state:-unknown}; --pr works only on an open PR.")
        if [ -d "$CLONE_DIR" ]; then
            # Name the real branch only once it is known to be a safe name (it is not vetted yet).
            local b="<branch>"
            safe_branch_name "$head_ref" && b="$head_ref"
            hints+=("A clone for '${TASK_SLUG}' exists: ${CLONE_DIR}"
                "  Push un-pushed work by hand (from the host repo, never git inside the clone):"
                "    $(manual_push_hint "$b")"
                "  That updates the branch only, not the ${state:-closed} PR (and recreates the branch if GitHub deleted it);"
                "  open a new PR if the work is still wanted."
                "  Then tear down: $0 --destroy ${TASK_SLUG}")
        fi
        pr_refuse not-open "${hints[@]}"
    fi
    # Fork check fails CLOSED: anything but the literal `false` (true, null, missing) refuses,
    # and the head repo must be the PR's own (base) repo, read from the PR's url.
    local url_re="^https://[^/]+/([^/]+)/([^/]+)/pull/${PR_NUMBER}\$"
    [[ "$url" =~ $url_re ]] \
        || pr_refuse unexpected-url "gh returned url '${url}', expected https://<host>/<owner>/<repo>/pull/${PR_NUMBER}."
    if [[ "$cross" != "false" || "$head_owner" != "${BASH_REMATCH[1]}" || "$head_repo" != "${BASH_REMATCH[2]}" ]]; then
        pr_refuse fork "PR #${PR_NUMBER} head is ${head_owner}/${head_repo} (isCrossRepository=${cross}); base is ${BASH_REMATCH[1]}/${BASH_REMATCH[2]}." \
            "Fork PRs are not supported: their head branch is not on origin, so the exit menu could not push to it."
    fi
    [[ "$head_oid" =~ ^[0-9a-f]{40}$ ]] || pr_refuse unexpected-head-oid "gh returned head commit '${head_oid}'."
    local which name
    for which in head base; do
        [[ "$which" == head ]] && name="$head_ref" || name="$base_ref"
        safe_branch_name "$name" || pr_refuse unsafe-ref \
            "PR #${PR_NUMBER} ${which} branch '${name}' is not a plain branch name (allowed: [A-Za-z0-9][A-Za-z0-9._/-]*, no '..', no trailing '/' or '.lock')."
    done
    BRANCH_NAME="$head_ref"
    BASE_BRANCH="$base_ref"
    PR_HEAD_OID="$head_oid"
    PR_URL_RESOLVED="$url"
}

# THE "is the clone on BRANCH_NAME" check (pre-flight, both modes). A clone on any other
# branch (or a detached HEAD) is broken for this run: the exit menu would list HEAD's commits
# but push/create a PR for BRANCH_NAME, a different branch. The clone is a host directory, so
# the operator can always fix it from the host. Returns 0 when there is no clone yet or it is
# on BRANCH_NAME; otherwise prints the ERROR + remedies and returns 1 (the caller exits).
check_clone_branch() {
    [ -d "$CLONE_DIR" ] || return 0
    local cur
    cur="$(clone_head_branch "$CLONE_DIR")" || cur="(no branch: detached HEAD or not a git clone)"
    [[ "$cur" == "$BRANCH_NAME" ]] && return 0
    echo "ERROR: CLONE_BRANCH_MISMATCH CLONE=${CLONE_DIR} ON=${cur} EXPECTED=${BRANCH_NAME}" >&2
    echo "  This run works on ${BRANCH_NAME}; the clone is on ${cur}. Nothing was started." >&2
    # In the container, never `git -C <clone>` on the host (its .git is container-writable).
    echo "  Switch it back (keeps everything), INSIDE the container while it is up: podman exec -it -w /work ${DEV_CONTAINER} git switch ${BRANCH_NAME}" >&2
    # Pre-flight runs before this script starts anything; after a reboot/podman stop they are down.
    # Both: a later re-attach checks only the dev container, so the DB must be up too.
    echo "  If the containers are stopped, start them first: podman start ${DEV_CONTAINER} ${DB_CONTAINER}" >&2
    if [[ -z "$PR_NUMBER" ]]; then
        echo "  If this clone was made with --pr, re-run with: $0 --pr <number> ${TASK_SLUG}" >&2
    else
        echo "  If this slug belongs to another PR or task, re-run with that --pr <number> (or none), or pick another <task-slug>." >&2
    fi
    echo "  Or tear it down: $0 --destroy ${TASK_SLUG}   (deletes the clone — push first)" >&2
    return 1
}

# Make sure the host has refs/heads/$BRANCH_NAME when origin has it (PR iteration), fetching
# it if only origin does. In PR mode it must ALSO be exactly the PR head the lookup vetted:
# absent on origin, a stale host branch, or a head that moved between lookup and fetch all
# refuse — a stale or diverged branch is never overwritten. Idempotent; PR mode runs it in
# pre-flight (so these refusals precede every podman call) and ensure_clone runs it again.
ensure_branch_ref() {
    local fetched=false rc
    if ! git show-ref --verify --quiet "refs/heads/${BRANCH_NAME}"; then
        local err=""
        rc=0
        err="$(git ls-remote --exit-code "$GITHUB_REMOTE" "refs/heads/${BRANCH_NAME}" 2>&1 >/dev/null)" || rc=$?
        if [ "$rc" -eq 0 ]; then
            echo "Fetching remote branch: ${BRANCH_NAME}"
            # Fully-qualified refspec: no `+`, so never a forced update.
            git fetch "$GITHUB_REMOTE" "refs/heads/${BRANCH_NAME}:refs/heads/${BRANCH_NAME}"
            fetched=true
            record_created_branch
        elif [[ -n "$PR_NUMBER" ]]; then
            [ "$rc" -eq 2 ] || pr_refuse origin-unreachable "git ls-remote ${GITHUB_REMOTE} failed (rc=${rc}): ${err}"
            pr_refuse head-not-on-origin "Branch ${BRANCH_NAME} is not on ${GITHUB_REMOTE}."
        elif [ "$rc" -ne 2 ]; then
            # Not refused: an offline start keeps working, as before — but never silently.
            echo "WARNING: git ls-remote ${GITHUB_REMOTE} failed (rc=${rc}): ${err}" >&2
            echo "  Cannot tell whether ${BRANCH_NAME} exists on origin; creating a new ${BRANCH_NAME} from ${BASE_BRANCH}." >&2
            echo "  If it already exists on origin (PR iteration), this DIVERGES from it and a later push will be rejected." >&2
        fi
    fi
    [[ -n "$PR_NUMBER" ]] || return 0
    local have
    have="$(git rev-parse -q --verify "refs/heads/${BRANCH_NAME}^{commit}")" || have=""
    [[ "$have" == "$PR_HEAD_OID" ]] && return 0
    if $fetched; then
        pr_refuse head-moved "PR #${PR_NUMBER} head was ${PR_HEAD_OID} at lookup; origin's ${BRANCH_NAME} is now ${have}." \
            "Someone pushed meanwhile — re-run the same command."
    fi
    local hint
    mapfile -t hint < <(local_branch_delete_hint "$BRANCH_NAME" "$have")
    pr_refuse local-branch-stale "The host repo's local ${BRANCH_NAME} is ${have}; PR #${PR_NUMBER} head is ${PR_HEAD_OID}." \
        "${hint[@]}" \
        "If it is just behind: $(printf 'git -C %q fetch origin %q' "$REPO_ROOT" "refs/heads/${BRANCH_NAME}:refs/heads/${BRANCH_NAME}")"
}

# The Dependabot caveat, printed at launch and above the exit menu's push (one text, one test).
dependabot_caveat() {
    [[ "$BRANCH_NAME" == dependabot/* ]] || return 0
    echo "WARNING: ${BRANCH_NAME} is a Dependabot branch. Once you push to it Dependabot stops rebasing this PR," >&2
    echo "  and '@dependabot rebase' / '@dependabot recreate' force-push over it, DISCARDING your commits." >&2
}

PR_HEAD_OID=""
PR_URL_RESOLVED=""
[[ -z "$PR_NUMBER" ]] || resolve_pr

# Re-entry carries the mode. REENTER_ARGS: what the printed hints tell the operator to run.
# RERUN_ARGS: what the menu's [r] execs (today's form also re-passes the base branch;
# PR mode must not — a positional base is refused with --pr).
if [[ -n "$PR_NUMBER" ]]; then
    REENTER_ARGS=(--pr "$PR_NUMBER" "$TASK_SLUG")
    RERUN_ARGS=("${REENTER_ARGS[@]}")
    # PR mode + `-- <command>` ends in the menu; [r] re-runs the same command, not Claude.
    [ ${#EXEC_CMD[@]} -eq 0 ] || RERUN_ARGS+=(-- "${EXEC_CMD[@]}")
else
    REENTER_ARGS=("$TASK_SLUG")
    RERUN_ARGS=("$TASK_SLUG" "$BASE_BRANCH")
fi
if ! $CLUSTER_ENABLED; then
    REENTER_ARGS=(--no-cluster "${REENTER_ARGS[@]}")
    RERUN_ARGS=(--no-cluster "${RERUN_ARGS[@]}")
fi

if ! check_clone_branch; then
    [[ -z "$PR_NUMBER" ]] || pr_refuse clone-branch-mismatch "The clone for '${TASK_SLUG}' is not on PR #${PR_NUMBER}'s head branch (see above)."
    exit 1
fi
if [[ -n "$PR_NUMBER" ]]; then
    [ -d "$CLONE_DIR" ] || ensure_branch_ref
    echo "=== PR mode: #${PR_NUMBER} ${PR_URL_RESOLVED} ==="
    echo "  head:  ${BRANCH_NAME} @ ${PR_HEAD_OID:0:12}"
    echo "  base:  ${BASE_BRANCH}"
    if [ -d "$CLONE_DIR" ]; then
        echo "  clone: ${CLONE_DIR} (existing, on ${BRANCH_NAME})"
    else
        echo "  clone: ${CLONE_DIR} (new)"
    fi
fi

# Build the devloop-helper binary (ADR-0030).
# Always rebuilds — cargo no-ops if source unchanged (~0.1s), and stale
# binaries cause hard-to-debug port-map issues.
# Runs from $REPO_ROOT: rustup finds rust-toolchain.toml by the CURRENT DIRECTORY (not
# --manifest-path), so this is what builds the helper on the pinned Rust wherever
# devloop.sh is invoked from.
build_helper() {
    echo "Building devloop-helper..."   # usually a no-op; a cold build takes minutes
    (cd "$REPO_ROOT" && quiet_unless_fails cargo build --release -p devloop-helper \
        --target-dir "$HELPER_TARGET_DIR" \
        --manifest-path "$REPO_ROOT/Cargo.toml") \
        || { echo "ERROR: building devloop-helper failed (cargo output above)." >&2; return 1; }
}

# Detect the podman host-gateway IP address (ADR-0030).
# Detection order per ADR-0030 Section 2:
#   1. podman info (authoritative, no container needed)
#   2. Parse /etc/hosts for host.containers.internal
#   3. Fail with actionable error
detect_host_gateway_ip() {
    local gw_ip

    # Method 1: podman info (most authoritative — queries podman directly)
    gw_ip=$(podman info --format '{{.Host.NetworkBackendInfo.DNS.HostGatewayIP}}' 2>/dev/null)
    if [ -n "$gw_ip" ] && [ "$gw_ip" != "<nil>" ] && [ "$gw_ip" != "<no value>" ]; then
        echo "$gw_ip"
        return 0
    fi

    # Method 2: check loopback interface for podman's host-gateway address
    gw_ip=$(ip addr show lo 2>/dev/null | grep -oP '10\.255\.\d+\.\d+' | head -1)
    if [ -n "$gw_ip" ]; then
        echo "$gw_ip"
        return 0
    fi

    # Method 3: parse /etc/hosts for host.containers.internal
    gw_ip=$(grep 'host.containers.internal' /etc/hosts 2>/dev/null | awk '{print $1}' | head -1)
    if [ -n "$gw_ip" ]; then
        echo "$gw_ip"
        return 0
    fi

    echo "ERROR: Cannot detect host-gateway IP." >&2
    echo "  Method 1: 'podman info --format {{.Host.NetworkBackendInfo.DNS.HostGatewayIP}}' returned empty" >&2
    echo "  Method 2: 'ip addr show lo' has no 10.255.x.x address" >&2
    echo "  Method 3: /etc/hosts has no host.containers.internal entry" >&2
    echo "  This is required for container-to-Kind networking (ADR-0030)." >&2
    return 1
}

# Launch the helper as a background process, reusing if already alive.
launch_helper() {
    local pid_file="$HELPER_RUNTIME_DIR/helper.pid"

    # Check if already running
    if [ -f "$pid_file" ]; then
        local pid
        pid=$(cat "$pid_file")
        if is_helper_process_alive "$pid"; then
            return 0
        fi
        echo "Stale helper PID file found, will relaunch..."
    fi

    # Launch helper binary as a background process.
    #
    # --project-root points at CLONE_DIR (NOT REPO_ROOT) so image builds,
    # provision.sh/deploy.sh, and kind-config generation reflect the devloop's branch state
    # — `/work` inside the dev container is mounted from CLONE_DIR, so this
    # is what edits land in. The helper *binary* is still compiled from
    # REPO_ROOT (see build_helper above) — that pin is what protects the
    # binary from container tampering. See ADR-0030 §"Build-context
    # trichotomy" for the full security model.
    local helper_args=("$TASK_SLUG" --project-root "$CLONE_DIR")
    if [ -n "${HOST_GATEWAY_IP:-}" ]; then
        helper_args+=(--host-gateway-ip "$HOST_GATEWAY_IP")
    fi
    "$HELPER_BINARY" "${helper_args[@]}" \
        2>>"$HELPER_RUNTIME_DIR/helper-stderr.log" &
    local helper_pid=$!

    # Wait for socket to appear (up to 10s)
    local socket_path="$HELPER_RUNTIME_DIR/helper.sock"
    for _ in $(seq 1 20); do
        if [ -S "$socket_path" ]; then
            echo "Helper ready (PID $helper_pid)"
            return 0
        fi
        # Check if helper exited early (crash on startup)
        if ! kill -0 "$helper_pid" 2>/dev/null; then
            echo "ERROR: Helper process exited during startup" >&2
            if [ -f "$HELPER_RUNTIME_DIR/helper-stderr.log" ]; then
                echo "Helper stderr:" >&2
                tail -20 "$HELPER_RUNTIME_DIR/helper-stderr.log" >&2
            fi
            return 1
        fi
        sleep 0.5
    done

    echo "ERROR: Helper socket not ready after 10s" >&2
    kill "$helper_pid" 2>/dev/null || true
    return 1
}

# Detect orphaned Kind clusters with no corresponding running devloop container.
detect_orphan_clusters() {
    if ! command -v kind &>/dev/null; then
        return 0
    fi

    # Cluster-name sites: the prefix that FINDS the cluster must be the same
    # CLUSTER_PREFIX const that BUILDS it (:724) and that the length cap subtracts —
    # a rename that missed here would make this grep match nothing and report a clean
    # host, masking the orphaned-cluster leak this function exists to catch (Finding 4,
    # @dry-reviewer). The `-dev`/`-net`/`/tmp/devloop-` sites below share the string
    # only incidentally and are deliberately left as literals.
    # A FAILED listing is "unknown", never "no orphans" (lib/common.sh:cluster_exists's
    # rc-2 rule): the scan is advisory, so it warns and skips.
    local clusters listing
    if ! listing="$(kind get clusters 2>/dev/null)"; then
        echo "WARNING: 'kind get clusters' failed; cannot scan for orphaned devloop clusters. Check the host container runtime." >&2
        return 0
    fi
    clusters=$(grep "^${CLUSTER_PREFIX}" <<< "${listing}" || true)
    if [ -z "$clusters" ]; then
        return 0
    fi

    while IFS= read -r cluster; do
        local slug="${cluster#"${CLUSTER_PREFIX}"}"
        local dev_container="devloop-${slug}-dev"
        local helper_pid_file="/tmp/${cluster}/helper.pid"

        # A cluster is only orphaned if BOTH the dev container is not running
        # AND the helper process is dead/missing. This avoids a race where
        # devloop.sh is mid-startup (helper launched, container not yet created).
        local container_alive=false
        local helper_alive=false
        if podman ps --format "{{.Names}}" 2>/dev/null | grep -q "^${dev_container}$"; then
            container_alive=true
        fi
        if [ -f "$helper_pid_file" ]; then
            local hpid
            hpid=$(cat "$helper_pid_file")
            if is_helper_process_alive "$hpid"; then
                helper_alive=true
            fi
        fi

        if ! $container_alive && ! $helper_alive; then
            if [[ -t 0 ]]; then
                echo "Orphaned Kind cluster found: $cluster (no running container or helper)"
                read -p "  Delete? [y/N] " -n 1 -r
                echo
                if [[ $REPLY =~ ^[Yy]$ ]]; then
                    echo "  Deleting $cluster..."
                    kind delete cluster --name "$cluster" 2>/dev/null || true
                    rm -rf "/tmp/${cluster}" 2>/dev/null || true
                    podman network rm "devloop-${slug}-net" 2>/dev/null || true
                    # Clean stale port registry entry
                    local registry="$HOME/.cache/devloop/port-registry.json"
                    if [ -f "$registry" ] && command -v jq &>/dev/null; then
                        jq --arg slug "$slug" '.entries |= map(select(.slug != $slug))' "$registry" > "${registry}.tmp" \
                            && mv "${registry}.tmp" "$registry"
                    fi
                fi
            else
                echo "WARNING: Orphaned Kind cluster: $cluster (no running container or helper)"
            fi
        fi
    done <<< "$clusters"
}

menu_reenter_or_cleanup() {
    echo "  [r] Re-enter container"
    echo "  [d] Destroy containers and clone (un-pushed changes will be lost)"
    echo "  [q] Quit (containers stay running)"
    read -p "Choice: " -n 1 -r
    echo
    case $REPLY in
        r|R) exec "$0" "${RERUN_ARGS[@]}" ;;
        d|D) cleanup ;;
        *) echo "Containers still running. Re-enter with: $0 ${REENTER_ARGS[*]}" ;;
    esac
}

# Take the clone's BRANCH_NAME out into the HOST repo: the ONE host-side operation against the
# clone once its container exists (upload-pack runs in the clone: no hooks, pack hooks only
# from protected config). Lands on a private ref, deleted first so the fetch only ever CREATES
# it (never a forced update, and a rebased clone branch still fetches). Sets CLONE_SHA; every
# later listing and push works on host objects with host config.
fetch_clone_branch() {
    CLONE_SHA=""
    # The one fetch-from-clone (infra/lib/clone-head.sh): fsck, no tags, delete-then-create.
    if ! fetch_from_clone "$REPO_ROOT" "$CLONE_DIR" "$BRANCH_NAME" "$DEVLOOP_REF"; then
        echo "WARNING: could not read ${BRANCH_NAME} from the clone ${CLONE_DIR} (git's message is above); nothing can be listed or pushed." >&2
        return 1
    fi
    CLONE_SHA="$(git -C "$REPO_ROOT" rev-parse --verify "${DEVLOOP_REF}^{commit}")"
}

# Sets COMMITS (CLONE_SHA's commits since BASE_BRANCH) and BASE_LABEL (what they were compared
# against), in the HOST repo: its local <base>, else its origin/<base> remote-tracking ref.
# Deliberately no fetch: the listing is informational, and the label names the host ref used —
# if that ref is behind GitHub the list can include commits already on the base.
# Returns 1 (with a WARNING) when neither ref exists or git fails — never a silent "no commits".
commits_since_base() {
    COMMITS=""; BASE_LABEL=""
    local ref
    if git -C "$REPO_ROOT" rev-parse -q --verify "refs/heads/${BASE_BRANCH}^{commit}" >/dev/null; then
        ref="refs/heads/${BASE_BRANCH}"; BASE_LABEL="${BASE_BRANCH}"
    elif git -C "$REPO_ROOT" rev-parse -q --verify "refs/remotes/origin/${BASE_BRANCH}^{commit}" >/dev/null; then
        ref="refs/remotes/origin/${BASE_BRANCH}"; BASE_LABEL="origin/${BASE_BRANCH} (host's remote-tracking ref, not fetched now)"
    else
        echo "WARNING: cannot list commits since ${BASE_BRANCH}: the host repo has neither refs/heads/${BASE_BRANCH} nor refs/remotes/origin/${BASE_BRANCH}." >&2
        return 1
    fi
    COMMITS="$(git -C "$REPO_ROOT" log --oneline "${ref}..${CLONE_SHA}")" || {
        echo "WARNING: git log ${ref}..${CLONE_SHA} failed in ${REPO_ROOT}." >&2
        return 1
    }
}

# Sets UNPUSHED: CLONE_SHA's commits not on origin's BRANCH_NAME, asked of origin itself
# (ls-remote) and computed in the host repo. Anything it cannot establish is "unknown" with a
# WARNING — never "nothing to push".
unpushed_commits() {
    UNPUSHED="(unknown — see WARNING above)"
    local out rc=0 remote_sha
    out="$(git -C "$REPO_ROOT" ls-remote "$GITHUB_REMOTE" "refs/heads/${BRANCH_NAME}")" || rc=$?
    remote_sha="${out%%[[:space:]]*}"
    if [ "$rc" -ne 0 ] || [[ ! "$remote_sha" =~ ^[0-9a-f]{40}$ ]]; then
        echo "WARNING: cannot tell what is unpushed: no ${BRANCH_NAME} found on ${GITHUB_REMOTE} (ls-remote rc=${rc})." >&2
        return 0
    fi
    if ! git -C "$REPO_ROOT" cat-file -e "${remote_sha}^{commit}" 2>/dev/null; then
        git -C "$REPO_ROOT" fetch -q --no-tags --no-write-fetch-head "$GITHUB_REMOTE" "refs/heads/${BRANCH_NAME}" || true
    fi
    if ! git -C "$REPO_ROOT" cat-file -e "${remote_sha}^{commit}" 2>/dev/null; then
        echo "WARNING: cannot tell what is unpushed: origin's ${BRANCH_NAME} (${remote_sha}) could not be fetched into the host repo." >&2
        return 0
    fi
    UNPUSHED="$(git -C "$REPO_ROOT" log --oneline "${remote_sha}..${CLONE_SHA}")"
}

# Push CLONE_SHA to origin's BRANCH_NAME from the host repo (host_push_cmd). Returns git's rc;
# a rejection (the branch moved on GitHub) prints the recovery. host_push create: the [c] arm,
# whose recovery must also create the PR.
host_push() {
    host_push_cmd "$CLONE_SHA" "$BRANCH_NAME"
    echo "Pushing ${CLONE_SHA:0:12} to ${GITHUB_REMOTE} refs/heads/${BRANCH_NAME} (from the host repo) ..."
    if "${HOST_PUSH[@]}"; then
        return 0
    fi
    echo "ERROR: push of ${BRANCH_NAME} was rejected (git's message is above)." >&2
    print_push_recovery "${1:-}" >&2
    return 1
}

# The push-rejected recovery: PRINTED with this run's values, never run. The branch moved on
# GitHub (e.g. Dependabot rebased it); the fix is the standard return path: /absorb-devloop into
# a fresh HOST worktree on that branch (never the operator's current branch), then a host push.
# Trust boundary: absorb's cherry-pick runs the repo's pre-commit hook (cargo fmt/clippy, so
# build.rs and proc macros) on container-authored commits — the crossing every /absorb-devloop
# already makes. The rule that holds is that the host never runs git with the clone as its repo
# or cwd (absorb reads it via fetch_from_clone). Do not reintroduce a host-made bundle or a
# container-writable mount to rebase inside the container instead.
# Only host-repo reads run here (rev-parse, worktree list); nothing is fetched, created or deleted.
# clone_rebase_base: the newest recorded commit the clone's branch (CLONE_SHA) descends from —
# candidates are devloop.<slug>.cloneBase (the commit the clone was created at) and
# devloop.<slug>.lastOnto (the upstream head the last printed recovery rebased onto). Not moved on
# a push: if a rewrite drops already-pushed commits, the recovery replays them too. Host-repo
# config and host objects only. Prints nothing when none qualifies (caller falls back).
clone_rebase_base() {
    local c best=""
    for c in "$(git -C "$REPO_ROOT" config --get "devloop.${TASK_SLUG}.cloneBase" || true)" \
             "$(git -C "$REPO_ROOT" config --get "devloop.${TASK_SLUG}.lastOnto" || true)"; do
        [[ "$c" =~ ^[0-9a-f]{40}$ ]] || continue
        git -C "$REPO_ROOT" merge-base --is-ancestor "$c" "$CLONE_SHA" 2>/dev/null || continue
        if [[ -z "$best" ]] || git -C "$REPO_ROOT" merge-base --is-ancestor "$best" "$c" 2>/dev/null; then best="$c"; fi
    done
    printf '%s' "$best"
}

# The push-rejected recovery. PR mode: ONE devloop.sh command that rebases the clone's branch
# INSIDE the container onto origin's moved branch — fetched anonymously over HTTPS from the PR's
# repo (a public repo needs no credentials, so the container gets none) — and ends in this menu
# again for [p]. The host still never runs git with the clone as its repo or cwd. If that fetch
# cannot work (private repo, no network) or outside PR mode, the host-side absorb path follows.
print_push_recovery() {
    local printed_header=""
    # Owner/repo/host limited to GitHub's characters: the url lands inside a printed `bash -c '...'`.
    if [[ -n "$PR_NUMBER" && "$PR_URL_RESOLVED" =~ ^(https://[A-Za-z0-9.-]+(:[0-9]+)?/[A-Za-z0-9._-]+/[A-Za-z0-9._-]+)/pull/[0-9]+$ ]]; then
        local repo_url="${BASH_REMATCH[1]}" vis inner
        printed_header=1
        # The container fetches anonymously, which works only for a public repo: ask GitHub (the
        # host's gh, already required in PR mode) and print the one recovery that applies.
        vis="$(gh repo view "$repo_url" --json visibility --jq .visibility 2>/dev/null)" || vis=""
        echo "  PR #${PR_NUMBER}'s branch ${BRANCH_NAME} probably moved on origin (e.g. Dependabot rebased it). Never force-push."
        if [[ "$vis" == PUBLIC || -z "$vis" ]]; then
            [[ -n "$vis" ]] || echo "  WARNING: could not read the repo's visibility (gh repo view ${repo_url} failed); printing both recoveries."
            # GIT_TERMINAL_PROMPT=0: a private repo fails loudly instead of prompting for a password.
            local base onto="FETCH_HEAD" remote_sha
            # --onto <clone base>: replay only the commits made in the clone, never the PR's own
            # earlier commits (a re-made bump with a different patch would conflict).
            base="$(clone_rebase_base)"
            if [[ -n "$base" ]]; then
                onto="--onto FETCH_HEAD ${base}"
            else
                echo "  WARNING: could not tell which commit the clone started from; the rebase below is a plain one and may"
                echo "  stop on the PR's earlier commit (e.g. the old bump): skip it with git rebase --skip."
            fi
            # Record the upstream head this recovery rebases onto (objects fetched into the HOST
            # repo), so a SECOND rewrite's recovery replays only the clone's own commits too.
            remote_sha="$(git -C "$REPO_ROOT" ls-remote "$GITHUB_REMOTE" "refs/heads/${BRANCH_NAME}" 2>/dev/null)" || remote_sha=""
            remote_sha="${remote_sha%%[[:space:]]*}"
            if [[ "$remote_sha" =~ ^[0-9a-f]{40}$ ]] \
                && git -C "$REPO_ROOT" fetch -q --no-tags --no-write-fetch-head "$GITHUB_REMOTE" "refs/heads/${BRANCH_NAME}" 2>/dev/null; then
                git -C "$REPO_ROOT" config "devloop.${TASK_SLUG}.lastOnto" "$remote_sha"
            fi
            inner="GIT_TERMINAL_PROMPT=0 git fetch -q ${repo_url}.git refs/heads/${BRANCH_NAME} && git -c advice.skippedCherryPicks=false rebase ${onto}"
            echo "  Rebase the clone onto it INSIDE the container (anonymous fetch from the public repo); the"
            echo "  command ends in this menu again — then pick [p]. Not run yet:"
            # Single-quoted: url (the regex above) and branch (safe_branch_name) cannot contain quotes.
            echo "    \$ $(printf '%q ' "$0" "${REENTER_ARGS[@]}")-- bash -c '${inner}'"
            echo "  On a rebase conflict it stops with the rebase in progress: re-enter with -- bash, resolve,"
            echo "  git rebase --continue (rebuild/test as needed), exit, then [p]."
            [[ -z "$vis" ]] || return 0
            echo "  If the fetch fails (private repo or no network), the host-side fallback:"
        else
            echo "  The repo is ${vis}, so the container cannot fetch it anonymously; recover on the host:"
        fi
    fi
    RECOVERY_FALLBACK="$printed_header" print_absorb_recovery "$@"
}

print_absorb_recovery() {
    local mode="${1:-}" b="$BRANCH_NAME" what="The branch"
    [[ -z "$PR_NUMBER" ]] || what="PR #${PR_NUMBER}'s branch"
    # A SIBLING of REPO_ROOT, so `/absorb-devloop <slug>` run there resolves its
    # <toplevel>/../worktrees/<slug> to this run's clone (the script line passes CLONE_DIR itself).
    local rebase="${REPO_ROOT%/*}/${TASK_SLUG}-rebase" clone="$CLONE_DIR"
    local r qb qrebase qclone
    r="$(printf '%q' "$REPO_ROOT")"; qb="$(printf '%q' "$b")"; qrebase="$(printf '%q' "$rebase")"
    qclone="$(printf '%q' "$clone")"

    # What would make the worktree add refuse — detected in the HOST repo (reads only), so the
    # remedies print FIRST, above the steps; none is run.
    local line cur="" wt="" rebase_registered=false have
    while IFS= read -r line; do
        case "$line" in
            "worktree "*) cur="${line#worktree }"; [[ "$cur" != "$rebase" ]] || rebase_registered=true ;;
            "branch refs/heads/${b}") wt="$cur" ;;
        esac
    done < <(git -C "$REPO_ROOT" worktree list --porcelain)
    have="$(git -C "$REPO_ROOT" rev-parse -q --verify "refs/heads/${b}^{commit}")" || have=""

    [[ -n "${RECOVERY_FALLBACK:-}" ]] || echo "  ${what} ${b} probably moved on origin (e.g. Dependabot rebased it). Never force-push."
    echo "  Recover on the HOST, in a new worktree on ${b} — never your current branch. Nothing below has been run."
    if [[ -n "$wt" || -e "$rebase" || -n "$have" ]]; then
        echo "  First, or the worktree add below refuses:"
    fi
    if [[ -n "$wt" && "$wt" == "$REPO_ROOT" ]]; then
        echo "    ${b} is checked out in ${REPO_ROOT} itself. Switch it: git -C ${r} switch <another-branch>"
    elif [[ -n "$wt" ]]; then
        echo "    ${b} is checked out in the worktree ${wt} (an earlier attempt?). Remove it (git refuses if it"
        echo "    has uncommitted work): git -C ${r} worktree remove $(printf '%q' "$wt")"
    fi
    if [[ -e "$rebase" && "$wt" != "$rebase" ]]; then
        if $rebase_registered; then
            echo "    ${rebase} is a leftover worktree (an earlier attempt?): git -C ${r} worktree remove ${qrebase}"
        else
            echo "    ${rebase} exists and is not a git worktree: inspect it, then remove it: rm -r ${qrebase}"
        fi
    fi
    local steps=""
    if [[ -n "$have" ]]; then
        echo "    The host repo has a local ${b}."
        # Safe to delete iff the clone's branch (CLONE_SHA, fetched into the host repo) contains it:
        # then the clone holds every commit on it. The usual case is the PR head the clone started
        # from — still that after a Dependabot rebase, when it is no longer the PR's head.
        if [[ -z "$wt" && -n "${CLONE_SHA:-}" ]] && git -C "$REPO_ROOT" merge-base --is-ancestor "$have" "$CLONE_SHA" 2>/dev/null; then
            echo "    It is at ${have:0:12}, which the clone's ${b} contains (the clone holds every commit on it),"
            echo "    so deleting it loses nothing while the clone exists (first step below) — do not --destroy before the push."
            steps="    \$ git -C ${r} branch -D ${qb}"$'\n'
        else
            local_branch_delete_hint "$b" "$have" | sed 's/^/    /'
        fi
    fi
    host_push_cmd "refs/heads/${b}" "$b"
    local push; push="$(printf '%q ' "${HOST_PUSH[@]}")"
    echo "  Then:"
    printf '%s' "$steps"
    echo "    \$ git -C ${r} fetch origin"
    echo "    \$ git -C ${r} worktree add -b ${qb} ${qrebase} origin/${qb}"
    echo "    \$ cd ${qrebase} && $(printf '%q' "${SCRIPT_DIR}/absorb-devloop.sh") ${qclone}"
    echo "    \$ ${push% }  # never force"
    if [[ "$mode" == create ]]; then
        echo "    \$ $(manual_pr_create_hint)"
    fi
    echo "  In Claude Code, /absorb-devloop ${TASK_SLUG} run from that worktree is the same absorb. Preview its picks first:"
    echo "    $(printf '%q' "${SCRIPT_DIR}/absorb-devloop.sh") --dry-run ${qclone}"
    if [[ -n "$PR_NUMBER" ]]; then
        echo "  If absorb stops on the PR's original commit (e.g. the bump, whose patch the upstream rebase changed):"
    else
        echo "  If absorb stops on a commit upstream already re-made with a different patch:"
    fi
    echo "    git cherry-pick --skip   — it goes on with your remaining commits; if it stops again on one of yours,"
    echo "    resolve, git add, then git cherry-pick --continue. Do NOT rerun absorb after a skip: it would queue"
    echo "    the skipped commit again."
    echo "  ONLY after the push succeeded (until then the clone holds the only copy of your commits):"
    echo "    cd ${r} && git -C ${r} worktree remove ${qrebase} && $(printf '%q' "${SCRIPT_DIR}/devloop.sh") --destroy $(printf '%q' "$TASK_SLUG")"
}

push_to_pr() {
    host_push && echo "Pushed. PR updated."
    echo ""
    menu_reenter_or_cleanup
}

is_container_running() {
    podman ps --format "{{.Names}}" 2>/dev/null | grep -q "^${1}$"
}

# Ensure the per-devloop git clone exists at $CLONE_DIR.
#
# Must run BEFORE launch_helper because the helper's --project-root points
# at $CLONE_DIR (so image builds and provision.sh/deploy.sh see the devloop's branch
# state, not the user's main checkout). Idempotent: no-ops if the clone
# already exists.
ensure_clone() {
    if [ -d "$CLONE_DIR" ]; then
        [[ -n "$PR_NUMBER" ]] || echo "Clone already exists: ${CLONE_DIR}"
        return 0
    fi

    # Ensure the branch exists locally before cloning.
    # If it only exists on the remote (e.g., PR iteration), fetch it first.
    # In PR mode this also pins it to the vetted PR head (see ensure_branch_ref).
    ensure_branch_ref

    echo "Creating local clone: ${CLONE_DIR}"
    if git show-ref --verify --quiet "refs/heads/${BRANCH_NAME}"; then
        # Branch exists — clone with it
        git clone -q --local --branch "$BRANCH_NAME" "$REPO_ROOT" "$CLONE_DIR"
    else
        # Fresh start — clone base branch, create the feature branch
        git clone -q --local --branch "$BASE_BRANCH" "$REPO_ROOT" "$CLONE_DIR"
        git -C "$CLONE_DIR" checkout -q -b "$BRANCH_NAME"
    fi
    # The only host git calls WITH THE CLONE AS ITS REPO (checkout -b above, set-url and the
    # rev-parse below): they run only on the clone `git clone` created moments ago in this call
    # (the early return covers an existing clone), before any container can have written into it.
    # Point origin at GitHub (not the local repo)
    git -C "$CLONE_DIR" remote set-url origin "$GITHUB_REMOTE"
    # The commit the clone starts from, in the HOST repo's config: the push-rejected recovery
    # rebases only the commits made on top of it (`rebase --onto`), so an upstream re-made bump
    # is never replayed. Removed by --destroy with the rest of devloop.<slug>.*.
    git -C "$REPO_ROOT" config "devloop.${TASK_SLUG}.cloneBase" "$(git -C "$CLONE_DIR" rev-parse HEAD)"
    if [[ -n "$PR_NUMBER" ]] && [[ "$(git -C "$CLONE_DIR" rev-parse HEAD)" != "$PR_HEAD_OID" ]]; then
        # This run created the clone moments ago (no container yet, nothing of the operator's
        # in it). Left behind, the next run would re-attach to the unvetted commit.
        rm -rf "$CLONE_DIR"
        pr_refuse head-moved "The new clone's HEAD is not PR #${PR_NUMBER}'s head ${PR_HEAD_OID}; the clone was removed." \
            "Re-run the same command."
    fi
}

# ─── Validate Prerequisites ─────────────────────────────────────

if ! command -v podman &>/dev/null; then
    echo "ERROR: podman is required. Install with: sudo apt install podman" >&2
    exit 1
fi

if ! $TOKEN_MODE && [ -z "${ANTHROPIC_API_KEY:-}" ] && [ ! -f "${HOME}/.claude/.credentials.json" ]; then
    echo "ERROR: No authentication found. Either run infra/devloop/claude-token.sh (token mode)," >&2
    echo "  set ANTHROPIC_API_KEY, or log in with 'claude' first." >&2
    exit 1
fi

# Check inotify limits for Kind multi-cluster support
if [ -f /proc/sys/fs/inotify/max_user_instances ]; then
    INOTIFY_INSTANCES=$(cat /proc/sys/fs/inotify/max_user_instances)
    if [ "$INOTIFY_INSTANCES" -lt 512 ]; then
        echo "WARNING: fs.inotify.max_user_instances=${INOTIFY_INSTANCES} (recommend 1024+)" >&2
        echo "  Kind clusters may fail with 'too many open files'." >&2
        echo "  Fix: sudo sysctl fs.inotify.max_user_instances=1024" >&2
        echo ""
    fi
fi

# ─── Build image if needed ───────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if $REBUILD_IMAGE || ! podman image exists "$IMAGE"; then
    build_devloop_image "$SCRIPT_DIR" "$IMAGE"
fi

# ─── Helper setup (ADR-0030/0031) ─────────────────────────────

# Ensure the devloop's git clone exists BEFORE launching the helper.
# The helper's --project-root points at CLONE_DIR, so the clone has to
# exist when the helper starts (see ADR-0030 §"Build-context trichotomy").
ensure_clone

# Build and launch the host-side cluster helper if kind is available.
# Without kind, devloop.sh works exactly as before (compile/test only, no cluster).
# The host-gateway IP (ADR-0030) is required for Kind NodePort binding —
# without it, cluster creation would produce invalid listenAddress values.
EXTRA_PODMAN_ARGS=()
HOST_GATEWAY_IP=""
# Whether a container created now gets the helper mount; recorded on it as a label, so a
# re-attach in the other mode is caught (mounts are fixed at `podman run`).
CLUSTER_MOUNTED=false

if $CLUSTER_ENABLED && command -v kind &>/dev/null; then
    # Detect host-gateway IP first — this gates all cluster features (ADR-0030).
    # An empty HOST_GATEWAY_IP would produce listenAddress: "" in the Kind config,
    # which could bind to 0.0.0.0 (explicitly prohibited by ADR-0030 Section 7).
    if HOST_GATEWAY_IP=$(detect_host_gateway_ip); then
        :   # detected; only a failure is worth printing
    else
        echo "ERROR: Host-gateway IP detection failed. Cluster features disabled." >&2
        echo "devloop will continue without Kind cluster support." >&2
        HOST_GATEWAY_IP=""
    fi

    if [ -n "$HOST_GATEWAY_IP" ]; then
        detect_orphan_clusters
        build_helper
        # Ensure runtime dir exists before launching (helper creates it too,
        # but we need it for the stderr log redirect)
        # umask, not `mkdir -p -m`: -m applies only to the deepest directory.
        (umask 077 && mkdir -p "$HELPER_RUNTIME_DIR")
        if launch_helper; then
            # Mount the helper runtime directory into the container. This is a read-write
            # mount because: (1) the unix socket requires rw for client connections, and
            # (2) files like kubeconfig/ports.json are created after container start (by
            # dev-cluster provision) so individual ro file mounts aren't possible (files must
            # exist at mount time). Accepted risk: container can modify host-generated
            # files, but blast radius is limited to the dev session's own state.
            EXTRA_PODMAN_ARGS+=(-v "$HELPER_RUNTIME_DIR:/tmp/devloop:Z")
            CLUSTER_MOUNTED=true
            EXTRA_PODMAN_ARGS+=(-e "KUBECONFIG=/tmp/devloop/kubeconfig")
        else
            echo "ERROR: Failed to start devloop helper. Cluster features disabled." >&2
            echo "devloop will continue without Kind cluster support." >&2
        fi
    fi
fi

# ─── Phase 1: Setup (idempotent) ────────────────────────────────

# --recreate: tear down containers but keep the clone
if $RECREATE; then
    harvest_before_removal "$DEV_CONTAINER" "$TASK_SLUG" "$REPO_ROOT" || exit 1
    echo "Recreating containers (clone preserved)..."
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true
    podman network rm "devloop-${TASK_SLUG}-net" 2>/dev/null || true
fi

if ! is_container_running "$DEV_CONTAINER"; then
    echo "=== Setting up containers for: ${TASK_SLUG} ==="

    # Clone is ensured earlier (before the helper launch) — see ensure_clone().

    # Clean up any stopped containers from a previous run
    harvest_before_removal "$DEV_CONTAINER" "$TASK_SLUG" "$REPO_ROOT" || exit 1
    podman rm -f "$DEV_CONTAINER" 2>/dev/null || true
    podman rm -f "$DB_CONTAINER" 2>/dev/null || true

    # Create named network for container DNS (ADR-0030).
    # Named networks allow DB access via container name instead of localhost,
    # and enable host.containers.internal routing to host-gateway-bound ports.
    NETWORK_NAME="devloop-${TASK_SLUG}-net"
    podman network create "$NETWORK_NAME" 2>/dev/null || true

    # Start postgres container
    echo "Starting PostgreSQL..."
    podman run -d --name "$DB_CONTAINER" \
        --network "$NETWORK_NAME" \
        -e POSTGRES_PASSWORD=postgres \
        -e POSTGRES_DB=dark_tower_test \
        docker.io/library/postgres:16-bookworm >/dev/null   # stdout is the container ID

    # Capture the DB container's IP on this network — used below as a static
    # /etc/hosts binding in the dev container. Bypasses aardvark-dns for the
    # most-frequent cross-container lookup: sqlx-tokio resolves DNS on every
    # connect (no caching), so under Gate-2 parallel `#[sqlx::test]` load the
    # lookups saturate aardvark-dns and produce intermittent EAI_AGAIN failures
    # (surfaced 2026-06-20 during task #55 Gate 2). The dev container's own
    # name is in /etc/hosts via Podman default, but sidecar hostnames are not.
    DB_IP=$(podman inspect "$DB_CONTAINER" \
        --format "{{(index .NetworkSettings.Networks \"$NETWORK_NAME\").IPAddress}}")
    if [ -z "$DB_IP" ]; then
        echo "ERROR: Could not determine IP for $DB_CONTAINER on $NETWORK_NAME" >&2
        exit 1
    fi

    # Mount user-level Claude config files if they exist (read-only, to fixed paths
    # that entrypoint.sh will copy to the correct $HOME inside the container)
    if [ -f "${HOME}/.claude/settings.json" ]; then
        EXTRA_PODMAN_ARGS+=(-v "${HOME}/.claude/settings.json:/tmp/claude-user-settings.json:ro")
    fi
    if [ -f "${HOME}/.claude.json" ]; then
        EXTRA_PODMAN_ARGS+=(-v "${HOME}/.claude.json:/tmp/claude-user-config.json:ro")
    fi
    if $TOKEN_MODE; then
        # By NAME, not NAME=value: podman copies the value from this process's
        # environment (exported by load_oauth_token), so the token never appears in
        # argv — i.e. never in `ps` output or shell history on the host.
        EXTRA_PODMAN_ARGS+=(-e CLAUDE_CODE_OAUTH_TOKEN)
    elif [ -f "${HOME}/.claude/.credentials.json" ]; then
        EXTRA_PODMAN_ARGS+=(-v "${HOME}/.claude/.credentials.json:/tmp/claude-credentials.json:ro")
    fi
    if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
        # By name, same reason as the token above.
        EXTRA_PODMAN_ARGS+=(-e ANTHROPIC_API_KEY)
    fi

    # Story-runner ledger mount (ADR-0037 D5) — UNCONDITIONAL (not gated on kind,
    # unlike the helper mount): the ledger must persist even on the no-cluster path.
    # Created 0700 BEFORE `podman run` so the runtime never auto-creates it
    # root-owned under --userns=keep-id (matches HELPER_RUNTIME_DIR at :515). With
    # `:z` the SELinux relabel is SHARED — required because this flat dir is mounted
    # into potentially-concurrent per-slug containers; `:Z` (private, as the helper
    # and /work mounts correctly use) would lock the second container out. 0700 is
    # therefore the load-bearing isolation control here, not defence-in-depth — do
    # NOT relax it to 0755. The .ledger-mount marker is run-story's positive control
    # that the bind actually happened (a writable dir that ISN'T this mount would
    # otherwise pass silently and lose the ledger on destroy — LEDGER-NOT-PERSISTENT).
    # umask, not `mkdir -p -m`: -m applies only to the deepest directory.
    (umask 077 && mkdir -p "$STORY_RUNS_HOST")
    : > "$STORY_RUNS_HOST/.ledger-mount"
    EXTRA_PODMAN_ARGS+=(-v "${STORY_RUNS_HOST}:${CONTAINER_LEDGER_BASE}:z")
    EXTRA_PODMAN_ARGS+=(-e "DEVLOOP_STORY_RUN_BASE=${CONTAINER_LEDGER_BASE}")

    # Start dev container on the named network (ADR-0030).
    # Uses container DNS to reach postgres via container name.
    # host.containers.internal routes to host-gateway-bound Kind NodePorts.
    # --add-host pins the DB sidecar to its IP in /etc/hosts (files-first per
    # nsswitch.conf), bypassing aardvark-dns for sqlx-tokio's per-connect
    # lookups under Gate-2 parallel-test load — see DB_IP capture above.
    echo "Starting dev container..."
    podman run -d --name "$DEV_CONTAINER" \
        --label "devloop.cluster=$($CLUSTER_MOUNTED && echo on || echo off)" \
        --init \
        --pids-limit 8192 \
        --memory "$DEVLOOP_CONTAINER_MEMORY" \
        --memory-swap "$DEVLOOP_CONTAINER_MEMORY" \
        --userns=keep-id \
        --network "$NETWORK_NAME" \
        --add-host "${DB_CONTAINER}:${DB_IP}" \
        -v "$(realpath "$CLONE_DIR"):/work:Z" \
        -v cargo-registry:/tmp/cargo-home/registry \
        -v cargo-git:/tmp/cargo-home/git \
        -v pnpm-store:/tmp/pnpm-store \
        -e CARGO_HOME=/tmp/cargo-home \
        -e npm_config_store_dir=/tmp/pnpm-store \
        ${EXTRA_PODMAN_ARGS[@]+"${EXTRA_PODMAN_ARGS[@]}"} \
        -e DATABASE_URL="postgresql://postgres:postgres@${DB_CONTAINER}:5432/dark_tower_test" \
        -e AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=" \
        -e GIT_AUTHOR_NAME="${GIT_AUTHOR_NAME}" \
        -e GIT_AUTHOR_EMAIL="${GIT_AUTHOR_EMAIL}" \
        -e GIT_COMMITTER_NAME="${GIT_AUTHOR_NAME}" \
        -e GIT_COMMITTER_EMAIL="${GIT_AUTHOR_EMAIL}" \
        -e RUST_BACKTRACE=1 \
        -e CARGO_TERM_COLOR=always \
        "$IMAGE" >/dev/null   # stdout is the container ID

    # Wait for entrypoint to complete setup
    echo "Waiting for container initialization..."
    sleep 5

    seed_container_memory "$DEV_CONTAINER" "$REPO_ROOT" || exit 1

    echo "=== Containers ready ==="
    CONTAINER_STATE="created"
else
    CONTAINER_STATE="running"
    # Mounts are fixed at `podman run`; a re-attach in the other cluster mode can't change them.
    CONTAINER_CLUSTER="$(podman inspect --format '{{index .Config.Labels "devloop.cluster"}}' "$DEV_CONTAINER" 2>/dev/null || true)"
    if [[ "$CONTAINER_CLUSTER" == off ]] && $CLUSTER_MOUNTED; then
        echo "WARNING: ${DEV_CONTAINER} was created without the cluster mount (--no-cluster, or no Kind then)." >&2
        echo "  The helper/cluster below run, but the container cannot reach them (Layer 7 will fail inside)." >&2
        echo "  Recreate it with the mount (clone kept): $0 --recreate $(printf '%q ' "${REENTER_ARGS[@]}")" >&2
    elif [[ "$CONTAINER_CLUSTER" == on ]] && ! $CLUSTER_ENABLED; then
        echo "WARNING: ${DEV_CONTAINER} was created with a cluster; its helper and Kind cluster from earlier" >&2
        echo "  launches are still running, unchecked, under --no-cluster. Stop them with: $0 --destroy ${TASK_SLUG}" >&2
    fi
fi

# ─── Infrastructure health check (ADR-0030 Step 6) ──────────────

# Verify helper and cluster health on every entry (including re-attach).
# This ensures the infrastructure is ready for env-tests in the devloop skill.
if $CLUSTER_ENABLED && [ -n "$HOST_GATEWAY_IP" ] && command -v kind &>/dev/null; then
    INFRA_STATUS="ok"

    # 1. Check if helper PID is alive — restart if dead
    if [ -f "$HELPER_RUNTIME_DIR/helper.pid" ]; then
        HELPER_PID=$(cat "$HELPER_RUNTIME_DIR/helper.pid")
        if ! is_helper_process_alive "$HELPER_PID"; then
            echo "Helper process dead (PID $HELPER_PID), restarting..."
            build_helper
            if launch_helper; then
                INFRA_STATUS="helper=restarted"
            else
                echo "WARNING: Failed to restart helper. Cluster features may not work." >&2
                INFRA_STATUS="helper=failed"
            fi
        fi
    fi

    # 2. Check cluster readiness and trigger eager setup if needed
    # CLUSTER_PREFIX is the SAME const the launch-time slug-length cap subtracts
    # (Finding 4) — the prefix that builds the name and the prefix the bound assumes
    # cannot drift apart.
    CLUSTER_NAME="${CLUSTER_PREFIX}${TASK_SLUG}"
    PORTS_FILE_PATH="$HELPER_RUNTIME_DIR/ports.json"
    NEEDS_SETUP=false
    CLUSTER_EXISTS_RC=0
    cluster_exists || CLUSTER_EXISTS_RC=$?
    if [ "$CLUSTER_EXISTS_RC" -eq 2 ]; then
        # Never read as "absent": provision re-lists and REFUSES on an unreadable
        # listing (BLUEPRINT REASON=unreadable), so the failure surfaces there, loudly.
        echo "WARNING: 'kind get clusters' failed; cannot tell whether ${CLUSTER_NAME} exists. Running provision + deploy in background, which refuses on an unreadable listing (see eager-setup.log)..." >&2
        NEEDS_SETUP=true
    elif [ "$CLUSTER_EXISTS_RC" -eq 1 ]; then
        echo "No Kind cluster found for ${TASK_SLUG}, starting provision + deploy in background..."
        NEEDS_SETUP=true
    elif [ ! -f "$PORTS_FILE_PATH" ]; then
        echo "Ports file missing, running provision + deploy in background (idempotent)..."
        NEEDS_SETUP=true
    fi

    if $NEEDS_SETUP; then
        # Eager bring-up = the two ADR-0038 verbs, in order: provision the
        # platform, then deploy the application (only if provision succeeded).
        # Subshell ensures setup.pid is cleaned up when both finish,
        # preventing stale PID issues on re-attach (PID recycling).
        # Use full path to dev-cluster: `podman exec` does NOT run the
        # container's entrypoint (which prepends /work/infra/devloop to PATH),
        # so a bare `dev-cluster` invocation fails to resolve.
        (podman exec "$DEV_CONTAINER" /work/infra/devloop/dev-cluster provision \
            >> "$HELPER_RUNTIME_DIR/eager-setup.log" 2>&1 \
            && podman exec "$DEV_CONTAINER" /work/infra/devloop/dev-cluster deploy \
            >> "$HELPER_RUNTIME_DIR/eager-setup.log" 2>&1; \
            rm -f "$HELPER_RUNTIME_DIR/setup.pid") &
        EAGER_SETUP_PID=$!
        echo "$EAGER_SETUP_PID" > "$HELPER_RUNTIME_DIR/setup.pid"
        INFRA_STATUS="cluster=setup-pending"
    elif [ "$INFRA_STATUS" = "ok" ]; then
        INFRA_STATUS="helper=alive cluster=ready"
    fi

    echo "Infrastructure: containers=${CONTAINER_STATE} ${INFRA_STATUS}"
else
    if $CLUSTER_ENABLED; then
        echo "Infrastructure: containers=${CONTAINER_STATE} (no Kind cluster)"
    else
        echo "Infrastructure: containers=${CONTAINER_STATE} cluster=skipped (--no-cluster; Layer 7 needs it: run scripts/layer-all.sh --max-layer 6)"
    fi
fi

# ─── Phase 2: Attach ────────────────────────────────────────────

echo ""
# Set terminal title to devloop slug
printf '\033]0;devloop: %s\007' "$TASK_SLUG"

if [ ${#EXEC_CMD[@]} -eq 0 ]; then
    echo "Dropping into Claude Code..."
    echo "(Ctrl-D or /exit to detach — containers stay running)"
    echo ""
fi

# Refresh OAuth credentials before each attach (host sessions may have rotated
# the refresh token since the container started, invalidating the container's copy).
# Token mode: nothing to copy — but a container REUSED here keeps the environment it
# was created with, so check it actually carries the current token. Compared with
# grep -q so the value is never printed.
if $TOKEN_MODE; then
    if ! podman inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$DEV_CONTAINER" \
        | grep -qxF "CLAUDE_CODE_OAUTH_TOKEN=${CLAUDE_CODE_OAUTH_TOKEN}"; then
        echo "WARNING: ${DEV_CONTAINER} was not created with the current token (it predates token" >&2
        echo "  mode, or the token was rotated since). It is still using whatever credential it" >&2
        echo "  started with. Recreate it (clone, ledgers and cluster preserved):" >&2
        echo "    devloop.sh --recreate ${REENTER_ARGS[*]}" >&2
    fi
elif [ -f "${HOME}/.claude/.credentials.json" ]; then
    podman exec "$DEV_CONTAINER" mkdir -p /home/dev/.claude 2>/dev/null || true
    podman cp "${HOME}/.claude/.credentials.json" "${DEV_CONTAINER}:/home/dev/.claude/.credentials.json"
fi

# Update Claude Code to latest before each attach — UNLESS a story run owns this
# container. run-story's preflight probes the experimental teams-in-print-mode
# substrate ONCE per invocation and binds to that CLI version; updating underneath
# a live run puts every later task on an unprobed version (run-story asserts the
# version per task and will halt the story if this happens).
#
# The update is skipped, NOT the attach: the credential refresh above is the
# documented AUTH-EXPIRED recovery (run-story.sh prints `devloop.sh --refresh-creds`),
# and blocking the attach would break that recovery in exactly the situation it is
# needed — while a story is in flight (security, 2026-08-10). Announced, never silent.
INFLIGHT_MARKER="${HELPER_RUNTIME_DIR}/story-runner/.run-in-flight"
if [ -f "$INFLIGHT_MARKER" ] && [ "${FORCE_CLI_UPDATE:-}" != "1" ]; then
    echo "Story run in flight ($(cat "$INFLIGHT_MARKER" 2>/dev/null || echo 'unknown')) —"
    echo "  SKIPPING the Claude Code update so the run's substrate stays as probed."
    echo "  Update after the run, or re-attach with FORCE_CLI_UPDATE=1 to override."
else
    echo "Updating Claude Code..."
    quiet_unless_fails podman exec --user=0 "$DEV_CONTAINER" npm install -g @anthropic-ai/claude-code --loglevel=warn \
        || { echo "ERROR: Claude Code update failed (npm output above)." >&2; exit 1; }
fi

if [ ${#EXEC_CMD[@]} -gt 0 ]; then
    # DEVLOOP_SLUG is exported because a process inside the container cannot see
    # the container's own name, and at least one — run-story's AUTH-EXPIRED lane —
    # has to tell the operator to run `devloop.sh --refresh-creds <slug>` on the
    # host. A recovery instruction is only actionable with the real value in it.
    echo "Running in ${DEV_CONTAINER}: $(printf '%q ' "${EXEC_CMD[@]}")"
    # `|| EXEC_RC=$?`, not a bare command + `$?`: under set -e a failing command would exit
    # right here, skipping the harvest, the report and (PR mode) the exit menu.
    EXEC_RC=0
    podman exec -it -e "DEVLOOP_SLUG=${TASK_SLUG}" -w /work "$DEV_CONTAINER" "${EXEC_CMD[@]}" || EXEC_RC=$?
    harvest_container_memory "$DEV_CONTAINER" "$TASK_SLUG" "$REPO_ROOT" || true
fi
if [ ${#EXEC_CMD[@]} -gt 0 ] && [[ -n "$PR_NUMBER" ]]; then
    # PR mode is interactive by nature (nothing unattended uses --pr; run-story launches plain
    # `devloop.sh <slug> -- ...`), so after the command it ends with the normal exit menu: [p]
    # pushes to the PR, and a rejected push prints the recovery. The command's exit code is
    # reported, not propagated.
    echo ""
    echo "=== exited rc=${EXEC_RC} ==="
elif [ ${#EXEC_CMD[@]} -gt 0 ]; then
    # Report and exit HERE rather than falling through to Phase 3, which ends in
    # `read -p "Choice: "` — that would block a finished unattended run on a
    # keypress nobody is present to give, holding the terminal and the container
    # indefinitely. This form exists to be left alone, so it reports and leaves.
    #
    # Exiting with the command's own code also preserves a distinction Phase 3
    # would erase. For run-story that is: 0 all tasks complete, 1 implementer-class
    # stop, 2 operator-class. A story that ended in escalation must not exit like
    # one that finished.
    echo ""
    echo "=== exited rc=${EXEC_RC} ==="
    if fetch_clone_branch && commits_since_base; then
        if [ -n "$COMMITS" ]; then
            echo "Commits on ${BRANCH_NAME} since ${BASE_LABEL}:"
            echo "$COMMITS"
        else
            echo "No new commits on ${BRANCH_NAME} since ${BASE_LABEL}."
        fi
    fi
    # Left running deliberately: teardown deletes the clone (and with it any
    # unpushed commits) and the run evidence under the helper runtime dir, and for
    # run-story specifically, rerunning is the documented recovery for several
    # lanes. Named explicitly because more is left up than "containers" suggests —
    # the Kind cluster behind the helper is the expensive part, and under-reporting
    # it is how it ends up running overnight after a run that finished.
    echo ""
    if $CLUSTER_ENABLED; then
        echo "Still running: ${DEV_CONTAINER}, ${DB_CONTAINER}, and the dev-cluster helper (Kind)."
    else
        echo "Still running: ${DEV_CONTAINER}, ${DB_CONTAINER} (cluster skipped: --no-cluster)."
    fi
    echo "  Re-enter:  $0 ${REENTER_ARGS[*]}"
    echo "  Tear down: $0 --destroy ${TASK_SLUG}   (deletes the clone — push first)"
    exit "$EXEC_RC"
fi

if [ ${#EXEC_CMD[@]} -eq 0 ]; then
    podman exec -it "$DEV_CONTAINER" claude --dangerously-skip-permissions "${LEAD_MODEL_ARGS[@]}" --remote-control "$TASK_SLUG" || true
    # Non-destructive copy: a failure prints ERROR and the next removal refuses.
    harvest_container_memory "$DEV_CONTAINER" "$TASK_SLUG" "$REPO_ROOT" || true
fi

# ─── Phase 3: Post-session ──────────────────────────────────────

echo ""

# Everything below works on the HOST repo: the clone's branch is fetched out once
# (fetch_clone_branch) and no git/gh runs with the clone as its repo or cwd.
if ! fetch_clone_branch; then
    echo ""
    menu_reenter_or_cleanup
    exit
fi

COMMITS_KNOWN=true
commits_since_base || COMMITS_KNOWN=false

# Check if a PR already exists for this branch — pinned to origin with -R, never resolved from
# the clone's config. gh's failure is reported, not read as "no PR". `--head` matches by branch
# NAME only (a fork PR can share it), so PR mode takes the row whose number is the PR it was
# started on; otherwise the first row, as before.
PR_URL=""
PR_QUERY_RC=0
PR_ROWS=$(gh pr list -R "$GITHUB_REMOTE" --head "$BRANCH_NAME" --state open --json number,url \
    --jq '.[] | "\(.number)\t\(.url)"') || PR_QUERY_RC=$?
if [ "$PR_QUERY_RC" -ne 0 ]; then
    echo "WARNING: could not query open PRs for ${BRANCH_NAME} (gh rc=${PR_QUERY_RC}; gh's message, if any, is above)." >&2
else
    PR_URL=$(awk -F'\t' -v n="$PR_NUMBER" '(n == "" && NR == 1) || (n != "" && $1 == n) { print $2; exit }' <<< "$PR_ROWS")
fi

if [ -n "$PR_URL" ]; then
    echo "=== PR exists: ${PR_URL} ==="
    echo ""

    unpushed_commits
    if [ -n "$UNPUSHED" ]; then
        echo "Unpushed commits:"
        echo "$UNPUSHED"
        echo ""
        dependabot_caveat
        echo "  [p] Push to update PR"
        echo "  [r] Re-enter container"
        echo "  [d] Destroy containers and clone (un-pushed changes will be lost)"
        echo "  [q] Quit (containers stay running)"
        read -p "Choice: " -n 1 -r
        echo
        case $REPLY in
            p|P) push_to_pr ;;
            r|R) exec "$0" "${RERUN_ARGS[@]}" ;;
            d|D) cleanup ;;
            *) echo "Containers still running. Re-enter with: $0 ${REENTER_ARGS[*]}" ;;
        esac
    else
        menu_reenter_or_cleanup
    fi

elif [[ -n "$PR_NUMBER" ]]; then
    # PR mode never offers to create a branch or PR.
    if [ "$PR_QUERY_RC" -eq 0 ]; then
        echo "WARNING: PR #${PR_NUMBER} is no longer open (no open PR #${PR_NUMBER} for head ${BRANCH_NAME})." >&2
    fi
    echo "Not offering a push. To push anyway (updates the branch only, not a closed PR):"
    echo "  $(manual_push_hint "$BRANCH_NAME")"
    echo ""
    menu_reenter_or_cleanup

elif [ -n "$COMMITS" ] || ! $COMMITS_KNOWN; then
    if $COMMITS_KNOWN; then
        echo "=== Commits on ${BRANCH_NAME} since ${BASE_LABEL} (no open PR) ==="
        echo "$COMMITS"
    else
        echo "=== ${BRANCH_NAME} (no open PR; commits since ${BASE_BRANCH} unknown — see WARNING above) ==="
    fi
    echo ""
    echo "  [c] Push and create PR into ${BASE_BRANCH}"
    echo "  [p] Push branch to origin (no PR)"
    echo "  [r] Re-enter container"
    echo "  [d] Destroy containers and clone (un-pushed changes will be lost)"
    echo "  [q] Quit (containers stay running)"
    read -p "Choice: " -n 1 -r
    echo

    case $REPLY in
        c|C)
            # Stateless PR creation from commit history (per eee1739: no .devloop-pr.json
            # state file). Title/body are built here from the HOST repo's view of the pushed
            # commits, as `--fill` would (one commit: its subject/body; several: the branch
            # name + the subjects) — `--fill` itself reads git in the cwd. --base must already
            # exist on origin; gh fails loudly here if it does not, and the branch is still pushed.
            if host_push create; then
                PR_TITLE="$BRANCH_NAME"; PR_BODY=""
                if $COMMITS_KNOWN && [ "$(wc -l <<< "$COMMITS")" -eq 1 ] && [ -n "$COMMITS" ]; then
                    PR_TITLE="$(git -C "$REPO_ROOT" log -1 --format=%s "$CLONE_SHA")"
                    PR_BODY="$(git -C "$REPO_ROOT" log -1 --format=%b "$CLONE_SHA")"
                elif $COMMITS_KNOWN; then
                    PR_BODY="$(sed 's/^[0-9a-f]* /- /' <<< "$COMMITS")"
                fi
                gh pr create -R "$GITHUB_REMOTE" --base "$BASE_BRANCH" --head "$BRANCH_NAME" \
                    --title "$PR_TITLE" --body "$PR_BODY" \
                    || echo "PR creation failed (is ${BASE_BRANCH} on origin?). Branch is pushed — create the PR manually: $(manual_pr_create_hint)"
            fi
            echo ""
            menu_reenter_or_cleanup
            ;;
        p|P)
            if host_push; then
                echo "Pushed ${BRANCH_NAME}. Open a PR when ready: $(manual_pr_create_hint)"
            fi
            echo ""
            menu_reenter_or_cleanup
            ;;
        r|R) exec "$0" "${RERUN_ARGS[@]}" ;;
        d|D) cleanup ;;
        *) echo "Containers still running. Re-enter with: $0 ${REENTER_ARGS[*]}" ;;
    esac

else
    echo "No new commits on ${BRANCH_NAME} since ${BASE_LABEL}."
    echo ""
    menu_reenter_or_cleanup
fi
exit
}
