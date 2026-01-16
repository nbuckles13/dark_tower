# Step: Validation

This file is read when entering the `validation` step of the dev-loop.

---

## Specialist Runs Verification

The implementing specialist is responsible for running verification and fixing failures. This keeps context intact and ensures the specialist learns from its own mistakes.

**Verification layers** (specialist runs all 7):

| Layer | Command | Purpose |
|-------|---------|---------|
| 1 | `cargo check` | Compilation |
| 2 | `cargo fmt` | Auto-formatting |
| 3 | `./scripts/guards/run-guards.sh` | Simple guards |
| 4 | `cargo test --lib` | Unit tests |
| 5 | `cargo test` | All tests |
| 6 | `cargo clippy -- -D warnings` | Lints |
| 7 | `./scripts/guards/semantic/credential-leak.sh {files}` | Semantic guards |

The specialist iterates until all pass, documenting results in the output file.

---

## Orchestrator Validates (Trust but Verify)

After the specialist returns, the orchestrator re-runs verification:

```bash
./scripts/workflow/verify-dev-loop.sh --output-dir docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}
```

Or run the standard verification:
```bash
./scripts/verify-completion.sh --verbose
```

**Exit codes**:
- 0 = Confirmed passing → Continue to code review
- 1 = Failed → Resume specialist to fix

This catches cases where the specialist skipped steps or didn't fully fix issues.

---

## Validating the Output File

The orchestrator also checks that the output file has required sections:

```bash
grep -q "## Verification Results" docs/dev-loop-outputs/YYYY-MM-DD-*.md
```

If verification results are missing from the output file, the specialist skipped that step - resume to fix.

---

## Resume for Fixes

When verification fails after specialist returns:

```markdown
## Verification Failed

The orchestrator re-ran verification and it failed:

**Failed at**: {layer}
**Output**:
{failure details}

Your output file shows verification passed, but re-running shows failures.
Please investigate, fix the issues, re-run verification, and update the output file.
```

---

## State Transition

**On validation pass**: Update Loop State to `code_review`, proceed to code review step.

**On validation fail**: Keep Loop State at `validation`, resume specialist with fix instructions.
