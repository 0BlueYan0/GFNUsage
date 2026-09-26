//! 在 webview 裡跑完一次授權，把 `code` 接下來。
//!
//! `webview.rs` 是這條路上所有能純函式化的部分（組網址、判斷導向），
//! 這裡只剩非得有視窗才能做的事。分開放是為了讓前者測得到 —— 導向判斷錯了
//! 的症狀是登入永遠走不完，那不該靠手動點擊才發現。
//!
//! cookie 由 WebView2／WKWebView 自己存在 app 的資料夾裡，程序重開還在。
//! 靜默續期能不能成功就看它 —— 這是換掉迴圈埠登入之後，唯一需要實際跑幾天
//! 才知道的事（見模組 `webview` 的說明）。

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use super::webview::{authorize_url, new_device_id, parse_redirect, Redirect, Token};
use crate::auth::oauth::{challenge, verifier, LOGIN_TIMEOUT};
use crate::error::GfnError;

/// 登入視窗的 label。互動與靜默各一個。
///
/// 開新視窗之前要把同名的舊視窗收掉 —— 連按兩次登入會得到兩個視窗，而
/// 第二個的 `state` 和第一個不同，先回來的那個會被另一個的判斷式當成
/// 別人的回呼丟掉。
///
/// 分成兩個 label，是因為背景的靜默續期也走這個函式：共用一個的話，
/// 輪詢到期時它會把使用者正在打帳密的那個視窗關掉。
const WINDOW_LABEL: &str = "nvidia-login";
const SILENT_WINDOW_LABEL: &str = "nvidia-login-silent";

/// 靜默續期的等待上限。
///
/// 比互動登入的 5 分鐘短得多：`prompt=none` 不需要使用者做任何事，
/// 逾時就是換不到。拖著不放的話，輪詢會跟著卡住。
const SILENT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 跑一次授權，回傳 id_token。
///
/// `silent` 為真時視窗不顯示，走 `prompt=none`。換不到會得到
/// `GfnError::NeedsLogin` —— 那是要使用者動手的訊號，呼叫端該把它轉成
/// 「需要重新登入」，不是當成網路錯誤重試。
///
/// `cancel` 由呼叫端在進來之前就訂閱好，理由見 `webview::await_outcome`。
pub async fn obtain_token<R: Runtime>(
    app: &AppHandle<R>,
    http: &reqwest::Client,
    auth_base: &str,
    device_id: &str,
    silent: bool,
    mut cancel: tokio::sync::watch::Receiver<u64>,
) -> Result<Token, GfnError> {
    let label = if silent {
        SILENT_WINDOW_LABEL
    } else {
        WINDOW_LABEL
    };
    // 三個都是一次性的隨機字串，同一個產生器就夠了。
    let (verifier, nonce, state) = (verifier()?, verifier()?, verifier()?);
    let url = authorize_url(
        auth_base,
        device_id,
        &challenge(&verifier),
        &nonce,
        &state,
        silent,
    );

    // 上一次的視窗還開著就先收掉，理由見 WINDOW_LABEL。
    if let Some(stale) = app.get_webview_window(label) {
        let _ = stale.close();
    }

    let parsed = url
        .parse()
        .map_err(|e| GfnError::LoginFailed(format!("授權網址組不起來：{e}")))?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Redirect>();
    // `on_navigation` 是同步 closure，而且會被呼叫很多次。用 Option 包住
    // sender，只送第一個有結論的導向 —— 回呼頁被導向之後瀏覽器還會繼續
    // 跑它自己的重導，那些不該再送進來。
    let sender = Arc::new(Mutex::new(Some(tx)));
    let expected = state.clone();

    if !silent {
        show_in_dock(app, true);
    }
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed))
        .title("登入 NVIDIA 帳號")
        .inner_size(480.0, 720.0)
        .visible(!silent)
        .focused(!silent)
        .on_navigation(move |url| {
            let outcome = parse_redirect(url.as_str(), &expected);
            if matches!(outcome, Redirect::Elsewhere) {
                return true;
            }
            if let Some(sender) = sender.lock().unwrap().take() {
                let _ = sender.send(outcome);
            }
            // 擋下這一步導向。`https://www.nvidia.com/auth/` 是 NVIDIA 自己的
            // 回呼頁，讓它載入只會把 code 再用掉一次，而我們已經拿到了。
            false
        })
        .build()
        .map_err(|e| {
            if !silent {
                show_in_dock(app, false);
            }
            GfnError::LoginFailed(format!("開不了登入視窗：{e}"))
        })?;

    // 使用者自己把視窗關掉也要收場，否則就卡滿 5 分鐘。
    //
    // `notify_one` 而不是 `notify_waiters`：關掉的事件可能比下面那個
    // select 早一步，那時還沒有人在等，`notify_waiters` 會把訊號丟掉。
    let closed = Arc::new(tokio::sync::Notify::new());
    {
        let closed = Arc::clone(&closed);
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                closed.notify_one();
            }
        });
    }

    let timeout = if silent {
        SILENT_TIMEOUT
    } else {
        LOGIN_TIMEOUT
    };
    let outcome =
        super::webview::await_outcome(&mut rx, &closed, &mut cancel, timeout, silent).await;

    let _ = window.close();
    if !silent {
        show_in_dock(app, false);
    }

    let code = match outcome? {
        Redirect::Code(code) => code,
        // 靜默換不到就是要使用者動手，這是 `prompt=none` 的正常結果。
        Redirect::LoginRequired => return Err(GfnError::NeedsLogin),
        Redirect::Denied(message) => return Err(GfnError::LoginFailed(message)),
        Redirect::Elsewhere => return Err(GfnError::LoginFailed("登入沒有走完".into())),
    };

    super::webview::exchange_code(http, auth_base, &code, &verifier, &nonce).await
}

/// 登入視窗開著的時候在 Dock 放一個圖示。
///
/// 平時是 Accessory（見 `main` 的 `setup`），Dock 與 Cmd+Tab 都沒有它。登入要
/// 切到別的程式查密碼或收驗證碼，切回來時登入視窗在別的視窗後面，沒有 Dock
/// 圖示就點不回來。靜默續期的視窗不顯示，不切。
fn show_in_dock<R: Runtime>(app: &AppHandle<R>, show: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        if let Err(e) = app.set_activation_policy(policy) {
            log::warn!("切換 Dock 圖示失敗：{e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}

/// 這台機器的 device_id，沒有就生一顆。
///
/// 放在 `ui-state.json`：它是這台機器的身分，跟著作息設定匯出到別台沒有意義。
pub fn device_id(path: &std::path::Path) -> String {
    let mut ui = crate::store::load_ui_state(path);
    if ui.device_id.is_empty() {
        ui.device_id = new_device_id().unwrap_or_else(|_| "gfnusage".into());
        // 存不起來就這次先用著。下次會再生一顆，代價是 NVIDIA 那邊多一個
        // 裝置紀錄，不影響登入。
        let _ = crate::store::save_ui_state(path, &ui);
    }
    ui.device_id
}
