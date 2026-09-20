use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
struct Claims {
    exp: Option<i64>,
    sub: Option<String>,
    nonce: Option<String>,
}

/// 解出 payload 的 claims。刻意不驗證簽章 —— 本程式不是資源伺服器，
/// 只需要知道 token 裡寫了什麼；真正的驗證由 NVIDIA 端負責。
fn claims(token: &str) -> Option<Claims> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// 讀取 JWT 的 `exp` claim，也就是何時該換發。
pub fn expiry(token: &str) -> Option<DateTime<Utc>> {
    let exp = claims(token)?.exp?;
    Utc.timestamp_opt(exp, 0).single()
}

/// 讀取 `sub` claim。OAuth 換碼的回應沒有頂層 `sub`，但 `StoredSession`
/// 需要它才能刷新。空字串一律當成沒有：存進去的憑證一樣不能用。
pub fn subject(token: &str) -> Option<String> {
    claims(token)?.sub.filter(|sub| !sub.is_empty())
}

/// 讀取 `nonce` claim，用來比對授權請求當初送出去的值。
pub fn nonce(token: &str) -> Option<String> {
    claims(token)?.nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    /// header.payload.signature，payload 為 {"exp":1789831422}
    const TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjE3ODk4MzE0MjJ9.sig";

    /// 用給定的 claims 造一顆 JWT。簽章是假的 —— 這個模組本來就不驗簽章。
    fn jwt_with(claims: &str) -> String {
        format!(
            "eyJhbGciOiJSUzI1NiJ9.{}.sig",
            URL_SAFE_NO_PAD.encode(claims)
        )
    }

    #[test]
    fn reads_the_subject_and_nonce() {
        let token = jwt_with(r#"{"exp":1789831422,"sub":"SUB456","nonce":"N1"}"#);

        assert_eq!(subject(&token).as_deref(), Some("SUB456"));
        assert_eq!(nonce(&token).as_deref(), Some("N1"));
    }

    /// 空字串的 sub 和沒有 sub 是同一件事：存進去的憑證一樣不能用。
    #[test]
    fn an_empty_subject_counts_as_missing() {
        assert!(subject(&jwt_with(r#"{"sub":""}"#)).is_none());
    }

    #[test]
    fn a_token_without_a_subject_has_none() {
        assert!(subject(TOKEN).is_none());
        assert!(nonce(TOKEN).is_none());
    }

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
