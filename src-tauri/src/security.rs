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
    #[error("Invalid plugin signature: {0}")]
    SignatureInvalid(String),
}

/// 官方插件签名公钥(base64 编码的 32 字节 Ed25519 公钥)。
///
/// - `None`:未配置信任锚 → 跳过强制验签(仍按 sha256 校验完整性)。
/// - `Some(key)`:市场清单带 `signature` 的插件包**必须**验签通过才能安装。
///
/// 生成方式:`./scripts/gen-sign-key.sh`(输出公钥 base64,填入此处)。
pub const OFFICIAL_PUBLIC_KEY: Option<&str> = None;

/// 验证插件包签名(Ed25519,对下载的原始 zip 字节验签)。
/// `signature_b64` / `public_key_b64` 均为 base64 编码。
pub fn verify_plugin_signature(
    data: &[u8],
    signature_b64: &str,
    public_key_b64: &str,
) -> Result<(), SecurityError> {
    use base64::Engine;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64.trim())
        .map_err(|e| SecurityError::SignatureInvalid(format!("bad signature base64: {}", e)))?;
    let sig = Signature::from_slice(&sig_bytes)
        .map_err(|_| SecurityError::SignatureInvalid("bad signature length".into()))?;

    let key_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64.trim())
        .map_err(|e| SecurityError::SignatureInvalid(format!("bad key base64: {}", e)))?
        .try_into()
        .map_err(|_| SecurityError::SignatureInvalid("bad public key length".into()))?;
    let key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| SecurityError::SignatureInvalid("bad public key".into()))?;

    key.verify(data, &sig)
        .map_err(|_| SecurityError::SignatureInvalid("verification failed".into()))
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
    // 十六进制比较统一小写(R-25):清单/脚本生成的哈希可能大写,大小写不敏感比较防误判
    if actual.to_lowercase() != expected.trim().to_lowercase() {
        return Err(SecurityError::Sha256Mismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

/// 校验插件路径位于用户目录内(预留 API,供将来扩展使用)。
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};

    /// 固定种子的测试密钥对(确定性,不依赖随机源)。
    fn test_signing_key() -> SigningKey {
        let mut seed = [0u8; 32];
        seed[0] = 42;
        SigningKey::from_bytes(&seed)
    }

    #[test]
    fn signature_roundtrip_ok() {
        let signing = test_signing_key();
        let verifying = signing.verifying_key();
        let msg = b"plugin zip bytes 0123456789";
        let sig = signing.sign(msg);

        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let key_b64 = base64::engine::general_purpose::STANDARD.encode(verifying.to_bytes());

        assert!(verify_plugin_signature(msg, &sig_b64, &key_b64).is_ok());
    }

    #[test]
    fn signature_tampered_rejected() {
        let signing = test_signing_key();
        let verifying = signing.verifying_key();
        let msg = b"plugin zip bytes";
        let sig = signing.sign(msg);

        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        let key_b64 = base64::engine::general_purpose::STANDARD.encode(verifying.to_bytes());

        assert!(verify_plugin_signature(b"tampered bytes", &sig_b64, &key_b64).is_err());
    }

    #[test]
    fn signature_bad_inputs_rejected() {
        assert!(verify_plugin_signature(b"x", "!!!not-base64!!!", "AAAA").is_err());
        assert!(verify_plugin_signature(b"x", "c2ln", "AAAA").is_err()); // 签名长度不对
    }
}
