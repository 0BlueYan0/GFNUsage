use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// 一天的分鐘數。
pub const MINUTES_PER_DAY: u16 = 1440;

/// 每週重複的不可遊玩時段。
///
/// 時間是本地時間距午夜的分鐘數。`end_minute <= start_minute` 代表跨日，
/// 例如 22:00–02:00 是 `start_minute = 1320, end_minute = 120`。整天是
/// `0 → 1440`，因為 `end` 可以等於 1440，不會和跨日的寫法混淆。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyWindow {
    /// 0 = 週一 … 6 = 週日，與 `chrono` 的 `num_days_from_monday()` 一致。
    pub weekdays: Vec<u8>,
    pub start_minute: u16,
    pub end_minute: u16,
    /// 使用者自己寫的標籤，例如「上班」。純顯示用。
    #[serde(default)]
    pub note: String,
}

/// 一次性例外的種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExceptionKind {
    /// 整段期間都不可遊玩。
    Blocked,
    /// 整段期間都可遊玩，蓋掉這幾天的每週時段。
    Free,
}

/// 一次性例外。日期是本地日期，含頭含尾。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exception {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub kind: ExceptionKind,
    #[serde(default)]
    pub note: String,
}

/// 不可遊玩時段的完整設定。
///
/// 空設定代表隨時都能玩 —— 此時 `avail()` 退化為單純的時間差，
/// 整套演算法等同 OpenUsage 的線性門檻（spec §6.1）。這是刻意的：
/// 一套演算法涵蓋兩種模式，不需要分支。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Schedule {
    pub weekly: Vec<WeeklyWindow>,
    pub exceptions: Vec<Exception>,
}

impl Schedule {
    /// 檢查設定是否合理。錯誤訊息會直接顯示在面板上，所以用繁體中文。
    pub fn validate(&self) -> Result<(), String> {
        for window in &self.weekly {
            if window.weekdays.is_empty() {
                return Err("每週時段至少要選一個星期".into());
            }
            if let Some(day) = window.weekdays.iter().find(|day| **day > 6) {
                return Err(format!("星期代號 {day} 超出範圍（0–6）"));
            }
            if window.start_minute >= MINUTES_PER_DAY || window.end_minute > MINUTES_PER_DAY {
                return Err("時段時間超出一天的範圍".into());
            }
            if window.start_minute == window.end_minute {
                return Err("時段的起訖時間不能相同".into());
            }
        }
        for exception in &self.exceptions {
            if exception.end_date < exception.start_date {
                return Err("例外的結束日期早於開始日期".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap()
    }

    #[test]
    fn an_empty_schedule_is_the_default() {
        let schedule = Schedule::default();
        assert!(schedule.weekly.is_empty());
        assert!(schedule.exceptions.is_empty());
        assert!(schedule.validate().is_ok());
    }

    #[test]
    fn a_schedule_round_trips_through_json() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4],
                start_minute: 540,
                end_minute: 1080,
                note: "上班".into(),
            }],
            exceptions: vec![Exception {
                start_date: date("2026-10-01"),
                end_date: date("2026-10-03"),
                kind: ExceptionKind::Free,
                note: "連假".into(),
            }],
        };

        let text = serde_json::to_string(&schedule).unwrap();
        assert!(text.contains("\"startMinute\":540"), "{text}");
        assert!(text.contains("\"kind\":\"free\""), "{text}");
        assert_eq!(serde_json::from_str::<Schedule>(&text).unwrap(), schedule);
    }

    /// 之後版本加的欄位不能讓舊版讀不懂，反之亦然。
    #[test]
    fn unknown_fields_and_missing_fields_are_tolerated() {
        let text = r#"{"weekly":[{"weekdays":[6],"startMinute":0,"endMinute":480}],"future":42}"#;
        let schedule: Schedule = serde_json::from_str(text).unwrap();
        assert_eq!(schedule.weekly.len(), 1);
        assert_eq!(schedule.weekly[0].note, "");
        assert!(schedule.exceptions.is_empty());
    }

    #[test]
    fn a_window_without_weekdays_is_rejected() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![],
                start_minute: 0,
                end_minute: 480,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        assert!(schedule.validate().is_err());
    }

    #[test]
    fn a_weekday_out_of_range_is_rejected() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![7],
                start_minute: 0,
                end_minute: 480,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        assert!(schedule.validate().is_err());
    }

    /// 起訖相同無法分辨「零長度」與「整天」，一律擋掉。
    #[test]
    fn a_zero_length_window_is_rejected() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![0],
                start_minute: 600,
                end_minute: 600,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        assert!(schedule.validate().is_err());
    }

    #[test]
    fn a_backwards_exception_is_rejected() {
        let schedule = Schedule {
            weekly: Vec::new(),
            exceptions: vec![Exception {
                start_date: date("2026-10-05"),
                end_date: date("2026-10-01"),
                kind: ExceptionKind::Blocked,
                note: String::new(),
            }],
        };
        assert!(schedule.validate().is_err());
    }

    /// 22:00–02:00 這種跨日時段是合法的，驗證不能把它當成起訖顛倒。
    #[test]
    fn a_window_crossing_midnight_is_valid() {
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![4],
                start_minute: 1320,
                end_minute: 120,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        assert!(schedule.validate().is_ok());
    }
}
