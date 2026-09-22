//! Atomic secret-file writes for files that live in the container-RW mount.
//!
//! The helper's `runtime_dir` (`/tmp/devloop-<slug>`) is bind-mounted into the
//! dev container **read-write at the same uid** (`infra/devloop/devloop.sh:583`,
//! `--userns=keep-id`). So a semi-trusted container can create symlinks and
//! pre-place files there, and `0600`/`0700` modes give NO protection against a
//! same-uid writer. Any secret the helper writes into that directory
//! (cluster-admin kubeconfig, the CSPRNG socket auth token) must therefore be
//! written so that:
//!
//!   1. a container-placed **symlink** at the destination cannot redirect the
//!      write to an attacker-chosen path, and
//!   2. an existing file's **looser mode is not preserved** (truncate-in-place
//!      keeps the old mode; `.mode()` only applies on create), and
//!   3. a crash never leaves a **partial** secret (a half-written key would fail
//!      a concurrent reader, or worse, be read as valid).
//!
//! `atomic_write_secret` gets all three: write a fresh temp with
//! `create_new(true)` (`O_CREAT|O_EXCL`, which refuses to follow a symlink and
//! refuses to open an existing file) at mode 0600, then `fs::rename` over the
//! destination. `rename` is atomic, replaces a symlink at the destination rather
//! than writing through it, and carries the temp's mode.
//!
//! This is the ONE home for the secret-write pattern (per @dry-reviewer /
//! @team-lead P8): both `commands::generate_container_kubeconfig` and
//! `auth::write_token` route through it. Every OTHER file the helper writes into
//! the mount is non-secret and keeps an ordinary write, classified as non-secret
//! AT ITS OWN SITE — this module deliberately does NOT enumerate them, because a
//! list here would go stale on every new write (@dry-reviewer). Grep for
//! "not secret-bearing" to find the classified sites.

use crate::error::HelperError;
use ring::rand::{SecureRandom, SystemRandom};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Write `contents` to `path` atomically at mode 0600, symlink-safe.
///
/// Writes a `<file-name>.tmp.<random>` sibling in the SAME directory (so the
/// final `rename` is atomic — rename is only atomic within a filesystem), then
/// renames it over `path`.
///
/// The temp suffix is CSPRNG, NOT `<pid>` (per @security): `helper.pid` is
/// container-readable, so a `<name>.tmp.<pid>` temp could be pre-created by the
/// container to wedge a restore via the loud `create_new` error below. A random
/// suffix removes that self-DoS. A pre-existing temp is still surfaced loudly
/// (`AlreadyExists`) rather than silently removed — silently reusing a temp an
/// attacker placed would re-open the symlink race this function closes.
pub fn atomic_write_secret(path: &Path, contents: &[u8]) -> Result<(), HelperError> {
    let parent = path.parent().ok_or_else(|| HelperError::CommandFailed {
        cmd: "atomic-write".to_string(),
        detail: format!("path has no parent directory: {}", path.display()),
    })?;
    let file_name =
        path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| HelperError::CommandFailed {
                cmd: "atomic-write".to_string(),
                detail: format!("path has no file name: {}", path.display()),
            })?;

    // CSPRNG suffix (8 bytes hex) — same directory as the destination.
    let mut rand_bytes = [0u8; 8];
    SystemRandom::new()
        .fill(&mut rand_bytes)
        .map_err(|_| HelperError::CommandFailed {
            cmd: "atomic-write".to_string(),
            detail: "CSPRNG failure generating temp suffix".to_string(),
        })?;
    let tmp_path = parent.join(format!("{file_name}.tmp.{}", hex::encode(rand_bytes)));

    write_secret_via_temp(&tmp_path, path, contents)
}

/// Write `contents` to `tmp_path` with `create_new` (O_CREAT|O_EXCL) at mode
/// 0600, then `rename` it over `dest`. Split out from the CSPRNG-suffix
/// generation so the adversarial paths (pre-existing temp, symlink at the temp
/// path) are unit-testable with a known `tmp_path`.
///
/// A pre-existing `tmp_path` fails LOUDLY via `create_new`'s `AlreadyExists` —
/// we do NOT `remove_file` and retry, which would re-open the symlink race
/// `create_new` closes. The pre-existing file is left untouched.
fn write_secret_via_temp(tmp_path: &Path, dest: &Path, contents: &[u8]) -> Result<(), HelperError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(tmp_path)?; // AlreadyExists / symlink ⇒ loud error, no clobber.
                          // Clean up the temp on any error path so a failed write leaves nothing stale.
    let write_result = (|| -> Result<(), HelperError> {
        file.write_all(contents)?;
        file.flush()?;
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(tmp_path);
        return Err(e);
    }
    drop(file);

    // Atomic replace. Carries the temp's 0600 mode; replaces a symlink at
    // `dest` rather than writing through it.
    if let Err(e) = fs::rename(tmp_path, dest) {
        let _ = fs::remove_file(tmp_path);
        return Err(HelperError::Io(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn test_writes_content_at_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        atomic_write_secret(&path, b"top-secret").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"top-secret");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "secret must be written 0600");
    }

    #[test]
    fn test_replaces_existing_looser_mode_file_at_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        // Pre-existing file with a LOOSER mode (0644). truncate-in-place would
        // preserve 0644; rename-over must land 0600.
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        atomic_write_secret(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "replaced file must be 0600, not the old 0644");
    }

    #[test]
    fn test_symlink_at_destination_is_replaced_not_followed() {
        // A container-placed symlink at the destination must NOT redirect the
        // write to the symlink target; rename replaces the link itself.
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside-target");
        fs::write(&outside, b"original-outside").unwrap();
        let path = dir.path().join("secret");
        symlink(&outside, &path).unwrap();

        atomic_write_secret(&path, b"secret-body").unwrap();

        // The symlink target is untouched; `path` is now a regular file 0600.
        assert_eq!(fs::read(&outside).unwrap(), b"original-outside");
        let meta = fs::symlink_metadata(&path).unwrap();
        assert!(
            meta.file_type().is_file(),
            "destination must be a real file"
        );
        assert_eq!(fs::read(&path).unwrap(), b"secret-body");
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn test_missing_parent_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-such-dir").join("secret");
        assert!(atomic_write_secret(&path, b"x").is_err());
    }

    #[test]
    fn test_preexisting_temp_fails_loudly_no_clobber() {
        // A pre-existing temp must fail LOUDLY (create_new AlreadyExists) and
        // must NOT be removed-and-retried; the existing temp is left untouched.
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("secret");
        let tmp = dir.path().join("secret.tmp.deadbeef");
        fs::write(&tmp, b"attacker-placed").unwrap();

        let err = write_secret_via_temp(&tmp, &dest, b"secret-body").unwrap_err();
        assert!(
            matches!(&err, HelperError::Io(e) if e.kind() == std::io::ErrorKind::AlreadyExists),
            "expected AlreadyExists Io error, got {err:?}"
        );
        // The pre-existing temp is untouched, and dest was never created.
        assert_eq!(fs::read(&tmp).unwrap(), b"attacker-placed");
        assert!(
            !dest.exists(),
            "dest must not be created on a loud temp failure"
        );
    }

    #[test]
    fn test_symlink_at_temp_path_is_not_followed() {
        // create_new (O_EXCL) refuses to follow a symlink at the temp path too —
        // a container-placed symlink there cannot redirect the secret write.
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside-target");
        fs::write(&outside, b"original-outside").unwrap();
        let dest = dir.path().join("secret");
        let tmp = dir.path().join("secret.tmp.cafebabe");
        symlink(&outside, &tmp).unwrap();

        let err = write_secret_via_temp(&tmp, &dest, b"secret-body").unwrap_err();
        assert!(matches!(err, HelperError::Io(_)));
        // The symlink target is untouched — the write did not follow the link.
        assert_eq!(fs::read(&outside).unwrap(), b"original-outside");
    }
}
