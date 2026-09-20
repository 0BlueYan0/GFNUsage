pub mod api;
pub mod auth;
pub mod commands;
pub mod error;
pub mod quota;
pub mod tray;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::auth::refresh::{TokenManager, STARFLEET_BASE};
use crate::auth::store::{KeyringStore, TokenStore};
use crate::quota::QuotaSnapshot;

/// 全應用程式共用的狀態。憑證本身不在這裡 —— 那只存在 keychain 與
/// `TokenManager` 的記憶體快取中。
pub struct AppState {
    pub tokens: Arc<TokenManager>,
    pub store: Arc<dyn TokenStore>,
    pub http: reqwest::Client,
    /// 這兩個用 std 的 Mutex 是刻意的：鎖絕不跨越 await 持有。
    /// 若之後需要在持鎖期間 await，要改成 tokio::sync::Mutex。
    pub snapshot: Mutex<Option<QuotaSnapshot>>,
    pub last_error: Mutex<Option<String>>,

    /// 面板因失去焦點而自動收起的時間點。
    ///
    /// 點系統匣圖示會讓面板先失去焦點，於是「失焦就收起」會和「點擊就開啟」
    /// 打架 —— 面板開著時點圖示，會先收起再立刻重開，等於關不掉。
    /// 記下收起的時間，讓緊接著的那次點擊知道自己是關閉動作而不是開啟動作。
    pub last_auto_hide: Mutex<Option<Instant>>,
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
            last_auto_hide: Mutex::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
