pub mod api;
pub mod auth;
pub mod commands;
pub mod error;
pub mod logging;
pub mod pace;
pub mod panel;
pub mod quota;
pub mod store;
pub mod tray;
pub mod trend;
pub mod update;
pub mod watcher;

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
        // 版本只寫在 Cargo.toml 一個地方。寫死字串的話發佈幾次之後
        // NVIDIA 那邊看到的永遠是 0.1，追問題時對不上是哪一版。
        .user_agent(concat!("GFNUsage/", env!("CARGO_PKG_VERSION")))
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
    /// 逐場遊玩紀錄的主機。和 `mes_base` 分開，是因為它們是不同主機，
    /// 而且只有這一個吃帳號頁那顆 client_id 簽的 token。
    pub paywall_base: String,
    /// NVIDIA 登入端點。`TokenManager` 內部也有一份，但 OAuth 登入流程
    /// 在指令端組授權網址，從這裡拿。測試注入 mock server 的位址。
    pub auth_base: String,
    /// 這兩個用 std 的 Mutex 是刻意的：鎖絕不跨越 await 持有。
    /// 若之後需要在持鎖期間 await，要改成 tokio::sync::Mutex。
    pub snapshot: Mutex<Option<QuotaSnapshot>>,

    /// 本期的逐場遊玩紀錄。
    ///
    /// `None` 與空 `Vec` 是兩件事：`None` 是這次沒抓到（拿它算今天用了多少
    /// 會得到零，等於把玩過的時間當成沒玩），空 `Vec` 是本期真的沒玩過。
    pub sessions: Mutex<Option<Vec<crate::api::playtime::PlaySession>>>,

    /// 上次成功抓到的逐場紀錄，只給走勢圖用。
    ///
    /// 和 `sessions` 分開存，是因為那個的 `None` 有語意（這次沒抓到），
    /// `used_today` 與 `recent_play` 靠它分辨「不知道」與「沒玩」。走勢圖
    /// 不需要這個分辨：抓失敗那一輪，快照已經換新，圖要用新的 U 當錨點
    /// 重畫，拿上一份成功的場次回推過去的日子，比留著整張舊圖準。留舊圖
    /// 的話，換期那一輪 playtime 沒回，圖上是上個月的日期，主要數字卻是
    /// 這個月的。
    pub last_good_sessions: Mutex<Option<Vec<crate::api::playtime::PlaySession>>>,
    pub last_error: Mutex<Option<String>>,

    /// 設定檔讀不動時的訊息。與 `last_error` 分開存，檔案修好就能單獨清掉
    /// —— 混在同一格裡分不出該清哪一個，修好了訊息還會繼續掛著。
    pub settings_error: Mutex<Option<String>>,

    /// 設定檔所在目錄。由 `AppHandle` 在啟動時解析，測試注入暫存目錄。
    pub settings_dir: PathBuf,

    /// 最近一次算出的配速。跟著快照一起被系統匣與面板讀取。
    pub pace: Mutex<Option<PaceReport>>,

    /// 最近一次算出的逐日累計，給面板那張折線圖。
    ///
    /// 每次 `recompute_pace` 都從 `last_good_sessions` 重建，錨點因此永遠是
    /// 目前快照的 U。存下來只是讓 `panel_data` 不必每次開面板重算。
    pub trend: Mutex<Vec<crate::trend::DailyPoint>>,

    /// 設定的記憶體快取。每個輪詢週期會從檔案重讀，手動改檔案不必重開程式。
    pub schedule: Mutex<Schedule>,

    /// 憑證被拒絕（401 重試後仍失敗）。這個旗標會黏住：輪詢暫停，直到
    /// 使用者重新匯入或手動更新成功。否則每個週期都會為了 401 重試再鑄一顆
    /// token，一小時內就把「同時有效 token 上限」撞滿。
    pub needs_login: AtomicBool,

    /// 有一次登入正在進行。輪詢也看它（`poll_due`）。
    ///
    /// 事實放在後端而不是前端：登入視窗一定會讓這個 flyout 失焦收起來，
    /// 面板回來時（或任何一次重新載入、開發時的 HMR）本地旗標就沒了，
    /// 畫面會變回「可以按登入」，於是使用者又開一次。
    ///
    /// 用 `Arc` 是因為 `LoginGuard` 要拿一份：旗標跟著它的生命週期走，
    /// 中途放棄（視窗開不起來就直接 return）也不會卡在 true。
    pub login_pending: Arc<AtomicBool>,

    /// 取消進行中的登入。按一次取消就送出一個新的世代號，登入流程持一個
    /// receiver，看到值變了就中止。
    ///
    /// 用 `watch` 而不是 `Notify`：`Notify` 只喚醒「當下已經在等」的人，
    /// 取消訊號若比「開始等」早一步抵達就整個掉了，使用者會覺得按了沒反應。
    /// `watch` 的 receiver 比對的是版本號，早到的一樣收得到。
    ///
    /// receiver 在 `start_login` 開視窗之前就訂閱好，所以上一次登入留下的
    /// 取消訊號不會誤殺下一次登入。背景的靜默續期也各訂一份，登出才收得掉。
    pub login_cancel: tokio::sync::watch::Sender<u64>,

    /// 面板因失去焦點而自動收起的時間點。
    ///
    /// 點系統匣圖示會讓面板先失去焦點，於是「失焦就收起」會和「點擊就開啟」
    /// 打架 —— 面板開著時點圖示，會先收起再立刻重開，等於關不掉。
    /// 記下收起的時間，讓緊接著的那次點擊知道自己是關閉動作而不是開啟動作。
    pub last_auto_hide: Mutex<Option<Instant>>,

    /// 上次開始抓取的時間點。定時器從它算間隔。
    ///
    /// 放共用狀態而不是輪詢迴圈的區域變數：系統匣「立即更新」、開面板、
    /// GFN 視窗轉換都會抓，抓完定時器要跟著往後推。不然剛抓過幾秒之後
    /// 定時器又抓一次，而間隔拉長之後這種重複更明顯。
    ///
    /// `None` 代表這個行程還沒抓過，所以啟動時第一圈就會抓，間隔設成
    /// 「關閉」也一樣。
    pub last_poll: Mutex<Option<Instant>>,
}

impl AppState {
    pub fn new(settings_dir: PathBuf) -> Self {
        Self::with(
            Arc::new(KeyringStore),
            http_client(HTTP_TIMEOUT),
            STARFLEET_BASE,
            MES_BASE,
            crate::api::playtime::PAYWALL_BASE,
            settings_dir,
        )
    }

    /// 可注入儲存區與端點的建構式，測試用。
    pub fn with(
        store: Arc<dyn TokenStore>,
        http: reqwest::Client,
        auth_base: &str,
        mes_base: &str,
        paywall_base: &str,
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
            paywall_base: paywall_base.to_string(),
            auth_base: auth_base.to_string(),
            settings_dir,
            snapshot: Mutex::new(None),
            sessions: Mutex::new(None),
            last_good_sessions: Mutex::new(None),
            pace: Mutex::new(None),
            trend: Mutex::new(Vec::new()),
            schedule: Mutex::new(schedule),
            last_error: Mutex::new(None),
            settings_error: Mutex::new(None),
            needs_login: AtomicBool::new(false),
            login_pending: Arc::new(AtomicBool::new(false)),
            login_cancel: tokio::sync::watch::Sender::new(0),
            last_auto_hide: Mutex::new(None),
            last_poll: Mutex::new(None),
        }
    }

    pub fn last_poll(&self) -> Option<Instant> {
        *self.last_poll.lock().unwrap()
    }

    pub fn schedule_path(&self) -> PathBuf {
        crate::store::schedule_path(&self.settings_dir)
    }

    pub fn history_path(&self) -> PathBuf {
        crate::store::history_path(&self.settings_dir)
    }

    pub fn ui_state_path(&self) -> PathBuf {
        crate::store::ui_state_path(&self.settings_dir)
    }

    /// 面板與系統匣要顯示的錯誤。
    ///
    /// 憑證失效與抓取失敗都比設定檔急：蓋掉它們會讓 tooltip 與登入畫面變成
    /// 「設定檔格式錯誤」，使用者就不知道該去重新匯入憑證了。
    pub fn display_error(&self) -> Option<String> {
        let last = self.last_error.lock().unwrap().clone();
        last.or_else(|| self.settings_error.lock().unwrap().clone())
    }
}
