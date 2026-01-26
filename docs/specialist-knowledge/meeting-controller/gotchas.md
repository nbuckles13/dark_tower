# Meeting Controller Gotchas

Mistakes to avoid and edge cases discovered in the Meeting Controller codebase.

---

## Gotcha: Borrow Checker Blocks Broadcast After Mutable Update
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/meeting.rs`

When updating participant state (e.g., mute status) then broadcasting to other participants, you cannot hold a mutable borrow of `self.participants` while calling `self.broadcast_update()`. Solution: extract the update into a local variable inside the `if let Some(participant) = self.participants.get_mut(id)` block, then broadcast after the block closes. Pattern: `let update = if let Some(p) = self.participants.get_mut(id) { p.field = value; Some(Update { ... }) } else { None }; if let Some(u) = update { self.broadcast(u).await; }`

---

## Gotcha: Don't Include IDs in Error Messages (MINOR-002)
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/meeting.rs`, `crates/meeting-controller/src/actors/controller.rs`

Avoid including meeting IDs, participant IDs, or user IDs in error messages returned to clients. These can leak information for enumeration attacks. Use generic messages like "Participant not found" or "Meeting already exists" instead of "Participant part-123 not found". Log the full details server-side for debugging.

---

## Gotcha: StoredBinding TTL Uses Instant, Not SystemTime
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/session.rs`

`StoredBinding::is_expired()` uses `Instant::now()` and `elapsed()` which is monotonic and immune to system clock changes. This is intentional - TTL checks should use monotonic time. However, `tokio::time::pause()` works with Tokio's internal clock, not `std::time::Instant`. For tests needing expired bindings, either: (1) use `#[tokio::test(start_paused = true)]` which affects both, or (2) construct binding with a custom `created_at` in the past.

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
