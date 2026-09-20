use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::api::subscriptions::Subscription;

/// 系統匣與面板的顯示狀態。里程碑 2 會加入 `OverPace`（超前消耗）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayState {
    Normal,
    Low,
    Exhausted,
    FreeTier,
}

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
