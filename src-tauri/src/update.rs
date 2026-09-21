//! 查有沒有新版本，以及使用者按下去之後的安裝。
//!
//! 這條路只打 GitHub 的 `latest.json`，不碰 NVIDIA，不進「同時有效 token
//! 數量」那個上限的帳。查不到就等下一圈，不重試（spec §9）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

/// 啟動後隔這麼久才做第一次檢查。
///
/// 剛開機網路多半還沒好，而且第一輪額度輪詢才是使用者盯著的東西，
/// 不跟它搶。
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);

/// 之後每隔這麼久查一次。常駐程式可能連開好幾週，只在啟動時查的話那些人
/// 永遠收不到。
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
pub struct UpdateState {
    /// 查到、還沒安裝的那一版。
    ///
    /// 用 std 的 `Mutex`，而且鎖只包住 `take()` 與寫回：
    /// `download_and_install` 是 async，鎖絕不能跨 await 持有。
    pending: Mutex<Option<Update>>,

    /// 給畫面看的版本號。和 `pending` 分開，因為 `take()` 之後它還要留著
    /// —— 安裝中的畫面仍然要講得出是哪一版。
    version: Mutex<Option<String>>,

    /// 已經在裝了。使用者可能在系統匣按一次、關於頁又按一次。
    installing: AtomicBool,
}

impl UpdateState {
    /// 查到的新版本號。沒有就是已經最新。
    pub fn available(&self) -> Option<String> {
        self.version.lock().unwrap().clone()
    }

    pub fn installing(&self) -> bool {
        self.installing.load(Ordering::SeqCst)
    }
}

/// 查一次。查不到、網路不通、端點壞掉，一律安靜結束。
pub async fn check_once<R: Runtime>(app: &AppHandle<R>) {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            log::warn!("更新檢查建不起來：{e}");
            return;
        }
    };
    let found = match updater.check().await {
        Ok(Some(found)) => found,
        Ok(None) => return,
        Err(e) => {
            log::warn!("更新檢查失敗：{e}");
            return;
        }
    };

    // await 結束之後才碰鎖。
    let version = found.version.clone();
    {
        let state = app.state::<UpdateState>();
        *state.pending.lock().unwrap() = Some(found);
        *state.version.lock().unwrap() = Some(version.clone());
    }
    log::info!("有新版本可用：{version}");
    if let Err(e) = crate::tray::menu::rebuild(app, Some(&version)) {
        log::warn!("系統匣選單更新不了：{e}");
    }
}

/// 下載並安裝。成功的話這個函式不會回來 —— updater 裝完會結束程序。
pub async fn install<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let state = app.state::<UpdateState>();
    if state.installing.swap(true, Ordering::SeqCst) {
        // 已經在裝了。第二次按當成沒按，不是錯誤。
        return Ok(());
    }

    // 鎖在這一行就放掉，底下才能 await。
    let pending = { state.pending.lock().unwrap().take() };
    let Some(pending) = pending else {
        state.installing.store(false, Ordering::SeqCst);
        return Err("沒有可安裝的更新".into());
    };

    let version = pending.version.clone();
    log::info!("開始安裝 {version}");
    match pending.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => Ok(()),
        Err(e) => {
            // `download_and_install` 收 `&self`，所以 `pending` 還在手上。
            // 放回去，讓使用者能再按一次，不必等下一個檢查週期。
            *state.pending.lock().unwrap() = Some(pending);
            state.installing.store(false, Ordering::SeqCst);
            log::warn!("{version} 安裝失敗：{e}");
            Err(format!("更新安裝失敗：{e}"))
        }
    }
}
