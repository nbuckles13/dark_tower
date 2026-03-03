# Client Navigation

## Architecture & Design
- Client architecture (SDK, testing, deployment, E2EE) → ADR-0028
- Approved cryptographic algorithms → ADR-0027
- User auth and meeting access flows → ADR-0020
- Observability standards → ADR-0011
- Guards methodology → ADR-0015
- Test strategy → ADR-0005

## Code Locations (Planned)
- SDK core package → `packages/sdk-core/`
- Svelte adapter package → `packages/sdk-svelte/`
- Web application → `packages/web-app/`
- Shared test utilities → `packages/test-utils/`
- WASM crates (MLS) → `crates/browser-wasm/`
- Protobuf-es generated types → `packages/sdk-core/src/proto/`

## Protocol Integration
- Signaling proto (client-server) → `proto/signaling.proto`
- 42-byte binary frame format → `crates/media-protocol/src/frame.rs`
- Cross-language test vectors → `proto/test-vectors/`
- Protobuf-es codegen → `@bufbuild/protobuf-es` (wire compatible with Rust prost)

## Observability
- Client alert rules → `client-alerts.yaml`
- Client dashboards → `client-overview.json`, `client-slo.json`, `client-synthetic.json`

## CI & Deployment
- Client CI workflow → `.github/workflows/ci-client.yml`
- Debate record → `docs/debates/2026-02-28-client-architecture/debate.md`
