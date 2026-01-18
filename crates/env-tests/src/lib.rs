//! Environment Integration Test Suite
//!
//! This crate provides integration tests for the Dark Tower local development environment.
//! Tests validate that the Kubernetes deployment, observability stack, and service flows
//! work correctly against actual deployment artifacts.
//!
//! # Features
//!
//! - `smoke`: Fast cluster health checks (30s)
//! - `flows`: Service flow tests (2-3min), including cross-service AC+GC flows
//! - `observability`: Metrics and logs validation (Loki optional)
//! - `resilience`: Pod restart and chaos tests (5min+)
//! - `all`: Enable all test categories
//!
//! # Prerequisites
//!
//! 1. Kind cluster running: `./infra/kind/scripts/setup.sh`
//! 2. Port-forwards active: AC (8082), GC (8080), Prometheus (9090), Grafana (3000), Loki (3100 optional)
//! 3. kubectl in PATH for NetworkPolicy diagnostics
//!
//! # Usage
//!
//! ```bash
//! # From repo root - runs 0 env-tests (no default features)
//! cargo test
//!
//! # Smoke tests only (30s)
//! cargo test -p env-tests --features smoke
//!
//! # Smoke + service flows (3min)
//! cargo test -p env-tests --features smoke,flows
//!
//! # Pre-deploy validation - full suite (8-10min)
//! cargo test -p env-tests --features all
//! ```

pub mod canary;
pub mod cluster;
pub mod eventual;
pub mod fixtures;
