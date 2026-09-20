use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::session::ImportedSession;
use crate::error::GfnError;

const SERVICE: &str = "GFNUsage";
const ACCOUNT: &str = "nvidia-credentials";

/// 保存在金鑰儲存區裡的完整工作階段。
///
/// 除了長效的 `client_token`，也存下最近一次拿到的 `id_token` 與其效期。
/// 這不只是快取：NVIDIA 限制同時有效的 access_token 數量，每次程序啟動
/// 就重新刷新會很快撞到上限。存下來，重開後就能沿用還沒過期的那顆。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    pub client_token: String,
    pub sub: String,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub id_token_expires_at: Option<DateTime<Utc>>,
}

impl From<ImportedSession> for StoredSession {
    fn from(session: ImportedSession) -> Self {
        Self {
            client_token: session.client_token,
            sub: session.sub,
            id_token: None,
            id_token_expires_at: None,
        }
    }
}

/// 憑證儲存介面。正式環境使用 OS 金鑰儲存區，測試使用記憶體實作。
pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<StoredSession>, GfnError>;
    fn save(&self, session: &StoredSession) -> Result<(), GfnError>;
    fn clear(&self) -> Result<(), GfnError>;
}

/// Windows 認證管理員 / macOS 鑰匙圈。
pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> Result<keyring::Entry, GfnError> {
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| GfnError::Keychain(e.to_string()))
    }
}

impl TokenStore for KeyringStore {
    fn load(&self) -> Result<Option<StoredSession>, GfnError> {
        match Self::entry()?.get_password() {
            Ok(raw) => {
                let stored: StoredSession = serde_json::from_str(&raw)
                    .map_err(|e| GfnError::Keychain(format!("儲存的憑證無法解析：{e}")))?;
                Ok(Some(stored))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(GfnError::Keychain(e.to_string())),
        }
    }

    fn save(&self, session: &StoredSession) -> Result<(), GfnError> {
        let raw = serde_json::to_string(session).map_err(|e| GfnError::Keychain(e.to_string()))?;
        Self::entry()?
            .set_password(&raw)
            .map_err(|e| GfnError::Keychain(e.to_string()))
    }

    fn clear(&self) -> Result<(), GfnError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(GfnError::Keychain(e.to_string())),
        }
    }
}

/// 測試用。不觸碰作業系統。
pub struct MemoryStore {
    inner: Mutex<Option<StoredSession>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenStore for MemoryStore {
    fn load(&self) -> Result<Option<StoredSession>, GfnError> {
        Ok(self.inner.lock().unwrap().clone())
    }

    fn save(&self, session: &StoredSession) -> Result<(), GfnError> {
        *self.inner.lock().unwrap() = Some(session.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), GfnError> {
        *self.inner.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StoredSession {
        ImportedSession {
            client_token: "CT123".into(),
            sub: "SUB456".into(),
        }
        .into()
    }

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryStore::new();
        assert_eq!(store.load().unwrap(), None);

        store.save(&sample()).unwrap();
        assert_eq!(store.load().unwrap(), Some(sample()));

        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn memory_store_overwrites_on_save() {
        let store = MemoryStore::new();
        store.save(&sample()).unwrap();

        let mut rotated = sample();
        rotated.client_token = "CT-ROTATED".into();
        store.save(&rotated).unwrap();

        assert_eq!(store.load().unwrap(), Some(rotated));
    }

    #[test]
    fn imported_session_starts_without_an_id_token() {
        let stored = sample();
        assert_eq!(stored.id_token, None);
        assert_eq!(stored.id_token_expires_at, None);
    }

    /// 舊版本存的內容沒有 id_token 欄位，升級後必須還讀得出來。
    #[test]
    fn reads_records_written_before_id_token_was_stored() {
        let legacy = r#"{"client_token":"CT123","sub":"SUB456"}"#;
        let stored: StoredSession = serde_json::from_str(legacy).unwrap();
        assert_eq!(stored.client_token, "CT123");
        assert_eq!(stored.id_token, None);
    }
}
