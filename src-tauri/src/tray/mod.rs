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
            tooltip: format!("GeForce NOW：{}", error.unwrap_or("需要重新登入")),
        };
    }

    let Some(snapshot) = snapshot else {
        return TrayFace {
            label: "\u{2013}".into(),
            color: muted,
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

/// 把算好的外觀畫上 Windows 的系統匣。macOS 走 `apply_menubar`。
///
/// 不設 title：Windows 的系統匣只吃圖片，`set_title` 在這裡無作用。`TrayFace`
/// 以前帶一個給 macOS 選單列的 title，`apply_menubar` 接手之後沒有平台顯示它。
pub fn apply_face<R: Runtime>(tray: &TrayIcon<R>, face: &TrayFace) {
    let _ = tray.set_icon(Some(image(&face.label, face.color)));
    let _ = tray.set_tooltip(Some(&face.tooltip));
}

/// 依目前的共用狀態重畫系統匣。所有會改動狀態的操作最後都呼叫這裡。
/// 系統匣尚未建立時靜默略過。
pub fn sync<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    // 現讀，跟 `panel_data` 同一個作法。不在 `AppState` 裡再放一份快取：
    // 兩份來源遲早會有一份忘了更新。
    let ui = crate::store::load_ui_state(&state.ui_state_path());
    // 排在系統匣之前：系統匣還沒建起來時下面會提早 return，widget 不該跟著不畫。
    crate::widget::sync(app, state, &ui);

    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let snapshot = state.snapshot.lock().unwrap().clone();
    let pace = state.pace.lock().unwrap().clone();
    let error = state.display_error();
    let needs_login = state.needs_login.load(Ordering::SeqCst);
    let stale_after = ui.poll_interval.stale_after();
    let now = Utc::now();
    let tray_face = face(
        snapshot.as_ref(),
        pace.as_ref(),
        error.as_deref(),
        needs_login,
        now,
        stale_after,
    );
    #[cfg(target_os = "macos")]
    {
        let widget = crate::widget::face::face(
            snapshot.as_ref(),
            pace.as_ref(),
            needs_login,
            ui.metric,
            now,
            stale_after,
        );
        apply_menubar(&tray, &widget, &tray_face.tooltip);
    }
    #[cfg(not(target_os = "macos"))]
    apply_face(&tray, &tray_face);
}

/// macOS 選單列畫成工作列 widget 那張圖：時間加進度條。尺寸在
/// `widget::render::Style::menubar` 與 `MENUBAR_H`，跟守它們的測試同一個檔。
///
/// 不用 `apply_face`：它畫的是 Windows 系統匣那種正方形數字圖示。
///
/// 正常與沒資料畫成 template 圖片，由系統上色。macOS 26 的選單列是透明的，字色
/// 跟著桌布深淺變，程式自己判斷深淺色會不準。沒資料那組灰是照 Windows 深色
/// 工作列挑的，淺色選單列上對比不到 2:1。偏低、超前、用完要看得出顏色，畫成
/// 彩色圖片。
///
/// title 設成空字串而不是 `None`：tray-icon 0.24 在 macOS 上收到 `None` 不動
/// 按鈕，上一次的文字會留著。
#[cfg(target_os = "macos")]
fn apply_menubar<R: Runtime>(
    tray: &TrayIcon<R>,
    widget: &crate::widget::face::WidgetFace,
    tooltip: &str,
) {
    use crate::widget::face::Tone;
    use crate::widget::render::{render_rgba, width_of, Style, MENUBAR_H, WIDEST};

    let template = matches!(widget.tone, Tone::Normal | Tone::Muted);
    let style = Style::menubar();
    // 有進度條時寬度固定成最寬的那串，理由同 `WIDEST`：數字變了旁邊的圖示
    // 不會跟著移動。沒有進度條時（沒資料、要重新登入）只有一個「–」或「!」，
    // 照最寬的留白的話兩邊是一大塊空的。
    let w = if widget.fill.is_some() {
        width_of(WIDEST, &style)
    } else {
        width_of(&widget.text, &style)
    };
    let rgba = render_rgba(widget, w, MENUBAR_H, &style, true);
    // 圖片與 template 一次設。tray-icon 0.24 的 `set_icon` 在 macOS 上把 template
    // 寫死成 false，分兩步設的話中間有一格畫面是黑字，面板打開後重畫的那一下
    // 看起來是閃一次（2026-09-26 錄影看到的）。
    let _ = tray.set_icon_with_as_template(
        Some(Image::new_owned(rgba, w as u32, MENUBAR_H as u32)),
        template,
    );
    let _ = tray.set_title(Some(""));
    let _ = tray.set_tooltip(Some(tooltip));
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
        assert_eq!(f.tooltip, IDLE_TOOLTIP);
    }

    #[test]
    fn healthy_snapshot_shows_hours_in_the_state_color() {
        let f = face(Some(&snapshot(now())), None, None, false, now(), stale());
        assert_eq!(f.label, "103");
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
