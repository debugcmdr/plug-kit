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
    let canonical_base = base.canonicalize().map_err(|e| {
        SecurityError::PathOutsideRoot(format!("Failed to canonicalize base: {}", e))
    })?;
    let canonical_entry = entry_path.canonicalize().map_err(|e| {
        SecurityError::PathOutsideRoot(format!("Failed to canonicalize entry: {}", e))
    })?;

    if !canonical_entry.starts_with(&canonical_base) {
        return Err(SecurityError::ZipSlip(format!(
            "Path traversal: {:?} not under {:?}",
            canonical_entry, canonical_base
        )));
    }
    Ok(canonical_entry)
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
