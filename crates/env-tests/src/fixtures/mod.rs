//! Test fixtures for interacting with cluster services.

pub mod alert_rules_loaded;
pub mod auth_client;
pub mod collector;
pub mod deployment_config;
pub mod egress_admission;
pub mod gate_zero;
pub mod gc_client;
pub mod gc_fleet;
pub mod injection;
pub mod kube;
pub mod loki;
pub mod mc_session;
pub mod media;
pub mod meeting_end;
pub mod metric_hygiene;
pub mod metrics;
pub mod mh_address;
pub mod mh_close;
pub mod mh_config;
pub mod mh_grpc;
pub mod mh_pool;
pub mod owned_placement;
pub mod participant;
pub mod video;

pub use auth_client::AuthClient;
pub use gc_client::GcClient;
pub use metrics::PrometheusClient;
