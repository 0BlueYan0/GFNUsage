//! widget 要擺在工作列的哪裡。純函式，吃 `taskbar::inspect()` 讀到的東西。
//!
//! 回傳的座標相對於工作列左上角，因為 widget 是 `Shell_TrayWnd` 的子視窗。

use serde::{Deserialize, Serialize};

use super::taskbar::{Rect, Taskbar};

/// 使用者選的那一側。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// 系統匣左邊。按鈕置中時這裡通常是空的。
    #[default]
    TrayLeft,
    /// 工作列最左邊。
    TaskbarLeft,
}

/// 相對於工作列左上角，實體像素。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// 選了工作列最左邊，但那一側被佔住，退回系統匣左邊。
    pub fell_back: bool,
}

/// 擺不上去的原因。寫進日誌用，所以每一種都要分得出來。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unplaced {
    /// 找不到 `Shell_TrayWnd`，explorer 沒在跑或正在重啟。
    NoTaskbar,
    /// 工作列不在下方。上／左／右還沒做。
    NotAtBottom,
    /// `TrayNotifyWnd` 讀不到或寬度是 0。explorer 剛重啟時會這樣（第 0 步實測約 2 秒），
    /// 24H2 開機時也會（TrafficMonitor #2098）。下一輪再試。
    TrayNotSettled,
    /// 算出來的位置跑到工作列外面。
    NoRoom,
}

/// 系統匣左緣與 widget 之間的間距，邏輯像素。
const GAP: i32 = 4;
/// widget 上下各留的空白，邏輯像素。
const INSET: i32 = 4;
/// 小工具按鈕佔的寬度，邏輯像素。
///
/// 沒有量過，160 是猜的。第 0 步那台機器的小工具被 `AllowNewsAndInterests`
/// 原則關掉，看不到按鈕。按鈕在 XAML 裡，沒有 HWND 可以讀矩形。2026-09-23
/// 決定先用這個數字，不為了量它去改系統原則，有人回報重疊或空太多再改。
/// 只影響選了「工作列最左邊」而且小工具按鈕有顯示的人。
const WIDGETS_BUTTON: i32 = 160;

pub fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

/// `width` 是 widget 的寬度，實體像素（`render::width_for` 算的）。
pub fn place(t: &Taskbar, side: Side, width: i32) -> Result<Placed, Unplaced> {
    let bar = t.bar.ok_or(Unplaced::NoTaskbar)?;
    // 主螢幕的原點一定是 (0, 0)，所以橫的、而且不貼頂端，就是在下方。
    if bar.width() <= bar.height() || bar.top <= 0 {
        return Err(Unplaced::NotAtBottom);
    }
    let tray = t
        .tray_notify
        .filter(|r: &Rect| r.width() > 0)
        .ok_or(Unplaced::TrayNotSettled)?;
    let dpi = t.dpi.unwrap_or(96);

    let h = bar.height() - 2 * scale(INSET, dpi);
    let y = (bar.height() - h) / 2;
    let at_tray = tray.left - bar.left - width - scale(GAP, dpi);

    // `TaskbarAl` 沒有值時 Windows 11 當作置中。
    let left_aligned = t.align == Some(0);
    let (x, fell_back) = match side {
        Side::TrayLeft => (at_tray, false),
        Side::TaskbarLeft if left_aligned => (at_tray, true),
        Side::TaskbarLeft => {
            // 原則關掉的話設定頁那個開關是灰的，`TaskbarDa` 不算數。
            let shown = t.widgets == Some(1) && t.widgets_policy != Some(0);
            let skip = if shown { WIDGETS_BUTTON } else { 0 };
            let x = scale(skip + GAP, dpi);
            // 登錄值判不出來的靠左，矩形判得出來：Windows 10 沒有 `TaskbarAl`，
            // 開始按鈕就貼著左緣。StartAllBack 這類工具把按鈕靠左也不一定寫那個值。
            // 置中但按鈕多到那一群頂到左緣，也算在這裡。
            let taken = t.start.is_some_and(|s| s.left - bar.left < x + width);
            if taken {
                (at_tray, true)
            } else {
                (x, false)
            }
        }
    };

    if x < 0 || x + width > tray.left - bar.left || h <= 0 {
        return Err(Unplaced::NoRoom);
    }
    Ok(Placed {
        x,
        y,
        w: width,
        h,
        fell_back,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 第 0 步那台機器讀到的數字：2560×1440，125%，按鈕置中。
    fn dev_machine() -> Taskbar {
        Taskbar {
            bar: Some(Rect {
                left: 0,
                top: 1370,
                right: 2560,
                bottom: 1440,
            }),
            tray_notify: Some(Rect {
                left: 2170,
                top: 1370,
                right: 2560,
                bottom: 1440,
            }),
            start: Some(Rect {
                left: 701,
                top: 1370,
                right: 758,
                bottom: 1440,
            }),
            dpi: Some(120),
            align: None,
            widgets: Some(1),
            // 這台機器被原則關掉了小工具，所以 `TaskbarDa` 是 1 也看不到按鈕。
            widgets_policy: Some(0),
            light: Some(0),
        }
    }

    #[test]
    fn tray_left_sits_just_left_of_the_tray() {
        let p = place(&dev_machine(), Side::TrayLeft, 150).unwrap();
        // 2170 - 150 - scale(4, 120) = 2015，第 0 步探測擺的位置。
        assert_eq!(p.x, 2015);
        assert_eq!(p.w, 150);
        // 高度扣掉上下各 5（scale(4, 120)），垂直置中。
        assert_eq!(p.h, 60);
        assert_eq!(p.y, 5);
        assert!(!p.fell_back);
    }

    #[test]
    fn taskbar_left_skips_the_widgets_button() {
        let mut t = dev_machine();
        t.widgets_policy = None;
        let p = place(&t, Side::TaskbarLeft, 150).unwrap();
        assert_eq!(p.x, scale(WIDGETS_BUTTON + GAP, 120));
        assert!(!p.fell_back);
    }

    #[test]
    fn taskbar_left_without_the_widgets_button_starts_at_the_edge() {
        let mut t = dev_machine();
        t.widgets = Some(0);
        let p = place(&t, Side::TaskbarLeft, 150).unwrap();
        assert_eq!(p.x, scale(GAP, 120));
    }

    /// 第 0 步那台機器：`TaskbarDa` 是 1，但 `AllowNewsAndInterests` 原則是 0，
    /// 設定頁的開關是灰的，工作列上沒有按鈕。只看 `TaskbarDa` 會空出 205px。
    #[test]
    fn a_policy_that_turns_widgets_off_means_no_button() {
        let p = place(&dev_machine(), Side::TaskbarLeft, 150).unwrap();
        assert_eq!(p.x, scale(GAP, 120));
    }

    /// 原則明確允許的話照 `TaskbarDa` 走。
    #[test]
    fn a_policy_that_allows_widgets_defers_to_the_user_setting() {
        let mut t = dev_machine();
        t.widgets_policy = Some(1);
        let p = place(&t, Side::TaskbarLeft, 150).unwrap();
        assert_eq!(p.x, scale(WIDGETS_BUTTON + GAP, 120));
    }

    /// 按鈕靠左時開始按鈕就在最左邊，那一側被佔住。
    #[test]
    fn left_aligned_buttons_push_the_widget_back_to_the_tray() {
        let mut t = dev_machine();
        t.align = Some(0);
        let p = place(&t, Side::TaskbarLeft, 150).unwrap();
        assert_eq!(p.x, 2015);
        assert!(p.fell_back);
    }

    /// Windows 10：沒有 `TaskbarAl`，開始按鈕貼著左緣。只看登錄值會把 widget 蓋在它上面。
    #[test]
    fn a_start_button_at_the_left_edge_pushes_the_widget_back_to_the_tray() {
        let mut t = dev_machine();
        t.align = None;
        t.start = Some(Rect {
            left: 0,
            top: 1370,
            right: 60,
            bottom: 1440,
        });
        let p = place(&t, Side::TaskbarLeft, 150).unwrap();
        assert_eq!(p.x, 2015);
        assert!(p.fell_back);
    }

    /// 置中，但按鈕多到那一群的左緣壓進 widget 的範圍。
    #[test]
    fn centered_buttons_reaching_the_widget_push_it_back_to_the_tray() {
        let mut t = dev_machine();
        // widget 佔 5 到 155，開始按鈕從 120 起。
        t.start = Some(Rect {
            left: 120,
            top: 1370,
            right: 180,
            bottom: 1440,
        });
        assert!(place(&t, Side::TaskbarLeft, 150).unwrap().fell_back);
        // 剛好接在 widget 右邊不算壓到。
        t.start = Some(Rect {
            left: 155,
            top: 1370,
            right: 215,
            bottom: 1440,
        });
        assert!(!place(&t, Side::TaskbarLeft, 150).unwrap().fell_back);
    }

    #[test]
    fn no_taskbar_is_reported() {
        let t = Taskbar::default();
        assert_eq!(place(&t, Side::TrayLeft, 150), Err(Unplaced::NoTaskbar));
    }

    #[test]
    fn a_taskbar_at_the_top_is_not_handled() {
        let mut t = dev_machine();
        t.bar = Some(Rect {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 70,
        });
        assert_eq!(place(&t, Side::TrayLeft, 150), Err(Unplaced::NotAtBottom));
    }

    #[test]
    fn a_taskbar_on_the_side_is_not_handled() {
        let mut t = dev_machine();
        t.bar = Some(Rect {
            left: 0,
            top: 0,
            right: 70,
            bottom: 1440,
        });
        assert_eq!(place(&t, Side::TrayLeft, 150), Err(Unplaced::NotAtBottom));
    }

    /// 第 0 步重啟 explorer 時讀到的：`TrayNotifyWnd` 在，但寬度是 0。
    #[test]
    fn an_empty_tray_means_try_again_later() {
        let mut t = dev_machine();
        t.tray_notify = Some(Rect {
            left: 0,
            top: 1370,
            right: 0,
            bottom: 1370,
        });
        assert_eq!(
            place(&t, Side::TrayLeft, 150),
            Err(Unplaced::TrayNotSettled)
        );
        t.tray_notify = None;
        assert_eq!(
            place(&t, Side::TrayLeft, 150),
            Err(Unplaced::TrayNotSettled)
        );
    }

    /// 工作列最左邊不看系統匣，但系統匣還沒好的話也不擺。explorer 剛起來時
    /// 其他東西也還在排，擺上去的位置會被蓋掉。
    #[test]
    fn taskbar_left_also_waits_for_the_tray() {
        let mut t = dev_machine();
        t.tray_notify = None;
        assert_eq!(
            place(&t, Side::TaskbarLeft, 150),
            Err(Unplaced::TrayNotSettled)
        );
    }

    #[test]
    fn a_widget_wider_than_the_space_is_refused() {
        assert_eq!(
            place(&dev_machine(), Side::TrayLeft, 3000),
            Err(Unplaced::NoRoom)
        );
    }

    #[test]
    fn scales_with_dpi() {
        let mut t = dev_machine();
        t.dpi = Some(144);
        let p = place(&t, Side::TrayLeft, 150).unwrap();
        assert_eq!(p.x, 2170 - 150 - scale(GAP, 144));
        assert_eq!(p.h, 70 - 2 * scale(INSET, 144));
    }

    /// DPI 讀不到就當 100%，不要讓整個 widget 消失。
    #[test]
    fn a_missing_dpi_is_treated_as_100_percent() {
        let mut t = dev_machine();
        t.dpi = None;
        let p = place(&t, Side::TrayLeft, 150).unwrap();
        assert_eq!(p.x, 2170 - 150 - GAP);
    }

    #[test]
    fn side_serializes_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&Side::TrayLeft).unwrap(),
            "\"trayLeft\""
        );
        assert_eq!(
            serde_json::to_string(&Side::TaskbarLeft).unwrap(),
            "\"taskbarLeft\""
        );
    }
}
