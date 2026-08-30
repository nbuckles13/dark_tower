# Devloop: run-story operator-intervention retry flags (--revalidate / --restart)

## Loop Metadata
- Start Commit: 574ae43d115645f480973db9dce7b7c224f88c15
- Branch: feature/hear-yourself-through-handler
- Lead Model: claude-opus-4-8[1m]
- Specialist: infrastructure
- Mode: light (Implementer + security + test)

## Loop State
- Phase: complete
- Security: pending
- Test: pending

## Validation (Gate 2)
- `./scripts/layer-all.sh` → TOTAL_RESULT clean, EXIT=0 (2nd run, after main.md scope-drift fix).
- L1 OK, L2 OK, L3 OK (guards + run-story-selftest green), L4 N/A (no Rust/TS test diff),
  L5 OK, L6 N/A (no dep-manifest change), L7 OK (env-tests + browser E2E, full cluster, 464s).
- First run red at L3 on `validate-cross-boundary-scope` — main.md was missing the file
  classification table (doc-completeness, not code); fixed and re-run green.
- Post-review Gate 2 (after all 11 findings fixed) re-run green: L1/2/3/5/7 OK, L4/6 N/A,
  L7 env-tests + browser E2E 474s, zero FAIL lines, EXIT=0.

## Task

Add two mutually-exclusive operator-intervention flags to `scripts/workflow/run-story.sh`
for retrying an escalated task after a human has diagnosed the failure. Both act on the
escalated task the next run reaches, both refuse on a `devloop-no-commit` escalation
(nothing to validate/restart from — masked-failure protection), and both compose with
`--stop-after`.

- `--revalidate`: re-run the authoritative gate against the tree as committed by the prior
  attempt (no devloop). Green → `dt-story complete` with recovered slug; red → escalate
  with the fresh gate log. Gate-only cost-ledger attempt (zero devloop cost).
- `--restart 'required text'`: reopen the task, discard the resume pointer, spawn a FRESH
  devloop whose prompt = manifest prompt + an appended operator paragraph (required text
  verbatim + prior commit sha + output-doc slug + STATUS=FAIL REASON= tail lines). Fourth
  prompt-splice site — validate the text (reject backtick-fence lines / contract-breaking
  content) and pass via the prompt FILE, never the command line. Fresh cost-ledger attempt.

Fail loudly on misuse: `--revalidate`+`--restart`, either flag with no escalated task,
`--restart` with empty text. Extend `run-story.test.sh`. Update usage header + escalation-lane
guidance text (currently just says "rerun"). Do not change dt-story.

## Cross-Boundary Classification

All changes are in `scripts/workflow/`, infrastructure-owned (story-runner tooling per
ADR-0035). No GSA paths touched (no `proto/**`, crypto primitives, `db/migrations/**`, or
wire-runtime coupling). `dt-story` is deliberately unchanged.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/workflow/run-story.sh` | Mine | — |
| `scripts/workflow/run-story.test.sh` | Mine | — |

## Security Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| (implementer to fill) | | |

## Validation

(Gate 2 results recorded here.)

## Review Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-FIXED | 6 | 6 | 0 |
| Test | RESOLVED-FIXED | 5 | 5 | 0 |

Security findings S-1..S-6: NO-COMMIT-EVIDENCE fail-closed (S-1, the masked-failure property),
gate-tail injection containment (S-2), RESTART-FLAG-AS-TEXT (S-3), DUPLICATE-FLAG / empty
`--stop-after=` regression (S-4), REVALIDATE-DIRTY-TREE tree-matches-commit check (S-5),
`prior_slug` canonical floor (S-6). Each pinned by a dedicated exit-2 test.
Test findings F1/F3/F4/F5 fixed (N7/N8/N10, heading normalization); F2 (RETRY_APPLIED one-shot)
FIXED not deferred via a real 2-task fixture (N16). Suite: 290 passed / 0 failed.

## Accepted Deferrals

None.
