use thiserror::Error;

#[derive(Debug, Error)]
pub enum GfnError {
    #[error("憑證格式無效：{0}")]
    InvalidCredential(String),

    #[error("找不到 GeForce NOW 客戶端資料：{0}")]
    SharedStorageNotFound(String),

    /// 金鑰儲存區裡沒有憑證。這不是錯誤狀態，只是還沒匯入。
    #[error("尚未連結 NVIDIA 帳號")]
    NotLinked,

    #[error("需要重新登入")]
    NeedsLogin,

    /// 登入流程本身沒走完：連接埠被占用、使用者取消、逾時、回呼帶著 error。
    ///
    /// 與 `NeedsLogin` 分開：那個是既有憑證被伺服器拒絕（要重新登入），
    /// 這個是新的登入沒成功（既有憑證，如果有的話，完全沒事）。
    #[error("登入未完成：{0}")]
    LoginFailed(String),

    /// NVIDIA 限制同時有效的 access_token 數量。撞到上限不是憑證失效，
    /// 重新登入也沒用 —— 只能等既有的 token 過期（最多 1 小時）。
    #[error("向 NVIDIA 索取的 token 太多了，等既有的過期後會自動恢復（最多 1 小時）")]
    TooManyTokens,

    /// 被限流。憑證沒問題，下個週期再試就好。
    #[error("NVIDIA 暫時拒絕請求（429），稍後會自動重試")]
    RateLimited,

    #[error("金鑰儲存區存取失敗：{0}")]
    Keychain(String),

    #[error("網路錯誤：{0}")]
    Network(String),

    #[error("NVIDIA 回應格式非預期：{0}")]
    UnexpectedResponse(String),

    #[error("讀取檔案失敗：{0}")]
    Io(#[from] std::io::Error),
}

/// 描述一段回應的「形狀」——只列出頂層欄位名稱，絕不吐出任何值。
///
/// 解析失敗時，唯一需要知道的就是「少了哪個欄位、對方到底給了什麼」，
/// 而欄位名稱本身不是機密。沒有這個，`error decoding response body`
/// 這句話除了再登入一次燒掉一顆 token 之外，沒有任何辦法查下去。
pub fn describe_shape(body: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(serde_json::Value::Object(map)) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            format!("實際收到的欄位：{}", keys.join("、"))
        }
        Ok(_) => "回應是 JSON，但不是物件".to_string(),
        Err(_) => format!("回應不是 JSON（{} bytes）", body.len()),
    }
}

/// 取出 OAuth 錯誤回應裡的 `error` 與 `error_description`（RFC 6749 §5.2）。
///
/// 只取這兩個具名欄位，不是把整包 body 倒出來 —— 錯誤回應照理不含憑證，
/// 但「照理」不值得賭，而這兩個欄位就是全部需要的東西。不是預期的形狀時
/// 退回 `describe_shape`，至少還說得出對方給了什麼。
pub fn describe_oauth_error(body: &str) -> String {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(body) else {
        return describe_shape(body);
    };
    let field = |key: &str| map.get(key).and_then(serde_json::Value::as_str);

    match (field("error"), field("error_description")) {
        (Some(error), Some(description)) => format!("{error}：{description}"),
        (Some(text), None) | (None, Some(text)) => text.to_string(),
        (None, None) => describe_shape(body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 這是第一次實測真正需要的那一行：哪個參數錯了。
    #[test]
    fn an_oauth_error_reads_out_the_code_and_the_description() {
        let body =
            r#"{"error":"invalid_grant","error_description":"code_verifier does not match"}"#;

        assert_eq!(
            describe_oauth_error(body),
            "invalid_grant：code_verifier does not match"
        );
    }

    #[test]
    fn an_oauth_error_without_a_description_is_just_the_code() {
        assert_eq!(
            describe_oauth_error(r#"{"error":"invalid_request"}"#),
            "invalid_request"
        );
    }

    /// 伺服器回了 JSON 但不是 OAuth 那套欄位。至少說得出它給了什麼，
    /// 而且一樣不吐出任何值。
    #[test]
    fn a_body_without_oauth_fields_falls_back_to_the_shape() {
        let described = describe_oauth_error(r#"{"message":"nope","code":42}"#);

        assert!(described.contains("message"), "{described}");
        assert!(described.contains("code"), "{described}");
        assert!(!described.contains("nope"), "不該吐出值：{described}");
    }

    #[test]
    fn a_body_that_is_not_json_falls_back_to_the_shape() {
        let described = describe_oauth_error("<html>502 Bad Gateway</html>");

        assert!(described.contains("不是 JSON"), "{described}");
    }
}
