use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::pace::schedule::Schedule;
use crate::quota::QuotaSnapshot;

/// 設定檔檔名。放在 app config 目錄，內容不含機密。
pub const SCHEDULE_FILE: &str = "schedule.json";

pub fn schedule_path(dir: &Path) -> PathBuf {
    dir.join(SCHEDULE_FILE)
}

/// 讀取設定。檔案不存在時回傳空設定 —— 全新安裝沒有設定不是錯誤。
///
/// 內容壞掉時回報錯誤而不是靜默用預設值：少算一段不可遊玩時段只會讓預測
/// 悄悄失準，使用者不會發現哪裡不對。
pub fn load(path: &Path) -> Result<Schedule, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Schedule::default()),
        Err(e) => return Err(format!("讀取設定檔失敗：{e}")),
    };
    let schedule: Schedule =
        serde_json::from_str(&text).map_err(|e| format!("設定檔格式錯誤：{e}"))?;
    schedule.validate()?;
    Ok(schedule)
}

/// 寫入設定。先寫暫存檔再改名，寫到一半斷電不會留下半個檔案。
pub fn save(path: &Path, schedule: &Schedule) -> Result<(), String> {
    schedule.validate()?;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("建立設定目錄失敗：{e}"))?;
    }

    let text =
        serde_json::to_string_pretty(schedule).map_err(|e| format!("設定序列化失敗：{e}"))?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp).map_err(|e| format!("寫入設定檔失敗：{e}"))?;
        file.write_all(text.as_bytes())
            .map_err(|e| format!("寫入設定檔失敗：{e}"))?;
        file.sync_all()
            .map_err(|e| format!("寫入設定檔失敗：{e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("置換設定檔失敗：{e}"))
}

/// 快照歷史檔名。一行一筆 JSON（JSONL），只增不改。
///
/// 第一版不拿它做任何計算 —— 配速與預測只需要當下的快照 —— 但從第一版
/// 就開始累積，否則之後要加趨勢圖或真實燃燒率時得從零開始等資料。
pub const HISTORY_FILE: &str = "snapshots.json";

pub fn history_path(dir: &Path) -> PathBuf {
    dir.join(HISTORY_FILE)
}

/// 歷史的一列（spec §8）。
///
/// 不直接存 `QuotaSnapshot`：那個型別只有 `Serialize`，回頭讀不出來，
/// 而且它帶著 `state` 這種由其他欄位推導出來的欄位 —— 把一個會變的判斷
/// 凍進歷史檔，之後改了判斷規則就對不起來了。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRow {
    pub fetched_at: DateTime<Utc>,
    pub remaining_minutes: u32,
    pub total_minutes: u32,
    /// 免費方案沒有「本期」，兩個都會是 null。
    pub span_start: Option<DateTime<Utc>>,
    pub span_end: Option<DateTime<Utc>>,
}

impl SnapshotRow {
    pub fn from_snapshot(snapshot: &QuotaSnapshot) -> Self {
        Self {
            fetched_at: snapshot.fetched_at,
            remaining_minutes: snapshot.remaining_minutes,
            total_minutes: snapshot.total_minutes,
            span_start: snapshot.span_start,
            span_end: snapshot.span_end,
        }
    }
}

/// 附加一列到歷史檔。
///
/// 用 append 開檔，不走 `save()` 的「寫暫存檔再改名」：那是整檔置換，
/// 等於每 5 分鐘把整份歷史讀出來再寫回去，檔案長大之後只會愈來愈慢。
/// 而且 `save()` 的 `with_extension("json.tmp")` 會把 `snapshots.json`
/// 變成 `snapshots.json.tmp`，跟設定檔的暫存檔撞名。
pub fn append_history(path: &Path, row: &SnapshotRow) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("建立設定目錄失敗：{e}"))?;
    }

    let mut line = serde_json::to_string(row).map_err(|e| format!("快照序列化失敗：{e}"))?;
    line.push('\n');

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("開啟快照歷史失敗：{e}"))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("寫入快照歷史失敗：{e}"))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::pace::schedule::WeeklyWindow;

    fn row(minute: u32, remaining: u32) -> SnapshotRow {
        SnapshotRow {
            fetched_at: Utc.with_ymd_and_hms(2026, 9, 20, 10, minute, 0).unwrap(),
            remaining_minutes: remaining,
            total_minutes: 6900,
            span_start: Some(Utc.with_ymd_and_hms(2026, 9, 15, 13, 18, 59).unwrap()),
            span_end: Some(Utc.with_ymd_and_hms(2026, 10, 15, 23, 59, 59).unwrap()),
        }
    }

    #[test]
    fn history_starts_a_file_and_then_appends_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = history_path(dir.path());

        append_history(&path, &row(0, 6180)).unwrap();
        append_history(&path, &row(5, 6120)).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<SnapshotRow> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows, vec![row(0, 6180), row(5, 6120)]);
    }

    /// 免費方案沒有「本期」可言，兩個 span 都是 null。歷史要容得下。
    #[test]
    fn history_tolerates_rows_without_a_span() {
        let dir = tempfile::tempdir().unwrap();
        let path = history_path(dir.path());
        let mut free = row(0, 0);
        free.span_start = None;
        free.span_end = None;

        append_history(&path, &free).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let back: SnapshotRow = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(back, free);
    }

    /// 設定目錄還不存在時（全新安裝的第一次抓取）也要寫得進去。
    #[test]
    fn history_creates_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = history_path(&dir.path().join("nested"));

        append_history(&path, &row(0, 6180)).unwrap();

        assert!(path.exists());
    }

    fn sample() -> Schedule {
        Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4],
                start_minute: 540,
                end_minute: 1080,
                note: "上班".into(),
            }],
            exceptions: Vec::new(),
        }
    }

    /// 全新安裝沒有設定檔，這不是錯誤。
    #[test]
    fn a_missing_file_loads_the_empty_schedule() {
        let dir = tempfile::tempdir().unwrap();
        let path = schedule_path(dir.path());
        assert_eq!(load(&path).unwrap(), Schedule::default());
    }

    #[test]
    fn a_saved_schedule_is_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = schedule_path(dir.path());
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path).unwrap(), sample());
    }

    #[test]
    fn saving_twice_replaces_the_file_and_leaves_no_temp_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = schedule_path(dir.path());
        save(&path, &sample()).unwrap();
        save(&path, &Schedule::default()).unwrap();
        assert_eq!(load(&path).unwrap(), Schedule::default());

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    /// 少算一段不可遊玩時段會讓預測悄悄失準，所以壞掉的檔案要吵。
    #[test]
    fn a_broken_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = schedule_path(dir.path());
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn a_file_with_invalid_content_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = schedule_path(dir.path());
        std::fs::write(
            &path,
            r#"{"weekly":[{"weekdays":[9],"startMinute":0,"endMinute":60}]}"#,
        )
        .unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn saving_an_invalid_schedule_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = schedule_path(dir.path());
        let bad = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![0],
                start_minute: 600,
                end_minute: 600,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        assert!(save(&path, &bad).is_err());
        assert!(!path.exists());
    }
}
