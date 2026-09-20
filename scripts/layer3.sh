#!/usr/bin/env bash
# Layer 3 — Guards (always-run: no skip-if-untouched).
#
# Wraps existing scripts/guards/run-guards.sh with the STATUS contract.
#
# Failure triage: docs/runbooks/devloop-validation.md §6.3 (Layer 3 + §4 two-token convention).
set -euo pipefail
IFS=$'\n\t'
__here="$(dirname "$0")"
source "${__here}/lang/_common.sh"

# CI-SENTINEL-LEAK runtime assertion (task #47, §J/C — security trust boundary).
# Shared single-locus check in _common.sh; also called from layer-all.sh. Fires here so
# it ALSO catches a DEVLOOP_TEST leak when layer3 runs standalone (not via layer-all).
assert_no_ci_sentinel_leak

layer_lifecycle_begin 3
"${__here}/lang/_get_base_ref.sh" >/dev/null

# Use run_and_emit (dry-reviewer F2) instead of inline if/else — single source
# of truth for STATUS line emission + exit-code mapping.
# The audit-suppressions guard is auto-discovered by run-guards.sh (simple/*.sh); its
# SELF-TEST is wired explicitly here (task #47) — there is no *.test.sh auto-runner, so an
# unrun test is untested. This drives the check's FAIL branches (past-due/drift/malformed/
# sentinel) every devloop + CI.
#
# NOT EVERY *.test.sh IS WIRED HERE, and the exception is deliberate.
# `scripts/release-feature-gate.test.sh` (ADR-0036 §11's release-build compile gate) is
# wired in `scripts/lang/rust/compile.sh` — Layer 1 — because it INVOKES CARGO, and
# ADR-0033 §3/§4 put variable compiler cost outside the layers-3+6 fast-tier budget. Every
# self-test below is hermetic: no cluster, no network, no cargo. Layer 3 invoking cargo
# zero times is structural — Layer 1 produces target/release/{dt-guard,dt-story}, Layer 3
# consumes them — so do not "consolidate" that one here to match the pattern. The reasoning
# lives once, at its invocation in compile.sh.
{
  run_and_emit "guards" "${__here}/guards/run-guards.sh" || true
  run_and_emit "changed-helpers-test" "${__here}/lang/_changed_helpers.test.sh" || true
  run_and_emit "audit-suppressions-selftest" "${__here}/audit-suppressions-check.test.sh" || true
  run_and_emit "audit-gate-test" "${__here}/lang/_audit_gate.test.sh" || true
  run_and_emit "layer7-selftest" "${__here}/layer7.test.sh" || true
  # Org-subdomain drift guard SELF-TEST (story R-7, task #3). The guard itself is
  # auto-discovered by run-guards.sh above and runs on every devloop — but a PASSING guard
  # exercises none of its failure branches, and those branches are its entire value: a check
  # that greps, finds nothing, compares nothing and reports success is the exact bug it was
  # written to prevent. This drives them (drifted literal, renamed site, a seventh encoding,
  # a skewed pinned count, and the zero-hit vacuity case) against synthetic trees via the
  # DEVLOOP_TEST-gated SUBDOMAIN_GUARD_ROOT seam. Deliberately NOT under guards/simple/:
  # run-guards.sh's `find -name '*.sh'` matches `*.test.sh` too, so it would be auto-run as a
  # guard as well as here. No cluster.
  run_and_emit "subdomain-regex-guard-selftest" "${__here}/guards/validate-subdomain-regex-sync.test.sh" || true
  # Slug-class sync self-test (story task #4, R-8): same shape and same reason as the
  # subdomain guard above. validate-slug-class-sync.sh pins /close-story's read-time slug
  # regex to the canonical `manifest::SLUG_PATTERN`; in-tree it always passes, so its drift
  # branch — the only branch with any value — is exercised here against synthetic pairs via
  # the SLUG_SYNC_RUST_SRC / SLUG_SYNC_SKILL seams. Also covers the vacuity cases (marker
  # missing, canonical declaration renamed), where a grep-based guard would otherwise compare
  # nothing and report success. Deliberately NOT under guards/simple/ for the same
  # find -name '*.sh' reason. No cluster.
  run_and_emit "slug-class-sync-guard-selftest" "${__here}/guards/validate-slug-class-sync.test.sh" || true
  # Counter-zero-init self-test (ADR-0036 story-1 counter-visibility fix): the guard PASSES on
  # every real run (all four services are zero-init'd), so its failure branches — the positive
  # control (a planted lazy counter MUST fire), the exempt/malformed/mutual-exclusion paths, the
  # slot()-witness predicate, the entrypoint-not-called check, and the scope-liveness tokens —
  # are exercised here against synthetic roots. Deliberately NOT under guards/simple/ (the
  # find -name '*.sh' reason). No cluster.
  run_and_emit "counter-zero-init-selftest" "${__here}/guards/counter-zero-init.test.sh" || true
  # Disk-guard self-test (envtest-infra-reliability devloop): forces
  # check_build_disk_space's trip branch so the `REASON=insufficient-disk` operator contract
  # (§6.7) is covered — Gate-2's rebuild only hits the guard's pass path. No cluster needed
  # (real df, absurd DEVLOOP_MIN_DISK_GB).
  run_and_emit "setup-disk-guard-selftest" "${__here}/setup.test.sh" || true
  # Orchestrator lane-integrity (task #56): exercises layer-all's exit-code/summary/budget/
  # bypass-closure behavior end-to-end via the DEVLOOP_TEST-gated LAYER_SCRIPT_DIR stub seam
  # — the layer-all-level seams layer7.test.sh structurally cannot reach. No cluster.
  run_and_emit "layer-all-orchestrator-test" "${__here}/layer-all.test.sh" || true
  # Story-runner self-test (ADR-0035 §12): the runner drives unattended
  # skip-permissions sessions and reaches `git reset --hard` + `git clean -fdq`
  # from a classification decision, with zero tests before this file. Hermetic —
  # real runner, throwaway fixture repo via the DEVLOOP_TEST-gated
  # STORY_REPO_ROOT/DT_STORY seams; stub claude/sleep/date, real git, real
  # dt-story. No cluster, no network, no real sessions.
  run_and_emit "run-story-selftest" "${__here}/workflow/run-story.test.sh" || true
  # Guard-runner self-test (fast-fail-guards devloop): pins run-guards.sh's timeout
  # emission + the violation/timeout counter split + the ladder-mirror exit precedence
  # (PRECONDITION>0 → exit 2 dominant), all via a PATH-stubbed `timeout` (hermetic, no real
  # sleep). The exit-precedence mirror in run-guards.sh (it sources guards/common.sh, not
  # lang/_common.sh, so cannot call status_to_exit_code) has NO other mechanical drift guard;
  # this test derives the expected exit from status_to_exit_code at test time. Deliberately
  # NOT under guards/simple/ — run-guards.sh's `find simple -name '*.sh'` would auto-run it as
  # a guard too. No cluster.
  run_and_emit "run-guards-selftest" "${__here}/guards/run-guards.test.sh" || true
  # Frame-vector drift guard SELF-TEST (story task 8, extended at task 15). The
  # guard itself is auto-discovered by run-guards.sh above and passes on every
  # devloop — but a passing guard exercises none of its failure branches, and
  # those branches are its entire value. This drives every vacuity PRECONDITION,
  # every VIOLATION, and all four g14 arms.
  #
  # NO CASE COUNT HERE ON PURPOSE. It used to say "all 33" against an actual 41,
  # having drifted by eight without anything noticing — two encodings of one
  # number, which is the defect this whole guard family exists to prevent. The
  # count now lives in exactly one place, `EXPECTED_CASES` inside the suite, which
  # fails loudly if the tally moves.
  #
  # Since task 15 both codecs are gated in tree, so g14's ungated arms describe a
  # state the repository has LEFT BEHIND and the fixtures synthesize it. That
  # includes `todo-tracking-entry-missing`, whose guard branch is now unreachable
  # on any real run — this suite is its only exerciser.
  #
  # Two arms exist because the obvious "fixes" are wrong: making g14 exit nonzero
  # while a codec was ungated would have made DELETING the guard the fastest route
  # to green, and a guard that redded on the fully-gated end state would only have
  # been discovered when reaching it. It also pins that the banner keeps its
  # `^WARN ` prefix, which run-guards.sh's exit-0 arm greps for —
  # run-guards.test.sh's stub is synthetic and cannot reach that leg. Hermetic via
  # the DEVLOOP_TEST-gated FRAME_VECTORS_ROOT seam; no cluster, no network, no
  # cargo. Deliberately NOT under guards/simple/ for the same `find -name '*.sh'`
  # reason as its neighbours above.
  run_and_emit "frame-vectors-guard-selftest" "${__here}/guards/validate-frame-vectors.test.sh" || true
  # dev-web preflight contract self-test (release-premise-and-preflight-guards devloop):
  # pins the WARN→HARD-FAIL escalation of the MC/MH WebTransport checks, which an
  # exit-code assertion structurally CANNOT pin — `check_port` sets HARD_FAIL without
  # exiting, so `dev-web.sh --check` exits nonzero in any cluster-less environment
  # regardless of what those branches decided. Asserts the per-branch ✗/! markers
  # instead, so reverting the escalation flips it red.
  #
  # It also carries the only automated link in the chain
  #   client-dev-local.md §3 Step 0 -> `--help` -> script header -> script body
  # after §3 Step 0's duplicate severity enumeration was deleted: the header-vs-body
  # drift assertions and the `--help` last-line sentinel. Without this file wired here,
  # that deletion would remove redundancy without adding a forcing function.
  #
  # Hermetic: PATH-stubbed ss/curl/getent, temp repo root, AC_PORT/GC_PORT overrides.
  # No cluster, no network. Deliberately NOT under guards/simple/ — run-guards.sh's
  # `find simple -name '*.sh'` would auto-run it as a guard.
  run_and_emit "dev-web-preflight-selftest" "${__here}/dev-web.test.sh" || true
  # Media-path telemetry deny SELF-TEST (ADR-0036 §11). The guard itself is
  # auto-discovered by run-guards.sh above and PASSES on every run — the media
  # directory is clean and is meant to stay that way — so a real run exercises
  # none of its failure branches, and those branches are its entire value. This
  # drives them: every scope/parse token (missing, empty, no-directories,
  # escapes-root, manifest-missing, manifest-unparseable), one plant per denied
  # macro family into a COPY of the real media tree with a verbatim copy of the
  # shipped manifest, and the REASON precedence ladder with two conditions live
  # at once.
  #
  # It also carries the two assertions that cannot be made from inside the Rust
  # module: that no `VIOLATION:`/`ERROR:` record spills onto a continuation line
  # (run-guards.sh greps and `head -5`s, so a wrapped record is silently
  # truncated), and that a planted macro's ARGUMENT text never reaches stdout,
  # stderr or `--explain` — a guard that echoes what it detected has moved
  # ADR-0036 §11's metadata leak into CI logs rather than closed it.
  #
  # The case with no counterpart anywhere else is the scope BOUNDARY one: a
  # denied macro planted in two non-configured siblings must stay GREEN. A
  # regression widening the walk root from the resolved directory to the crate
  # or repo root passes every other case in the suite and surfaces only as a
  # mystery red on an unrelated file in someone else's diff.
  #
  # Hermetic: throwaway roots under mktemp -d, real binary via `--root` (a
  # production flag, so there is NO test seam to gate and no disarm switch to
  # misuse). No cluster, no network, no cargo. Deliberately NOT under
  # guards/simple/ — run-guards.sh's `find simple -name '*.sh'` would auto-run
  # it as a production guard as well as here.
  run_and_emit "media-telemetry-deny-selftest" "${__here}/guards/media-telemetry-deny.test.sh" || true
  # _common.sh unit tests (ADR-0037 §D7 wires this in — it was orphaned/unrun, docs/TODO.md:2006). Hosts
  # the STATUS-aggregation precedence, fail_fast_mode, AND the fmt_mode truth table (incl. the two
  # load-bearing apply-opt-in-loses-to-CI/override cells). Hermetic: pure bash, no cargo/cluster/network.
  run_and_emit "common-selftest" "${__here}/lang/_common.test.sh" || true
  # Rust fmt wrapper integration (ADR-0037 §D7): stubbed `cargo` on PATH (hermetic — a PATH stub, NOT the
  # real toolchain, so the "no cargo" invariant above holds), argv-asserted mode dispatch + the four-arm
  # apply pre-pass + positive controls. Under scripts/lang/rust/ (NOT guards/simple/ — the find-name reason).
  run_and_emit "rust-fmt-selftest" "${__here}/lang/rust/fmt.test.sh" || true
  # Proto fmt wrapper + _buf.sh preflight (ADR-0037 §D7 COLLAPSE branch): stubbed `pnpm` on PATH (hermetic
  # — a PATH stub, NOT the real toolchain/buf), asserting the four-token preflight taxonomy, the P-6 version-
  # precedence (version-mismatch beats format), the buf rc map (drift=100/parse=1, INVERSE of rust), the
  # apply post-condition re-check, and the GSA WARN anchor. The pnpm-unavailable discriminator proves the
  # lane never falls back to a bare-PATH buf (the COLLAPSE decision).
  run_and_emit "proto-fmt-selftest" "${__here}/lang/proto/fmt.test.sh" || true
  # Proto GOLDEN-FORMAT behavioral pin (ADR-0037 §D7; @security control). UNLIKE the stubbed fmt self-tests
  # above, this runs the REAL `pnpm exec buf` (offline, no network, sub-second over a ~25-line fixture — the
  # same node-tooling-in-Layer-3 precedent as ts-fmt-selftest). It asserts the pinned buf is idempotent on the
  # committed golden and normalizes a deformed copy back to it, so a @bufbuild/buf bump that reformats reds
  # here and its diff is the review evidence — the disjoint control the version assertion (green on a bump)
  # cannot provide. Fail-loud via the shared preflight; never a skip.
  run_and_emit "proto-golden-format-selftest" "${__here}/lang/proto/golden-format.test.sh" || true
  # TS fmt wrapper integration (ADR-0037 §D7). DELIBERATELY NOT STUBBED (unlike rust/proto's PATH stubs):
  # the TS lane forwards `-- <flag>` through `nx run-many` to prettier, and bare prettier with no mode
  # flag exits 0 (vacuous pass), so ONLY the REAL nx→prettier chain proves the flag arrives — a stub
  # can't exercise nx arg-forwarding. Still hermetic + NO NETWORK: a synthetic nx workspace under
  # DEVLOOP_TMP with node_modules SYMLINKED to the repo's (nx/prettier/plugin already installed —
  # offline), never the repo tree; NX_DAEMON=false + isolated NX_CACHE_DIRECTORY so no stale graph/cache
  # is served. Node tooling, NOT cargo — the "no cargo" invariant above is about compiler cost/variance
  # in the fast tier; nx+prettier over tiny fixtures run ~10s (budget recorded with @operations per §6.3).
  run_and_emit "ts-fmt-selftest" "${__here}/lang/ts/fmt.test.sh" || true
  # Fmt-lane SSoT policy guard SELF-TEST (ADR-0037 §D7/§D9). The guard (auto-discovered under
  # guards/simple/) PASSES on the real tree, so its failure branches — the SSoT-bypass detector, the
  # attesting-gate prefix checks, the pre-commit check-only form, and the comment-vs-code discriminator
  # that keeps the bypass detector from forbidding documentation — are exercised here against synthetic
  # fixtures via the FMT_SSOT_* seams. Deliberately NOT under guards/simple/ (the find -name '*.sh'
  # reason). No cluster.
  run_and_emit "fmt-lane-ssot-guard-selftest" "${__here}/guards/validate-fmt-lane-ssot.test.sh" || true
  # TS-fmt proto-exclusion standing guard SELF-TEST (ADR-0037 §D7; @dry-reviewer). The guard (auto-discovered
  # under guards/simple/) PASSES on the real tree, so its failure branches — the missing proto-ignore line,
  # a format-target cwd, and the zero-targets vacuity guard — are exercised here against synthetic config via
  # the TSFMT_* seams. Deliberately NOT under guards/simple/ (the find -name '*.sh' reason). No cluster.
  run_and_emit "ts-fmt-proto-excluded-guard-selftest" "${__here}/guards/validate-ts-fmt-proto-excluded.test.sh" || true
} | tee_collect_statuses
