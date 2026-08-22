use semver::{Version, VersionReq, Error as SemverError};
use thiserror::Error;

pub const CURRENT_BRIDGE_VERSION: &str = "1.0.0";

#[derive(Error, Debug)]
pub enum BridgeError {
    /// 预留:版本不匹配时构造。
    #[allow(dead_code)]
    #[error("Version mismatch: required={required}, actual={actual}")]
    VersionMismatch { required: String, actual: String },
    #[error("Invalid semver: {0}")]
    InvalidSemver(#[from] SemverError),
}

pub fn check_bridge_version(required: &str, actual: &str) -> Result<bool, BridgeError> {
    let required_ver = VersionReq::parse(required)?;
    let actual_ver = Version::parse(actual)?;
    Ok(required_ver.matches(&actual_ver))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_match() {
        assert!(check_bridge_version(">=1.0.0", "1.0.0").unwrap());
        assert!(check_bridge_version(">=1.0.0", "1.1.0").unwrap());
        assert!(!check_bridge_version(">=2.0.0", "1.0.0").unwrap());
        // Invalid constraint parses to an error
        assert!(check_bridge_version("not-a-semver", "1.0.0").is_err());
    }
}
