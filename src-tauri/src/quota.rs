use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::api::subscriptions::Subscription;

/// 系統匣與面板的顯示狀態。
///
/// `OverPace` 不由 `from_subscription` 產生 —— 它取決於 `now` 與不可遊玩時段
/// 設定，由 `pace::display_state()` 併進來。這個欄位永遠是「還沒併入配速」的
/// 基礎狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayState {
    Normal,
    Low,
    OverPace,
    Exhausted,
    FreeTier,
}

/// NVIDIA 官方規定：未用完時數最多結轉 15 小時，無例外。
///
/// 此值不在 API 回應中。放在這裡而不是 `pace`：它是額度政策，面板要顯示
/// 它、`pace` 要拿它算浪費，而 `pace` 本來就相依於 `quota`，反過來不成立。
/// spec §12 要求它只有一份。
pub const ROLLOVER_CAP_MINUTES: u32 = 900;

/// 傳給前端與系統匣的顯示模型。所有時間單位為分鐘。
///
/// 本期起訖可能缺席：沒有時數上限的方案沒有「本期」可言。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSnapshot {
    pub fetched_at: DateTime<Utc>,
    pub tier: String,
    pub time_capped: bool,
    pub total_minutes: u32,
    pub remaining_minutes: u32,
    pub used_minutes: u32,
    pub rolled_over_minutes: u32,
    /// 結轉上限。常數，跟著快照下去是為了讓面板不必自己再寫一份。
    pub rollover_cap_minutes: u32,
    pub purchased_minutes: u32,
    pub span_start: Option<DateTime<Utc>>,
    pub span_end: Option<DateTime<Utc>>,
    pub game_play_allowed: bool,
    pub low_threshold_minutes: u32,
    pub state: DisplayState,
}

impl QuotaSnapshot {
    pub fn from_subscription(sub: &Subscription, fetched_at: DateTime<Utc>) -> Self {
        let time_capped = sub.sub_type == "TIME_CAPPED";
        let remaining = sub.remaining_time_in_minutes;
        let low_threshold = sub.notifications.notify_user_when_time_remaining_in_minutes;

        // 優先順序：免費方案 -> 已用完 -> 低量 -> 正常。
        let state = if !time_capped {
            DisplayState::FreeTier
        } else if !sub.current_subscription_state.is_game_play_allowed {
            DisplayState::Exhausted
        } else if remaining <= low_threshold {
            DisplayState::Low
        } else {
            DisplayState::Normal
        };

        Self {
            fetched_at,
            tier: sub.membership_tier.clone(),
            time_capped,
            total_minutes: sub.total_time_in_minutes,
            remaining_minutes: remaining,
            used_minutes: sub.total_time_in_minutes.saturating_sub(remaining),
            rolled_over_minutes: sub.rolled_over_time_in_minutes,
            rollover_cap_minutes: ROLLOVER_CAP_MINUTES,
            purchased_minutes: sub.purchased_time_in_minutes,
            span_start: sub.current_span_start_date_time,
            span_end: sub.current_span_end_date_time,
            game_play_allowed: sub.current_subscription_state.is_game_play_allowed,
            low_threshold_minutes: low_threshold,
            state,
        }
    }

    /// 系統匣要顯示的文字。免費方案沒有配額可言，顯示破折號。
    pub fn tray_label(&self) -> String {
        if !self.time_capped {
            return "\u{2013}".to_string();
        }
        (self.remaining_minutes / 60).to_string()
    }
}

/// 給人看的長度。不用小數點的小時 ——「2.5 小時」要讀的人自己在心裡乘六十，
/// 有餘數就直接講成分鐘。前端的 `formatDuration` 是同一條規則。
pub fn human_duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} 分鐘"),
        (h, 0) => format!("{h} 小時"),
        (h, m) => format!("{h} 小時 {m} 分鐘"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::api::subscriptions::Subscription;

    const FIXTURE: &str = include_str!("../tests/fixtures/subscription.json");

    fn fixture() -> Subscription {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn computes_used_minutes() {
        let snap = QuotaSnapshot::from_subscription(&fixture(), at(1789817022));
        assert_eq!(snap.used_minutes, 720);
        assert_eq!(snap.remaining_minutes, 6180);
        assert_eq!(snap.total_minutes, 6900);
    }

    #[test]
    fn normal_state_when_plenty_remaining() {
        let snap = QuotaSnapshot::from_subscription(&fixture(), at(1789817022));
        assert_eq!(snap.state, DisplayState::Normal);
    }

    #[test]
    fn low_state_at_official_threshold() {
        let mut sub = fixture();
        sub.remaining_time_in_minutes = 300;
        let snap = QuotaSnapshot::from_subscription(&sub, at(1789817022));
        assert_eq!(snap.state, DisplayState::Low);
    }

    #[test]
    fn exhausted_state_wins_over_low() {
        let mut sub = fixture();
        sub.remaining_time_in_minutes = 0;
        sub.current_subscription_state.is_game_play_allowed = false;
        let snap = QuotaSnapshot::from_subscription(&sub, at(1789817022));
        assert_eq!(snap.state, DisplayState::Exhausted);
    }

    #[test]
    fn free_tier_is_not_time_capped() {
        let mut sub = fixture();
        sub.sub_type = "FREE".into();
        let snap = QuotaSnapshot::from_subscription(&sub, at(1789817022));
        assert_eq!(snap.state, DisplayState::FreeTier);
        assert!(!snap.time_capped);
    }

    /// 免費方案的回應可能連本期起訖都沒有，模型要能表達「沒有本期」。
    #[test]
    fn free_tier_without_quota_fields_has_no_span() {
        let sub: Subscription =
            serde_json::from_str(r#"{"membershipTier":"FREE","subType":"FREE"}"#).unwrap();
        let snap = QuotaSnapshot::from_subscription(&sub, at(1789817022));
        assert_eq!(snap.state, DisplayState::FreeTier);
        assert!(snap.span_end.is_none());
        assert_eq!(snap.tray_label(), "–");
    }

    /// spec §12：上限只有一份。面板要顯示它，所以跟著快照下去，
    /// 而不是讓前端再寫一個 900。
    #[test]
    fn the_snapshot_carries_the_rollover_cap() {
        let snap = QuotaSnapshot::from_subscription(&fixture(), at(1789817022));
        assert_eq!(snap.rollover_cap_minutes, ROLLOVER_CAP_MINUTES);
        // fixture 剛好撞到上限，面板上會是「15 小時 / 15 小時」。
        assert_eq!(snap.rolled_over_minutes, 900);
    }

    #[test]
    fn human_duration_drops_the_decimal_point() {
        assert_eq!(human_duration(150), "2 小時 30 分鐘");
        assert_eq!(human_duration(120), "2 小時");
        assert_eq!(human_duration(45), "45 分鐘");
        assert_eq!(human_duration(0), "0 分鐘");
    }

    #[test]
    fn tray_label_is_whole_hours() {
        let snap = QuotaSnapshot::from_subscription(&fixture(), at(1789817022));
        assert_eq!(snap.tray_label(), "103");
    }

    #[test]
    fn tray_label_for_free_tier_has_no_number() {
        let mut sub = fixture();
        sub.sub_type = "FREE".into();
        let snap = QuotaSnapshot::from_subscription(&sub, at(1789817022));
        assert_eq!(snap.tray_label(), "–");
    }
}
