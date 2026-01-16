# Step: Validation

This file is read when entering the `validation` step of the dev-loop.

---

## Specialist Runs Verification

The implementing specialist is responsible for running verification and fixing failures. This keeps context intact and ensures the specialist learns from its own mistakes.

**Verification layers** (specialist runs all 7):

| Layer | Command | Purpose |
|-------|---------|---------|
| 1 | `cargo check --workspace` | Compilation (entire workspace) |
| 2 | `cargo fmt --all --check` | Formatting check |
| 3 | `./scripts/guards/run-guards.sh` | Simple guards |
| 4 | `./scripts/test.sh --workspace --lib` | Unit tests (entire workspace) |
| 5 | `./scripts/test.sh --workspace` | All tests (entire workspace) |
| 6 | `cargo clippy --workspace -- -D warnings` | Lints (entire workspace) |
| 7 | `./scripts/guards/semantic/credential-leak.sh {files}` | Semantic guards |

**CRITICAL**: Always use `--workspace` for cargo/test commands. Changes in one crate can break others.

**Note**: `./scripts/test.sh` is a wrapper that:
1. Starts the test database if not running (podman or docker)
2. Applies database migrations automatically
3. Sets `DATABASE_URL` and runs `cargo test` with all provided arguments

The specialist iterates until all pass, documenting results in the output file.

---

## Semantic Guards (Layer 7)

Semantic guards MUST always run on all new and modified Rust files. Never skip with "N/A".

**How to run**:
```bash
# Run on all modified .rs files
git diff --name-only HEAD~1 | grep '\.rs$' | xargs ./scripts/guards/semantic/credential-leak.sh
```

**Output table format** (must show PASSED or FAILED, never N/A):
```
| Semantic guards | PASSED | credential-leak.sh on 5 files |
```

**Exit codes**:
- `0` = SAFE (no credential leak risks)
- `1` = UNSAFE (risks detected - must fix)
- `2` = UNCLEAR (manual review recommended)
- `3` = Script error

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
