use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::pace::schedule::Schedule;

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
        file.sync_all().map_err(|e| format!("寫入設定檔失敗：{e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("置換設定檔失敗：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pace::schedule::WeeklyWindow;

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
        std::fs::write(&path, r#"{"weekly":[{"weekdays":[9],"startMinute":0,"endMinute":60}]}"#)
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
