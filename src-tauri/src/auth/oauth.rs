//! localhost 迴圈 OAuth 登入（授權碼 + PKCE S256）。
//!
//! 參數樣板取自 GFN 客戶端的 bundle 與
//! `Mall\sharedssets\config\config.json`，**未經實測**（spec §4.1）。
//! 從已安裝 GFN 的機器匯入那條備援路徑仍然保留，失敗時還有路可走。

use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use sha2::{Digest, Sha256};

use crate::auth::refresh::STARFLEET_CLIENT_ID;
use crate::error::GfnError;

/// NVIDIA 白名單上的迴圈埠，來自客戶端設定的 `starfleet.portNumbers`。
/// 不能自己挑：不在這份清單上的 `redirect_uri` 會被拒絕。
pub const PORTS: [u16; 5] = [2259, 6460, 7119, 8870, 9096];

/// 等使用者在瀏覽器裡完成登入的上限。夠一個人找密碼、收兩步驟驗證簡訊，
/// 又不至於讓忘了這回事的人留下一個永遠掛著的監聽埠。
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// query 參數的編碼字元集：RFC 3986 的 unreserved 字元（`A-Za-z0-9-._~`）
/// 原樣保留，其餘一律編碼。
///
/// 不能直接用 `NON_ALPHANUMERIC`：它連 `-` 和 `_` 都編成 `%2D`、`%5F`，
/// 而 `code_challenge` 是 base64url，本來就含這兩個字元。伺服器若拿沒解碼
/// 的字串去比對 PKCE，就會無聲對不上 —— 症狀是一個看不出原因的 400。
/// `client_id` 裡也有 `-`，同理。
const QUERY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

const DEVICE_ID: &str = "gfnclient";
/// `tk_client` 是拿到 90 天 `client_token` 的關鍵，不能省。
const SCOPE: &str = "openid consent email tk_client age";
const UI_LOCALES: &str = "zh-TW";

/// PKCE 的 `code_verifier`（RFC 7636 §4.1）。
///
/// 32 bytes 的作業系統亂數，以 base64url 無填充編碼後正好 43 個字元，
/// 落在規格要求的 43–128 之間，字元集也天然符合 unreserved。
/// 同一個函式也用來產生 `nonce` —— 需求一模一樣：夠長、不可預測、URL 安全。
pub fn verifier() -> Result<String, GfnError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| GfnError::LoginFailed(format!("取不到亂數：{e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// `code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))`（RFC 7636 §4.2）。
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// 迴圈回呼網址。**沒有尾斜線**，而且用 `localhost` 而不是 `127.0.0.1`。
///
/// 綁定 socket 時用 `127.0.0.1` 是對的（只收本機連線），但送給 NVIDIA 的
/// 字串必須跟客戶端樣板逐字元相同，而且授權與換碼兩步要一模一樣。
pub fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}")
}

/// 授權網址。參數順序照抄客戶端 bundle 的樣板。
pub fn authorize_url(base: &str, redirect_uri: &str, challenge: &str, nonce: &str) -> String {
    let q = |value: &str| utf8_percent_encode(value, QUERY).to_string();
    format!(
        "{base}/authorize?response_type=code&device_id={device_id}&scope={scope}\
         &client_id={client_id}&redirect_uri={redirect}&ui_locales={locales}\
         &nonce={nonce}&prompt=select_account\
         &code_challenge={challenge}&code_challenge_method=S256",
        device_id = q(DEVICE_ID),
        scope = q(SCOPE),
        client_id = q(STARFLEET_CLIENT_ID),
        redirect = q(redirect_uri),
        locales = q(UI_LOCALES),
        nonce = q(nonce),
        challenge = q(challenge),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7636 附錄 B 的測試向量。這一對值是規格書寫死的，
    /// 對得上就代表 S256 的實作沒有搞錯編碼或摘要。
    const RFC_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const RFC_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    #[test]
    fn challenge_matches_the_rfc_test_vector() {
        assert_eq!(challenge(RFC_VERIFIER), RFC_CHALLENGE);
    }

    /// RFC 7636 §4.1：43 到 128 個 unreserved 字元。
    /// 32 bytes 的亂數以 base64url 無填充編碼正好是 43 個字元。
    #[test]
    fn verifier_is_a_valid_length_and_charset() {
        let v = verifier().unwrap();

        assert_eq!(v.len(), 43);
        assert!(v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn two_verifiers_differ() {
        assert_ne!(verifier().unwrap(), verifier().unwrap());
    }

    /// 送出去的字面值必須是 `http://localhost:{port}`。綁定時用 `127.0.0.1`
    /// 是對的，但 NVIDIA 白名單上的字串是 localhost，而且授權與換碼兩次
    /// 要逐字元相同 —— 差一個尾斜線就是一個看不出原因的 400。
    #[test]
    fn redirect_uri_uses_localhost_without_a_trailing_slash() {
        assert_eq!(redirect_uri(2259), "http://localhost:2259");
    }

    #[test]
    fn authorize_url_carries_every_required_parameter() {
        let url = authorize_url(
            "https://login.nvidia.com",
            &redirect_uri(2259),
            "CHALLENGE",
            "NONCE",
        );

        assert!(url.starts_with("https://login.nvidia.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("device_id=gfnclient"));
        assert!(url.contains("code_challenge=CHALLENGE"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("nonce=NONCE"));
        assert!(url.contains("prompt=select_account"));
        assert!(url.contains(&format!("client_id={STARFLEET_CLIENT_ID}")));
        // scope 裡的 tk_client 就是拿到 90 天 client_token 的關鍵。
        assert!(url.contains("tk_client"));
        // 空白與冒號斜線都必須編碼過，否則這串網址在瀏覽器裡會斷掉。
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A2259"));
        assert!(!url.contains(' '));
    }

    /// `code_challenge` 是 base64url，天生含 `-` 與 `_`。把它們編成
    /// `%2D`／`%5F` 之後，伺服器若拿沒解碼的字串比對 PKCE 就會對不上，
    /// 而症狀只會是一個看不出原因的 400。
    #[test]
    fn unreserved_characters_are_left_alone() {
        let url = authorize_url("https://x", "http://localhost:2259", RFC_CHALLENGE, "N-1_2");

        assert!(url.contains(&format!("code_challenge={RFC_CHALLENGE}")));
        assert!(url.contains("nonce=N-1_2"));
    }
}
