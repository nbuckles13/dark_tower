//! Fixed test IDs for deterministic tests
//!
//! All test IDs are deterministic to ensure reproducible test results.
//! Using fixed UUIDs prevents flaky tests caused by random data.

use uuid::Uuid;

// Credential IDs (1-99)
pub const TEST_CREDENTIAL_ID_1: Uuid = Uuid::from_u128(1);
pub const TEST_CREDENTIAL_ID_2: Uuid = Uuid::from_u128(2);
pub const TEST_CREDENTIAL_ID_3: Uuid = Uuid::from_u128(3);

// User IDs (100-199)
pub const TEST_USER_ALICE: Uuid = Uuid::from_u128(100);
pub const TEST_USER_BOB: Uuid = Uuid::from_u128(101);
pub const TEST_USER_CHARLIE: Uuid = Uuid::from_u128(102);

// Organization IDs (1000-1099)
pub const TEST_ORG_ACME: Uuid = Uuid::from_u128(1000);
pub const TEST_ORG_GLOBEX: Uuid = Uuid::from_u128(1001);

// Signing Key IDs (strings)
pub const TEST_KEY_ID_1: &str = "test-key-2025-01";
pub const TEST_KEY_ID_2: &str = "test-key-2025-02";

// Client IDs
pub const TEST_CLIENT_ID_GC: &str = "gc-test-client";
pub const TEST_CLIENT_ID_MC: &str = "mc-test-client";
pub const TEST_CLIENT_ID_MH: &str = "mh-test-client";

// Test secrets (for registration)
pub const TEST_CLIENT_SECRET: &str = "test-secret-do-not-use-in-production";

// Test scopes
pub const SCOPE_MEETING_CREATE: &str = "meeting:create";
pub const SCOPE_MEETING_READ: &str = "meeting:read";
pub const SCOPE_ADMIN_SERVICES: &str = "admin:services";
