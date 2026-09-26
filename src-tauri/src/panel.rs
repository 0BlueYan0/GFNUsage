//! 面板視窗的定位與顯示。
//!
//! 放在 lib 而不是 `main.rs`：登入完成與首次啟動都要把面板叫出來，
//! 而那兩處都在指令端，碰不到 binary 裡的函式。

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewWindow};

use crate::AppState;

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

/// 自動收起後多久之內的點擊，視為「關閉」而不是「開啟」。
///
/// 點圖示會先讓面板失焦，失焦處理器把它收起來，接著點擊事件才送到 —
/// 沒有這個寬限期，面板就會在同一次點擊中收起又立刻重開，變成關不掉。
/// 工作列 widget 也是同一個情況，所以放在這裡讓兩邊共用。
const REOPEN_GRACE: Duration = Duration::from_millis(300);

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

    // macOS 的選單列圖示擠在同一條上，貼角落的話面板離圖示可能隔了半個螢幕
    // （2026-09-26 回報）。水平置中在圖示下方，超出去的由下面的 clamp 推回來。
    let x = if cfg!(target_os = "macos") {
        (near.x as i32 - ww / 2).min(ax + aw - ww - PANEL_MARGIN)
    } else if (near.x as i32) > ax + aw / 2 {
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
    // macOS 的選單列在上面，右下角是錯的角（2026-09-26 登入完成後面板開在
    // 螢幕底下）。直接問選單列圖示在哪裡，跟點圖示叫出來的位置一樣。
    // Windows 不走這條：圖示收在溢位區時量到的是溢位區的位置。
    #[cfg(target_os = "macos")]
    if let Some(near) = tray_center(app, &window) {
        show(&window, near);
        return;
    }
    let near = match window.primary_monitor() {
        Ok(Some(monitor)) => {
            let area = monitor.work_area();
            // 查不到圖示時退回右上角，理由同上。
            let y = if cfg!(target_os = "macos") {
                area.position.y
            } else {
                area.position.y + area.size.height as i32
            };
            PhysicalPosition::new(
                f64::from(area.position.x + area.size.width as i32),
                f64::from(y),
            )
        }
        _ => PhysicalPosition::new(0.0, 0.0),
    };
    show(&window, near);
}

/// 選單列圖示的中心點，實體像素。
#[cfg(target_os = "macos")]
fn tray_center<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> Option<PhysicalPosition<f64>> {
    let rect = app.tray_by_id(crate::tray::TRAY_ID)?.rect().ok()??;
    let scale = window.scale_factor().unwrap_or(1.0);
    let at: PhysicalPosition<f64> = rect.position.to_physical(scale);
    let size: tauri::PhysicalSize<f64> = rect.size.to_physical(scale);
    Some(PhysicalPosition::new(
        at.x + size.width / 2.0,
        at.y + size.height / 2.0,
    ))
}

/// 點了系統匣圖示或工作列 widget。面板剛因為這一下失焦收起的話，這一下是關閉。
pub fn toggle_from_click<R: Runtime>(app: &AppHandle<R>, position: PhysicalPosition<f64>) {
    let state = app.state::<Arc<AppState>>();
    let since = state.last_auto_hide.lock().unwrap().map(|at| at.elapsed());
    let just_closed = since.is_some_and(|d| d < REOPEN_GRACE);
    // 點下去沒反應時要分得出是被寬限期吞掉、還是根本沒收到點擊，所以沒吞的也記。
    match since {
        Some(d) => log::info!(
            "面板：點擊，上次自動收起在 {} 毫秒前{}",
            d.as_millis(),
            if just_closed { "，當成關閉" } else { "" }
        ),
        None => log::info!("面板：點擊，沒有自動收起過"),
    }
    if just_closed {
        return;
    }
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    // macOS 點選單列圖示不會讓面板失焦，上面那段寬限期等不到，面板開著就要在
    // 這裡收。Windows 點下去之前面板已經因為失焦收掉，走不到這裡。
    if window.is_visible().unwrap_or(false) {
        log::info!("面板：點擊，面板開著，收起");
        let _ = window.hide();
        return;
    }
    show(&window, position);
}

/// 在 `at`（螢幕座標）叫出系統匣那一份選單。
///
/// 借面板那扇視窗來叫：`popup_menu_at` 要一扇視窗，它隱藏著也叫得出來（第 0 步實測）。
/// 座標要換成相對於那扇視窗，傳螢幕座標的話選單會畫到螢幕外面。
/// 選單事件進的是 `main.rs` 裡 `TrayIconBuilder::on_menu_event` 那一個 handler：
/// Tauri 把它註冊在全域的選單監聽清單裡（`tray/mod.rs` 的 `register`）。
pub fn popup_tray_menu<R: Runtime>(app: &AppHandle<R>, at: PhysicalPosition<i32>) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    let update = app
        .try_state::<crate::update::UpdateState>()
        .and_then(|u| u.available());
    let menu = match crate::tray::menu::build(app, update.as_deref()) {
        Ok(menu) => menu,
        Err(e) => {
            log::warn!("選單建不起來：{e}");
            return;
        }
    };
    let origin = window.inner_position().unwrap_or_default();
    let relative = PhysicalPosition::new(at.x - origin.x, at.y - origin.y);
    log::info!("面板：選單開");
    if let Err(e) = window.popup_menu_at(&menu, relative) {
        log::warn!("選單叫不出來：{e}");
    }
    log::info!("面板：選單關");
}
