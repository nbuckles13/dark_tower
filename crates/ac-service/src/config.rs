use base64::{engine::general_purpose, Engine as _};
use std::env;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_address: String,
    pub master_key: Vec<u8>,
    #[expect(dead_code)] // Will be used in Phase 4 for observability
    pub otlp_endpoint: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Invalid master key format: {0}")]
    InvalidMasterKey(String),

    #[error("Base64 decode error: {0}")]
    Base64Error(#[from] base64::DecodeError),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingEnvVar("DATABASE_URL".to_string()))?;

        let bind_address = env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8082".to_string());

        let master_key_base64 = env::var("AC_MASTER_KEY")
            .map_err(|_| ConfigError::MissingEnvVar("AC_MASTER_KEY".to_string()))?;

        let master_key = general_purpose::STANDARD
            .decode(&master_key_base64)
            .map_err(ConfigError::Base64Error)?;

        if master_key.len() != 32 {
            return Err(ConfigError::InvalidMasterKey(format!(
                "Expected 32 bytes, got {}",
                master_key.len()
            )));
        }

        let otlp_endpoint = env::var("OTLP_ENDPOINT").ok();

        Ok(Config {
            database_url,
            bind_address,
            master_key,
            otlp_endpoint,
        })
    }
}
