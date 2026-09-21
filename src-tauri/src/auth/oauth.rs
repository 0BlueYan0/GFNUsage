//! 授權碼 + PKCE 這條路上，兩個客戶端共用的那幾件事。
//!
//! 迴圈埠登入（`bind_first_free`／`wait_for_code`／`exchange`）已經拆掉：
//! 帳號頁那顆 client_id 不收 localhost 回呼，而逐場紀錄只有它拿得到
//! （spike 3a）。現在走的是 `webview` 與 `window` 那條。
//!
//! 留在這裡的都是那條路還在用的：PKCE 的 verifier 與 challenge、query 的
//! 編碼字元集、等使用者完成登入的上限。

use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use sha2::{Digest, Sha256};

use crate::error::GfnError;

/// 等使用者在登入視窗裡完成登入的上限。夠一個人找密碼、收兩步驟驗證簡訊，
/// 又不至於讓忘了這回事的人留下一個永遠開著的視窗。
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// query 參數的編碼字元集：RFC 3986 的 unreserved 字元（`A-Za-z0-9-._~`）
/// 原樣保留，其餘一律編碼。
///
/// 不能直接用 `NON_ALPHANUMERIC`：它連 `-` 和 `_` 都編成 `%2D`、`%5F`，
/// 而 `code_challenge` 是 base64url，本來就含這兩個字元。伺服器若拿沒解碼
/// 的字串去比對 PKCE，就會無聲對不上 —— 症狀是一個看不出原因的 400。
/// `client_id` 裡也有 `-`，同理。
pub(crate) const QUERY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// PKCE 的 `code_verifier`（RFC 7636 §4.1）。
///
/// 32 bytes 的作業系統亂數，以 base64url 無填充編碼後正好 43 個字元，
/// 落在規格要求的 43–128 之間，字元集也天然符合 unreserved。
/// 同一個函式也用來產生 `nonce` 與 `state` —— 需求一模一樣：夠長、
/// 不可預測、URL 安全。
pub fn verifier() -> Result<String, GfnError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| GfnError::LoginFailed(format!("取不到亂數：{e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// `code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))`（RFC 7636 §4.2）。
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
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
}
