//! widget 上要畫什麼。純資料，由 `face()` 算出，`render` 畫出來。

use chrono::{DateTime, Duration, Utc};

use crate::pace::PaceReport;
use crate::quota::{human_duration, DisplayState, QuotaSnapshot};
use crate::store::Metric;

/// 字與進度條的色調。實際顏色由 `render::color` 依工作列深淺決定。
///
/// 不在這裡決定顏色：使用者切換淺色／深色時，host 執行緒要自己重畫，
/// 那時手上只有上一份 `WidgetFace`。顏色放這裡的話要等下一次 `tray::sync`
/// 才會跟著變，最久五分鐘（`PACE_INTERVAL`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Low,
    /// 超前、用完、需要登入。跟系統匣一樣，超前與用完同為紅色（spec §7.2）。
    Alert,
    /// 沒資料、沒有時數上限的方案。
    Muted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetFace {
    pub text: String,
    pub tone: Tone,
    /// 進度條填滿的比例，0 到 1。`None` 是不畫進度條。
    pub fill: Option<f32>,
    /// 資料過期。畫的時候整個變淡。
    pub stale: bool,
}

fn tone(state: DisplayState) -> Tone {
    match state {
        DisplayState::Normal => Tone::Normal,
        DisplayState::Low => Tone::Low,
        DisplayState::OverPace | DisplayState::Exhausted => Tone::Alert,
        DisplayState::FreeTier => Tone::Muted,
    }
}

pub fn face(
    snapshot: Option<&QuotaSnapshot>,
    pace: Option<&PaceReport>,
    needs_login: bool,
    metric: Metric,
    now: DateTime<Utc>,
    stale_after: Duration,
) -> WidgetFace {
    let muted = WidgetFace {
        text: "\u{2013}".into(),
        tone: Tone::Muted,
        fill: None,
        stale: false,
    };

    // 數字不可信，而且要使用者動手，所以不畫數字、用紅色。
    // 系統匣這時是灰色驚嘆號，widget 比它醒目，2026-09-23 使用者選的。
    if needs_login {
        return WidgetFace {
            text: "!".into(),
            tone: Tone::Alert,
            fill: None,
            stale: false,
        };
    }
    let Some(snapshot) = snapshot else {
        return muted;
    };
    if !snapshot.time_capped {
        return muted;
    }

    let state = crate::pace::display_state(snapshot, pace);
    let value = match metric {
        Metric::Remaining => snapshot.remaining_minutes,
        Metric::Used => snapshot.used_minutes,
    };
    // 分母跟面板的進度條一樣是 `total_minutes`（`App.tsx` 的 `percentOf`）。
    let fill = if snapshot.total_minutes == 0 {
        0.0
    } else {
        (value as f32 / snapshot.total_minutes as f32).clamp(0.0, 1.0)
    };

    WidgetFace {
        // 已用完時不顯示看起來很正常的 0（spec §7.2），跟系統匣同一條。
        text: if state == DisplayState::Exhausted {
            "!".into()
        } else {
            human_duration(value)
        },
        tone: tone(state),
        fill: Some(fill),
        stale: now - snapshot.fetched_at > stale_after,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::api::subscriptions::Subscription;

    fn fixture() -> Subscription {
        serde_json::from_str(include_str!("../../tests/fixtures/subscription.json")).unwrap()
    }

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    const FETCHED: i64 = 1789817022;

    fn snapshot() -> QuotaSnapshot {
        QuotaSnapshot::from_subscription(&fixture(), at(FETCHED))
    }

    fn fresh(snap: Option<&QuotaSnapshot>, metric: Metric) -> WidgetFace {
        face(
            snap,
            None,
            false,
            metric,
            at(FETCHED),
            Duration::minutes(60),
        )
    }

    #[test]
    fn remaining_shows_the_remaining_time_and_its_share() {
        let snap = snapshot();
        let f = fresh(Some(&snap), Metric::Remaining);
        assert_eq!(f.text, human_duration(snap.remaining_minutes));
        let want = snap.remaining_minutes as f32 / snap.total_minutes as f32;
        assert!((f.fill.unwrap() - want).abs() < 1e-6);
        assert_eq!(f.tone, Tone::Normal);
        assert!(!f.stale);
    }

    #[test]
    fn used_shows_the_used_time_and_its_share() {
        let snap = snapshot();
        let f = fresh(Some(&snap), Metric::Used);
        assert_eq!(f.text, human_duration(snap.used_minutes));
        let want = snap.used_minutes as f32 / snap.total_minutes as f32;
        assert!((f.fill.unwrap() - want).abs() < 1e-6);
    }

    #[test]
    fn no_data_is_a_muted_dash_without_a_bar() {
        let f = fresh(None, Metric::Remaining);
        assert_eq!(f.text, "\u{2013}");
        assert_eq!(f.tone, Tone::Muted);
        assert_eq!(f.fill, None);
    }

    #[test]
    fn needing_login_is_a_red_exclamation_mark() {
        let snap = snapshot();
        let f = face(
            Some(&snap),
            None,
            true,
            Metric::Remaining,
            at(FETCHED),
            Duration::minutes(60),
        );
        assert_eq!(f.text, "!");
        assert_eq!(f.tone, Tone::Alert);
        assert_eq!(f.fill, None);
    }

    #[test]
    fn a_plan_without_a_cap_has_no_number() {
        let mut sub = fixture();
        sub.sub_type = "FREE".into();
        let snap = QuotaSnapshot::from_subscription(&sub, at(FETCHED));
        let f = fresh(Some(&snap), Metric::Remaining);
        assert_eq!(f.text, "\u{2013}");
        assert_eq!(f.tone, Tone::Muted);
        assert_eq!(f.fill, None);
    }

    /// 跟系統匣一樣：用完時不顯示看起來很正常的 0（spec §7.2）。
    #[test]
    fn exhausted_is_a_red_exclamation_mark() {
        let mut sub = fixture();
        sub.current_subscription_state.is_game_play_allowed = false;
        let snap = QuotaSnapshot::from_subscription(&sub, at(FETCHED));
        let f = fresh(Some(&snap), Metric::Remaining);
        assert_eq!(f.text, "!");
        assert_eq!(f.tone, Tone::Alert);
        assert!(f.fill.is_some());
    }

    #[test]
    fn low_uses_the_low_tone() {
        let mut sub = fixture();
        sub.remaining_time_in_minutes =
            sub.notifications.notify_user_when_time_remaining_in_minutes;
        let snap = QuotaSnapshot::from_subscription(&sub, at(FETCHED));
        let f = fresh(Some(&snap), Metric::Remaining);
        assert_eq!(f.tone, Tone::Low);
    }

    #[test]
    fn old_data_is_marked_stale() {
        let snap = snapshot();
        let f = face(
            Some(&snap),
            None,
            false,
            Metric::Remaining,
            at(FETCHED) + Duration::minutes(61),
            Duration::minutes(60),
        );
        assert!(f.stale);
        assert_eq!(f.text, human_duration(snap.remaining_minutes));
    }

    /// 總額是 0 時不除以零，也不畫出超過 1 的進度條。
    #[test]
    fn a_zero_total_does_not_divide_by_zero() {
        let mut snap = snapshot();
        snap.total_minutes = 0;
        let f = fresh(Some(&snap), Metric::Remaining);
        let fill = f.fill.unwrap();
        assert!((0.0..=1.0).contains(&fill), "{fill}");
    }
}
