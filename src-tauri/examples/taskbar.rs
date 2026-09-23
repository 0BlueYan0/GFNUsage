//! 工作列 widget 擺放的診斷版。
//!
//! ```
//! cargo run --example taskbar --manifest-path src-tauri/Cargo.toml
//! ```
//!
//! 使用者回報「widget 位置不對」「widget 沒出現」時跑這支。它印出 widget
//! 算位置時讀的每一樣東西，分得出：
//!
//! - 找不到 `Shell_TrayWnd`（explorer 沒在跑，或正在重啟）
//! - `TrayNotifyWnd` 寬度是 0（explorer 剛起來，24H2 開機時也會這樣）
//! - 工作列不在下方
//! - 登錄值跟畫面上看到的不一致（按鈕靠左／置中、小工具按鈕）
//!
//! 不掛視窗。這支 example 沒有 app 的 manifest，layered 子視窗在這裡建不起來，
//! 掛不掛得上要看 app 本身的日誌（`工作列 widget：` 開頭的那幾行）。
//! 只讀本機狀態，不發任何網路請求。
//!
//! 座標是實體像素：`GetWindowRect` 的回傳值跟著呼叫端的 DPI 感知走，這支
//! 程序沒有宣告 DPI 感知的話數字會被縮放過。對照時以 `dpi` 那一欄換算。

use gfnusage_lib::widget::taskbar;

fn main() {
    if !cfg!(windows) {
        println!("工作列 widget 只有 Windows 有實作。");
        return;
    }

    let t = taskbar::inspect();
    let show = |name: &str, r: Option<taskbar::Rect>| match r {
        Some(r) => println!(
            "{name:<14} ({}, {}) – ({}, {})  寬 {} 高 {}",
            r.left,
            r.top,
            r.right,
            r.bottom,
            r.width(),
            r.height()
        ),
        None => println!("{name:<14} 讀不到"),
    };

    show("Shell_TrayWnd", t.bar);
    show("TrayNotifyWnd", t.tray_notify);
    show("Start", t.start);
    println!("{:<14} {:?}", "dpi", t.dpi);
    println!(
        "{:<14} {:?}（0 靠左，1 或沒有值是置中）",
        "TaskbarAl", t.align
    );
    println!("{:<14} {:?}（1 顯示小工具按鈕）", "TaskbarDa", t.widgets);
    println!(
        "{:<14} {:?}（0 是原則關掉了小工具，TaskbarDa 不算數）",
        "Dsh 原則", t.widgets_policy
    );
    println!("{:<14} {:?}（1 淺色）", "LightTheme", t.light);

    if t.bar.is_none() {
        println!("\n找不到工作列。explorer 沒在跑，或正在重啟。");
    } else if t.tray_notify.is_some_and(|r| r.width() == 0) {
        println!("\nTrayNotifyWnd 寬度是 0。explorer 剛起來時會這樣，widget 會每 2 秒重試。");
    }
}
