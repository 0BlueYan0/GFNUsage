use ab_glyph::{point, Font, FontRef, OutlinedGlyph, PxScale, Rect, ScaleFont};

use crate::quota::DisplayState;

pub const ICON_SIZE: u32 = 32;

/// 圖示四周保留的空白，避免字緊貼邊緣。
const PADDING: f32 = 1.0;

/// 自動縮放的上下限。從大往小試，取第一個塞得下的。
const MAX_PX: u32 = 30;
const MIN_PX: u32 = 8;

/// 內嵌字型，避免依賴系統字型探索 —— 系統匣圖示需要每台機器長得一樣。
const FONT_DATA: &[u8] = include_bytes!("../../assets/Roboto-Bold.ttf");

/// 各顯示狀態的顏色。超前消耗與已用完同為紅色（spec §7.2），
/// 兩者靠驚嘆號區分。
pub fn state_color(state: DisplayState) -> [u8; 3] {
    match state {
        DisplayState::Normal => [236, 236, 236],
        DisplayState::Low => [245, 158, 11],
        DisplayState::OverPace => [239, 68, 68],
        DisplayState::Exhausted => [239, 68, 68],
        DisplayState::FreeTier => [148, 163, 184],
    }
}

/// 資料過期時用的變淡顏色：每個色版乘 0.55，暗得看得出來、又不至於消失。
pub fn dimmed(color: [u8; 3]) -> [u8; 3] {
    color.map(|c| (c as u16 * 55 / 100) as u8)
}

/// 以基線 y=0、起筆 x=0 排版，回傳所有字形與其外框聯集。
///
/// 用實際外框而非字型的 advance／ascent 來定位：數字沒有下伸部，
/// 用字型度量置中會讓字看起來偏高，而 advance 也比實際墨跡寬。
fn layout(font: &FontRef, text: &str, px: f32) -> (Vec<OutlinedGlyph>, Option<Rect>) {
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);

    let mut glyphs = Vec::new();
    let mut bounds: Option<Rect> = None;
    let mut pen_x = 0.0;
    let mut previous: Option<char> = None;

    for ch in text.chars() {
        let id = font.glyph_id(ch);
        if let Some(prev) = previous {
            pen_x += scaled.kern(font.glyph_id(prev), id);
        }

        if let Some(outlined) =
            font.outline_glyph(id.with_scale_and_position(scale, point(pen_x, 0.0)))
        {
            let b = outlined.px_bounds();
            bounds = Some(match bounds {
                None => b,
                Some(acc) => Rect {
                    min: point(acc.min.x.min(b.min.x), acc.min.y.min(b.min.y)),
                    max: point(acc.max.x.max(b.max.x), acc.max.y.max(b.max.y)),
                },
            });
            glyphs.push(outlined);
        }

        pen_x += scaled.h_advance(id);
        previous = Some(ch);
    }

    (glyphs, bounds)
}

/// 把文字盡可能大地置中繪製成 ICON_SIZE 見方的 RGBA 圖示。
///
/// 字級由大往小試到塞得下為止，所以 "9" 會佔滿整格，而 "103" 自動縮到剛好。
pub fn render(text: &str, color: [u8; 3]) -> Vec<u8> {
    let mut buffer = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    if text.is_empty() {
        return buffer;
    }

    let font = FontRef::try_from_slice(FONT_DATA).expect("內嵌字型應可解析");
    let limit = ICON_SIZE as f32 - PADDING * 2.0;

    let fitted = (MIN_PX..=MAX_PX).rev().find_map(|px| {
        let (glyphs, bounds) = layout(&font, text, px as f32);
        let b = bounds?;
        (b.width() <= limit && b.height() <= limit).then_some((glyphs, b))
    });

    let Some((glyphs, bounds)) = fitted else {
        return buffer;
    };

    // 依實際墨跡外框置中，而不是依字型度量。
    let dx = (ICON_SIZE as f32 - bounds.width()) / 2.0 - bounds.min.x;
    let dy = (ICON_SIZE as f32 - bounds.height()) / 2.0 - bounds.min.y;

    for outlined in glyphs {
        let origin = outlined.px_bounds().min;
        outlined.draw(|gx, gy, coverage| {
            let x = (origin.x + dx).round() as i32 + gx as i32;
            let y = (origin.y + dy).round() as i32 + gy as i32;
            if x < 0 || y < 0 || x >= ICON_SIZE as i32 || y >= ICON_SIZE as i32 {
                return;
            }
            let alpha = (coverage * 255.0).clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                return;
            }
            let offset = ((y as u32 * ICON_SIZE + x as u32) * 4) as usize;
            buffer[offset] = color[0];
            buffer[offset + 1] = color[1];
            buffer[offset + 2] = color[2];
            buffer[offset + 3] = buffer[offset + 3].max(alpha);
        });
    }

    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::DisplayState;

    const WHITE: [u8; 3] = [255, 255, 255];

    fn opaque_pixels(rgba: &[u8]) -> usize {
        rgba.chunks_exact(4).filter(|px| px[3] > 0).count()
    }

    /// 回傳有墨跡的像素範圍 (min_x, min_y, max_x, max_y)。
    fn ink_bounds(rgba: &[u8]) -> (u32, u32, u32, u32) {
        let (mut x0, mut y0, mut x1, mut y1) = (ICON_SIZE, ICON_SIZE, 0u32, 0u32);
        for y in 0..ICON_SIZE {
            for x in 0..ICON_SIZE {
                if rgba[((y * ICON_SIZE + x) * 4 + 3) as usize] > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0, y0, x1, y1)
    }

    #[test]
    fn renders_buffer_of_expected_size() {
        assert_eq!(render("103", WHITE).len() as u32, ICON_SIZE * ICON_SIZE * 4);
    }

    #[test]
    fn renders_visible_pixels() {
        assert!(
            opaque_pixels(&render("103", WHITE)) > 0,
            "圖示不應該是全透明的"
        );
    }

    #[test]
    fn wider_text_covers_more_pixels_than_narrower() {
        let one = opaque_pixels(&render("1", WHITE));
        let three = opaque_pixels(&render("103", WHITE));
        assert!(three > one, "三位數 {three} 應該比一位數 {one} 佔更多像素");
    }

    #[test]
    fn empty_text_renders_nothing() {
        assert_eq!(opaque_pixels(&render("", WHITE)), 0);
    }

    /// 這條是 icon 真正的品質關卡：不能碰到邊界，碰到就代表被裁掉了。
    #[test]
    fn never_clips_at_the_edges() {
        for text in ["1", "9", "87", "103", "115", "–", "!"] {
            let rgba = render(text, WHITE);
            let (x0, y0, x1, y1) = ink_bounds(&rgba);
            assert!(x0 >= 1, "{text}：左邊被裁（x0={x0}）");
            assert!(y0 >= 1, "{text}：上方被裁（y0={y0}）");
            assert!(x1 <= ICON_SIZE - 2, "{text}：右邊被裁（x1={x1}）");
            assert!(y1 <= ICON_SIZE - 2, "{text}：下方被裁（y1={y1}）");
        }
    }

    /// 自動縮放要真的把空間用掉，否則三位數會小到看不清。
    #[test]
    fn fills_most_of_the_available_width() {
        for text in ["87", "103", "115"] {
            let (x0, _, x1, _) = ink_bounds(&render(text, WHITE));
            let width = x1 - x0 + 1;
            assert!(width >= ICON_SIZE * 3 / 4, "{text}：只佔了 {width}px，太小");
        }
    }

    /// 墨跡應該置中，左右留白差距不該超過 1px。
    #[test]
    fn centers_horizontally() {
        for text in ["1", "87", "103"] {
            let (x0, _, x1, _) = ink_bounds(&render(text, WHITE));
            let left = x0 as i32;
            let right = (ICON_SIZE - 1 - x1) as i32;
            assert!(
                (left - right).abs() <= 1,
                "{text}：左留白 {left}px、右留白 {right}px，沒有置中"
            );
        }
    }

    #[test]
    fn each_state_has_a_distinct_color() {
        // 超前消耗與已用完刻意同為紅色（spec §7.2），兩者靠驚嘆號區分，
        // 所以這裡只取其中一個代表紅色那一組。
        let colors = [
            state_color(DisplayState::Normal),
            state_color(DisplayState::Low),
            state_color(DisplayState::Exhausted),
            state_color(DisplayState::FreeTier),
        ];
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "狀態 {i} 與 {j} 的顏色重複了");
            }
        }
        assert_eq!(
            state_color(DisplayState::OverPace),
            state_color(DisplayState::Exhausted),
            "spec §7.2：兩者都是紅色"
        );
    }

    /// 除錯用：把幾個代表性的圖示傾印成原始 RGBA，供人眼檢查。
    /// 平時不跑：`cargo test -- --ignored dumps_icons`
    #[test]
    #[ignore]
    fn dumps_icons_for_visual_inspection() {
        let dir = std::env::temp_dir().join("gfnusage-icons");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text, state) in [
            ("a-exhausted-0", "0", DisplayState::Exhausted),
            ("b-free", "\u{2013}", DisplayState::FreeTier),
            ("c-low-4", "4", DisplayState::Low),
            ("d-two-digit-87", "87", DisplayState::Normal),
            ("e-normal-103", "103", DisplayState::Normal),
        ] {
            let rgba = render(text, state_color(state));
            std::fs::write(dir.join(format!("{name}.rgba")), &rgba).unwrap();
        }
        println!("dumped to {}", dir.display());
    }

    /// 變淡的顏色要看得出和原色不同，但也不能淡到看不見。
    #[test]
    fn dimmed_color_is_darker_but_still_visible() {
        let base = state_color(DisplayState::Normal);
        let dim = dimmed(base);
        assert!(
            dim.iter().zip(base.iter()).all(|(d, b)| d < b),
            "{dim:?} 沒有比 {base:?} 暗"
        );
        assert!(dim.iter().all(|&c| c >= 64), "{dim:?} 太暗");
    }
}
