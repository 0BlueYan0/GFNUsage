#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::Duration;

use gfnusage_lib::commands::{self, refresh_into_state};
use gfnusage_lib::tray;
use gfnusage_lib::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WebviewWindow};

const POLL_INTERVAL: Duration = Duration::from_secs(300);

fn toggle_panel(window: &WebviewWindow) {
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let state = Arc::new(AppState::new());
            app.manage(state.clone());

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
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            toggle_panel(&window);
                        }
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
