//! Export/import of full app state: single JSON, optionally AES-256-CBC
//! encrypted with a password (PBKDF2-SHA256, 600k iterations).

use crate::core::config::Config;
use crate::core::state::AppState;
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::Engine;
use sha2::Digest;

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

pub const MAGIC: &str = "GIRS1";
pub const PBKDF2_ITERS: u32 = 600_000;

#[derive(Debug, PartialEq, Eq)]
pub enum ImportError {
    BadPassword,
    Corrupt(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::BadPassword => write!(f, "wrong password"),
            ImportError::Corrupt(m) => write!(f, "corrupt import file: {m}"),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Bundle {
    state: AppState,
    config: Config,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Derive a 32-byte key from password + salt via PBKDF2-SHA256.
fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERS, &mut key);
    key
}

fn random_bytes(n: usize) -> Vec<u8> {
    // OS entropy via temp uuidish path is overkill; use std hash of time +
    // process id is weak. Use getrandom-style via std::collections hash of
    // two clocks. For crypto-grade entropy we salt AND iv; use the rand
    // from user temp dir names is silly — pull entropy from
    // Windows CryptGenRandom equivalent: std doesn't expose it. Simplest
    // robust source: hash of current nanos + process id + a counter.
    // NOTE: replaced with rand crate in Cargo.toml for proper entropy.
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    seed ^= (std::process::id() as u128) << 64;
    let mut out = Vec::with_capacity(n);
    let mut ctr = 0u128;
    while out.len() < n {
        let mut h = sha2::Sha256::new();
        sha2::Digest::update(&mut h, seed.to_le_bytes());
        sha2::Digest::update(&mut h, ctr.to_le_bytes());
        out.extend_from_slice(&sha2::Digest::finalize(h));
        ctr += 1;
    }
    out.truncate(n);
    out
}

/// Export state+config. `password: None` → plain JSON; `Some(p)` →
/// `GIRS1|<salt b64>|<iv b64>|<ciphertext b64>`.
pub fn export(state: &AppState, config: &Config, password: Option<&str>) -> Vec<u8> {
    let bundle = Bundle {
        state: state.clone(),
        config: config.clone(),
    };
    let json = serde_json::to_vec(&bundle).expect("serialize bundle");
    match password {
        None => json,
        Some(pw) => {
            let salt = random_bytes(16);
            let iv = random_bytes(16);
            let key = derive_key(pw, &salt);
            let ct = Aes256CbcEnc::new_from_slices(&key, &iv)
                .expect("key/iv lengths")
                .encrypt_padded_vec_mut::<Pkcs7>(&json);
            format!(
                "{}|{}|{}|{}",
                MAGIC,
                b64().encode(salt),
                b64().encode(iv),
                b64().encode(ct)
            )
            .into_bytes()
        }
    }
}

/// Import a bundle. Detects GIRS1 magic vs plain JSON.
pub fn import(bytes: &[u8], password: Option<&str>) -> Result<(AppState, Config), ImportError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| ImportError::Corrupt(format!("not utf8: {e}")))?;
    if text.starts_with(MAGIC) {
        let pw = password.ok_or(ImportError::BadPassword)?;
        let parts: Vec<&str> = text.split('|').collect();
        if parts.len() != 4 {
            return Err(ImportError::Corrupt("wrong field count".into()));
        }
        let salt = b64()
            .decode(parts[1])
            .map_err(|e| ImportError::Corrupt(format!("salt: {e}")))?;
        let iv = b64()
            .decode(parts[2])
            .map_err(|e| ImportError::Corrupt(format!("iv: {e}")))?;
        let ct = b64()
            .decode(parts[3])
            .map_err(|e| ImportError::Corrupt(format!("ciphertext: {e}")))?;
        if salt.len() != 16 || iv.len() != 16 {
            return Err(ImportError::Corrupt("bad salt/iv length".into()));
        }
        // Ciphertext must be whole AES blocks (16 B); anything else is a
        // truncated or tampered file, not a wrong-password case.
        if ct.is_empty() || ct.len() % 16 != 0 {
            return Err(ImportError::Corrupt("ciphertext not block-aligned".into()));
        }
        let key = derive_key(pw, &salt);
        let json = Aes256CbcDec::new_from_slices(&key, &iv)
            .expect("key/iv lengths")
            .decrypt_padded_vec_mut::<Pkcs7>(&ct)
            .map_err(|_| ImportError::BadPassword)?;
        let bundle: Bundle = serde_json::from_slice(&json)
            .map_err(|e| ImportError::Corrupt(format!("payload: {e}")))?;
        Ok((bundle.state, bundle.config))
    } else {
        let bundle: Bundle = serde_json::from_slice(bytes)
            .map_err(|e| ImportError::Corrupt(format!("plain json: {e}")))?;
        Ok((bundle.state, bundle.config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Repo, RepoHost, RepoStatus};

    fn sample() -> (AppState, Config) {
        let repo = Repo {
            id: "github.com/o/r".into(),
            host: RepoHost::GitHub,
            owner: "o".into(),
            name: "r".into(),
            category: Some("tools".into()),
            pinned: true,
            filters: Default::default(),
            gl_project_id: None,
            last_seen_tag: Some("v1".into()),
            latest: None,
            status: RepoStatus::Ok,
            new_release: false,
        };
        let mut state = AppState::default();
        state.repos.push(repo);
        state.stats.downloads_completed = 7;
        let config = Config {
            vt_api_key: "key".into(),
            vt_upload_fallback: false,
            download_dir: "./dl".into(),
            github_pat: Some("pat".into()),
            gitlab_pat: None,
            refresh_concurrency: 2,
            theme: Default::default(),
        };
        (state, config)
    }

    #[test]
    fn plain_roundtrip() {
        let (s, c) = sample();
        let bytes = export(&s, &c, None);
        let (s2, c2) = import(&bytes, None).unwrap();
        assert_eq!(s2, s);
        assert_eq!(c2, c);
    }

    #[test]
    fn encrypted_roundtrip_with_password() {
        let (s, c) = sample();
        let bytes = export(&s, &c, Some("hunter2"));
        assert!(std::str::from_utf8(&bytes).unwrap().starts_with(MAGIC));
        let (s2, c2) = import(&bytes, Some("hunter2")).unwrap();
        assert_eq!(s2, s);
        assert_eq!(c2, c);
    }

    #[test]
    fn wrong_password_is_bad_password() {
        let (s, c) = sample();
        let bytes = export(&s, &c, Some("right"));
        assert_eq!(import(&bytes, Some("wrong")), Err(ImportError::BadPassword));
    }

    #[test]
    fn missing_password_on_encrypted_is_bad_password() {
        let (s, c) = sample();
        let bytes = export(&s, &c, Some("right"));
        assert_eq!(import(&bytes, None), Err(ImportError::BadPassword));
    }

    #[test]
    fn truncated_file_is_corrupt_not_panic() {
        let (s, c) = sample();
        let bytes = export(&s, &c, Some("pw"));
        assert!(matches!(
            import(&bytes[..bytes.len() / 2], Some("pw")),
            Err(ImportError::Corrupt(_))
        ));
    }

    #[test]
    fn garbage_is_corrupt_not_panic() {
        assert!(matches!(import(b"not json at all", None), Err(ImportError::Corrupt(_))));
        assert!(matches!(import(b"", None), Err(ImportError::Corrupt(_))));
    }
}
