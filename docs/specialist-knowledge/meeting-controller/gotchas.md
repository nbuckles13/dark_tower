# Meeting Controller Gotchas

Mistakes to avoid and edge cases discovered in the Meeting Controller codebase.

---

## Gotcha: Clippy Warns on Excessive Bools in Proto-Generated Code
**Added**: 2026-01-25
**Related files**: `crates/proto-gen/src/lib.rs`, `proto/signaling.proto`

Proto messages with multiple boolean fields (e.g., `is_self_muted`, `is_host_muted`, `is_video_enabled`) trigger Clippy's `fn_params_excessive_bools` lint on generated code. Add `#![allow(clippy::fn_params_excessive_bools)]` to `proto-gen/src/lib.rs` since we can't control prost's code generation. This is acceptable - proto field types are dictated by protocol design, not Rust ergonomics.

---

## Gotcha: Dead Code Warnings in Skeleton Crates
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/config.rs`, `crates/meeting-controller/src/error.rs`

Skeleton crates defining types for future use trigger dead_code warnings. Add `#[allow(dead_code)]` with explanatory comment: `// Skeleton: will be used in Phase 6b`. Do NOT use `#[expect(dead_code)]` - it warns when the code IS eventually used, requiring removal. The `#[allow(...)]` attribute silently permits unused code without complaining when it becomes used.

---

## Gotcha: Doc Markdown Lint Requires Backticks
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/*.rs`

Rustdoc markdown linter requires backticks around code identifiers in doc comments. Write `/// Validates the \`session_token\` field` not `/// Validates the session_token field`. Also use backticks for: type names, field names, function names, enum variants, and file paths. The `cargo doc` command will warn but not fail; `cargo clippy` with `warn(rustdoc::all)` makes these errors.

---

## Gotcha: New Crates Must Be Added to Workspace Members
**Added**: 2026-01-25
**Related files**: `Cargo.toml`, `crates/mc-test-utils/Cargo.toml`

When creating new crates like `mc-test-utils`, you must add them to the workspace `members` array in the root `Cargo.toml`. Forgetting this causes: `cargo build` ignores the crate, `cargo test --workspace` skips its tests, and inter-crate dependencies fail to resolve. Always verify with `cargo metadata --no-deps | jq '.workspace_members'` after adding a new crate.

---
