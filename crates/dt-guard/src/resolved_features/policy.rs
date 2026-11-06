//! Crypto-provider policy for `dt-guard resolved-features` — DATA ONLY.
//!
//! AUTHORITY: `docs/decisions/adr-0027-approved-crypto.md` (Approved Algorithms
//! table + Usage Guidelines). This file is how that ADR's backend choices are
//! enforced; amend both in the same commit.
//!
//! OWNERSHIP (CLAUDE.md guard-crate rule: membership is content): which crates
//! are listed, their provider vocabularies and required sets, and the allowed
//! reverse dependents are policy CONTENT owned by `security` with
//! `auth-controller` (ADR-0027 crypto primitives are a Guarded Shared Area,
//! ADR-0024 §6.4). The struct shapes and how they are evaluated
//! (`resolved_features/mod.rs`) are machinery, owned by `infrastructure`.
//!
//! Every vocabulary term is a feature name as declared in that crate's own
//! `[features]` table (read from the manifest of the locked version, not from
//! memory). The guard fails with a PRECONDITION if a term stops being declared,
//! so an upstream rename cannot silently drop out of the comparison.

/// A crate that selects its crypto backend from Cargo features.
///
/// Check: `resolved features ∩ vocabulary == required` for EVERY resolved
/// instance of `krate`. Features outside the vocabulary (`std`, `tls12`,
/// `use_pem`, quinn's `rustls` alias, ...) are additive API and never matter.
#[derive(Debug, Clone, Copy)]
pub struct ProviderPolicy {
    pub krate: &'static str,
    /// Every feature of `krate` that selects or implies a crypto backend.
    pub vocabulary: &'static [&'static str],
    /// The exact subset of `vocabulary` that must be on.
    pub required: &'static [&'static str],
    /// One line: why, for the finding.
    pub reason: &'static str,
}

/// A crate that may only be reached through the listed dependents.
///
/// Backstop for what a vocabulary cannot know: a provider feature name added
/// in a future version of some crate. Any new path to aws-lc shows up here as a
/// new reverse dependent, whatever the feature that opened it is called.
#[derive(Debug, Clone, Copy)]
pub struct ReverseDepPolicy {
    pub krate: &'static str,
    pub allowed_dependents: &'static [&'static str],
    pub reason: &'static str,
}

/// quinn and quinn-proto declare the same backend features (0.11.12 / 0.11.19).
const QUINN_VOCABULARY: &[&str] = &[
    "ring",
    "rustls-ring",
    "aws-lc-rs",
    "aws-lc-rs-fips",
    "rustls-aws-lc-rs",
    "rustls-aws-lc-rs-fips",
    // Implies `rustls-aws-lc-rs` (quinn) / pairs with it (quinn-proto).
    "__rustls-post-quantum-test",
];

pub const PROVIDER_POLICY: &[ProviderPolicy] = &[
    ProviderPolicy {
        krate: "jsonwebtoken",
        vocabulary: &["aws_lc_rs", "rust_crypto"],
        required: &["aws_lc_rs"],
        reason: "ADR-0027: JWT EdDSA via the aws_lc_rs backend only; rust_crypto is not approved \
                 (non-strict dalek verify, pulls rsa / RUSTSEC-2023-0071); zero or both backends \
                 panic at the first sign/verify",
    },
    ProviderPolicy {
        krate: "rustls",
        // `aws-lc-rs` is an alias of `aws_lc_rs`; `fips` and `prefer-post-quantum`
        // both imply it. Listing them names the actual culprit in a finding.
        vocabulary: &[
            "ring",
            "aws_lc_rs",
            "aws-lc-rs",
            "fips",
            "prefer-post-quantum",
        ],
        required: &["ring"],
        reason: "ADR-0027: runtime TLS uses rustls' ring provider; a second provider makes every \
                 implicit-provider builder() panic",
    },
    ProviderPolicy {
        krate: "rustls-webpki",
        vocabulary: &["ring", "aws-lc-rs", "aws-lc-rs-fips", "aws-lc-rs-unstable"],
        required: &["ring"],
        reason:
            "ADR-0027: certificate-signature verification (the other half of TLS) stays on ring",
    },
    ProviderPolicy {
        krate: "quinn",
        vocabulary: QUINN_VOCABULARY,
        required: &["ring", "rustls-ring"],
        reason: "ADR-0027: QUIC/WebTransport TLS uses the ring provider; no aws-lc backend",
    },
    ProviderPolicy {
        krate: "quinn-proto",
        vocabulary: QUINN_VOCABULARY,
        required: &["ring", "rustls-ring"],
        reason: "ADR-0027: QUIC/WebTransport TLS uses the ring provider; no aws-lc backend",
    },
];

pub const REVERSE_DEP_POLICY: &[ReverseDepPolicy] = &[
    ReverseDepPolicy {
        krate: "aws-lc-rs",
        allowed_dependents: &["jsonwebtoken"],
        reason:
            "ADR-0027: aws-lc-rs is approved only inside jsonwebtoken; do not call it directly \
                 (or reach it via another crate) without amending the ADR",
    },
    ReverseDepPolicy {
        krate: "aws-lc-sys",
        allowed_dependents: &["aws-lc-rs"],
        reason: "ADR-0027: the aws-lc C library is reached only through aws-lc-rs",
    },
];
