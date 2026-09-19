use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::auth::session::ImportedSession;
use crate::error::GfnError;

const SERVICE: &str = "GFNUsage";
const ACCOUNT: &str = "nvidia-credentials";

/// 憑證儲存介面。正式環境使用 OS 金鑰儲存區，測試使用記憶體實作。
pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<ImportedSession>, GfnError>;
    fn save(&self, session: &ImportedSession) -> Result<(), GfnError>;
    fn clear(&self) -> Result<(), GfnError>;
}

#[derive(Serialize, Deserialize)]
struct Stored {
    client_token: String,
    sub: String,
}

/// Windows 認證管理員 / macOS 鑰匙圈。
pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> Result<keyring::Entry, GfnError> {
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| GfnError::Keychain(e.to_string()))
    }
}

impl TokenStore for KeyringStore {
    fn load(&self) -> Result<Option<ImportedSession>, GfnError> {
        match Self::entry()?.get_password() {
            Ok(raw) => {
                let stored: Stored = serde_json::from_str(&raw)
                    .map_err(|e| GfnError::Keychain(format!("儲存的憑證無法解析：{e}")))?;
                Ok(Some(ImportedSession {
                    client_token: stored.client_token,
                    sub: stored.sub,
                }))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(GfnError::Keychain(e.to_string())),
        }
    }

    fn save(&self, session: &ImportedSession) -> Result<(), GfnError> {
        let raw = serde_json::to_string(&Stored {
            client_token: session.client_token.clone(),
            sub: session.sub.clone(),
        })
        .map_err(|e| GfnError::Keychain(e.to_string()))?;
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
    inner: Mutex<Option<ImportedSession>>,
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
    fn load(&self) -> Result<Option<ImportedSession>, GfnError> {
        Ok(self.inner.lock().unwrap().clone())
    }

    fn save(&self, session: &ImportedSession) -> Result<(), GfnError> {
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

    fn sample() -> ImportedSession {
        ImportedSession {
            client_token: "CT123".into(),
            sub: "SUB456".into(),
        }
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

        let rotated = ImportedSession {
            client_token: "CT-ROTATED".into(),
            sub: "SUB456".into(),
        };
        store.save(&rotated).unwrap();

        assert_eq!(store.load().unwrap(), Some(rotated));
    }
}
