use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::auth::jwt;
use crate::auth::store::{StoredSession, TokenStore};
use crate::error::GfnError;

pub const STARFLEET_CLIENT_ID: &str = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
pub const STARFLEET_BASE: &str = "https://login.nvidia.com";
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:client_token";

/// id_token 剩餘效期低於此值時才重新刷新。
const REFRESH_MARGIN_MINUTES: i64 = 5;

/// NVIDIA 在撞到「同時有效的 access_token 數量上限」時回應的字樣。
const TOO_MANY_TOKENS: &str = "Max allowed simultaneous valid access_token exceeded";

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
    client_token: String,
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

/// 取得 id_token 的唯一入口。
///
/// 內部的 mutex 保證整個程序同時只有一次刷新在進行，因此定時輪詢、開啟面板、
/// 以及 401 後的重試三者同時發生時，也不會各自呼叫一次 `/token`。
///
/// 刷新會盡量少做：NVIDIA 限制同時有效的 access_token 數量，撞到上限會被擋，
/// 所以 id_token 連同效期一起存進金鑰儲存區，程序重開後沿用而不是重鑄。
pub struct TokenManager {
    store: Arc<dyn TokenStore>,
    http: reqwest::Client,
    auth_base: String,
    cached: Mutex<Option<CachedToken>>,
}

impl TokenManager {
    pub fn new(store: Arc<dyn TokenStore>, http: reqwest::Client, auth_base: String) -> Self {
        Self {
            store,
            http,
            auth_base,
            cached: Mutex::new(None),
        }
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

        let stored = self.store.load()?.ok_or(GfnError::NeedsLogin)?;

        // 程序剛啟動時記憶體是空的，但金鑰儲存區裡可能還有沒過期的 id_token。
        // 沿用它，才不會每次重開都向 NVIDIA 多要一顆。
        if let Some(persisted) = persisted_token(&stored) {
            if persisted.usable() {
                *guard = Some(persisted.clone());
                return Ok(persisted.id_token);
            }
        }

        let fresh = self.refresh(stored).await?;
        *guard = Some(fresh.clone());
        Ok(fresh.id_token)
    }

    /// 丟棄目前的 id_token，強制下次 `ensure_token` 重新取得。
    ///
    /// 記憶體與金鑰儲存區都要清 —— 只清記憶體的話，重新讀取又會把同一顆
    /// 已經被伺服器拒絕的 token 載回來。
    pub async fn invalidate(&self) {
        *self.cached.lock().await = None;
        if let Ok(Some(mut stored)) = self.store.load() {
            stored.id_token = None;
            stored.id_token_expires_at = None;
            let _ = self.store.save(&stored);
        }
    }

    async fn refresh(&self, stored: StoredSession) -> Result<CachedToken, GfnError> {
        let response = self
            .http
            .post(format!("{}/token", self.auth_base))
            .form(&[
                ("grant_type", GRANT_TYPE),
                ("client_token", stored.client_token.as_str()),
                ("client_id", STARFLEET_CLIENT_ID),
                ("sub", stored.sub.as_str()),
            ])
            .send()
            .await
            .map_err(|e| GfnError::Network(e.to_string()))?;

        let status = response.status();
        if status.is_client_error() {
            let body = response.text().await.unwrap_or_default();
            // 撞到 token 數量上限不是憑證失效，叫使用者重新登入毫無幫助。
            return Err(if body.contains(TOO_MANY_TOKENS) {
                GfnError::TooManyTokens
            } else {
                GfnError::NeedsLogin
            });
        }
        if !status.is_success() {
            return Err(GfnError::Network(format!("/token 回應 {status}")));
        }

        let body: TokenResponse = response
            .json()
            .await
            .map_err(|e| GfnError::UnexpectedResponse(e.to_string()))?;

        let expires_at = jwt::expiry(&body.id_token)
            .ok_or_else(|| GfnError::UnexpectedResponse("id_token 沒有 exp claim".into()))?;

        // 順序至關重要：新的 client_token 必須先寫進金鑰儲存區。
        // 伺服器在回應的當下就已作廢舊的那顆；若先使用 id_token 再存檔而中途失敗，
        // 儲存區裡會留著一顆已死的憑證，使用者將被鎖在外面直到手動重新匯入。
        self.store.save(&StoredSession {
            client_token: body.client_token,
            sub: stored.sub,
            id_token: Some(body.id_token.clone()),
            id_token_expires_at: Some(expires_at),
        })?;

        Ok(CachedToken {
            id_token: body.id_token,
            expires_at,
        })
    }
}

fn persisted_token(stored: &StoredSession) -> Option<CachedToken> {
    Some(CachedToken {
        id_token: stored.id_token.clone()?,
        expires_at: stored.id_token_expires_at?,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::auth::session::ImportedSession;
    use crate::auth::store::MemoryStore;

    /// exp 設在西元 2286 年，測試期間永遠有效。
    const FAR_FUTURE_JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjk5OTk5OTk5OTl9.sig";

    fn base_session() -> StoredSession {
        ImportedSession {
            client_token: "CT-OLD".into(),
            sub: "SUB456".into(),
        }
        .into()
    }

    fn seeded_store() -> Arc<MemoryStore> {
        let store = Arc::new(MemoryStore::new());
        store.save(&base_session()).unwrap();
        store
    }

    /// 已經存有一顆 id_token 的儲存區，模擬程序重新啟動。
    fn store_with_id_token(expires_at: DateTime<Utc>) -> Arc<MemoryStore> {
        let store = Arc::new(MemoryStore::new());
        store
            .save(&StoredSession {
                id_token: Some(FAR_FUTURE_JWT.into()),
                id_token_expires_at: Some(expires_at),
                ..base_session()
            })
            .unwrap();
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

        let manager = TokenManager::new(seeded_store(), reqwest::Client::new(), server.uri());

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    #[tokio::test]
    async fn persists_rotated_client_token_and_id_token() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let store = seeded_store();
        let manager = TokenManager::new(store.clone(), reqwest::Client::new(), server.uri());
        manager.ensure_token().await.unwrap();

        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.client_token, "CT-NEW");
        assert_eq!(saved.id_token.as_deref(), Some(FAR_FUTURE_JWT));
        assert!(saved.id_token_expires_at.is_some());
    }

    /// 這條是 access_token 數量上限的防線：重開程序不該再要一顆。
    #[tokio::test]
    async fn reuses_the_persisted_id_token_after_a_restart() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 0).await;

        // 全新的 TokenManager，記憶體快取是空的，等同程序剛啟動。
        let manager = TokenManager::new(
            store_with_id_token(Utc::now() + Duration::hours(3)),
            reqwest::Client::new(),
            server.uri(),
        );

        assert_eq!(manager.ensure_token().await.unwrap(), FAR_FUTURE_JWT);
    }

    #[tokio::test]
    async fn refreshes_when_the_persisted_id_token_is_almost_expired() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let manager = TokenManager::new(
            store_with_id_token(Utc::now() + Duration::minutes(2)),
            reqwest::Client::new(),
            server.uri(),
        );

        manager.ensure_token().await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_callers_refresh_only_once() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let manager = Arc::new(TokenManager::new(
            seeded_store(),
            reqwest::Client::new(),
            server.uri(),
        ));

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

        let manager = TokenManager::new(seeded_store(), reqwest::Client::new(), server.uri());
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
        let manager = TokenManager::new(store.clone(), reqwest::Client::new(), server.uri());

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
        let manager = TokenManager::new(store.clone(), reqwest::Client::new(), server.uri());

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::TooManyTokens)
        ));
        // 憑證仍然有效，不能動它。
        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-OLD");
    }

    #[tokio::test]
    async fn missing_credentials_means_needs_login() {
        let manager = TokenManager::new(
            Arc::new(MemoryStore::new()),
            reqwest::Client::new(),
            "http://127.0.0.1:1".into(),
        );

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::NeedsLogin)
        ));
    }

    #[tokio::test]
    async fn invalidate_clears_the_persisted_id_token_too() {
        let store = store_with_id_token(Utc::now() + Duration::hours(3));
        let manager = TokenManager::new(
            store.clone(),
            reqwest::Client::new(),
            "http://127.0.0.1:1".into(),
        );

        manager.invalidate().await;

        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.id_token, None);
        assert_eq!(saved.id_token_expires_at, None);
        // 長效憑證必須留著，否則使用者得重新匯入。
        assert_eq!(saved.client_token, "CT-OLD");
    }

    #[tokio::test]
    async fn invalidate_forces_a_fresh_refresh() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 2).await;

        let manager = TokenManager::new(seeded_store(), reqwest::Client::new(), server.uri());
        manager.ensure_token().await.unwrap();
        manager.ensure_token().await.unwrap(); // 用快取，不打網路

        manager.invalidate().await;
        manager.ensure_token().await.unwrap(); // 快取沒了，必須重抓
    }

    #[tokio::test]
    async fn invalidated_manager_with_cleared_store_needs_login() {
        let server = MockServer::start().await;
        mount_token(&server, token_response("CT-NEW"), 1).await;

        let store = seeded_store();
        let manager = TokenManager::new(store.clone(), reqwest::Client::new(), server.uri());
        manager.ensure_token().await.unwrap();

        // 這就是「解除連結」做的事：清 store + 丟快取。
        store.clear().unwrap();
        manager.invalidate().await;

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::NeedsLogin)
        ));
    }
}
