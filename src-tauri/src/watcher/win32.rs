//! 列舉視窗、找出 GFN 客戶端的那一扇、讀它的標題。
//!
//! 作法來自參考專案 GeForce-NOW-Rich-Presence（`src/core/presence_manager.py`），
//! 但順序刻意反過來：它先列出所有 `GeForceNOW.exe` 的 PID，再對每扇視窗比對；
//! 這裡先看標題有沒有品牌字樣，有才去查那扇視窗的程序。GFN 沒開的時候
//! （也就是絕大多數時候）完全不必碰程序表。
//!
//! 另外它在第一扇屬於 GFN 的視窗就 `break`，而 CEF 的輔助視窗可能有空標題
//! 又剛好排在前面，於是整個偵測會誤判成「沒開」。這裡改成繼續往下找。
//!
//! `sweep()` 與 `gfn_pids()` 是給 `examples/gfnwindow.rs` 的診斷路徑，
//! 走的是同一份列舉與同一份判讀 —— 診斷用另一套程式碼的話，診斷過了不代表
//! 正式路徑也過。

use super::GfnState;

/// 診斷用：掃到的一扇視窗。
#[derive(Debug, Clone)]
pub struct Seen {
    pub title: String,
    /// 擁有它的執行檔名。查不到是 `None`。
    pub process: Option<String>,
    /// `title::classify` 的判讀。`None` 代表標題裡沒有品牌字樣。
    pub classified: Option<GfnState>,
}

#[cfg(windows)]
mod imp {
    use super::{GfnState, Seen};

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
    };

    /// GFN 是 CEF 應用，同名的程序有好幾個（主程序加上 GPU／算繪／工具）。
    /// 所以比對的是名字，不是某一個 PID。
    pub const GFN_EXE: &str = "GeForceNOW.exe";

    /// `EnumWindows` 的回呼。
    ///
    /// 裡面只做 `push`。這是 FFI 邊界，panic 跨過去是未定義行為，所以讀標題
    /// 與查程序那些會配置記憶體的事全部留到迴圈外面做。
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
        let list = &mut *(lparam as *mut Vec<HWND>);
        if IsWindowVisible(hwnd) != 0 {
            list.push(hwnd);
        }
        // TRUE：繼續列舉。
        1
    }

    fn visible_windows() -> Option<Vec<HWND>> {
        let mut windows: Vec<HWND> = Vec::new();
        // 回呼永遠回 TRUE，所以回傳 FALSE 只會是列舉本身失敗。
        let ok = unsafe { EnumWindows(Some(collect), &mut windows as *mut _ as LPARAM) };
        (ok != 0).then_some(windows)
    }

    pub fn current_state() -> Option<GfnState> {
        for hwnd in visible_windows()? {
            let Some(title) = window_title(hwnd) else {
                continue;
            };
            // 沒有品牌字樣就不是我們要的那扇，也就不必為它查程序。
            let Some(state) = super::super::title::classify(&title) else {
                continue;
            };
            // 瀏覽器開著 GFN 網頁版時標題也帶品牌字樣，靠這一步排掉。
            if process_name(hwnd).is_some_and(|name| name.eq_ignore_ascii_case(GFN_EXE)) {
                return Some(state);
            }
        }
        Some(GfnState::Absent)
    }

    /// 診斷用的一次掃描。
    ///
    /// 列出可見視窗裡「標題帶品牌字樣」或「由 `GeForceNOW.exe` 擁有」的那些。
    /// 比 `current_state` 貴得多 —— 每扇可見視窗都要查一次程序 —— 所以只給
    /// 診斷用，不放進 10 秒一次的迴圈。
    pub fn sweep() -> Option<Vec<Seen>> {
        let mut out = Vec::new();
        for hwnd in visible_windows()? {
            let title = window_title(hwnd).unwrap_or_default();
            let classified = super::super::title::classify(&title);
            let process = process_name(hwnd);
            let is_gfn = process
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(GFN_EXE));
            if classified.is_some() || is_gfn {
                out.push(Seen {
                    title,
                    process,
                    classified,
                });
            }
        }
        Some(out)
    }

    /// 診斷用：現在有哪些 `GeForceNOW.exe`。
    ///
    /// 和視窗分開問，才分得出「GFN 根本沒開」與「GFN 開著但沒有可見視窗」。
    pub fn gfn_pids() -> Vec<u32> {
        let mut pids = Vec::new();
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot.is_null() {
            return pids;
        }

        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
        while ok != 0 {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if name.eq_ignore_ascii_case(GFN_EXE) {
                pids.push(entry.th32ProcessID);
            }
            ok = unsafe { Process32NextW(snapshot, &mut entry) };
        }
        unsafe { CloseHandle(snapshot) };
        pids
    }

    fn window_title(hwnd: HWND) -> Option<String> {
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return None;
        }
        // +1 是結尾的 NUL，`GetWindowTextW` 一定會寫。
        let mut buf = vec![0u16; len as usize + 1];
        let written = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        if written <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..written as usize]))
    }

    /// 擁有這扇視窗的執行檔名，例如 `GeForceNOW.exe`。
    fn process_name(hwnd: HWND) -> Option<String> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == 0 {
            return None;
        }

        // LIMITED 就夠讀執行檔路徑，而且不需要提權 —— GFN 可能是用另一個
        // 完整性等級跑的。
        let handle: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }

        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(str::to_string)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{GfnState, Seen};

    pub const GFN_EXE: &str = "GeForceNOW.exe";

    /// macOS 上沒有等價的作法：讀別的 app 的視窗標題要輔助使用權限，
    /// 而為了一個「抓得比較準」的功能去要那個權限不划算。那邊的抓取
    /// 來源只剩定時、開面板、睡眠喚醒、手動。
    pub fn current_state() -> Option<GfnState> {
        None
    }

    pub fn sweep() -> Option<Vec<Seen>> {
        None
    }

    pub fn gfn_pids() -> Vec<u32> {
        Vec::new()
    }
}

pub use imp::{current_state, gfn_pids, sweep, GFN_EXE};
