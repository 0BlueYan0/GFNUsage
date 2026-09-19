use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

use crate::error::GfnError;

/// 從 GFN 客戶端匯入的憑證。這是啟動 token 輪替所需的全部資料。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSession {
    pub client_token: String,
    pub sub: String,
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
