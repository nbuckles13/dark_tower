//! `media_dial` credential verification for client admission (ADR-0039 §5).
//!
//! MH admits a client ONLY on an MC-minted `media_dial` token for that meeting
//! and this handler. The AC JWKS client is NOT on this path (it serves only
//! `MhAuthLayer`'s service tokens): MH never fetches a key here. The key is the
//! meeting's PUBLIC Ed25519 verification key from its `RegisterMeeting`
//! registration ([`MediaDialKey`]).
//!
//! # Verification order (ADR-0039 §5; story 4 MH-1)
//!
//! (a) size cap; (b) JWS header `alg == EdDSA`; (c) the `meeting_id` claim read
//! UNVERIFIED, only to look up that meeting's registered key; (d) the signature
//! with that key; (e) the verified claims: `meeting_id` equals the looked-up
//! meeting, `token_type == media_dial`, `exp`/`iat` within the configured
//! skew, `sub` non-empty; (f) only then `aud` == this handler's `handler_id`.
//!
//! [`verify_media_dial`] is THE one home of that order; the lookup (the one
//! async step) is injected, so production (`webtransport::close::admit`, the
//! session actor) and the unit tests run the same composition.
//!
//! Every failure in (a)–(e) is the one generic `DECLINED` at the peer; (f) is
//! `WRONG_HANDLER` ([`DialRefusal`] carries which). The reason is metric-only
//! (`mh_jwt_validations_total{failure_reason}`).
//!
//! # Header members
//!
//! `kid` / `jwk` / `jku` / `x5u` / `x5c` are never read and never steer the
//! key. Present and well-typed, they are ignored. A member that is not a valid
//! JOSE type (e.g. `"kid": 5`) makes the header itself malformed, so the token
//! is `Malformed` — the header parser and the signature check would both refuse
//! it, and the boundary is deliberate (pinned by test).
//!
//! # Hygiene
//!
//! The token, its header, the unverified claim set and the key never reach a
//! log, a span, an error string or a metric label. [`MediaDialRejection`]'s
//! `Display` is its label; library error text is discarded.

use common::jwt::{validate_iat, MediaDialClaims, MAX_JWT_SIZE_BYTES, MEDIA_DIAL_TOKEN_TYPE};
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

/// A meeting's PUBLIC `media_dial` verification key: exactly 32 raw Ed25519
/// bytes from `RegisterMeetingRequest.media_dial_public_key`.
///
/// Public, but still never logged or Debug-printed (ADR-0039 §5): `Debug`
/// renders only the length, there is no `Display` and no serde, and the bytes
/// are never string-encoded.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MediaDialKey([u8; MEDIA_DIAL_KEY_BYTES]);

/// The one length of a `media_dial` public key (Ed25519).
pub const MEDIA_DIAL_KEY_BYTES: usize = 32;

impl MediaDialKey {
    /// The ONE length check: `Some` only for exactly 32 bytes.
    #[must_use]
    pub fn from_wire(bytes: &[u8]) -> Option<Self> {
        <[u8; MEDIA_DIAL_KEY_BYTES]>::try_from(bytes).ok().map(Self)
    }
}

impl fmt::Debug for MediaDialKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MediaDialKey(<{MEDIA_DIAL_KEY_BYTES} bytes>)")
    }
}

/// Why a `media_dial` token was refused. Each variant is exactly one
/// ADR-0039 §8 `failure_reason` label; [`Self::close_code`] is the ONE mapping
/// to the peer-visible code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDialRejection {
    /// Oversized, not `EdDSA`, unparseable, an empty `sub`, or a verified
    /// `meeting_id` that differs from the meeting whose key verified it.
    Malformed,
    /// The `meeting_id` names a meeting this handler holds no key for (never
    /// registered, or released).
    NoMeetingKey,
    /// The signature does not verify against the meeting's registered key
    /// (an AC meeting token for a held meeting lands here).
    SignatureInvalid,
    /// `exp` passed, or `iat` is in the future, beyond the configured skew.
    Expired,
    /// Signed with the meeting key but `token_type` is not `media_dial`.
    WrongType,
    /// Fully valid, but `aud` names another handler (step f).
    AudMismatch,
}

impl MediaDialRejection {
    /// Every reason; the zero-init and the vocabulary pin iterate this.
    pub const ALL: [Self; 6] = [
        Self::Malformed,
        Self::NoMeetingKey,
        Self::SignatureInvalid,
        Self::Expired,
        Self::WrongType,
        Self::AudMismatch,
    ];

    /// The `failure_reason` label on `mh_jwt_validations_total`.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::NoMeetingKey => "no_meeting_key",
            Self::SignatureInvalid => "signature_invalid",
            Self::Expired => "expired",
            Self::WrongType => "wrong_type",
            Self::AudMismatch => "aud_mismatch",
        }
    }
}

impl fmt::Display for MediaDialRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_label())
    }
}

/// The `meeting_id` claim read BEFORE the signature is checked. Untrusted:
/// used only to look up the meeting's registered key, never logged, labelled
/// or traced. No `Debug`.
struct UnverifiedMeetingId(String);

#[derive(Deserialize)]
struct MeetingIdClaim {
    meeting_id: String,
}

/// Steps (a)–(c): size cap, `alg` pinned to `EdDSA`, and the unverified
/// `meeting_id`. Nothing here selects or fetches a key.
///
/// # Errors
///
/// [`MediaDialRejection::Malformed`] for every failure.
fn unverified_meeting(token: &str) -> Result<UnverifiedMeetingId, MediaDialRejection> {
    // (a) Before any decode.
    if token.len() > MAX_JWT_SIZE_BYTES {
        return Err(MediaDialRejection::Malformed);
    }
    // (b) The library's one header parser (the same one `decode` uses at d):
    // an unknown `alg` (including "none") fails the parse; every other
    // algorithm fails the compare. Key-locating members are never read.
    let header = decode_header(token).map_err(|_| MediaDialRejection::Malformed)?;
    if header.alg != Algorithm::EdDSA {
        return Err(MediaDialRejection::Malformed);
    }
    // (c) Only the `meeting_id`, only for the lookup.
    let claim: MeetingIdClaim = jsonwebtoken::dangerous::insecure_decode_claims(token)
        .map_err(|_| MediaDialRejection::Malformed)?;
    Ok(UnverifiedMeetingId(claim.meeting_id))
}

/// Steps (d)–(e): the signature with the meeting's registered key, then the
/// verified claims. `meeting` is the id whose key was looked up.
///
/// # Errors
///
/// The rejection for the first failing check.
fn verify_with_key(
    token: &str,
    meeting: &UnverifiedMeetingId,
    key: MediaDialKey,
    skew: Duration,
) -> Result<MediaDialClaims, MediaDialRejection> {
    // Raw 32-byte Ed25519 key, built per call exactly as
    // `common::jwt::verify_token` builds its key.
    let decoding_key = DecodingKey::from_ed_der(&key.0);
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.leeway = skew.as_secs();
    validation.validate_exp = true;
    // (f) is ours, after everything else, so a mismatch is distinguishable.
    validation.validate_aud = false;
    validation.required_spec_claims = HashSet::from(["exp".to_string()]);

    // (d) + exp.
    let claims = decode::<MediaDialClaims>(token, &decoding_key, &validation)
        .map_err(|e| rejection_for(e.kind()))?
        .claims;

    // (e) iat in the future beyond the skew is a clock-window failure.
    validate_iat(claims.iat, skew).map_err(|_| MediaDialRejection::Expired)?;
    // Load-bearing, not a tautology: the unverified read and the verified
    // parse can disagree (duplicate keys and the like).
    if claims.meeting_id != meeting.0 {
        return Err(MediaDialRejection::Malformed);
    }
    if claims.token_type != MEDIA_DIAL_TOKEN_TYPE {
        return Err(MediaDialRejection::WrongType);
    }
    // The authority on the fixed fields; with the type already checked, what
    // is left to fail is the empty `sub`.
    claims
        .validate()
        .map_err(|_| MediaDialRejection::Malformed)?;
    Ok(claims)
}

/// A token that passed steps (a)–(e): its verified claims and the registered
/// key that verified them (admission re-checks that key is still the
/// meeting's, so a re-key between lookup and admission admits nothing).
pub struct Verified {
    /// The verified claims. Never logged whole (`sub` is redacted in `Debug`).
    pub claims: MediaDialClaims,
    /// The key that verified them.
    pub key: MediaDialKey,
}

/// Why [`verify_media_dial`] refused, by the code the peer is told.
pub enum DialRefusal {
    /// A failure in (a)–(e): the one generic `DECLINED`.
    Declined(MediaDialRejection),
    /// Step (f) only: fully valid, `aud` names another handler —
    /// `WRONG_HANDLER`, built from the verified token.
    WrongHandler(Box<Verified>),
}

impl DialRefusal {
    /// The counted `failure_reason`.
    #[must_use]
    pub fn rejection(&self) -> MediaDialRejection {
        match self {
            Self::Declined(rejection) => *rejection,
            Self::WrongHandler(_) => MediaDialRejection::AudMismatch,
        }
    }
}

/// Verify a `media_dial` token in exactly the ADR-0039 §5 order — THE one
/// home of it: (a)–(c) [`unverified_meeting`]; the meeting's registered key
/// from `lookup` (given only the unverified `meeting_id`; `None` =
/// `no_meeting_key`, and the lookup must create nothing); (d)–(e)
/// [`verify_with_key`]; then (f) `aud` against `handler_id`, last.
///
/// # Errors
///
/// The [`DialRefusal`] for the first failing step.
pub async fn verify_media_dial<F, Fut>(
    token: &str,
    handler_id: &str,
    skew: Duration,
    lookup: F,
) -> Result<Verified, DialRefusal>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Option<MediaDialKey>>,
{
    let meeting = unverified_meeting(token).map_err(DialRefusal::Declined)?;
    let key = lookup(meeting.0.clone())
        .await
        .ok_or(DialRefusal::Declined(MediaDialRejection::NoMeetingKey))?;
    let claims = verify_with_key(token, &meeting, key, skew).map_err(DialRefusal::Declined)?;
    let verified = Verified { claims, key };
    if verified.claims.aud == handler_id {
        Ok(verified)
    } else {
        Err(DialRefusal::WrongHandler(Box::new(verified)))
    }
}

/// The library's error kind as a rejection. The catch-all is DELIBERATELY
/// fail-closed: anything that is not a signature or expiry verdict (base64,
/// JSON, claim shape — including an AC claim set signed by the meeting key) is
/// a malformed token.
fn rejection_for(kind: &ErrorKind) -> MediaDialRejection {
    match kind {
        ErrorKind::InvalidSignature => MediaDialRejection::SignatureInvalid,
        ErrorKind::ExpiredSignature => MediaDialRejection::Expired,
        _ => MediaDialRejection::Malformed,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mh_test_utils::media_dial::{DialForge, ATTACKER_SEED, MEETING_SEED};
    use serde_json::json;

    const HANDLER: &str = "mh-handler-1";
    const MEETING: &str = "meeting-1";
    const SKEW: Duration = Duration::from_secs(7);

    fn key(forge: &DialForge) -> MediaDialKey {
        MediaDialKey::from_wire(&forge.public_key()).unwrap()
    }

    /// The production composition ([`verify_media_dial`]) with a one-meeting
    /// registry holding `registered`'s key.
    fn run(token: &str, registered: &DialForge) -> Result<MediaDialClaims, MediaDialRejection> {
        let key = key(registered);
        let lookup = |meeting: String| async move { (meeting == MEETING).then_some(key) };
        let outcome = block_on(verify_media_dial(token, HANDLER, SKEW, lookup));
        outcome.map(|v| v.claims).map_err(|r| r.rejection())
    }

    /// The lookup here never awaits, so a single poll completes the future.
    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn valid() -> serde_json::Value {
        DialForge::claims_json(MEETING, HANDLER, "user-1", Utc::now().timestamp(), 300)
    }

    #[test]
    fn a_meeting_key_signed_token_is_accepted_and_reusable() {
        let forge = DialForge::new(MEETING_SEED);
        let token = forge.sign(&valid());
        let first = run(&token, &forge).unwrap();
        assert_eq!(first.sub, "user-1");
        // No jti cache: the same token again.
        assert!(run(&token, &forge).is_ok());
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one literal table of every reason plus its twins"
    )]
    fn every_reason_has_its_input_and_the_valid_twin_passes() {
        let forge = DialForge::new(MEETING_SEED);
        let other = DialForge::new(ATTACKER_SEED);
        let now = Utc::now().timestamp();
        let mut wrong_type = valid();
        wrong_type["token_type"] = json!("meeting");
        let mut empty_sub = valid();
        empty_sub["sub"] = json!("");
        let mut other_meeting = valid();
        other_meeting["meeting_id"] = json!("meeting-2");
        let expired = DialForge::claims_json(MEETING, HANDLER, "user-1", now - 320, 300);
        let future_iat = DialForge::claims_json(MEETING, HANDLER, "user-1", now + 60, 300);
        let wrong_aud = DialForge::claims_json(MEETING, "mh-other", "user-1", now, 300);

        let cases: Vec<(&str, String, MediaDialRejection)> = vec![
            (
                // VALIDLY signed, one byte over: only step (a) refuses it.
                "one byte over the size cap",
                forge.sign_padded_to(&valid(), MAX_JWT_SIZE_BYTES + 1),
                MediaDialRejection::Malformed,
            ),
            ("garbage", "not-a-jwt".into(), MediaDialRejection::Malformed),
            (
                "alg none",
                forge.sign_with_header(&json!({"alg":"none"}), &valid()),
                MediaDialRejection::Malformed,
            ),
            (
                "alg HS256",
                forge.sign_with_header(&json!({"alg":"HS256"}), &valid()),
                MediaDialRejection::Malformed,
            ),
            (
                "unheld meeting",
                forge.sign(&other_meeting),
                MediaDialRejection::NoMeetingKey,
            ),
            (
                "other meeting's key",
                other.sign(&valid()),
                MediaDialRejection::SignatureInvalid,
            ),
            (
                "expired beyond skew",
                forge.sign(&expired),
                MediaDialRejection::Expired,
            ),
            (
                "iat in the future",
                forge.sign(&future_iat),
                MediaDialRejection::Expired,
            ),
            (
                "token_type",
                forge.sign(&wrong_type),
                MediaDialRejection::WrongType,
            ),
            (
                "empty sub",
                forge.sign(&empty_sub),
                MediaDialRejection::Malformed,
            ),
            (
                "aud",
                forge.sign(&wrong_aud),
                MediaDialRejection::AudMismatch,
            ),
            (
                "bad signature + wrong aud",
                other.sign(&wrong_aud),
                MediaDialRejection::SignatureInvalid,
            ),
            (
                "expired + wrong aud",
                forge.sign(&DialForge::claims_json(
                    MEETING,
                    "mh-other",
                    "user-1",
                    now - 320,
                    300,
                )),
                MediaDialRejection::Expired,
            ),
        ];
        for (name, token, want) in cases {
            assert_eq!(run(&token, &forge).err(), Some(want), "{name}");
        }
        // Positive twins at the boundaries. `now` is RE-SAMPLED here: the twin
        // has only 4 s of slack (exp = now - 3 against a 7 s skew), and under
        // Layer-4 load the table above can take longer than that, which made
        // this arm flaky against the first sample.
        let now = Utc::now().timestamp();
        let within = DialForge::claims_json(MEETING, HANDLER, "user-1", now - 303, 300);
        assert!(
            run(&forge.sign(&within), &forge).is_ok(),
            "expired within skew"
        );
        let iat_ahead_within = DialForge::claims_json(
            MEETING,
            HANDLER,
            "user-1",
            now + i64::try_from(SKEW.as_secs()).unwrap() - 2,
            300,
        );
        assert!(
            run(&forge.sign(&iat_ahead_within), &forge).is_ok(),
            "iat ahead of MH's clock, within the skew"
        );
        let at_cap = forge.sign_padded_to(&valid(), MAX_JWT_SIZE_BYTES);
        assert_eq!(at_cap.len(), MAX_JWT_SIZE_BYTES);
        assert!(run(&at_cap, &forge).is_ok(), "exactly at the size cap");
    }

    #[test]
    fn key_locating_headers_never_steer_the_key() {
        let forge = DialForge::new(MEETING_SEED);
        let attacker = DialForge::new(ATTACKER_SEED);
        let header = json!({"alg":"EdDSA","kid":"k1","jwk": attacker.jwk(), "jku":"https://x", "x5u":"https://y"});
        // Accept arm: meeting key, headers present.
        assert!(run(&forge.sign_with_header(&header, &valid()), &forge).is_ok());
        // Reject arm: the attacker's key, announced in the header, signed it.
        assert_eq!(
            run(&attacker.sign_with_header(&header, &valid()), &forge).err(),
            Some(MediaDialRejection::SignatureInvalid)
        );
        // A header member of an invalid JOSE type is a malformed header.
        assert_eq!(
            run(
                &forge.sign_with_header(&json!({"alg":"EdDSA","kid":5}), &valid()),
                &forge
            )
            .err(),
            Some(MediaDialRejection::Malformed)
        );
    }

    #[test]
    fn key_is_exactly_32_bytes_and_never_rendered() {
        assert!(MediaDialKey::from_wire(&[7; 31]).is_none());
        assert!(MediaDialKey::from_wire(&[7; 33]).is_none());
        let k = MediaDialKey::from_wire(&[7; 32]).unwrap();
        assert_eq!(format!("{k:?}"), "MediaDialKey(<32 bytes>)");
    }

    #[test]
    fn labels_and_codes_are_the_adr_vocabulary() {
        let labels: Vec<_> = MediaDialRejection::ALL
            .iter()
            .map(|r| r.as_label())
            .collect();
        assert_eq!(
            labels,
            [
                "malformed",
                "no_meeting_key",
                "signature_invalid",
                "expired",
                "wrong_type",
                "aud_mismatch"
            ]
        );
        for r in MediaDialRejection::ALL {
            assert_eq!(r.to_string(), r.as_label());
        }
    }
}
