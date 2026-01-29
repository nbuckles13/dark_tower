# DRY Reviewer - Gotchas to Avoid

Pitfalls encountered during cross-service duplication review in Dark Tower.

---

## Gotcha: Over-Blocking on Established Tech Debt
**Added**: 2026-01-15
**Related files**: `docs/decisions/adr-0019-dry-reviewer.md`, `docs/specialist-knowledge/dry-reviewer/integration.md`

When reviewing code that duplicates existing patterns, check the tech debt registry BEFORE marking as BLOCKER. If the pattern has a TD-N ID, classify as TECH_DEBT instead. Example: JWT signing duplication (TD-1) spans ac-service and global-controller - marking as BLOCKER would halt progress on legitimate features.

---

## Gotcha: Recommending Extraction in Code Review
**Added**: 2026-01-15
**Related files**: `docs/decisions/adr-0019-dry-reviewer.md`

For TECH_DEBT duplication, do NOT recommend extraction as a code review action. This blocks feature progress. Instead: (1) Document in tech debt registry with TD-N ID, (2) Let architectural refactoring be planned separately in future phases, (3) Reference in .claude/TODO.md if scheduled. Extraction is a follow-up task, not a review gate.

---

## Gotcha: Security Code Duplication Requires Escalation
**Added**: 2026-01-15
**Related files**: `.claude/agents/security.md`

Never compromise security checks to reduce duplication. When duplication involves cryptographic code, authentication, or authorization: (1) Escalate to Security specialist for assessment, (2) Security may prefer duplication over coupling, (3) Document security rationale in integration.md. Duplicate security code is safer than insecure shortcuts for DRY compliance.

---

## Gotcha: Rust Trait Extraction Threshold
**Added**: 2026-01-15
**Related files**: `crates/common/src/`

Only recommend trait extraction when 3+ similar implementations exist. Rust trait bounds increase complexity (generics, associated types, where clauses) that may outweigh DRY benefits for just 2 implementations. For 2 implementations, classify as TECH_DEBT and reassess when a third appears.

---

## Gotcha: Tokio Background Task Shutdown Patterns
**Added**: 2026-01-24
**Related files**: `crates/global-controller/src/mh_health_checker.rs`, `crates/ac-service/src/background/`

Background tasks using Tokio typically share similar shutdown patterns: `select!` on cancellation token + task loop, cleanup on shutdown, tracing spans. Do NOT flag these as duplication requiring extraction - the similarity is inherent to the Tokio async runtime model. Mark as ACCEPTABLE. Only flag if business logic within the task is duplicated, not the infrastructure pattern around it.

---

## Gotcha: Tokio Actor Pattern is Service-Specific
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/session/actor.rs`

Do NOT flag Tokio actor implementations (task + mpsc channel + message enum) as candidates for extraction to common/. Actor patterns are inherently service-specific: the message types, state machine logic, and business rules are unique to each service's domain. The infrastructure pattern (spawn task, receive messages, handle each variant) is idiomatic Tokio, not duplication. Only flag if two services have identical business logic within their actors.

---

## Gotcha: Security Wrapper Response Types Need Duplication Context
**Added**: 2026-01-28
**Related files**: `crates/ac-service/src/models/mod.rs`

When reviewing custom Debug/Serialize implementations for SecretString/SecretBox response fields, DO NOT automatically flag as duplication when the same pattern appears across multiple response types (e.g., 3 response types with identical client_secret handling). This is acceptable single-service duplication. The implementations are intentionally simple (2-3 lines each) to maintain security clarity. Only escalate to TECH_DEBT if: (1) the pattern spans a SECOND service, or (2) more than 4 response types in same service require identical handling. Context: Security-critical boilerplate is intentionally terse per OWASP guidelines; extracting prematurely obscures intent.

---
