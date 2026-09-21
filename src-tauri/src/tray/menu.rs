//! 系統匣的右鍵選單。
//!
//! 獨立成一個模組是因為它要重建：查到新版本時多一行「更新到 x.y.z」，
//! 而 `TrayIcon::set_menu` 吃的是一份新的 `Menu`，沒有「插一列」這種操作。

use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Runtime};

pub const REFRESH_ID: &str = "refresh";
pub const QUIT_ID: &str = "quit";
pub const INSTALL_UPDATE_ID: &str = "install-update";

/// 組一份選單。`update` 有值時中間多一行安裝的入口。
pub fn build<R: Runtime>(app: &AppHandle<R>, update: Option<&str>) -> tauri::Result<Menu<R>> {
    let refresh = MenuItem::with_id(app, REFRESH_ID, "立即更新", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "結束", true, None::<&str>)?;
    let Some(version) = update else {
        return Menu::with_items(app, &[&refresh, &quit]);
    };
    let install = MenuItem::with_id(
        app,
        INSTALL_UPDATE_ID,
        format!("更新到 {version}"),
        true,
        None::<&str>,
    )?;
    Menu::with_items(app, &[&refresh, &install, &quit])
}

/// 換掉已經掛上去的那一份。系統匣還沒建起來就當作沒事。
pub fn rebuild<R: Runtime>(app: &AppHandle<R>, update: Option<&str>) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(super::TRAY_ID) else {
        return Ok(());
    };
    tray.set_menu(Some(build(app, update)?))
}
