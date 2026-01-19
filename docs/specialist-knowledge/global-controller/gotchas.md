# Global Controller Gotchas

Mistakes to avoid and edge cases discovered in the Global Controller codebase.

---

## Gotcha: AC JWKS URL Must Be Reachable
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

GC validates JWTs using AC's JWKS endpoint. AC_JWKS_URL must be reachable at runtime. In tests, use wiremock to mock the endpoint. In production, ensure network connectivity to AC.

---

## Gotcha: Clock Skew Must Match AC Configuration
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

JWT_CLOCK_SKEW_SECONDS should match AC's value (default 300s). Mismatched skew can cause valid tokens to be rejected. Both services should read from a shared configuration source in production.

---

## Gotcha: kid Extraction Happens BEFORE Signature Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

The kid is extracted from JWT header without signature verification. This is correct for key lookup but:
- Never trust kid value
- Always validate JWK (kty, alg) after fetching
- Attacker can claim any kid, but must have valid signature from that key
If JWK doesn't exist, return "invalid or expired" not "kid not found" (info leak prevention).

---

## Gotcha: JWKS Cache TTL Affects Key Rotation Latency
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwks.rs`

5-minute cache TTL means AC key rotations take up to 5 minutes to propagate. If AC rotates keys and GC still has old key cached, tokens signed with new key will fail until cache expires. This is intentional tradeoff:
- Shorter TTL (1 min): Faster rotation but higher load on AC
- Longer TTL (10 min): Lower AC load but slower rotation
Verify TTL matches operational requirements during deployment.

---

## Gotcha: Generic Error Messages Hide JWT Validation Details
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

All JWT validation failures return "The access token is invalid or expired" to clients. This is intentional - don't leak:
- Whether kid was found
- Why JWK validation failed
- Specific signature error details
Log detailed error internally, but never include in HTTP response. This prevents attackers from probing token format.

---

## Gotcha: Algorithm Confusion Attacks via alg:none
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

The jsonwebtoken library defaults to accepting `alg:none` if not explicitly pinned. ALWAYS use `Validation::new(Algorithm::EdDSA)` - never `Validation::default()`. Test for this specifically:
- Token with `alg:none` should be rejected
- Token with `alg:HS256` should be rejected
- Only `alg:EdDSA` should be accepted

---

## Gotcha: GC_SERVICE_TOKEN Required for AC Communication
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/config.rs`, `crates/global-controller/src/services/ac_client.rs`

GC_SERVICE_TOKEN env var is required for internal AC endpoint calls (meeting tokens, guest tokens). Empty string default causes silent 401 failures from AC. In tests, mock the AC endpoints or provide valid test token. Production MUST set this via secrets management.

---

## Gotcha: Captcha Validation is Placeholder
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/handlers/meetings.rs`

Guest token endpoint has TODO placeholder for captcha validation. Currently accepts any captcha_token value. Phase 3+ must integrate real captcha provider (reCAPTCHA, hCaptcha). Do not deploy guest access without implementing this security control.

---

## Gotcha: JWT kid Extraction Returns None for Non-String Values
**Added**: 2026-01-18
**Related files**: `crates/global-controller/src/auth/jwt.rs`

The `extract_kid()` function returns `None` (not an error) when the JWT header contains a `kid` that is not a JSON string - including numeric values, null, or empty strings. This is by design: attackers may send malformed headers to probe error handling. Always handle `None` as "key not found" and return generic error message.

---
