//! 打包後唯一的診斷管道。
//!
//! release 建構有 `windows_subsystem = "windows"`，沒有主控台，`eprintln!`
//! 全部寫進空的 handle。使用者回報「它不動了」的時候，沒有這個檔案就沒有
//! 任何東西可看。

use tauri::{plugin::TauriPlugin, Runtime};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

/// 單一日誌檔的上限。外掛預設只有 40KB，幾天就轉掉了。
const MAX_FILE_SIZE: u128 = 2 * 1024 * 1024;

/// 連同目前這份總共留幾份。2MB × 3 是天花板。
const KEEP_FILES: usize = 3;

/// 會把 HTTP 標頭原樣印出來的 crate。
///
/// 這份清單不是調校，是 spec §9 那條「憑證不得出現在日誌」的實作：`log` 是
/// 全域門面，掛上去就同時收到這幾個 crate 的訊息，而它們在 DEBUG／TRACE
/// 會印出 `Authorization` 標頭與完整網址 —— 授權碼就在網址裡。任何時候想
/// 「開 debug 看看」，都不准把這幾個調上去。
const NOISY_HTTP_CRATES: &[&str] = &["reqwest", "hyper", "hyper_util", "h2", "rustls"];

/// 框架自己的訊息量很大，而且跟這個程式的問題無關。
const NOISY_FRAMEWORK_CRATES: &[&str] = &["tauri", "wry", "tao"];

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    let mut builder = tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::LogDir {
            file_name: Some("gfnusage".into()),
        }))
        // dev 時看得到，release 沒有人接。留著是因為 `tauri dev` 的主控台
        // 比去翻檔案快。
        .target(Target::new(TargetKind::Stderr))
        .level(log::LevelFilter::Info)
        .max_file_size(MAX_FILE_SIZE)
        .rotation_strategy(RotationStrategy::KeepSome(KEEP_FILES))
        .timezone_strategy(TimezoneStrategy::UseLocal);

    for target in NOISY_HTTP_CRATES.iter().chain(NOISY_FRAMEWORK_CRATES) {
        builder = builder.level_for(*target, log::LevelFilter::Warn);
    }

    builder.build()
}

/// 把 panic 導進日誌。
///
/// 預設的 panic hook 寫 stderr，打包後那裡沒有人接 —— 輪詢 task 裡的
/// `.lock().unwrap()` 炸掉時，畫面只會停在最後一個數字，看不出發生過什麼事。
/// 這是當掉這件事唯一能留下痕跡的地方。
pub fn log_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // panic 訊息可能來自任何地方，但這個 codebase 裡帶憑證的型別都不
        // derive Debug，`unwrap()` 印不出內容物。
        log::error!("panic：{info}");
        previous(info);
    }));
}
