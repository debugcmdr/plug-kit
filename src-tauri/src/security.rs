use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Zip-Slip detected: {0}")]
    ZipSlip(String),
    #[error("SHA256 mismatch: expected {expected}, got {actual}")]
    Sha256Mismatch { expected: String, actual: String },
    #[error("Path outside allowed root: {0}")]
    PathOutsideRoot(String),
    #[error("Command timed out after {0}ms")]
    Timeout(u64),
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),
    #[error("Subprocess error: {0}")]
    SubprocessError(String),
}

pub fn validate_unzip_path(base: &Path, entry_path: &Path) -> Result<PathBuf, SecurityError> {
    // `entry_path` is the FULL target path (base.join(zip_entry_name)), which is
    // absolute. Lexically normalize it (resolve `.`/`..`) WITHOUT touching the FS
    // — the extraction target dir often doesn't exist yet, so canonicalize would
    // fail and block every install. Then require the result stays under `base`.
    let normalized = normalize_absolute(entry_path)?;
    if !normalized.starts_with(base) {
        return Err(SecurityError::ZipSlip(format!(
            "Path traversal: {:?} not under {:?}",
            normalized, base
        )));
    }
    Ok(normalized)
}

/// Lexically normalize an absolute-or-relative path, resolving `.` / `..`
/// components. Rejects `..` that would escape above the anchor (root for
/// absolute paths, empty for relative).
fn normalize_absolute(path: &Path) -> Result<PathBuf, SecurityError> {
    let mut result = PathBuf::new();
    let mut anchored = false; // saw RootDir => `..` at root is a no-op, not an escape
    for comp in path.components() {
        match comp {
            std::path::Component::RootDir => {
                result.push(std::path::MAIN_SEPARATOR.to_string());
                anchored = true;
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !result.pop() && !anchored {
                    return Err(SecurityError::ZipSlip(format!(
                        "Path escapes via '..': {:?}",
                        path
                    )));
                }
                // If anchored (absolute), pop() at root just stays at root.
            }
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

pub fn validate_sha256(data: &[u8], expected: &str) -> Result<(), SecurityError> {
    use sha2::{Sha256, Digest};
    let actual = format!("{:x}", Sha256::digest(data));
    if actual != expected {
        return Err(SecurityError::Sha256Mismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

pub fn validate_plugin_path(plugin_path: &Path) -> Result<(), SecurityError> {
    let base = dirs::home_dir()
        .ok_or_else(|| SecurityError::PathOutsideRoot("Cannot determine home directory".into()))?;
    if !plugin_path.starts_with(&base) {
        return Err(SecurityError::PathOutsideRoot(format!(
            "Plugin path {:?} is outside home directory",
            plugin_path
        )));
    }
    Ok(())
}
