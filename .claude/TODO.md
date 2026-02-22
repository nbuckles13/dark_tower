# Technical Debt

## Cross-Service Duplication (DRY)

### From DRY Reviewer (Ongoing)

- [ ] **TD-9: Error response boilerplate**: `crates/ac-service/src/errors.rs`, `crates/gc-service/src/errors.rs`
- [ ] **TD-19: HTTP metrics middleware**: `crates/ac-service/src/middleware/http_metrics.rs`, `crates/gc-service/src/middleware/http_metrics.rs`
- [ ] **TD-11: Shutdown signal handlers**: `crates/*/src/main.rs`
- [ ] **Per-service observability duplication**: `crates/ac-service/src/observability/metrics.rs`, `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`

### From ADR-0010 Phase 4a Review (2026-01-31)

- [ ] **HealthStatus::from_proto() inconsistency**: MH uses inline match with `Pending` default, MC uses centralized method with `Unhealthy` default. Location: `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **gRPC input validation duplication**: MC and MH services duplicate validation logic (~100 lines). Locations: `crates/gc-service/src/grpc/mc_service.rs`, `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **Heartbeat interval constants**: Defined in 3 places with different names. Locations: `mc_service.rs`, `mh_service.rs`, `meeting-controller/gc_client.rs`

## Code Quality

- [ ] **dead_code lint cleanup**: Review `#[allow(dead_code)]` attributes across `crates/ac-service/src/` once more code paths are exercised by binaries
