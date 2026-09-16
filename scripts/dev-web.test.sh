#!/usr/bin/env bash
# dev-web.test.sh — hermetic self-test for scripts/dev-web.sh's preflight
# contract.
#
# ─── Why this suite asserts MARKERS and never the exit code ──────────────────
#
# `dev-web.sh --check` exits nonzero in ANY cluster-less environment — CI
# included — regardless of what the WebTransport branches decided, because
# `check_port` for AC (8443) and GC (8444) calls `fail()` (setting HARD_FAIL=1)
# WITHOUT exiting early, and the single `exit 1` sits at the end of preflight.
#
# So an exit-code assertion like "`--check` exits nonzero when the fingerprints
# file is missing" is VACUOUS: it was green before the WARN→HARD-FAIL
# escalation and would be green again the moment someone reverted it. A suite
# that cannot distinguish the pre- and post-escalation states does not test the
# escalation.
#
# The property this suite pins instead: **reverting WARN→HARD FAIL re-emits the
# `!` WARN marker for those branches, which flips `assert_status '✗'` red.**
# Every escalation assertion below is therefore a paired marker check —
# the `✗` must be present AND the `!` must be absent for that branch's text.
#
# ─── Hermeticity ────────────────────────────────────────────────────────────
#
# PATH-stubs `ss`, `curl` and `getent` (the `run-guards.test.sh` PATH-stubbed-
# `timeout` technique), runs against a temp repo root with synthetic
# configmaps, and overrides AC_PORT/GC_PORT. Nothing here touches a cluster, a
# network, or the developer's real toolchain. If this could not be made
# hermetic it would NOT be wired into layer3.sh: a permanently-red shared
# Layer-3 step is worse for everyone on the branch than no self-test, and
# quarantining is not an available escape (ADR-0028).

set -euo pipefail
IFS=$'\n\t'

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/.." && pwd)"
# shellcheck source=./lang/_test_helpers.sh
source "$HERE/lang/_test_helpers.sh"

DEV_WEB="$REPO_ROOT/scripts/dev-web.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A hermetic PATH for the script under test: every executable reachable on the
# outer PATH, symlinked into one directory, EXCEPT `ss`. The stub dir is then
# prefixed to shadow curl/getent and to supply `ss` for the present/broken
# modes. This is what makes the ss-absent case testable at all — prefixing a
# stub dir onto the real PATH can hide nothing, so on any machine with
# iproute2 installed (every CI runner, most workstations) the absent branch
# was unreachable and its three assertions were guaranteed red.
TOOLBOX="$WORK/toolbox"
mkdir -p "$TOOLBOX"
IFS=: read -r -a __path_dirs <<<"$PATH"
for __d in "${__path_dirs[@]}"; do
  [[ -d "$__d" ]] || continue
  for __f in "$__d"/*; do
    [[ -f "$__f" && -x "$__f" ]] || continue
    __n="${__f##*/}"
    [[ "$__n" == "ss" ]] && continue
    [[ -e "$TOOLBOX/$__n" ]] || ln -s "$__f" "$TOOLBOX/$__n"
  done
done
unset __path_dirs __d __f __n

# ---------------------------------------------------------------------------
# Fixture: a temp "repo" carrying only what dev-web.sh's preflight reads.
# ---------------------------------------------------------------------------

# Build a temp root. $1 = "with-fingerprints" | "without-fingerprints".
# $2 = advertise address to write into every configmap ("" = omit the key).
make_root() {
  local fingerprints="$1" advertise="${2-}"
  local root; root="$(mktemp -d "$WORK/root.XXXXXX")"

  mkdir -p "$root/scripts" "$root/packages/web-app" "$root/infra/docker/certs"
  cp "$DEV_WEB" "$root/scripts/dev-web.sh"
  chmod +x "$root/scripts/dev-web.sh"

  # SSoT files the preflight reads for version pins.
  printf '22.13.0\n' >"$root/.nvmrc"
  cat >"$root/package.json" <<'JSON'
{ "packageManager": "pnpm@10.33.2", "engines": { "node": ">=22.13.0 <23" } }
JSON

  if [[ "$fingerprints" == "with-fingerprints" ]]; then
    printf '{"mc":"AA","mh":"BB"}\n' >"$root/infra/docker/certs/fingerprints.json"
  fi

  local svc
  for svc in mc-0 mc-1 mh-0 mh-1; do
    local dir="$root/infra/services/${svc%%-*}-service"
    mkdir -p "$dir"
    local key
    key="$(printf '%s' "${svc%%-*}" | tr '[:lower:]' '[:upper:]')_WEBTRANSPORT_ADVERTISE_ADDRESS"
    if [[ -n "$advertise" ]]; then
      printf 'data:\n  %s: "%s"\n' "$key" "$advertise" >"$dir/${svc}-configmap.yaml"
    else
      printf 'data:\n  SOMETHING_ELSE: "x"\n' >"$dir/${svc}-configmap.yaml"
    fi
  done
  printf '%s' "$root"
}

# Build a PATH stub dir. $1 = "ss-present" | "ss-absent";
# $2 = the `ss -uln` output the stub should print.
make_stubs() {
  local ss_mode="$1" ss_output="${2-}"
  local d; d="$(mktemp -d "$WORK/stub.XXXXXX")"

  # curl: always "connection refused" so AC/GC hard-fail deterministically.
  # That is fine — this suite never reads the exit code.
  cat >"$d/curl" <<'SH'
#!/usr/bin/env bash
echo "curl: (7) Failed to connect" >&2
exit 7
SH

  # getent: no demo.localhost, no IPv6-first localhost.
  cat >"$d/getent" <<'SH'
#!/usr/bin/env bash
exit 2
SH

  if [[ "$ss_mode" == "ss-broken" ]]; then
    # Present on PATH but exits nonzero — restricted container / missing /proc/net.
    printf '#!/usr/bin/env bash\nexit 1\n' >"$d/ss"
  fi
  if [[ "$ss_mode" == "ss-present" ]]; then
    cat >"$d/ss" <<SH
#!/usr/bin/env bash
cat <<'OUT'
${ss_output}
OUT
SH
  fi

  chmod +x "$d"/*
  printf '%s' "$d"
}

# Run `dev-web.sh --check` hermetically. Sets RUN_OUT.
# PATH is stubs + TOOLBOX only — never the outer PATH — so `ss` exists exactly
# when the stub dir provides it.
run_check() {
  local root="$1" stubs="$2"
  RUN_OUT="$(cd "$root" && env PATH="${stubs}:${TOOLBOX}" AC_PORT=1 GC_PORT=2 \
    bash "$root/scripts/dev-web.sh" --check 2>&1 || true)"
}

# Strip ANSI so marker assertions do not depend on TTY detection.
plain() { printf '%s' "$1" | sed -e 's/\x1b\[[0-9;]*m//g'; }

# Assert a branch is HARD FAIL: its text carries `✗` and NOT `!`.
# This pairing is the escalation assertion — a revert re-emits `!` and the
# absence check flips red.
assert_hard_fail_branch() {
  local label="$1" needle="$2" out="$3"
  local line
  line="$(printf '%s\n' "$out" | grep -F -- "$needle" | head -1 || true)"
  if [[ -z "$line" ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] branch text not found: ${needle}")
    return
  fi
  assert_status   "${label}-is-hard-fail" '✗' "$line"
  assert_absent   "${label}-not-warn"     '!' "$line"
}

# ===========================================================================
# (1) Cert fingerprints: HARD FAIL, with a remedy.
# ===========================================================================
root="$(make_root without-fingerprints 'https://127.0.0.1:4434')"
stubs="$(make_stubs ss-present '')"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
assert_hard_fail_branch "fingerprints-missing" "fingerprints missing" "$out"
assert_status "fingerprints-states-the-stop"   "Not starting the dev server" "$out"
assert_status "fingerprints-has-remedy"        "scripts/generate-dev-certs.sh" "$out"
# The COUNTER-MESSAGE, at the one output that actually prints when a developer
# meets "the browser refuses the handshake" — which is the moment a
# certificate-validation bypass gets pasted into a launch line. The script header
# carries the same warning, but the header is only ever reached through `--help`.
#
# Pinned on a phrase from our own prose, deliberately NOT on a flag name: this
# file is scanned by `dt-guard no-insecure-browser-flags` (`.sh` is a candidate
# suffix and this script is named in that guard's own test), so an assertion
# spelling the literal would reintroduce the very thing the clause warns against.
assert_status "fingerprints-warns-off-browser-flag" \
  "disables certificate validation" "$out"

# ...and the positive control: present fingerprints do NOT emit that branch.
root="$(make_root with-fingerprints 'https://127.0.0.1:4434')"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
assert_absent "fingerprints-present-no-failure" "fingerprints missing" "$out"

# ===========================================================================
# (2) Listener missing: HARD FAIL for ALL FOUR instances (MC as well as MH).
#     `ss` present but reporting no listener on the advertised port.
# ===========================================================================
root="$(make_root with-fingerprints 'https://127.0.0.1:4434')"
stubs="$(make_stubs ss-present 'udp   UNCONN  0  0   127.0.0.1:9999   0.0.0.0:*')"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
for label in mc-0 mc-1 mh-0 mh-1; do
  assert_hard_fail_branch "listener-${label}" "${label}: nothing listening" "$out"
done
assert_status "listener-states-user-impact" "you would hear nothing" "$out"
assert_status "listener-has-setup-remedy"   "./infra/kind/scripts/setup.sh" "$out"

# The remedy command must name the SERVICE-PREFIXED ConfigMap key. An
# unprefixed jsonpath returns EMPTY rather than erroring, which reads as "the
# live ConfigMap has no advertise address either" — manufacturing the exact
# false conclusion the remedy exists to prevent. Two-sided: the right form
# present AND the wrong form absent, so a regression flips this red.
assert_status "remedy-has-mh-prefixed-key" "{.data.MH_WEBTRANSPORT_ADVERTISE_ADDRESS}" "$out"
assert_status "remedy-has-mc-prefixed-key" "{.data.MC_WEBTRANSPORT_ADVERTISE_ADDRESS}" "$out"
assert_absent "remedy-no-unprefixed-key"   "{.data.WEBTRANSPORT_ADVERTISE_ADDRESS}"    "$out"

# ...positive control: a matching listener passes and emits no failure.
root="$(make_root with-fingerprints 'https://127.0.0.1:4434')"
stubs="$(make_stubs ss-present 'udp   UNCONN  0  0   127.0.0.1:4434   0.0.0.0:*')"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
assert_status "listener-present-passes"      "WebTransport listener on 127.0.0.1:4434" "$out"
assert_absent "listener-present-no-failure"  "nothing listening" "$out"

# ===========================================================================
# (3) CANNOT VERIFY branches are HARD FAIL, in two distinct owner lanes.
# ===========================================================================
# (3a) `ss` absent — the operator's machine. A missing package must not
#      reproduce the silent no-audio failure this escalation closes.
root="$(make_root with-fingerprints 'https://127.0.0.1:4434')"
stubs="$(make_stubs ss-absent)"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
assert_hard_fail_branch "ss-missing-cannot-verify" "'ss' is not installed" "$out"
assert_status "ss-missing-says-cannot-verify" "CANNOT VERIFY" "$out"
assert_status "ss-missing-has-remedy"         "apt-get install -y iproute2" "$out"

# (3b) No advertise address — REPO/CONFIG DRIFT, a different owner lane, and
#      the message must say so or a reader restarts a healthy cluster for an
#      hour. Distinct message shape is the assertion.
root="$(make_root with-fingerprints '')"
stubs="$(make_stubs ss-present '')"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
assert_hard_fail_branch "no-advertise-cannot-verify" "no WEBTRANSPORT_ADVERTISE_ADDRESS found" "$out"
assert_status "no-advertise-names-its-lane"    "REPO/CONFIG DRIFT" "$out"
assert_status "no-advertise-nothing-to-fix"    "Nothing to fix locally" "$out"

# ===========================================================================
# (4) No bypass environment variable exists.
#     An escape hatch here is the "silently skip a failure to make progress"
#     pattern CLAUDE.md forbids; this pins its absence rather than trusting it.
# ===========================================================================
dev_web_src="$(cat "$DEV_WEB")"
assert_absent "no-skip-wt-bypass"   "DEV_WEB_SKIP_WT"   "$dev_web_src"
assert_absent "no-generic-bypass"   "SKIP_PREFLIGHT"    "$dev_web_src"

# ===========================================================================
# (5) --help is derived, complete, and cannot silently truncate.
#     The runbook (§3 Step 0) now points at `--help` as the SSoT for which
#     check carries which severity, so a truncated --help silently truncates
#     the runbook's answer. Sentinel = a marker from the header's LAST line.
# ===========================================================================
help_out="$(bash "$DEV_WEB" --help)"
# Sentinel: the final header line. If a stray BARE empty line is ever inserted
# mid-header, the contiguous-comment-run extraction stops early and THIS
# assertion goes red — a test failure instead of a reader discovering it.
assert_status "help-reaches-last-header-line" "AC_PORT / GC_PORT env if your overlay differs." "$help_out"
assert_status "help-includes-usage"           "scripts/dev-web.sh --check" "$help_out"
assert_status "help-includes-contract"        "THE CONTRACT" "$help_out"
assert_status "help-includes-secure-context"  "SECURE CONTEXT" "$help_out"
# Bounded by construction: --help must stop at the header, never run to EOF.
assert_absent "help-does-not-dump-script"     "set -euo pipefail" "$help_out"

# ===========================================================================
# (6) Header-vs-body drift: the severity table and the code must agree.
#     This is the ONE automated link in the chain
#       runbook §3 Step 0 -> --help -> header -> body
#     after the runbook's duplicate enumeration was deleted, so it carries the
#     whole forcing function.
# ===========================================================================
# Every row the header tags [HARD FAIL] for WebTransport must have escalated in
# the body — i.e. no `warn` call may remain inside check_wt_endpoint or the
# fingerprint check.
#
# ─── OPS-9: both extractions anchor on STABLE CODE, never on the severity word ──
#
# An `assert_absent` over an EMPTY extraction passes vacuously, so the sed range
# is part of the assertion's soundness, not plumbing. The fingerprint range used
# to anchor on `Cert fingerprints (HARD FAIL` — the severity word inside the
# section comment, i.e. the very text a revert rewrites. Demonstrated: flipping
# that comment back to `(WARN — …)` and `fail`→`warn` yielded a zero-length
# block, `assert_absent` went green, and the top-of-file header row (untouched by
# that revert) still read `[HARD FAIL]` — so BOTH assertions passed while the body
# warned. That is exactly the drift this block exists to catch.
#
# Same failure class as a walk-based guard that matches nothing and reports OK,
# which `release_build_profile` fails closed on with five vacuity tokens. This
# suite holds itself to the rule its subject enforces.
#
# Two independent controls, and neither may be dropped on the grounds that the
# other suffices:
#   (a) anchor on a code line a revert leaves alone (a function definition, a
#       test expression) rather than on prose that encodes the answer; and
#   (b) assert the block is NON-VACUOUS before asserting what it must not
#       contain — so a future anchor that stops matching reds here instead of
#       silently disarming the check.
assert_block_nonvacuous() {
  local label="$1" anchor="$2" block="$3"
  assert_status "${label}-extraction-nonvacuous" "$anchor" "$block"
}

wt_fn="$(printf '%s\n' "$dev_web_src" | sed -n '/^check_wt_endpoint() {/,/^}/p')"
assert_block_nonvacuous "wt-check" "WEBTRANSPORT_ADVERTISE_ADDRESS" "$wt_fn"
assert_absent "no-warn-left-in-wt-check"  "    warn " "$wt_fn"
assert_status "wt-check-uses-fail"        "        fail " "$wt_fn"

# Anchored on the test expression, not on the comment: survives any rewording of
# the section header while still bounding the block at its closing `fi`.
fp_block="$(printf '%s\n' "$dev_web_src" | sed -n '/if \[\[ -s "\$FINGERPRINTS_JSON" \]\]/,/^fi$/p')"
assert_block_nonvacuous "fingerprints" "FINGERPRINTS_JSON" "$fp_block"
assert_absent "no-warn-left-in-fingerprints" "    warn " "$fp_block"
assert_status "fingerprints-uses-fail"       "    fail " "$fp_block"

# The header must NOT still tag the WebTransport rows as WARN.
# Read the FILE, never `printf "$src" | awk '…{exit}'`. That pipeline is the
# documented SIGPIPE trap (docs/TODO.md §Infrastructure Validation in Devloops):
# awk exits at the first non-comment line while printf is still writing ~31 KB,
# printf takes SIGPIPE, `pipefail` propagates, and `set -e` kills this suite
# BEFORE `report_results` — so it exits 141 with ZERO output and zero
# assertions. Measured at the current file size: 40/40 failures.
#
# It is a size-dependent race, so it passed for most of this devloop and began
# failing deterministically once dev-web.sh grew past ~31 KB — i.e. the suite
# would have started silently self-disabling as the script it guards got bigger.
# Reading the file directly removes the pipe, which removes the class.
header_block="$(awk 'NR==1{next} /^#/{print; next} {exit}' "$DEV_WEB")"
assert_absent "header-no-wt-warn-tag" "WARN — join only" "$header_block"
assert_status "header-tags-fingerprints-hard-fail" "cert fingerprints present            [HARD FAIL]" "$header_block"
assert_status "header-tags-listener-hard-fail"     "right-family listener   [HARD FAIL]" "$header_block"
# The browser-only row is genuinely a side path and must STAY a warn — the
# escalation is scoped, not a blanket flip.
assert_status "header-keeps-demo-localhost-warn" "demo.localhost resolves (/etc/hosts)                    [WARN" "$header_block"

# ===========================================================================
# (7) The secure-context contract, now enforced on BOTH sides.
#
#     Story task 20 landed `## Secure Context and Media Setup` in the runbook and
#     trimmed this script's header to one claim plus the pointer (the decision
#     recorded at docs/TODO.md:759 / F-DRY-E). That reconciliation moved prose
#     out of a file this suite guards and into one it did not — so the
#     assertions below were RETARGETED rather than retired. The rule the trim had
#     to satisfy: the runbook side must not end up carrying FEWER guarantees than
#     the header side it replaced.
#
#     OWNERSHIP, because a failure here spans two owners: `scripts/dev-web.*` is
#     infrastructure's; `docs/runbooks/client-dev-local.md` is OPERATIONS'. Every
#     `runbook-*` failure below says so in its own message — do not "fix" one by
#     editing runbook prose you do not own.
# ===========================================================================
assert_status "pointer-cites-frozen-anchor" \
  "docs/runbooks/client-dev-local.md#secure-context-and-media-setup" "$header_block"
# NOTE: `pointer-names-the-apis` used to live here, asserting the four gated API
# names in THIS header. It was not deleted — it MOVED to `runbook-names-the-apis`
# below, because the enumeration itself moved to the runbook, which is now
# canonical for it. Retiring it outright would have left that claim guarded
# nowhere: not here (assertion gone) and not there (never covered).
assert_status "pointer-says-non-loopback"   "NON-LOOPBACK" "$header_block"
# NOT just the word NON-LOOPBACK: that token survives any rewrite, so the header
# could lose the symptom sentence entirely without reddening. Pin the corrected
# OBSERVABLE too — the join dying at connecting-mc, which is the claim that
# replaced the false "joined, no audio" this diff removed.
assert_status "pointer-states-the-symptom"  "connecting-mc" "$header_block"
# THE ANTI-FLAG CLAUSE. §9 retains a duplicate of this header block on the
# argument that this one sentence is the one whose absence causes harm — and
# until now four assertions pinned which API names appear and ZERO pinned the
# sentence the argument was actually about. `dt-guard no-insecure-browser-flags`
# does not cover this: it denies a prohibited literal being ADDED; nothing
# detects the counter-message being REMOVED. Different failure modes.
# The needle names no prohibited literal, so this assertion does not trip it.
assert_status "pointer-warns-against-browser-flag" "Never a browser flag" "$header_block"
assert_status "pointer-gives-approved-fix"  "Use a .localhost name or a loopback literal" "$header_block"
# Must NOT say "an HTTP origin" flatly: http://<org>.localhost:5173 IS a secure
# context, and a reader told otherwise chases the wrong problem. The runbook
# asserts the same fact from its side — see `runbook-does-not-contradict-header`.
assert_status "pointer-does-not-contradict-task-20" \
  "ARE potentially trustworthy, which is why the demo works over plain http://" "$header_block"

# ─── The frozen anchor is a CONTRACT, and it is now FAIL-CLOSED ──────────────
#
# This block used to end in `else PASS=$((PASS+1))`, which was right while the
# target legitimately did not exist. It no longer is: the heading exists by
# construction, so that branch became a check that observes nothing and reports
# clean — it would stay green forever if someone renamed the heading away, which
# is the one failure it was written to catch.
#
# Two DISTINCT reason tokens, deliberately: "the pointer's target vanished" and
# "the pointer is misspelled" have different fixes, and a single token would send
# whoever hits it to the wrong file.
RUNBOOK="docs/runbooks/client-dev-local.md"
CITED_ANCHOR="secure-context-and-media-setup"

# A prose claim the runbook is canonical for, with a failure that ROUTES ITSELF.
#
# `assert_status` emits a fixed shape — `[label] expected substring ... not
# found in output` — with no file and no owner. Fine for a claim about THIS
# script, wrong for these: the two structural failures in this block already
# name the file and its owner, and it is the inconsistency INSIDE one block
# whose whole purpose is cross-owner coordination that makes the plain form a
# defect here. Prose reds are also the likelier ones, since runbook wording is
# reworded far more often than headings are deleted.
#
# THE LAST CLAUSE IS LOAD-BEARING. Without it the cheapest-looking fix for a red
# here is to put the sentence back into this script header — which silently
# undoes the reconciliation the trim exists to land.
assert_runbook_claim() { # $1=label $2=needle $3=body
    local label="$1" needle="$2" body="$3"
    if [[ "$body" == *"$needle"* ]]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        FAILURES+=("[${label}] ${RUNBOOK} (OPERATIONS-owned) no longer states: '"'"'${needle}'"'"'. dev-web.sh header prose was trimmed on the promise this claim lives there — restore it in the runbook; do NOT re-add prose to the script.")
    fi
}
sc_heading="$(grep -inE '^#{2,3} .*secure.context' "$REPO_ROOT/$RUNBOOK" | head -1 || true)"
if [[ -z "$sc_heading" ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[secure-context-heading-missing] no secure-context heading in ${RUNBOOK}, but scripts/dev-web.sh points at #${CITED_ANCHOR}. The pointer's TARGET is gone (not misspelled). ${RUNBOOK} is OPERATIONS-owned — restore the '## Secure Context and Media Setup' section there rather than editing the pointer.")
else
    # GitHub slug: lowercase, drop non-alphanumeric/space/hyphen, spaces -> hyphens.
    sc_text="${sc_heading#*:}"; sc_text="${sc_text#\#* }"
    sc_slug="$(printf '%s' "$sc_text" \
        | tr '[:upper:]' '[:lower:]' \
        | sed -e 's/[^a-z0-9 -]//g' -e 's/^ *//' -e 's/ *$//' -e 's/ /-/g')"
    assert_status "secure-context-anchor-resolves" "$CITED_ANCHOR" "$sc_slug"

    # ─── PROSE PARITY: the runbook must carry what the trim removed ──────────
    #
    # docs/TODO.md:759 noted that the anchor check pins the SLUG but not prose
    # DIVERGENCE. With the header down to one claim, closing that gap is cheap.
    #
    # BOUNDED extraction, structurally: heading line to the line before the next
    # `^## `, or EOF. Same reasoning as the awk-vs-`sed -n '2,/^$/p'` note in
    # dev-web.sh's --help arm — a range whose worst case is truncation, never an
    # unbounded run to EOF that would make every `contains` below pass by
    # swallowing the whole document. Note this stops at the next `##`, so a
    # `###` subsection IS included and a new `##` section is NOT.
    sc_line="${sc_heading%%:*}"
    sc_body="$(awk -v start="$sc_line" 'NR < start { next } NR == start { print; next } /^## / { exit } { print }' "$REPO_ROOT/$RUNBOOK")"

    # Vacuity guard, with its OWN token: an empty extraction makes every
    # `contains` below fail, and without this they would all read as "the runbook
    # lost the claim" when the truth is "the extraction produced nothing".
    if [[ -z "${sc_body//[[:space:]]/}" ]]; then
        FAIL=$((FAIL + 1))
        FAILURES+=("[secure-context-section-extraction-empty] extracted an EMPTY body for '${CITED_ANCHOR}' in ${RUNBOOK} — the extraction is broken, NOT the prose. Check the awk range above before touching the runbook.")
    else
        # The four claims the runbook is now canonical for. Each has its own
        # reason token so a failure names WHICH claim went missing.
        assert_runbook_claim "runbook-names-the-apis" "WebTransport" "$sc_body"
        assert_runbook_claim "runbook-names-getusermedia" "getUserMedia" "$sc_body"
        assert_runbook_claim "runbook-names-webcodecs" "WebCodecs" "$sc_body"
        assert_runbook_claim "runbook-names-webcrypto" "WebCrypto" "$sc_body"
        # THE SYMPTOM, and it is NOT "joined with no audio" — that spelling was
        # WRONG and this assertion used to pin it. WebTransport is one of the
        # four gated APIs and MC signalling has no fallback, so a non-loopback
        # origin fails at `connecting-mc` and never reaches a roster at all.
        # Telling a reader to expect silence sends them down §4'"'"'s
        # MC-reachability ladder for a fault that is not there — which is the
        # moment a certificate-validation flag gets pasted in, i.e. the exact
        # outcome this section exists to prevent. A parity assertion pinning a
        # FALSE claim is worse than none: it makes the correction look like a
        # regression.
        assert_runbook_claim "runbook-says-non-loopback" "connecting-mc" "$sc_body"
        # The instruction, matching dev-web.sh'"'"'s approved remedy set exactly.
        assert_runbook_claim "runbook-gives-approved-fix" "loopback literal" "$sc_body"
        assert_runbook_claim "runbook-gives-tls-fix" "terminate real TLS" "$sc_body"
        # The runbook half of the anti-flag clause — see
        # `pointer-warns-against-browser-flag` above for why removal, not
        # addition, is the uncovered failure mode. Inherits the non-empty-body
        # vacuity guard, so it adds no new vacuity surface.
        assert_runbook_claim "runbook-warns-against-browser-flag" \
            "Do not reach for a browser flag" "$sc_body"
        # The non-contradiction pin, mirroring `pointer-does-not-contradict-task-20`
        # on the runbook side. The header comment above that assertion has said
        # since task #61 that "story task 20 asserts the same fact from the
        # runbook side; the two must not contradict" — this is that half. Without
        # it the runbook could be reworded to "HTTP origins are insecure" (false
        # for http://<org>.localhost:5173) with nothing red.
        assert_runbook_claim "runbook-does-not-contradict-header" "potentially trustworthy" "$sc_body"

        # ─── FLAG PARITY: the runbook's fake-media launch line vs the suite's ──
        #
        # The runbook documents the two sanctioned fake-media flags for a demo
        # machine with no microphone. They are a fourth in-tree encoding of a
        # pair that `packages/web-app/playwright.config.ts` also launches with,
        # and prose cannot import a constant — so they are tied together here.
        #
        # These two flags are NOT security bypasses: they inject a device and
        # answer a prompt. They pass `dt-guard no-insecure-browser-flags` on
        # vocabulary non-membership, never via an allowlist entry.
        # DERIVED FROM THE ARTIFACT, never spelled here. An earlier version put
        # the pair in a local array — which made this check, whose job is to stop
        # the flag names drifting, itself a FIFTH in-tree copy of them. It was
        # also weaker: asking "do the flags from OUR list that appear in the
        # runbook also appear in playwright.config?" cannot see a third flag the
        # runbook documents and the config does not, because a list written from
        # memory only finds what it already knew to look for.
        #
        # Reading playwright.config FIRST also makes the check direction match
        # what the runbook itself claims — "the same two flags
        # packages/web-app/playwright.config.ts launches with".
        PW_CONFIG="packages/web-app/playwright.config.ts"
        mapfile -t FAKE_MEDIA_FLAGS < <(grep -oE -- '--use-fake-[a-z-]+' "$REPO_ROOT/$PW_CONFIG" | sort -u)
        sc_flag_hits=0
        for flag in "${FAKE_MEDIA_FLAGS[@]}"; do
            [[ "$sc_body" == *"$flag"* ]] && sc_flag_hits=$((sc_flag_hits + 1))
        done
        # POSITIVE CONTROL. Without it the loop above is an "every X has a Y"
        # over an extracted set, which passes VACUOUSLY when the extraction finds
        # zero flags — e.g. after a reflow pushes the launch line into the next
        # `##` section. That is the same shape as the `else PASS++` removed above;
        # do not reintroduce it here.
        # TWO positive controls, because the derivation has two ends and either
        # yielding zero makes the loop pass vacuously.
        assert_status "flag-parity-config-declares-both" \
            "config declares 2" "config declares ${#FAKE_MEDIA_FLAGS[@]}"
        assert_status "flag-parity-extracted-both" \
            "found 2 fake-media flags" "found ${sc_flag_hits} fake-media flags"
    fi
fi

# ===========================================================================
# (8) The bash-dialect siblings of F-DRY-F: "could not evaluate" must not pass.
#     Found by sweeping @dry-reviewer's method into the dialect their Rust
#     sweep could not reach. Both branches are unreachable on today's tree,
#     which is exactly why they needed tests rather than inspection.
# ===========================================================================

# (8a) Hostname advertise address with no resolver answer. THE SILENT PASS:
#      the IPv6-mismatch test is `[[ "$LOCALHOST_FIRST" == ::* && ... ]]`, which
#      is merely FALSE when the value is empty, so control fell through to the
#      `elif has4 || has6` arm and PASSED — while a hostname resolving IPv6-first
#      against an IPv4-only listener is a guaranteed QUIC timeout and silent
#      no-audio. The getent stub returns nothing, reproducing an absent resolver.
root="$(make_root with-fingerprints 'https://demo.localhost:4434')"
stubs="$(make_stubs ss-present 'udp   UNCONN  0  0   127.0.0.1:4434   0.0.0.0:*')"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
assert_hard_fail_branch "hostname-no-resolver-cannot-verify" "advertises a HOSTNAME" "$out"
assert_status "hostname-no-resolver-says-cannot-verify" "CANNOT VERIFY" "$out"
assert_status "hostname-no-resolver-has-remedy"         "libc-bin" "$out"
# The regression proof: an IPv4 listener EXISTS, so the pre-fix code reported a
# clean pass here. Absence of the pass line is what pins the fix.
assert_absent "hostname-no-resolver-does-not-pass" \
  "WebTransport listener on demo.localhost:4434" "$out"

# (8b) `ss` present but failing. Fails closed either way, so this is not a
#      silent pass — but the pre-fix message blamed the cluster, sending the
#      reader to restart a healthy one. Misdirecting-remedy class (OPS-3).
root="$(make_root with-fingerprints 'https://127.0.0.1:4434')"
stubs="$(make_stubs ss-broken)"
run_check "$root" "$stubs"
out="$(plain "$RUN_OUT")"
assert_hard_fail_branch "ss-broken-cannot-verify" "'ss' failed to run" "$out"
assert_status "ss-broken-says-cannot-verify"   "CANNOT VERIFY" "$out"
assert_status "ss-broken-warns-off-restart"    "do not restart it on this signal" "$out"
# Must NOT misattribute to the cluster being down.
assert_absent "ss-broken-does-not-blame-cluster" "nothing listening" "$out"

# ===========================================================================
# (9) OPS-10: the header's CANNOT-RUN enumeration must not under-list the body.
#
#     The drift block above pins the severity TAGS and the absence of `warn`.
#     Nothing pinned the header's cannot-run CAUSE LIST against the body's
#     branch set — which is why adding the `ss`-failed and no-resolver branches
#     moved the body and left the header enumerating two of four, with 61/61
#     green. Same dimension-of-drift blind spot as OPS-9, one axis over: the
#     assertion covered one property of the block and not the neighbouring one.
#
#     Matters because @operations' §3 Step 0 deletion made `--help` the SSoT for
#     which check carries which severity. An under-listing SSoT sends a reader
#     asking "is getent-unavailable a hard fail?" away without an answer, even
#     though the contract paragraph below states the general rule correctly.
#
#     ONE list drives BOTH assertions, so it cannot rot in halves: every cause
#     phrase must appear in the header, AND the body's CANNOT VERIFY branch
#     count must equal the number of causes this list knows about. A fifth
#     branch therefore reds here until its phrase is added to the header and to
#     this array — which is the forcing function, not the enumeration itself.
CANNOT_RUN_CAUSES=("\`ss\` missing" "\`ss\` failed" "no advertise address" "no resolver")

for cause in "${CANNOT_RUN_CAUSES[@]}"; do
    assert_status "header-enumerates-cannot-run-cause" "$cause" "$header_block"
done

# The count anchors on the USER-VISIBLE MARKER (`CANNOT VERIFY —`) on non-comment
# lines, NOT on the `fail "${label}: …` call shape. That distinction is load-bearing
# (@test, 2026-09-02): a 5th branch written with any other prefix — say
# `fail "${label} (${file}): CANNOT VERIFY — …"`, which would be perfectly correct —
# slips a call-shape grep, leaving the count green while the header under-lists again,
# i.e. the exact failure this block exists to close, one formatting variation away.
# The marker cannot be varied the same way: the script header states that these
# messages say CANNOT VERIFY explicitly, so it is the contract a new branch must
# carry to be correct, not incidental internal style. Comment lines are excluded so
# the header's own prose mention of the marker does not inflate the count.
body_cannot_verify_count="$(printf '%s\n' "$dev_web_src" \
    | grep -v '^[[:space:]]*#' | grep -c 'CANNOT VERIFY —' || true)"
if [[ "$body_cannot_verify_count" -eq "${#CANNOT_RUN_CAUSES[@]}" ]]; then
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1))
    FAILURES+=("[header-cannot-run-count-matches-body] body has ${body_cannot_verify_count} CANNOT VERIFY branches but the header enumerates ${#CANNOT_RUN_CAUSES[@]} causes — add the new cause to BOTH the script header and CANNOT_RUN_CAUSES here")
fi

report_results "scripts/dev-web.test.sh"
