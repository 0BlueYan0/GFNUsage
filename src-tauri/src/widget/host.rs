//! 擁有 widget 那扇視窗的執行緒。只有 Windows 有。
//!
//! 自己一條執行緒跑 `GetMessageW`，不用 Tauri 的事件迴圈：widget 是
//! `Shell_TrayWnd` 的子視窗，跨程序的父子視窗會把兩邊的輸入佇列接在一起
//! （Raymond Chen 2013-04-12），這條執行緒卡住，工作列就跟著卡。所以這裡
//! 不 await、不拿 `AppState` 的鎖、不做網路，點擊也只是把事件丟回主程式。
//!
//! 所有會動到視窗的事都在訊息迴圈的最外層做，兩個 wndproc 只負責把事件
//! post 回迴圈。跨程序的 `SetWindowPos` 會等 explorer 回應，等的期間送來的
//! 訊息會重新進入 wndproc，在那裡動執行緒狀態就是重入。點擊是例外，它在
//! wndproc 裡直接呼叫 `on_click`，因為那裡面的 `run_on_main_thread` 只是
//! post 給 Tauri 的事件迴圈，不等，也不碰 `Local`。

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, Once};

use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    GetWindow, IsWindow, IsWindowVisible, PeekMessageW, PostQuitMessage, PostThreadMessageW,
    RegisterClassW, RegisterWindowMessageW, SetTimer, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, GW_CHILD, HWND_TOP, MSG, PM_NOREMOVE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, ULW_ALPHA, WM_APP, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_DPICHANGED_AFTERPARENT, WM_LBUTTONUP, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER, WM_USER,
    WNDCLASSW, WS_CHILD, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_POPUP,
};

use super::face::WidgetFace;
use super::layout::{self, Placed, Side, Unplaced};
use super::render;
use super::taskbar::{self, wide};

#[derive(Debug, Clone, Copy)]
pub enum Button {
    Left,
    Right,
}

/// 在 host 執行緒上被呼叫，裡面只能把事件丟走。
pub type OnClick = Box<dyn Fn(Button, i32, i32) + Send + Sync>;

/// 主程式要 widget 長成什麼樣子。
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub face: WidgetFace,
    pub side: Side,
}

struct Shared {
    wanted: Mutex<Wanted>,
    on_click: OnClick,
}

/// 主程式手上的那一端。丟掉就關掉 widget。
pub struct Host {
    thread: u32,
    shared: Arc<Shared>,
    join: std::thread::JoinHandle<()>,
}

const UPDATE: u32 = WM_APP + 1;
const REBUILD: u32 = WM_APP + 2;
const QUIT: u32 = WM_APP + 3;

const CHILD_CLASS: &str = "GFNUsageTaskbarWidget";
const LISTENER_CLASS: &str = "GFNUsageTaskbarWidgetListener";

/// 位置多久重算一次。系統匣的圖示會增減，`TrayNotifyWnd` 的左緣會跟著動
/// （第 0 步看到 2170、2180、2140 來回變）。explorer 剛重啟時也靠它重試。
const TICK_MS: u32 = 2000;
const TICK_ID: usize = 1;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static REGISTER: Once = Once::new();

impl Host {
    pub fn start(wanted: Wanted, on_click: OnClick) -> Option<Host> {
        let shared = Arc::new(Shared {
            wanted: Mutex::new(wanted),
            on_click,
        });
        let (tx, rx) = mpsc::channel();
        let for_thread = Arc::clone(&shared);
        let join = std::thread::Builder::new()
            .name("taskbar-widget".into())
            .spawn(move || {
                // 先建好訊息佇列再把 thread id 交出去，不然第一個 PostThreadMessageW 會失敗。
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                unsafe {
                    PeekMessageW(
                        &mut msg,
                        std::ptr::null_mut(),
                        WM_USER,
                        WM_USER,
                        PM_NOREMOVE,
                    )
                };
                let _ = tx.send(unsafe { GetCurrentThreadId() });
                run(for_thread);
            });
        let join = match join {
            Ok(join) => join,
            Err(e) => {
                log::warn!("工作列 widget：執行緒開不起來 {e}");
                return None;
            }
        };
        let thread = rx.recv().ok()?;
        Some(Host {
            thread,
            shared,
            join,
        })
    }

    /// 執行緒還在跑。panic 或提早 return 之後是 false，`update` 送過去的訊息
    /// 沒有人收，widget 會停在畫面上消失的狀態（執行緒結束時它的視窗會被摧毀）。
    pub fn is_alive(&self) -> bool {
        !self.join.is_finished()
    }

    pub fn update(&self, wanted: Wanted) {
        {
            let mut current = self.shared.wanted.lock().unwrap();
            if *current == wanted {
                return;
            }
            *current = wanted;
        }
        post(self.thread, UPDATE);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        post(self.thread, QUIT);
    }
}

fn post(thread: u32, msg: u32) {
    unsafe { PostThreadMessageW(thread, msg, 0, 0) };
}

/// host 執行緒自己的狀態。只在訊息迴圈的最外層借用。
struct Local {
    shared: Arc<Shared>,
    listener: HWND,
    child: HWND,
    placed: Option<Placed>,
    /// 上一次畫上去的東西。一樣就不重畫。
    drawn: Option<(WidgetFace, i32, i32, u32, bool)>,
    /// 上一次擺不上去的原因。同一個原因只記一次日誌，不然每 2 秒一行。
    unplaced: Option<Unplaced>,
}

thread_local! {
    /// 只給 wndproc 拿點擊回呼用。跟 `Local` 分開，wndproc 重入時不會碰到借用中的 `Local`。
    static SHARED: RefCell<Option<Arc<Shared>>> = const { RefCell::new(None) };
}

fn register_classes(instance: *mut core::ffi::c_void) {
    REGISTER.call_once(|| {
        let name = wide("TaskbarCreated");
        TASKBAR_CREATED.store(
            unsafe { RegisterWindowMessageW(name.as_ptr()) },
            Ordering::Relaxed,
        );
        for (class, proc_) in [
            (
                CHILD_CLASS,
                child_proc as unsafe extern "system" fn(_, _, _, _) -> _,
            ),
            (LISTENER_CLASS, listener_proc),
        ] {
            let name = wide(class);
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(proc_),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: name.as_ptr(),
            };
            if unsafe { RegisterClassW(&wc) } == 0 {
                log::warn!("工作列 widget：RegisterClassW({class}) 失敗 {}", unsafe {
                    GetLastError()
                });
            }
        }
    });
}

fn run(shared: Arc<Shared>) {
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    register_classes(instance);
    SHARED.with(|s| *s.borrow_mut() = Some(Arc::clone(&shared)));

    // 頂層、不可見。`TaskbarCreated`、`WM_SETTINGCHANGE`、`WM_DISPLAYCHANGE` 都是
    // 廣播，只送頂層視窗，子視窗收不到。`HWND_MESSAGE` 那種也收不到廣播。
    let name = wide(LISTENER_CLASS);
    let listener = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if listener.is_null() {
        log::warn!("工作列 widget：listener 建不起來 {}", unsafe {
            GetLastError()
        });
        return;
    }
    if unsafe { SetTimer(listener, TICK_ID, TICK_MS, None) } == 0 {
        log::warn!("工作列 widget：SetTimer 失敗 {}", unsafe {
            GetLastError()
        });
    }

    let mut local = Local {
        shared,
        listener,
        child: std::ptr::null_mut(),
        placed: None,
        drawn: None,
        unplaced: None,
    };
    apply(&mut local);

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        match msg.message {
            UPDATE => apply(&mut local),
            REBUILD => {
                log::info!("工作列 widget：explorer 重啟，重建");
                // explorer 真的重啟過的話，舊的子視窗已經跟著被摧毀。只是有人廣播
                // `TaskbarCreated`（殼層替換工具會這樣）的話它還在，不收掉就會有
                // 兩個，舊的那個沒人重畫。
                if !local.child.is_null() && unsafe { IsWindow(local.child) } != 0 {
                    unsafe { DestroyWindow(local.child) };
                }
                local.child = std::ptr::null_mut();
                local.placed = None;
                local.drawn = None;
                apply(&mut local);
            }
            QUIT => {
                if !local.child.is_null() && unsafe { IsWindow(local.child) } != 0 {
                    unsafe { DestroyWindow(local.child) };
                }
                unsafe { DestroyWindow(local.listener) };
                unsafe { PostQuitMessage(0) };
            }
            WM_TIMER if msg.hwnd == local.listener => apply(&mut local),
            _ => unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            },
        }
    }
    SHARED.with(|s| *s.borrow_mut() = None);
}

fn apply(local: &mut Local) {
    let wanted = local.shared.wanted.lock().unwrap().clone();
    let t = taskbar::inspect();
    let dpi = t.dpi.unwrap_or(96);
    let light = t.light == Some(1);
    let width = render::width_for(dpi);

    // explorer 重啟但 `TaskbarCreated` 沒收到的話，靠這裡發現舊的已經不在。
    if !local.child.is_null() && unsafe { IsWindow(local.child) } == 0 {
        local.child = std::ptr::null_mut();
        local.placed = None;
        local.drawn = None;
    }

    let p = match layout::place(&t, wanted.side, width) {
        Ok(p) => p,
        Err(why) => {
            if !local.child.is_null() {
                unsafe { ShowWindow(local.child, SW_HIDE) };
            }
            if local.unplaced != Some(why) {
                log::info!("工作列 widget：擺不上去 {why:?}");
                local.unplaced = Some(why);
            }
            local.placed = None;
            return;
        }
    };
    local.unplaced = None;

    let bar = taskbar::bar_hwnd();
    if local.child.is_null() {
        let Some(child) = create(bar, p) else {
            return;
        };
        local.child = child;
        // 基準也要記。只記轉換的話，開了就一直正常的人日誌上一行都沒有，
        // 跟完全沒掛上分不出來（同 GFN 視窗偵測那條）。
        log::info!(
            "工作列 widget：掛上，{:?}{}，寬 {}px，dpi {dpi}",
            wanted.side,
            if p.fell_back {
                "（退回系統匣左邊）"
            } else {
                ""
            },
            p.w
        );
    }

    if local.placed != Some(p) {
        let ok = unsafe {
            SetWindowPos(
                local.child,
                HWND_TOP,
                p.x,
                p.y,
                p.w,
                p.h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
        if ok == 0 {
            log::warn!("工作列 widget：SetWindowPos 失敗 {}", unsafe {
                GetLastError()
            });
        }
        local.placed = Some(p);
    } else {
        raise(bar, local.child);
    }

    let key = (wanted.face.clone(), p.w, p.h, dpi, light);
    if local.drawn.as_ref() != Some(&key) {
        let pixels = render::render(&wanted.face, p.w, p.h, dpi, light);
        if paint(local.child, p.w, p.h, &pixels) {
            local.drawn = Some(key);
        }
    }
    if unsafe { IsWindowVisible(local.child) } == 0 {
        unsafe {
            ShowWindow(
                local.child,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
            )
        };
    }
}

fn create(bar: HWND, p: Placed) -> Option<HWND> {
    if bar.is_null() {
        return None;
    }
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let name = wide(CHILD_CLASS);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            name.as_ptr(),
            std::ptr::null(),
            WS_CHILD,
            p.x,
            p.y,
            p.w,
            p.h,
            bar,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        log::warn!("工作列 widget：CreateWindowExW 失敗 {}", unsafe {
            GetLastError()
        });
        return None;
    }
    Some(hwnd)
}

/// 排到兄弟視窗的最上層。
///
/// 新建的子視窗排在最底下，而 Win11 工作列的 XAML 內容
/// （`Windows.UI.Composition.DesktopWindowContentBridge`）蓋滿整條工作列，
/// 不拉上來就被它壓住，建好了也看不到（第 0 步實測）。explorer 之後也會把它
/// 壓回去，第 0 步看到自動隱藏時每 2 秒一次，所以每一輪都檢查。
fn raise(bar: HWND, child: HWND) {
    if bar.is_null() || unsafe { GetWindow(bar, GW_CHILD) } == child {
        return;
    }
    unsafe {
        SetWindowPos(
            child,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// 把預乘 alpha 的 BGRA 交給 `UpdateLayeredWindow`。
fn paint(hwnd: HWND, w: i32, h: i32, pixels: &[u8]) -> bool {
    let mut ok = false;
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            // 負的高度是由上往下。
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(
            mem,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if dib.is_null() || bits.is_null() {
            log::warn!("工作列 widget：CreateDIBSection 失敗 {}", GetLastError());
        } else {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
            let old = SelectObject(mem, dib);
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            ok = UpdateLayeredWindow(
                hwnd,
                screen,
                std::ptr::null(),
                &size,
                mem,
                &src,
                0,
                &blend,
                ULW_ALPHA,
            ) != 0;
            if !ok {
                log::warn!("工作列 widget：UpdateLayeredWindow 失敗 {}", GetLastError());
            }
            SelectObject(mem, old);
            DeleteObject(dib);
        }
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
    ok
}

fn click(button: Button) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    // 先把 Arc 拿出來再呼叫，呼叫期間不借用 `SHARED`。
    let shared = SHARED.with(|s| s.borrow().clone());
    if let Some(shared) = shared {
        (shared.on_click)(button, p.x, p.y);
    }
}

fn post_self(msg: u32) {
    post(unsafe { GetCurrentThreadId() }, msg);
}

unsafe extern "system" fn child_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_LBUTTONUP => {
            click(Button::Left);
            0
        }
        WM_RBUTTONUP => {
            click(Button::Right);
            0
        }
        // 子視窗收到的是 `WM_DPICHANGED_AFTERPARENT`，`WM_DPICHANGED` 只送頂層視窗。
        // 兩個都收，沒收到也有 2 秒一次的 tick 補上。
        WM_DPICHANGED | WM_DPICHANGED_AFTERPARENT => {
            post_self(UPDATE);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn listener_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if created != 0 && msg == created {
        post_self(REBUILD);
        return 0;
    }
    if msg == WM_SETTINGCHANGE || msg == WM_DISPLAYCHANGE {
        // 淺色／深色切換、解析度、縮放都從這裡來。
        post_self(UPDATE);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
