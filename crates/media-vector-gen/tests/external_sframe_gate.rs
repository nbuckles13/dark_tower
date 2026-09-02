//! The no-correlated-error gate: our key schedule against the **external**
//! sframe-wg vectors.
//!
//! # What makes this different from every other test in this changeset
//!
//! Nothing our generator produced appears here. The inputs are the external
//! row's `base_key` / `kid` / `ctr`; the expected values are the external row's
//! `sframe_secret` / `sframe_key` / `sframe_salt` / `nonce`. If our schedule
//! and the specification disagree, this fires — which is a property no
//! self-generated fixture can have, however carefully written.
//!
//! # What it does NOT gate
//!
//! Our SFrame object layout, our AAD span, our signed range, our detached-tag
//! split, our KEK unwrap, and the entire frame header. See
//! `proto/test-vectors/external/sframe-wg/PROVENANCE.md` for the in/out list.
//! Do not read a pass here as covering the three protected computations.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "Test code: a panic is the failure report. Matches the existing \
              convention in crates/media-protocol/tests/*.rs."
)]

use media_vector_gen::schedule::{self, AEAD_KEY_LEN, PRK_LEN, SALT_LEN};
use serde_json::Value;

const EXTERNAL_DIR: &str = "../../proto/test-vectors/external/sframe-wg";

/// The suite Dark Tower uses. Anything gated here is genuinely gated.
const SUITE_PRIMARY: u64 = 5;
/// SHA-256 contrast case. Its PRK is 32 bytes, which is what makes
/// "our PRK is 64 bytes" falsifiable rather than self-referential.
const SUITE_CONTRAST: u64 = 4;

fn read_json(name: &str) -> Value {
    let path = format!("{EXTERNAL_DIR}/{name}");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("vendored external artifact missing at {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path} is not valid JSON: {e}"))
}

fn selected_suites() -> Vec<u64> {
    let manifest = read_json("manifest.json");
    let suites: Vec<u64> = manifest["cipher_suites"]
        .as_array()
        .expect("manifest.cipher_suites must be an array")
        .iter()
        .map(|v| v.as_u64().expect("cipher_suite ids are integers"))
        .collect();
    // Anti-vacuity. The selector is shared with the TypeScript harness, so an
    // empty or narrowed list would silently reduce BOTH gates at once.
    assert!(
        !suites.is_empty(),
        "manifest.cipher_suites is empty: the external gate would iterate zero rows \
         and report success, which is the exact vacuity this anchor exists to prevent"
    );
    assert!(
        suites.contains(&SUITE_PRIMARY),
        "manifest.cipher_suites must select 0x0005 ({SUITE_PRIMARY}), the suite we actually use"
    );
    suites
}

fn row_for(suite: u64) -> Value {
    let vectors = read_json("test-vectors.json");
    let rows = vectors["sframe"].as_array().expect("sframe array");
    let found: Vec<&Value> = rows
        .iter()
        .filter(|r| r["cipher_suite"].as_u64() == Some(suite))
        .collect();
    // Per-suite, never "at least one row overall": the aggregate form passes
    // when 0x0005 vanishes upstream and 0x0004 survives, which is precisely the
    // degradation the anchor exists to catch.
    assert_eq!(
        found.len(),
        1,
        "expected exactly one upstream row for cipher_suite {suite}, found {}",
        found.len()
    );
    found[0].clone()
}

/// Assert every field the manifest declares required is present on `row`.
///
/// Anti-vacuity on the **artifact**, distinct from the anti-vacuity on the
/// selector. If an upstream re-vendor drops or renames a field, reading it as
/// absent would compare nothing and report success.
fn assert_required_fields_present(row: &Value) {
    let manifest = read_json("manifest.json");
    let required = manifest["required_fields"]
        .as_array()
        .expect("manifest.required_fields must be an array");
    assert!(
        !required.is_empty(),
        "manifest.required_fields is empty: the presence check would assert nothing"
    );
    for f in required {
        let name = f.as_str().expect("required_fields entries are strings");
        assert!(
            !row[name].is_null(),
            "upstream row is missing required field `{name}`; an upstream re-vendor \
             changed the shape and the gate would otherwise compare nothing"
        );
    }
}

/// The manifest's declared expectations for one suite, cross-checked against
/// **our own constants**.
///
/// Deliberately a three-way pin rather than one home for the numbers
/// (@paired-client raised this as a duplication question; ruling recorded in
/// `main.md`). The three claims are genuinely different:
///
/// * our constants say what **our implementation** produces;
/// * the manifest says what the **upstream artifact** should contain;
/// * the vendored bytes are what it **does** contain.
///
/// Collapsing to one home would turn "our derivation matches our declared
/// width" into "our derivation matches whatever the manifest says", which is a
/// weaker claim exactly where a weaker claim is not wanted. Asserting all three
/// agree costs two lines and catches a disagreement between **any** pair — so
/// the shared integers are a cross-check rather than duplication.
fn expectation(suite: u64) -> Value {
    let manifest = read_json("manifest.json");
    let found = manifest["expectations"]
        .as_array()
        .expect("manifest.expectations must be an array")
        .iter()
        .find(|e| e["cipher_suite"].as_u64() == Some(suite))
        .unwrap_or_else(|| panic!("manifest declares no expectations for suite {suite}"))
        .clone();
    if suite == SUITE_PRIMARY {
        assert_eq!(
            found["prk_bytes"].as_u64(),
            Some(PRK_LEN as u64),
            "manifest prk_bytes disagrees with our PRK_LEN constant"
        );
        assert_eq!(
            found["key_bytes"].as_u64(),
            Some(AEAD_KEY_LEN as u64),
            "manifest key_bytes disagrees with our AEAD_KEY_LEN constant"
        );
        assert_eq!(
            found["salt_bytes"].as_u64(),
            Some(SALT_LEN as u64),
            "manifest salt_bytes disagrees with our SALT_LEN constant"
        );
        assert_eq!(found["hash"].as_str(), Some("SHA-512"));
    }
    found
}

fn hex_field(row: &Value, field: &str) -> Vec<u8> {
    let s = row[field]
        .as_str()
        .unwrap_or_else(|| panic!("upstream row missing required field `{field}`"));
    hex::decode(s).unwrap_or_else(|e| panic!("upstream field `{field}` is not hex: {e}"))
}

/// The KID is taken as **raw bytes**, never through our packer.
///
/// The upstream `kid` is `291`, which under our layout decomposes to
/// `sender_id == 0` — the value we treat as reserved-invalid, because a shared
/// zero is N colliding key ids. Taking `[u8; 8]` directly means there is no
/// code path where our layout could be interposed, so a future layout change
/// cannot break an external gate that has nothing to do with our layout and be
/// misread as an upstream problem.
fn kid_bytes(row: &Value) -> [u8; 8] {
    let kid = row["kid"].as_u64().expect("kid is an integer");
    kid.to_be_bytes()
}

#[test]
fn primary_suite_key_schedule_matches_external_vectors() {
    let suites = selected_suites();
    assert!(suites.contains(&SUITE_PRIMARY));
    let row = row_for(SUITE_PRIMARY);
    assert_required_fields_present(&row);
    let expect = expectation(SUITE_PRIMARY);
    assert_eq!(expect["gates_aead"].as_bool(), Some(true));

    let base_key = hex_field(&row, "base_key");
    let kid = kid_bytes(&row);
    let suite_u16 = u16::try_from(SUITE_PRIMARY).unwrap();

    // PRK length and hash, asserted explicitly. The claim "HKDF is SHA-512
    // throughout, so the PRK is 64 bytes not 32" is worth nothing as a comment;
    // this is the external evidence for it.
    let prk = schedule::extract_prk(&base_key);
    assert_eq!(
        prk.len(),
        PRK_LEN,
        "PRK must be {PRK_LEN} bytes (SHA-512). 32 would mean SHA-256, i.e. the wrong \
         ciphersuite silently in use"
    );
    assert_eq!(
        hex::encode(prk),
        row["sframe_secret"].as_str().unwrap(),
        "our HKDF-Extract disagrees with the upstream sframe_secret"
    );

    let derived = schedule::derive(&base_key, &kid, suite_u16).expect("derive");
    assert_eq!(
        hex::encode(derived.key),
        row["sframe_key"].as_str().unwrap(),
        "our HKDF-Expand (key) disagrees with the upstream sframe_key"
    );
    assert_eq!(
        hex::encode(derived.salt),
        row["sframe_salt"].as_str().unwrap(),
        "our HKDF-Expand (salt) disagrees with the upstream sframe_salt"
    );

    // Direct GCM form: the derived key length IS the AEAD key length. An
    // enc_key/auth_key split would make this longer, and that split belongs to
    // the AES-CTR suites, not to 0x0005.
    assert_eq!(derived.key.len(), AEAD_KEY_LEN);
    assert_eq!(derived.salt.len(), SALT_LEN);

    // Nonce derivation, gated by the same row. `ctr` is 17767, which exceeds
    // 16 bits, so multi-byte placement is genuinely exercised.
    let ctr = row["ctr"].as_u64().expect("ctr");
    assert!(ctr > u64::from(u16::MAX) / 4, "ctr should exercise >1 byte");
    let nonce = schedule::nonce_from(&derived.salt, ctr);
    assert_eq!(
        hex::encode(nonce),
        row["nonce"].as_str().unwrap(),
        "our nonce derivation disagrees with the upstream nonce"
    );
}

#[test]
fn primary_suite_aead_primitive_matches_external_vectors() {
    let row = row_for(SUITE_PRIMARY);
    let base_key = hex_field(&row, "base_key");
    let kid = kid_bytes(&row);
    let derived =
        schedule::derive(&base_key, &kid, u16::try_from(SUITE_PRIMARY).unwrap()).expect("derive");
    let nonce = schedule::nonce_from(&derived.salt, row["ctr"].as_u64().unwrap());

    let aad = hex_field(&row, "aad");
    let pt = hex_field(&row, "pt");
    let sealed = schedule::seal_detached(&derived.key, &nonce, &aad, &pt).expect("seal");

    // Upstream `ct` is `sframe_header || ciphertext || tag`. Our object layout
    // differs (key_id || tag || ciphertext), so we compare the CRYPTOGRAPHIC
    // output only: the upstream ct ends with ciphertext then tag, and its
    // header is exactly the leading `aad` bytes that are not metadata.
    let ct = hex_field(&row, "ct");
    let tail = &ct[ct.len() - (pt.len() + sealed.tag.len())..];
    let (ct_body, ct_tag) = tail.split_at(pt.len());
    assert_eq!(
        hex::encode(ct_body),
        hex::encode(&sealed.ciphertext),
        "our AES-256-GCM ciphertext disagrees with the upstream ct"
    );
    assert_eq!(
        hex::encode(ct_tag),
        hex::encode(sealed.tag),
        "our AES-256-GCM tag disagrees with the upstream ct's tag"
    );
    assert_eq!(
        sealed.ciphertext.len(),
        pt.len(),
        "GCM is length-preserving"
    );
}

/// The SHA-256 contrast case. Terminates at key-schedule output and never
/// reaches a seal or open call.
#[test]
fn contrast_suite_proves_the_prk_length_claim_is_falsifiable() {
    let suites = selected_suites();
    if !suites.contains(&SUITE_CONTRAST) {
        panic!(
            "manifest.cipher_suites no longer selects {SUITE_CONTRAST}: without the SHA-256 \
             contrast row, `our PRK is 64 bytes` is self-referential rather than falsifiable"
        );
    }
    let row = row_for(SUITE_CONTRAST);
    assert_required_fields_present(&row);
    let expect = expectation(SUITE_CONTRAST);
    // The manifest must itself declare that this row does NOT gate the AEAD.
    // If that flag ever flips, an AES-128 seal path would be required to
    // satisfy it — which is the acceptance path we deliberately do not have.
    assert_eq!(
        expect["gates_aead"].as_bool(),
        Some(false),
        "the contrast suite must not claim to gate the AEAD: satisfying that claim would \
         require an AES-128 code path built in order to test that we do not have one"
    );
    let secret = hex_field(&row, "sframe_secret");
    assert_eq!(
        secret.len(),
        32,
        "0x0004's PRK must be 32 bytes (SHA-256); if this is 64 the two suites are \
         indistinguishable and the primary suite's PRK-length assertion proves nothing"
    );
    assert_ne!(
        secret.len(),
        PRK_LEN,
        "the contrast suite must differ from the primary suite in PRK length"
    );
    // Derived key length is 16 for AES-128. Asserted from the vector, not
    // computed by us: we deliberately have no AES-128 code path.
    assert_eq!(
        hex_field(&row, "sframe_key").len(),
        usize::try_from(expect["key_bytes"].as_u64().unwrap()).unwrap()
    );
    assert_eq!(
        secret.len(),
        usize::try_from(expect["prk_bytes"].as_u64().unwrap()).unwrap()
    );
}

/// We must have no AES-128 acceptance path at all.
#[test]
fn aead_rejects_every_key_length_except_32() {
    for len in [0usize, 15, 16, 24, 31, 33, 64] {
        assert!(
            schedule::reject_wrong_key_len(len),
            "AES_256_GCM accepted a {len}-byte key. A 16-byte acceptance path would be \
             reachable by anything able to influence a suite id — built in order to test \
             that we do not have one"
        );
    }
    assert!(
        !schedule::reject_wrong_key_len(AEAD_KEY_LEN),
        "AES_256_GCM must accept a 32-byte key, or this test is vacuous"
    );
}
