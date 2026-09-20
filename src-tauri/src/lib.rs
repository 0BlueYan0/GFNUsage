pub mod api;
pub mod auth;
pub mod commands;
pub mod error;
pub mod pace;
pub mod quota;
pub mod store;
pub mod tray;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::subscriptions::MES_BASE;
use crate::auth::refresh::{TokenManager, STARFLEET_BASE};
use crate::auth::store::{KeyringStore, TokenStore};
use crate::pace::schedule::Schedule;
use crate::pace::PaceReport;
use crate::quota::QuotaSnapshot;

/// 單次 HTTP 請求的上限。刷新在 mutex 內進行，沒有上限的話一次卡住的連線
/// 會讓整個程式永遠不再更新，直到重啟。
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

pub fn http_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("GFNUsage/0.1")
        .timeout(timeout)
        .build()
        .expect("HTTP 用戶端應可建立")
}

/// 全應用程式共用的狀態。憑證本身不在這裡 —— 那只存在 keychain 與
/// `TokenManager` 的記憶體快取中。
pub struct AppState {
    pub tokens: Arc<TokenManager>,
    pub store: Arc<dyn TokenStore>,
    pub http: reqwest::Client,
    pub mes_base: String,
    /// 這兩個用 std 的 Mutex 是刻意的：鎖絕不跨越 await 持有。
    /// 若之後需要在持鎖期間 await，要改成 tokio::sync::Mutex。
    pub snapshot: Mutex<Option<QuotaSnapshot>>,
    pub last_error: Mutex<Option<String>>,

    /// 設定檔所在目錄。由 `AppHandle` 在啟動時解析，測試注入暫存目錄。
    pub settings_dir: PathBuf,

    /// 最近一次算出的配速。跟著快照一起被系統匣與面板讀取。
    pub pace: Mutex<Option<PaceReport>>,

    /// 設定的記憶體快取。每個輪詢週期會從檔案重讀，手動改檔案不必重開程式。
    pub schedule: Mutex<Schedule>,

    /// 憑證被拒絕（401 重試後仍失敗）。這個旗標會黏住：輪詢暫停，直到
    /// 使用者重新匯入或手動更新成功。否則每個週期都會為了 401 重試再鑄一顆
    /// token，一小時內就把「同時有效 token 上限」撞滿。
    pub needs_login: AtomicBool,

    /// 面板因失去焦點而自動收起的時間點。
    ///
    /// 點系統匣圖示會讓面板先失去焦點，於是「失焦就收起」會和「點擊就開啟」
    /// 打架 —— 面板開著時點圖示，會先收起再立刻重開，等於關不掉。
    /// 記下收起的時間，讓緊接著的那次點擊知道自己是關閉動作而不是開啟動作。
    pub last_auto_hide: Mutex<Option<Instant>>,
}

impl AppState {
    pub fn new(settings_dir: PathBuf) -> Self {
        Self::with(
            Arc::new(KeyringStore),
            http_client(HTTP_TIMEOUT),
            STARFLEET_BASE,
            MES_BASE,
            settings_dir,
        )
    }

    /// 可注入儲存區與端點的建構式，測試用。
    pub fn with(
        store: Arc<dyn TokenStore>,
        http: reqwest::Client,
        auth_base: &str,
        mes_base: &str,
        settings_dir: PathBuf,
    ) -> Self {
        let tokens = Arc::new(TokenManager::new(
            store.clone(),
            http.clone(),
            auth_base.to_string(),
        ));
        // 啟動時讀不到或讀壞了就先用空設定；第一個輪詢週期會再讀一次，
        // 並把錯誤寫進 `last_error` 讓面板看得到。
        let schedule =
            crate::store::load(&crate::store::schedule_path(&settings_dir)).unwrap_or_default();

        Self {
            tokens,
            store,
            http,
            mes_base: mes_base.to_string(),
            settings_dir,
            snapshot: Mutex::new(None),
            pace: Mutex::new(None),
            schedule: Mutex::new(schedule),
            last_error: Mutex::new(None),
            needs_login: AtomicBool::new(false),
            last_auto_hide: Mutex::new(None),
        }
    }

    pub fn schedule_path(&self) -> PathBuf {
        crate::store::schedule_path(&self.settings_dir)
    }
}
