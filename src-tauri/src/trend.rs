//! 本期的逐日累計用量，給主面板那張折線圖。
//!
//! 放在頂層而不是 `pace/` 底下，是因為它同時要用 `api::playtime` 的逐場紀錄
//! 與 `pace::avail` 的可遊玩時間。放進 `pace` 會讓 `pace` 依賴 `api`。

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use crate::api::playtime::{minutes_between, PlaySession};
use crate::pace::avail::{avail, local_midnight};
use crate::pace::schedule::Schedule;
use crate::quota::QuotaSnapshot;

/// 點數上限。一個計費期是一個月，正常是 28 到 32 點。
///
/// 上限只是防呆：`span_end` 要是哪天變成很遠的未來，沒有上限就會把幾千點
/// 搬過 IPC，而面板只有 360 點寬。
const MAX_POINTS: usize = 400;

/// 本期某一個當地日期的累計已使用量。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyPoint {
    pub date: NaiveDate,
    /// 已經過完的那些天是那一天結束時的累計。今天的值是「現在」的累計，
    /// 不是今天結束時的 —— 今天還沒過完。未來的日子沒有值。
    pub used_minutes: Option<f64>,
    /// 用真實燃燒率往後推的累計。今天與未來才有值，而今天的值與
    /// `used_minutes` 相同，實線與虛線因此在今天接得起來，不會出現折角。
    pub projected_used_minutes: Option<f64>,
}

/// 由逐場紀錄與快照組出本期的逐日累計。
///
/// `rate` 傳 `PaceReport::burn_rate`。它是 `None`（樣本還不夠，`compute` 沒
/// 給預測）時只吐已經過完的那一段，虛線不畫。
///
/// `now` 與 `tz` 是參數，跟 `pace` 一樣不碰系統時鐘。
pub fn build(
    sessions: &[PlaySession],
    snapshot: &QuotaSnapshot,
    rate: Option<f64>,
    schedule: &Schedule,
    now: DateTime<Utc>,
    tz: Tz,
) -> Vec<DailyPoint> {
    let (Some(span_start), Some(span_end)) = (snapshot.span_start, snapshot.span_end) else {
        return Vec::new();
    };
    if !snapshot.time_capped || now >= span_end {
        return Vec::new();
    }

    // 錨點是快照的 U，每一天往回推，不是從本期起點往前加。
    //
    // 往前加的話最後一點會是場次加總，而那個不保證等於 T − R：還在玩的那一場
    // 沒計費、`totalPlaytime` 有進位、跨在本期起點上的那筆被 `started_at >=
    // span_start` 濾掉了。圖的末端因此會對不上進度條，實線的末端與虛線的起點
    // 也會在今天差一截。往回推的話末端等於進度條是結構上成立的。
    let used_now = snapshot.used_minutes as f64;
    let today = now.with_timezone(&tz).date_naive();

    // 退一秒再取日期：`span_end` 剛好落在當地午夜時，那一天一分鐘都不屬於
    // 本期，不該有一個點。
    let last = (span_end - Duration::seconds(1))
        .with_timezone(&tz)
        .date_naive();

    let mut points = Vec::new();
    let mut date = span_start.with_timezone(&tz).date_naive();
    while date <= last && points.len() < MAX_POINTS {
        let boundary = local_midnight(date + Duration::days(1), tz)
            .unwrap_or(span_end)
            .min(span_end);

        let point = if boundary <= now {
            DailyPoint {
                date,
                // 進位有可能讓回推的量略大於 U，夾住才不會讓線掉到軸下面。
                used_minutes: Some(
                    (used_now - minutes_between(sessions, boundary, now) as f64).max(0.0),
                ),
                projected_used_minutes: None,
            }
        } else if date <= today {
            DailyPoint {
                date,
                used_minutes: Some(used_now),
                projected_used_minutes: rate.is_some().then_some(used_now),
            }
        } else {
            DailyPoint {
                date,
                used_minutes: None,
                projected_used_minutes: rate
                    .map(|rate| used_now + rate * avail(now, boundary, schedule, tz) as f64),
            }
        };
        points.push(point);
        date += Duration::days(1);
    }

    points
}

#[cfg(test)]
mod tests {
    use chrono_tz::{Asia::Taipei, UTC};

    use super::*;
    use crate::api::subscriptions::Subscription;

    const FIXTURE: &str = include_str!("../tests/fixtures/subscription.json");

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    /// 本期 2026-09-01 → 10-01，與 `pace` 的測試同一組數字。
    fn snapshot(total: u32, remaining: u32) -> QuotaSnapshot {
        let mut sub: Subscription = serde_json::from_str(FIXTURE).unwrap();
        sub.total_time_in_minutes = total;
        sub.remaining_time_in_minutes = remaining;
        sub.current_span_start_date_time = Some(at("2026-09-01T00:00:00Z"));
        sub.current_span_end_date_time = Some(at("2026-10-01T00:00:00Z"));
        QuotaSnapshot::from_subscription(&sub, at("2026-09-11T00:00:00Z"))
    }

    fn session(start: &str, end: &str, minutes: f64) -> PlaySession {
        PlaySession {
            game_title: "Wuthering Waves".into(),
            started_at: at(start),
            ended_at: Some(at(end)),
            minutes,
        }
    }

    fn on(points: &[DailyPoint], date: &str) -> DailyPoint {
        let date: NaiveDate = date.parse().unwrap();
        points
            .iter()
            .find(|point| point.date == date)
            .unwrap_or_else(|| panic!("{date} 應該有一個點"))
            .clone()
    }

    fn used(points: &[DailyPoint], date: &str) -> f64 {
        on(points, date).used_minutes.expect("這一天應該有實線的值")
    }

    /// 兩場共 900 分鐘，而 U 是 1500：API 少報的那 600 分鐘不會讓線走偏，
    /// 因為錨點是 U。
    fn two_sessions() -> Vec<PlaySession> {
        vec![
            session("2026-09-05T10:00:00Z", "2026-09-05T20:00:00Z", 600.0),
            session("2026-09-09T10:00:00Z", "2026-09-09T15:00:00Z", 300.0),
        ]
    }

    fn built(sessions: &[PlaySession], rate: Option<f64>, now: &str) -> Vec<DailyPoint> {
        build(
            sessions,
            &snapshot(6000, 4500),
            rate,
            &Schedule::default(),
            at(now),
            UTC,
        )
    }

    /// 這是整張圖的錨點。逐場紀錄加總與 U 對不上時，末端要站在 U 上，
    /// 不是站在加總上 —— 否則圖的末端與進度條會指到不同的位置。
    #[test]
    fn the_last_actual_point_is_the_snapshots_used_total() {
        let points = built(&two_sessions(), Some(0.1), "2026-09-11T00:00:00Z");
        let last_actual = points
            .iter()
            .rfind(|point| point.used_minutes.is_some())
            .unwrap();
        assert_eq!(last_actual.used_minutes, Some(1500.0));
    }

    /// 1500 − 後面那 900 = 600，第一天結束時的累計。
    #[test]
    fn earlier_days_are_reconstructed_backwards_from_the_anchor() {
        let points = built(&two_sessions(), Some(0.1), "2026-09-11T00:00:00Z");
        assert_eq!(used(&points, "2026-09-01"), 600.0);
        assert_eq!(used(&points, "2026-09-05"), 1200.0);
        assert_eq!(used(&points, "2026-09-09"), 1500.0);
    }

    /// 沒玩的那幾天是平的，不是缺口。
    #[test]
    fn a_day_without_sessions_repeats_the_previous_total() {
        let points = built(&two_sessions(), Some(0.1), "2026-09-11T00:00:00Z");
        assert_eq!(used(&points, "2026-09-06"), 1200.0);
        assert_eq!(used(&points, "2026-09-07"), 1200.0);
        assert_eq!(used(&points, "2026-09-08"), 1200.0);
    }

    /// 實線與虛線在今天共用同一個值，接起來不會有折角。
    #[test]
    fn today_carries_both_lines_at_the_same_value() {
        let points = built(&two_sessions(), Some(0.1), "2026-09-11T00:00:00Z");
        let today = on(&points, "2026-09-11");
        assert_eq!(today.used_minutes, Some(1500.0));
        assert_eq!(today.projected_used_minutes, Some(1500.0));
    }

    /// 虛線每過一天長 rate × 1440（空排程下一天全可玩）。
    ///
    /// 絕對值那條是 2880 而不是 1440：now 是 09-11 00:00，而 09-12 那個點是
    /// 09-12 結束時，中間隔了兩整天。
    #[test]
    fn the_projection_grows_by_the_rate_times_playable_time() {
        let points = built(&two_sessions(), Some(0.1), "2026-09-11T00:00:00Z");
        let tomorrow = on(&points, "2026-09-12");
        assert_eq!(tomorrow.used_minutes, None);
        assert_eq!(tomorrow.projected_used_minutes, Some(1500.0 + 0.1 * 2880.0));

        let day_after = on(&points, "2026-09-13").projected_used_minutes.unwrap();
        assert_eq!(
            day_after - tomorrow.projected_used_minutes.unwrap(),
            0.1 * 1440.0
        );
    }

    /// 樣本不夠、`compute` 沒給預測時不畫虛線，與「配速不足就不顯示預測」
    /// 是同一個判斷。
    #[test]
    fn no_rate_means_no_projected_points() {
        let points = built(&two_sessions(), None, "2026-09-11T00:00:00Z");
        assert!(points
            .iter()
            .all(|point| point.projected_used_minutes.is_none()));
    }

    /// 本期第一天只有一個實線的點，就是現在。
    #[test]
    fn the_first_day_of_a_period_has_a_single_actual_point() {
        let points = built(&[], Some(0.1), "2026-09-01T12:00:00Z");
        let actual: Vec<_> = points
            .iter()
            .filter(|point| point.used_minutes.is_some())
            .collect();
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].date, "2026-09-01".parse::<NaiveDate>().unwrap());
    }

    /// `span_end` 落在當地午夜時，那一天一分鐘都不屬於本期，不給它一個點。
    #[test]
    fn the_series_ends_on_the_last_day_the_period_actually_covers() {
        let points = built(&two_sessions(), Some(0.1), "2026-09-11T00:00:00Z");
        assert_eq!(points.len(), 30);
        assert_eq!(points[0].date, "2026-09-01".parse::<NaiveDate>().unwrap());
        assert_eq!(points[29].date, "2026-09-30".parse::<NaiveDate>().unwrap());
    }

    /// 日界看當地時區。台灣時間 9/6 23:00 玩到 9/7 01:00，114 分鐘裡只有 57
    /// 分鐘算在 9/6 —— 與 `used_today` 同一條規則。
    #[test]
    fn a_session_across_local_midnight_is_split_by_the_local_day() {
        let crossing = vec![session(
            "2026-09-06T15:00:00Z",
            "2026-09-06T17:00:00Z",
            114.0,
        )];
        let points = build(
            &crossing,
            &snapshot(6000, 5886),
            Some(0.1),
            &Schedule::default(),
            at("2026-09-11T00:00:00Z"),
            Taipei,
        );
        assert_eq!(used(&points, "2026-09-06"), 57.0);
        assert_eq!(used(&points, "2026-09-07"), 114.0);
    }

    /// 封鎖時段裡不長虛線。每天 00:00–08:00 睡覺，一天只長 rate × 960。
    #[test]
    fn the_projection_skips_blocked_windows() {
        let sleeping = Schedule {
            weekly: vec![crate::pace::schedule::WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4, 5, 6],
                start_minute: 0,
                end_minute: 480,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        let points = build(
            &two_sessions(),
            &snapshot(6000, 4500),
            Some(0.1),
            &sleeping,
            at("2026-09-11T00:00:00Z"),
            UTC,
        );
        // 兩整天扣掉兩段 480 分鐘的睡覺時間。
        assert_eq!(
            on(&points, "2026-09-12").projected_used_minutes,
            Some(1500.0 + 0.1 * 1920.0)
        );
        let day_after = on(&points, "2026-09-13").projected_used_minutes.unwrap();
        assert_eq!(
            day_after - on(&points, "2026-09-12").projected_used_minutes.unwrap(),
            0.1 * 960.0
        );
    }

    #[test]
    fn a_free_tier_has_no_trend() {
        let sub: Subscription = serde_json::from_str(
            r#"{"membershipTier":"FREE","subType":"FREE","currentSubscriptionState":{"state":"ACTIVE","isGamePlayAllowed":true},"notifications":{"notifyUserWhenTimeRemainingInMinutes":0,"notifyUserOnSessionWhenRemainingTimeInMinutes":0}}"#,
        )
        .unwrap();
        let snap = QuotaSnapshot::from_subscription(&sub, at("2026-09-11T00:00:00Z"));
        assert!(build(
            &[],
            &snap,
            Some(0.1),
            &Schedule::default(),
            at("2026-09-11T00:00:00Z"),
            UTC
        )
        .is_empty());
    }

    /// 本期已經結束的快照不畫圖，等下一次抓取換到新的一期。
    #[test]
    fn an_expired_span_has_no_trend() {
        assert!(built(&two_sessions(), Some(0.1), "2026-10-02T00:00:00Z").is_empty());
    }
}
