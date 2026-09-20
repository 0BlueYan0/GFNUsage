pub mod icon;

use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Runtime};

use crate::quota::{DisplayState, QuotaSnapshot};

pub const TRAY_ID: &str = "main";

const IDLE_TOOLTIP: &str = "GFNUsage：尚未取得資料";

fn image(text: &str, state: DisplayState) -> Image<'static> {
    Image::new_owned(
        icon::render(text, icon::state_color(state)),
        icon::ICON_SIZE,
        icon::ICON_SIZE,
    )
}

/// 尚未取得資料時的圖示。
pub fn placeholder_image() -> Image<'static> {
    image("\u{2013}", DisplayState::FreeTier)
}

/// 把快照畫上系統匣。
///
/// macOS 的選單列可以顯示文字，Windows 的系統匣只吃圖片 —— `set_title` 在
/// Windows 上是無作用的，所以兩者都設，各自在自己的平台生效。
pub fn apply_snapshot<R: Runtime>(tray: &TrayIcon<R>, snapshot: &QuotaSnapshot) {
    let label = snapshot.tray_label();

    let _ = tray.set_icon(Some(image(&label, snapshot.state)));
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

/// 回到「尚未取得資料」的樣子。解除連結後用。
pub fn apply_idle<R: Runtime>(tray: &TrayIcon<R>) {
    let _ = tray.set_icon(Some(placeholder_image()));
    let _ = tray.set_title(None::<&str>);
    let _ = tray.set_tooltip(Some(IDLE_TOOLTIP));
}

/// 依 id 取得系統匣並套用快照。系統匣尚未建立時靜默略過。
pub fn update<R: Runtime>(app: &AppHandle<R>, snapshot: &QuotaSnapshot) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        apply_snapshot(&tray, snapshot);
    }
}

/// 依 id 取得系統匣並重設為未取得資料的狀態。
pub fn clear<R: Runtime>(app: &AppHandle<R>) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        apply_idle(&tray);
    }
}
