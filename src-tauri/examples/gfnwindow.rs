//! GFN 視窗偵測的診斷版。
//!
//! ```
//! cargo run --example gfnwindow --manifest-path src-tauri/Cargo.toml
//! cargo run --example gfnwindow --manifest-path src-tauri/Cargo.toml -- --watch
//! ```
//!
//! 為什麼要這支：偵測不到的時候，正式路徑只會安靜地回 `Absent`，日誌上什麼
//! 都不會有 —— 使用者看到「沒反應」，但貼不出任何東西來讓人查。這支把中間
//! 每一步攤開，一次就分得出是哪一種：
//!
//! - GFN 根本沒開
//! - GFN 開著但沒有可見視窗（縮到工作列？被別的完整性等級擋住？）
//! - 有視窗，但標題裡沒有品牌字樣（語系模板不一樣？）
//! - 標題對，但擁有它的執行檔不叫 `GeForceNOW.exe`
//!
//! 只讀本機狀態，不發任何網路請求，也不碰金鑰儲存區。

use gfnusage_lib::watcher::{self, win32};

fn main() {
    if !cfg!(windows) {
        println!("視窗偵測只有 Windows 有實作。");
        return;
    }

    let watch = std::env::args().any(|a| a == "--watch");
    if !watch {
        report();
        return;
    }

    // `--watch` 用正式路徑的間隔與防抖，走一遍「開遊戲、退出來」看轉換對不對。
    println!(
        "每 {} 秒掃一次，連續 {} 次相同才算數。Ctrl+C 結束。\n",
        watcher::SWEEP.as_secs(),
        watcher::CONFIRM
    );
    let mut probe = watcher::Probe::new();
    loop {
        let seen = win32::current_state();
        match probe.observe(seen) {
            Some(watcher::Change::Baseline(state)) => println!("起點 {state:?}"),
            Some(watcher::Change::Moved(from, to)) => {
                println!("{from:?} → {to:?}　（{:?}）", watcher::trigger(from, to));
            }
            None => {}
        }
        std::thread::sleep(watcher::SWEEP);
    }
}

fn report() {
    let pids = win32::gfn_pids();
    println!("{} 程序：{} 個", win32::GFN_EXE, pids.len());
    if pids.is_empty() {
        println!("  GFN 沒開。偵測回 Absent 是對的。");
    } else {
        println!("  PID {pids:?}");
    }

    let Some(seen) = win32::sweep() else {
        println!("\n列舉視窗失敗。偵測會回 None，也就是「這次不算」，維持上一個狀態。");
        return;
    };

    println!("\n可見視窗裡，標題帶品牌字樣或由 GFN 擁有的：");
    if seen.is_empty() {
        println!("  一扇也沒有。");
        if !pids.is_empty() {
            println!("  但 GFN 程序在跑 —— 它現在沒有可見的頂層視窗。");
            println!("  縮到工作列時 IsWindowVisible 仍該回 true，所以這代表");
            println!("  它被收進系統匣，或者視窗屬於看不到的那一層。");
        }
    }
    for window in &seen {
        let owner = window.process.as_deref().unwrap_or("（查不到擁有者）");
        let verdict = match window.classified {
            Some(state) => format!("{state:?}"),
            None => "標題沒有品牌字樣".to_string(),
        };
        let counted = window
            .process
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(win32::GFN_EXE))
            && window.classified.is_some();
        println!(
            "  [{}] {:<22} {:?}\n      判讀：{}",
            if counted { "採用" } else { "略過" },
            owner,
            window.title,
            verdict
        );
    }

    println!("\n偵測結果：{:?}", win32::current_state());
    println!(
        "（正式路徑每 {} 秒掃一次，連續 {} 次相同才認定，所以實際反應最多慢 {} 秒。）",
        watcher::SWEEP.as_secs(),
        watcher::CONFIRM,
        (watcher::SWEEP * u32::from(watcher::CONFIRM)).as_secs()
    );
}
