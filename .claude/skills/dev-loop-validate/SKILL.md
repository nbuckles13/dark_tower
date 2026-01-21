---
name: dev-loop-validate
description: Run 7-layer verification to validate specialist's implementation. Run after /dev-loop-implement.
disable-model-invocation: true
---

# Dev-Loop Validate

Run the orchestrator's "trust but verify" check on the specialist's work. Even though the specialist should have run verification, we re-run it to ensure nothing was missed.

## Arguments

```
/dev-loop-validate [output-dir]
```

- **output-dir** (optional): Override auto-detected output directory

## Instructions

### Step 1: Locate Active Dev-Loop

If output-dir not provided, auto-detect:

1. List directories in `docs/dev-loop-outputs/` (excluding `_template`)
2. Filter to `Current Step` in (`implementation`, `validation`)
3. If exactly one: use it
4. If multiple: ask user which one
5. If none: error - "No dev-loop ready for validation."

### Step 2: Update Loop State

Update `main.md` Loop State:

| Field | Value |
|-------|-------|
| Current Step | `validation` |

### Step 3: Run 7-Layer Verification

Run each layer in sequence. Stop at first failure.

#### Layer 1: Compilation

```bash
cargo check --workspace
```

Exit code 0 = PASS

#### Layer 2: Formatting

```bash
cargo fmt --all --check
```

Exit code 0 = PASS

#### Layer 3: Simple Guards

```bash
./scripts/guards/run-guards.sh
```

Exit code 0 = PASS

#### Layer 4: Unit Tests

```bash
./scripts/test.sh --workspace --lib
```

Exit code 0 = PASS

#### Layer 5: All Tests (Integration)

```bash
./scripts/test.sh --workspace
```

Exit code 0 = PASS

#### Layer 6: Clippy Lints

```bash
cargo clippy --workspace -- -D warnings
```

Exit code 0 = PASS

#### Layer 7: Semantic Guards

Get list of modified Rust files and run semantic guards:

```bash
git diff --name-only HEAD~1 | grep '\.rs$' | xargs ./scripts/guards/semantic/credential-leak.sh
```

Exit codes:
- 0 = SAFE (PASS)
- 1 = UNSAFE (FAIL - must fix)
- 2 = UNCLEAR (manual review recommended)
- 3 = Script error

### Step 4: Update Verification Results in main.md

Update the "Dev-Loop Verification Steps" section in main.md with results:

```markdown
## Dev-Loop Verification Steps

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {relevant notes}

...etc for all 7 layers...
```

### Step 5: Report Results

#### If All Layers Pass

Update Loop State:

| Field | Value |
|-------|-------|
| Current Step | `code_review` |

Report:

```
**Validation Passed**

All 7 verification layers passed:
- Layer 1: cargo check ✓
- Layer 2: cargo fmt ✓
- Layer 3: Simple guards ✓
- Layer 4: Unit tests ✓
- Layer 5: Integration tests ✓
- Layer 6: Clippy ✓
- Layer 7: Semantic guards ✓

**Next step**: Run `/dev-loop-review`
```

#### If Any Layer Fails

Keep Loop State at `validation` (do not advance).

Report:

```
**Validation Failed**

Failed at Layer {N}: {layer name}

**Error Details**:
{failure output}

**Files likely affected**:
{list files from error output}

**Next step**: Run `/dev-loop-fix` to resume specialist with fix instructions
```

## Critical Constraints

- **--workspace flag**: All cargo/test commands MUST use --workspace
- **Sequential layers**: Run layers in order, stop at first failure
- **Update main.md**: Always update verification section regardless of pass/fail
- **No auto-fix**: Do NOT attempt to fix failures. Report them for `/dev-loop-fix`.

## Verification Script (Alternative)

If `./scripts/workflow/verify-dev-loop.sh` exists, you can use it:

```bash
./scripts/workflow/verify-dev-loop.sh --output-dir {output_dir} --verbose
```

This runs all layers and reports results.

---

**Next step**: Run `/dev-loop-review` (if passed) or `/dev-loop-fix` (if failed)
