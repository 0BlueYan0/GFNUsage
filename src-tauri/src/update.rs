//! 查有沒有新版本，以及使用者按下去之後的安裝。
//!
//! 這條路只打 GitHub 的 `latest.json`，不碰 NVIDIA，不進「同時有效 token
//! 數量」那個上限的帳。查不到就等下一圈，不重試（spec §9）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::AppState;

/// 一次請求最多等這麼久。
///
/// 外掛預設是沒有上限（`timeout: None`）。使用者機器上的系統 proxy 可能只是
/// 「有時候連得上」—— 實測一台裝了本機 proxy 的機器，同一個網址連跑三次有
/// 兩次連不完成，一次檢查卡了 1 分 52 秒才回來，而那段時間關於頁停在
/// 「檢查中…」。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

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

/// 一次檢查的結果。
///
/// 三種都要傳得回前端。只記日誌的話「已是最新」和「端點壞掉」在畫面上
/// 長得一模一樣 —— 而這整套機制（latest.json 的網址、公鑰與私鑰配不配得
/// 起來、草稿發佈後資產能不能公開下載）沒有人跑過，它失敗時得看得出來。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum CheckResult {
    /// 查到新版本。
    Found(String),
    /// 已經是最新的。
    UpToDate,
    /// 沒查成：網路不通、端點壞掉、簽章對不上。字串直接給使用者看。
    Failed(String),
}

/// 查一次，`direct` 為真時繞過系統 proxy。
///
/// reqwest 預設會去讀 Windows 的 `Internet Settings`（`ProxyEnable`／
/// `ProxyServer`），所以這支程式的每一個 HTTP 請求都跟著系統 proxy 走。
/// 那顆 proxy 是使用者裝的，好不好用不歸這裡管。
async fn check_with<R: Runtime>(
    app: &AppHandle<R>,
    direct: bool,
) -> Result<Option<Update>, String> {
    let mut builder = app.updater_builder().timeout(REQUEST_TIMEOUT);
    if direct {
        builder = builder.no_proxy();
    }
    let updater = builder
        .build()
        .map_err(|e| format!("更新檢查建不起來：{e}"))?;
    updater
        .check()
        .await
        .map_err(|e| format!("更新檢查失敗：{e}"))
}

/// 查一次。
///
/// 背景那條迴圈把回傳值丟掉，關於頁的「檢查更新」則把它畫出來。
///
/// 先照系統設定走，不通才直連再試一次。兩條都要試：有人是非得過 proxy 才
/// 出得去，也有人的 proxy 只是偶爾通，而 GitHub 直連是好的。
///
/// 這不是 spec §9 決定不做的那種退避重試 —— 沒有等待、沒有第二次同樣的請求，
/// 是換一條路徑，而且這條路只打 GitHub，不碰 NVIDIA，不進 token 上限的帳。
/// 成功的那一邊建出來的 `Update` 帶著自己的 proxy 設定，待會下載會走同一條。
pub async fn check_once<R: Runtime>(app: &AppHandle<R>) -> CheckResult {
    let found = match check_with(app, false).await {
        Ok(found) => found,
        Err(proxied) => {
            log::warn!("{proxied}（照系統 proxy 設定）。改直連再試一次");
            match check_with(app, true).await {
                Ok(found) => found,
                Err(e) => {
                    log::warn!("{e}（直連）");
                    return CheckResult::Failed(format!("{e}（系統 proxy 與直連都不通）"));
                }
            }
        }
    };
    let Some(found) = found else {
        return CheckResult::UpToDate;
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
    CheckResult::Found(version)
}

/// 下載並安裝。
///
/// Windows 走不回來：NSIS 那條的結尾是 `std::process::exit(0)`。macOS 換完
/// bundle 就回傳，要自己重新啟動才會跑到新的程式碼。
pub async fn install<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    // 安裝結尾會結束程序。在 `tauri dev` 底下那是殺掉開發中的程式去裝正式版
    // —— 而開發樹的版本號比最新的 release 舊時（舊 checkout）真的查得到更新。
    // 背景檢查在 debug 本來就不排（`main.rs`），擋不到的是關於頁與系統匣
    // 那兩顆按鈕，所以閘門放在這裡。
    if cfg!(debug_assertions) {
        return Err("開發建構不安裝更新".into());
    }

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
        Ok(()) => {
            // Windows 到不了這裡。macOS 會：換好的是磁碟上的 bundle，記憶體裡
            // 跑的還是舊的程式碼。不重設旗標的話關於頁永遠停在「更新安裝中」，
            // 而 `pending` 已經被 take 走，系統匣那行再按一次會在上面的 swap
            // 直接 return，按下去沒反應。
            state.installing.store(false, Ordering::SeqCst);
            log::info!("{version} 安裝完成，重新啟動");
            app.restart();
        }
        Err(e) => {
            // `download_and_install` 收 `&self`，所以 `pending` 還在手上。
            // 放回去，讓使用者能再按一次，不必等下一個檢查週期。
            *state.pending.lock().unwrap() = Some(pending);
            state.installing.store(false, Ordering::SeqCst);
            log::warn!("{version} 安裝失敗：{e}");

            // 寫進面板的錯誤格，不是只回傳。系統匣那條路是
            // `let _ = install(...)`，面板那條走 `runQuietly` 也會吞掉回傳值
            // —— 兩邊都靠這一行，不寫的話使用者按下去只看到按鈕彈回來。
            //
            // await 已經結束，這裡拿鎖不跨 await。
            let message = format!("更新安裝失敗：{e}");
            let app_state = app.state::<Arc<AppState>>().inner().clone();
            *app_state.last_error.lock().unwrap() = Some(message.clone());
            crate::tray::sync(&app, &app_state);
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl UpdateState {
        /// 測試用：直接擺一個查到的版本進去。
        ///
        /// `pending` 留空 —— 那要一個真的 `Update`，而它只能從
        /// `updater.check()` 拿。這裡測的是「要不要講」，不是「怎麼裝」。
        pub fn found(version: &str) -> Self {
            let state = Self::default();
            *state.version.lock().unwrap() = Some(version.into());
            state
        }
    }

    #[test]
    fn a_fresh_state_has_nothing_to_announce() {
        let state = UpdateState::default();

        assert_eq!(state.available(), None);
        assert!(!state.installing());
    }

    #[test]
    fn a_found_version_is_available() {
        let state = UpdateState::found("0.1.1");

        assert_eq!(state.available().as_deref(), Some("0.1.1"));
    }

    /// 前端靠 `kind` 分三種結果。改了這個形狀，關於頁就分不出
    /// 「已是最新」和「端點壞掉」，而那正是這個 enum 存在的理由。
    #[test]
    fn the_three_results_reach_the_panel_apart() {
        let json = |r: &CheckResult| serde_json::to_string(r).unwrap();

        assert_eq!(
            json(&CheckResult::Found("0.1.1".into())),
            r#"{"kind":"found","value":"0.1.1"}"#
        );
        assert_eq!(json(&CheckResult::UpToDate), r#"{"kind":"upToDate"}"#);
        assert_eq!(
            json(&CheckResult::Failed("端點壞了".into())),
            r#"{"kind":"failed","value":"端點壞了"}"#
        );
    }
}
