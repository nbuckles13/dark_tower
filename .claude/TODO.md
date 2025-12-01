# Technical Debt and Future Work

## Low Priority

### Clean up dead_code lints (Phase 5+)
Once more of the system is implemented and library functions are actually used by binaries:
- Review all `#[allow(dead_code)]` attributes
- Replace with `#[expect(dead_code)]` where appropriate
- Remove attributes entirely for code that's now in use
- Consider splitting library into smaller modules if dead code patterns persist

**Why deferred**: Currently many library functions are tested but not used by binaries yet. The dead_code lint situation will resolve naturally as we implement Phase 4+ features (admin endpoints, audit endpoints, key rotation, etc).

**Files affected**: 
- `crates/ac-service/src/config.rs`
- `crates/ac-service/src/models/mod.rs`
- `crates/ac-service/src/repositories/*.rs`
- `crates/ac-service/src/services/*.rs`
