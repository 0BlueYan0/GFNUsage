use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::session::ImportedSession;
use crate::error::GfnError;

const SERVICE: &str = "GFNUsage";
const ACCOUNT: &str = "nvidia-credentials";
const ID_TOKEN_ACCOUNT: &str = "nvidia-id-token";

/// Windows 認證管理員單筆祕密的上限（bytes，內容以 UTF-16 存放），超過時
/// keyring 會拒絕寫入。憑證與 id_token 各自一筆就是為了各自塞得下：
/// 一顆 id_token 約 1150 字元，和憑證塞在同一筆會超過這個上限。
pub const WINDOWS_CREDENTIAL_BLOB_LIMIT: usize = 2560;

/// `client_token` 的效期。`/token` 的回應裡沒有這個值，但 GFN 客戶端在
/// `sharedstorage.json` 存了 `clientTokenExpiryLength = 7776000000` 毫秒，
/// 剛好 90 天。自行登入拿到的憑證就用這個長度推算到期時刻。
pub const CLIENT_TOKEN_LIFETIME_DAYS: i64 = 90;

/// 保存在金鑰儲存區裡的長效憑證。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    pub client_token: String,
    pub sub: String,

    /// `client_token` 的到期時刻。
    ///
    /// **輪替不會重設它。** 實測：GFN 客戶端在一小時前才刷新過，
    /// `clientTokenExpiry` 仍指向 13.85 天前那次登入 + 90 天。
    /// 效期錨定在最初的登入，不是最後一次輪替。
    ///
    /// 里程碑 1／2 存下的紀錄沒有這個欄位，會是 `None`。那時不顯示到期
    /// 橫幅就好，不要拿 `Utc::now()` 回填 —— 那是假造一個晚了幾十天的日期。
    #[serde(default)]
    pub client_token_expires_at: Option<DateTime<Utc>>,
}

impl From<ImportedSession> for StoredSession {
    fn from(session: ImportedSession) -> Self {
        Self {
            client_token: session.client_token,
            sub: session.sub,
            client_token_expires_at: session.client_token_expires_at,
        }
    }
}

/// 憑證儲存介面。正式環境使用 OS 金鑰儲存區，測試使用記憶體實作。
///
/// id_token 另存一筆。它只是快取（NVIDIA 限制同時有效的 access_token 數量，
/// 每次啟動都重鑄會撞上限），寫不進去不能牽連憑證本身那筆。
pub trait TokenStore: Send + Sync {
    fn load(&self) -> Result<Option<StoredSession>, GfnError>;
    fn save(&self, session: &StoredSession) -> Result<(), GfnError>;
    /// 清除憑證，連同 id_token。
    fn clear(&self) -> Result<(), GfnError>;

    fn load_id_token(&self) -> Result<Option<String>, GfnError>;
    fn save_id_token(&self, id_token: &str) -> Result<(), GfnError>;
    fn clear_id_token(&self) -> Result<(), GfnError>;
}

/// Windows 認證管理員 / macOS 鑰匙圈。
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account: &str) -> Result<keyring::Entry, GfnError> {
        keyring::Entry::new(SERVICE, account).map_err(|e| GfnError::Keychain(e.to_string()))
    }

    fn read(account: &str) -> Result<Option<String>, GfnError> {
        match Self::entry(account)?.get_password() {
            Ok(raw) => Ok(Some(raw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(GfnError::Keychain(e.to_string())),
        }
    }

    fn write(account: &str, raw: &str) -> Result<(), GfnError> {
        Self::entry(account)?
            .set_password(raw)
            .map_err(|e| GfnError::Keychain(e.to_string()))
    }

    fn delete(account: &str) -> Result<(), GfnError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(GfnError::Keychain(e.to_string())),
        }
    }
}

impl TokenStore for KeyringStore {
    fn load(&self) -> Result<Option<StoredSession>, GfnError> {
        let Some(raw) = Self::read(ACCOUNT)? else {
            return Ok(None);
        };
        let stored = serde_json::from_str(&raw)
            .map_err(|e| GfnError::Keychain(format!("儲存的憑證無法解析：{e}")))?;
        Ok(Some(stored))
    }

    fn save(&self, session: &StoredSession) -> Result<(), GfnError> {
        let raw = serde_json::to_string(session).map_err(|e| GfnError::Keychain(e.to_string()))?;
        Self::write(ACCOUNT, &raw)
    }

    fn clear(&self) -> Result<(), GfnError> {
        Self::delete(ID_TOKEN_ACCOUNT)?;
        Self::delete(ACCOUNT)
    }

    fn load_id_token(&self) -> Result<Option<String>, GfnError> {
        Self::read(ID_TOKEN_ACCOUNT)
    }

    fn save_id_token(&self, id_token: &str) -> Result<(), GfnError> {
        Self::write(ID_TOKEN_ACCOUNT, id_token)
    }

    fn clear_id_token(&self) -> Result<(), GfnError> {
        Self::delete(ID_TOKEN_ACCOUNT)
    }
}

/// 測試用。不觸碰作業系統。
pub struct MemoryStore {
    session: Mutex<Option<StoredSession>>,
    id_token: Mutex<Option<String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            id_token: Mutex::new(None),
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
        Ok(self.session.lock().unwrap().clone())
    }

    fn save(&self, session: &StoredSession) -> Result<(), GfnError> {
        *self.session.lock().unwrap() = Some(session.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), GfnError> {
        *self.id_token.lock().unwrap() = None;
        *self.session.lock().unwrap() = None;
        Ok(())
    }

    fn load_id_token(&self) -> Result<Option<String>, GfnError> {
        Ok(self.id_token.lock().unwrap().clone())
    }

    fn save_id_token(&self, id_token: &str) -> Result<(), GfnError> {
        *self.id_token.lock().unwrap() = Some(id_token.to_string());
        Ok(())
    }

    fn clear_id_token(&self) -> Result<(), GfnError> {
        *self.id_token.lock().unwrap() = None;
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
            client_token_expires_at: None,
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
    fn id_token_is_stored_separately_from_the_credential() {
        let store = MemoryStore::new();
        store.save(&sample()).unwrap();
        assert_eq!(store.load_id_token().unwrap(), None);

        store.save_id_token("JWT").unwrap();

        assert_eq!(store.load_id_token().unwrap().as_deref(), Some("JWT"));
        assert_eq!(store.load().unwrap(), Some(sample()));
    }

    #[test]
    fn clear_id_token_keeps_the_credential() {
        let store = MemoryStore::new();
        store.save(&sample()).unwrap();
        store.save_id_token("JWT").unwrap();

        store.clear_id_token().unwrap();

        assert_eq!(store.load_id_token().unwrap(), None);
        assert_eq!(store.load().unwrap(), Some(sample()));
    }

    #[test]
    fn clearing_the_credential_also_drops_the_id_token() {
        let store = MemoryStore::new();
        store.save(&sample()).unwrap();
        store.save_id_token("JWT").unwrap();

        store.clear().unwrap();

        assert_eq!(store.load_id_token().unwrap(), None);
    }

    /// 前一版把 id_token 塞在同一筆紀錄裡；使用者的金鑰儲存區現在就是這個樣子，
    /// 升級後必須還讀得出來。
    ///
    /// 這個測試同時證明了反方向：新版多出來的 `client_token_expires_at`
    /// 在舊紀錄裡不存在，反序列化要補成 `None` 而不是整筆讀不出來。
    #[test]
    fn ignores_the_id_token_fields_an_older_version_embedded() {
        let legacy = r#"{"client_token":"CT123","sub":"SUB456","id_token":null,"id_token_expires_at":null}"#;
        let stored: StoredSession = serde_json::from_str(legacy).unwrap();
        assert_eq!(stored, sample());
    }

    /// Windows 認證管理員單筆祕密上限 2560 bytes（UTF-16）。
    /// 把 id_token 和憑證塞在同一筆就是超過這個上限才炸的，所以兩筆各自都要塞得下。
    #[test]
    fn each_record_fits_the_windows_blob_limit() {
        // 到期時刻有值時才是最長的情況，用 `None` 量等於量了個寂寞。
        let session = StoredSession {
            client_token: "c".repeat(86),
            sub: "s".repeat(43),
            client_token_expires_at: Some(Utc::now()),
        };
        let record = serde_json::to_string(&session).unwrap();
        let utf16_bytes = |s: &str| s.encode_utf16().count() * 2;

        assert!(utf16_bytes(&record) <= WINDOWS_CREDENTIAL_BLOB_LIMIT);
        assert!(utf16_bytes(&"j".repeat(1151)) <= WINDOWS_CREDENTIAL_BLOB_LIMIT);
    }
}
