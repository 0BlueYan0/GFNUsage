#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gfnusage_lib::commands::{self, refresh_into_state};
use gfnusage_lib::tray;
use gfnusage_lib::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, PhysicalPosition, WebviewWindow, WindowEvent};

const POLL_INTERVAL: Duration = Duration::from_secs(300);

/// 自動收起後多久之內的系統匣點擊，視為「關閉」而不是「開啟」。
///
/// 點圖示會先讓面板失焦，失焦處理器把它收起來，接著點擊事件才送到 —
/// 沒有這個寬限期，面板就會在同一次點擊中收起又立刻重開，變成關不掉。
const REOPEN_GRACE: Duration = Duration::from_millis(300);

const PANEL_LABEL: &str = "main";

/// 面板與螢幕可用區邊緣的間距。
const PANEL_MARGIN: i32 = 12;

/// 把面板貼到離點擊處最近的可用區角落。
///
/// 用「最近的角落」而不是寫死右下角：工作列可以在四邊任何一側，而系統匣永遠
/// 緊鄰工作列，所以離點擊處最近的角落一定就是正確的那個角落。可用區
/// （work area）已經排除工作列本身，面板不會被壓在它底下。
fn position_panel(window: &WebviewWindow, near: PhysicalPosition<f64>) {
    let monitor = match window.app_handle().monitor_from_point(near.x, near.y) {
        Ok(Some(monitor)) => monitor,
        _ => match window.primary_monitor() {
            Ok(Some(monitor)) => monitor,
            _ => return,
        },
    };

    let area = monitor.work_area();
    let Ok(size) = window.outer_size() else {
        return;
    };

    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);
    let (ww, wh) = (size.width as i32, size.height as i32);

    let x = if (near.x as i32) > ax + aw / 2 {
        ax + aw - ww - PANEL_MARGIN
    } else {
        ax + PANEL_MARGIN
    };
    let y = if (near.y as i32) > ay + ah / 2 {
        ay + ah - wh - PANEL_MARGIN
    } else {
        ay + PANEL_MARGIN
    };

    // 面板比可用區還大時（極小螢幕），夾在區域內而不是跑到畫面外。
    let x = x.clamp(ax, (ax + aw - ww).max(ax));
    let y = y.clamp(ay, (ay + ah - wh).max(ay));

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show_panel(window: &WebviewWindow, near: PhysicalPosition<f64>) {
    // 先定位再顯示，否則會在舊位置閃一下。
    position_panel(window, near);
    let _ = window.show();
    let _ = window.set_focus();
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let state = Arc::new(AppState::new());
            app.manage(state.clone());

            // 標準 flyout 行為：點到別的地方就收起來。
            if let Some(window) = app.get_webview_window(PANEL_LABEL) {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Focused(false)) {
                        if let Some(window) = handle.get_webview_window(PANEL_LABEL) {
                            let _ = window.hide();
                        }
                        let state = handle.state::<Arc<AppState>>();
                        *state.last_auto_hide.lock().unwrap() = Some(Instant::now());
                    }
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
                        show_panel(&window, position);
                    }
                })
                .build(app)?;

            // 輪詢迴圈。額度只在串流時變動，5 分鐘一次已足夠；
            // 串流中的即時警示是 GFN 客戶端自己的職責。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let _ = refresh_into_state(&handle, &state).await;
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::import_from_local_gfn,
            commands::import_manual,
            commands::refresh_now,
            commands::sign_out,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
