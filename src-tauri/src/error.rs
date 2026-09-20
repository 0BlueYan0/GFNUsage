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
