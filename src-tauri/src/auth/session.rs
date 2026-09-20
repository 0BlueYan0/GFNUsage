use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use crate::error::GfnError;

/// 從 GFN 客戶端匯入的憑證。這是啟動 token 輪替所需的全部資料。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSession {
    pub client_token: String,
    pub sub: String,
    /// `client_token` 的到期時刻，取自 `clientTokenExpiry`。
    /// 舊版客戶端或手改過的檔案可能沒有這個欄位。
    pub client_token_expires_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct SharedStorage {
    #[serde(rename = "starfleetSession")]
    starfleet_session: StarfleetSession,
}

#[derive(Deserialize)]
struct StarfleetSession {
    data: String,
}

#[derive(Deserialize)]
struct SessionPayload {
    #[serde(rename = "clientToken")]
    client_token: String,
    /// epoch 毫秒。GFN 客戶端自己算好並存起來的到期時刻 ——
    /// `/token` 的回應裡沒有這個值，所以這是唯一撿得到現成答案的地方。
    #[serde(rename = "clientTokenExpiry")]
    client_token_expiry: Option<i64>,
    user: SessionUser,
}

#[derive(Deserialize)]
struct SessionUser {
    sub: String,
}

/// 解碼 `starfleetSession.data`。編碼鏈為 base64 -> URL-decode -> JSON。
pub fn decode_session_data(data: &str) -> Result<ImportedSession, GfnError> {
    let mut padded = data.trim().to_string();
    while padded.len() % 4 != 0 {
        padded.push('=');
    }

    let bytes = STANDARD
        .decode(&padded)
        .map_err(|e| GfnError::InvalidCredential(format!("base64 解碼失敗：{e}")))?;
    let text = String::from_utf8(bytes)
        .map_err(|e| GfnError::InvalidCredential(format!("內容不是有效的 UTF-8：{e}")))?;
    let json = percent_encoding::percent_decode_str(&text)
        .decode_utf8()
        .map_err(|e| GfnError::InvalidCredential(format!("URL 解碼失敗：{e}")))?;

    let payload: SessionPayload = serde_json::from_str(&json)
        .map_err(|e| GfnError::InvalidCredential(format!("JSON 結構非預期：{e}")))?;

    if payload.client_token.is_empty() || payload.user.sub.is_empty() {
        return Err(GfnError::InvalidCredential(
            "clientToken 或 sub 為空".to_string(),
        ));
    }

    Ok(ImportedSession {
        client_token: payload.client_token,
        sub: payload.user.sub,
        client_token_expires_at: payload
            .client_token_expiry
            .and_then(|ms| Utc.timestamp_millis_opt(ms).single()),
    })
}

/// 讀取 GFN 客戶端的 `sharedstorage.json` 並取出憑證。
/// 該檔案帶有 UTF-8 BOM，需先剝除。
pub fn read_shared_storage(path: &Path) -> Result<ImportedSession, GfnError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| GfnError::SharedStorageNotFound(format!("{}：{e}", path.display())))?;
    let body = raw.trim_start_matches('\u{feff}');
    let storage: SharedStorage = serde_json::from_str(body)
        .map_err(|e| GfnError::InvalidCredential(format!("sharedstorage.json 結構非預期：{e}")))?;
    decode_session_data(&storage.starfleet_session.data)
}

/// GFN 客戶端在本機的預設路徑。非 Windows 平台回傳 `None`。
pub fn default_shared_storage_path() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local_app_data)
            .join("NVIDIA Corporation")
            .join("GeForceNOW")
            .join("sharedstorage.json"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// base64( urlencode( {"clientToken":"CT123","user":{"sub":"SUB456"}} ) )
    const SAMPLE: &str = "JTdCJTIyY2xpZW50VG9rZW4lMjIlM0ElMjJDVDEyMyUyMiUyQyUyMnVzZXIlMjIlM0ElN0IlMjJzdWIlMjIlM0ElMjJTVUI0NTYlMjIlN0QlN0Q=";

    /// 用同一條編碼鏈（JSON -> URL-encode -> base64）造測試資料。
    /// 手工重算一長串 base64 每改一個欄位就得重來一遍，不划算。
    fn encode(json: &str) -> String {
        let escaped =
            percent_encoding::utf8_percent_encode(json, percent_encoding::NON_ALPHANUMERIC)
                .to_string();
        STANDARD.encode(escaped)
    }

    /// GFN 客戶端自己算好了 client_token 的到期時刻，撿現成的就好。
    #[test]
    fn reads_the_client_token_expiry() {
        let data = encode(
            r#"{"clientToken":"CT123","clientTokenExpiry":1796462263706,"user":{"sub":"SUB456"}}"#,
        );

        let session = decode_session_data(&data).unwrap();

        assert_eq!(
            session.client_token_expires_at,
            Some(Utc.timestamp_millis_opt(1796462263706).unwrap())
        );
    }

    /// 舊版客戶端或手改過的檔案可能沒有這個欄位。沒有就是沒有，
    /// 不要拿 `Utc::now()` 回填 —— 那等於假造一個晚了幾十天的到期日。
    #[test]
    fn tolerates_a_missing_client_token_expiry() {
        let session = decode_session_data(SAMPLE).unwrap();

        assert_eq!(session.client_token_expires_at, None);
    }

    #[test]
    fn decodes_base64_then_urlencoded_json() {
        let session = decode_session_data(SAMPLE).unwrap();
        assert_eq!(session.client_token, "CT123");
        assert_eq!(session.sub, "SUB456");
    }

    #[test]
    fn tolerates_missing_base64_padding() {
        let unpadded = SAMPLE.trim_end_matches('=');
        let session = decode_session_data(unpadded).unwrap();
        assert_eq!(session.client_token, "CT123");
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_session_data("not base64 at all !!!").is_err());
    }

    #[test]
    fn reads_shared_storage_with_bom() {
        let dir = std::env::temp_dir().join("gfnusage-test-bom");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sharedstorage.json");
        let body = format!(
            "\u{feff}{{\"starfleetSession\":{{\"authProvider\":\"starfleet\",\"data\":\"{SAMPLE}\"}}}}"
        );
        std::fs::write(&path, body).unwrap();

        let session = read_shared_storage(&path).unwrap();
        assert_eq!(session.client_token, "CT123");
    }
}
