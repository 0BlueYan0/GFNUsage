pub mod avail;
pub mod schedule;

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use crate::quota::{DisplayState, QuotaSnapshot, ROLLOVER_CAP_MINUTES};
use avail::{advance, avail, local_midnight};
use schedule::Schedule;

/// 累積可遊玩時間未達此值前不顯示預測，避免樣本過少導致的失真外推。
pub const MIN_AVAIL_FOR_PROJECTION_MINUTES: u32 = 720;

/// 機器目前的 IANA 時區。
///
/// 取不到或名稱不認得時退回 UTC：時段設定會失準，但不至於讓整個計算崩掉。
/// `chrono::Local` 只給得到位移、給不到時區名稱，而日光節約時間的判斷需要名稱。
pub fn machine_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse::<Tz>().ok())
        .unwrap_or(Tz::UTC)
}

/// 沒有預測時的原因，決定面板顯示哪一句話（spec §6.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PaceNote {
    /// 本期剩下的時間全是不可遊玩時段。
    NoTimeLeft,
    /// 本期到現在為止沒有任何可遊玩時間，配速與遊玩率都算不出來。
    Insufficient,
    /// 可遊玩時間還不到 12 小時，外推會失真。
    Collecting,
}

/// 最近一段區間實際玩掉的分鐘數，由呼叫端從逐場紀錄算出來。
///
/// 收這個而不是直接收 `&[PlaySession]`，是為了不讓 `pace` 依賴 `api`。
/// `since` 必須已經 clamp 過本期起點：逐場紀錄拿不到本期之前的場次，
/// 分子少了那一段、分母卻照算七天，算出來的遊玩率會偏低。
#[derive(Debug, Clone, Copy)]
pub struct RecentPlay {
    pub since: DateTime<Utc>,
    pub minutes: f64,
}

/// 配速與預測。所有時間單位為分鐘，比例運算用 `f64`
/// —— `T − projected_used` 在超支時是負數。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaceReport {
    /// `A_past`：本期開始到現在的可遊玩分鐘數。
    pub avail_past_minutes: u32,
    /// `A_left`：現在到本期結束的可遊玩分鐘數。
    pub avail_left_minutes: u32,
    /// 到現在為止「該用掉」多少：`T × A_past / A_total`。
    pub expected_used_minutes: Option<f64>,
    /// `U − expected_used`。大於 0 代表超前消耗，UI 轉紅。
    pub over_pace_minutes: Option<f64>,
    /// 預測實際採用的遊玩率：`recent` 那段區間裡玩掉的分鐘數除以同一段的
    /// 可遊玩分鐘數。`recent` 是 `None`，或那段區間整個落在封鎖時段裡，
    /// 就退回整期的 `U / A_past`。只有算得出預測時才有值。
    pub burn_rate: Option<f64>,
    /// 以目前速度推估的期末總用量：`U + r × A_left`。
    pub projected_used_minutes: Option<f64>,
    /// `projected_used − T`。大於 0 才是超支。
    pub overshoot_minutes: Option<f64>,
    /// 預估用完的時點。不會用完時為 `None`。
    pub runs_out_at: Option<DateTime<Utc>>,
    /// 預估期末剩餘中超過 15 小時結轉上限、會直接作廢的部分。
    pub wasted_minutes: Option<f64>,
    /// 今天還能玩多久。
    pub today_budget_minutes: Option<f64>,
    /// 缺了哪些推算，以及為什麼。
    pub note: Option<PaceNote>,
}

/// 今天本地時間結束的時點，也就是隔天的午夜。
///
/// spec 寫的是 `avail(now, 今日 23:59:59)`，用隔天午夜只差一秒，
/// 而且不必處理日光節約時間造成的「今天沒有 23:59」這種情況。
fn end_of_local_day(now: DateTime<Utc>, tz: Tz) -> DateTime<Utc> {
    let tomorrow = now.with_timezone(&tz).date_naive() + Duration::days(1);
    local_midnight(tomorrow, tz).unwrap_or(now)
}

/// 由快照與設定算出配速與預測（spec §6）。
///
/// `now` 與 `tz` 都是參數：這個模組不碰系統時鐘，測試才能固定時點。
/// `used_today`（今天已經用掉的分鐘數）與 `recent`（最近一段區間玩掉多少）
/// 同理 —— 讀歷史檔與逐場紀錄是呼叫端的事。
/// 免費方案（沒有本期）或本期已結束時回傳 `None` —— 沒有配速可言。
pub fn compute(
    snapshot: &QuotaSnapshot,
    schedule: &Schedule,
    now: DateTime<Utc>,
    tz: Tz,
    used_today: Option<u32>,
    recent: Option<RecentPlay>,
) -> Option<PaceReport> {
    if !snapshot.time_capped {
        return None;
    }
    let (span_start, span_end) = (snapshot.span_start?, snapshot.span_end?);
    if now >= span_end {
        return None;
    }

    let total = snapshot.total_minutes as f64;
    let used = snapshot.used_minutes as f64;
    let remaining = snapshot.remaining_minutes as f64;

    let past = avail(span_start, now, schedule, tz);
    let left = avail(now, span_end, schedule, tz);
    let total_avail = past + left;

    let mut report = PaceReport {
        avail_past_minutes: past,
        avail_left_minutes: left,
        expected_used_minutes: None,
        over_pace_minutes: None,
        burn_rate: None,
        projected_used_minutes: None,
        overshoot_minutes: None,
        runs_out_at: None,
        wasted_minutes: None,
        today_budget_minutes: None,
        note: None,
    };

    // 配速門檻不需要外推，有任何可遊玩時間就算得出來。
    if past > 0 && total_avail > 0 {
        let expected = total * past as f64 / total_avail as f64;
        report.expected_used_minutes = Some(expected);
        report.over_pace_minutes = Some(used - expected);
    }

    if left == 0 {
        report.note = Some(PaceNote::NoTimeLeft);
        return Some(report);
    }

    // 今日額度只用到 R 與 A_left，不做任何外推，所以不受樣本下限限制 ——
    // 本期第一天就是有效的（spec §6.5）。`sustainable` 可能大於 1
    // （額度多到用不完），所以要用今天實際能玩的時間與剩餘額度封頂。
    //
    // 與 spec §6.4 的公式不同，這是刻意的。那條把份額算在「現在到今天結束」
    // 上，於是同一天從早看到晚，沒玩也會愈看愈少：早上沒用掉的份額直接蒸發。
    // 這裡份額算一整天的，再扣掉今天已經用掉的，數字只因為你玩而變少。
    let day_end = end_of_local_day(now, tz).min(span_end);
    let rest_of_today = avail(now, day_end, schedule, tz) as f64;
    let sustainable = remaining / left as f64;

    // 答不出今天用了多少（沒有歷史、機器午夜前後關著）就當成沒玩。
    // 不為此改用另一套算法：那會讓同一個欄位在不同日子有兩種行為，而多給
    // 的那點份額本來就不是上限，超前與超支由旁邊兩列如實報告。
    let used_today = used_today.unwrap_or(0) as f64;
    let day_start = local_midnight(now.with_timezone(&tz).date_naive(), tz)
        .unwrap_or(now)
        .max(span_start);
    let whole_day = avail(day_start, day_end, schedule, tz) as f64;

    report.today_budget_minutes = Some(
        (sustainable * whole_day - used_today)
            .max(0.0)
            // 份額比今天剩下的還多時以剩下的為準 —— 過了午夜就是明天的份額了。
            .min(rest_of_today)
            .min(remaining),
    );

    if past == 0 {
        report.note = Some(PaceNote::Insufficient);
        return Some(report);
    }
    if past < MIN_AVAIL_FOR_PROJECTION_MINUTES {
        report.note = Some(PaceNote::Collecting);
        return Some(report);
    }

    // 預測採用最近一段區間的實際遊玩率，不是整期平均。整期的場次加總等於 U
    // （spike §3a 實測），所以拿整期的逐場紀錄來算，結果就是下面那條退路，
    // 一點差別都沒有。要反映「最近開始每天都玩」只能靠窗口。
    //
    // 分母站不住（那段區間整個落在封鎖時段裡）或這次沒抓到逐場紀錄，就退回
    // 整期平均。期初窗口起點被 clamp 到本期起點時兩者相等，所以期初不必另
    // 設門檻，`Collecting` 那條已經在管樣本太少。
    let windowed = recent.and_then(|recent| {
        let window = avail(recent.since, now, schedule, tz);
        if window > 0 {
            Some(recent.minutes / window as f64)
        } else {
            None
        }
    });
    let rate = windowed.unwrap_or(used / past as f64);
    report.burn_rate = Some(rate);

    let projected = used + rate * left as f64;
    report.projected_used_minutes = Some(projected);
    report.overshoot_minutes = Some(projected - total);

    // 上限套用的對象是期末剩餘量，不是本期總額度，也不是累計結轉量。
    let leftover = total - projected;
    let carried = leftover.min(ROLLOVER_CAP_MINUTES as f64);
    report.wasted_minutes = Some((leftover - carried).max(0.0));

    if rate > 0.0 {
        report.runs_out_at = advance(now, remaining / rate, span_end, schedule, tz);
    }

    Some(report)
}

/// 併入配速後的最終顯示狀態。
///
/// 「已用完」與「官方低量」優先於「超前消耗」（spec §7.2），所以只有基礎狀態
/// 是「正常」時才可能變成 `OverPace`。系統匣與面板都讀這個函式的結果，
/// 不各算一次 —— 狀態不再是抓取當下就定案的，它會隨時間與設定改變。
pub fn display_state(snapshot: &QuotaSnapshot, pace: Option<&PaceReport>) -> DisplayState {
    if snapshot.state != DisplayState::Normal {
        return snapshot.state;
    }
    match pace.and_then(|report| report.over_pace_minutes) {
        Some(over) if over > 0.0 => DisplayState::OverPace,
        _ => DisplayState::Normal,
    }
}

#[cfg(test)]
mod tests {
    use chrono_tz::UTC;

    use super::*;
    use crate::api::subscriptions::Subscription;

    const FIXTURE: &str = include_str!("../../tests/fixtures/subscription.json");

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    /// 本期是 2026-09-01 → 10-01 整整 30 天（43200 分鐘），
    /// 測試的 now 一律取 09-11 00:00Z，剛好 A_past = 14400、A_left = 28800。
    /// 數字取得這麼整齊是刻意的：期望值要能手算驗證。
    fn snapshot(total: u32, remaining: u32) -> QuotaSnapshot {
        let mut sub: Subscription = serde_json::from_str(FIXTURE).unwrap();
        sub.total_time_in_minutes = total;
        sub.remaining_time_in_minutes = remaining;
        sub.current_span_start_date_time = Some(at("2026-09-01T00:00:00Z"));
        sub.current_span_end_date_time = Some(at("2026-10-01T00:00:00Z"));
        QuotaSnapshot::from_subscription(&sub, at("2026-09-11T00:00:00Z"))
    }

    fn now() -> DateTime<Utc> {
        at("2026-09-11T00:00:00Z")
    }

    fn report(total: u32, remaining: u32) -> PaceReport {
        compute(
            &snapshot(total, remaining),
            &Schedule::default(),
            now(),
            UTC,
            None,
            None,
        )
        .expect("時數方案且本期未結束，應該算得出配速")
    }

    fn report_with(total: u32, remaining: u32, recent: RecentPlay) -> PaceReport {
        compute(
            &snapshot(total, remaining),
            &Schedule::default(),
            now(),
            UTC,
            None,
            Some(recent),
        )
        .expect("時數方案且本期未結束，應該算得出配速")
    }

    /// 最近七天的窗口。now 是 09-11 00:00Z，所以起點是 09-04 00:00Z，
    /// 空排程下分母剛好 10080 分鐘。
    fn last_week(minutes: f64) -> RecentPlay {
        RecentPlay {
            since: at("2026-09-04T00:00:00Z"),
            minutes,
        }
    }

    fn close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("這個欄位應該有值");
        assert!(
            (actual - expected).abs() < 0.5,
            "得到 {actual}，期望 {expected}"
        );
    }

    /// spec §6.3：expected_used = T × A_past / A_total = 6000 × 14400 / 43200
    #[test]
    fn expected_used_is_the_share_of_playable_time_already_passed() {
        let r = report(6000, 4500);
        assert_eq!(r.avail_past_minutes, 14400);
        assert_eq!(r.avail_left_minutes, 28800);
        close(r.expected_used_minutes, 2000.0);
    }

    #[test]
    fn under_pace_is_negative() {
        close(report(6000, 4500).over_pace_minutes, -500.0);
    }

    #[test]
    fn over_pace_is_positive() {
        close(report(6000, 3000).over_pace_minutes, 1000.0);
    }

    /// spec §6.4：projected = U + r × A_left = 1500 + 0.10417 × 28800
    #[test]
    fn projection_extrapolates_the_burn_rate() {
        let r = report(6000, 4500);
        close(r.projected_used_minutes, 4500.0);
        close(r.overshoot_minutes, -1500.0);
    }

    /// spec §6.4：期末剩 1500，只帶得走 900，其餘 600 作廢。
    #[test]
    fn leftover_above_the_rollover_cap_is_reported_as_wasted() {
        close(report(6000, 4500).wasted_minutes, 600.0);
    }

    #[test]
    fn nothing_is_wasted_when_the_projection_overshoots() {
        close(report(6000, 3000).wasted_minutes, 0.0);
    }

    /// 用量翻倍就會用完：R = 3000、r = 0.20833 → 還需要 14400 分鐘可遊玩時間。
    #[test]
    fn runs_out_at_is_reported_when_the_quota_will_not_last() {
        assert_eq!(
            report(6000, 3000).runs_out_at,
            Some(at("2026-09-21T00:00:00Z"))
        );
    }

    /// 窗口起點被 clamp 到本期起點時（期初前六天），窗口的分子分母就是整期的
    /// 分子分母，所以預測必須與整期平均一字不差。期初不另設門檻靠的是這一點。
    #[test]
    fn a_window_clamped_to_the_span_start_matches_the_whole_period() {
        let clamped = RecentPlay {
            since: at("2026-09-01T00:00:00Z"),
            minutes: 1500.0,
        };
        let r = report_with(6000, 4500, clamped);
        close(r.burn_rate, 1500.0 / 14400.0);
        close(r.projected_used_minutes, 4500.0);
        assert_eq!(r.runs_out_at, None);
    }

    /// 同一份快照，1500 分鐘全集中在最近七天：r 從 1500/14400 變成 1500/10080，
    /// 預測跟著從 4500 變成 1500 + 0.14881 × 28800。
    #[test]
    fn a_recent_burst_raises_the_projection() {
        let r = report_with(6000, 4500, last_week(1500.0));
        close(r.burn_rate, 1500.0 / 10080.0);
        close(r.projected_used_minutes, 5785.7);
    }

    /// 3000 分鐘全在最近七天，用完的日子從整期平均的 09-21 提早到 09-18：
    /// r = 3000/10080，R/r = 10080 分鐘，空排程下就是七天。
    #[test]
    fn a_recent_burst_brings_the_run_out_date_forward() {
        assert_eq!(
            report_with(6000, 3000, last_week(3000.0)).runs_out_at,
            Some(at("2026-09-18T00:00:00Z"))
        );
    }

    /// 最近七天完全沒玩，預測就停在現在的用量。整期平均會說 4500，
    /// 而它答的是 1500 —— 這就是換成窗口的用意。
    #[test]
    fn a_quiet_week_projects_no_further_use() {
        let r = report_with(6000, 4500, last_week(0.0));
        close(r.burn_rate, 0.0);
        close(r.projected_used_minutes, 1500.0);
        assert_eq!(r.runs_out_at, None);
        // 期末會剩 4500，只帶得走 900。
        close(r.wasted_minutes, 3600.0);
    }

    /// 窗口整段落在封鎖時段裡（分母為 0）就退回整期平均，不是回答零。
    #[test]
    fn a_fully_blocked_window_falls_back_to_the_whole_period() {
        let blocked = Schedule {
            weekly: Vec::new(),
            exceptions: vec![crate::pace::schedule::Exception {
                start_date: "2026-09-04".parse().unwrap(),
                end_date: "2026-09-10".parse().unwrap(),
                kind: crate::pace::schedule::ExceptionKind::Blocked,
                note: String::new(),
            }],
        };
        let snap = snapshot(6000, 4500);
        let with_window = compute(&snap, &blocked, now(), UTC, None, Some(last_week(0.0))).unwrap();
        let without = compute(&snap, &blocked, now(), UTC, None, None).unwrap();
        assert_eq!(with_window.avail_past_minutes, 4320);
        assert_eq!(
            with_window.projected_used_minutes,
            without.projected_used_minutes
        );
    }

    /// 配速那一列是均勻門檻，不是速度估計，所以不受窗口影響。
    #[test]
    fn the_pace_threshold_ignores_the_window() {
        let windowed = report_with(6000, 4500, last_week(1500.0));
        let whole = report(6000, 4500);
        assert_eq!(windowed.expected_used_minutes, whole.expected_used_minutes);
        assert_eq!(windowed.over_pace_minutes, whole.over_pace_minutes);
    }

    /// spec §6.5：r = 0 時「以目前速度不會用完」。
    #[test]
    fn a_zero_burn_rate_never_runs_out() {
        let r = report(6000, 6000);
        close(r.burn_rate, 0.0);
        assert_eq!(r.runs_out_at, None);
        close(r.projected_used_minutes, 0.0);
    }

    /// spec §6.4：sustainable_rate = R / A_left = 4500 / 28800，
    /// 今天還剩 1440 分鐘 → 225 分鐘。
    #[test]
    fn today_budget_spreads_the_remainder_over_the_playable_time_left() {
        close(report(6000, 4500).today_budget_minutes, 225.0);
    }

    /// spec §6.4：額度多到用不完時，今日額度要被今天實際能玩的時間封頂。
    #[test]
    fn today_budget_never_exceeds_the_hours_left_in_the_day() {
        let r = report(60000, 60000);
        close(r.today_budget_minutes, 1440.0);
    }

    /// spec §6.5：A_past < 720 只顯示「資料累積中」，但今日額度照給。
    #[test]
    fn projections_wait_until_twelve_hours_of_playable_time_have_passed() {
        let snap = snapshot(6000, 5900);
        let early = at("2026-09-01T06:00:00Z");
        let r = compute(&snap, &Schedule::default(), early, UTC, None, None).unwrap();
        assert_eq!(r.note, Some(PaceNote::Collecting));
        assert_eq!(r.projected_used_minutes, None);
        assert_eq!(r.wasted_minutes, None);
        assert_eq!(r.runs_out_at, None);
        assert!(r.over_pace_minutes.is_some(), "配速門檻不受樣本數限制");
        assert!(
            r.today_budget_minutes.is_some(),
            "今日額度不做外推，不受限制"
        );
    }

    /// spec §6.5：A_past = 0 連配速都算不出來。
    #[test]
    fn no_playable_time_yet_means_no_pace_at_all() {
        let snap = snapshot(6000, 6000);
        let r = compute(
            &snap,
            &Schedule::default(),
            at("2026-09-01T00:00:00Z"),
            UTC,
            None,
            None,
        )
        .unwrap();
        assert_eq!(r.note, Some(PaceNote::Insufficient));
        assert_eq!(r.over_pace_minutes, None);
        assert_eq!(r.burn_rate, None);
    }

    /// spec §6.5：剩下的時間全是不可遊玩時段。
    #[test]
    fn no_playable_time_left_hides_the_projection_and_today_budget() {
        let blocked = Schedule {
            weekly: Vec::new(),
            exceptions: vec![crate::pace::schedule::Exception {
                start_date: "2026-09-11".parse().unwrap(),
                end_date: "2026-10-01".parse().unwrap(),
                kind: crate::pace::schedule::ExceptionKind::Blocked,
                note: String::new(),
            }],
        };
        let r = compute(&snapshot(6000, 4500), &blocked, now(), UTC, None, None).unwrap();
        assert_eq!(r.avail_left_minutes, 0);
        assert_eq!(r.note, Some(PaceNote::NoTimeLeft));
        assert_eq!(r.today_budget_minutes, None);
        assert_eq!(r.projected_used_minutes, None);
    }

    /// spec §6.5：免費方案沒有配額可言。
    #[test]
    fn a_free_tier_has_no_pace() {
        let mut sub: Subscription = serde_json::from_str(FIXTURE).unwrap();
        sub.sub_type = "FREE".into();
        let snap = QuotaSnapshot::from_subscription(&sub, now());
        assert!(compute(&snap, &Schedule::default(), now(), UTC, None, None).is_none());
    }

    /// spec §6.5：本期已經結束的資料不做任何推算，等下一次抓取。
    #[test]
    fn an_expired_span_has_no_pace() {
        let snap = snapshot(6000, 4500);
        assert!(compute(
            &snap,
            &Schedule::default(),
            at("2026-10-02T00:00:00Z"),
            UTC,
            None,
            None
        )
        .is_none());
    }

    /// 每天 00:00–08:00 睡覺，可遊玩時間一天 960 分鐘。
    fn sleeping() -> Schedule {
        Schedule {
            weekly: vec![crate::pace::schedule::WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4, 5, 6],
                start_minute: 0,
                end_minute: 480,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        }
    }

    fn budget(at_time: &str, used_today: Option<u32>) -> f64 {
        compute(
            &snapshot(6000, 4500),
            &sleeping(),
            at(at_time),
            UTC,
            used_today,
            None,
        )
        .unwrap()
        .today_budget_minutes
        .unwrap()
    }

    /// 這是整個改動的重點。同一天從早看到晚、中間一分鐘都沒玩，今日額度
    /// 不該掉下來 —— 份額算的是一整天，不是「現在到今天結束」。
    #[test]
    fn todays_budget_holds_through_the_day_when_nothing_was_played() {
        let morning = budget("2026-09-11T09:00:00Z", Some(0));
        let evening = budget("2026-09-11T20:00:00Z", Some(0));

        assert!(
            (evening - morning).abs() < 15.0,
            "早上 {morning:.0} 分鐘，晚上 {evening:.0} 分鐘"
        );
    }

    /// 答不出今天用了多少就當成沒玩，不換一套算法。同一個欄位在不同日子
    /// 有兩種行為，比多給一點份額更難理解。
    #[test]
    fn an_unknown_usage_counts_as_none_played() {
        for at_time in ["2026-09-11T09:00:00Z", "2026-09-11T20:00:00Z"] {
            assert_eq!(budget(at_time, None), budget(at_time, Some(0)), "{at_time}");
        }
    }

    /// 份額比今天剩下的可遊玩時間還多時，以剩下的為準。
    #[test]
    fn todays_budget_never_exceeds_what_is_left_of_today() {
        let late = budget("2026-09-11T22:00:00Z", Some(0));
        assert!((late - 120.0).abs() < 0.001, "{late}");
    }

    /// 數字該因為你玩而變少，而且是一比一。
    #[test]
    fn playing_eats_into_todays_budget() {
        let untouched = budget("2026-09-11T20:00:00Z", Some(0));
        let after_two_hours = budget("2026-09-11T20:00:00Z", Some(120));

        assert!((untouched - after_two_hours - 120.0).abs() < 0.001);
    }

    /// 玩超過份額就是零，不是負數。
    #[test]
    fn overspending_todays_budget_floors_at_zero() {
        assert_eq!(budget("2026-09-11T20:00:00Z", Some(9999)), 0.0);
    }

    /// 不可遊玩時段會改變分母：睡覺 7 小時後，同樣的用量就不算超前了。
    #[test]
    fn a_blackout_schedule_changes_the_expectation() {
        let sleep = Schedule {
            weekly: vec![crate::pace::schedule::WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4, 5, 6],
                start_minute: 0,
                end_minute: 420,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        let with_sleep = compute(&snapshot(6000, 4500), &sleep, now(), UTC, None, None).unwrap();
        let without = report(6000, 4500);
        assert!(with_sleep.avail_past_minutes < without.avail_past_minutes);
        assert!(with_sleep.avail_left_minutes < without.avail_left_minutes);
    }

    #[test]
    fn over_pace_only_applies_when_the_base_state_is_normal() {
        let snap = snapshot(6000, 3000);
        let r = report(6000, 3000);
        assert_eq!(display_state(&snap, Some(&r)), DisplayState::OverPace);
    }

    #[test]
    fn a_low_quota_outranks_over_pace() {
        let mut sub: Subscription = serde_json::from_str(FIXTURE).unwrap();
        sub.remaining_time_in_minutes = 100; // 低於 notifications 的 300
        sub.current_span_start_date_time = Some(at("2026-09-01T00:00:00Z"));
        sub.current_span_end_date_time = Some(at("2026-10-01T00:00:00Z"));
        let snap = QuotaSnapshot::from_subscription(&sub, now());
        let r = compute(&snap, &Schedule::default(), now(), UTC, None, None).unwrap();
        assert_eq!(display_state(&snap, Some(&r)), DisplayState::Low);
    }

    #[test]
    fn no_pace_report_leaves_the_base_state_alone() {
        let snap = snapshot(6000, 4500);
        assert_eq!(display_state(&snap, None), DisplayState::Normal);
    }
}
