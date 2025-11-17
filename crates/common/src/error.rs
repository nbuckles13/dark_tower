//! Common error types for Dark Tower components.

use thiserror::Error;

/// Common errors that can occur across Dark Tower components
#[derive(Error, Debug)]
pub enum DarkTowerError {
    /// Database operation failed
    #[error("Database error: {0}")]
    Database(String),

    /// Redis operation failed
    #[error("Redis error: {0}")]
    Redis(String),

    /// Network transport error
    #[error("Transport error: {0}")]
    Transport(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Invalid configuration
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Unauthorized access
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias using `DarkTowerError`
pub type Result<T> = std::result::Result<T, DarkTowerError>;
