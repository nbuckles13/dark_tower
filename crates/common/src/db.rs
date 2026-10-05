//! Shared Postgres connect path for the services that own a database (AC, GC).
//!
//! One home for the pool settings, the per-session `statement_timeout`, and
//! the credential-safe mapping of connect errors. The module does not log:
//! each service logs the returned [`DbConnectError`] under its own target, so
//! its `RUST_LOG` fallback filter still emits it.

use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

/// Pool and session settings for [`connect_pool`].
///
/// `Default` holds the current operational defaults, identical for AC and
/// GC. Callers pass the struct explicitly so the values have one home and can
/// become config-driven without touching the connect path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolSettings {
    /// Upper bound on open connections.
    pub max_connections: u32,
    /// Connections kept warm to reduce latency.
    pub min_connections: u32,
    /// How long `acquire` waits before failing fast on connection issues.
    pub acquire_timeout: Duration,
    /// Idle connections are closed after this long.
    pub idle_timeout: Duration,
    /// Connections are recycled after this long.
    pub max_lifetime: Duration,
    /// Server-side `statement_timeout` for every session, so a hung query
    /// fails fast instead of holding a connection indefinitely.
    pub statement_timeout: Duration,
}

impl Default for PoolSettings {
    fn default() -> Self {
        Self {
            max_connections: 20,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            statement_timeout: Duration::from_secs(5),
        }
    }
}

/// Error returned by [`connect_pool`].
///
/// `InvalidConfiguration` deliberately carries NO detail and no source. sqlx
/// 0.9 builds `Error::Configuration` from the connection settings themselves:
/// URL parse errors, and `SASLprep` failures whose text embeds a character of
/// the password (`sqlx-postgres` `connection/sasl.rs`). Dropping the original
/// is a credential-custody exception to error-context preservation; restoring
/// the context here reopens that leak. Every other variant cannot carry
/// credential material and keeps its full text and source.
#[derive(Debug, thiserror::Error)]
pub enum DbConnectError {
    /// The connection settings were rejected before or during authentication.
    #[error(
        "invalid database configuration (detail redacted: may contain credential material; check the DATABASE_URL secret)"
    )]
    InvalidConfiguration,
    /// Any other connect failure (I/O, TLS, database, pool timeout).
    #[error(transparent)]
    Connect(sqlx::Error),
}

impl DbConnectError {
    /// The only mapping from `sqlx::Error`. Deliberately not a `From` impl, so
    /// no `?` can bypass the `Configuration` redaction.
    fn from_sqlx(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::Configuration(_) => Self::InvalidConfiguration,
            other => Self::Connect(other),
        }
    }
}

/// Parse `database_url` and open an eagerly connected pool.
///
/// Startup is the only place a `Configuration` error can arise: the options
/// are fixed here, and every later connection the pool opens reuses them, so
/// `SASLprep` either fails on this first connect or never. Switching to
/// `connect_lazy`, or rotating credentials into a live pool, would move that
/// failure onto runtime query paths that log `sqlx::Error` unredacted.
///
/// # Errors
///
/// [`DbConnectError::InvalidConfiguration`] if the URL or credentials are
/// rejected, [`DbConnectError::Connect`] for any other connect failure.
pub async fn connect_pool(
    database_url: &str,
    settings: &PoolSettings,
) -> Result<PgPool, DbConnectError> {
    let options = PgConnectOptions::from_str(database_url).map_err(DbConnectError::from_sqlx)?;
    connect_pool_with(options, settings).await
}

// `connect_pool` for already-parsed options. Private: the only public entry
// is `connect_pool`, so every caller goes through the redacting URL parse.
//
// `statement_timeout` is appended after any `options=` the URL carried;
// Postgres applies startup `-c` switches in order, so this value wins.
async fn connect_pool_with(
    options: PgConnectOptions,
    settings: &PoolSettings,
) -> Result<PgPool, DbConnectError> {
    let options = options.options([(
        "statement_timeout",
        format!("{}ms", settings.statement_timeout.as_millis()),
    )]);

    PgPoolOptions::new()
        .max_connections(settings.max_connections)
        .min_connections(settings.min_connections)
        .acquire_timeout(settings.acquire_timeout)
        .idle_timeout(settings.idle_timeout)
        .max_lifetime(settings.max_lifetime)
        .connect_with(options)
        .await
        .map_err(DbConnectError::from_sqlx)
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    const SENTINEL: &str = "S3NT1NEL\u{0007}";

    #[test]
    fn configuration_error_is_redacted_in_display_debug_and_source() {
        let raw =
            sqlx::Error::Configuration(format!("Failed to saslprep password: {SENTINEL:?}").into());
        // Positive control: the raw error really carries the sentinel.
        assert!(raw.to_string().contains("S3NT1NEL"));
        assert!(format!("{raw:?}").contains("S3NT1NEL"));

        let err = DbConnectError::from_sqlx(raw);
        assert!(matches!(err, DbConnectError::InvalidConfiguration));
        for text in [err.to_string(), format!("{err:?}")] {
            assert!(!text.contains("S3NT1NEL"), "sentinel leaked: {text}");
            assert!(!text.contains('\u{0007}'), "control char leaked: {text}");
            assert!(
                !text.contains("\\u{7}"),
                "escaped control char leaked: {text}"
            );
        }
        assert!(err.source().is_none());
    }

    #[test]
    fn non_configuration_error_passes_through_with_source() {
        let raw = sqlx::Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "connection reset by peer",
        ));
        let raw_text = raw.to_string();

        let err = DbConnectError::from_sqlx(raw);
        assert!(matches!(err, DbConnectError::Connect(sqlx::Error::Io(_))));
        assert_eq!(err.to_string(), raw_text);
        assert!(err.source().is_some());
    }

    #[tokio::test]
    async fn malformed_url_is_redacted_through_the_real_parse_path() {
        let err = connect_pool(
            "postgres://user:S3NT1NEL@localhost:notaport/db",
            &PoolSettings::default(),
        )
        .await
        .expect_err("a non-numeric port must be rejected");

        assert!(matches!(err, DbConnectError::InvalidConfiguration));
        assert!(!format!("{err} {err:?}").contains("S3NT1NEL"));
    }

    async fn session_statement_timeout(pool: &PgPool) -> String {
        sqlx::query_scalar("SHOW statement_timeout")
            .fetch_one(pool)
            .await
            .expect("SHOW statement_timeout")
    }

    /// A plain single-connection pool: the control for what a URL/options
    /// value applies on its own, without `connect_pool`'s override.
    async fn control_pool(connect_options: PgConnectOptions) -> PgPool {
        PgPoolOptions::new()
            .max_connections(1)
            .connect_with(connect_options)
            .await
            .expect("control connect")
    }

    /// The production `DATABASE_URL`. `#[sqlx::test]` requires it, so a
    /// missing value fails loudly rather than skipping.
    fn database_url() -> String {
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set under #[sqlx::test]")
    }

    /// `url` plus an operator-style `options=-c statement_timeout=45s`.
    /// (45s, not 60s: Postgres would normalise 60s to `1min` in `SHOW`.)
    fn with_url_statement_timeout(url: &str) -> String {
        let separator = if url.contains('?') { '&' } else { '?' };
        format!("{url}{separator}options=-c%20statement_timeout%3D45s")
    }

    #[sqlx::test(migrations = false)]
    async fn connect_pool_applies_statement_timeout_via_the_url_path(_pool: PgPool) {
        let pool = connect_pool(&database_url(), &PoolSettings::default())
            .await
            .expect("connect");

        assert_eq!(session_statement_timeout(&pool).await, "5s");
    }

    #[sqlx::test(migrations = false)]
    async fn connect_pool_overrides_a_statement_timeout_in_the_url(_pool: PgPool) {
        let url = with_url_statement_timeout(&database_url());

        // Control: the URL value really applies on its own.
        let control = control_pool(PgConnectOptions::from_str(&url).expect("parse")).await;
        assert_eq!(session_statement_timeout(&control).await, "45s");

        let pool = connect_pool(&url, &PoolSettings::default())
            .await
            .expect("connect");
        assert_eq!(session_statement_timeout(&pool).await, "5s");
    }

    #[sqlx::test(migrations = false)]
    async fn connect_pool_with_overrides_a_statement_timeout_set_via_options(
        _pool_options: PgPoolOptions,
        connect_options: PgConnectOptions,
    ) {
        let connect_options = connect_options.options([("statement_timeout", "45s")]);

        // Control: the `.options()` value really applies on its own.
        let control = control_pool(connect_options.clone()).await;
        assert_eq!(session_statement_timeout(&control).await, "45s");

        let pool = connect_pool_with(connect_options, &PoolSettings::default())
            .await
            .expect("connect");
        assert_eq!(session_statement_timeout(&pool).await, "5s");
    }
}
