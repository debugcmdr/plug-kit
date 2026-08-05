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
    // Lexical validation first: reject absolute paths and any `..` component.
    // Do NOT rely on canonicalize() here — the extraction target directory often
    // does not exist yet, so canonicalize would fail and block every install.
    let normalized = normalize_join(base, entry_path)?;
    if !normalized.starts_with(base) {
        return Err(SecurityError::ZipSlip(format!(
            "Path traversal: {:?} not under {:?}",
            normalized, base
        )));
    }
    Ok(normalized)
}

/// Lexically join `entry` onto `base` and normalize `.` / `..` components.
/// Rejects absolute `entry` paths and any escape above `base`.
fn normalize_join(base: &Path, entry: &Path) -> Result<PathBuf, SecurityError> {
    if entry.is_absolute() {
        return Err(SecurityError::ZipSlip(format!(
            "Absolute path in archive entry: {:?}",
            entry
        )));
    }

    let mut components = Vec::new();
    for comp in entry.components() {
        match comp {
            std::path::Component::ParentDir => {
                if components.pop().is_none() {
                    return Err(SecurityError::ZipSlip(format!(
                        "Path escapes base via '..': {:?}",
                        entry
                    )));
                }
            }
            std::path::Component::CurDir => {}
            other => components.push(other.as_os_str().to_os_string()),
        }
    }

    let mut result = base.to_path_buf();
    for comp in components {
        result.push(comp);
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
