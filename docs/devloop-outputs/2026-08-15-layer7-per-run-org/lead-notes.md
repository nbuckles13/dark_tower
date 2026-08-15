# Lead notes (working file — folded into main.md at Step 9)

## Out-of-scope findings raised during planning — to record in `docs/TODO.md` at Step 9

### 1. The schema-evolution GSA entry matches no path in this repo

**Raised by**: @paired-database, during Gate-1 pre-work.
**Verified by Lead** at commit `9fa3bc6`:

- ADR-0024 §6.4 and its mirrors enumerate `db/migrations/**` as the schema-evolution
  Guarded Shared Area — `docs/decisions/adr-0024-agent-teams-workflow.md:404`,
  `.claude/skills/devloop/SKILL.md:134`,
  `.claude/skills/devloop/review-protocol.md:30`,
  `scripts/guards/simple/cross-boundary-ownership.yaml:44`.
- `db/migrations/` **does not exist**. This repo's migrations live at `migrations/`.

So the schema-evolution GSA glob matches nothing, and migrations are effectively
unguarded by the GSA list. Not a finding against task 3 (no migration in this diff), but
it silently weakens the strictest rule in the ownership regime.

Task-sized rather than a trivial in-tree edit: the path is mirrored in five locations
(the four above plus the CANON array in `scripts/guards/simple/validate-gsa-sync.sh`),
the mirrors are guard-enforced to stay in sync, and ADR-0024 governs how the enumerated
list may change. It also wants the owning specialists (database + security + operations)
rather than a headless drive-by.

### 2. `checks.md` has no error-swallow check, but `layer7.sh` addresses `@semantic-guard` for one

**Raised by**: @semantic-guard, during Gate-1 pre-work.

`scripts/guards/semantic/checks.md` lists exactly five checks (Credential Leak, Client
Credential Lifetime, Actor Blocking, Error Context Preservation, Metrics Path
Completeness). None covers "a blanket error-swallow that converts a hard failure into a
soft one", and Error Context Preservation is Rust-only, keyed on `.map_err`. Yet
`scripts/layer7.sh:464` carries a comment addressed to `@semantic-guard` explaining that
a per-probe disposition is "NOT a blanket `|| true` that could swallow a hard fail" — the
codebase believes semantic-guard owns the pattern; `checks.md` says it does not.

One of the two is wrong. Preferred fix is adding an error-swallow check rather than
deleting the comment: the pattern is real, recurring, and pattern-guards genuinely cannot
catch it — which is `checks.md`'s own stated admission criterion. Task-sized (a new check
plus its guard), so not fixed inline.

**Lead ruling for this devloop**: the shell error-swallow lane is owned explicitly and
jointly by @code-reviewer (bash idiom) and @operations (lane correctness). @semantic-guard
does a second-eyes pass, reporting anything found as `[out-of-check: error-swallow]` —
recorded as a judgment, not a fired check. A CLEAR from semantic-guard does not clear this
risk.

## Lead rulings issued during planning

- Address the Lead as `main`; `team-lead` is not a reachable address in this run.
- The zero-client-input provisioning design (helper generates the subdomain host-side and
  returns it) is the default expectation; anything weaker carries the burden of proof.
- The stale-host-side-helper case (`unknown command` from a helper predating the tree) is
  a required lane: `PRECONDITION_FAILURE` exit 2 naming the host-side rebuild. A fallback
  to the static `devtest`/`demo` org is a false-green and is prohibited.

## Gate 1 — the decisive ruling (2026-08-15)

**The task description's stated mechanism rested on a premise verified false, and was overruled.**

The prompt required provisioning "through the dev-cluster helper verb allowlist only," justified
by "Phase 1 has neither psql nor kubectl" and by ADR-0030's injection-impossibility property.
@infrastructure challenged both; the Lead verified both in this container:

- `command -v psql` → `/usr/bin/psql`; `command -v kubectl` → `/usr/local/bin/kubectl`;
  `KUBECONFIG=/tmp/devloop/kubeconfig` exported and `/tmp/devloop` mounted
  (`infra/devloop/Dockerfile:33`, `:69-78`; `infra/devloop/devloop.sh:519-520`).
  **The no-psql/no-kubectl premise is false.**
- `devloop.sh:235-239` `build_helper()` builds from `$REPO_ROOT/Cargo.toml` — the host checkout,
  never `CLONE_DIR`. The running helper has been up since Aug 13. **A verb added on this branch
  cannot exist in the helper serving this devloop**, and nothing container-side can rebuild or
  restart it. Task #3 is `env_tests: true`, so the verb route was a *certain* Layer-7
  operator-lane red, not a risk to mitigate — and in headless mode that terminates the story.

**Ruling**: no helper verb. `layer7.sh` Phase 1h invokes `infra/kind/scripts/setup.sh
--provision-org <sub>` directly, container-side. This satisfies every substantive requirement
(Phase 1; `max_concurrent_meetings` explicitly 1000; `max_participants_per_meeting` untouched;
lowercase by construction; `PRECONDITION_FAILURE` exit 2 on any failure) and adds **no verb at
all**, so it cannot undo the injection-impossibility property — which ADR-0030 scopes to the
socket surface, while §"Container-Side Test Execution" deliberately grants the container
cluster access. `crates/devloop-helper/**` and `infra/devloop/**` are untouched.

The story file gets a dated *premise corrected* note on R-7's Task 3 notes, in the form that
file already uses for R-6, citing all four `file:line` facts. Corrected in place, not deleted.

## Notable Gate-1 findings (all pre-implementation)

- **`psql -c` does not interpolate `:'var'`** (@infrastructure, verified on the live pod):
  `-c` hands the string to the server. Working form is SQL on stdin + `kubectl exec -i` +
  `--set`. Adversarially verified that `--set="sub=a'; DROP TABLE organizations; --"` renders
  as data. The accepted interface would have failed on first execution.
- **`psql "$DATABASE_URL"` fails *silently*** (@paired-database, sharpened by @infrastructure):
  the devloop's unit-test DB has its own `organizations` table, so the wrong call exits 0 and
  creates a row nothing reads — surfacing later as task 2's `org_not_provisioned`.
- **Wrong-cluster write** (@infrastructure): `setup.sh:38,79` default to `kind-dark-tower`;
  on a workstation with a manual `dark-tower` cluster that *resolves*. Fail-closed read of
  `.cluster_name` from `ports.json`, scoped to the provision path (@paired-database refinement).
- **SIGPIPE generator trap** (@security): `tr -dc … | head -c N` under `set -euo pipefail`
  kills `layer7.sh` with no STATUS line and no lane. Resolved with `od -An -tx1 -N8` —
  which also met @test's independently-computed 64-bit entropy floor for the 1000-draws case.
- **Client-side cross-origin break** (@client): the AC origin is built from the sign-up form
  field, so subdomain and page host must be one knob, not two.
- **`--provision-org` must dispatch before `check_prerequisites`**: `kind`, `podman` and
  `docker` are all absent in the container.

## Classification outcomes

- `packages/web-app/e2e/env.ts` — Domain-judgment, owner `client`, who **declined** a Lead-offered
  downgrade and took §6.5 in-loop pairing instead. Their reasoning is worth preserving: the
  pressure to downgrade was structural (headless run, frictionless path, sole authorizing party)
  and a conforming route existed.
- `infra/kind/scripts/setup.sh` and the ADR-0030 amendment — Domain-judgment, owner
  `infrastructure`, resolved by owner-implements in-loop (§6.6); they write the hunks.
- No Guarded Shared Area is touched. No migration.

## In-scope hazard confirmed by Lead

`scripts/layer7.test.sh:74` is `*) exit 0 ;;` — the fake `dev-cluster` succeeds on any
unrecognized verb, so a misspelled or dropped provisioning call leaves the success case
green. Raised by @test. Must be addressed in this diff, not deferred.
