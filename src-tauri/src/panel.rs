//! 面板視窗的定位與顯示。
//!
//! 放在 lib 而不是 `main.rs`：登入完成與首次啟動都要把面板叫出來，
//! 而那兩處都在指令端，碰不到 binary 裡的函式。

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewWindow};

pub const PANEL_LABEL: &str = "main";

/// 面板顯示出來了，前端該重讀一次資料。
///
/// 前端本來只靠 `document` 的 `visibilitychange`，而在 Windows 上那個事件
/// 不會來：`window.hide()` 走的是 `WindowMessage::Hide`，動到的是 tao 的原生
/// 視窗；會讓 `visibilityState` 變 hidden 的是 WebView2 控制器的 `IsVisible`，
/// 那要 `WebviewMessage::Hide` 才會碰，而這支程式沒有走那條路。
///
/// 症狀是面板停在啟動當下那一份：那時第一輪輪詢還沒回來，所以系統匣有數字、
/// 面板寫「沒有資料」，而且按了「立即更新」以外的方式都不會變。
pub const SHOWN_EVENT: &str = "panel-shown";

/// 後端剛抓到新資料，前端該重讀一次。
///
/// 沒有這個事件的話，面板只在被叫出來時重讀，開著的期間數字不會變。
/// GFN 視窗偵測會在一場玩完的兩分鐘後抓，而那時使用者多半正開著面板。
///
/// 收到這個事件只能重讀，不能再抓一次：抓完會再送一次事件，那是迴圈。
pub const REFRESHED_EVENT: &str = "data-refreshed";

/// 面板與螢幕可用區邊緣的間距。
const PANEL_MARGIN: i32 = 12;

/// 把面板貼到離 `near` 最近的可用區角落。
///
/// 用「最近的角落」而不是寫死右下角：工作列可以在四邊任何一側，而系統匣永遠
/// 緊鄰工作列，所以離點擊處最近的角落一定就是正確的那個角落。可用區
/// （work area）已經排除工作列本身，面板不會被壓在它底下。
pub fn position<R: Runtime>(window: &WebviewWindow<R>, near: PhysicalPosition<f64>) {
    let monitor = match window.app_handle().monitor_from_point(near.x, near.y) {
        Ok(Some(monitor)) => monitor,
        _ => match window.primary_monitor() {
            Ok(Some(monitor)) => monitor,
            _ => return,
        },
    };

    let area = monitor.work_area();
    let Ok(size) = window.outer_size() else {
        return;
    };

    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);
    let (ww, wh) = (size.width as i32, size.height as i32);

    let x = if (near.x as i32) > ax + aw / 2 {
        ax + aw - ww - PANEL_MARGIN
    } else {
        ax + PANEL_MARGIN
    };
    let y = if (near.y as i32) > ay + ah / 2 {
        ay + ah - wh - PANEL_MARGIN
    } else {
        ay + PANEL_MARGIN
    };

    // 面板比可用區還大時（極小螢幕），夾在區域內而不是跑到畫面外。
    let x = x.clamp(ax, (ax + aw - ww).max(ax));
    let y = y.clamp(ay, (ay + ah - wh).max(ay));

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

pub fn show<R: Runtime>(window: &WebviewWindow<R>, near: PhysicalPosition<f64>) {
    // 先定位再顯示，否則會在舊位置閃一下。
    position(window, near);
    let _ = window.show();
    let _ = window.set_focus();
    // 每一條把面板叫出來的路都經過這裡：系統匣點擊、首次啟動、登入完成、
    // 第二份程序。事件送不出去不算失敗 —— 那時畫面上是上一份資料，
    // 跟沒有這行一樣，不值得為它中斷顯示。
    let _ = window.emit(SHOWN_EVENT, ());
}

/// 沒有點擊座標時把面板叫出來（首次啟動、登入完成）。
///
/// 假裝點在主螢幕可用區的右下角：系統匣多半就在那裡，`position()` 的
/// 「最近的角落」邏輯會把面板貼到同一個角，看起來就跟使用者自己點開的一樣。
pub fn show_default<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    let near = match window.primary_monitor() {
        Ok(Some(monitor)) => {
            let area = monitor.work_area();
            PhysicalPosition::new(
                f64::from(area.position.x + area.size.width as i32),
                f64::from(area.position.y + area.size.height as i32),
            )
        }
        _ => PhysicalPosition::new(0.0, 0.0),
    };
    show(&window, near);
}
