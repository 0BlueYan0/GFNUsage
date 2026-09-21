use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::auth::jwt;
use crate::auth::store::{StoredSession, TokenStore};
use crate::error::{describe_oauth_error, describe_shape, GfnError};

pub const STARFLEET_CLIENT_ID: &str = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
pub const STARFLEET_BASE: &str = "https://login.nvidia.com";
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:client_token";

/// id_token 剩餘效期低於此值時才重新刷新。
const REFRESH_MARGIN_MINUTES: i64 = 5;

/// NVIDIA 在撞到「同時有效的 access_token 數量上限」時回應的字樣。
const TOO_MANY_TOKENS: &str = "Max allowed simultaneous valid access_token exceeded";

/// 刻意**不** derive `Debug`：這些型別裝著憑證，`{:?}` 會把內容整個印出來，
/// 而 spec §9 規定日誌與錯誤訊息不得出現憑證。這條適用本檔所有回應型別。
///
/// 刷新（`grant_type=…client_token`）的回應。這一條**有** `client_token`。
#[derive(Deserialize)]
pub struct TokenResponse {
    pub id_token: String,
    pub client_token: String,
}

/// 授權碼換發（`grant_type=authorization_code`）的回應。
///
/// **沒有 `client_token`。** 帳號頁那顆 client_id 要不到（scope 帶 `tk_client`
/// 會被拒），所以換碼只有這一段，拿到的就是一小時的 id_token。
#[derive(Deserialize)]
pub struct AuthCodeResponse {
    pub access_token: String,
    pub id_token: String,
}

/// 這次 `/token` 走的是哪一條路。
///
/// 只影響 4xx 的分類，其餘規則兩條路共用 —— 尤其是「同時有效 token 上限」，
/// 那個在哪一條路上都是會自己好的暫時狀況，不能變成叫使用者重新登入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// 拿既有的 `client_token` 換一顆新的 id_token。
    Refresh,
    /// OAuth 授權碼換發。
    Exchange,
}

/// 送一次 `/token` 並把回應分類。
///
/// 刷新（`grant_type=…client_token`）與 OAuth 換碼
/// （`grant_type=authorization_code`）共用這裡，但 4xx 的意思**不一樣**：
/// 刷新被拒代表既有憑證死了（要重新登入），換碼被拒代表這次登入沒成功
/// （既有憑證，如果有的話，完全沒事）。報錯的對象搞反，使用者會在登入
/// 途中被告知「需要重新登入」。
///
/// 換碼失敗還要把伺服器的 `error_description` 帶出來：這條路的參數是從
/// 客戶端 bundle 猜的，那句話是唯一能指出哪個參數錯了的線索。
///
/// 4xx 一律不重試（spec §4.3 規則 3）。網路層錯誤回 `Network`，
/// 由呼叫端決定要不要退避重試 —— 那種請求可能根本沒送達。
pub async fn post_token<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    auth_base: &str,
    grant: Grant,
    form: &[(&str, &str)],
) -> Result<T, GfnError> {
    let response = http
        .post(format!("{auth_base}/token"))
        .form(form)
        .send()
        .await
        .map_err(|e| GfnError::Network(e.to_string()))?;

    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(GfnError::RateLimited);
    }
    if status.is_client_error() {
        let body = response.text().await.unwrap_or_default();
        // 撞到 token 數量上限不是憑證失效，叫使用者重新登入毫無幫助。
        // 這一條在兩條路上都一樣。
        if body.contains(TOO_MANY_TOKENS) {
            return Err(GfnError::TooManyTokens);
        }
        return Err(match grant {
            Grant::Refresh => GfnError::NeedsLogin,
            Grant::Exchange => GfnError::LoginFailed(format!(
                "/token 回應 {status}；{}",
                describe_oauth_error(&body)
            )),
        });
    }
    if !status.is_success() {
        return Err(GfnError::Network(format!("/token 回應 {status}")));
    }

    // 先取文字再自己解析，而不是直接 `.json()`：後者失敗時只會回一句
    // 「error decoding response body」，連少了哪個欄位都不說。
    let body = response
        .text()
        .await
        .map_err(|e| GfnError::Network(e.to_string()))?;

    serde_json::from_str(&body).map_err(|e| {
        // grant_type 是哪一條路（刷新或換碼）的唯一線索，而且它不是機密。
        let grant = form
            .iter()
            .find(|(key, _)| *key == "grant_type")
            .map_or("（未知）", |(_, value)| *value);
        GfnError::UnexpectedResponse(format!(
            "/token（grant_type={grant}）的回應解析失敗：{e}；{}",
            describe_shape(&body)
        ))
    })
}

#[derive(Clone)]
struct CachedToken {
    id_token: String,
    expires_at: DateTime<Utc>,
}

impl CachedToken {
    fn usable(&self) -> bool {
        self.expires_at - Utc::now() > Duration::minutes(REFRESH_MARGIN_MINUTES)
    }
}

/// 取得 id_token 的唯一入口，也是所有會改動憑證儲存區的操作的唯一入口。
///
/// 內部的 mutex 保證整個程序同時只有一次刷新在進行，因此定時輪詢、開啟面板、
/// 以及 401 後的重試三者同時發生時，也不會各自呼叫一次 `/token`。
/// 匯入與登出也在同一把鎖底下做：否則進行中的刷新寫回輪替結果時，
/// 會把剛匯入的憑證蓋掉、或把剛清掉的憑證悄悄還原。
///
/// 刷新會盡量少做：NVIDIA 限制同時有效的 access_token 數量，撞到上限會被擋，
/// 所以 id_token 另外存進金鑰儲存區，程序重開後沿用而不是重鑄。
pub struct TokenManager {
    store: Arc<dyn TokenStore>,
    http: reqwest::Client,
    auth_base: String,
    cached: Mutex<Option<CachedToken>>,
    renewer: OnceLock<Renewer>,
}

/// 取得一顆新 id_token 的方法。
///
/// 做成可插拔的，是因為現在那個方法要開 webview，而 webview 需要
/// `AppHandle` —— 那是 `main` 才有的東西，測試裡不可能有。`main` 啟動時插進
/// 來，測試不插，於是測試走的還是舊的 `client_token` 刷新那條路。
///
/// 回傳 id_token 與它的到期時刻。
pub type Renewal = Pin<Box<dyn Future<Output = Result<(String, DateTime<Utc>), GfnError>> + Send>>;
pub type Renewer = Arc<dyn Fn() -> Renewal + Send + Sync>;

impl TokenManager {
    pub fn new(store: Arc<dyn TokenStore>, http: reqwest::Client, auth_base: String) -> Self {
        Self {
            store,
            http,
            auth_base,
            cached: Mutex::new(None),
            renewer: OnceLock::new(),
        }
    }

    /// 裝上取得 id_token 的方法。`main` 在 `setup` 裡呼叫一次。
    ///
    /// 同步而不是 async，而且用 `OnceLock`：它必須在第一次輪詢之前就裝好。
    /// 丟去 `spawn` 的話，輸掉那一圈的 `ensure_token` 會走 `None` 分支打
    /// `/token`，多鑄一顆 access_token，而且鑄出來的是 GFN 客戶端那顆簽的，
    /// 換不到逐場紀錄（spike 3a）。
    pub fn set_renewer(&self, renewer: Renewer) {
        let _ = self.renewer.set(renewer);
    }

    /// 回傳可用的 id_token，必要時刷新。
    pub async fn ensure_token(&self) -> Result<String, GfnError> {
        // 整段 check-and-refresh 都在鎖內，這是不可分割的。
        let mut guard = self.cached.lock().await;

        if let Some(cached) = guard.as_ref() {
            if cached.usable() {
                return Ok(cached.id_token.clone());
            }
        }

        // 程序剛啟動時記憶體是空的，但金鑰儲存區裡可能還有沒過期的 id_token。
        // 沿用它，才不會每次重開都向 NVIDIA 多要一顆。
        if let Some(persisted) = self.persisted_token() {
            if persisted.usable() {
                *guard = Some(persisted.clone());
                return Ok(persisted.id_token);
            }
        }

        let fresh = match self.renewer.get().cloned() {
            // webview 的靜默授權。它自己就是憑證的來源，不需要
            // `StoredSession` —— cookie 存在 webview 那邊。
            Some(renew) => {
                let (id_token, expires_at) = renew().await?;
                // 寫回金鑰儲存區，程序重開沿用。同刷新那條路，寫不進去不算失敗。
                if let Err(e) = self.store.save_id_token(&id_token) {
                    eprintln!(
                        "id_token（{} 字元）未能寫入金鑰儲存區，重啟後會重新取得：{e}",
                        id_token.len()
                    );
                }
                CachedToken {
                    id_token,
                    expires_at,
                }
            }
            // `refresh` 自己寫回 id_token，這裡不必再寫一次。
            None => {
                let stored = self.store.load()?.ok_or(GfnError::NotLinked)?;
                self.refresh(stored).await?
            }
        };

        *guard = Some(fresh.clone());
        Ok(fresh.id_token)
    }

    /// 收下一顆 webview 剛換到的 id_token。
    ///
    /// 沒有 `StoredSession` 可寫 —— 帳號頁那顆 client_id 不給 `client_token`
    /// （spike 3a），憑證那一半在 webview 的 cookie 裡，不歸金鑰儲存區管。
    /// 所以這裡只放 id_token，跟 `replace_credentials_with_token` 不同。
    pub async fn adopt_token(&self, id_token: &str, expires_at: DateTime<Utc>) {
        let mut guard = self.cached.lock().await;
        // 舊的 `StoredSession` 一併清掉（`clear` 連 id_token 也清，所以要先做）。
        // 留著它，面板會為一顆再也用不到的 client_token 倒數到期 ——
        // `panel_data` 的 `client_token_expires_at` 就是從那裡來的。
        let _ = self.store.clear();
        if let Err(e) = self.store.save_id_token(id_token) {
            eprintln!(
                "id_token（{} 字元）未能寫入金鑰儲存區，重啟後會重新登入：{e}",
                id_token.len()
            );
        }
        *guard = Some(CachedToken {
            id_token: id_token.to_string(),
            expires_at,
        });
    }

    /// 丟棄目前的 id_token，強制下次 `ensure_token` 重新取得。
    ///
    /// 記憶體與金鑰儲存區都要清 —— 只清記憶體的話，重新讀取又會把同一顆
    /// 已經被伺服器拒絕的 token 載回來。整段持鎖，不與刷新交錯。
    pub async fn invalidate(&self) {
        let mut guard = self.cached.lock().await;
        *guard = None;
        let _ = self.store.clear_id_token();
    }

    /// 換成另一組憑證（匯入）。舊帳號的 id_token 一併丟棄，否則換了帳號之後
    /// 還沒過期的那顆會被繼續拿來查詢，畫面上就會顯示錯的人的額度。
    pub async fn replace_credentials(&self, session: StoredSession) -> Result<(), GfnError> {
        self.replace_credentials_with_token(session, None).await
    }

    /// 同上，但順便收下一顆剛拿到的 id_token。
    ///
    /// OAuth 換碼的回應本來就附了一顆能用的 id_token。丟掉它再讓
    /// `ensure_token()` 去要一顆新的，等於白白多鑄一顆，離「同時有效
    /// access_token 上限」更近一步 —— 而剛登入完正是最不該撞上限的時候。
    ///
    /// 整段在同一把鎖底下：中途若有另一次刷新插進來，會把剛寫好的憑證
    /// 或 id_token 蓋掉。
    pub async fn replace_credentials_with_token(
        &self,
        session: StoredSession,
        id_token: Option<&str>,
    ) -> Result<(), GfnError> {
        let mut guard = self.cached.lock().await;
        *guard = None;
        self.store.clear_id_token()?;
        self.store.save(&session)?;

        let Some(id_token) = id_token else {
            return Ok(());
        };
        let Some(expires_at) = jwt::expiry(id_token) else {
            return Ok(());
        };
        // 和刷新那裡同樣的取捨：id_token 只是快取，寫不進去不算失敗。
        // 只記長度，不記內容。
        if let Err(e) = self.store.save_id_token(id_token) {
            eprintln!(
                "id_token（{} 字元）未能寫入金鑰儲存區，重啟後會重新取得：{e}",
                id_token.len()
            );
        }
        *guard = Some(CachedToken {
            id_token: id_token.to_string(),
            expires_at,
        });
        Ok(())
    }

    /// 清除憑證（登出）。
    pub async fn clear_credentials(&self) -> Result<(), GfnError> {
        let mut guard = self.cached.lock().await;
        *guard = None;
        self.store.clear()
    }

    async fn refresh(&self, stored: StoredSession) -> Result<CachedToken, GfnError> {
        let body = post_token::<TokenResponse>(
            &self.http,
            &self.auth_base,
            Grant::Refresh,
            &[
                ("grant_type", GRANT_TYPE),
                ("client_token", stored.client_token.as_str()),
                ("client_id", STARFLEET_CLIENT_ID),
                ("sub", stored.sub.as_str()),
            ],
        )
        .await?;

        let expires_at = jwt::expiry(&body.id_token)
            .ok_or_else(|| GfnError::UnexpectedResponse("id_token 沒有 exp claim".into()))?;

        // 順序至關重要：新的 client_token 必須先寫進金鑰儲存區。
        // 伺服器在回應的當下就已作廢舊的那顆；若先使用 id_token 再存檔而中途失敗，
        // 儲存區裡會留著一顆已死的憑證，使用者將被鎖在外面直到手動重新匯入。
        self.store.save(&StoredSession {
            client_token: body.client_token,
            sub: stored.sub,
            // 輪替不重設 90 天的效期（見 `StoredSession` 的註解）。這裡若改成
            // `Utc::now() + 90 天`，到期橫幅就永遠不會出現，使用者會在毫無
            // 預警的情況下被鎖在外面。
            client_token_expires_at: stored.client_token_expires_at,
        })?;

        // id_token 只是重啟後的快取，寫不進去不能讓刷新失敗 —— 憑證已經輪替了，
        // 這裡失敗就等於把使用者鎖在外面。只記長度，不記內容。
        if let Err(e) = self.store.save_id_token(&body.id_token) {
            eprintln!(
                "id_token（{} 字元）未能寫入金鑰儲存區，重啟後會重新取得：{e}",
                body.id_token.len()
            );
        }

        Ok(CachedToken {
            id_token: body.id_token,
            expires_at,
        })
    }

    /// 金鑰儲存區裡的 id_token；效期直接從 token 本身讀，不另外存。
    fn persisted_token(&self) -> Option<CachedToken> {
        let id_token = self.store.load_id_token().ok().flatten()?;
        let expires_at = jwt::expiry(&id_token)?;
        Some(CachedToken {
            id_token,
            expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::auth::session::ImportedSession;
    use crate::auth::store::MemoryStore;
    use crate::http_client;

    /// exp 設在西元 2286 年，測試期間永遠有效。
    const FAR_FUTURE_JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjk5OTk5OTk5OTl9.sig";

    /// 效期由 token 本身決定，所以「快過期」的情境要一顆真的快過期的 JWT。
    fn jwt_expiring_at(at: DateTime<Utc>) -> String {
        let claims = format!(r#"{{"exp":{}}}"#, at.timestamp());
        format!(
            "eyJhbGciOiJSUzI1NiJ9.{}.sig",
            URL_SAFE_NO_PAD.encode(claims)
        )
    }

    fn base_session() -> StoredSession {
        ImportedSession {
            client_token: "CT-OLD".into(),
            sub: "SUB456".into(),
            client_token_expires_at: None,
        }
        .into()
    }

    fn seeded_store() -> Arc<MemoryStore> {
        let store = Arc::new(MemoryStore::new());
        store.save(&base_session()).unwrap();
        store
    }

    /// 已經存有一顆 id_token 的儲存區，模擬程序重新啟動。
    fn store_with_id_token(jwt: &str) -> Arc<MemoryStore> {
        let store = seeded_store();
        store.save_id_token(jwt).unwrap();
        store
    }

    fn token_response(client_token: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "AT",
            "token_type": "Bearer",
            "expires_in": 3600,
            "client_token": client_token,
            "id_token": FAR_FUTURE_JWT,
        }))
    }

    async fn mount_token(server: &MockServer, template: ResponseTemplate, expect: u64) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(template)
            .expect(expect)
            .mount(server)
            .await;
    }

    fn manager(store: Arc<dyn TokenStore>, server: &MockServer) -> TokenManager {
        TokenManager::new(store, reqwest::Client::new(), server.uri())
    }

    /// 少了一個欄位時，訊息要講出少了哪個、以及對方到底給了什麼欄位。
    /// 只有「error decoding response body」的話，除了再登入一次燒掉一顆
    /// token 之外沒有任何辦法查下去。
    #[tokio::test]
    async fn a_missing_field_is_named_along_with_what_did_arrive() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "AT",
                "token_type": "Bearer",
                "expires_in": 3600,
                "id_token": FAR_FUTURE_JWT,
            })),
            1,
        )
        .await;

        let problem = post_token::<TokenResponse>(
            &reqwest::Client::new(),
            &server.uri(),
            Grant::Exchange,
            &[("grant_type", "authorization_code")],
        )
        .await
        .err()
        // `TokenResponse` 刻意不 derive Debug —— 它裝著憑證，`{:?}` 會把
        // 內容整個印出來。所以這裡不能用 `unwrap_err()`。
        .expect("這個回應應該要解析失敗")
        .to_string();

        assert!(problem.contains("client_token"), "{problem}");
        assert!(problem.contains("access_token"), "{problem}");
        assert!(problem.contains("authorization_code"), "{problem}");
    }

    /// spec §9：訊息裡不准出現憑證內容。欄位名稱可以，值不行。
    #[tokio::test]
    async fn the_diagnostic_never_leaks_a_token_value() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "SECRET-ACCESS-VALUE",
                "id_token": "SECRET-ID-VALUE",
            })),
            1,
        )
        .await;

        let problem = post_token::<TokenResponse>(
            &reqwest::Client::new(),
            &server.uri(),
            Grant::Exchange,
            &[("grant_type", "authorization_code")],
        )
        .await
        .err()
        // `TokenResponse` 刻意不 derive Debug —— 它裝著憑證，`{:?}` 會把
        // 內容整個印出來。所以這裡不能用 `unwrap_err()`。
        .expect("這個回應應該要解析失敗")
        .to_string();

        assert!(!problem.contains("SECRET-ACCESS-VALUE"), "{problem}");
        assert!(!problem.contains("SECRET-ID-VALUE"), "{problem}");
    }

    /// 對方回了 HTML 錯誤頁或表單編碼時，要說得出「這根本不是 JSON」。
    #[tokio::test]
    async fn a_non_json_body_says_so() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            ResponseTemplate::new(200).set_body_raw("<html>nope</html>", "text/html"),
            1,
        )
        .await;

        let problem = post_token::<TokenResponse>(
            &reqwest::Client::new(),
            &server.uri(),
            Grant::Exchange,
            &[("grant_type", "authorization_code")],
        )
        .await
        .err()
        // `TokenResponse` 刻意不 derive Debug —— 它裝著憑證，`{:?}` 會把
        // 內容整個印出來。所以這裡不能用 `unwrap_err()`。
        .expect("這個回應應該要解析失敗")
        .to_string();

        assert!(problem.contains("不是 JSON"), "{problem}");
    }

    /// 換碼的回應已經附了一顆能用的 id_token。收下它之後，第一次
    /// `ensure_token()` 不該再打一次 `/token` —— 那等於白白多鑄一顆，
    /// 而剛登入完正是最不該逼近「同時有效 access_token 上限」的時候。
    ///
    /// `expect(0)` 就是這個測試的主張：一次都不准打。
    #[tokio::test]
    async fn an_adopted_id_token_is_used_without_minting_another() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEVER"), 0).await;
        let store = Arc::new(MemoryStore::new());
        let manager = manager(store.clone(), &server);

        manager
            .replace_credentials_with_token(base_session(), Some(FAR_FUTURE_JWT))
            .await
            .unwrap();

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
        assert_eq!(
            store.load_id_token().unwrap().as_deref(),
            Some(FAR_FUTURE_JWT)
        );
    }

    /// 沒帶 id_token 的匯入維持原樣：舊帳號那顆要丟掉，下次才會重新取得。
    #[tokio::test]
    async fn importing_without_an_id_token_drops_the_previous_one() {
        let server = MockServer::start().await;
        let store = store_with_id_token(FAR_FUTURE_JWT);
        let manager = manager(store.clone(), &server);

        manager.replace_credentials(base_session()).await.unwrap();

        assert_eq!(store.load_id_token().unwrap(), None);
    }

    /// 輪替不會重設 90 天的效期。實測依據：GFN 客戶端刷新過後，
    /// `clientTokenExpiry` 仍指向最初那次登入 + 90 天。
    ///
    /// 這裡若改成「輪替時重新計時」，到期橫幅就永遠不會出現，
    /// 使用者會在毫無預警的情況下被鎖在外面。
    #[tokio::test]
    async fn rotation_keeps_the_original_client_token_expiry() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;
        let expires_at = Utc::now() + Duration::days(40);
        let store = Arc::new(MemoryStore::new());
        store
            .save(&StoredSession {
                client_token: "CT-OLD".into(),
                sub: "SUB456".into(),
                client_token_expires_at: Some(expires_at),
            })
            .unwrap();
        let manager = manager(store.clone(), &server);

        manager.ensure_token().await.unwrap();

        let stored = store.load().unwrap().unwrap();
        assert_eq!(stored.client_token, "CT-NEW");
        assert_eq!(stored.client_token_expires_at, Some(expires_at));
    }

    /// 可以指定哪一種寫入會失敗的儲存區，用來驗證兩筆紀錄的失敗互不牽連。
    struct FailingStore {
        inner: MemoryStore,
        fail_save: AtomicBool,
        fail_save_id_token: AtomicBool,
    }

    impl FailingStore {
        fn seeded() -> Arc<Self> {
            let inner = MemoryStore::new();
            inner.save(&base_session()).unwrap();
            Arc::new(Self {
                inner,
                fail_save: AtomicBool::new(false),
                fail_save_id_token: AtomicBool::new(false),
            })
        }
    }

    impl TokenStore for FailingStore {
        fn load(&self) -> Result<Option<StoredSession>, GfnError> {
            self.inner.load()
        }
        fn save(&self, session: &StoredSession) -> Result<(), GfnError> {
            if self.fail_save.load(Ordering::SeqCst) {
                return Err(GfnError::Keychain("模擬寫入失敗".into()));
            }
            self.inner.save(session)
        }
        fn clear(&self) -> Result<(), GfnError> {
            self.inner.clear()
        }
        fn load_id_token(&self) -> Result<Option<String>, GfnError> {
            self.inner.load_id_token()
        }
        fn save_id_token(&self, id_token: &str) -> Result<(), GfnError> {
            if self.fail_save_id_token.load(Ordering::SeqCst) {
                return Err(GfnError::Keychain("模擬 id_token 寫入失敗".into()));
            }
            self.inner.save_id_token(id_token)
        }
        fn clear_id_token(&self) -> Result<(), GfnError> {
            self.inner.clear_id_token()
        }
    }

    /// 裝了 renewer 就完全不碰 `/token`。
    ///
    /// 這是 spike 3a 之後整條路的重點：逐場紀錄要的是帳號頁那顆 client_id
    /// 簽的 token，`/token` 的 `client_token` 刷新換不到。真的打過去也只是
    /// 白白多鑄一顆，往「同時有效 access_token 上限」再靠近一步。
    #[tokio::test]
    async fn a_renewer_replaces_the_client_token_refresh() {
        let server = MockServer::start().await;
        // 掛一個會 panic 的 mock：走到 `/token` 就讓測試紅。
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::new());
        let manager = TokenManager::new(store.clone(), reqwest::Client::new(), server.uri());
        let calls = Arc::new(AtomicUsize::new(0));

        let counter = calls.clone();
        manager.set_renewer(Arc::new(move || {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok((FAR_FUTURE_JWT.to_string(), Utc::now() + Duration::hours(1)))
            })
        }));

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // 第二次走記憶體快取，不該再叫一次。
        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // 而且寫回了金鑰儲存區，程序重開沿用。
        assert_eq!(
            store.load_id_token().unwrap().as_deref(),
            Some(FAR_FUTURE_JWT)
        );
    }

    /// renewer 這條路不需要 `StoredSession` —— cookie 存在 webview 那邊，
    /// 金鑰儲存區裡沒有 client_token 也該走得通。
    #[tokio::test]
    async fn a_renewer_does_not_need_stored_credentials() {
        let manager = TokenManager::new(
            Arc::new(MemoryStore::new()),
            reqwest::Client::new(),
            "http://127.0.0.1:1".into(),
        );
        manager.set_renewer(Arc::new(move || {
            Box::pin(
                async move { Ok((FAR_FUTURE_JWT.to_string(), Utc::now() + Duration::hours(1))) },
            )
        }));

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    /// 收下 webview 那顆 token 時，舊的 `StoredSession` 要跟著走。
    ///
    /// 留著它，面板會為一顆再也用不到的 client_token 倒數到期，而那顆
    /// 憑證從升級的那一刻起就不會再被刷新了。
    #[tokio::test]
    async fn adopting_a_token_drops_the_credential_it_replaces() {
        let store = seeded_store();
        let manager = TokenManager::new(
            store.clone(),
            reqwest::Client::new(),
            "http://127.0.0.1:1".into(),
        );

        manager
            .adopt_token(FAR_FUTURE_JWT, Utc::now() + Duration::hours(1))
            .await;

        assert_eq!(store.load().unwrap(), None);
        assert_eq!(
            store.load_id_token().unwrap().as_deref(),
            Some(FAR_FUTURE_JWT)
        );
        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    #[tokio::test]
    async fn refreshes_and_returns_id_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("client_token=CT-OLD"))
            .and(body_string_contains("sub=SUB456"))
            .respond_with(token_response("CT-NEW"))
            .mount(&server)
            .await;

        let manager = manager(seeded_store(), &server);

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    #[tokio::test]
    async fn persists_rotated_client_token_and_id_token() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let store = seeded_store();
        let manager = manager(store.clone(), &server);
        manager.ensure_token().await.unwrap();

        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-NEW");
        assert_eq!(
            store.load_id_token().unwrap().as_deref(),
            Some(FAR_FUTURE_JWT)
        );
    }

    /// 這條是 access_token 數量上限的防線：重開程序不該再要一顆。
    #[tokio::test]
    async fn reuses_the_persisted_id_token_after_a_restart() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 0).await;

        // 全新的 TokenManager，記憶體快取是空的，等同程序剛啟動。
        let manager = manager(store_with_id_token(FAR_FUTURE_JWT), &server);

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    #[tokio::test]
    async fn refreshes_when_the_persisted_id_token_is_almost_expired() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let almost_expired = jwt_expiring_at(Utc::now() + Duration::minutes(2));
        let manager = manager(store_with_id_token(&almost_expired), &server);

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    #[tokio::test]
    async fn concurrent_callers_refresh_only_once() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let manager = Arc::new(manager(seeded_store(), &server));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let m = manager.clone();
                tokio::spawn(async move { m.ensure_token().await })
            })
            .collect();

        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }
    }

    #[tokio::test]
    async fn reuses_cached_token_without_hitting_network() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let manager = manager(seeded_store(), &server);
        manager.ensure_token().await.unwrap();
        manager.ensure_token().await.unwrap();
        manager.ensure_token().await.unwrap();
    }

    #[tokio::test]
    async fn four_xx_means_needs_login_and_leaves_store_untouched() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_request",
                "error_description": "ClientId does not match",
            })),
            1, // 不得重試
        )
        .await;

        let store = seeded_store();
        let manager = manager(store.clone(), &server);

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::NeedsLogin)
        ));
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-OLD");
    }

    /// 撞到 token 數量上限時，不能告訴使用者去重新登入 —— 那沒有用。
    #[tokio::test]
    async fn hitting_the_token_cap_is_not_reported_as_a_login_problem() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_request",
                "error_description": TOO_MANY_TOKENS,
            })),
            1,
        )
        .await;

        let store = seeded_store();
        let manager = manager(store.clone(), &server);

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::TooManyTokens)
        ));
        // 憑證仍然有效，不能動它。
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-OLD");
    }

    /// 被限流也不是憑證問題。
    #[tokio::test]
    async fn too_many_requests_is_reported_as_rate_limited_not_login() {
        let server = MockServer::start().await;
        mount_token(&server, ResponseTemplate::new(429), 1).await;

        let store = seeded_store();
        let manager = manager(store.clone(), &server);

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::RateLimited)
        ));
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-OLD");
    }

    #[tokio::test]
    async fn missing_credentials_means_not_linked() {
        let manager = TokenManager::new(
            Arc::new(MemoryStore::new()),
            reqwest::Client::new(),
            "http://127.0.0.1:1".into(),
        );

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::NotLinked)
        ));
    }

    #[tokio::test]
    async fn invalidate_clears_the_persisted_id_token_too() {
        let store = store_with_id_token(FAR_FUTURE_JWT);
        let manager = TokenManager::new(
            store.clone(),
            reqwest::Client::new(),
            "http://127.0.0.1:1".into(),
        );

        manager.invalidate().await;

        assert_eq!(store.load_id_token().unwrap(), None);
        // 長效憑證必須留著，否則使用者得重新匯入。
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-OLD");
    }

    #[tokio::test]
    async fn invalidate_forces_a_fresh_refresh() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 2).await;

        let manager = manager(seeded_store(), &server);
        manager.ensure_token().await.unwrap();
        manager.ensure_token().await.unwrap(); // 用快取，不打網路

        manager.invalidate().await;
        manager.ensure_token().await.unwrap(); // 快取沒了，必須重抓
    }

    /// spec §10「寫入後立即中斷」：憑證寫不進去時，同批的 id_token 不得被拿來用，
    /// 儲存區也不能被改到一半。這條記錄的是被鎖在外面的情境，不是避免它。
    #[tokio::test]
    async fn a_failed_credential_write_does_not_expose_the_new_id_token() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 2).await;

        let store = FailingStore::seeded();
        store.fail_save.store(true, Ordering::SeqCst);
        let manager = manager(store.clone(), &server);

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::Keychain(_))
        ));
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-OLD");
        assert_eq!(store.load_id_token().unwrap(), None);

        // 快取也必須是空的：下一次呼叫得重新走一遍網路（mock 期望 2 次）。
        store.fail_save.store(false, Ordering::SeqCst);
        manager.ensure_token().await.unwrap();
    }

    /// id_token 那筆寫不進去只是少了重啟後的快取，不能讓整次刷新失敗 ——
    /// 憑證已經輪替了，失敗就等於把使用者鎖在外面。
    #[tokio::test]
    async fn a_failed_id_token_write_is_not_fatal() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let store = FailingStore::seeded();
        store.fail_save_id_token.store(true, Ordering::SeqCst);
        let manager = manager(store.clone(), &server);

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-NEW");
        assert_eq!(store.load_id_token().unwrap(), None);

        // 記憶體快取仍然有效，不會再打一次網路（mock 期望 1 次）。
        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    /// 「登出」若不等進行中的刷新結束，刷新寫回的憑證會把清除悄悄還原。
    #[tokio::test]
    async fn clear_credentials_waits_for_an_in_flight_refresh() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            token_response("CT-NEW").set_delay(StdDuration::from_millis(200)),
            1,
        )
        .await;

        let store = seeded_store();
        let manager = Arc::new(manager(store.clone(), &server));

        let in_flight = {
            let m = manager.clone();
            tokio::spawn(async move { m.ensure_token().await })
        };
        tokio::time::sleep(StdDuration::from_millis(50)).await;

        manager.clear_credentials().await.unwrap();
        in_flight.await.unwrap().unwrap();

        assert_eq!(store.load().unwrap(), None);
        assert_eq!(store.load_id_token().unwrap(), None);
        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::NotLinked)
        ));
    }

    /// 匯入新帳號時同理：不能被舊帳號進行中的輪替結果蓋掉。
    #[tokio::test]
    async fn replace_credentials_waits_for_an_in_flight_refresh() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("client_token=CT-OLD"))
            .respond_with(token_response("CT-NEW").set_delay(StdDuration::from_millis(200)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("client_token=CT-IMPORTED"))
            .respond_with(token_response("CT-NEW-2"))
            .expect(1)
            .mount(&server)
            .await;

        let store = seeded_store();
        let manager = Arc::new(manager(store.clone(), &server));

        let in_flight = {
            let m = manager.clone();
            tokio::spawn(async move { m.ensure_token().await })
        };
        tokio::time::sleep(StdDuration::from_millis(50)).await;

        manager
            .replace_credentials(StoredSession {
                client_token: "CT-IMPORTED".into(),
                sub: "SUB-NEW".into(),
                client_token_expires_at: None,
            })
            .await
            .unwrap();
        in_flight.await.unwrap().unwrap();

        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-IMPORTED");
        assert_eq!(store.load_id_token().unwrap(), None);

        // 舊帳號的 id_token 快取也必須丟掉：下一次要用新憑證刷新。
        manager.ensure_token().await.unwrap();
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-NEW-2");
    }

    /// 連線卡住時必須放棄，否則 mutex 會被永遠持有，整個程式再也不更新。
    #[tokio::test]
    async fn refresh_gives_up_on_a_stalled_token_endpoint() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            token_response("CT-NEW").set_delay(StdDuration::from_secs(5)),
            1,
        )
        .await;

        let manager = TokenManager::new(
            seeded_store(),
            http_client(StdDuration::from_millis(200)),
            server.uri(),
        );

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::Network(_))
        ));
    }
}
