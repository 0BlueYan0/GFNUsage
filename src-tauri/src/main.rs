#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::Duration;

use gfnusage_lib::commands::{self, refresh_into_state};
use gfnusage_lib::quota::QuotaSnapshot;
use gfnusage_lib::tray::icon;
use gfnusage_lib::AppState;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, Runtime, WebviewWindow};

const TRAY_ID: &str = "main";
const POLL_INTERVAL: Duration = Duration::from_secs(300);

/// 把快照畫上系統匣。
///
/// macOS 的選單列可以顯示文字，Windows 的系統匣只吃圖片 —— `set_title` 在
/// Windows 上是無作用的，所以兩者都設，各自在自己的平台生效。
fn apply_snapshot<R: Runtime>(tray: &TrayIcon<R>, snapshot: &QuotaSnapshot) {
    let label = snapshot.tray_label();
    let rgba = icon::render(&label, icon::state_color(snapshot.state));

    let _ = tray.set_icon(Some(Image::new_owned(
        rgba,
        icon::ICON_SIZE,
        icon::ICON_SIZE,
    )));
    let _ = tray.set_title(Some(format!("{label}h")));

    let tooltip = if snapshot.time_capped {
        format!(
            "GeForce NOW：剩餘 {:.1} / {:.1} 小時",
            snapshot.remaining_minutes as f32 / 60.0,
            snapshot.total_minutes as f32 / 60.0
        )
    } else {
        format!("GeForce NOW · {}：此方案沒有時數上限", snapshot.tier)
    };
    let _ = tray.set_tooltip(Some(tooltip));
}

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

            let refresh_item =
                MenuItem::with_id(app, "refresh", "立即更新", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "結束", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_item, &quit_item])?;

            TrayIconBuilder::with_id(TRAY_ID)
                .icon(Image::new_owned(
                    icon::render("\u{2013}", icon::state_color(
                        gfnusage_lib::quota::DisplayState::FreeTier,
                    )),
                    icon::ICON_SIZE,
                    icon::ICON_SIZE,
                ))
                .tooltip("GFNUsage：尚未取得資料")
                .menu(&menu)
                // 左鍵留給面板，選單走右鍵。
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "quit" => app.exit(0),
                    "refresh" => {
                        let state = app.state::<Arc<AppState>>().inner().clone();
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Ok(snapshot) = refresh_into_state(&state).await {
                                if let Some(tray) = handle.tray_by_id(TRAY_ID) {
                                    apply_snapshot(&tray, &snapshot);
                                }
                            }
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
                    if let Ok(snapshot) = refresh_into_state(&state).await {
                        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
                            apply_snapshot(&tray, &snapshot);
                        }
                    }
                    // 失敗時保留上一次的快照，錯誤已記在 state 裡供面板顯示。
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
