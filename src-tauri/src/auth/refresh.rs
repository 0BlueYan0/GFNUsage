use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::auth::jwt;
use crate::auth::session::ImportedSession;
use crate::auth::store::TokenStore;
use crate::error::GfnError;

pub const STARFLEET_CLIENT_ID: &str = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
pub const STARFLEET_BASE: &str = "https://login.nvidia.com";
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:client_token";

/// id_token 剩餘效期低於此值時才重新刷新。
const REFRESH_MARGIN_MINUTES: i64 = 5;

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

/// 取得 id_token 的唯一入口。
///
/// 內部的 mutex 保證整個程序同時只有一次刷新在進行，因此定時輪詢、開啟面板、
/// 以及 401 後的重試三者同時發生時，也不會各自呼叫一次 `/token`。
/// 這很重要：每次 `/token` 回應都會作廢舊的 client_token，並行刷新會互相踩掉。
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
            if cached.expires_at - Utc::now() > Duration::minutes(REFRESH_MARGIN_MINUTES) {
                return Ok(cached.id_token.clone());
            }
        }

        let credentials = self.store.load()?.ok_or(GfnError::NeedsLogin)?;
        let fresh = self.refresh(&credentials).await?;
        *guard = Some(fresh.clone());
        Ok(fresh.id_token)
    }

    /// 丟棄快取的 token，強制下次 `ensure_token` 重新刷新。
    pub async fn invalidate(&self) {
        *self.cached.lock().await = None;
    }

    async fn refresh(&self, credentials: &ImportedSession) -> Result<CachedToken, GfnError> {
        let response = self
            .http
            .post(format!("{}/token", self.auth_base))
            .form(&[
                ("grant_type", GRANT_TYPE),
                ("client_token", credentials.client_token.as_str()),
                ("client_id", STARFLEET_CLIENT_ID),
                ("sub", credentials.sub.as_str()),
            ])
            .send()
            .await
            .map_err(|e| GfnError::Network(e.to_string()))?;

        let status = response.status();
        if status.is_client_error() {
            // 憑證已失效，重試只會反覆失敗，而且每次嘗試都可能再燒掉一顆。
            return Err(GfnError::NeedsLogin);
        }
        if !status.is_success() {
            return Err(GfnError::Network(format!("/token 回應 {status}")));
        }

        let body: TokenResponse = response
            .json()
            .await
            .map_err(|e| GfnError::UnexpectedResponse(e.to_string()))?;

        // 順序至關重要：新的 client_token 必須先寫進 keychain。
        // 伺服器在回應的當下就已作廢舊的那顆；若先使用 id_token 再存檔而中途失敗，
        // keychain 裡會留著一顆已死的憑證，使用者將被鎖在外面直到手動重新登入。
        self.store.save(&ImportedSession {
            client_token: body.client_token,
            sub: credentials.sub.clone(),
        })?;

        let expires_at = jwt::expiry(&body.id_token)
            .ok_or_else(|| GfnError::UnexpectedResponse("id_token 沒有 exp claim".into()))?;

        Ok(CachedToken {
            id_token: body.id_token,
            expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::auth::store::MemoryStore;

    /// exp 設在西元 2286 年，測試期間永遠有效。
    const FAR_FUTURE_JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjk5OTk5OTk5OTl9.sig";

    fn seeded_store() -> Arc<MemoryStore> {
        let store = Arc::new(MemoryStore::new());
        store
            .save(&ImportedSession {
                client_token: "CT-OLD".into(),
                sub: "SUB456".into(),
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
    async fn persists_rotated_client_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token_response("CT-NEW"))
            .mount(&server)
            .await;

        let store = seeded_store();
        let manager = TokenManager::new(store.clone(), reqwest::Client::new(), server.uri());
        manager.ensure_token().await.unwrap();

        assert_eq!(store.load().unwrap().unwrap().client_token, "CT-NEW");
    }

    #[tokio::test]
    async fn concurrent_callers_refresh_only_once() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token_response("CT-NEW"))
            .expect(1) // 關鍵斷言：並行呼叫只能打一次 /token
            .mount(&server)
            .await;

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
        // MockServer 於 drop 時驗證 expect(1)
    }

    #[tokio::test]
    async fn reuses_cached_token_without_hitting_network() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token_response("CT-NEW"))
            .expect(1)
            .mount(&server)
            .await;

        let manager = TokenManager::new(seeded_store(), reqwest::Client::new(), server.uri());
        manager.ensure_token().await.unwrap();
        manager.ensure_token().await.unwrap();
        manager.ensure_token().await.unwrap();
    }

    #[tokio::test]
    async fn four_xx_means_needs_login_and_leaves_store_untouched() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_request",
                "error_description": "ClientId does not match",
            })))
            .expect(1) // 不得重試
            .mount(&server)
            .await;

        let store = seeded_store();
        let manager = TokenManager::new(store.clone(), reqwest::Client::new(), server.uri());

        assert!(matches!(
            manager.ensure_token().await,
            Err(GfnError::NeedsLogin)
        ));
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
    async fn invalidate_forces_a_fresh_refresh() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token_response("CT-NEW"))
            .expect(2) // 一次是首抓，一次是 invalidate 之後
            .mount(&server)
            .await;

        let manager = TokenManager::new(seeded_store(), reqwest::Client::new(), server.uri());
        manager.ensure_token().await.unwrap();
        manager.ensure_token().await.unwrap(); // 用快取，不打網路

        manager.invalidate().await;
        manager.ensure_token().await.unwrap(); // 快取沒了，必須重抓
    }

    #[tokio::test]
    async fn invalidated_manager_with_cleared_store_needs_login() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token_response("CT-NEW"))
            .mount(&server)
            .await;

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
