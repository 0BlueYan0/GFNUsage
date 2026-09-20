pub mod icon;

use std::sync::atomic::Ordering;

use chrono::{DateTime, Duration, Local, Utc};
use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Runtime};

use crate::quota::{DisplayState, QuotaSnapshot};
use crate::AppState;

pub const TRAY_ID: &str = "main";

const IDLE_TOOLTIP: &str = "GFNUsage：尚未取得資料";

/// 上次成功抓取距今超過此值，數字變淡（spec §7.2「資料過期」）。
pub const STALE_AFTER_MINUTES: i64 = 30;

/// 系統匣該長什麼樣子。純資料：由 `face()` 算出，`apply_face()` 畫上去。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayFace {
    /// 畫進圖示的文字（Windows 的系統匣只吃圖片）。
    pub label: String,
    pub color: [u8; 3],
    /// 選單列文字（macOS）。`None` 代表只顯示圖示。
    pub title: Option<String>,
    pub tooltip: String,
}

/// 由目前狀態決定系統匣外觀。
///
/// 優先順序：需重新登入 → 沒資料 → 有資料（過期就變淡）。
/// 錯誤一律寫進 tooltip，圖示上的數字則留著 —— 一次失敗的輪詢不該把上次的
/// 好資料藏起來；但憑證死了就不能再一副數字很正常的樣子。
pub fn face(
    snapshot: Option<&QuotaSnapshot>,
    error: Option<&str>,
    needs_login: bool,
    now: DateTime<Utc>,
) -> TrayFace {
    let muted = icon::state_color(DisplayState::FreeTier);

    if needs_login {
        return TrayFace {
            label: "!".into(),
            color: muted,
            title: None,
            tooltip: format!("GeForce NOW：{}", error.unwrap_or("需要重新登入")),
        };
    }

    let Some(snapshot) = snapshot else {
        return TrayFace {
            label: "\u{2013}".into(),
            color: muted,
            title: None,
            tooltip: match error {
                Some(e) => format!("GFNUsage：{e}"),
                None => IDLE_TOOLTIP.into(),
            },
        };
    };

    let label = snapshot.tray_label();
    let stale = now - snapshot.fetched_at > Duration::minutes(STALE_AFTER_MINUTES);
    let base = icon::state_color(snapshot.state);

    let mut tooltip = if snapshot.time_capped {
        format!(
            "GeForce NOW：剩餘 {:.1} / {:.1} 小時",
            snapshot.remaining_minutes as f32 / 60.0,
            snapshot.total_minutes as f32 / 60.0
        )
    } else {
        format!("GeForce NOW · {}：此方案沒有時數上限", snapshot.tier)
    };
    if let Some(e) = error {
        tooltip.push('\n');
        tooltip.push_str(e);
    }
    if error.is_some() || stale {
        let at = snapshot.fetched_at.with_timezone(&Local).format("%H:%M");
        tooltip.push_str(&format!("\n資料時間 {at}"));
    }

    TrayFace {
        title: Some(if snapshot.time_capped {
            format!("{label}h")
        } else {
            label.clone()
        }),
        label,
        color: if stale { icon::dimmed(base) } else { base },
        tooltip,
    }
}

fn image(text: &str, color: [u8; 3]) -> Image<'static> {
    Image::new_owned(icon::render(text, color), icon::ICON_SIZE, icon::ICON_SIZE)
}

/// 尚未取得資料時的圖示。
pub fn placeholder_image() -> Image<'static> {
    image("\u{2013}", icon::state_color(DisplayState::FreeTier))
}

/// 把算好的外觀畫上系統匣。
///
/// macOS 的選單列可以顯示文字，Windows 的系統匣只吃圖片 —— `set_title` 在
/// Windows 上是無作用的，所以兩者都設，各自在自己的平台生效。
pub fn apply_face<R: Runtime>(tray: &TrayIcon<R>, face: &TrayFace) {
    let _ = tray.set_icon(Some(image(&face.label, face.color)));
    let _ = tray.set_title(face.title.as_deref());
    let _ = tray.set_tooltip(Some(&face.tooltip));
}

/// 依目前的共用狀態重畫系統匣。所有會改動狀態的操作最後都呼叫這裡。
/// 系統匣尚未建立時靜默略過。
pub fn sync<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let snapshot = state.snapshot.lock().unwrap().clone();
    let error = state.last_error.lock().unwrap().clone();
    let needs_login = state.needs_login.load(Ordering::SeqCst);
    apply_face(
        &tray,
        &face(snapshot.as_ref(), error.as_deref(), needs_login, Utc::now()),
    );
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Duration, TimeZone, Utc};

    use super::*;
    use crate::api::subscriptions::Subscription;

    const FIXTURE: &str = include_str!("../../tests/fixtures/subscription.json");

    fn now() -> DateTime<Utc> {
        Utc.timestamp_opt(1789817022, 0).unwrap()
    }

    fn snapshot(fetched_at: DateTime<Utc>) -> QuotaSnapshot {
        let sub: Subscription = serde_json::from_str(FIXTURE).unwrap();
        QuotaSnapshot::from_subscription(&sub, fetched_at)
    }

    #[test]
    fn idle_face_when_nothing_has_been_fetched() {
        let f = face(None, None, false, now());
        assert_eq!(f.label, "–");
        assert_eq!(f.title, None);
        assert_eq!(f.tooltip, IDLE_TOOLTIP);
    }

    #[test]
    fn healthy_snapshot_shows_hours_in_the_state_color() {
        let f = face(Some(&snapshot(now())), None, false, now());
        assert_eq!(f.label, "103");
        assert_eq!(f.title.as_deref(), Some("103h"));
        assert_eq!(f.color, icon::state_color(DisplayState::Normal));
        assert_eq!(f.tooltip, "GeForce NOW：剩餘 103.0 / 115.0 小時");
    }

    /// 憑證死了，圖示不能還一副數字很正常的樣子。
    #[test]
    fn needs_login_shows_an_exclamation_mark() {
        let f = face(Some(&snapshot(now())), Some("需要重新登入"), true, now());
        assert_eq!(f.label, "!");
        assert_eq!(f.color, icon::state_color(DisplayState::FreeTier));
        assert_eq!(f.title, None);
        assert!(f.tooltip.contains("需要重新登入"), "{}", f.tooltip);
    }

    #[test]
    fn a_recent_error_keeps_the_number_and_reports_it_in_the_tooltip() {
        let fetched = now() - Duration::minutes(3);
        let f = face(Some(&snapshot(fetched)), Some("網路錯誤：離線"), false, now());
        assert_eq!(f.label, "103");
        assert_eq!(f.color, icon::state_color(DisplayState::Normal));
        assert!(f.tooltip.contains("網路錯誤：離線"), "{}", f.tooltip);
        assert!(f.tooltip.contains("資料時間"), "{}", f.tooltip);
    }

    /// spec §7.2：上次成功抓取距今超過 30 分鐘，數字變淡。
    #[test]
    fn a_stale_snapshot_is_dimmed() {
        let fetched = now() - Duration::minutes(STALE_AFTER_MINUTES + 1);
        let f = face(Some(&snapshot(fetched)), Some("網路錯誤：離線"), false, now());
        assert_eq!(f.label, "103");
        assert_eq!(f.color, icon::dimmed(icon::state_color(DisplayState::Normal)));
    }

    #[test]
    fn an_error_without_a_snapshot_goes_in_the_tooltip() {
        let f = face(None, Some("網路錯誤：離線"), false, now());
        assert_eq!(f.label, "–");
        assert!(f.tooltip.contains("網路錯誤：離線"), "{}", f.tooltip);
    }
}
