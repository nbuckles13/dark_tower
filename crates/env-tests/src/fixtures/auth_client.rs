//! Authentication client fixture for token issuance and JWKS operations.

use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Once};
use thiserror::Error;
use uuid::Uuid;

/// Default password for test users (meets AC's 8-char minimum).
pub const TEST_USER_PASSWORD: &str = "test-env-password-42";

/// Environment variable carrying the organization this run registers users into.
///
/// REQUIRED, with NO fallback and NO default — see [`resolve_org_subdomain`].
pub const ORG_SUBDOMAIN_VAR: &str = "ENV_TEST_ORG_SUBDOMAIN";

// ANCHOR (DRY): the org-subdomain shape, copied VERBATIM (not paraphrased) from the
// schema's own CHECK at `migrations/20250118000001_initial_schema.sql:16`, which is the
// SSoT. The same literal is mirrored at several further sites; the AUTHORITATIVE INVENTORY
// is the `SITES` table in `scripts/guards/simple/validate-subdomain-regex-sync.sh`, and it
// is deliberately NOT re-listed here — a hand-copied inventory is itself a mirror, and it
// drifted exactly that way once (`docs/DATABASE_SCHEMA.md` was added to the guard's table
// while this list still read "all six"). Byte-identity across every enumerated site is
// ENFORCED by that guard (CLAUDE.md's "derive one from the other, or add a guard that
// fails validation on drift"), so this copy is checked rather than trusted.
//
// Deliberately the SAME literal rather than a new hand-rolled predicate: AC's own
// `org_extraction.rs::extract_subdomain` is already a hand-rolled, weaker variant of this
// rule (it does not bound the length), and a second invented-but-equivalent formulation
// would be uncheckable against anything.
const SUBDOMAIN_PATTERN: &str = r"^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$";

/// Compiled [`SUBDOMAIN_PATTERN`], built once. Same `LazyLock<Regex>` canonical-home shape
/// this crate already uses for `JWT_PATTERN` / `BEARER_PATTERN` in `gc_client.rs`.
#[expect(
    clippy::disallowed_methods,
    reason = "test fixture LazyLock<Regex>; static pattern compiles or load-time panic. Per ADR-0034 §6 + ADR-0002 §expect-over-allow — same canonical-home discipline as crates/dt-guard/."
)]
static SUBDOMAIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(SUBDOMAIN_PATTERN).unwrap());

/// The per-run organization subdomain could not be resolved.
///
/// A distinct type rather than a variant of [`AuthClientError`] so
/// [`resolve_org_subdomain`] stays a pure, independently testable function; it converts
/// into `AuthClientError` at the one call site that needs it.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct OrgSubdomainError(String);

/// Authentication client errors.
#[derive(Debug, Error)]
pub enum AuthClientError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Token issuance failed with status {status}: {body}")]
    IssuanceFailed { status: u16, body: String },

    #[error("JWKS fetch failed: {0}")]
    JwksFetchFailed(String),

    #[error("JSON deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("{0}")]
    OrgSubdomainUnresolved(#[from] OrgSubdomainError),
}

/// Resolve the per-run organization subdomain from an already-read raw value.
///
/// PURE by construction — the caller does the `std::env::var`, so the unit tests below
/// exercise every branch without mutating process-global environment (no `#[serial]`, no
/// cross-test interference).
///
/// # Why there is no default
///
/// R-7: no production code ever marks a meeting ended, so an organization's live-meeting
/// count only CLIMBS toward `max_concurrent_meetings`. A static organization therefore
/// makes run N's verdict a function of runs 1..N-1 — green on the first run of the day, a
/// 403 later, attributed to the code under test. `scripts/layer7.sh` Phase 1h provisions a
/// FRESH organization per run and exports it here.
///
/// A `unwrap_or("devtest")` fallback would be that same defect wearing a default: the
/// suite would go green while the fix sat inert. So unset AND blank both fail loudly.
/// `devtest` remains reachable only through the EXPLICIT manual invocation named in the
/// error message below (and seeded by `infra/kind/scripts/setup.sh:seed_test_data`, which
/// carries the matching half of this cross-reference).
///
/// Follows `crates/env-tests/src/cluster.rs::read_env_url`'s conventions — empty string
/// treated as unset, validation eager rather than at first use — with a subdomain
/// validator in place of the URL one.
pub fn resolve_org_subdomain(raw: Option<&str>) -> Result<String, OrgSubdomainError> {
    // `Some("")` collapses to the unset case exactly as `read_env_url` does: an exported
    // -but-empty variable is what a failed command substitution in a shell wrapper
    // produces, and treating it as "present" would push a blank Host label to AC.
    let value = match raw {
        None => return Err(OrgSubdomainError(unset_message("unset"))),
        Some("") => return Err(OrgSubdomainError(unset_message("set but empty"))),
        Some(v) => v,
    };

    // Validated as given, NOT trimmed first: silently repairing " foo " into "foo" would
    // resolve a DIFFERENT organization than the one the operator exported.
    if !SUBDOMAIN_RE.is_match(value) {
        return Err(OrgSubdomainError(format!(
            "{ORG_SUBDOMAIN_VAR}=\"{value}\" is not a valid organization subdomain.\n\
             Expected a DNS label matching {SUBDOMAIN_PATTERN} (ASCII lowercase letters, \
             digits and internal hyphens, 1-63 characters) — the same rule the database \
             enforces as the `subdomain_format` CHECK on `organizations`.\n\
             AC resolves the organization from the Host header and fails closed on any \
             other shape, so catching it here turns an unattributable 400/404 mid-suite \
             into a named failure before the first request."
        )));
    }

    Ok(value.to_string())
}

/// The fail-loud message shared by the unset and blank branches.
fn unset_message(state: &str) -> String {
    format!(
        "{ORG_SUBDOMAIN_VAR} is required but {state}.\n\
         `scripts/layer7.sh` Phase 1h provisions a fresh organization for every layer-7 \
         run and exports this variable to the suite; if you are seeing this inside a \
         devloop, Phase 1h did not run or did not export.\n\
         For a STANDALONE run, either provision one \
         (`infra/kind/scripts/setup.sh --provision-org <subdomain>`) and export the \
         subdomain it reports, or use the seeded development organization explicitly:\n\
         \n    {ORG_SUBDOMAIN_VAR}=devtest cargo test -p env-tests\n\n\
         There is deliberately NO default: silently falling back to a static organization \
         re-creates the cross-run meeting-cap exhaustion (R-7) this variable exists to \
         remove, with every gate still green."
    )
}

/// Read and validate [`ORG_SUBDOMAIN_VAR`] from the process environment.
///
/// Thin wrapper over the pure [`resolve_org_subdomain`]. Emits the same
/// `[env-tests] VAR = … (from env)` provenance line `cluster.rs` emits for each URL, once
/// per process so a suite of ~4 registrations does not repeat it.
fn org_subdomain() -> Result<String, OrgSubdomainError> {
    // NOT `.ok()`, deliberately (@semantic-guard). `.ok()` collapses `VarError::NotUnicode` into
    // `None`, so a variable that IS set to non-UTF-8 bytes would report "required but unset" and
    // send the operator hunting a Phase-1h export that demonstrably happened. That is the
    // wrong-lane diagnostic this entire story exists to remove, so it does not get to survive in
    // the fixture that reads the story's own variable — near-unreachable or not.
    // `cluster.rs::read_env_url` uses the `.ok()` shape; this deliberately diverges rather than
    // matching a precedent whose weakness is the exact class under repair here.
    let raw = match std::env::var(ORG_SUBDOMAIN_VAR) {
        Ok(v) => Some(v),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(bytes)) => {
            return Err(OrgSubdomainError(format!(
                "{ORG_SUBDOMAIN_VAR} is SET but its value is not valid UTF-8 ({bytes:?}).\n\
                 This is NOT the same as unset: something did export the variable, so do not go \
                 looking for a missing `scripts/layer7.sh` Phase-1h export. A generated \
                 subdomain is ASCII by construction (`e2e-<16 hex>`), so non-UTF-8 bytes here \
                 mean the value was corrupted in transit or set by hand."
            )));
        }
    };
    let resolved = resolve_org_subdomain(raw.as_deref())?;
    static LOGGED: Once = Once::new();
    LOGGED.call_once(|| {
        eprintln!("[env-tests] {ORG_SUBDOMAIN_VAR} = {resolved} (from env)");
    });
    Ok(resolved)
}

/// OAuth 2.0 token request.
#[derive(Debug, Serialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl TokenRequest {
    /// Create a new client credentials token request with optional scope.
    pub fn client_credentials(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        let scope_str = scope.into();
        Self {
            grant_type: "client_credentials".to_string(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            scope: if scope_str.is_empty() {
                None
            } else {
                Some(scope_str)
            },
        }
    }
}

/// OAuth 2.0 token response (service-token / client_credentials endpoint).
///
/// Mirror of AC's `crates/ac-service/src/models/mod.rs:TokenResponse`. Under task #51's single rule,
/// the wire is uniform camelCase (`accessToken`/`tokenType`/`expiresIn`/`scope`) — flipped in lockstep
/// with the server struct AND the production consumer `common::token_manager::OAuthTokenResponse`.
/// Rust field idents stay snake_case (`rename_all` flips only the wire keys).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
}

/// JWKS (JSON Web Key Set) response.
#[derive(Debug, Deserialize, Clone)]
pub struct JwksResponse {
    pub keys: Vec<JwkKey>,
}

/// A single JWK (JSON Web Key).
#[derive(Debug, Deserialize, Clone)]
pub struct JwkKey {
    pub kty: String,
    pub kid: String,
    pub alg: Option<String>,
    pub crv: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    #[serde(rename = "use")]
    pub key_use: Option<String>,
}

/// Client for interacting with the Authentication Controller service.
pub struct AuthClient {
    base_url: String,
    http_client: Client,
    /// Count of AC requests issued through THIS instance — see [`AuthClient::call_count`].
    calls: AtomicUsize,
}

impl AuthClient {
    /// Create a new authentication client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http_client: Client::new(),
            calls: AtomicUsize::new(0),
        }
    }

    /// Number of AC requests this client instance has issued.
    ///
    /// Exists so a test can assert on calls that ACTUALLY happened rather than on a
    /// value it hopes did not change. Added for story task #58, where the token-only
    /// join property needs a net that fails under the regression shape it targets:
    /// a re-auth regression introduces a NEW binding (`let token2 = register(...)`)
    /// and leaves the original untouched, so any assertion comparing the original
    /// token to a saved copy of itself passes while the property is gone. Counting
    /// requests observes the regression; comparing a value to its own clone cannot.
    ///
    /// **Residual, stated rather than overclaimed**: this counts calls through this
    /// INSTANCE. An edit that constructs a second `AuthClient` evades it. That is a
    /// far less natural regression than adding a call on the client already in hand,
    /// and it is a real net where there was none.
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    /// Issue a token using client credentials.
    pub async fn issue_token(
        &self,
        request: TokenRequest,
    ) -> Result<TokenResponse, AuthClientError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        // AC service token endpoint is at /api/v1/auth/service/token
        let token_url = format!("{}/api/v1/auth/service/token", self.base_url);

        let response = self
            .http_client
            .post(&token_url)
            .json(&request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AuthClientError::IssuanceFailed {
                status: status.as_u16(),
                body,
            });
        }

        let token_response = response.json::<TokenResponse>().await?;
        Ok(token_response)
    }

    /// Fetch the JWKS (JSON Web Key Set) from the service.
    pub async fn fetch_jwks(&self) -> Result<JwksResponse, AuthClientError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        // AC service JWKS endpoint is at /.well-known/jwks.json
        let jwks_url = format!("{}/.well-known/jwks.json", self.base_url);

        let response = self.http_client.get(&jwks_url).send().await?;

        if !response.status().is_success() {
            return Err(AuthClientError::JwksFetchFailed(format!(
                "Status: {}",
                response.status()
            )));
        }

        let jwks = response.json::<JwksResponse>().await?;
        Ok(jwks)
    }

    /// Get the HTTP client for custom requests.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }

    /// Get the base URL for the authentication service.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Register a new user and return the registration response with auto-login token.
    ///
    /// The registration endpoint requires org context from the `Host` header. The
    /// organization is the PER-RUN one provisioned by `scripts/layer7.sh` Phase 1h and
    /// read from [`ORG_SUBDOMAIN_VAR`] — never a static `devtest`/`demo` constant, which
    /// is R-7 (see [`resolve_org_subdomain`]).
    ///
    /// # Arguments
    ///
    /// * `request` - Registration request with email, password, display_name
    ///
    /// # Errors
    ///
    /// Returns [`AuthClientError::OrgSubdomainUnresolved`] BEFORE issuing any request when
    /// [`ORG_SUBDOMAIN_VAR`] is unset, blank or malformed.
    pub async fn register_user(
        &self,
        request: &UserRegistrationRequest,
    ) -> Result<UserRegistrationResponse, AuthClientError> {
        // Resolved BEFORE the call counter moves and before any network I/O: a missing
        // organization is a configuration fault, not an AC interaction, and must not be
        // counted as one by `call_count()`.
        let subdomain = org_subdomain()?;
        self.calls.fetch_add(1, Ordering::Relaxed);
        let register_url = format!("{}/api/v1/auth/register", self.base_url);

        // Extract host and port from base_url for the Host header.
        // AC's org extraction middleware requires subdomain.domain format.
        let host_header = build_org_host_header(&self.base_url, &subdomain);

        let response = self
            .http_client
            .post(&register_url)
            .header("Host", &host_header)
            .json(request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AuthClientError::IssuanceFailed {
                status: status.as_u16(),
                body,
            });
        }

        let registration_response = response.json::<UserRegistrationResponse>().await?;
        Ok(registration_response)
    }
}

/// Build a Host header with org subdomain for AC's org extraction middleware.
///
/// Given base_url `http://localhost:8082` and subdomain `devtest`,
/// produces `devtest.localhost:8082`.
fn build_org_host_header(base_url: &str, subdomain: &str) -> String {
    // Strip scheme
    let without_scheme = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))
        .unwrap_or(base_url);

    // Strip trailing slash
    let host = without_scheme.trim_end_matches('/');

    // Format as subdomain.host[:port]
    format!("{}.{}", subdomain, host)
}

/// User registration request.
///
/// Sent to AC's `POST /api/v1/auth/register` endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRegistrationRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

impl UserRegistrationRequest {
    /// Create a registration request with a unique UUID-based email.
    ///
    /// Uses a UUID in the email to prevent collisions across test runs.
    pub fn unique(display_name: impl Into<String>) -> Self {
        Self {
            email: format!("test-{}@envtest.dev", Uuid::new_v4()),
            password: TEST_USER_PASSWORD.to_string(),
            display_name: display_name.into(),
        }
    }
}

/// User registration response.
///
/// Returned by AC's `POST /api/v1/auth/register` endpoint.
/// Contains an auto-login user JWT in `access_token` (Rust ident; wire key is `accessToken`).
///
/// Mirror of `crates/ac-service/src/handlers/auth_handler.rs:UserRegistrationResponse`;
/// uses the SAME uniform camelCase wire shape — task #51 removed the former per-field
/// OAuth snake_case carve-out on this user-flow endpoint, so all wire keys are camelCase
/// (`userId`/`email`/`displayName`/`accessToken`/`tokenType`/`expiresIn`). Rust field idents
/// stay snake_case (the `rename_all` flips only the wire keys).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRegistrationResponse {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

impl std::fmt::Debug for UserRegistrationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserRegistrationResponse")
            .field("user_id", &self.user_id)
            .field("email", &self.email)
            .field("display_name", &self.display_name)
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === resolve_org_subdomain (R-7) =====================================================
    //
    // Every case calls the PURE function with an explicit `Option<&str>`, so none of them
    // touch process-global environment: no `#[serial]`, no ordering dependence, and no way
    // for one case to leak a value into another. That is the reason the env read was split
    // out of the resolver in the first place.

    #[test]
    fn test_resolve_org_subdomain_accepts_a_valid_generated_subdomain() {
        // The exact shape scripts/layer7.sh's __generate_org_subdomain emits.
        let resolved = resolve_org_subdomain(Some("e2e-0123456789abcdef"))
            .expect("a well-formed per-run subdomain must resolve");
        assert_eq!(resolved, "e2e-0123456789abcdef");
    }

    #[test]
    fn test_resolve_org_subdomain_accepts_the_documented_manual_escape_hatch() {
        // `ENV_TEST_ORG_SUBDOMAIN=devtest cargo test -p env-tests` is the invocation the
        // unset-error names; if it did not validate, that message would send an operator
        // to a command that cannot work.
        assert_eq!(
            resolve_org_subdomain(Some("devtest")).expect("devtest must remain valid"),
            "devtest"
        );
    }

    #[test]
    fn test_resolve_org_subdomain_rejects_unset() {
        let err = resolve_org_subdomain(None).expect_err("unset must NOT fall back to a default");
        let msg = err.to_string();
        // The three things an operator needs, per the no-fallback ruling: the variable, who
        // normally sets it, and how to run standalone.
        assert!(
            msg.contains("ENV_TEST_ORG_SUBDOMAIN"),
            "error must name the variable: {msg}"
        );
        assert!(
            msg.contains("layer7.sh") && msg.contains("Phase 1h"),
            "error must name layer7.sh Phase 1h as the normal provider: {msg}"
        );
        assert!(
            msg.contains("ENV_TEST_ORG_SUBDOMAIN=devtest cargo test -p env-tests"),
            "error must name the manual escape hatch verbatim: {msg}"
        );
    }

    #[test]
    fn test_resolve_org_subdomain_rejects_blank() {
        // `Some("")` must behave exactly like `None` (cluster.rs's read_env_url convention).
        // A shell exporting the result of a failed command substitution produces this, and
        // treating it as "present" would send a Host header with an empty first label.
        let err = resolve_org_subdomain(Some("")).expect_err("an empty value must not be accepted");
        assert!(
            err.to_string().contains("ENV_TEST_ORG_SUBDOMAIN"),
            "blank must produce the named required-variable error, not a regex error"
        );
    }

    #[test]
    fn test_resolve_org_subdomain_rejects_uppercase() {
        // The single most likely bad value, and the one the schema CHECK rejects at the far
        // end of the run: AC lowercases nothing, so `Demo` resolves no organization.
        let err = resolve_org_subdomain(Some("E2E-ABCDEF0123456789"))
            .expect_err("uppercase must be rejected, NOT silently lowercased");
        let msg = err.to_string();
        assert!(msg.contains("not a valid organization subdomain"), "{msg}");
        // Rejected, never repaired — a normalized value would address a different org than
        // the one layer7.sh actually provisioned and exported.
        assert!(
            !msg.contains("e2e-abcdef0123456789"),
            "the resolver must not offer a normalized form: {msg}"
        );
    }

    #[test]
    fn test_resolve_org_subdomain_rejects_regex_invalid_shapes() {
        // Boundary conditions of the anchored DNS-label rule, each a distinct way to be
        // invalid rather than five spellings of one.
        for bad in [
            "-leading",         // leading hyphen
            "trailing-",        // trailing hyphen
            "has_underscore",   // charset
            "has space",        // charset
            "e2e-abc\ndevtest", // embedded newline — the anchors are `^`/`$`, so a
            // multiline value must NOT pass on its second line
            "аbc", // Cyrillic U+0430, not ASCII 'a'
            "0123456789012345678901234567890123456789012345678901234567890123", // 64 chars
        ] {
            assert!(
                resolve_org_subdomain(Some(bad)).is_err(),
                "{bad:?} must be rejected by the anchored subdomain pattern"
            );
        }
    }

    #[test]
    fn test_resolve_org_subdomain_accepts_maximum_length_label() {
        // 63 chars is the schema's VARCHAR(63) ceiling and the pattern's `{0,61}` + 2
        // anchors. Pinned alongside the 64-char rejection above so the boundary is proven
        // to be in the right place rather than merely somewhere.
        let max = "a".repeat(63);
        assert_eq!(
            resolve_org_subdomain(Some(&max)).expect("63 chars is the documented maximum"),
            max
        );
    }

    #[test]
    fn test_subdomain_pattern_matches_the_schema_check_verbatim() {
        // Byte-identity with the migration's `subdomain_format` CHECK. The cross-file
        // version of this is enforced by
        // scripts/guards/simple/validate-subdomain-regex-sync.sh; this in-crate assertion
        // fails first, in-clone, without needing the guard to run.
        assert_eq!(SUBDOMAIN_PATTERN, "^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$");
    }

    #[test]
    fn test_build_org_host_header_http() {
        let result = build_org_host_header("http://localhost:8082", "devtest");
        assert_eq!(result, "devtest.localhost:8082");
    }

    #[test]
    fn test_build_org_host_header_https() {
        let result = build_org_host_header("https://example.com:443", "acme");
        assert_eq!(result, "acme.example.com:443");
    }

    #[test]
    fn test_build_org_host_header_trailing_slash() {
        let result = build_org_host_header("http://localhost:8082/", "devtest");
        assert_eq!(result, "devtest.localhost:8082");
    }

    #[test]
    fn test_user_registration_request_unique_email() {
        let req1 = UserRegistrationRequest::unique("User 1");
        let req2 = UserRegistrationRequest::unique("User 2");

        assert_ne!(req1.email, req2.email, "Emails should be unique");
        assert!(req1.email.ends_with("@envtest.dev"));
        assert_eq!(req1.password, TEST_USER_PASSWORD);
    }

    #[test]
    fn test_user_registration_response_debug_redacts_token() {
        let response = UserRegistrationResponse {
            user_id: Uuid::nil(),
            email: "test@example.com".to_string(),
            display_name: "Test User".to_string(),
            access_token: "eyJhbGciOiJFZERTQSJ9.secret.sig".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
        };

        let debug_output = format!("{:?}", response);

        assert!(
            !debug_output.contains("eyJhbGciOiJFZERTQSJ9"),
            "access_token should be redacted"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "Should contain [REDACTED]"
        );
        assert!(
            debug_output.contains("test@example.com"),
            "Email should be visible"
        );
    }

    /// WIRE-SHAPE ROUND-TRIP LOCK — `UserRegistrationResponse` mirror (task #51).
    ///
    /// DB-free guard that the env-tests fixture deserializes AC's CURRENT register
    /// wire shape: uniform camelCase, no per-field OAuth carve-out. The live
    /// round-trip (via `register_user()`) only runs in the cluster-gated env-tests;
    /// this asserts the mirror tracks the server shape without a DB — the same
    /// always-on protection the AC-side struct locks provide. If the AC server flips
    /// the wire and this mirror lags, the cluster round-trip (matrix c) would fail at
    /// deserialize; this test trips first, in-clone.
    #[test]
    fn test_user_registration_response_camel_wire_round_trip() {
        // CURRENT camelCase wire (post-task-#51) deserializes and populates all fields.
        let camel = r#"{
            "userId": "00000000-0000-0000-0000-000000000000",
            "email": "alice@example.com",
            "displayName": "Alice",
            "accessToken": "FAKE_TOKEN_FOR_TEST",
            "tokenType": "Bearer",
            "expiresIn": 3600
        }"#;
        let resp: UserRegistrationResponse =
            serde_json::from_str(camel).expect("camelCase register wire must deserialize");
        assert_eq!(resp.user_id, Uuid::nil());
        assert_eq!(resp.email, "alice@example.com");
        assert_eq!(resp.display_name, "Alice");
        assert_eq!(resp.token_type, "Bearer");
        assert_eq!(resp.expires_in, 3600);

        // Wire-break: the legacy mixed snake_case OAuth keys no longer populate the
        // token fields. With `rename_all = "camelCase"` and no per-field overrides,
        // `access_token`/`token_type`/`expires_in` are unknown keys; since the struct
        // does not `deny_unknown_fields`, they're ignored and the required `accessToken`
        // is then missing → deserialize error. This pins the carve-out's removal.
        let snake = r#"{
            "userId": "00000000-0000-0000-0000-000000000000",
            "email": "alice@example.com",
            "displayName": "Alice",
            "access_token": "FAKE_TOKEN_FOR_TEST",
            "token_type": "Bearer",
            "expires_in": 3600
        }"#;
        let res: Result<UserRegistrationResponse, _> = serde_json::from_str(snake);
        assert!(
            res.is_err(),
            "legacy snake_case OAuth keys must no longer satisfy the register response \
             (task #51 removed the per-field carve-out; the mirror must track the camel wire)"
        );
    }
}
