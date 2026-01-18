//! Test fixtures for interacting with cluster services.

pub mod auth_client;
pub mod gc_client;
pub mod metrics;

pub use auth_client::AuthClient;
pub use gc_client::GcClient;
pub use metrics::PrometheusClient;
