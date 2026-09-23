//! 工作列 widget：掛進 `Shell_TrayWnd` 的一扇 layered 子視窗，只有 Windows 有。
//!
//! `face` 決定畫什麼，`render` 畫成點陣圖，`layout` 算擺在哪裡，三個都是純函式。
//! `host` 是擁有那扇視窗的執行緒。`sync` 由 `tray::sync` 呼叫，所以系統匣會
//! 重畫的時候 widget 也會。

pub mod face;
pub mod layout;
pub mod render;
pub mod taskbar;

#[cfg(windows)]
mod host;

use tauri::{AppHandle, Runtime};

use crate::store::UiState;
use crate::AppState;

#[cfg(windows)]
static HOST: std::sync::Mutex<Option<host::Host>> = std::sync::Mutex::new(None);

/// 依設定開、關、重畫 widget。
#[cfg(windows)]
pub fn sync<R: Runtime>(app: &AppHandle<R>, state: &AppState, ui: &UiState) {
    use std::sync::atomic::Ordering;

    let mut current = HOST.lock().unwrap();
    // 執行緒 panic 的話換一個新的。panic 本身已經由 `logging::log_panics` 記下。
    // 不在 host 裡面 catch_unwind：panic 時 `Local` 停在哪一半不知道。換一條新的，
    // 狀態從頭建，舊執行緒的視窗在它結束時已經被 Windows 摧毀。
    if current.as_ref().is_some_and(|host| !host.is_alive()) {
        log::warn!("工作列 widget：執行緒停了，重開");
        *current = None;
    }
    if !ui.taskbar_widget.enabled {
        if current.take().is_some() {
            log::info!("工作列 widget：關閉");
        }
        return;
    }

    let snapshot = state.snapshot.lock().unwrap().clone();
    let pace = state.pace.lock().unwrap().clone();
    let wanted = host::Wanted {
        face: face::face(
            snapshot.as_ref(),
            pace.as_ref(),
            state.needs_login.load(Ordering::SeqCst),
            ui.metric,
            chrono::Utc::now(),
            ui.poll_interval.stale_after(),
        ),
        side: ui.taskbar_widget.side,
    };

    match current.as_ref() {
        Some(host) => host.update(wanted),
        None => {
            log::info!("工作列 widget：開啟");
            *current = host::Host::start(wanted, on_click(app.clone()));
        }
    }
}

/// macOS 沒有工作列。
#[cfg(not(windows))]
pub fn sync<R: Runtime>(_app: &AppHandle<R>, _state: &AppState, _ui: &UiState) {}

/// 點擊在 host 執行緒上收到，丟回主執行緒處理：面板與選單都是 Tauri 的東西。
#[cfg(windows)]
fn on_click<R: Runtime>(app: AppHandle<R>) -> host::OnClick {
    Box::new(move |button, x, y| {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || match button {
            host::Button::Left => crate::panel::toggle_from_click(
                &handle,
                tauri::PhysicalPosition::new(f64::from(x), f64::from(y)),
            ),
            host::Button::Right => {
                crate::panel::popup_tray_menu(&handle, tauri::PhysicalPosition::new(x, y))
            }
        });
    })
}
