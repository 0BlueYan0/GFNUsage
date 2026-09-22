pub mod icon;

pub mod menu;

use std::sync::atomic::Ordering;

use chrono::{DateTime, Duration, Local, Utc};
use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Runtime};

use crate::pace::PaceReport;
use crate::quota::{human_duration, DisplayState, QuotaSnapshot};
use crate::AppState;

pub const TRAY_ID: &str = "main";

const IDLE_TOOLTIP: &str = "GFNUsage：尚未取得資料";

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
///
/// `stale_after` 由呼叫端從抓取間隔換算（`PollInterval::stale_after`），
/// 不寫死在這裡：間隔設成 6 小時的時候，資料 40 分鐘舊是正常的。
pub fn face(
    snapshot: Option<&QuotaSnapshot>,
    pace: Option<&PaceReport>,
    error: Option<&str>,
    needs_login: bool,
    now: DateTime<Utc>,
    stale_after: Duration,
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

    let state = crate::pace::display_state(snapshot, pace);
    // 已用完時不顯示那個看起來很正常的 0（spec §7.2）。實際數字在 tooltip 裡。
    let label = if state == DisplayState::Exhausted {
        "!".to_string()
    } else {
        snapshot.tray_label()
    };
    let stale = now - snapshot.fetched_at > stale_after;
    let base = icon::state_color(state);

    let mut tooltip = if snapshot.time_capped {
        format!(
            "GeForce NOW：剩餘 {} / {}",
            human_duration(snapshot.remaining_minutes),
            human_duration(snapshot.total_minutes)
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
    let pace = state.pace.lock().unwrap().clone();
    let error = state.display_error();
    let needs_login = state.needs_login.load(Ordering::SeqCst);
    // 現讀，跟 `panel_data` 同一個作法。不在 `AppState` 裡再放一份快取：
    // 兩份來源遲早會有一份忘了更新。
    let stale_after = crate::store::load_ui_state(&state.ui_state_path())
        .poll_interval
        .stale_after();
    apply_face(
        &tray,
        &face(
            snapshot.as_ref(),
            pace.as_ref(),
            error.as_deref(),
            needs_login,
            Utc::now(),
            stale_after,
        ),
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

    /// 預設間隔換算出來的過期門檻，一小時。
    fn stale() -> Duration {
        crate::store::PollInterval::default().stale_after()
    }

    #[test]
    fn idle_face_when_nothing_has_been_fetched() {
        let f = face(None, None, None, false, now(), stale());
        assert_eq!(f.label, "–");
        assert_eq!(f.title, None);
        assert_eq!(f.tooltip, IDLE_TOOLTIP);
    }

    #[test]
    fn healthy_snapshot_shows_hours_in_the_state_color() {
        let f = face(Some(&snapshot(now())), None, None, false, now(), stale());
        assert_eq!(f.label, "103");
        assert_eq!(f.title.as_deref(), Some("103h"));
        assert_eq!(f.color, icon::state_color(DisplayState::Normal));
        assert_eq!(f.tooltip, "GeForce NOW：剩餘 103 小時 / 115 小時");
    }

    /// 憑證死了，圖示不能還一副數字很正常的樣子。
    #[test]
    fn needs_login_shows_an_exclamation_mark() {
        let f = face(
            Some(&snapshot(now())),
            None,
            Some("需要重新登入"),
            true,
            now(),
            stale(),
        );
        assert_eq!(f.label, "!");
        assert_eq!(f.color, icon::state_color(DisplayState::FreeTier));
        assert_eq!(f.title, None);
        assert!(f.tooltip.contains("需要重新登入"), "{}", f.tooltip);
    }

    #[test]
    fn a_recent_error_keeps_the_number_and_reports_it_in_the_tooltip() {
        let fetched = now() - Duration::minutes(3);
        let f = face(
            Some(&snapshot(fetched)),
            None,
            Some("網路錯誤：離線"),
            false,
            now(),
            stale(),
        );
        assert_eq!(f.label, "103");
        assert_eq!(f.color, icon::state_color(DisplayState::Normal));
        assert!(f.tooltip.contains("網路錯誤：離線"), "{}", f.tooltip);
        assert!(f.tooltip.contains("資料時間"), "{}", f.tooltip);
    }

    /// spec §7.2：上次成功抓取距今超過門檻，數字變淡。
    #[test]
    fn a_stale_snapshot_is_dimmed() {
        let fetched = now() - stale() - Duration::minutes(1);
        let f = face(
            Some(&snapshot(fetched)),
            None,
            Some("網路錯誤：離線"),
            false,
            now(),
            stale(),
        );
        assert_eq!(f.label, "103");
        assert_eq!(
            f.color,
            icon::dimmed(icon::state_color(DisplayState::Normal))
        );
    }

    /// 同一份資料，間隔拉長就不算過期了。
    ///
    /// 沒有這條的話「淡」會變成常態：使用者選 6 小時，而門檻若還寫死 30 分鐘，
    /// 圖示幾乎永遠是淡的，那個訊號就不再指出任何事情。
    #[test]
    fn a_longer_interval_pushes_the_staleness_threshold_out() {
        use crate::store::PollInterval;

        let fetched = now() - Duration::hours(3);
        let dimmed = |interval: PollInterval| {
            face(
                Some(&snapshot(fetched)),
                None,
                None,
                false,
                now(),
                interval.stale_after(),
            )
            .color
                == icon::dimmed(icon::state_color(DisplayState::Normal))
        };

        assert!(dimmed(PollInterval::Min30));
        assert!(dimmed(PollInterval::Hour1));
        assert!(!dimmed(PollInterval::Hour6));
        assert!(!dimmed(PollInterval::Off));
    }

    #[test]
    fn an_error_without_a_snapshot_goes_in_the_tooltip() {
        let f = face(None, None, Some("網路錯誤：離線"), false, now(), stale());
        assert_eq!(f.label, "–");
        assert!(f.tooltip.contains("網路錯誤：離線"), "{}", f.tooltip);
    }

    /// spec §7.2：超前消耗轉紅。
    #[test]
    fn over_pace_turns_the_icon_red() {
        use crate::pace::{self, PaceReport};

        let snap = snapshot(now());
        let report = PaceReport {
            avail_past_minutes: 14400,
            avail_left_minutes: 28800,
            expected_used_minutes: Some(2000.0),
            over_pace_minutes: Some(1000.0),
            burn_rate: Some(0.2),
            projected_used_minutes: None,
            overshoot_minutes: None,
            runs_out_at: None,
            wasted_minutes: None,
            today_budget_minutes: None,
            note: None,
        };
        let f = face(Some(&snap), Some(&report), None, false, now(), stale());
        assert_eq!(f.label, "103");
        assert_eq!(f.color, icon::state_color(DisplayState::OverPace));
        assert_eq!(
            pace::display_state(&snap, Some(&report)),
            DisplayState::OverPace
        );
    }

    /// spec §7.2：已用完是紅色加驚嘆號，不是一個看起來很正常的 0。
    #[test]
    fn an_exhausted_quota_shows_an_exclamation_mark() {
        let mut sub: Subscription = serde_json::from_str(FIXTURE).unwrap();
        sub.remaining_time_in_minutes = 0;
        sub.current_subscription_state.is_game_play_allowed = false;
        let snap = QuotaSnapshot::from_subscription(&sub, now());

        let f = face(Some(&snap), None, None, false, now(), stale());
        assert_eq!(f.label, "!");
        assert_eq!(f.color, icon::state_color(DisplayState::Exhausted));
        assert!(f.tooltip.contains("0 分鐘"), "{}", f.tooltip);
    }
}
