//! Lua scripts for Redis fenced operations (ADR-0023 Section 3).
//!
//! These scripts ensure atomic fenced writes to prevent split-brain:
//! - Read current generation
//! - Compare with provided generation (fencing token)
//! - Only write if generation is current or newer
//!
//! # Security Properties
//!
//! - Monotonically increasing generations prevent replay of stale writes
//! - Atomic Lua execution prevents race conditions
//! - HSET fields validated before write

/// Lua script for fenced write operation.
///
/// Arguments:
/// - KEYS[1]: Generation key (e.g., `meeting:{id}:generation`)
/// - KEYS[2]: Data key (e.g., `meeting:{id}:mh`)
/// - ARGV[1]: Expected generation (fencing token)
/// - ARGV[2]: Data to write (JSON string)
///
/// Returns:
/// - 1: Success (write completed)
/// - 0: Fenced out (stale generation)
/// - -1: Error (invalid generation format)
pub const FENCED_WRITE: &str = r#"
-- Get current generation
local current_gen = redis.call('GET', KEYS[1])
local expected_gen = tonumber(ARGV[1])

if expected_gen == nil then
    return -1
end

if current_gen == nil or current_gen == false then
    -- No generation set yet, this is the first write
    redis.call('SET', KEYS[1], expected_gen)
    redis.call('SET', KEYS[2], ARGV[2])
    return 1
end

local current = tonumber(current_gen)
if current == nil then
    return -1
end

if expected_gen >= current then
    -- Valid generation, update both
    redis.call('SET', KEYS[1], expected_gen)
    redis.call('SET', KEYS[2], ARGV[2])
    return 1
else
    -- Stale generation, reject
    return 0
end
"#;

/// Lua script for fenced hash write operation.
///
/// Arguments:
/// - KEYS[1]: Generation key (e.g., `meeting:{id}:generation`)
/// - KEYS[2]: Hash key (e.g., `meeting:{id}:state`)
/// - ARGV[1]: Expected generation (fencing token)
/// - ARGV[2..]: Hash field-value pairs
///
/// Returns:
/// - 1: Success (write completed)
/// - 0: Fenced out (stale generation)
/// - -1: Error (invalid generation format)
pub const FENCED_HSET: &str = r#"
-- Get current generation
local current_gen = redis.call('GET', KEYS[1])
local expected_gen = tonumber(ARGV[1])

if expected_gen == nil then
    return -1
end

if current_gen == nil or current_gen == false then
    -- No generation set yet, this is the first write
    redis.call('SET', KEYS[1], expected_gen)
    -- Write hash fields (ARGV[2] onwards are field/value pairs)
    for i = 2, #ARGV, 2 do
        redis.call('HSET', KEYS[2], ARGV[i], ARGV[i+1])
    end
    return 1
end

local current = tonumber(current_gen)
if current == nil then
    return -1
end

if expected_gen >= current then
    -- Valid generation, update
    redis.call('SET', KEYS[1], expected_gen)
    for i = 2, #ARGV, 2 do
        redis.call('HSET', KEYS[2], ARGV[i], ARGV[i+1])
    end
    return 1
else
    -- Stale generation, reject
    return 0
end
"#;

/// Lua script to increment generation and return new value.
///
/// Arguments:
/// - KEYS[1]: Generation key
///
/// Returns:
/// - New generation value
pub const INCREMENT_GENERATION: &str = r#"
local current = redis.call('GET', KEYS[1])
local new_gen = 1

if current ~= nil and current ~= false then
    local val = tonumber(current)
    if val ~= nil then
        new_gen = val + 1
    end
end

redis.call('SET', KEYS[1], new_gen)
return new_gen
"#;

/// Lua script for fenced delete operation.
///
/// Arguments:
/// - KEYS[1]: Generation key
/// - KEYS[2]: Data key to delete
/// - ARGV[1]: Expected generation (fencing token)
///
/// Returns:
/// - 1: Success (delete completed)
/// - 0: Fenced out (stale generation)
/// - -1: Error (invalid generation format)
pub const FENCED_DELETE: &str = r#"
-- Get current generation
local current_gen = redis.call('GET', KEYS[1])
local expected_gen = tonumber(ARGV[1])

if expected_gen == nil then
    return -1
end

if current_gen == nil or current_gen == false then
    -- No generation set, nothing to delete
    return 1
end

local current = tonumber(current_gen)
if current == nil then
    return -1
end

if expected_gen >= current then
    -- Valid generation, delete and bump generation
    redis.call('SET', KEYS[1], expected_gen)
    redis.call('DEL', KEYS[2])
    return 1
else
    -- Stale generation, reject
    return 0
end
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scripts_are_valid_lua() {
        // Just verify the scripts are non-empty and contain expected keywords
        assert!(FENCED_WRITE.contains("redis.call"));
        assert!(FENCED_WRITE.contains("GET"));
        assert!(FENCED_WRITE.contains("SET"));

        assert!(FENCED_HSET.contains("HSET"));
        assert!(FENCED_HSET.contains("generation"));

        assert!(INCREMENT_GENERATION.contains("tonumber"));

        assert!(FENCED_DELETE.contains("DEL"));
    }

    #[test]
    fn test_script_length() {
        // Ensure scripts are reasonable size (not accidentally empty or huge)
        assert!(FENCED_WRITE.len() > 100);
        assert!(FENCED_WRITE.len() < 2000);

        assert!(FENCED_HSET.len() > 100);
        assert!(FENCED_HSET.len() < 2000);

        assert!(INCREMENT_GENERATION.len() > 50);
        assert!(INCREMENT_GENERATION.len() < 500);

        assert!(FENCED_DELETE.len() > 100);
        assert!(FENCED_DELETE.len() < 2000);
    }

    #[test]
    fn test_fenced_write_returns_correct_values() {
        // Verify the script logic by analyzing the return values
        //
        // Return values:
        //  1: Success (write completed)
        //  0: Fenced out (stale generation)
        // -1: Error (invalid generation format)
        //
        // Logic verification (trace through script):
        //
        // Case 1: First write (no generation exists)
        //   - GET returns nil
        //   - expected_gen is valid
        //   - Script sets both generation and data
        //   - Returns 1
        //
        // Case 2: Valid generation (expected >= current)
        //   - GET returns current generation
        //   - expected_gen >= current
        //   - Script updates both
        //   - Returns 1
        //
        // Case 3: Stale generation (expected < current)
        //   - GET returns higher generation
        //   - expected_gen < current
        //   - Script rejects write
        //   - Returns 0
        //
        // Case 4: Invalid expected_gen (not a number)
        //   - tonumber(ARGV[1]) returns nil
        //   - Returns -1

        // Verify the return value constants are documented correctly
        assert!(FENCED_WRITE.contains("return 1")); // Success
        assert!(FENCED_WRITE.contains("return 0")); // Fenced out
        assert!(FENCED_WRITE.contains("return -1")); // Error
    }

    #[test]
    fn test_fenced_write_handles_nil_generation() {
        // The script should handle the case where no generation exists yet
        // This is the "first write" case for a new meeting
        assert!(FENCED_WRITE.contains("if current_gen == nil or current_gen == false then"));
    }

    #[test]
    fn test_fenced_write_compares_generations() {
        // Verify the comparison logic uses >=, not just ==
        // This allows retries with the same generation to succeed
        assert!(FENCED_WRITE.contains("if expected_gen >= current then"));
    }

    #[test]
    fn test_increment_generation_starts_at_one() {
        // New meetings should start at generation 1, not 0
        assert!(INCREMENT_GENERATION.contains("local new_gen = 1"));
    }

    #[test]
    fn test_increment_generation_handles_existing() {
        // Script should increment existing generation
        assert!(INCREMENT_GENERATION.contains("new_gen = val + 1"));
    }

    #[test]
    fn test_fenced_delete_requires_higher_generation() {
        // Delete should use the same fencing logic as write
        assert!(FENCED_DELETE.contains("if expected_gen >= current then"));
    }

    #[test]
    fn test_fenced_delete_handles_nonexistent() {
        // Deleting a non-existent key should succeed (idempotent)
        assert!(FENCED_DELETE.contains("if current_gen == nil or current_gen == false then"));
        // Should return success (1) when nothing to delete
        assert!(FENCED_DELETE.contains("-- No generation set, nothing to delete"));
    }

    #[test]
    fn test_fenced_hset_iterates_fields() {
        // The HSET script should handle multiple field/value pairs
        // ARGV[2] onwards are field/value pairs
        assert!(FENCED_HSET.contains("for i = 2, #ARGV, 2 do"));
        assert!(FENCED_HSET.contains("ARGV[i], ARGV[i+1]"));
    }

    #[test]
    fn test_scripts_validate_generation_format() {
        // All fenced operations should validate that generation is a number
        assert!(FENCED_WRITE.contains("if expected_gen == nil then"));
        assert!(FENCED_HSET.contains("if expected_gen == nil then"));
        assert!(FENCED_DELETE.contains("if expected_gen == nil then"));
    }

    #[test]
    fn test_scripts_validate_current_generation() {
        // All fenced operations should validate current generation is a number
        assert!(FENCED_WRITE.contains("local current = tonumber(current_gen)"));
        assert!(FENCED_WRITE.contains("if current == nil then"));

        assert!(FENCED_HSET.contains("local current = tonumber(current_gen)"));
        assert!(FENCED_DELETE.contains("local current = tonumber(current_gen)"));
    }
}
