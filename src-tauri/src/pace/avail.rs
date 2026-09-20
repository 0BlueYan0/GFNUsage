use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use super::schedule::{ExceptionKind, Schedule, MINUTES_PER_DAY};

/// 跳過日光節約時間缺口時最多往前找幾分鐘。世界上沒有超過 2 小時的跳躍，
/// 180 分鐘是留了餘裕的上限，也讓迴圈一定會結束。
const MAX_GAP_MINUTES: i64 = 180;

/// 本地時間換成 UTC 時點。
///
/// 日光節約時間會讓某些本地時間不存在（春天往前跳）或出現兩次（秋天往回撥）。
/// 不存在時取跳躍結束後的第一個有效時點，重複時取較早的那一個。
/// 位移一律由 chrono-tz 決定 —— 自己加減位移正是 spec 明令禁止的做法。
fn to_utc(naive: NaiveDateTime, tz: Tz) -> Option<DateTime<Utc>> {
    for step in 0..=MAX_GAP_MINUTES {
        match tz.from_local_datetime(&(naive + Duration::minutes(step))) {
            chrono::LocalResult::Single(t) => return Some(t.with_timezone(&Utc)),
            chrono::LocalResult::Ambiguous(earliest, _) => {
                return Some(earliest.with_timezone(&Utc))
            }
            chrono::LocalResult::None => continue,
        }
    }
    None
}

/// 本地日期的午夜對應的 UTC 時點。有些時區的日光節約時間正好在午夜換，
/// 所以這裡也走 `to_utc` 的缺口處理。
pub fn local_midnight(date: NaiveDate, tz: Tz) -> Option<DateTime<Utc>> {
    to_utc(date.and_hms_opt(0, 0, 0)?, tz)
}

/// 某個本地日期適用哪一種規則。
enum DayRule {
    /// 照每週時段。
    Weekly,
    /// 整天不可遊玩。
    Blocked,
    /// 整天可遊玩。
    Free,
}

/// 一次性例外優先於每週時段。同一天被多個例外涵蓋時以清單中較後者為準，
/// 使用者才能先標一整週出差不可遊玩，再在裡面挖一天出來。
fn day_rule(date: NaiveDate, schedule: &Schedule) -> DayRule {
    let mut rule = DayRule::Weekly;
    for exception in &schedule.exceptions {
        if exception.start_date <= date && date <= exception.end_date {
            rule = match exception.kind {
                ExceptionKind::Blocked => DayRule::Blocked,
                ExceptionKind::Free => DayRule::Free,
            };
        }
    }
    rule
}

/// 該本地日期的每週時段，換算成「距當日 00:00 的分鐘數」。
/// 跨日的時段回傳超過 1440 的結束值，讓它自然延伸到隔天。
fn weekly_spans(date: NaiveDate, schedule: &Schedule) -> Vec<(i64, i64)> {
    let weekday = date.weekday().num_days_from_monday() as u8;
    schedule
        .weekly
        .iter()
        .filter(|window| window.weekdays.contains(&weekday))
        .map(|window| {
            let start = window.start_minute as i64;
            let mut end = window.end_minute as i64;
            if end <= start {
                end += MINUTES_PER_DAY as i64;
            }
            (start, end)
        })
        .collect()
}

fn push_span(
    out: &mut Vec<(DateTime<Utc>, DateTime<Utc>)>,
    midnight: NaiveDateTime,
    start: i64,
    end: i64,
    tz: Tz,
) {
    let (Some(from), Some(to)) = (
        to_utc(midnight + Duration::minutes(start), tz),
        to_utc(midnight + Duration::minutes(end), tz),
    ) else {
        return;
    };
    if to > from {
        out.push((from, to));
    }
}

/// 排序後把重疊或相接的區間併起來。
fn merge(mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)>) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    spans.sort_by_key(|(from, _)| *from);
    let mut out: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (from, to) in spans {
        match out.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => out.push((from, to)),
        }
    }
    out
}

/// 從 `spans` 挖掉 `holes`。兩邊都必須已排序、已合併。
fn subtract(
    spans: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    holes: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut out = Vec::new();
    for (mut from, to) in spans {
        for (hole_from, hole_to) in holes {
            if *hole_to <= from || *hole_from >= to {
                continue;
            }
            if *hole_from > from {
                out.push((from, *hole_from));
            }
            from = from.max(*hole_to);
            if from >= to {
                break;
            }
        }
        if from < to {
            out.push((from, to));
        }
    }
    out
}

fn clip(
    spans: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    spans
        .into_iter()
        .filter_map(|(a, b)| {
            let a = a.max(from);
            let b = b.min(to);
            (b > a).then_some((a, b))
        })
        .collect()
}

/// `[from, to)` 之間的不可遊玩區間，已排序、已合併、已扣掉「全天可遊玩」的日子。
///
/// 「全天可遊玩」是整天都能玩，所以它是最後才挖的洞 —— 這樣連前一晚跨進來的
/// 時段也一併清掉，符合使用者標這一天時的意思。
pub fn blocked_intervals(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    schedule: &Schedule,
    tz: Tz,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    if to <= from {
        return Vec::new();
    }

    // 前一天的跨日時段會延伸進範圍內，所以從前一個本地日開始掃。
    let first = from.with_timezone(&tz).date_naive() - Duration::days(1);
    let last = to.with_timezone(&tz).date_naive();

    let mut blocked = Vec::new();
    let mut free = Vec::new();
    let mut date = first;
    let whole_day = MINUTES_PER_DAY as i64;

    while date <= last {
        let Some(midnight) = date.and_hms_opt(0, 0, 0) else {
            break;
        };
        match day_rule(date, schedule) {
            DayRule::Free => push_span(&mut free, midnight, 0, whole_day, tz),
            DayRule::Blocked => push_span(&mut blocked, midnight, 0, whole_day, tz),
            DayRule::Weekly => {
                for (start, end) in weekly_spans(date, schedule) {
                    push_span(&mut blocked, midnight, start, end, tz);
                }
            }
        }
        date += Duration::days(1);
    }

    clip(subtract(merge(blocked), &merge(free)), from, to)
}

/// `[from, to)` 之間可以遊玩的區間，依時間排序。
pub fn free_intervals(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    schedule: &Schedule,
    tz: Tz,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    if to <= from {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = from;
    for (blocked_from, blocked_to) in blocked_intervals(from, to, schedule, tz) {
        if blocked_from > cursor {
            out.push((cursor, blocked_from));
        }
        cursor = cursor.max(blocked_to);
    }
    if cursor < to {
        out.push((cursor, to));
    }
    out
}

/// `from` 到 `to` 之間的可遊玩分鐘數（spec §6.1）。
///
/// 沒有任何不可遊玩時段時就是單純的時間差。內部以秒計算再換成分鐘，
/// 免得總長與各區間分別無條件捨去後湊出誤差。
pub fn avail(from: DateTime<Utc>, to: DateTime<Utc>, schedule: &Schedule, tz: Tz) -> u32 {
    if to <= from {
        return 0;
    }
    let total = (to - from).num_seconds();
    let blocked: i64 = blocked_intervals(from, to, schedule, tz)
        .iter()
        .map(|(a, b)| (*b - *a).num_seconds())
        .sum();
    ((total - blocked).max(0) / 60) as u32
}

#[cfg(test)]
mod tests {
    use chrono_tz::America::New_York;
    use chrono_tz::Asia::Taipei;
    use chrono_tz::UTC;

    use super::*;
    use crate::pace::schedule::{Exception, WeeklyWindow};

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    fn nightly_sleep() -> Schedule {
        Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4, 5, 6],
                start_minute: 0,
                end_minute: 420,
                note: "睡覺".into(),
            }],
            exceptions: Vec::new(),
        }
    }

    /// spec §6.1：沒有任何設定時就是單純的時間差。
    #[test]
    fn an_empty_schedule_is_plain_wall_clock() {
        let empty = Schedule::default();
        assert_eq!(
            avail(
                at("2026-09-20T00:00:00Z"),
                at("2026-09-21T00:00:00Z"),
                &empty,
                Taipei
            ),
            1440
        );
    }

    #[test]
    fn a_backwards_range_is_zero() {
        let empty = Schedule::default();
        assert_eq!(
            avail(
                at("2026-09-21T00:00:00Z"),
                at("2026-09-20T00:00:00Z"),
                &empty,
                Taipei
            ),
            0
        );
    }

    /// 台北 00:00–07:00 睡覺，一整個本地日扣掉 420 分鐘。
    #[test]
    fn a_daily_window_is_subtracted_once_per_day() {
        // 台北 2026-09-20 00:00 = 2026-09-19T16:00Z
        assert_eq!(
            avail(
                at("2026-09-19T16:00:00Z"),
                at("2026-09-20T16:00:00Z"),
                &nightly_sleep(),
                Taipei
            ),
            1440 - 420
        );
    }

    /// 範圍只涵蓋時段的一半，就只扣一半。
    #[test]
    fn a_partially_covered_window_is_clipped() {
        // 台北 2026-09-20 03:00 → 09:00：睡覺時段只重疊到 03:00–07:00
        assert_eq!(
            avail(
                at("2026-09-19T19:00:00Z"),
                at("2026-09-20T01:00:00Z"),
                &nightly_sleep(),
                Taipei
            ),
            360 - 240
        );
    }

    /// 22:00–02:00 的時段要跨過午夜繼續算。
    #[test]
    fn a_window_crossing_midnight_spills_into_the_next_day() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![4], // 週五
                start_minute: 1320,
                end_minute: 120,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        // 2026-09-18 是週五。台北 週五 00:00 → 週六 12:00 = 36 小時，
        // 扣掉週五 22:00–週六 02:00 的 240 分鐘。
        assert_eq!(
            avail(
                at("2026-09-17T16:00:00Z"),
                at("2026-09-19T04:00:00Z"),
                &schedule,
                Taipei
            ),
            2160 - 240
        );
    }

    /// spec §10 點名要測跨週：週日的時段延伸到週一，星期是逐日判斷的，
    /// 不能因為換了一週就漏算。
    #[test]
    fn a_window_crossing_the_week_boundary_spills_into_monday() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![6], // 週日
                start_minute: 1320,
                end_minute: 120,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        // 2026-09-20 是週日。台北 週日 20:00 → 週一 04:00 = 480 分鐘，
        // 扣掉週日 22:00–週一 02:00 的 240 分鐘。
        assert_eq!(
            avail(
                at("2026-09-20T12:00:00Z"),
                at("2026-09-20T20:00:00Z"),
                &schedule,
                Taipei
            ),
            480 - 240
        );
    }

    /// 兩段重疊的時段不能扣兩次。
    #[test]
    fn overlapping_windows_are_not_double_counted() {
        let mut schedule = nightly_sleep();
        schedule.weekly.push(WeeklyWindow {
            weekdays: vec![0, 1, 2, 3, 4, 5, 6],
            start_minute: 300, // 05:00–09:00，與睡覺的 05:00–07:00 重疊
            end_minute: 540,
            note: String::new(),
        });
        assert_eq!(
            avail(
                at("2026-09-19T16:00:00Z"),
                at("2026-09-20T16:00:00Z"),
                &schedule,
                Taipei
            ),
            1440 - 540
        );
    }

    #[test]
    fn a_blocked_exception_removes_the_whole_local_day() {
        let mut schedule = nightly_sleep();
        schedule.exceptions.push(Exception {
            start_date: date("2026-09-20"),
            end_date: date("2026-09-20"),
            kind: ExceptionKind::Blocked,
            note: "出差".into(),
        });
        assert_eq!(
            avail(
                at("2026-09-19T16:00:00Z"),
                at("2026-09-20T16:00:00Z"),
                &schedule,
                Taipei
            ),
            0
        );
    }

    /// 全天可遊玩要蓋掉當天的每週時段（spec §6.1：一次性例外優先）。
    #[test]
    fn a_free_exception_overrides_the_weekly_windows() {
        let mut schedule = nightly_sleep();
        schedule.exceptions.push(Exception {
            start_date: date("2026-09-20"),
            end_date: date("2026-09-20"),
            kind: ExceptionKind::Free,
            note: "連假".into(),
        });
        assert_eq!(
            avail(
                at("2026-09-19T16:00:00Z"),
                at("2026-09-20T16:00:00Z"),
                &schedule,
                Taipei
            ),
            1440
        );
    }

    /// 「全天可遊玩」就是整天都能玩，連前一晚溢進來的時段也不算。
    #[test]
    fn a_free_exception_also_clears_a_window_spilling_in_from_the_night_before() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![4],
                start_minute: 1320, // 週五 22:00 → 週六 02:00
                end_minute: 120,
                note: String::new(),
            }],
            exceptions: vec![Exception {
                start_date: date("2026-09-19"), // 週六
                end_date: date("2026-09-19"),
                kind: ExceptionKind::Free,
                note: String::new(),
            }],
        };
        // 台北 週五 20:00 → 週六 04:00 = 480 分鐘，只扣週五的 22:00–24:00。
        assert_eq!(
            avail(
                at("2026-09-18T12:00:00Z"),
                at("2026-09-18T20:00:00Z"),
                &schedule,
                Taipei
            ),
            480 - 120
        );
    }

    /// 長假整段不可遊玩，中間挖一天可玩 —— 後面的例外贏。
    #[test]
    fn the_later_exception_wins_when_two_cover_the_same_day() {
        let schedule = Schedule {
            weekly: Vec::new(),
            exceptions: vec![
                Exception {
                    start_date: date("2026-09-14"),
                    end_date: date("2026-09-25"),
                    kind: ExceptionKind::Blocked,
                    note: "出差".into(),
                },
                Exception {
                    start_date: date("2026-09-20"),
                    end_date: date("2026-09-20"),
                    kind: ExceptionKind::Free,
                    note: "週日休息".into(),
                },
            ],
        };
        assert_eq!(
            avail(
                at("2026-09-19T16:00:00Z"),
                at("2026-09-20T16:00:00Z"),
                &schedule,
                Taipei
            ),
            1440
        );
    }

    /// 2026-03-08 美東進入日光節約時間：本地日只有 23 小時，
    /// 而本地 00:00–07:00 的睡覺時段只佔 6 個實際小時。
    #[test]
    fn spring_forward_shortens_both_the_day_and_the_window() {
        let empty = Schedule::default();
        let start = at("2026-03-08T05:00:00Z"); // 紐約 00:00 EST
        let end = at("2026-03-09T04:00:00Z"); // 紐約 00:00 EDT
        assert_eq!(avail(start, end, &empty, New_York), 1380);
        assert_eq!(avail(start, end, &nightly_sleep(), New_York), 1380 - 360);
    }

    /// 2026-11-01 美東離開日光節約時間：本地日有 25 小時，
    /// 而 00:00–07:00 的睡覺時段佔了 8 個實際小時。
    #[test]
    fn fall_back_lengthens_both_the_day_and_the_window() {
        let empty = Schedule::default();
        let start = at("2026-11-01T04:00:00Z"); // 紐約 00:00 EDT
        let end = at("2026-11-02T05:00:00Z"); // 紐約 00:00 EST
        assert_eq!(avail(start, end, &empty, New_York), 1500);
        assert_eq!(avail(start, end, &nightly_sleep(), New_York), 1500 - 480);
    }

    /// 春天往前跳掉的那個小時裡，本地時間不存在；時段起點要落在跳完之後，
    /// 不能整段消失、也不能自己往回算位移。
    #[test]
    fn a_window_starting_inside_the_spring_forward_gap_survives() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![6], // 2026-03-08 是週日
                start_minute: 150, // 02:30 —— 這個本地時間不存在
                end_minute: 240,   // 04:00
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        // 跳躍後的第一個有效時點是 03:00 EDT，所以扣掉 03:00–04:00 共 60 分鐘。
        let start = at("2026-03-08T05:00:00Z");
        let end = at("2026-03-09T04:00:00Z");
        assert_eq!(avail(start, end, &schedule, New_York), 1380 - 60);
    }

    #[test]
    fn a_range_entirely_inside_a_blocked_window_is_zero() {
        // 台北 02:00 → 04:00，整段都在睡覺時段裡
        assert_eq!(
            avail(
                at("2026-09-19T18:00:00Z"),
                at("2026-09-19T20:00:00Z"),
                &nightly_sleep(),
                Taipei
            ),
            0
        );
    }

    #[test]
    fn free_intervals_are_the_complement_of_the_blocked_ones() {
        let intervals = free_intervals(
            at("2026-09-19T16:00:00Z"),
            at("2026-09-20T16:00:00Z"),
            &nightly_sleep(),
            Taipei,
        );
        assert_eq!(
            intervals,
            vec![(at("2026-09-19T23:00:00Z"), at("2026-09-20T16:00:00Z"))]
        );
    }

    #[test]
    fn utc_needs_no_special_casing() {
        assert_eq!(
            avail(
                at("2026-09-20T00:00:00Z"),
                at("2026-09-21T00:00:00Z"),
                &nightly_sleep(),
                UTC
            ),
            1440 - 420
        );
    }
}
