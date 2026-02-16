# ADR-0025: Containerized Dev-Loop Execution

**Status**: Accepted

**Date**: 2026-02-12

**Deciders**: Nathan Buckles, Claude (orchestrator)

---

## Context

The Agent Teams devloop (ADR-0024) runs Claude Code with autonomous specialist teammates. For maximum autonomy, Claude Code supports `--dangerously-skip-permissions` which disables all permission prompts — but this is only safe in isolated environments where damage cannot escape.

Running on the host (even inside WSL2) exposes:
- SSH keys and git credentials
- Cloud credentials (`~/.aws/`, etc.)
- Windows filesystem via `/mnt/c/`
- All other projects and personal files
- WSL2 interop (can execute Windows binaries)

Claude Code's `/sandbox` feature (bubblewrap on Linux) provides defense-in-depth but is not a hard security boundary — it allows read-only access to the entire filesystem by default, is vulnerable to domain fronting, and has documented escape paths.

We need a development environment where Claude can operate with full autonomy (`--dangerously-skip-permissions`) while limiting blast radius to the current task's clone.

### Requirements

1. Claude must be able to run the full validation pipeline (compile, format, guards, tests, clippy, audit, semantic analysis)
2. No GitHub credentials or SSH keys exposed to the container
3. Parallel devloops must be possible (multiple tasks simultaneously)
4. Accidental session exit (Ctrl-D) must not destroy state
5. Build cache should persist across sessions to avoid rebuilding ~400 crates each time
6. Git commits inside the container must have proper user attribution
7. PR creation should leverage Claude's context (task description, reviewer verdicts)

## Decision

### 1. Containerized Execution via Podman

Each devloop runs two **podman containers** sharing a network namespace:
- A **dev container** with the full Rust toolchain, Claude Code CLI, and development tools
- A **PostgreSQL 16 container** for integration tests (accessible at `localhost:5432` via `--network container:`)

The dev container's entrypoint is `sleep infinity` (keeps the container alive). Users interact via `podman exec -it <container> claude --dangerously-skip-permissions`.

### 2. Git Clone Isolation

Each task gets its own `git clone --local` (hardlinked objects, self-contained `.git`), bind-mounted into the container as `/work`. This provides:
- Filesystem isolation (Claude can only see the clone)
- Build artifact persistence (`target/` lives on the bind mount)
- Clean separation between parallel tasks
- No cross-directory `.git` references (unlike worktrees, which reference the main repo's `.git` directory and break inside containers)

### 3. Credential Separation

| Credential | Location | Exposed to Claude? |
|---|---|---|
| OAuth credentials | Container (read-only mount) | Yes (required for Claude to function) |
| `ANTHROPIC_API_KEY` | Container env var (if set) | Yes (optional, OAuth preferred) |
| GitHub PAT / `gh` auth | Host only | No |
| SSH keys | Host only | No |
| Git identity | Container env vars (`GIT_AUTHOR_*`, `GIT_COMMITTER_*`) | Yes (name/email, not secrets) |
| `AC_MASTER_KEY` | Container env var | Yes (test fixture, not production) |

OAuth credentials (`~/.claude/.credentials.json`) are mounted read-only. The risk is limited to API charge abuse; Claude cannot access the user's repositories, infrastructure, or personal data.

### 4. PR Metadata File

Since GitHub credentials are not available inside the container, Claude writes a `.devloop-pr.json` file to the clone root during devloop completion (Step 9). The host-side wrapper script reads this file to push and create the PR using the host's credentials.

```json
{
  "title": "Extract generic health checker (TD-13)",
  "body": "## Summary\n..."
}
```

This preserves Claude's context (task description, reviewer verdicts, files changed) in the PR description without exposing GitHub credentials.

### 5. Shared Cargo Registry, Isolated Target

- `cargo-registry` and `cargo-git` are named podman volumes mounted at `$CARGO_HOME` (`/tmp/cargo-home`), shared across all devloops (safe — cargo uses file locking)
- `target/` lives inside the clone bind mount (isolated per task, persists between sessions)

### 6. Persistent Containers with Attach/Detach

The containers run in the background. Users attach via `podman exec` and detach freely. This means:
- Ctrl-D exits the Claude session, not the container
- PostgreSQL keeps its state (migrations, test data) between sessions
- Build artifacts in `target/` persist
- Re-entry is instant (no boot time)

Only Claude's conversation context is lost on session exit. Code, database, and build state all survive.

### 7. Wrapper Script Lifecycle

A `devloop.sh` script manages the full lifecycle:

```
./devloop.sh <task-slug> [base-branch]
```

1. Creates git clone (idempotent)
2. Starts containers in background (idempotent)
3. Runs migrations, copies Claude settings
4. Drops into container via `podman exec`
5. On exit, checks for commits and PR metadata
6. Offers: push + create PR / edit PR description / re-enter / cleanup

### 8. Container Image

Pre-built image (`darktower-dev:latest`) containing:
- Rust stable + nightly toolchains
- rustfmt, clippy, llvm-tools-preview components
- cargo-llvm-cov, sqlx-cli, cargo-audit
- System: build-essential, pkg-config, protobuf-compiler, git, jq, bc
- Node.js 22 + Claude Code CLI (`@anthropic-ai/claude-code`)

Rebuilt weekly or when toolchain changes. ~3-4 GB.

## Consequences

**Positive**:
- Full `--dangerously-skip-permissions` autonomy with container-level blast radius
- No GitHub/SSH credentials exposed to Claude
- Parallel devloops via separate clones and container pairs
- Session resilience (Ctrl-D doesn't destroy state)
- PR descriptions written by Claude with full task context
- Aligns with existing podman requirement (test database)

**Negative**:
- ~3-4 GB container image to maintain
- First build in a new container is slower (cargo registry cache helps after first run)
- Claude conversation context lost on session exit (code/DB state preserved)
- OAuth credentials are still exposed inside the container (read-only)
- Additional tooling to learn (podman containers, wrapper script)

**Neutral**:
- `--userns=keep-id` maps container UID to host UID (proper file ownership)
- Git attribution via environment variables instead of `~/.gitconfig`
- `.devloop-pr.json` must be in `.gitignore`

## Implementation Status

| Section | Component | Status | Notes |
|---------|-----------|--------|-------|
| 1 | Dockerfile | ✅ Done | `infra/devloop/Dockerfile` |
| 2 | Entrypoint script | ✅ Done | `infra/devloop/entrypoint.sh` |
| 3 | Wrapper script | ✅ Done | `infra/devloop/devloop.sh` |
| 4 | SKILL.md Step 9 update | ✅ Done | `.devloop-pr.json` output |
| 5 | .gitignore update | ✅ Done | Add `.devloop-pr.json` |
| 6 | Documentation updates | ✅ Done | AI_DEVELOPMENT.md, CLAUDE.md, DEVELOPMENT_WORKFLOW.md |

## Alternatives Considered

- **Claude Code `/sandbox` only**: Defense-in-depth but not a hard boundary. Read-only access to entire filesystem, domain fronting risk, documented escape paths. Insufficient for `--dangerously-skip-permissions`.
- **Docker instead of Podman**: Requires daemon, rootful by default. Podman is rootless, daemonless, and already a project requirement.
- **Devcontainer spec**: More complex, IDE-coupled. The wrapper script is simpler and works from any terminal.
- **GitHub credentials in container with repo-scoped PAT**: Simpler PR workflow, but any credential in a `--dangerously-skip-permissions` container should be considered compromised. Host-side PR creation is safer.
- **Ephemeral containers (--rm)**: Simpler but Ctrl-D destroys everything. Persistent containers with attach/detach are more resilient.
- **Podman pods**: Originally attempted, but `--pod` is incompatible with `--userns=keep-id`. Replaced with `--network container:` to share the network namespace while preserving UID mapping.

## References

- ADR-0024: Agent Teams Development Workflow
- ADR-0013: Local Development Environment
- Claude Code documentation: Sandbox security model
- Podman rootless documentation: User namespace mapping
