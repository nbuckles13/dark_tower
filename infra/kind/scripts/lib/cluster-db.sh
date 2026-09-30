# shellcheck shell=bash
#
# Cluster-database helpers shared by deploy.sh (seeds) and setup.sh
# (--provision-org). NOT part of the provision blueprint (ADR-0038 step 3): an
# edit here must never rebuild a cluster. Requires KUBECTL (lib/common.sh:
# dt_init_cluster_env) before dt_psql is called.

# --- Cluster Postgres access (single source of truth) ------------------------
# EVERY psql call in these scripts goes through dt_psql(). The connection identity
# is stated once here; it was copy-pasted across three call sites before, and a
# fourth copy in scripts/layer7.sh was the alternative this function replaced.
#
# These are set unconditionally at file scope, NOT inside a function: the
# --provision-org path early-returns from main() and must not depend on anything
# the full-setup flow initialises on its way past.
#
# WHY NOT "$DATABASE_URL": inside the devloop container that variable points at
# the per-devloop UNIT-TEST database (…-db:5432/dark_tower_test), which AC and GC
# never read. That database has its own organizations table, so the wrong-target
# write SUCCEEDS silently rather than erroring (verified: `psql "$DATABASE_URL"
# -tAc "SELECT to_regclass('public.organizations')"` returns a non-null
# regclass). An org inserted there leaves R-7 looking fixed in Phase 1 and broken
# in Phase 2, surfacing as GC's org_not_provisioned discriminant — a message that
# points at provisioning which demonstrably ran.
#
# AUTH SHAPE is deliberate: in-pod `-U <user> -d <db>`, never a
# postgresql://user:pass@host URI. A URI would land in `ps` output and, when this
# script runs under Layer 7, in ${DEVLOOP_TMP}/layer-7-*.log.
DT_PG_NAMESPACE="dark-tower"
DT_PG_POD="postgres-0"
# The pod has an init container; naming the target container suppresses kubectl's
# `Defaulted container "postgres" out of: postgres, init-permissions (init)`
# banner, which would otherwise interleave with captured output.
DT_PG_CONTAINER="postgres"
# Derived from the credential's one owner (lib/common.sh:pg_secret_value); the
# sourcing script defines PROJECT_ROOT and sources lib/common.sh first.
DT_PG_USER="$(pg_secret_value POSTGRES_USER "${PROJECT_ROOT}/infra/services/postgres/secret.yaml")"
DT_PG_DB="$(pg_secret_value POSTGRES_DB "${PROJECT_ROOT}/infra/services/postgres/secret.yaml")"

# Run psql inside the cluster's Postgres pod.
#
#   dt_psql [--stdin] <psql args...>
#
# --stdin adds `kubectl exec -i` and is REQUIRED when the SQL arrives on stdin
# (without it psql reads an empty stdin and exits 0 — a silent no-op). It is
# opt-in rather than always-on because the `-c` call sites never read stdin, and
# forwarding an open non-TTY stdin to `kubectl exec` on those calls risks a hang.
# Never `-it`: a TTY performs CRLF translation and can corrupt SQL on stdin.
dt_psql() {
    local stdin_flag=()
    if [[ "${1:-}" == "--stdin" ]]; then
        stdin_flag=(-i)
        shift
    fi
    # NB two different `-c` flags on this line: the one before `--` is kubectl's
    # --container; any `-c` in "$@" after `--` is psql's --command. The `--` is
    # what keeps them unambiguous.
    ${KUBECTL} exec "${stdin_flag[@]}" -n "${DT_PG_NAMESPACE}" -c "${DT_PG_CONTAINER}" \
        "${DT_PG_POD}" -- psql -U "${DT_PG_USER}" -d "${DT_PG_DB}" "$@"
}

# Concurrent-meeting cap for organizations the TEST SUITES drive.
#
# CARVE-OUT — this applies to seed_test_data()'s `devtest` org and to
# provision_run_org() ONLY. It deliberately does NOT apply to seed_demo_org(),
# which takes the schema default of 10 because it is the user-facing browser
# sign-up org and the default is the right profile for it (see the rationale
# above seed_demo_org). A reader seeing two of three call sites use this constant
# is looking at a deliberate exclusion, not an unfinished refactor.
#
# The schema default of 10 is the R-7 bug: no production code marks a meeting
# ROW ended (MC's meeting-end notify, story 2 task 12, ends only GC's
# MC-assignment row, never `meetings.status`, which is what the cap counts),
# so an org's live-meeting count only climbs, and the browser suite's ~7
# meetings/run exhausts a cap of 10 on the second run against the same cluster.
# Config over hardcoding (CLAUDE.md), and the knob is what makes the validator below a LIVE
# check rather than a vacuous one (@code-reviewer F3): with a bare literal, `1000` can never
# fail `^[1-9][0-9]{0,8}$`, so the validation would describe a control with no reachable input.
# `readonly` is deliberately omitted — no other constant in this script uses it.
ORG_MAX_CONCURRENT_MEETINGS="${DT_ORG_MAX_CONCURRENT_MEETINGS:-1000}"

# Validate the constant at the boundary that forms SQL statements from it, so a
# bad DT_ORG_MAX_CONCURRENT_MEETINGS dies at script start with a clear message
# rather than as a psql cast error mid-provision. This is not belt-and-braces:
# seed_test_data interpolates this value directly into SQL TEXT (`psql -c "…
# max_concurrent_meetings = ${ORG_MAX_CONCURRENT_MEETINGS};"`), which is the one
# site where it does NOT go through --set + (:'max_meetings')::int — so this
# regex is the only thing standing between an env override and a SQL injection
# point. The `(:'var')::int` quoting in those statements is the
# structural backstop; this is the fail-fast, exactly as the subdomain regex is
# the fail-fast in front of the schema's own CHECK.
if [[ ! "${ORG_MAX_CONCURRENT_MEETINGS}" =~ ^[1-9][0-9]{0,8}$ ]]; then
    echo "ERROR: ORG_MAX_CONCURRENT_MEETINGS must be a positive integer with no leading zero, got '${ORG_MAX_CONCURRENT_MEETINGS}'" >&2
    exit 1
fi
