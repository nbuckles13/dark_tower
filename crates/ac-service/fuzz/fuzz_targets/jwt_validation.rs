#![no_main]

use libfuzzer_sys::fuzz_target;
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
}

fuzz_target!(|data: &[u8]| {
    // Try to interpret the fuzz input as a UTF-8 string (JWT format)
    if let Ok(token_str) = std::str::from_utf8(data) {
        // Generate a dummy public key for validation
        // In real fuzzing, we'd use fixed test keys
        let dummy_key = DecodingKey::from_secret(&[0u8; 32]);

        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false; // Don't validate expiration for fuzzing

        // Try to decode - should never panic
        let _ = decode::<Claims>(token_str, &dummy_key, &validation);
    }

    // Also fuzz the raw JWT parser by trying different separators
    // JWT format: header.payload.signature
    if data.len() >= 3 {
        if let Ok(s) = std::str::from_utf8(data) {
            // Look for potential JWT-like structures
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() == 3 {
                // Try base64 decoding each part
                use base64::engine::general_purpose;
                use base64::Engine;

                let _ = general_purpose::URL_SAFE_NO_PAD.decode(parts[0]);
                let _ = general_purpose::URL_SAFE_NO_PAD.decode(parts[1]);
                let _ = general_purpose::URL_SAFE_NO_PAD.decode(parts[2]);
            }
        }
    }
});
