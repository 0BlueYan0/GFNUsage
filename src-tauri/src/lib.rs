pub mod api;
pub mod auth;
pub mod commands;
pub mod error;
pub mod quota;
pub mod tray;

use std::sync::{Arc, Mutex};

use crate::auth::refresh::{TokenManager, STARFLEET_BASE};
use crate::auth::store::{KeyringStore, TokenStore};
use crate::quota::QuotaSnapshot;

/// 全應用程式共用的狀態。憑證本身不在這裡 —— 那只存在 keychain 與
/// `TokenManager` 的記憶體快取中。
pub struct AppState {
    pub tokens: Arc<TokenManager>,
    pub store: Arc<dyn TokenStore>,
    pub http: reqwest::Client,
    pub snapshot: Mutex<Option<QuotaSnapshot>>,
    pub last_error: Mutex<Option<String>>,
}

impl AppState {
    pub fn new() -> Self {
        let store: Arc<dyn TokenStore> = Arc::new(KeyringStore);
        let http = reqwest::Client::builder()
            .user_agent("GFNUsage/0.1")
            .build()
            .expect("HTTP 用戶端應可建立");
        let tokens = Arc::new(TokenManager::new(
            store.clone(),
            http.clone(),
            STARFLEET_BASE.to_string(),
        ));

        Self {
            tokens,
            store,
            http,
            snapshot: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
