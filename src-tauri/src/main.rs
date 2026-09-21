#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gfnusage_lib::commands::{self, poll_due, refresh_into_state};
use gfnusage_lib::panel::{self, PANEL_LABEL};
use gfnusage_lib::tray;
use gfnusage_lib::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

const POLL_INTERVAL: Duration = Duration::from_secs(300);

/// 迴圈的心跳。比輪詢間隔短得多，好讓睡眠喚醒在半分鐘內就被發現。
const TICK: Duration = Duration::from_secs(30);

/// 自動收起後多久之內的系統匣點擊，視為「關閉」而不是「開啟」。
///
/// 點圖示會先讓面板失焦，失焦處理器把它收起來，接著點擊事件才送到 —
/// 沒有這個寬限期，面板就會在同一次點擊中收起又立刻重開，變成關不掉。
const REOPEN_GRACE: Duration = Duration::from_millis(300);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings_dir = app
                .path()
                .app_config_dir()
                .map_err(|e| format!("找不到設定目錄：{e}"))?;
            let state = Arc::new(AppState::new(settings_dir));
            app.manage(state.clone());

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
                            // 這一下 hide 會再觸發 Focused(false)，於是下面那一段
                            // 也會跑一次，把 last_auto_hide 蓋上時間戳。那正是要的：
                            // 緊接著的系統匣點擊應該被當成「剛關掉」，不要立刻重開。
                            let _ = window.hide();
                        }
                    }
                    // 標準 flyout 行為：點到別的地方就收起來。
                    WindowEvent::Focused(false) => {
                        if let Some(window) = handle.get_webview_window(PANEL_LABEL) {
                            let _ = window.hide();
                        }
                        let state = handle.state::<Arc<AppState>>();
                        *state.last_auto_hide.lock().unwrap() = Some(Instant::now());
                    }
                    _ => {}
                });
            }

            let refresh_item = MenuItem::with_id(app, "refresh", "立即更新", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "結束", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_item, &quit_item])?;

            TrayIconBuilder::with_id(tray::TRAY_ID)
                .icon(tray::placeholder_image())
                .tooltip("GFNUsage：尚未取得資料")
                .menu(&menu)
                // 左鍵留給面板，選單走右鍵。
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "quit" => app.exit(0),
                    // 使用者主動更新：不受「需重新登入」的暫停限制。
                    "refresh" => {
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

                    let app = tray.app_handle();
                    let state = app.state::<Arc<AppState>>();

                    // 這一下點擊剛剛才讓面板失焦收起，所以它是關閉動作。
                    let just_closed = state
                        .last_auto_hide
                        .lock()
                        .unwrap()
                        .is_some_and(|at| at.elapsed() < REOPEN_GRACE);
                    if just_closed {
                        return;
                    }

                    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
                        panel::show(&window, position);
                    }
                })
                .build(app)?;

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

            // 輪詢迴圈。額度只在串流時變動，5 分鐘一次已足夠；
            // 串流中的即時警示是 GFN 客戶端自己的職責。
            //
            // 心跳是 30 秒而不是 5 分鐘，為的是睡眠喚醒（spec §8）：
            // 筆電闔上八小時再打開，使用者不該盯著一個睡前的數字等滿五分鐘。
            // 心跳本身不做事，只比對兩個時鐘。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // None 代表還沒抓過，所以啟動時第一圈就會抓。
                let mut last_poll: Option<Instant> = None;
                let mut beat = (Instant::now(), chrono::Utc::now());

                loop {
                    let now = Instant::now();
                    let wall = chrono::Utc::now();
                    let woke = commands::woke_from_sleep(now - beat.0, wall - beat.1);
                    beat = (now, wall);

                    let due = woke
                        || last_poll.map_or(true, |last| now.duration_since(last) >= POLL_INTERVAL);
                    if !due {
                        tokio::time::sleep(TICK).await;
                        continue;
                    }
                    last_poll = Some(now);

                    // 喚醒也要走這道閘門。憑證被拒絕後暫停輪詢，是因為每次抓
                    // 都會為了 401 重試再鑄一顆 token，不停的話一小時就撞上限
                    // —— 讓喚醒繞過它，就是把那個洞重新打開。
                    if poll_due(&state) {
                        let _ = refresh_into_state(&handle, &state).await;
                    } else {
                        // 沒抓也要重算：A_past 變大會讓配速結論翻轉。
                        commands::recompute_pace(&state, chrono::Utc::now());
                        tray::sync(&handle, &state);
                    }

                    tokio::time::sleep(TICK).await;
                }
            });

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
