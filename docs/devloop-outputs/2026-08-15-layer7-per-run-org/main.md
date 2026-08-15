# Devloop Output: Provision a fresh organization per layer-7 run

**Date**: 2026-08-15
**Task**: Provision a freshly generated organization per layer-7 run so repeated runs against the same dev cluster reach the same verdict (story R-7, task 3)
**Specialist**: test (paired with database)
**Mode**: Agent Teams (v2) — full, headless (run-story task #3)
**Branch**: `feature/story-runner-hardening`
**Duration**: ~19h wall-clock across three sessions (2026-08-15 01:42 → 20:30), of which the resumed
final session was ~2h. Two interruptions: one mid-implementation (§Issues #1), one between review and
the Gate-2 pipeline run. Two full `layer-all.sh` runs at 1085s and 1140s.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `9fa3bc6d962f396c7e1b4eb0dd280237e48a0961` |
| Branch | `feature/story-runner-hardening` |
| Story | `docs/user-stories/2026-08-11-story-runner-hardening.md` R-7 |
| Headless | yes (`DEVLOOP_HEADLESS=1`) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` — Gate 1 passed, implementation complete, **Gate 2 passed with Layer 7 exercised live** (both Phase-2 suites), **Gate 3 passed with all ten verdicts collected** (§Gate 3), committed. Resumed twice: once mid-implementation (§Issues #1), once between review and the Gate-2 run. On the second resumption this row read `implementation` while §Code Review Results was already fully populated — corrected rather than trusted, and the discrepancy is why the Lead re-collected verdicts instead of inferring them. |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `test` |
| Iteration | `1` |
| Paired Database | `paired-database` (spawned) |
| Security | `security` (spawned) |
| Test | `test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |
| Infrastructure (conditional) | `infrastructure` (spawned) |
| Client (conditional) | `client` (spawned) |

### Gate 1 — Plan confirmations

All ten confirmed; Gate 1 was passed on 2026-08-15 with the decisive premise ruling recorded in
`lead-notes.md` §"Gate 1 — the decisive ruling". The table below was never updated from its
`pending` template state during the run — corrected here rather than deleted, since a table of
`pending` rows sitting under a gate the loop demonstrably passed reads as an unfinished gate.

| Reviewer | Plan Status |
|----------|-------------|
| Paired Database | confirmed |
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (upgraded `layer7.sh` → Domain-judgment; resolved via §6.5 pairing) |
| Semantic Guard | confirmed |
| Infrastructure | confirmed (owner-implemented the `setup.sh` hunks) |
| Client | confirmed (owner-implemented the `env.ts` hunk) |

---

## Task Overview

### Objective

Layer 7's Nth consecutive run against the same dev cluster must reach the same verdict as
its first, for both Phase-2 suites. Today no production code marks a meeting ended, so each
organization's live-meeting count only climbs toward its cap. The browser suite creates ~7
meetings per run against the `demo` org's cap of 10, so a second run fails partway with a
403 the pipeline attributes to the code under test.

Fix: provision a freshly generated organization per layer-7 run, in `scripts/layer7.sh`
Phase 1, through the dev-cluster helper verb allowlist only.

### Constraints (from the story / task prompt — all load-bearing)

- Provision in `layer7.sh` **Phase 1**, through the **dev-cluster helper verb allowlist only**
  (ADR-0030). Phase 1 has neither `psql` nor `kubectl`. Adding a **SQL verb** would undo
  ADR-0030's injection-impossibility property.
- Set `max_concurrent_meetings` explicitly to **1000**.
- Do **not** touch `max_participants_per_meeting` — an env-test asserts it equals 100, and
  `LEAST()` would silently cap it.
- Subdomains must be **lowercase** (schema CHECK: `^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`).
- A provisioning failure must report on the **operator lane** as `PRECONDITION_FAILURE`
  exit 2 — never as a suite failure.

### Scope
- **Service(s)**: none (validation pipeline + devloop helper + test fixtures)
- **Schema**: No migration expected
- **Cross-cutting**: Yes — `scripts/`, `infra/devloop/`, `crates/env-tests/`, `packages/web-app/e2e/`

### Debate Decision
NOT NEEDED — ADR-0030 and ADR-0035 already frame the decision; the story records the
architecture validation (12/12 PASS) and the two corrections to the lead's briefing.

---

## Cross-Boundary Classification

Every planned file change. Classified per ADR-0024 §6.2. No path here is a Guarded Shared
Area (§6.4): no `proto/**`, no `crates/common/src/{jwt,meeting_token,token_manager,secret}.rs`,
no `crates/ac-service/src/{jwks,token,crypto,audit}/**`, and **no migration** — every column,
default, CHECK and index this needs already exists (confirmed by @paired-database).

### Design B (recommended — see §Planning "BLOCKING")

| Path | Change | Classification | Owner (if not mine) |
|------|--------|----------------|---------------------|
| `scripts/layer7.sh` | New Phase-1h: generate subdomain, call `setup.sh --provision-org`, verify, export to both suites; 5 new `precondition_fail` tokens | Not mine, Minor-judgment | `operations` |
| `scripts/layer7.test.sh` | New hermetic cases (Nth-run, lane, consumption, DNS-label, generator); stub-consulted markers; `*)` catch-all made loud | Mine | — |
| `infra/kind/scripts/setup.sh` | Extract `dt_psql()`; one `ORG_MAX_CONCURRENT_MEETINGS` constant; new `provision_run_org()` + `--provision-org <sub>` early-return mode; fix `seed_test_data`'s unreachable `else` | Not mine, Minor-judgment | `infrastructure` (SQL body co-signed by `database`) |
| `crates/env-tests/src/fixtures/auth_client.rs` | `const TEST_ORG_SUBDOMAIN` deleted; `ENV_TEST_ORG_SUBDOMAIN` required, no fallback; pure `resolve_org_subdomain()` + unit tests | Mine | — |
| `packages/web-app/e2e/env.ts` | `orgSubdomain` from required `E2E_ORG_SUBDOMAIN`; `E2E_BASE_URL` default **derived** from it + equality check (C1); mirrored-regex validation w/ `ANCHOR (DRY):` naming `sdk-core/src/validation/limits.ts:SUBDOMAIN_REGEX` (C3a); `describeEnv()` row (C4); stale `demo` comments at `:64-66`, `:83-85` | Not mine, **Domain-judgment** | `client` — **owner confirmed; routed `--paired-with=client` under §6.5**, hunk goes to them before Gate 2 |
| `packages/web-app/tests/e2e-env.test.ts` (new) | Node-vitest: unset/blank/uppercase/invalid throws; base-URL↔subdomain mismatch throws; derived default | Not mine, Minor-judgment | `client` (confirmed) |
| `packages/web-app/e2e/README.md` | New `## Environment knobs` row (`:163-168`); fix the now-false claim at `:158-160` | Not mine, Minor-judgment | `client` (confirmed) |
| `packages/web-app/vite.config.ts` | **Comment only** (`:53-55` asserts `demo` is the load-bearing label; it isn't) | Not mine, Minor-judgment | `client` (confirmed) |
| `docs/runbooks/devloop-validation.md` | 5 new REASON rows in §6.7 lane table; §3 enum-examples cell; §8 symptom rows; §11 changelog; §6.7 Phase-1 prose | Not mine, Minor-judgment | `operations` |
| `docs/user-stories/2026-08-11-story-runner-hardening.md` | *Premise corrected* note on R-7 / Task-3 notes (the `psql`/`kubectl` premise), in the form the file already uses for R-6 | Not mine, Minor-judgment | `operations` |
| `packages/web-app/e2e/fixtures.ts` | **No longer comment-only — see the Gate-3 correction below.** Docstring on `fillAuthFields` recording that the org-subdomain fill is LOAD-BEARING: `SignUp.svelte:14`/`SignIn.svelte:13` prefill `demo` in `$state`, so a spec that skips the fill and rides the prefill signs into the shared `demo` org and silently un-fixes R-7 for the one suite whose cap exhaustion motivated the change — with every gate green | Not mine, Minor-judgment | `client` (confirmed) |
| `docs/decisions/adr-0030-host-side-cluster-helper.md` | **Comment/prose only** — new corollary under §"build-context trichotomy": no devloop can validate a change to `crates/devloop-helper/` within its own run, because the binary is compiled from `REPO_ROOT` and launched before the dev container starts. Generalises the Gate-1 ruling so the next planner does not re-derive the same dead end. **Not** the API-table amendment Design A needed — that row stays dropped | Not mine, Minor-judgment | `infrastructure` |
| `scripts/setup.test.sh` | Test-plan items 7-9: real `setup.sh --provision-org` against PATH-stubbed `kubectl`/`psql`/`kind`/`docker`/`podman`; early-dispatch pin; the 17-row rejection table; readback-asserts-on-output | Mine | — |
| `packages/web-app/tests/e2e-env.test.ts` (new) | Node-vitest: unset/blank/uppercase/regex-invalid throw; base-URL↔subdomain host-label mismatch throws; derived default | Not mine, Minor-judgment | `client` (confirmed) |
| `scripts/guards/simple/validate-subdomain-regex-sync.sh` (new) | Test-plan item 12 — org-subdomain pattern drift guard. Delimiter-exact enumerated site table + two independently pinned counts | Mine | — |
| `scripts/guards/validate-subdomain-regex-sync.test.sh` (new) | Self-test driving the guard's FAILURE branches (a passing guard exercises none of them). Placed outside `guards/simple/` so `run-guards.sh`'s `find -name '*.sh'` does not also auto-run it as a guard | Mine | — |
| `scripts/layer3.sh` | One `run_and_emit` row wiring the drift guard's self-test (there is no `*.test.sh` auto-runner). The guard ITSELF needs no wiring — `run-guards.sh:105-112` auto-discovers `simple/**/*.sh` | Not mine, Minor-judgment | `operations` |
| `docs/devloop-outputs/2026-08-15-layer7-per-run-org/main.md` | This document | Mine | — |
| `docs/devloop-outputs/2026-08-15-layer7-per-run-org/lead-notes.md` | Lead's working file — Gate-1 rulings, the decisive premise verification, and the two out-of-scope findings destined for `docs/TODO.md` | Not mine, Minor-judgment | `main` (Lead) |
| `scripts/lang/_get_base_ref.behavior-equivalence.test.sh` | Widen case 7a's `bash -n` sweep from `guards/simple` to all of `scripts/` + `infra/` (117 files, was ~30), and add a zero-match vacuity trap. This was the repo's ONLY parse-check sweep and it covered neither `layer*.sh` nor `infra/**` — the class this diff's worst defect belonged to. Count-free by design: the old comment pinned "29 scripts", a number nothing enforced and which was already wrong | Not mine, Minor-judgment | `operations` |
| `docs/TODO.md` | One entry under §Infrastructure Validation in Devloops: the ugrep post-operand-flag class found while writing the drift guard (mechanism, 13-vs-9 reproduction, fail-open blast radius, fix shape, explicitly marked NOT swept). Task-sized, so recorded rather than fixed blind — the one site found was fixed in-tree | Not mine, Minor-judgment | `infrastructure` |

**Gate-3 correction to two rows above (@client, owner of both files).** The shipped diff is
wider than this table records, and the table is corrected rather than left standing because a
classification row that understates a change is the record a future reader cites as evidence the
change was reviewed at that tier:

- `packages/web-app/e2e/env.ts` also gained a new **exported function** `toLoopbackUrl()` and its
  `parsedBaseUrl` binding — not in the row above, and not in §R9's list of what @client authored.
  It was added during **Gate 2**, after `route.fetch: getaddrinfo ENOTFOUND
  e2e-88ef8994f5c08774.localhost` red the browser suite: Chromium resolves any `*.localhost`
  label internally, Node does not, and the old fixed `demo` label only ever worked Node-side via
  a hardcoded `/etc/hosts` entry a per-run random label can never have. Still Domain-judgment,
  still `client`-owned; **confirmed by @client at Gate 3** (this is that confirmation), together
  with the 7 new node-tier cases pinning it.
- `packages/web-app/e2e/fixtures.ts` is therefore **not comment-only**: the same Gate-2 fix
  changed the import and one behavioural line at `fixtures.ts:631`
  (`route.fetch({ url: toLoopbackUrl(route.request().url()) })`). Same owner, so the routing
  outcome is unchanged; the Minor-judgment/comment-only *description* was wrong. Also corrected
  at Gate 3: the `TestCredentials` docstring still read "for the seeded `demo` org" — a stale
  `demo` claim of exactly the kind this task removed elsewhere.

**Deliberately absent, checked rather than assumed** (@operations, for the Gate-2 scope-drift
guard's listed-but-untouched / touched-but-unlisted check): `packages/web-app/playwright.config.ts`
needs no row — it derives everything from `e2eEnv` (`baseURL: e2eEnv.baseUrl`,
`url: e2eEnv.loopbackBaseUrl`), so the `env.ts` seam covers it. It *does* import the module at
config-load time, which is why a missing `E2E_ORG_SUBDOMAIN` fails before any browser launches.

**Attribution correction** (@operations, who declined credit): the `--only` `-z "${2:-}"`
leading-hyphen hole at `setup.sh:101-113` — the reason `-x` and `--skip-build` are in the
rejection table — was found by **@test**, not @operations. I mis-credited it in my contract
message to @infrastructure. Corrected in place rather than silently, per this story's own
precedent.

**No Guarded Shared Area is touched.** Two Domain-judgment rows are routed (`setup.sh` →
@infrastructure, owner-implemented and delivered; `env.ts` → @client, §6.5 paired). A third is
**with Lead**: @operations has upgraded `scripts/layer7.sh` Minor-judgment → **Domain-judgment**,
which under §6.2 auto-routes to ESCALATE and is not negotiated in-thread. Their reasoning is
sound and I am not contesting it — §6.2's Minor-judgment examples are threshold bumps and log
fields, whereas this adds a Phase-1 step, a timeout budget, cluster-name plumbing and five
terminal lane tokens, i.e. a behaviour change to the operator/implementer lane boundary; and the
empirical test settles it, since the design required owner domain knowledge twice at Gate 1,
once reversing a `current-context` decision I had already drafted. Their proposed remedy is the
§6.5 Paired flag on the `layer7.sh` hunks — no re-route, no new devloop. No `migrations/**`
(@code-reviewer: correct that §6.4's *criterion* makes it Guarded regardless of the enumerated
`db/migrations/**` path — which is why the answer is zero migration files, confirmed by
@paired-database). No `proto/**`, no crypto, no `crates/common`.

### Design A rows — NOT IMPLEMENTED (ruled out by @main, recorded for the audit trail)

These five rows were in my first draft and are **dropped**: `crates/devloop-helper/src/protocol.rs`,
`commands.rs`, `main.rs`, `infra/devloop/dev-cluster`, and the ADR-0030 API-table amendment.
Three were **Domain-judgment** in `infrastructure`'s domain, which under ADR-0024 §6.3 routes
to owner-implements and, in a headless run, terminates the story. I classified them that way
honestly rather than downgrading to make the routing easier (§6.2 forbids downgrade) — and
that honest classification is what surfaced the conflict at Gate 1, where a plan change is
cheap, instead of at Gate 3.

**Net effect of the ruling on this table: `crates/devloop-helper/**` is untouched, and every
remaining row is Mine or Minor-judgment except `packages/web-app/e2e/env.ts`, which is
Domain-judgment with its owner already paired in under §6.5.** No Domain-judgment row is left
unrouted.

---

## Planning

### Premise correction (resolved by Lead ruling, 2026-08-15)

**Status: settled.** I raised this as a blocking premise conflict; @infrastructure raised it
independently; @main verified both premises directly and ruled. **The helper verb is dropped.**
`layer7.sh` Phase 1h invokes `infra/kind/scripts/setup.sh --provision-org <sub>` directly,
container-side. The record of why is kept below because the task description still says
otherwise, and a corrected premise that leaves no trace gets re-litigated.

The task prompt says: *"Phase 1 has neither `psql` nor `kubectl`, and adding a SQL verb would
undo that ADR's injection-impossibility property."* The story repeats it in Architecture
Validation ("two corrections to the lead's briefing … `layer7.sh` Phase 1 has **neither
`psql` nor `kubectl`**").

**Both tools are present.** I verified this in this container *before* @infrastructure's
message arrived, and they independently verified the same:

```
$ command -v kubectl psql
/usr/local/bin/kubectl
/usr/bin/psql
```

- `infra/devloop/Dockerfile:33` installs `postgresql-client`; `:69-78` installs kubectl
  (sha256-verified).
- `infra/devloop/devloop.sh:519-520` exports `KUBECONFIG=/tmp/devloop/kubeconfig` in the
  *same conditional block* that bind-mounts the helper runtime dir — so helper-socket-present
  ⟺ kubeconfig-present, and `layer7.sh` only reaches Phase 1 after
  `__env_test_cluster_available` (`:375-396`) confirms the socket.
- ADR-0030 §"Container-Side Test Execution" grants the container kubectl + a **cluster-admin**
  kubeconfig as an explicit, risk-accepted design decision.
- Container-side `kubectl exec` is already established practice, not a new capability:
  `crates/env-tests/src/canary.rs:262,297` and `crates/env-tests/tests/40_resilience.rs`.

I therefore have **two candidate designs**, and I am not choosing between them unilaterally —
this is a classification-and-premise conflict and ADR-0024 §6.6 routes it to the Lead.

| | **Design A — new `provision-org` helper verb** (what the task brief mandates) | **Design B — container-side `kubectl exec … psql`** (what @infrastructure will accept) |
|---|---|---|
| Injection surface | Zero client-supplied string; helper generates the subdomain | Zero: the subdomain is generated *in `layer7.sh` itself* from a closed lowercase alphabet — there is no trust boundary to cross, so there is nothing to validate |
| ADR-0030 fit | **Role extension** — the ADR says the helper "is a build-and-deploy tool only… does NOT run tests, proxy kubectl, or serve logs". Needs an ADR amendment | Uses a capability ADR-0030 already grants explicitly. No amendment |
| Does it reduce container authority? | **No.** The container already holds cluster-admin via kubeconfig; a verb moves SQL-string construction *onto the host side* of the trust boundary — the wrong direction | n/a |
| **Deployable in this run?** | **NO.** `devloop.sh:128-129,235-239` builds the helper from `REPO_ROOT` (host checkout), never `CLONE_DIR` — that pin is ADR-0030's tamper protection. The running helper cannot know a verb added in this tree. Task #3 is `env_tests: true`, so Layer 7 reds at Phase 1 (`helper-verb-unsupported`, exit 2) for the whole headless run, unfixable from inside the container | **Yes.** The helper's *runtime* project-root is `CLONE_DIR`, and `layer7.sh`/`infra/kind/scripts/**` in this tree are live immediately |
| Cross-boundary cost | `crates/devloop-helper/**` + `dev-cluster` + ADR = **Domain-judgment**, which @infrastructure has stated pre-plan they will rule owner-implements → **terminates this headless story** | `infra/kind/scripts/_psql.sh` + `setup.sh` call-site rewrite = Minor-judgment, and @infrastructure has offered to co-sign the hunks or write them |
| Name surface (@code-reviewer) | Six places (enum, parse arm, `is_write`, dispatch, client `case` + 2 usage blocks + error string, ADR table) — partial-addition hazard | One place |
| Stale-helper lane | Required, and it is the lane that fires | **Does not exist** — the entire failure class is removed |

**Ruling (@main): Design B, with the socket hop removed rather than replaced by raw
`kubectl exec` in `layer7.sh`.** The decisive fact is the last row — Design A *cannot pass its
own gate in this run*: the running helper has been up since Aug 13 and nothing in this
container can rebuild or restart it, so Phase 1h would meet `invalid_command` with certainty,
not with some probability worth a REASON token.

Two things the ruling preserves that neither raw option had:

- **The SQL stays in exactly one place, in one language** — `setup.sh`, which already owns org
  seeding. `layer7.sh` calls `setup.sh --provision-org <sub>`; it contains no SQL, no
  connection details, and no `kubectl exec` of its own. That answers @infrastructure §4 and
  @dry-reviewer #1 more completely than a shared `dt_psql` fragment would have: there is no
  second caller to keep in step.
- **`setup.sh` needs no new configuration knob.** I verified the mechanism directly: the
  container kubeconfig holds exactly one context, `kind-devloop-story-runner-hardening`, and
  `ports.json.cluster_name` is `devloop-story-runner-hardening` — so `layer7.sh` passing
  `DT_CLUSTER_NAME` from the ports.json it *already reads* makes `setup.sh`'s existing
  `KUBECTL="kubectl --context kind-${CLUSTER_NAME}"` (`setup.sh:79`) resolve correctly. That
  is ADR-0030's own documented §"setup.sh Parameterization" parameter, used as intended. No
  `DT_KUBECTL` override, no `_psql.sh` fragment, no new surface.

**What evaporates with the verb, stated explicitly so no dead scaffolding is left behind**
(@main): the `helper-verb-unsupported` token and its `invalid_command` /
`deny_unknown_fields`-`invalid_request` detector (@operations B, @code-reviewer 1,
@security 6); the `org-provision-helper-busy` token and the whole write-mutex/busy-collision
question (@observability's addendum, @operations A5); the `__dev_cluster_setup` →
`__dev_cluster_write` generalization (@dry-reviewer #4 — with no socket call left there is
nothing to be busy-tolerant about, so that refactor would only churn working code:
`__dev_cluster_setup` is left exactly as it is); the six-place verb name surface
(@code-reviewer 1); the ADR-0030 amendment; and the entire `crates/devloop-helper/**`
diff, including its injection-corpus additions. **Six new REASON tokens become five.**

**Required with the ruling**: a *premise corrected* note on R-7 / the Task-3 notes in
`docs/user-stories/2026-08-11-story-runner-hardening.md`, in the same form that file already
uses for R-6 and the 2026-08-14 correction — the no-psql/no-kubectl premise verified false on
2026-08-15, with file:line evidence. **Corrected, not deleted**, for exactly the reason the
file states for its own earlier correction: otherwise the original sentence stays citable as
evidence that a helper verb was required.

§"Design A, retained" at the end keeps the verb design on the record.

### v2 reconciliations (post-ruling reviewer input)

Recorded here rather than scattered, because two of these were *conflicting* rulings and one
was a live defect a reviewer measured.

**R1 — kubectl context: @paired-database (plain `kubectl`) vs @observability (derive
`DT_CLUSTER_NAME` from ports.json). Reconciled in favour of @paired-database's option 1, via
@infrastructure's `DT_KUBECTL` knob.**
@observability measured the live defect: a container-side `setup.sh` gets no `DT_CLUSTER_NAME`,
so `setup.sh:37,79` resolve to `kubectl --context kind-dark-tower`, and
`kubectl --context kind-dark-tower get pods -n dark-tower` → `context "kind-dark-tower" does
not exist`. Without a fix Phase 1h fails **100% of the time** and blames the database.
My draft fixed it by passing `DT_CLUSTER_NAME` from `ports.json`. @paired-database is right
that this is the weaker of the two options: it still re-encodes Kind's `kind-<name>` prefix in
a second place, whereas the container kubeconfig has **exactly one context, which is by
construction the cluster the Phase-2 suites talk to**. So:
- `setup.sh:79` becomes `KUBECTL="${DT_KUBECTL:-kubectl --context kind-${CLUSTER_NAME}}"`
  (@infrastructure's proposed knob, one line).
- `layer7.sh` sets `DT_KUBECTL=kubectl` — current context, **zero encodings of cluster
  identity anywhere**, matching what `canary.rs:262,297` already does container-side.
- Host-side callers are unchanged (`DT_KUBECTL` unset ⇒ today's exact string).
- `layer7.sh` no longer needs `ports.json` for this at all.
- **Assert before the INSERT**, per @paired-database: the provision path checks the context
  resolves, failing with `org-provision-context-unresolved`
  naming the resolved context — so a mismatch reads "wrong cluster", never "provisioning bug".
  **Corrected at Gate 3 (@paired-database):** this bullet previously also claimed the path
  probes that `dark-tower`/`postgres-0` are *reachable*. It does not, and deliberately so —
  `setup.sh:894-900` is a pure kubeconfig read. Phase 1e (pods-healthy) already establishes pod
  reachability immediately before 1h, and a dead `postgres-0` surfaces through
  `org-provision-failed` with `kubectl exec`'s own error relayed verbatim and a remediation line
  that names the postgres pod. A dedicated probe would need a sixth REASON token against a
  runbook @operations capped at five. Corrected rather than deleted: a record claiming a control
  that is not in the code is the drift class this story exists to remove.
  @observability: this is why that token stays rather than being the row you'd expect me to
  drop — the failure class is reachable (the socket⇒kubeconfig coupling at `devloop.sh:519-520`
  is emergent, not guaranteed — @infrastructure §5) and its fix differs from every other token.

**R2 — @paired-database: do NOT use `$DATABASE_URL`.** Confirmed and already the design, but
worth recording because `psql` really is on PATH now and `psql "$DATABASE_URL"` is the natural-
looking implementation. `DATABASE_URL` points at
`devloop-story-runner-hardening-db:5432/dark_tower_test` — the devloop's *unit-test* DB
container — while the cluster services read `postgres.dark-tower.svc.cluster.local/dark_tower`.
An org inserted there is invisible to AC and GC, and post-task-2 the failure is *actively
misleading*: Phase 1 reports success and Phase 2 fails citing provisioning that did run.
Also folding in: `kubectl exec … -c postgres` explicitly (the pod has an init container;
without it kubectl emits a `Defaulted container` note), capture **stdout only, never `2>&1`**,
and `psql -X` alongside `ON_ERROR_STOP=1` so a stray `.psqlrc` can't alter session settings.

**R3 — @observability NEW-1: channel discipline.** `setup.sh`'s `log_*` are bare `echo -e`
(stdout, with ANSI codes) while `layer7.sh`'s stdout is the `STATUS=` channel that
`tee_collect_statuses`/`parse_status_line` parse. Phase 1h therefore **captures** setup.sh's
stdout, relays it to **stderr** for the operator, and greps the captured text for the
`PROVISIONED_ORG ` prefix. Nothing from setup.sh reaches layer7's stdout. This keeps the
existing `__dev_cluster_setup:311-312` invariant rather than inventing one.

**R4 — @observability NEW-3 / @test B: `--provision-org` must early-exit.** Following the
`--only` precedent at `setup.sh:974-980`, which early-`return`s before `check_prerequisites`.
@observability's second reason is the sharper one and I had not considered it: falling through
to `main()` reaches `seed_test_data` (`:538-552`) and `create_ac_secrets`, both of which put a
**`client_secret_hash` / master key into SQL passed to `psql -c`** — i.e. a credential into
`${DEVLOOP_TMP}/layer-7*.log`. That makes the early-exit a credential-containment control,
not just a performance one, and it is why @test's item B (assert `kind`/`docker`/other-kubectl
stubs were NOT invoked) is a required test rather than a nice-to-have.

**R5 — @test C: do not copy the seeders' non-fatal shape.** `seed_test_data`/`seed_demo_org`
`log_error` and **continue**, returning 0. `provision_run_org` must return non-zero on any
failure, or R-7's "PRECONDITION_FAILURE exit 2, never a suite failure" is violated in exactly
the forbidden direction.

**R6 — @test E: the regex is in FOUR places today, not three — and my design would make six.**
`migrations/20250118000001_initial_schema.sql:16`, `infra/docker/postgres/init.sql:29`
(@test found this one; it was in nobody's count), `packages/sdk-core/src/validation/limits.ts:101`,
plus my `layer7.sh` generator, `setup.sh` validator and `env.ts` mirror. Every one of the three
new copies has been independently ruled *required* (defense-in-depth at a trust boundary —
@paired-database, @dry-reviewer, @client). CLAUDE.md's rule for exactly this situation is
"derive one from the other, **or add a guard that fails validation on drift**." So: **a drift
guard that extracts the pattern literal from all six sites and asserts byte-identity**, wired
into `layer3.sh`. That converts six copies into one *enforced* encoding, and it fails loudly
the day someone edits one. (`org_extraction.rs:67-77` is a fifth, hand-rolled, weaker variant
— out of scope, and explicitly do not add a sixth style.)

**R7 — @test G: the 1000-draws case does not ship as drafted.** @test's arithmetic is right and
I had not done it: with an 8-hex (32-bit) suffix, P(collision) over k draws ≈ k²/2^33, so
k=1000 gives ~1.2e-4 — one unattributable red per ~8500 runs, in a file that runs on every
devloop and every CI run, under ADR-0028's zero-retry policy. Two changes:
- **Suffix widened 8 hex → 12 hex** (6 bytes, 48 bits). Costs nothing (`head -c6`). Total
  length ≤ 57 ≤ 63. Realistic bound: 1000 runs accumulated on one cluster gives
  ~1000²/2^49 ≈ 1.8e-9. And a collision is **loud** anyway — no `ON CONFLICT`, so it is a
  unique violation → PRECONDITION_FAILURE, never a silent reuse.
- **The 1000-draw statistical case is dropped**, taking @test's own second option: assert the
  entropy width, the charset, and the `/dev/urandom` source structurally, plus two-draws-
  distinct (which the Nth-run case A already covers end-to-end). No statistical assertion, no
  wall-clock cost, nothing to flake.

**R8 — @paired-database 5: retries must regenerate.** With no `ON CONFLICT`, re-invoking
Phase 1h with the *same* subdomain after a transient failure hits the unique violation and
reads as a provisioning defect. Phase 1h does not retry internally; if it ever does, the
subdomain is regenerated per attempt. Stated as an invariant in the code comment.

**R9 — @client has authored the `env.ts` hunk** (paired under §6.5) and I am taking it as
written: `readSubdomain` throwing on unset *and* blank, the mirrored `SUBDOMAIN_REGEX` with the
`ANCHOR (DRY):` comment (now also covered by R6's drift guard), `baseUrl` defaulting from the
subdomain, the host-label equality check with `?? ''` for `noUncheckedIndexedAccess`, the two
comment corrections, and `E2E_ORG_SUBDOMAIN` first in `describeEnv()`. One consequence to
record: `playwright.config.ts:13` imports the module at config-load time, so a missing var
fails **before any browser launches**, with the named error rather than a spec timeout. That
is the intended shape, and it is why the README row marks this knob **required (no default)**
while the other four keep their documented defaults.

**Token recount for @observability: six → five.** Alive: `org-provision-context-unresolved`,
`org-provision-timeout`, `org-provision-failed`, `ac-unreachable`, `org-provision-unverified`
(which also covers @test H's zero-row readback). **Dead, and not written anywhere:**
`helper-verb-unsupported` and `org-provision-helper-busy` — both were stale-helper/write-mutex
cases that cannot occur without a socket call. A runbook row for an unreachable failure mode
is drift in the same way a missing row is, and worse, so neither is being written.

### The mechanism, restated — and it is wider than the task names

**Instance language** (the task): "the `demo` org's concurrent-meeting cap fills up across
layer-7 runs, so run 2 gets a 403."

**Mechanism language**: *Layer 7 runs against a cluster whose database persists across runs,
and both Phase-2 suites bind to **fixed-name rows** in that database. Any per-row counter
that only grows and is bounded by a cap therefore makes run N's verdict a function of runs
1..N−1.* The live-meeting count is one instance of that class.

Restating it that way produces a wider class than "meetings":

- **Also in the class, and fixed by this change for free:** anything keyed by `org_id`.
  `users_org_email_unique (org_id, email)` is the sharpest — today the browser suite is safe
  only because `fixtures.ts` memoizes one shared registration and the Rust suite generates
  `test-{uuid}@envtest.dev`; a spec that ever used a fixed email would collide on run 2 with
  no cap involved. Per-run orgs make that airtight by construction (the story's
  architecture-validation section already notes AC "accumulates no state across runs …
  user uniqueness is scoped `(org_id, email)`, which per-run orgs make airtight").
- **Still in the class, and NOT fixed by this change:** anything keyed *above* `org_id`.
  Verified concretely: AC's registration rate limiter counts `auth_events` rows
  **by IP address only, org-independent** (`crates/ac-service/src/services/user_service.rs:203-229`
  — `WHERE ip_address = $1::inet AND event_type = 'user_login' AND success = true`, 5 per
  60 min). A per-run org does **not** reset that budget. It is not a regression (the same
  number of registrations from the same IP happens today), but it is the same mechanism one
  level up, and it bounds how many suites can share a cluster-hour. Also in this residue:
  `service_credentials`, AC signing keys, and schema drift on a reused cluster (already in
  the story's Deferred list).

Two decisions fall directly out of the restatement, and I flag them for reviewers:

1. **One org per run, not two.** I checked whether the two suites need separate orgs to keep
   their registration budgets apart. They do not, because the limiter is IP-keyed and
   org-independent — merging them changes nothing about it. One org keeps the SSoT simple.
2. **The verification probe must not consume a budget the suites share.** See §Verification.

### Design

Six pieces. The load-bearing one is the shape of the helper verb.

#### 1. Provisioning transport: `layer7.sh` → `setup.sh --provision-org <sub>`, no new verb

```
DT_CLUSTER_NAME="$(jq -r '.cluster_name' "$ENV_TEST_PORTS_JSON")" \
  timeout "${DEVLOOP_ORG_PROVISION_TIMEOUT:-120}" \
  "${__repo_root}/infra/kind/scripts/setup.sh" --provision-org "$sub"
```

`layer7.sh` holds **no SQL, no connection details, no `kubectl exec`** — only the generated
subdomain and the cluster name it already reads from `ports.json`.

**Injection posture — the explicit argument constraint 2 asks for.** ADR-0030's
injection-impossibility property is about *what crosses the helper socket*; this design
crosses it not at all, so the property is untouched by construction. The relevant question
becomes "can anything attacker-influenced reach a SQL text?", and the answer is no, on three
independent links:

- **The subdomain is generated locally, in `layer7.sh`, over a closed alphabet** (§3).
  Lowercase and quote-free become **structural properties of the generator**, not validation
  rules that can be forgotten.
- **It still reaches psql as a bound literal, not concatenation** — via **SQL on stdin with
  `--set`**, not `psql -c`:

  ```
  ${KUBECTL} exec -i -n dark-tower postgres-0 -c postgres -- \
      psql -X -U darktower -d dark_tower -v ON_ERROR_STOP=1 \
           --set=sub="$sub" --set=name="$name" --set=max_meetings="$ORG_MAX_CONCURRENT_MEETINGS" \
           -tAq <<'SQL'
  INSERT INTO organizations (subdomain, display_name, plan_tier, max_concurrent_meetings)
  VALUES (:'sub', :'name', 'enterprise', :max_meetings)
  RETURNING org_id;
  SQL
  ```

  `:'sub'` / `:'name'` are quoted (SQL string literals); `:max_meetings` is **unquoted** so it
  lands as a numeric literal. The heredoc delimiter is **quoted** (`<<'SQL'`) so the shell
  performs no expansion on the SQL body (@security). `-i` is required — omitting it gives a
  confusing empty read.

  **An earlier draft of this section specified `psql -v … -c "… :'sub' …"` and claimed the same
  quoting property. That form does not work**: @infrastructure verified on the live pod that
  `psql -c` performs no variable interpolation — `psql -v sub=abc -tAc "SELECT :'sub'"` returns
  `ERROR: syntax error at or near ":"`. Corrected here rather than quietly rewritten, because the
  original sentence named a control that was not there, and a reviewer could otherwise cite it as
  evidence the `-c` form had been assessed and approved. The shipped code was always the stdin
  form (`setup.sh:889-895`); this was documentation drift, not a build risk.

  Defense in depth: the safety property must not rest solely on the generator's alphabet holding
  forever (@paired-database, @security).
- **No shell interpreter on the path.** `kubectl exec … -- psql …` is argv; no `sh -c`, no
  `eval`.

@infrastructure's sharpest point is worth recording because it applies to *any* design:
`Command::new().arg()` protects the **shell**; SQL is a **second interpreter** it does not
protect. That is why `:'sub'` + a closed generator alphabet is load-bearing here and would
have been load-bearing in Design A too.

**Also removed by this transport, and worth naming:** the entire stale-helper failure class
(@operations item B, @code-reviewer item 1, @security item 6). There is no verb, so there is
no version skew between the host-built binary and this tree, no `invalid_command` /
`deny_unknown_fields`-`invalid_request` detector to write, and no host-side rebuild step
standing between this commit and a green Layer 7.

#### 2. `setup.sh` — one psql invocation, one cap constant, one new mode

Approved unchanged by @main; restated for completeness.

- Extract `dt_psql()` wrapping the
  `${KUBECTL} exec -n dark-tower postgres-0 -- psql -U darktower -d dark_tower` prefix that
  appears three times today. The four DB constants get one home. Real duplication, no
  semantic mode flag.
- One `readonly ORG_MAX_CONCURRENT_MEETINGS="${DT_ORG_MAX_CONCURRENT_MEETINGS:-1000}"` — kills
  the two `1000` literals on `setup.sh:566` and gives the new path the same home
  (config-over-hardcoding). Explicitly **not** unified with the schema `DEFAULT 10` or
  `infra/docker/postgres/init.sql` (@dry-reviewer's own carve-out).
- New `provision_run_org <subdomain>`, run by a new `--provision-org <sub>` mode that returns
  from `main()` before any cluster work:

```sql
INSERT INTO organizations (subdomain, display_name, plan_tier, max_concurrent_meetings)
VALUES (:'sub', :'name', 'enterprise', 1000)
RETURNING org_id;
```

  Column-by-column per @paired-database: `max_participants_per_meeting` **omitted entirely**
  (schema default 100, so `LEAST($6, o.max_participants_per_meeting)` at
  `gc-service/src/repositories/meetings.rs:245` cannot cap the 100 that
  `env-tests/tests/23_meeting_creation.rs:116` asserts); `is_active` omitted (default true);
  `org_id` omitted (`gen_random_uuid()`); `plan_tier='enterprise'` mirroring `devtest`.
  **No `ON CONFLICT`** — the row is fresh per run, so a unique violation is a *collision to
  detect*, not a state to reconcile; `DO NOTHING` would hand the suites a stale at-cap org
  (R-7 wearing a new hat) and `DO UPDATE` would resurrect one.
- `-v ON_ERROR_STOP=1` so a statement-level SQL error actually yields non-zero
  (@semantic-guard: psql otherwise exits 0 on a failed statement — the precise route to
  "provisioning silently didn't happen"). **But fail-loud does not rest on it**: here the exit
  code is `kubectl exec` propagating a *remote* status, which no hermetic stub can pin, so the
  authoritative check is the readback below asserting on **output**.
- **Readback asserts on output, not exit code**: `SELECT count(*) … WHERE subdomain = :'sub'
  AND is_active = true` must equal exactly `1`. A zero-row `SELECT` **exits 0**, so the natural
  "the readback ran and exited 0" assertion would move the success-looking-no-op bug one step
  later instead of killing it. The predicate is copied from AC's `organizations::get_by_subdomain`
  so the check answers the question Phase 2 will actually ask.
- Emits exactly one stable machine-readable line on stdout,
  `PROVISIONED_ORG org_id=<uuid> subdomain=<sub>`; **`layer7.sh` positive-matches** that prefix
  (fail-closed, mirroring `__cluster_ready`'s discipline) rather than parsing free text.
  `log_step`/`log_info` also write to stdout in this script, which is why a prefix match rather
  than a "last line" assumption is required.
- **Also fixing in-tree now** (CLAUDE.md "fix, don't defer"): `seed_test_data`'s
  `if [ $? -eq 0 ] … else log_error` at `setup.sh:554-558` and `:569-573` is **unreachable** —
  the script is `set -euo pipefail`, so it aborts at the `psql` line and the `else` never
  runs. Converted to `seed_demo_org`'s correct if-condition form (which documents the trap at
  `:585-587`). One-line class of fix, found by @semantic-guard, done in this commit.
- **`seed_demo_org` left alone, with a reason — @dry-reviewer has since ruled this ACCEPTED**
  (ADR-0019 Pattern A, deliberate parallel statements), conditional on two one-liners I am
  folding in:
  - **Condition 1**, cross-reference comments at both ends so a future "superseded by per-run
    provisioning" cleanup cannot silently break manual env-test runs: `auth_client.rs` points
    at `infra/kind/scripts/setup.sh:seed_test_data`, and `seed_test_data` notes what it backs.
    **One correction to @dry-reviewer's framing, and it matters:** there is **no fallback**.
    Every other reviewer (@operations, @test, @client) and @main require fail-loud on unset,
    because `${…:-devtest}` is R-7 in different clothing. `devtest` is reached only by an
    *explicit* `ENV_TEST_ORG_SUBDOMAIN=devtest cargo test -p env-tests`, which the loud error
    message names. So the comment should read "backs the documented manual invocation", not
    "backs the fallback constant" — the dependency @dry-reviewer identified is real and the
    comment is still required; only its wording changes.
  - **Condition 2**, `ORG_MAX_CONCURRENT_MEETINGS` applies at `seed_test_data` and the new
    provision path **only**, with the carve-out stated on the constant's own definition:
    `seed_demo_org` deliberately takes the schema default of 10 ("the right profile for a
    user-facing demo org", `setup.sh:580-583`), so a reader seeing two of three call sites
    using the constant does not "finish the job".
  - Two checkable facts @dry-reviewer supplied that strengthen the case, recorded here:
    `SignUp.svelte:14` / `SignIn.svelte:13` both prefill `$state('demo')`, so retiring
    `seed_demo_org` breaks interactive `pnpm dev` sign-up outright; and `fixtures.ts:169`
    `.fill()`s from `e2eEnv.orgSubdomain`, so the E2E suite does not ride on that prefill.
    **Standing constraint for future specs**: any spec that relies on the prefilled default
    instead of filling from `e2eEnv` silently signs into `demo` and un-fixes R-7 for the very
    suite that fails today.

  The original reasoning, unchanged: `demo` is the *interactive* default typed into `SignUp.svelte:14` /
  `SignIn.svelte:13` for `pnpm dev`, unrelated to layer 7. `devtest` likewise stays for
  manual `cargo test -p env-tests`. Both are additive to, not replaced by, the per-run org.
  Unifying the three INSERTs into one function would need a conflict-policy parameter that
  changes the statement's core semantics (DO UPDATE / DO NOTHING / bare) — three different
  intents, correctly three statements. Sharing the *invocation* and the *cap* is the real
  duplication and that is what I am removing. **@dry-reviewer: please rule on this.**

#### 3. Subdomain generation — same rules, in bash in `layer7.sh`

**FINAL SHAPE: `e2e-<16 lowercase hex>`, 20 chars, zero inputs.** `__generate_org_subdomain()`
in `layer7.sh`. Implemented; the bullets below are superseded where they conflict.

The slug is **gone** — @client found that it was valid by *sanitization*, not construction, and
that the gap lands on the wrong lane: a malformed subdomain is exported, reaches `env.ts`'s
required-var check, and throws at Playwright **config-load**, so the browser suite exits
non-zero and Phase 2 records `FAIL browser-e2e-failed`, **exit 1, implementer lane** — a
provisioning-input defect billed to the diff, the exact misattribution R-7 forbids. They also
caught my arithmetic (`4+40+1+16 = 61`, not the 57 I quoted — still under 63, but one of us was
miscounting and a later slug-cap bump would cross it silently). Dropping the slug removes the
failure class instead of guarding it, and human-readable identification moves to the org's
`display_name`, which has no DNS-label constraint. Entropy is unaffected: the 64 bits were
always in the hex.

Superseded historical bullets (kept for the reasoning, not the shape):

- slug lowercased, every char outside `[a-z0-9-]` → `-`, runs collapsed, truncated to 40
  **before** the suffix is appended (so truncation can never leave a trailing `-`), leading/
  trailing `-` trimmed; empty result → literal `run`.
- **8 bytes → 16 hex chars, via `od -An -tx1 -N8 /dev/urandom | LC_ALL=C tr -d ' \n'`.**
  Explicitly **not** `$RANDOM`, PID, or seconds-resolution time; and explicitly **not**
  `tr -dc … | head -c N`, which @test measured returns **rc 141** under `pipefail` (@security's
  SIGPIPE finding) while the `od` form returns 0. `od` emits lowercase hex by construction, so
  nothing is normalized between generation and INSERT (@security S12).
- Total ≤ 53 ≤ 63 (`VARCHAR(63)`), starts `e`, ends hex.
- Validated against `^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$` **copied verbatim from
  `migrations/20250118000001_initial_schema.sql:16`**, not paraphrased, before use.
- Cannot collide with `devtest` or `demo` by construction (fixed `e2e-` prefix).
- Random, not PID/second-resolution — a predictable source can collide with a concurrent
  devloop or a fast rerun, which reintroduces exactly the cross-run bleed R-7 removes. This
  identifier is **not a secret** and I am not claiming entropy properties beyond collision
  avoidance (@security).
- Tests: see §Test plan item 10 — structural shape assertion (`e2e-` + exactly 16 chars from
  `[0-9a-f]`), source, and two-draws-distinct. **No 1000-draw statistical case** and no
  slug-sanitizer cases (there is no slug and no sanitizer). Called directly on the sourced
  helper, the way `layer7.test.sh:446-451`/`:461` already call `__dev_cluster_setup` /
  `__wait_http_ready`.

#### 4. `layer7.sh` Phase 1h — the operator lane

Placed **after** step (f), so a failure can legitimately be narrowed: (e) confirmed pods
healthy and (f) confirmed the NodePort HTTP path works. Before (g)/Phase 2.

**No busy-tolerance and no `__dev_cluster_write` refactor** (@main's ruling): with no socket
call there is no write slot to contend, so generalizing `__dev_cluster_setup` would churn
working code for nothing. It is left exactly as it is, and its two existing self-test cases
(`layer7.test.sh:445-451`) stay untouched.

Bounded by `timeout "${DEVLOOP_ORG_PROVISION_TIMEOUT:-120}"` (@operations C — there is no
budget anywhere on this path today; a wedged `kubectl exec … psql` currently hangs the
devloop, and under `run-story` hangs the whole story through no lane). 120s sits well under
`DEVLOOP_HEALTH_BUDGET` (300) and the suite budgets (600). The verification probe is bounded
too.

**Five REASON tokens, all `PRECONDITION_FAILURE` / exit 2, all cause-named not step-named:**

| Token | Fires when | Operator fix |
|---|---|---|
| `org-provision-context-unresolved` | `kubectl` absent, or `ports.json` has no `cluster_name` | Fail-closed precondition, **probed not assumed** (@infrastructure §5). The socket⇒kubeconfig coupling at `devloop.sh:519-520` is an emergent fact, not a guarantee. Re-run `devloop.sh` on the host. |
| `org-provision-timeout` | rc 124 from the bounded call | `kubectl exec … psql` wedged; check the postgres pod (`kubectl -n dark-tower get pods`). |
| `org-provision-failed` | any other non-zero, or stdout lacking the `PROVISIONED_ORG ` line, or a generated subdomain failing the regex | Real provisioning failure; the relayed `setup.sh` stderr carries the psql error. |
| `ac-unreachable` | AC `/health` not 2xx within budget | AC is not answering — the org probe's result would be unattributable. Check the ac-service pod. |
| `org-provision-unverified` | AC **is** healthy but returns 404 for the new subdomain's Host | The row was reported created but AC will not resolve it (`get_by_subdomain`: `WHERE subdomain=$1 AND is_active=true`). Provisioning fault; do not rerun expecting a different answer. |

**Name corrected 2026-08-15 (implementation).** The first token was drafted here as
`cluster-access-missing` and SHIPPED as **`org-provision-context-unresolved`** — the name in
`layer7.sh:578`, in the runbook's §6.7 row and §3 enum list, and in `layer7.test.sh`'s lane
case. Corrected in place rather than left as-is: an operator who hits the token greps the plan
for it, and a REASON token that appears in the design document but in no code is drift of the
worst kind — it reads as documentation of a lane that does not exist. The shipped name is also
the better one (it names the CAUSE, "we cannot resolve which cluster to provision into",
rather than the symptom "access is missing"), which is why the code was not changed to match
the plan.

`ac-unreachable` is ordered *before* `org-provision-unverified` precisely so the two causes
@observability named are separated by evidence I actually capture, rather than by an "either
X or Y" cell.

**Shell-lane discipline** (@code-reviewer 4, @operations): no `|| true` / `|| :`; no post-hoc
`$?` under `set -e` — every check is `if ! cmd; then precondition_fail …`; `local` declaration
split from command-substitution assignment so the callee's status is not masked; and the file
runs under `IFS=$'\n\t'` **without space**, so any multi-word command string is split into an
array the way `__wait_http_ready:284-291` does (the worked example of what that breaks).

**No fallback, anywhere** (@operations, Lead ruling 2). There is no `|| true`, no `||`-chained
default, no warn-and-proceed, and specifically **no `${…:-devtest}` / `${…:-demo}`** in the
shell or in either consumer. If the subdomain is absent at suite time the consumer fails
loudly — and Phase 1 will already have exited 2 before it could get there. Loki's SOFT probe
(`:479-482`) is this layer's one deliberate warn-and-proceed and it is justified by the
env-tests crate's *declared* optional-Loki semantics; provisioning has no such semantics.

**Loud, greppable, on every path** (@observability 3): the moment the response is read,
`Layer7: provisioned org subdomain=<x> org_id=<y>` goes to stderr — unconditionally and
before any subsequent branch — so it precedes every later failure note in the same stream,
including `env-tests-failed` and `browser-e2e-failed`. Step timing via
`t_step=$(layer_now)` / `emit_step_duration provision-org "$t_step"`; **no `date +%s`**
(`_layer_skeleton.test.sh` greps for it).

#### 5. Verification — what "token obtainable" is actually probed as

Two layers:

- **Helper-side, authoritative for "exists + is_active":** the `RETURNING org_id` row must
  come back, exactly one, or the verb errors. Zero rows is a hard failure, never a shrug.
- **layer7-side, for "AC will resolve it":** one `curl` to
  `POST ${ENV_TEST_AC_URL}/api/v1/auth/user/token` with `Host: <sub>.<achost>` and a
  **deliberately non-existent** email. **404 → `org-provision-unverified`. 401 → PASS.**

This exercises the exact middleware the story identifies as where a provisioning failure
surfaces (`org_extraction` → `organizations::get_by_subdomain` → `AcError::NotFound`), and it
is the discrimination that makes the check meaningful rather than decorative.

**Why not register a probe user to prove a token is literally minted.** I checked the cost.
Registration is rate-limited at 5 per 60 min counted **by IP, org-independent**
(`user_service.rs:203-229`), so a registering probe spends from the very budget the two
suites share (Rust ~4 registrations, browser 2) and could itself cause the flake it exists to
prevent. The 401 probe by contrast consumes **nothing**: `token_service.rs:222-235` looks the
user up first and only rate-limits `by_user` when the user exists, so a non-existent email
never touches a limiter, and the counter that *is* IP-keyed only counts `success = true`
events. I am stating plainly that this proves "AC resolves the org and reaches credential
checking" rather than "a JWT was minted" — one step short of the story's wording, bought back
by not perturbing the thing being tested. **@security / @test: flag if you want the stronger
probe and I will take the budget hit.**

#### 6. Both suites consume it — or the change is inert

- **Rust** (`crates/env-tests/src/fixtures/auth_client.rs`): `const TEST_ORG_SUBDOMAIN`
  **deleted**, not kept as a fallback. New `ENV_TEST_ORG_SUBDOMAIN`, required. The read splits
  into a pure `resolve_org_subdomain(raw: Option<&str>) -> Result<String, _>` (unit-testable
  without process-global env mutation) plus a thin wrapper that fails loudly with a message
  naming the variable, `layer7.sh` Phase 1h, and the manual escape hatch
  (`ENV_TEST_ORG_SUBDOMAIN=devtest cargo test -p env-tests`). Follows `cluster.rs`'s
  `read_env_url` conventions (empty ⇒ unset, eager validation, `[env-tests] VAR = … (from env)`
  provenance line) with a subdomain validator instead of a URL one.
  **@test asked me to confirm my reading: yes — making standalone `cargo test -p env-tests`
  fail loudly without the var is correct and intended.** A silent `devtest` fallback is R-7
  exactly: green on the first run of the day, 403 later.
- **TS** (`packages/web-app/e2e/env.ts`): a **new reader**, deliberately not `readUrl`'s shape
  — `env.ts:33` treats `''` exactly like unset and returns the default, so reusing it would
  silently restore `demo` on a blank var (@test, confirmed by @client). The new reader throws
  on unset **and** blank, validates against a mirrored `SUBDOMAIN_REGEX` carrying an
  `ANCHOR (DRY):` comment naming `packages/sdk-core/src/validation/limits.ts:SUBDOMAIN_REGEX`
  as SSoT (route (a); `validateSubdomain` is not exported from the sdk-core barrel and @main
  confirmed mirror-plus-ANCHOR over widening the SDK surface), and returns a `Resolved` so it
  appears in `describeEnv()` → `global-setup.ts:43-46`.

  **C1 is a correctness constraint, not tidiness, and it is why there is exactly one knob.**
  `src/lib/config.ts:56`'s `acOriginTemplate` + `sdk-core/src/http/AuthApiClient.ts:62,84`
  build the AC origin from the *form field*, so today's auth calls are same-origin only
  because `E2E_BASE_URL`'s host label and `orgSubdomain` are both `demo`. Setting the
  subdomain without moving the page host makes every auth call cross-origin with a JSON POST
  — a CORS-preflight path this suite has never exercised. So: `E2E_BASE_URL`'s default becomes
  `http://${orgSubdomain}.localhost:5173`, and if both are set explicitly, a hard equality
  check between `orgSubdomain` and the first label of the base-URL hostname throws on
  mismatch. That kills the third encoding @dry-reviewer flagged by **derivation**, rather than
  duplicating into it.

  No other client change is needed: the Vite proxy is already subdomain-agnostic
  (`changeOrigin: false` on `/api/v1/auth` forwards Host verbatim), Vite 8.2's host check
  short-circuits any `*.localhost` before `allowedHosts`, default CORS matches
  `(?:[^:]+\.)?localhost`, `src/` never reads `window.location`, and `fixtures.ts:169` already
  reads `e2eEnv.orgSubdomain`. Only stale comments change (`vite.config.ts:53-55`,
  `env.ts:64-66`, `env.ts:83-85`).
- **`layer7.sh`**: one generation site, and the exported value is **the one actually
  provisioned** — never regenerated (@client). `ENV_TEST_ORG_SUBDOMAIN` exported at Phase 1h;
  `E2E_ORG_SUBDOMAIN="$ENV_TEST_ORG_SUBDOMAIN"` added to the existing Phase-2 export block at
  `:581` beside its `E2E_AC_URL`/`VITE_*` siblings. **`E2E_BASE_URL` is deliberately NOT
  exported** — it derives, and a second export would trip the equality check on the first
  mismatch (@client).

### Test plan (`scripts/layer7.test.sh`, hermetic, already wired at `layer3.sh:34`)

Answering @test's seven items:

**Re-cut for Design B** — @test correctly flagged that this section was still the Design A
helper-verb version and contradicted §v2's own dead-token list. There is no `provision-org`
verb and no `crates/devloop-helper` diff, so every case below is pointed at the `setup.sh`
stub or the real script.

1. **Nth-run — the case that pins R-7.** The **`setup.sh` stub** (installed via the
   `DEVLOOP_SETUP_SH` seam) appends the subdomain it was passed to `$WORK/provisioned`;
   `layer7.sh` invoked **twice in one test process**; assert two non-empty lines and
   line1 ≠ line2. A case proving "an org gets created" does not pin R-7; this does.
2. **DNS-label validity**: the captured subdomain asserted against the anchored pattern — no
   uppercase, no leading/trailing hyphen, ≤63. With `e2e-<16 hex>` this is 20 chars and valid
   by construction, so the case is a construction-check, not a sanitizer-check.
3. **PRECONDITION lane**: `setup.sh` stub exits non-zero → exit 2,
   `STATUS=PRECONDITION_FAILURE`, `REASON=org-provision-failed`, stderr
   `PRECONDITION_FAILURE:` banner, **plus** the not-run proof (`! grep -q 'REASON=env-tests'`)
   and `assert_no_marker` on the suite stub. Separate cases for the four other live tokens:
   `org-provision-context-unresolved`, `org-provision-timeout`, `ac-unreachable`,
   `org-provision-unverified`. **No cases for `helper-verb-unsupported`,
   `org-provision-helper-busy`, or the `deny_unknown_fields` `invalid_request` shape** — those
   are the dead tokens from §v2 and writing cases for them would document unreachable failure
   modes.
4. **Stub-consulted**: the `setup.sh` stub drops a marker, the success case `assert_marker`s
   it, **and `layer7.test.sh:74`'s `*) exit 0 ;;` catch-all becomes a loud non-zero.** Today
   that catch-all means a misspelled or dropped verb still goes green — the structural gap the
   story names, and the commit that adds a new call path is the right place to stop widening it.
5. **End-to-end consumption**: `ENVCMD` and `BROWSER_CMD` stubs record
   `$ENV_TEST_ORG_SUBDOMAIN` / `$E2E_ORG_SUBDOMAIN`; assert each equals the subdomain the
   `setup.sh` stub was actually passed. Same-value-in-both is the assertion — one export
   reaching only the Rust suite leaves the browser suite (the one that bites at ~7 meetings/run
   vs cap 10) on the stale org. Plus the Rust unit tests on `resolve_org_subdomain` and
   `packages/web-app/tests/e2e-env.test.ts` (node tier: `test:unit`, `vitest.node.config.ts`,
   `include: tests/**/*.test.ts`).
6. **The 100 non-regression**: grounded **solely** on the `setup.sh` assertion that the
   INSERT's column list does not name `max_participants_per_meeting` (the "unrepresentable on
   the wire / `args_for_log()`" argument was Design A's and does not survive — @test), kept
   alongside the `crates/env-tests/tests/23_meeting_creation.rs:116` non-regression.
7. **Real-`setup.sh` case** — the one the layer7 tier structurally cannot give us, since
   `setup.sh` is stubbed there. Run the **real** script with PATH-stubbed
   `kubectl`/`psql`/`kind`/`docker`: exit 0; `assert_marker` the psql stub ran **exactly once**;
   **`assert_no_marker` on `kind` and `docker`** — the early-dispatch pin. Without it, a
   `--provision-org` that sets a flag without short-circuiting `main()` rebuilds the cluster
   every run *and every hermetic layer7 case still passes green*. @observability's second reason
   is the stronger one: falling through reaches `seed_test_data`/`create_ac_secrets`, putting a
   `client_secret_hash` into `psql -c` and thence into `${DEVLOOP_TMP}/layer-7*.log`, so this is
   credential containment. Mirrored by a case in `scripts/setup.test.sh` (wired at
   `layer3.sh:39`; @security confirmed `run_and_emit` emits `STATUS=FAIL` into the aggregate, so
   the `|| true` there does **not** mask a failing `setup.test.sh`).
8. **Rejection table against `setup.sh --provision-org`** (merged @security/@test corpus; not
   in `crates/devloop-helper`, which is untouched). Each row: **non-zero exit AND
   `assert_no_marker` on the psql stub** — the no-marker half is load-bearing, since an
   exit-code-only assertion passes even if the value reached psql and the *database* rejected
   it. Shell class: `x'; DROP TABLE organizations;--`, `$(id)`, backticks, embedded newline.
   psql class: `'`, `''`, `:sub`, `:'other'`, `\`. Argv class: `-x`, `--skip-build` — a
   leading-hyphen value, which `--only`'s `-z "${2:-}"` check at `setup.sh:101-113` does **not**
   catch; this is the case that stops that hole being copied. Format class: `UPPER`, `-leading`,
   `trailing-`, empty, a 64-char all-lowercase value (rejected by the regex, **not** outsourced
   to `VARCHAR(63)` truncation). Unicode: Cyrillic `а` (U+0430).
   **Run under pinned `LC_ALL=C`.** @test measured `LC_CTYPE=POSIX` here, so the Cyrillic case
   currently passes because of the ambient locale, **not the control it names** — the same
   vacuous-pass class as the `*) exit 0` catch-all. Pin it or the assertion tests nothing and
   inverts silently under a UTF-8 locale.
9. **Readback asserts on OUTPUT, not exit code.** A `SELECT` matching zero rows **exits 0**, so
   "the readback ran and exited 0" — the natural thing to write — would move the
   success-looking-no-op bug one step later instead of killing it. `psql -tAc "SELECT count(*) …"`
   against AC's exact predicate (`WHERE subdomain = … AND is_active = true`) and require
   exactly `1`. This is why fail-loud does not rest on `ON_ERROR_STOP=1` alone, whose exit
   behaviour here is `kubectl exec` propagating a *remote* code that no hermetic stub can pin.
10. **Generator**: assert the shape (`e2e-` + exactly 16 chars from `[0-9a-f]`), the
    `/dev/urandom` source, and two-draws-distinct. **No 1000-draw statistical case** — @test's
    arithmetic: at 8 hex (32 bits) P ≈ 1.2e-4, one unattributable red per ~8500 runs under
    ADR-0028's zero-retry policy. Resolved by widening to `-N8` (64 bits, bound ~2.7e-14) *and*
    dropping the statistical assertion for a structural one. The bound arithmetic goes in the
    case comment.
11. **Hermeticity + seam inertness**: no case needs a cluster. `DEVLOOP_SETUP_SH` **executes a
    script**, so it is `DEVLOOP_TEST`-gated with both halves and its inertness pinned at the
    variable level exactly as `:222-234` pins the socket seam. `DEVLOOP_ORG_PROVISION_TIMEOUT`
    is a plain budget knob, not a redirect/exec lever, so it needs no sentinel. The generated
    subdomain is **deliberately not** overridable at all — a pinned subdomain reaching
    production is cross-run org reuse, i.e. the defect itself, wearing a config knob
    (@security S11).
12. **Regex drift guard — IN THIS DIFF.** Stood down by @main, then **reinstated by @main** on a
    corrected fact, recorded here rather than quietly reversed. The stand-down reasoning was
    "with `e2e-<16 hex>` this diff introduces no new encoding of the pattern." That is false:
    **@client's `env.ts` hunk adds a mirrored `SUBDOMAIN_REGEX`** — a new encoding, added by this
    diff, which is precisely the trigger for CLAUDE.md's SSoT rule.

    @test's framing is the load-bearing one and survives independently of that fact:
    @semantic-guard traced a **present-tense** question ("is there a silent branch today") and
    answered it correctly; @test is asking about **drift under future edits**. A present-tense
    audit finding no current defect is not evidence about future edits — orthogonal claims, only
    one retired. And @client's finding *strengthens* rather than dissolves it: now that a
    malformed subdomain throws at Playwright config-load → `FAIL browser-e2e-failed` → **exit 1**,
    drift has a **demonstrated misattribution path** — loosen the DB CHECK without the `env.ts`
    mirror and you get an org the database accepts and the browser suite rejects, on the
    **implementer lane**. R-4's exact class, inside a story about that confusion. Before
    @client's finding this was hypothetical; it is not any more.

    **@test's hard condition is binding**: the guard must pin an **exact literal site count** and
    fail loudly if any single site yields zero matches. A guard that greps N files and compares
    whatever it finds passes trivially when a file is renamed and its extraction returns nothing
    — comparing four identical strings instead of five, and going green. That is the
    `layer7.test.sh:74` `*) exit 0` vacuous-pass bug reappearing *inside the guard built to
    prevent it*. Adding a sixth site must be a deliberate edit that reds until the count is
    updated.

    **Ground truth, measured by @test: FOUR sites today** — `migrations/20250118000001_initial_schema.sql:16`,
    `infra/docker/postgres/init.sql:29` (found by @test; in nobody's count), 
    `packages/sdk-core/src/validation/limits.ts:101`, and `infra/kind/scripts/setup.sh:859`
    (`subdomain_re`, preceded by `local LC_ALL=C` — the extraction must not break on that line).
    **Five** once @client's `env.ts` mirror lands. `scripts/layer7.sh` carries **none**, correctly,
    since the generator is constructive. An earlier revision of this section listed five as
    though all existed — corrected. `scripts/layer3.sh` is back on the classification table to
    wire it.

**Case comments must describe the stdin + `--set` form**, not `-c` interpolation:
@infrastructure verified on the live pod that `psql -c` performs no variable interpolation
(`psql -v sub=abc -tAc "SELECT :'sub'"` → `ERROR: syntax error at or near ":"`), so the working
shape is SQL on stdin via `kubectl exec -i` with `--set`. A case comment claiming `:'sub'`
quoting as the control under a `-c` invocation would be claiming a control that isn't there.

**Not tested, deliberately** (@security's reasoned non-requirement, relayed by @main): no
automated test that psql's quoting neutralizes a hostile value. Driving one would require
bypassing `setup.sh`'s regex and reaching a live database, so it cannot be hermetic and does not
belong in layer 3. The *enforced* control is pinned by the rejection table above; the inner
layer was verified once against the live pod by @infrastructure and preserved in a call-site
comment.

### Accumulation — decision, with the number, not a TODO

Per @paired-database's ruling and @operations' item F: **no reaper.** Per run this adds 1 org,
2–6 users, ~15 meetings, ~30 participants. A *thousand* runs is tens of thousands of rows;
every hot path is org-scoped and index-backed (`idx_organizations_subdomain` partial on
`is_active`, `idx_meetings_org_id`), so per-query cost is O(rows-per-org) and rows-per-org is
now bounded by a **single run** instead of climbing forever — the change strictly *improves*
the growth profile it introduces. The Postgres instance dies with the Kind cluster on
`teardown`. Cutting the other way, reaping means a cascading DELETE (`organizations` cascades
to users, meetings, participants, audit_logs) and a mis-scoped one deletes a live run's org —
a larger risk than the storage saved. This is recorded as a decision **in the verb's doc
comment with these numbers**, not in `docs/TODO.md`: nothing is being deferred, so a TODO
entry would be noise someone later "fixes" by adding the risky thing.

### No host-side remaining action

The first draft's biggest operational cost — "the helper serving this devloop predates the
change, so Layer 7 reds until a human rebuilds it on the host" — **is gone**. Everything this
design touches (`scripts/**`, `infra/kind/scripts/**`, `crates/env-tests/**`,
`packages/web-app/**`) is live in this tree immediately: the helper's *runtime* project-root
is `CLONE_DIR` per ADR-0030's build-context trichotomy, and Phase 1h does not go through the
helper at all. There is no rebuild, no restart, and no step that must happen outside this
container before Layer 7 can pass.

### Not doing

- No new metric, dashboard, alert or SLO (@observability explicitly).
- No migration (@paired-database — a migration here would be a signal something else is wrong;
  and @code-reviewer is right that §6.4's *criterion* makes `migrations/**` Guarded regardless
  of the enumerated `db/migrations/**` path, which is another reason the answer is zero files).
- No helper verb, no `dev-cluster` client change, no ADR-0030 amendment (@main's ruling).
- No `__dev_cluster_setup` → `__dev_cluster_write` refactor (nothing left to be busy-tolerant
  about).
- No org-provisioning HTTP API on AC (the `setup.sh:559` TODO stays a TODO; out of scope).
- No change to `seed_demo_org` / `devtest` seeding beyond the shared `dt_psql` + cap constant.

### Design A, retained (not implemented)

The zero-argument `provision-org` helper verb — `HelperCommand::ProvisionOrg` with no
`Request` field, `args_for_log() == vec![]` (so the subdomain could not reach the
`rejected_busy` / `cancel` log shapes @semantic-guard named), host-side `ring::SystemRandom`
generation, `commands.rs` delegating to `setup.sh --provision-org` exactly as `cmd_setup`
(`:706`) and `cmd_deploy` (`:1148`) already delegate, write-class through
`run_with_write_slot` so the audit entry came from the existing derivation at
`main.rs:402-418` with no new `log_command` site, plus parse-level and socket-level
injection-corpus additions per ADR-0030 §Injection regression tests.

It is recorded here rather than deleted for the same reason the story records its own
corrected premises: it was ruled out by a *deployment* fact (the helper binary is host-built
and cannot be rebuilt from this container), not by a design defect, so if the helper ever
gains a rebuild path from inside a devloop this is the shape to reach for.

---

## Pre-Work

None.

---

## Implementation Summary

Layer 7 now provisions a freshly generated organization on every run and both Phase-2 suites
consume it, so run N's verdict no longer depends on runs 1..N−1 for anything keyed by
`org_id`. Nine pieces:

1. **`layer7.sh` Phase 1h** — `__generate_org_subdomain` (`e2e-<16 hex>` from
   `od -An -tx1 -N8 /dev/urandom`, with a postcondition on the entropy read), a bounded call
   to `setup.sh --provision-org`, a fail-closed positive match on `PROVISIONED_ORG `, an AC
   `/health` gate then a 401-vs-404 org-resolution probe, and five `PRECONDITION_FAILURE`
   exit-2 tokens. `ENV_TEST_ORG_SUBDOMAIN` exported here, `E2E_ORG_SUBDOMAIN` in the Phase-2
   block. `DEVLOOP_SETUP_SH` / `DEVLOOP_ORG_PROBE` seams added, `DEVLOOP_TEST`-gated.
2. **`setup.sh`** — `dt_psql()` extracted, one `ORG_MAX_CONCURRENT_MEETINGS` constant,
   `provision_run_org()` + a `--provision-org` mode that early-returns from `main()` before
   `check_prerequisites`, and `seed_test_data`'s unreachable `else` fixed.
3. **`auth_client.rs`** — `const TEST_ORG_SUBDOMAIN = "devtest"` **deleted**. Pure
   `resolve_org_subdomain(Option<&str>) -> Result<String, OrgSubdomainError>` plus a thin
   env-reading wrapper, required with no fallback, following `cluster.rs::read_env_url`'s
   conventions. 8 unit tests. **Until this landed the whole feature was inert for the Rust
   suite** — Phase 1h provisioned an org that nothing read.
4. **`env.ts` / `fixtures.ts` / `README.md` / `vite.config.ts`** — required `E2E_ORG_SUBDOMAIN`
   throwing on unset *and* blank, `E2E_BASE_URL` **derived** from it with a host-label equality
   check, mirrored `SUBDOMAIN_REGEX`, and the stale `demo` comments corrected.
5. **`layer7.test.sh`** (+426) — 12 hermetic Phase-1h case groups: the Nth-run distinctness
   case that actually pins R-7, DNS-label validity, end-to-end consumption by BOTH suites, all
   five precondition lanes with not-run proofs, generator shape/source/distinctness, and the
   `max_participants_per_meeting` non-regression. The fake `dev-cluster`'s `*) exit 0`
   catch-all is now a **loud non-zero**.
6. **`setup.test.sh`** (+257) — the tier `layer7.test.sh` structurally cannot reach, since
   `setup.sh` is stubbed there. Real script, PATH-stubbed `kubectl`/`psql`/`kind`/`docker`/
   `podman`; early-dispatch pin (credential containment); 17-row rejection table; readback
   asserts on output.
7. **`e2e-env.test.ts`** (new) — 15 node-tier cases; the browser tier cannot test a module that
   throws at Playwright config-load.
8. **Drift guard + self-test** (new) — the org-subdomain pattern is now enforced across SEVEN
   encodings (including `docs/DATABASE_SCHEMA.md`, a live spec mirror last touched 2025-11-22),
   with the guard's own failure branches driven by a self-test wired at `layer3.sh`.
9. **Story-file premise correction** — the no-`psql`/no-`kubectl` premise recorded as verified
   FALSE, with the four `file:line` facts, corrected in place rather than deleted.

**Defect found and fixed during the resumption, outside the task list: `scripts/layer7.sh` did
not parse.** See §Issues Encountered — this is the most important thing in this diff.

---

## Files Modified

| Path | Δ | What |
|------|---|------|
| `scripts/layer7.sh` | +215 | Phase 1h, generator, 5 tokens, 2 seams, **stray-`fi` syntax fix** |
| `infra/kind/scripts/setup.sh` | +447 | `dt_psql`, cap constant, `provision_run_org`, `--provision-org` early dispatch, unreachable-`else` fix |
| `scripts/layer7.test.sh` | +560 | 16 Phase-1h case groups; loud `*)` catch-all; seam symmetry + behavioural seam inertness |
| `crates/env-tests/src/fixtures/auth_client.rs` | +258 | `resolve_org_subdomain` + 8 unit tests; const deleted |
| `scripts/setup.test.sh` | +257 | `--provision-org` group: happy path, early dispatch, rejection table, readback |
| `scripts/guards/simple/validate-subdomain-regex-sync.sh` | +193 (new) | Pattern drift guard |
| `scripts/guards/validate-subdomain-regex-sync.test.sh` | +179 (new) | Guard self-test (8 case groups) |
| `packages/web-app/tests/e2e-env.test.ts` | +152 (new) | Node-tier `E2E_ORG_SUBDOMAIN` contract |
| `packages/web-app/e2e/env.ts` | +94 | Required subdomain, derived base URL, mirrored regex |
| `packages/web-app/e2e/README.md` | +38 | `## Environment knobs` row; false claim fixed |
| `packages/web-app/vite.config.ts` | +15 | **Comment only** — `demo` is no longer the load-bearing label |
| `packages/web-app/e2e/fixtures.ts` | +11 | Load-bearing org-subdomain-fill docstring |
| `docs/user-stories/2026-08-11-story-runner-hardening.md` | +11 | *Premise corrected 2026-08-15* note |
| `scripts/layer3.sh` | +10 | Wire the drift-guard self-test |
| `docs/runbooks/devloop-validation.md` | +7 | 5 REASON rows, symptom rows, changelog |
| `docs/decisions/adr-0030-host-side-cluster-helper.md` | +2 | Helper-cannot-self-validate corollary |

Net: 13 modified + 3 new, ~1741 insertions.

---

## Devloop Verification Steps

All hermetic; **none requires a cluster**. Results are from this container at
implementation-complete.

| # | Command | Result |
|---|---------|--------|
| 1 | `bash -n scripts/layer7.sh` | **SYNTAX OK** (was a hard syntax error — see §Issues) |
| 2 | `bash scripts/layer7.test.sh` | **182 passed, 0 failed** |
| 3 | `bash scripts/setup.test.sh` | **63 passed, 0 failed** |
| 4 | `bash scripts/guards/validate-subdomain-regex-sync.test.sh` | **22 passed, 0 failed** |
| 5 | `scripts/guards/simple/validate-subdomain-regex-sync.sh` | **OK** — 7 sites in sync, 10 occurrences |
| 6 | `cargo test -p env-tests --lib` | **59 passed, 0 failed** |
| 7 | `cargo clippy -p env-tests --all-targets` | clean (0 warnings) |
| 8 | `cargo fmt -p env-tests -- --check` | clean |
| 9 | `pnpm --filter @darktower/web-app exec vitest run --config vitest.node.config.ts tests/e2e-env.test.ts` | **22 passed** (15 at this point in the run; the 7 `toLoopbackUrl` cases were added at Gate 2 — re-run at Gate 3, 22 passed / 0 failed) |

Layers run end-to-end in this container (Layer 7 deferred to Gate 2 — it is the one that needs
the live cluster):

| Layer | Result |
|-------|--------|
| 1 Compile | `OK cargo-build-passed` (+ `nx-typecheck-passed`) |
| 2 Format | `OK cargo-fmt-passed` (+ `nx-format-passed`) |
| 3 Guards | `OK guards-passed` — **all 11 sub-steps OK**, including `subdomain-regex-guard-selftest`, `layer7-selftest` (182), `setup-disk-guard-selftest` (63), `run-story-selftest` (153) |
| 4 Test | `OK cargo-test-passed` (+ `nx-test-passed`), 170s |
| 5 Lint | `OK cargo-clippy-passed` (+ `nx-lint-passed`) |

`validate-cross-boundary-scope` red once during this pass — `lead-notes.md` was touched but
unlisted — and is green after adding its classification row. That is the scope-drift guard
doing exactly its job, recorded rather than quietly fixed.

### Gate 2 — the authoritative run (resumed session, 2026-08-15)

The table above is the **implementation-complete self-check**, and its counts are superseded: Gate 3
added cases to three suites. The numbers there are left as-written rather than back-edited, because
they are what was true at that point in the run and this file's convention is to correct in place
with a date, not to rewrite history.

Two full `./scripts/layer-all.sh` runs were executed in the resumed session. **Layer 7 had never run
at all before the first of them** — the interrupted session's `gate2-verdict` artifact recorded
`GATE2=PASS LAYER_ALL_EXIT=0` with only `LAYER 1/2/3` lines present and no `layer-7*.log` on disk, so
the pipeline's own record of a pass covered three of seven layers. Treating that artifact as "Gate 2
already passed" would have shipped the entire feature unvalidated against a cluster.

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | `OK` 8s | |
| 2 Format | `OK` 2s | |
| 3 Guards | `OK` 17s | `run-guards.sh` 36/36 |
| 4 Test | `OK` 190s | `passed=3349 failed=0 ignored=38` |
| 5 Lint | `OK` 1s | |
| 6 Audit | `N/A` 1s | `no-dep-changes`; `buf-breaking-passed` |
| 7 Env-tests | **`OK` 866s** | **`env-tests-passed` AND `browser-e2e-passed`** — both Phase-2 suites, live cluster |

**The R-7 property, observed rather than asserted.** Layer 7 provisioned
`PROVISIONED_ORG org_id=e6e60539-0270-4d26-9b26-71bd90b66d67 subdomain=e2e-8f252573d7b52fe4`
with `max_concurrent_meetings=1000`, and the browser suite consumed it with
`E2E_BASE_URL=http://e2e-8f252573d7b52fe4.localhost:5173 **(default)**` — the `(default)` marker
being the direct evidence that @client's C1 derivation fired rather than an exported second knob.
Generator shape (`e2e-` + 16 hex), the cap, the export path and both consumers were confirmed in one
live run.

A **second full run** was required after Gate 3, because six reviewers made in-tree fixes and the
Gate-2 verdict binding (`scripts/lang/_gate2_binding.sh`) is a digest over the whole changeset —
`docs/devloop-outputs/**`, `docs/TODO.md`, `docs/specialist-knowledge/*/INDEX.md` and
`docs/user-stories/*.md` are binding-excluded, everything else is not. A partial re-run of "just the
affected layers" was offered and declined: the point of the binding is that the tree which passed is
the tree that ships.

Final `gate2-verdict`: `GATE2=PASS LAYER_ALL_EXIT=0`, `RUN_AT=2026-08-15T20:16:32Z`, all seven layers
recorded — `LAYER 4 OK 175 passed=3349 failed=0`, `LAYER 7 OK 939`.

### The requirement was verified by the run, not just by the tests

R-7 asks that **Layer 7's Nth consecutive run against the same dev cluster reach the same verdict as
its first, for both Phase-2 suites.** That is not something the hermetic suites can prove — they stub
the cluster. It happened for real here, as a side effect of needing a second Gate-2 run:

| Run | Org provisioned | Layer 7 |
|---|---|---|
| 1st (19:2x) | `e2e-8f252573d7b52fe4` | `OK` 866s — `env-tests-passed` + `browser-e2e-passed` |
| 2nd (20:1x) | `e2e-1498a596e754ee78` | `OK` 939s — `env-tests-passed` + `browser-e2e-passed` |

Same cluster, same database, **distinct freshly-generated orgs, identical verdict**. Before this
change the browser suite created ~7 meetings per run against the `demo` org's cap of 10, so the
second run failed partway with a 403 the pipeline attributed to the code under test. Two
back-to-back green runs on one cluster is the acceptance criterion met end-to-end, and it is worth
distinguishing from the 199-case hermetic suite: the suite pins the *mechanism*, this pins the
*outcome*.

**Manual escape hatch, unchanged and documented:** `ENV_TEST_ORG_SUBDOMAIN=devtest cargo test
-p env-tests` for a standalone Rust run against the seeded org. Standalone `cargo test -p
env-tests` with the variable unset now fails loudly, by design.

**No host-side remaining action.** No helper rebuild, no restart, nothing outside this
container.

---

## Code Review Results

Seven reviewers. **Every finding fixed; none deferred.** Grouped by what they changed, because
three of them changed the same thing and the convergence is the point.

### The probe lane — four reviewers, one bug, one ruled shape

@observability (F1), @semantic-guard (`[out-of-check: error-swallow]`), @code-reviewer (F2) and
@operations (F1) independently found that `!= 401 → org-provision-unverified` swept **`000`**
(curl transport failure / the probe's own 10s cap) and **5xx** into a token whose remediation
asserts *"this is a PROVISIONING fault, not a flake — re-running will not change it."* For a
timeout that instruction is confidently wrong and sends the operator to inspect a healthy row —
R-7's own misattribution class, produced by the check built to prevent it.

**@operations' shape governs** (they own the lane; @semantic-guard withdrew a 400 carve-out and
@code-reviewer explicitly superseded their own first proposal):

- `401` → pass · `404` → `org-provision-unverified` · **everything else** → `ac-unreachable`,
  observed code in the cause line.

`org-provision-unverified` makes a positive claim about the row's state, so an unknown code must
not default there; `ac-unreachable` claims only that the question could not be put.

**The dedicated 429 arm was deleted, not widened.** @security (F3) established — and I verified
in the AC source — that 429 is **unreachable** from this probe: `issue_user_token`'s limiter sits
inside `if let Some(ref u) = user`, and the probe's email cannot exist; the other two 429 sources
are the service-credential and registration paths; AC has no rate-limit middleware. A branch that
cannot fire is the dead-code defect this story already refused to commit for the two dead REASON
tokens — applied to my own code. The catch-all still routes a future 429 correctly, so the
forward-compatible fail-safe survives without the dead branch, and the **runbook's 429 sub-cause
went in the same edit** (a row for an unreachable mode is the drift the token recount forbids).
The false justification ("repeated failed auth is what AC's limiter counts") is corrected in
place rather than deleted, because a reader citing it would "fix" the probe by registering a
user — spending the IP-keyed budget the two suites genuinely share.

### Tests that could not fail — three mutation-proven gaps

@test-reviewer mutation-tested rather than read, and found three controls present in the code
whose absence would not have red'd the suite:

| Finding | Mutation | Before | After |
|---|---|---|---|
| F1 — `DEVLOOP_SETUP_SH`/`DEVLOOP_ORG_PROBE` inertness unpinned | honor both seams in the **production** branch | 141/0 green | 2 named failures |
| F2 — the 429 branch untested | delete the branch | 141/0 green | resolved by deleting the branch instead |
| F3 — P4 consumption self-neutralizes | delete Phase 1h entirely | 44 failures, **none** P4's | 5 failures, led by an explicit non-vacuity assertion |

F3 is the sharpest: `mapfile` failed on the absent file, `consumed` became `""`, and
`assert_status` is a **substring** check — so the empty needle matched everything and the case
whose entire job is "both suites received the value" reported success when neither did.
`resolve_socket_var` was generalised to `resolve_seam_var`; I re-ran both mutations to confirm
they now red.

### Claims that were not true — including two of my own

- **@operations F3 — my `ugrep` diagnosis was wrong.** See §Issues #2. Corrected in `docs/TODO.md`
  (now CLOSED, sweep done, zero other sites), in the guard comment, and here.
- **@code-reviewer N3 — my `LC_ALL=C` comment claimed a measurement that does not reproduce.** I
  re-measured: `subdomain_re` gives **identical** verdicts on `аbc`/`Ä`/`ábc`/`ABC`/`aBc`/`abc`
  with and without the pin, and this image ships only `C`/`C.utf8`/`POSIX`, so the locales where
  range-vs-collation bites are not installed. The pin is now described as cheap insurance rather
  than as a demonstrated control, and @test-reviewer's static assertion is what protects it.

### Controls asserted by proxy — @security F2

Credential containment was pinned by `assert_no_marker` on `kind`/`docker`/`podman`, which only
detects a fall-through *via `check_prerequisites`*. Four `assert_absent` lines now name
`client_secret_hash`, `service_credentials`, `AC_MASTER_KEY` and `MH_CLIENT_SECRET` directly over
everything the mode emits or executes — the property in the language of the thing protected, so
it survives a refactor that renames the proxy.

### Also fixed

@operations F4 (the repo's only `bash -n` sweep widened from `guards/simple` to all of
`scripts/` + `infra/` — 117 files, count-free, with a zero-match vacuity trap; this is the class
§Issues #1 belongs to) · F5 (the one load-bearing `|| true` now carries the comment that stops it
being deleted) · @security F1 (`local LC_ALL=C` in the generator + a static pin) ·
@code-reviewer F1 (`env -u DEVLOOP_TEST` — the guard self-test inverted under an inherited
sentinel, the one case whose subject is inertness being the one whose own inputs were inherited)
· F3 (`DT_ORG_MAX_CONCURRENT_MEETINGS` knob restored, which makes its validator reachable again —
it guards the one site that string-interpolates the value into SQL text) · N1, N2 ·
@dry-reviewer #1 (`DATABASE_SCHEMA.md` enumerated; 7 sites / 10 occurrences) · #2 (TODO #29
counts updated, line numbers dropped as inherently stale, defer trigger recorded as **fired**).

**@semantic-guard verdict: CLEAR** on all five enumerated checks, with the error-swallow lane
ruled by @operations as above.

---

## Gate 3 — Final Approval (2026-08-15, resumed session)

The interrupted session collected findings but only ever recorded **one** verdict
(@semantic-guard's CLEAR). The Lead re-convened the full roster against the **final** diff
rather than deriving verdicts from the written record — a verdict no reviewer gave is not a
verdict, and inferring nine of them from a summary would have been the same
success-looking-no-op this diff spent three findings eliminating.

Each reviewer was asked to do two things: verify their own prior findings' fixes **in the code**
(not from this document), and review the final diff fresh. Every one of them checked rather than
trusted, and the round found **15 new findings** — including three tests that could not fail, one
live regression that made the feature non-functional, and one silently-skipped validation gate.

| Reviewer | Verdict | New findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 | F4: the 2xx auth-bypass signal was wrapped in `ac-unreachable`'s "AC simply could not be asked" cause line and a "re-running is reasonable" remediation — the wrapper arguing against its own detail line on the highest-severity signal the probe can produce. Declined a cost-based deferral on the grounds that a tracker entry costs the identical tree edit and Gate-2 re-run as the fix. Fixed by @operations as lane owner, in a **stronger shape than proposed** (below). F5: residue of that fix — see §The prose-shaped residue. |
| Test | RESOLVED-FIXED | 4 | 4 | 0 | Mutation-tested rather than read. Three of the four were **tests that could not fail** (details below). Also caught the live `KUBE_CONTEXT` regression. |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | All four were runbook deliverables the plan's own row listed and the implementation dropped — planned scope, not new scope. §8 symptom catalogue had **no entry for any of the five tokens**; the existing `cluster-*` glob matches none of them, so the operator's documented first stop landed nowhere. |
| Code Quality | RESOLVED-FIXED | 1 | 1 | 0 | N4: the comment claiming "the fail-safe survives WITHOUT a dead branch" sat twenty lines above a `429)` arm that cannot fire — a comment naming a property the code lacks, in the file whose thesis is that stale comments get cited as evidence. Declined to edit an owner's file; routed. |
| DRY | RESOLVED-DEFERRED | 2 | 2 | 1 obs. | Both fixed in-tree. Verdict is DEFERRED per the protocol solely because a tech-debt observation was appended to `docs/TODO.md`: the repo has **two** DNS-label rule families and this diff guarded one. |
| Operations | RESOLVED-DEFERRED | 2 | 2 | 2 | Owns the operator lane. Overruled their own O1 fix as insufficient when shipping @security's F4 (below). Both pre-existing deferrals re-confirmed. |
| Semantic Guard | CLEAR | 0 | — | — | Re-ran all five enumerated checks against the changed diff. One `[out-of-check: error-swallow]` judgment: the probe is `curl -s` without `-S`, so the lane is right but the operator's evidence is discarded. Per the standing Lead ruling, this CLEAR does not clear the shell error-swallow risk. |
| Infrastructure (owner) | RESOLVED-DEFERRED | 7 | 6 | 1 | Confirmed all five owned `setup.sh` hunks + the ADR corollary. Their #1 was a **pipefail/SIGPIPE false negative in their own new context guard** — measured against a synthetic 3000-context kubeconfig. Their #6 is the one I'd most want carried forward: they found their *own* fix's comment overstated what they had measured, and corrected it. #7 is deferred and repo-wide (below). |
| Client (paired, owner) | RESOLVED-FIXED | 2 | 2 | 0 | Confirmed the client hunks **including `toLoopbackUrl`, which they did not author** — added at Gate 2 and reviewed as owner here. |
| Database (paired) | RESOLVED-FIXED | 2 | 2 | 0 | Confirmed **no migration required** and the `max_participants_per_meeting = 100` non-regression, both by reading the schema rather than restating the ruling. |

### The three tests that could not fail

@test's findings are the most consequential of the round, because each was a control that was
*present in the code* and *asserted by a test* that would not have red'd if the control were
deleted:

1. **`org-insert-omits-max-participants` was a tautology.** It matched the exact column list, then
   asserted the forbidden column was absent *from what it had matched* — so adding the column
   removes the line from the match set instead of exposing it. Doubly blind: the pattern also
   matched `seed_test_data`'s byte-identical list, so whichever site was mutated the other stayed
   as a clean witness. Mutating either alone: 182/0.
2. **`org-generator-locale-pin` asserted on a comment.** The `grep -A6` window ended before the
   real `local LC_ALL=C` statement, and the function's own docstring *mentioning* the pin
   satisfied the count. Deleting the actual control left the suite at 182/0 — @security's F1
   control was unprotected by the case that exists to protect it.
3. **The context-resolution branch had zero coverage** (raised by @infrastructure, mutation-
   confirmed by @test). The stub was already parameterised with `${STUB_CONTEXTS:-…}` and nothing
   ever overrode it — a seam built for a case never written. Without the check a cross-cluster
   write *succeeds*, Phase 1h reports `PROVISIONED_ORG`, and both suites fail two phases later at
   AC token acquisition: R-7's own lane confusion, produced by the guard meant to prevent it.

A fourth is the sharpest process point: **@security's 2xx lane split silently dropped a property
with no test deleted and nothing turning red.** Once 200 got its own token, every remaining probe
case matched a *named* arm, so deleting the `*)` catch-all would have red nothing. Fixed with a
302 case.

### The live regression, and why it was caught

Mid-round, a concurrent edit left `setup.sh` referencing an **unassigned `KUBE_CONTEXT`**. Under
`set -u` every `--provision-org` invocation died before the context check, which would have made
the entire R-7 change non-functional on every run — `org-provision-failed` forever. `setup.test.sh`
went 54/9 immediately. @test reported it rather than editing an owner's file mid-write, and
@infrastructure fixed it at `setup.sh:104-105` by deriving `KUBE_CONTEXT` once and building
`KUBECTL` from it — which also closed their own finding 2 (the `kind-${CLUSTER_NAME}` double
encoding). Verified by the Lead in the final tree: assigned at `:104`, `bash -n` clean,
`setup.test.sh` 68/0.

This is the same class as §Issues #1 (the stray `fi`), and it was caught the same way: by running
the suite, not by reading the diff.

### The prose-shaped residue (@security F5) — and why the Lead's verification missed it

Shipping F4 left a one-string contradiction one lane over. `ac-unreachable`'s remediation still
read "…re-running Layer 7 is a reasonable action here — **WITH ONE EXCEPTION**: if the cause line
above reports a 2xx…", while the comment eleven lines above it stated the opposite and was correct:
"No 2xx arm here: 2xx is claimed by its own lane above and can never reach this case." The clause
directed an operator to check for a condition the code guarantees cannot appear — the same
dead-guidance class this story invoked to refuse `helper-verb-unsupported`, applied one string over.
@operations had already reverted it; a concurrent write to `layer7.sh` restored it, exactly as their
`2??` deletion had to be reapplied. So the Lead's fix restored the owner's stated intent rather than
overriding it, which is why it did not need a third round-trip.

**The instructive part is why the Lead's check passed it.** @operations asked the Lead to verify
`grep -c '2??' scripts/layer7.sh` → 1, which it was. That count was *correct and the residue was
still there*, because the leftover was spelled `2xx` in an English sentence, not as a glob pattern.
**A pattern-shaped check cannot see prose-shaped residue** — and this diff's own thesis is that
comments get cited as evidence, so a stale remediation string is not a lesser defect than a stale
branch. The generalisable rule: when deleting a behaviour, grep for the *concept* in prose as well
as the *token* in code.

@security then made the sharper point about the Lead's follow-up check (`grep -c '2xx'` → 8): **the
count is not the property.** Eight is only reassuring because the sites were inspected; a remembered
number does not survive concurrent writes. The durable form is the invariant — 2xx routing must be
asserted in exactly **one** place (`scripts/layer7.sh:800`), every other mention being documentation
of that lane rather than a competing route — and `layer7.test.sh` could pin it the way it already
pins the `LC_ALL=C` locale pin and seam inertness. **Deliberately not added in this diff** (the tree
is correct, and @security explicitly did not raise it as a finding), and deliberately not filed as a
deferral either, since nothing is being carried: it is a note for whoever next touches the Phase-1h
lane table. Recorded here because that is where they will look.

The security-relevant shape of the concurrent write, in @security's framing: a silent revert restored
a **documentation** string while leaving the **code** correct — the one direction pattern guards do
not cover, where the code keeps working and only the operator's instructions lie.

### Concurrent-edit hazard — recorded because it nearly cost something

Six reviewers held write access to overlapping files during this round. Two collisions actually
occurred: the `KUBE_CONTEXT` transient above, and @operations' deletion of the dead `2??`
sub-cause arm being reverted by another agent's write, requiring reapplication. @operations asked
the Lead to verify the final tree carries exactly one `2??`; it does (`grep -c` → 1, the lane
guard at `scripts/layer7.sh:801`). The Lead closed Gate-3 edits before the final pipeline run
rather than letting reviewers and validation race. **For future resumed devloops: serialize
Gate-3 in-tree edits, or scope each reviewer to files they own.**

---

## Accepted Deferrals

Two, both deferred by Lead ruling with the justification recorded in `docs/TODO.md` (§Infrastructure Validation in Devloops), both owner `operations`, both surfaced by this task's own Gate-2 run rather than by inspection. Neither is a fix parked in a tracker to avoid doing it — each is new work on a **pre-existing** path this diff does not touch, which is the line CLAUDE.md's "fix, don't defer" draws. Bodies live in the tracker, not here.

- **Teardown busy-tolerance asymmetry in `layer7.sh`** → `docs/TODO.md`. `__dev_cluster_setup()` retries on `(busy)`; the rebuild branch's `teardown` call does not, so a benign helper-mutex collision reports as `cluster-rebuild-failed` and sends the operator to inspect a healthy cluster (R-4's misattribution class). Reachability is measured, not theoretical — Gate 2 produced a live `rejected_busy` record. Deferred: the line is byte-identical to its pre-task version, and busy-tolerance on the write path is a behaviour change to the operator-lane boundary in an @operations-owned Domain-judgment file.

- **The Gate-1 ruling that stood that work down is corrected in place** in the tracker entry: its premise ("with no socket call there is no write slot to contend") is true of Phase 1h and false of the teardown path, which was observed contending. The conclusion survives on the two grounds above; the reasoning does not.

- **The repo's only `bash -n` sweep runs only by hand** → `docs/TODO.md`. `_get_base_ref.behavior-equivalence.test.sh` case 7a (widened here from ~30 files to **117**) is referenced by nothing — not `layer3.sh`, not any workflow, not `run-guards.sh`'s glob. It is the designed guard for the class of §Issues #1, which was caught only incidentally.

- **Deliberately not wired in this diff**: that file carries 2 pre-existing failures (confirmed independently by @test and by me to be identical with and without my edit), and wiring a red gate trains people to ignore Layer 3 — "fail loudly, never mask" violated in the other direction.

### Added at Gate 3 (resumed session)

Two more, both new work on **pre-existing** paths this diff does not touch, both bodied in `docs/TODO.md` rather than fixed blind. **This subsection is deliberately pointer-only**: the Lead's first draft inlined both bodies here and `validate-todo-tracking` red'd it (`[inline_debt_body]`, blocking Layer 3) — recorded rather than quietly reformatted, because the guard caught the Lead committing the exact duplication-of-record the section exists to prevent.

- **`producer | early-exit-consumer` is a repo-wide silent-skip class** (@infrastructure #7, @test concurring) → `docs/TODO.md` §Infrastructure Validation in Devloops. **Highest-priority item out of this devloop**: on a large Rust changeset, compile/fmt/clippy/test **and** the Rust dependency audit skip silently with the pipeline green. Owner `operations` (lead) + `infrastructure` (siblings).
- **Two DNS-label rule families, one guarded** (@dry-reviewer) → `docs/TODO.md`. Family B (slug / Kind cluster name) has 4 unguarded sites, one already divergent with no length bound. Must not be merged with Family A.

**Why the first of those outranks the other tracker items**: it is a *masked* gate, and CLAUDE.md ranks masking above severity — a red gate gets fixed, a silent one does not get noticed. It is also the same mechanism as @security's Gate-1 SIGPIPE generator trap (`tr -dc … | head -c N` → rc 141): that instance was fixed, the class was never swept, and this is the sweep being scheduled rather than assumed.

**A measurement discipline came out of this and belongs in the record.** On this one tracker entry, three separate threshold claims were made and *all three* were wrong in the same way — @infrastructure's "stops firing at ~80+ files", @test's "deterministic at ~68 KB", and @test's "no arrangement holds" were each a distribution flattened into a step, caught only by re-running at more than one sample per point (30 runs: N=80 → 0/30, N=200 → 8/30, N=300 → 27/30). A "safe under N" framing would have been cited later to reintroduce the pipe. Two consequences were folded in before commit: the prescribed regression test changed from a count-based repro (still flaky ~1 run in 30 at N=400, under ADR-0028's zero-retry policy) to a deterministic blocking producer verified 30/30; and @infrastructure found the **same error inside their own shipped code** — `setup.sh:906`'s comment asserted a clean threshold from one sample per point — and corrected the comment rather than only the tracker.

**Closed at Gate 3, not deferred**: @infrastructure's finding 5 (context guard mutation-untested) was routed to @test, who landed the B7 case — so the tracking entry @infrastructure had filed was **removed**. A tracker entry for a finding that did not survive into the diff is drift in the same way a missing entry is.

**For the record**: all 14 findings from the first review round, and 14 of the 15 new Gate-3 findings, were **fixed in this diff** — the sole exception is @infrastructure's repo-wide `producer | early-exit-consumer` class above. Nothing was deferred to avoid doing it; every deferral is out-of-changeset work on a pre-existing path, with a named owner and a measured reproduction.

---

## Rollback Procedure

1. Verify start commit from Loop Metadata: `9fa3bc6d962f396c7e1b4eb0dd280237e48a0961`
2. Review all changes: `git diff 9fa3bc6..HEAD`
3. Soft reset (preserves changes): `git reset --soft 9fa3bc6`
4. Hard reset (clean revert): `git reset --hard 9fa3bc6`
5. If a helper binary was rebuilt on the host, the host-side helper must be restarted to
   pick up the reverted build.

---

## Issues Encountered & Resolutions

### 1. `scripts/layer7.sh` did not parse — every Layer-7 run died with no lane

The single most important finding of this diff, and it was not on any task list.

At the start of the resumption, `bash -n scripts/layer7.sh` failed:

```
scripts/layer7.sh: line 680: syntax error near unexpected token `fi'
```

**Cause.** The Phase-1h AC-URL check at `:639` was inverted during review from an
`if [[ -n "$ENV_TEST_AC_URL" ]]; then … fi` wrapper (skip verification when absent) to a
fail-closed `if [[ -z … ]]; then precondition_fail; fi` guard. The body was correctly
un-wrapped and re-commented "Verification runs UNCONDITIONALLY from here" — but the wrapper's
closing `fi` was left behind.

**Severity.** Worse than any lane bug. Bash rejects the whole file before line 1 executes, so
Layer 7 exited **from the shell itself**: no `STATUS=` line, no `REASON=` token, no lane, and
`layer-all` records `UNKNOWN`. A change made to guarantee an operator-lane failure instead
removed the operator lane entirely — **Layer 7 failing in precisely the unattributable shape
this whole story exists to eliminate.**

**Scope — confirmed introduced by this devloop, not inherited.** Checked both ends
independently (@main verified the same):

```
bash -n scripts/layer7.sh                        →  syntax error (working tree, pre-fix)
git show HEAD:scripts/layer7.sh | bash -n        →  parses OK   (HEAD = 9fa3bc6, adr_0035)
```

So nothing on the branch was ever shipping broken, and the blast radius was this uncommitted
hunk alone. That is the good version of the news, and it makes the near-miss *sharper* rather
than softer: the hunk had already been through reviewer eyes and was sitting in the tree as
"landed" while `bash -n` failed on it. The defect was one command away from detection for the
entire time it existed.

**Resolution.** Removed the orphan `fi`, de-indented the block, and left a comment at the site
naming the class and the `bash -n` symptom, so the next person inverting a guard there knows
what to check.

**Why it survived review.** Nothing had executed `layer7.sh` since the hunk landed — the
reviewers read the diff. `layer7.test.sh` runs the real script for every flow case and caught
it on the first invocation: the suite went **44 passed / 27 failed** before the fix and
**141 passed / 0 failed** after (27 of those failures were the syntax error; the rest were the
new Phase-1h fixtures the suite had not yet grown). A test file that had been written but not
run is not a test.

### 2. `grep`: flags after a `--` terminator are parsed as operands — and my first diagnosis of this was itself wrong

**The fix was right; the stated reason was not, twice over.** Recorded in full because the
second error is more instructive than the bug.

**What actually happens.** `--` is grep's end-of-options terminator: after it, every remaining
argument is an operand by definition. So `grep -rn -F -- "$PAT" "$ROOT" --exclude-dir=docs`
parses the exclusion as a *path to search*. Measured with `/usr/bin/grep` on a synthetic tree:

```
grep -rn -F     NEEDLE /tmp/pt --exclude-dir=docs   -> exclusion honored,  rc 0
grep -rn -F --  NEEDLE /tmp/pt --exclude-dir=docs   -> exclusion IGNORED,  rc 2
grep -rn --exclude-dir=docs -F -- NEEDLE /tmp/pt    -> exclusion honored,  rc 0
```

The rule: put every flag **before** the `--`, and reserve `--` for when the pattern may itself
begin with `-`. The drift guard now does that.

**PREMISE CORRECTED 2026-08-15 (@operations), same day.** My first write-up of this claimed
`grep` on this image is **ugrep 7.5.0**, that ugrep — unlike GNU grep — does not permute
options after operands, that the failure is **silent with exit status unchanged**, and that the
blast radius was "every post-operand `--exclude*` in the repo", filed **NOT SWEPT** with an
instruction to treat prior clean checks as unverified. **All four were false.** I verified each
correction directly rather than accepting the report:

- `/usr/bin/grep` is **GNU grep 3.8**, and that is what every guard, layer script and CI job
  runs. `grep --version` reports ugrep **only inside a Claude Code agent's own Bash-tool
  shell**, where the harness defines `grep` as a shell *function* (`type grep` → "grep is a
  function") shimming the `claude` binary. That function is not exported and does not exist
  under `bash script.sh` — confirmed by writing a script file and running it.
- **Both greps permute options after operands identically.** The `--` terminator was the real
  mechanism all along; it merely happened to be present in my reproduction command.
- It is **not silent**: `grep: --exclude-dir=docs: No such file or directory`, exit **2**.
- The sweep is one command and I ran it: **zero** other `grep --exclude*`/`--include*` sites in
  the repo. The only other `--exclude` hits are `git ls-files --exclude-standard`, a git flag.
  Closed, not open, and never `infrastructure`'s to own.

**The generalisable lesson, which is why this is kept rather than deleted: a measurement of the
environment taken from inside an agent's tool shell is not a measurement of the environment
scripts run in.** Verify with `bash -c` or a script file. I had a *correct* reproduction of a
*real* symptom and drew a confident, wrong conclusion about its cause — then recorded that
conclusion in the tracker as a fail-open defect in validation tooling, with an owner and an
explicit "prior clean checks are unverified" instruction. That entry would have sent someone
hunting nothing, or "fixing" working call sites. Corrected in place in `docs/TODO.md`, in the
guard's own comment (the site a future scanner author copies from), and here.

### 3. Corrections to the test plan, made rather than worked around

- **Item 7 says "assert the psql stub ran exactly once".** The shipped `provision_run_org`
  issues **two** statements — the INSERT and the readback item 9 requires — so "exactly once"
  would red against correct code. Asserted as exactly **two**, with one INSERT and one
  readback identified by their SQL. Relaxing it to `>= 1` was the other option and was
  rejected: it would stop distinguishing "the readback ran" from "the readback was dropped",
  which is the whole point of item 9.
- **Item 9 was in the test plan but in nobody's task list** (B covers 1-6 and 10, C covers 7-8).
  Implemented in `setup.test.sh` (B2) rather than left to fall through the gap: the stub
  returns `count=0` with exit 0 and provisioning must still fail.
- **The Rust validator adds a sixth encoding of the pattern.** main.md §R6 says "explicitly do
  not add a sixth *style*". Resolved by adding the same literal in the same style — not a new
  hand-rolled predicate — and enumerating it in the drift guard, so the copy is *checked*
  rather than trusted. The guard's counts were re-derived by measurement (6 enumerated sites,
  9 total occurrences), never adjusted until green.

### 4. Two clippy warnings introduced and fixed before hand-off

`regex::Regex::new` is on the workspace `disallowed-methods` list; switched to the
`LazyLock<Regex>` canonical-home shape this crate already uses in `gc_client.rs`, with the same
`#[expect(...)]` reason string. `redundant_guards` on `Some(v) if v.is_empty()` → `Some("")`.

---

## Lessons Learned

**1. A written test that has never been executed is not a test.** The two worst defects in this
devloop were caught by *running* things, not reading them. §Issues #1 — `layer7.sh` did not parse at
all, so Layer 7 would have died from the shell itself with no `STATUS=`, no `REASON=`, no lane — sat
in the tree as "landed" through reviewer eyes, one `bash -n` away from detection the whole time. The
`KUBE_CONTEXT` regression at Gate 3 was the same shape. Both were found on first invocation of a
suite that already existed.

**2. Verify the premise before designing to it.** The task brief mandated a helper verb on the
grounds that Phase 1 has neither `psql` nor `kubectl`. Both are present, and the helper is built from
the *host* checkout so a verb added on this branch could not exist in the helper serving this
devloop — the mandated design could not have passed its own gate. That was surfaced at Gate 1, where
a plan change is cheap, because the implementer classified the cross-boundary rows honestly instead
of downgrading them to make the routing easier.

**3. Tests that cannot fail are the dominant failure mode here, and only mutation finds them.**
Across two rounds @test found six: a tautological assertion that checked a forbidden column was
absent from a pattern that had already excluded it; a locale-pin case satisfied by a *comment*
mentioning the pin; a consumption case where `mapfile` on a missing file made `assert_status`'s
substring check match the empty string, so "both suites received the value" passed when neither did;
and a lane-default pin silently voided by someone else's correct fix. Every one of them was green.
Reading the test would not have found any of them.

**4. Fixing a lane means fixing the token, the cause line, and the remediation — not one of them.**
Four reviewers independently found the probe sweeping curl transport failures into a token whose
remediation asserts "re-running will not change it." @operations then overruled their *own* first
fix on the grounds that scoping the remediation left the token name and cause line still lying. The
residue after that (§The prose-shaped residue) is the same lesson a third time: a `grep` for the
code-shaped token cannot see the concept spelled out in English one branch over.

**5. Guard the mechanism, not the instance.** @security's Gate-1 SIGPIPE finding
(`tr -dc … | head -c N` → rc 141) was fixed as an instance; the *class* was never swept. It came
back at Gate 3 as a repo-wide `producer | early-exit-consumer` defect that silently skips the entire
Rust layer — compile, fmt, clippy, test **and** the dependency audit — on large Rust changesets,
with the pipeline green. Self-reinforcing, since only a big change triggers it. That is now the
highest-priority tracker item out of this devloop.

**6. Distributions get flattened into thresholds, repeatedly, by people who know better.** On one
tracker entry three separate "fails above N" claims were made and all three were wrong — including
one written *after* its author had authored the correction to the previous one. Only re-running at
more than one sample per point caught any of them. @infrastructure found the same error inside their
own shipped comment and corrected it. "Safe under N" is the framing that gets cited later to
reintroduce the bug.

**7. Resumed devloops need their state re-derived, not trusted.** main.md's Loop State said
`implementation` while §Code Review Results was fully populated, and the pipeline's own
`gate2-verdict` artifact said `GATE2=PASS LAYER_ALL_EXIT=0` with only three of seven layers recorded
and no `layer-7*.log` on disk. Believing either would have shipped the feature with Layer 7 never
having run once. **Process change for next time**: serialize Gate-3 in-tree edits, or scope each
reviewer to files they own — six concurrent writers cost one reverted fix and one transient that
broke the feature outright.

**8. Reviewers who edit are worth more than reviewers who report, when they own the file.** Nine of
the fifteen Gate-3 findings were fixed by the reviewer who raised them, in files they owned, while
the implementer was no longer live. The two that were routed instead (@code-reviewer declining to
edit @operations' file; @test declining to edit @infrastructure's mid-write) were both correct calls
— and the one time that discipline lapsed, a concurrent write reverted somebody's fix.
