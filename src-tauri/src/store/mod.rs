use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
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

/// 找到的那一列離目標時刻超過這麼久，就不拿它當「今天開始時的剩餘量」。
///
/// 代價講明白：這段空白裡玩掉的時間會被算成今天的。程式每 5 分鐘寫一列，
/// 正常運作時誤差是幾分鐘。設成一小時，是願意把昨晚最後一小時的遊玩算進
/// 今天，換取「機器在午夜前後短暫關掉」這種常見情況仍然算得出答案。
pub const MAX_HISTORY_GAP: Duration = Duration::hours(1);

/// 只讀檔尾這麼多位元組。一列約 160 bytes、每 5 分鐘一列，一天約 288 列
/// 也就是 46 KB，256 KB 涵蓋五天多。要找的只是今天午夜前那一列，再往前的
/// `MAX_HISTORY_GAP` 本來就會擋掉。
///
/// 不整份讀進來：歷史只增不減，跑滿一年是 17 MB，而這個函式每一輪都會叫。
const TAIL_BYTES: u64 = 256 * 1024;

/// 歷史裡不晚於 `at` 的最後一列的剩餘量。
///
/// 只看 `span_start` 相同的列。本期重置後剩餘量會跳回滿，拿上一期的數字
/// 去減，算出來的「今天用了多少」會是負的。
///
/// 回傳 `None` 代表答不出來：沒有歷史、檔案讀不動、或最近的一列離 `at`
/// 太遠。呼叫端要有退路，不能把它當成零 —— 那會把沒觀測到的遊玩當成沒玩。
pub fn remaining_at(
    path: &Path,
    at: DateTime<Utc>,
    span_start: Option<DateTime<Utc>>,
) -> Option<u32> {
    let mut file = fs::File::open(path).ok()?;
    let from = file.metadata().ok()?.len().saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(from)).ok()?;

    let mut reader = BufReader::new(file);
    if from > 0 {
        // 從中間切進去，第一行多半是半行。丟掉。
        let mut partial = String::new();
        reader.read_line(&mut partial).ok()?;
    }

    let mut best: Option<SnapshotRow> = None;
    for line in reader.lines() {
        // 讀到一半壞掉就收在這裡，用已經找到的那一列。再往下讀也讀不出
        // 更好的答案。
        let Ok(line) = line else { break };
        let Ok(row) = serde_json::from_str::<SnapshotRow>(&line) else {
            continue;
        };
        // 檔案是時間順序的，後面只會更晚。
        if row.fetched_at > at {
            break;
        }
        if row.span_start == span_start {
            best = Some(row);
        }
    }

    let best = best?;
    (at - best.fetched_at <= MAX_HISTORY_GAP).then_some(best.remaining_minutes)
}

/// 面板的一次性 UI 狀態。
///
/// 和設定分開存，因為它**不該被匯出**：「提示看過了沒」是這台機器的事，
/// 跟著作息設定搬到另一台機器只會讓那台機器的提示憑空消失。
pub const UI_STATE_FILE: &str = "ui-state.json";

/// 面板的主要數字看哪一邊。進度條跟著它走 —— 兩邊各看各的話，數字往下掉
/// 而長條往上長。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    /// 還剩多少。預設，這個工具存在的理由就是盯著它。
    #[default]
    Remaining,
    /// 已經用掉多少。
    Used,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiState {
    /// 已經啟動過至少一次。用來決定要不要主動把面板叫出來。
    pub first_run_done: bool,
    /// 系統匣溢位區的提示已經被關掉了。
    pub tray_hint_dismissed: bool,
    /// 主要數字顯示剩餘還是已使用。放這裡不放 `schedule.json`：它是這台
    /// 機器上的看法，不該跟著不可遊玩時段一起匯出到別台。
    pub metric: Metric,

    /// 送給 NVIDIA 授權端點的裝置識別碼。第一次登入時生成。
    ///
    /// 和 `metric` 同一個理由放這裡：它是這台機器的身分。空字串代表還沒生過。
    pub device_id: String,
}

pub fn ui_state_path(dir: &Path) -> PathBuf {
    dir.join(UI_STATE_FILE)
}

/// 讀取 UI 狀態。檔案不存在、壞掉、讀不動，一律回預設值。
///
/// 和設定檔的處理刻意不同：設定讀壞了要講出來（少算一段不可遊玩時段會讓
/// 預測悄悄失準），但這裡面只有兩個布林值，為它們在面板上擺一行錯誤訊息
/// 完全不成比例 —— 最壞的後果是提示多出現一次。
pub fn load_ui_state(path: &Path) -> UiState {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// 寫入 UI 狀態。
///
/// 不走設定檔那套「寫暫存檔再改名」：整份檔案就兩個布林值，寫壞了下次讀
/// 不出來就回預設值，代價只是提示多出現一次。設定檔值得那道保險，這個不值得。
pub fn save_ui_state(path: &Path, ui: &UiState) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("建立設定目錄失敗：{e}"))?;
    }
    let text = serde_json::to_string_pretty(ui).map_err(|e| format!("序列化失敗：{e}"))?;
    fs::write(path, text).map_err(|e| format!("寫入 UI 狀態失敗：{e}"))
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

    fn span() -> Option<DateTime<Utc>> {
        Some(Utc.with_ymd_and_hms(2026, 9, 15, 13, 18, 59).unwrap())
    }

    fn cutoff(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 20, 10, minute, 0).unwrap()
    }

    #[test]
    fn remaining_at_takes_the_last_row_before_the_cutoff() {
        let dir = tempfile::tempdir().unwrap();
        let path = history_path(dir.path());
        append_history(&path, &row(0, 6300)).unwrap();
        append_history(&path, &row(30, 6240)).unwrap();
        // 截止之後的這一列不算，它是「今天」的遊玩。
        append_history(&path, &row(45, 6180)).unwrap();

        assert_eq!(remaining_at(&path, cutoff(40), span()), Some(6240));
    }

    /// 機器關掉太久，午夜前後那段沒有人觀測。寧可答不出來。
    #[test]
    fn remaining_at_gives_up_when_the_last_row_is_too_old() {
        let dir = tempfile::tempdir().unwrap();
        let path = history_path(dir.path());
        append_history(&path, &row(0, 6300)).unwrap();

        let two_hours_later = Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap();
        assert_eq!(remaining_at(&path, two_hours_later, span()), None);
    }

    /// 本期重置後剩餘量跳回滿。拿上一期的列去減會得到負的用量。
    #[test]
    fn remaining_at_ignores_rows_from_another_period() {
        let dir = tempfile::tempdir().unwrap();
        let path = history_path(dir.path());
        let mut previous = row(0, 120);
        previous.span_start = Some(Utc.with_ymd_and_hms(2026, 8, 15, 13, 18, 59).unwrap());
        append_history(&path, &previous).unwrap();
        append_history(&path, &row(30, 6240)).unwrap();

        assert_eq!(remaining_at(&path, cutoff(40), span()), Some(6240));
    }

    #[test]
    fn remaining_at_with_only_another_period_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = history_path(dir.path());
        let mut previous = row(0, 120);
        previous.span_start = Some(Utc.with_ymd_and_hms(2026, 8, 15, 13, 18, 59).unwrap());
        append_history(&path, &previous).unwrap();

        assert_eq!(remaining_at(&path, cutoff(40), span()), None);
    }

    #[test]
    fn remaining_at_without_a_history_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            remaining_at(&history_path(dir.path()), cutoff(40), span()),
            None
        );
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
    fn ui_state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = ui_state_path(dir.path());
        let ui = UiState {
            first_run_done: true,
            tray_hint_dismissed: true,
            metric: Metric::Used,
            device_id: "105c3409-aace-4e9b-a3dd-30260a61a188".into(),
        };

        save_ui_state(&path, &ui).unwrap();

        assert_eq!(load_ui_state(&path), ui);
    }

    /// 第一次啟動時檔案根本不存在。這不是錯誤，也不該在面板上留下訊息 ——
    /// 這裡面沒有一個欄位值得為它擺一行紅字。
    #[test]
    fn a_missing_ui_state_is_the_default() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            load_ui_state(&ui_state_path(dir.path())),
            UiState::default()
        );
    }

    #[test]
    fn a_broken_ui_state_falls_back_to_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = ui_state_path(dir.path());
        std::fs::write(&path, "{ not json").unwrap();

        assert_eq!(load_ui_state(&path), UiState::default());
    }

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
