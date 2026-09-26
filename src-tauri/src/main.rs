#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gfnusage_lib::commands::{self, poll_due, refresh_into_state};
use gfnusage_lib::panel::{self, PANEL_LABEL};
use gfnusage_lib::tray;
use gfnusage_lib::AppState;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

/// 迴圈的心跳。比任何一個抓取間隔都短得多，好讓睡眠喚醒在半分鐘內就被發現。
const TICK: Duration = Duration::from_secs(30);

/// 配速重算的間隔。
///
/// 和抓取間隔無關，所以是寫死的：沒抓資料 `A_past` 照樣變大，結論會自己
/// 翻轉（spec §6.5）。綁在一起的話，抓取間隔設成「關閉」就再也不重算，
/// 配速與預測會停在啟動時的數字。
const PACE_INTERVAL: Duration = Duration::from_secs(300);

fn main() {
    gfnusage_lib::logging::log_panics();

    tauri::Builder::default()
        // 必須排在所有外掛的最前面（官方文件明講）。第二份程序唯一的職責是把
        // 既有的面板叫出來 —— 使用者會再點一次圖示，多半是因為 Windows 11 把
        // 圖示收進溢位區，他根本看不到它已經在跑了。兩份程序同時活著就是兩個
        // 輪詢迴圈、兩條 `ensure_token` 路徑，直接撞 NVIDIA 那個未公開的
        // 同時有效 token 數量上限，撞到就是一小時內誰都換不到新的。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            panel::show_default(app);
        }))
        // 排第二：後面每一個外掛的初始化錯誤都要寫得進日誌。
        .plugin(gfnusage_lib::logging::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 選單列程式不佔 Dock。`skipTaskbar` 只管 Windows 工作列，Dock 圖示要靠
            // activation policy。Regular 的話要讓 Dock 圖示消失只能 Cmd+Q，選單列圖示
            // 會跟著一起結束。登入視窗開著的時候例外，見 `auth::window::show_in_dock`。
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings_dir = app
                .path()
                .app_config_dir()
                .map_err(|e| format!("找不到設定目錄：{e}"))?;
            let state = Arc::new(AppState::new(settings_dir));
            app.manage(state.clone());
            // 和 `AppState` 放在一起，不要挪到下面。`setup` 是在 config 裡的
            // 視窗建好之後才跑的，webview 那時已經開始載入 —— `get_snapshot`
            // 的簽章要 `State<UpdateState>`，沒 manage 就是 `expect` panic。
            // 現在打不到（事件迴圈還沒開始，IPC 送不進來），但只要有人在底下
            // 加一個會 pump 訊息的呼叫就會變成真的。
            app.manage(gfnusage_lib::update::UpdateState::default());

            // 首次啟動主動把面板叫出來一次。
            //
            // Windows 11 預設把新的系統匣圖示收進溢位區，使用者看不到圖示
            // 就點不開面板 —— 而要他把圖示拖出來的那句提示，正好在面板裡。
            // 不主動出現的話，那句話永遠沒有人看得到。
            let ui_path = state.ui_state_path();
            let mut ui = gfnusage_lib::store::load_ui_state(&ui_path);
            if !ui.first_run_done {
                ui.first_run_done = true;
                // 寫不進去就下次再叫一次，不值得為它讓整個啟動失敗。
                let _ = gfnusage_lib::store::save_ui_state(&ui_path, &ui);
                panel::show_default(app.handle());
            }

            if let Some(window) = app.get_webview_window(PANEL_LABEL) {
                let handle = app.handle().clone();
                window.on_window_event(move |event| match event {
                    // Alt+F4 是這個面板唯一的關閉手勢 —— `decorations: false` 沒有
                    // 關閉鈕，而首次啟動會主動把面板叫出來，正好是使用者想關掉它的
                    // 時刻。不攔的話最後一個視窗被銷毀，整個程序跟著結束：使用者
                    // 以為自己只是把面板收起來，實際上關掉的是常駐程式，額度從此
                    // 不再更新，而且沒有任何提示。真的要離開走系統匣的「結束」，
                    // 那條路是 `app.exit(0)`，不經過這裡。
                    //
                    // 變體與 enum 都是 `#[non_exhaustive]`，`..` 不能省。
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        if let Some(window) = handle.get_webview_window(PANEL_LABEL) {
                            let _ = window.hide();
                        }
                        // 緊接著的系統匣點擊應該被當成「剛關掉」，不要立刻重開。
                        // 自己蓋時間戳，不靠 hide 觸發的 Focused(false)：那一段
                        // 只在面板可見時才蓋。
                        let state = handle.state::<Arc<AppState>>();
                        *state.last_auto_hide.lock().unwrap() = Some(Instant::now());
                    }
                    // 標準 flyout 行為：點到別的地方就收起來。
                    WindowEvent::Focused(false) => {
                        let Some(window) = handle.get_webview_window(PANEL_LABEL) else {
                            return;
                        };
                        // 隱藏中的面板也會失焦：工作列 widget 的右鍵選單借它當擁有者，
                        // muda 會先把它設成前景視窗，選單關掉後它還是前景，下一次點擊
                        // 才把焦點拿走。那一下不是自動收起，蓋了時間戳的話，點在 widget
                        // 上的那一下會被當成「剛關掉」吞掉。查不到可見與否就照舊蓋。
                        let visible = window.is_visible().unwrap_or(true);
                        log::info!(
                            "面板：失焦，{}",
                            if visible {
                                "收起"
                            } else {
                                "隱藏中，不算自動收起"
                            }
                        );
                        if !visible {
                            return;
                        }
                        let _ = window.hide();
                        let state = handle.state::<Arc<AppState>>();
                        *state.last_auto_hide.lock().unwrap() = Some(Instant::now());
                    }
                    _ => {}
                });
            }

            let menu = tray::menu::build(app.handle(), None)?;

            TrayIconBuilder::with_id(tray::TRAY_ID)
                .icon(tray::placeholder_image())
                .tooltip("GFNUsage：尚未取得資料")
                .menu(&menu)
                // 左鍵留給面板，選單走右鍵。
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    tray::menu::QUIT_ID => app.exit(0),
                    // 使用者按了才裝。裝完 updater 會自己結束程序。
                    tray::menu::INSTALL_UPDATE_ID => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = gfnusage_lib::update::install(app).await;
                        });
                    }
                    // 使用者主動更新：不受「需重新登入」的暫停限制。
                    tray::menu::REFRESH_ID => {
                        let state = app.state::<Arc<AppState>>().inner().clone();
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = refresh_into_state(&app, &state).await;
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } = event
                    else {
                        return;
                    };

                    panel::toggle_from_click(tray.app_handle(), position);
                })
                .build(app)?;
            // 上面的 placeholder 是 Windows 系統匣的正方形圖示。macOS 選單列畫的是
            // 另一種（`tray::apply_menubar`），不重畫的話第一輪輪詢回來之前是那一個。
            tray::sync(app.handle(), &state);

            // id_token 的來源：帳號頁那顆 client_id 的靜默授權。
            //
            // 裝在這裡而不是 `AppState::with`，是因為它需要 `AppHandle`
            // —— webview 才拿得到逐場紀錄要的 token（spike 3a）。沒裝的話
            // `ensure_token` 會走舊的 `client_token` 刷新，那條換不到。
            //
            // 就地裝好，不丟給 `spawn`：它必須早於下面那個輪詢迴圈的第一圈。
            {
                let handle = app.handle().clone();
                let owner = Arc::clone(&state);
                let renewer: gfnusage_lib::auth::refresh::Renewer = Arc::new(move || {
                    let handle = handle.clone();
                    let state = Arc::clone(&owner);
                    Box::pin(async move {
                        let device_id =
                            gfnusage_lib::auth::window::device_id(&state.ui_state_path());
                        // 登出會送出取消訊號，進行中的靜默續期要跟著收掉 ——
                        // 不然它換到的那顆會蓋回剛清空的儲存區。
                        let cancel = state.login_cancel.subscribe();
                        let token = gfnusage_lib::auth::window::obtain_token(
                            &handle,
                            &state.http,
                            &state.auth_base,
                            &device_id,
                            true,
                            cancel,
                        )
                        .await?;
                        // 沒有 exp 就不知道什麼時候該換，當成拿不到。
                        let expires_at = token
                            .expires_at
                            .ok_or(gfnusage_lib::error::GfnError::NeedsLogin)?;
                        Ok((token.id_token, expires_at))
                    })
                });
                state.tokens.set_renewer(renewer);
            }

            // GFN 客戶端的視窗偵測。
            //
            // 額度只在串流時變動，而「一場剛玩完」是唯一想立刻看到新數字的
            // 時刻。把那一刻指出來，定時抓就可以拉長成備援。沒有設定開關。
            //
            // 獨立一條 task，不掛在 30 秒心跳上：要分得出「進遊戲」和「退出
            // 遊戲」就得掃得比心跳密，而掃描只是列舉視窗，不發任何請求。
            //
            // 只在 Windows 排。macOS 讀不到別的 app 的視窗標題（見
            // `watcher::win32`），那邊的抓取來源只剩定時與那幾個事件。
            #[cfg(windows)]
            {
                let handle = app.handle().clone();
                let state = Arc::clone(&state);
                tauri::async_runtime::spawn(async move {
                    let mut probe = gfnusage_lib::watcher::Probe::new();
                    // 遊戲結束後的補抓。放在這個迴圈裡而不是另開一條 task：
                    // 連續兩場結束只會留下最後一個到期時間。
                    let mut settle_at: Option<Instant> = None;

                    loop {
                        tokio::time::sleep(gfnusage_lib::watcher::SWEEP).await;

                        if settle_at.is_some_and(|due| Instant::now() >= due) {
                            settle_at = None;
                            if poll_due(&state) {
                                let _ = refresh_into_state(&handle, &state).await;
                            }
                        }

                        let seen = gfnusage_lib::watcher::win32::current_state();
                        let Some(change) = probe.observe(seen) else {
                            continue;
                        };
                        // 記列舉出來的狀態，不記視窗標題 —— 標題裡有遊戲名。
                        let (from, to) = match change {
                            // 第一次確認。不抓，但要寫出來 —— 不然啟動時 GFN
                            // 已經開著的話日誌上不會有任何一行，和「完全沒
                            // 偵測到」分不出來。
                            gfnusage_lib::watcher::Change::Baseline(state) => {
                                log::info!("GFN 視窗：起點 {state:?}");
                                continue;
                            }
                            gfnusage_lib::watcher::Change::Moved(from, to) => (from, to),
                        };
                        log::info!("GFN 視窗：{from:?} → {to:?}");

                        let trigger = gfnusage_lib::watcher::trigger(from, to);
                        if trigger == gfnusage_lib::watcher::Trigger::Twice {
                            settle_at = Some(Instant::now() + gfnusage_lib::watcher::SETTLE);
                        }
                        // `poll_due` 這道閘門和面板開啟同一個理由：憑證被拒
                        // 之後每玩一場就抓一次，等於從前門把 token 上限撞滿。
                        if trigger == gfnusage_lib::watcher::Trigger::None || !poll_due(&state) {
                            continue;
                        }
                        // 剛抓過就跳過立刻那次。補抓照排 —— 它要的是晚一點
                        // 的數字，不是重複現在這一份。
                        if gfnusage_lib::watcher::too_soon(state.last_poll(), Instant::now()) {
                            continue;
                        }
                        let _ = refresh_into_state(&handle, &state).await;
                    }
                });
            }

            // 輪詢迴圈。抓取間隔由使用者在設定頁決定，最長到「關閉」；
            // 串流中的即時警示是 GFN 客戶端自己的職責。
            //
            // 心跳是 30 秒而不是一個抓取間隔，為的是睡眠喚醒（spec §8）：
            // 筆電闔上八小時再打開，使用者不該盯著一個睡前的數字等滿一輪。
            // 心跳本身不做事，只比對兩個時鐘。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_pace: Option<Instant> = None;
                let mut beat = (Instant::now(), chrono::Utc::now());

                loop {
                    let now = Instant::now();
                    let wall = chrono::Utc::now();
                    let woke = commands::woke_from_sleep(now - beat.0, wall - beat.1);
                    beat = (now, wall);

                    // 每一圈現讀，跟 `recompute_pace` 讀 `schedule.json` 同一個
                    // 作法：設定頁改完立刻生效，不必為它開一條通知管道。
                    let interval =
                        gfnusage_lib::store::load_ui_state(&state.ui_state_path()).poll_interval;
                    // 喚醒與開機第一圈即使在「關閉」也抓。關掉的是定時，不是
                    // 事件。睡了八小時之後系統匣掛著睡前的數字，正是這條要擋的。
                    let fetch_due =
                        woke || commands::interval_elapsed(state.last_poll(), now, interval);

                    // 喚醒也要走 `poll_due` 這道閘門。憑證被拒絕後暫停輪詢，是因為
                    // 每次抓都會為了 401 重試再鑄一顆 token，不停的話一小時就撞
                    // 上限 —— 讓喚醒繞過它，就是把那個洞重新打開。
                    if fetch_due && poll_due(&state) {
                        last_pace = Some(now);
                        // `last_poll` 由 `refresh_into_state` 自己更新，因為
                        // 系統匣與面板那幾條路也要往後推同一個計時器。
                        let _ = refresh_into_state(&handle, &state).await;
                    } else if woke
                        || last_pace.map_or(true, |last| now.duration_since(last) >= PACE_INTERVAL)
                    {
                        // 沒抓也要重算：A_past 變大會讓配速結論翻轉。
                        last_pace = Some(now);
                        commands::recompute_pace(&state, chrono::Utc::now());
                        tray::sync(&handle, &state);
                    }

                    tokio::time::sleep(TICK).await;
                }
            });

            // 更新檢查獨立一條 task。不掛在 30 秒心跳上：心跳每半分鐘醒一次，
            // 要不要發網路請求的判斷會變成每半分鐘一次；更要緊的是，檢查失敗
            // 若走 `refresh_into_state` 的錯誤路徑，會蓋掉面板上額度的錯誤。
            //
            // debug 建構不排這條：開發中每天對 GitHub 發一次請求沒有意義。
            // 「按下去會不會真的裝」那道閘門在 `update::install` 裡，所以關於頁
            // 的「檢查更新」在 `tauri dev` 仍然按得到，正好拿來驗端點通不通。
            #[cfg(not(debug_assertions))]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(gfnusage_lib::update::FIRST_CHECK_DELAY).await;
                    loop {
                        gfnusage_lib::update::check_once(&handle).await;
                        tokio::time::sleep(gfnusage_lib::update::CHECK_INTERVAL).await;
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_schedule,
            commands::set_schedule,
            commands::import_from_local_gfn,
            commands::import_manual,
            commands::refresh_now,
            commands::refresh_if_due,
            commands::sign_out,
            commands::start_login,
            commands::cancel_login_command,
            commands::export_schedule,
            commands::import_schedule,
            commands::dismiss_tray_hint,
            commands::set_metric,
            commands::set_poll_interval,
            commands::set_taskbar_widget,
            commands::app_version,
            commands::get_update_status,
            commands::check_update_now,
            commands::install_update,
            commands::dismiss_update,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
