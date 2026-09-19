use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
struct Claims {
    exp: Option<i64>,
}

/// 讀取 JWT 的 `exp` claim。刻意不驗證簽章 —— 本程式不是資源伺服器，
/// 只需要知道何時該換發；真正的驗證由 NVIDIA 端負責。
pub fn expiry(token: &str) -> Option<DateTime<Utc>> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Claims = serde_json::from_slice(&bytes).ok()?;
    let exp = claims.exp?;
    Utc.timestamp_opt(exp, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// header.payload.signature，payload 為 {"exp":1789831422}
    const TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjE3ODk4MzE0MjJ9.sig";

    #[test]
    fn reads_exp_claim() {
        let at = expiry(TOKEN).unwrap();
        assert_eq!(at.timestamp(), 1789831422);
    }

    #[test]
    fn returns_none_for_non_jwt() {
        assert!(expiry("opaque-token-without-dots").is_none());
    }

    #[test]
    fn returns_none_when_exp_missing() {
        assert!(expiry("eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ4In0.sig").is_none());
    }
}
