# Dark Tower

A video conferencing platform built with Rust and WebTransport.

## Overview

Dark Tower is an open-source video conferencing platform designed for scalability, performance, and user flexibility. It uses WebTransport over QUIC/HTTP/3 for media and signaling, WebCodecs in the browser, and SFrame end-to-end encryption, so the media handlers forward frames they cannot read. The code is written by AI agents.

> **Curious how we build with AI?** See [AI_DEVELOPMENT.md](AI_DEVELOPMENT.md) for our specialist agent teams, validation pipeline, and story-driven workflow.

## Architecture

| Component | Crate / package | Role |
|-----------|-----------------|------|
| Auth Controller (AC) | `crates/ac-service` | OAuth 2.0 client credentials, user auth, JWT issuance, JWKS, key rotation |
| Global Controller (GC) | `crates/gc-service` | HTTP/3 API entry point, meeting management, MC/MH registration and assignment |
| Meeting Controller (MC) | `crates/mc-service` | WebTransport signaling, sessions, participant roster, media routing decisions, key distribution |
| Media Handler (MH) | `crates/mh-service` | Keyless SFU: forwards encrypted media frames over WebTransport |
| Browser client | `packages/sdk-core`, `sdk-svelte`, `web-app` | TypeScript SDK and Svelte web app: join flow, WebCodecs, SFrame |

AC and GC are stateless and load-balanced; MC and MH are stateful and individually addressable. Services talk to each other over gRPC with contracts in `proto/`.

**Where it stands:** participants can join a meeting from the browser and hear each other, with end-to-end encrypted audio forwarded by the media handler. Multi-participant video is in progress ([see-each-other story](docs/user-stories/2026-10-07-see-each-other.md)). Every story, finished or planned, is in [docs/user-stories/](docs/user-stories/).

Design: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) · Decisions: [docs/decisions/](docs/decisions/)

## Project Structure

```
dark_tower/
├── crates/                 # Rust workspace
│   ├── ac-service/ gc-service/ mc-service/ mh-service/   # the four services
│   ├── common/             # shared Rust code
│   ├── media-protocol/     # media frame wire format
│   ├── proto-gen/          # generated protobuf code
│   ├── *-test-utils/       # per-service test helpers
│   ├── env-tests/          # tests against a live Kind cluster
│   ├── dt-guard/           # validation guards (ADR-0034)
│   ├── dt-story/           # story manifest tool (ADR-0035)
│   └── devloop-helper/     # host-side cluster helper for devloops
├── packages/               # TypeScript monorepo (pnpm + Nx): sdk-core, sdk-svelte, web-app, test-utils, proto-gen
├── proto/                  # Protocol Buffers contracts
├── migrations/             # PostgreSQL migrations
├── infra/
│   ├── kind/               # local Kind cluster setup and deploy
│   ├── kubernetes/ services/ grafana/   # manifests, service config, dashboards
│   └── devloop/            # containerized devloop tooling
├── scripts/                # validation pipeline (layer-all.sh → layer1..7.sh) and guards
├── .claude/                # agent definitions and skills
└── docs/                   # architecture, ADRs, runbooks, user stories, specialist knowledge
```

## Development

### Prerequisites

- **Rust** via [rustup](https://rustup.rs/). The toolchain is pinned in `rust-toolchain.toml` and installs automatically.
- **C toolchain**, `pkg-config`, `protoc` and the protobuf well-known types (`libprotobuf-dev` on Debian/Ubuntu). See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md#system-build-tools).
- **Node.js** at the version in `.nvmrc` (nvm recommended), and **pnpm** at the version pinned by `packageManager` in `package.json` (via corepack).
- **Podman** (or Docker), **kind** and **kubectl** for the local cluster.

On Ubuntu/WSL2:

```bash
sudo apt update && sudo apt install -y build-essential pkg-config protobuf-compiler libprotobuf-dev podman
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Getting Started

```bash
git clone https://github.com/nbuckles13/dark_tower.git
cd dark_tower
./.githooks/install.sh            # pre-commit: fmt + clippy

cargo build                       # Rust workspace
pnpm install --frozen-lockfile    # TypeScript workspace

./infra/kind/scripts/setup.sh     # local Kind cluster with all services, Postgres, Redis, observability
./scripts/dev-web.sh              # preflight checks, then the web-app dev server
```

Cluster details, observability, and troubleshooting: [docs/LOCAL_DEVELOPMENT.md](docs/LOCAL_DEVELOPMENT.md). Running the browser demo: [docs/runbooks/client-dev-local.md](docs/runbooks/client-dev-local.md).

### Validation

Every change passes the same 7-layer pipeline locally and in CI ([ADR-0033](docs/decisions/adr-0033-polyglot-validation-pipeline.md)):

```bash
./scripts/layer-fast.sh   # layers 1-6: compile, format, guards, tests, lint, audit
./scripts/layer-all.sh    # adds layer 7: env-tests and browser E2E against the Kind cluster
```

Production Rust code may not panic: `unwrap`, `expect`, `panic!` and unchecked indexing are denied at compile time ([ADR-0002](docs/decisions/adr-0002-no-panic-policy.md)). Dependencies are audited on every lockfile change and weekly.

Coverage: [Codecov](https://app.codecov.io/gh/nbuckles13/dark_tower).

## Contributing

All code is written by AI agents through the [devloop](.claude/skills/devloop/SKILL.md) and [user-story](.claude/skills/user-story/SKILL.md) workflows. Contributions should follow the same patterns and pass the same pipeline.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
