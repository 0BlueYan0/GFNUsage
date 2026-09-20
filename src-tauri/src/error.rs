use thiserror::Error;

#[derive(Debug, Error)]
pub enum GfnError {
    #[error("憑證格式無效：{0}")]
    InvalidCredential(String),

    #[error("找不到 GeForce NOW 客戶端資料：{0}")]
    SharedStorageNotFound(String),

    #[error("需要重新登入")]
    NeedsLogin,

    /// NVIDIA 限制同時有效的 access_token 數量。撞到上限不是憑證失效，
    /// 重新登入也沒用 —— 只能等既有的 token 過期（最多 1 小時）。
    #[error("向 NVIDIA 索取的 token 太多了，等既有的過期後會自動恢復（最多 1 小時）")]
    TooManyTokens,

    #[error("金鑰儲存區存取失敗：{0}")]
    Keychain(String),

    #[error("網路錯誤：{0}")]
    Network(String),

    #[error("NVIDIA 回應格式非預期：{0}")]
    UnexpectedResponse(String),

    #[error("讀取檔案失敗：{0}")]
    Io(#[from] std::io::Error),
}
