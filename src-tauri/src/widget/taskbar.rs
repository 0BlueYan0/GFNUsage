//! 讀工作列的現況：幾扇視窗的 handle 與矩形、DPI、會影響擺放的登錄值。
//!
//! 只讀不改。正式路徑與 `examples/taskbar.rs` 走同一份，理由同
//! `watcher::win32::sweep()`：診斷用另一套程式碼的話，診斷過了不代表正式路徑也過。

/// 螢幕座標的矩形，單位是實體像素。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// 一次讀到的工作列狀態。讀不到的欄位是 `None`，不猜。
#[derive(Debug, Clone, Default)]
pub struct Taskbar {
    /// `Shell_TrayWnd`，主螢幕的工作列。
    pub bar: Option<Rect>,
    /// `TrayNotifyWnd`，系統匣與時鐘那一塊。24H2 開機時讀得到但寬度是 0（TrafficMonitor #2098）。
    pub tray_notify: Option<Rect>,
    /// `Start`，開始按鈕。
    pub start: Option<Rect>,
    /// 工作列那扇視窗的 DPI。100% 縮放是 96。
    pub dpi: Option<u32>,
    /// `TaskbarAl`：0 是按鈕靠左，1 是置中。沒有這個值時 Windows 11 當作置中。
    pub align: Option<u32>,
    /// `TaskbarDa`：1 是顯示小工具按鈕。
    pub widgets: Option<u32>,
    /// HKLM 的 `AllowNewsAndInterests` 原則：0 是小工具被關掉。這時設定頁的開關是灰的，
    /// `TaskbarDa` 就算是 1 也不會有按鈕（第 0 步那台機器就是這樣）。
    pub widgets_policy: Option<u32>,
    /// `SystemUsesLightTheme`：1 是淺色工作列。
    pub light: Option<u32>,
}

#[cfg(windows)]
mod imp {
    use super::{Rect, Taskbar};

    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD,
    };
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, GetWindowRect};

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 主螢幕的工作列。explorer 重啟中會是 null。
    pub fn bar_hwnd() -> HWND {
        let class = wide("Shell_TrayWnd");
        unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) }
    }

    pub fn child(parent: HWND, class: &str) -> HWND {
        let class = wide(class);
        unsafe {
            FindWindowExW(
                parent,
                std::ptr::null_mut(),
                class.as_ptr(),
                std::ptr::null(),
            )
        }
    }

    pub fn rect_of(hwnd: HWND) -> Option<Rect> {
        if hwnd.is_null() {
            return None;
        }
        let mut r: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
            return None;
        }
        Some(Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        })
    }

    fn dword(root: HKEY, subkey: &str, value: &str) -> Option<u32> {
        let subkey = wide(subkey);
        let value = wide(value);
        let mut data = 0u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                root,
                subkey.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                &mut data as *mut u32 as *mut _,
                &mut len,
            )
        };
        (status == 0).then_some(data)
    }

    const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
    const DSH_POLICY: &str = r"SOFTWARE\Policies\Microsoft\Dsh";
    const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    pub fn inspect() -> Taskbar {
        let bar = bar_hwnd();
        let (tray_notify, start, dpi) = if bar.is_null() {
            (None, None, None)
        } else {
            let dpi = unsafe { GetDpiForWindow(bar) };
            (
                rect_of(child(bar, "TrayNotifyWnd")),
                rect_of(child(bar, "Start")),
                (dpi != 0).then_some(dpi),
            )
        };
        Taskbar {
            bar: rect_of(bar),
            tray_notify,
            start,
            dpi,
            align: dword(HKEY_CURRENT_USER, ADVANCED, "TaskbarAl"),
            widgets: dword(HKEY_CURRENT_USER, ADVANCED, "TaskbarDa"),
            widgets_policy: dword(HKEY_LOCAL_MACHINE, DSH_POLICY, "AllowNewsAndInterests"),
            light: dword(HKEY_CURRENT_USER, PERSONALIZE, "SystemUsesLightTheme"),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Taskbar;

    /// macOS 沒有工作列。
    pub fn inspect() -> Taskbar {
        Taskbar::default()
    }
}

pub use imp::inspect;
#[cfg(windows)]
pub(crate) use imp::{bar_hwnd, wide};
