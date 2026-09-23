//! 把 `WidgetFace` 畫成 `UpdateLayeredWindow` 要的點陣圖。純函式。
//!
//! 格式是由上往下、每像素 BGRA、預乘 alpha。背景是 `HIT_ALPHA`，看不出來，
//! 但整塊都點得到。

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};

use super::face::{Tone, WidgetFace};
use super::layout::scale;
use crate::quota::DisplayState;
use crate::tray::icon::state_color;

/// 中文只用得到「小時」「分鐘」四個字，子集只收這四個（`assets/NotoSansTC-OFL.txt`）。
/// 其他字元（數字、空白、「–」「!」）走 Roboto。
const CJK_FONT: &[u8] = include_bytes!("../../assets/NotoSansTC-Bold-subset.otf");

/// 寬度固定成這一串的寬度。跟著文字變的話每分鐘寬度變一次，每次都要
/// `SetWindowPos`，旁邊的東西會跟著動。
pub const WIDEST: &str = "999 小時 59 分鐘";

/// 背景的 alpha。layered 視窗 alpha 0 的像素點擊會穿透到工作列，字與進度條
/// 中間那條空白就點不到面板（2026-09-23 實機回報）。1/255、顏色 0，疊在
/// 工作列上只暗 0.4%，看不出來。
const HIT_ALPHA: u8 = 1;

/// 以下都是邏輯像素，畫的時候乘上 DPI。
const TEXT_PX: f32 = 15.0;
/// 同一個 px 下，CJK 字的墨跡比 Roboto 的數字高。縮一點兩邊看起來才一樣大。
const CJK_SCALE: f32 = 0.85;
const PAD_X: i32 = 6;
const BAR_H: i32 = 4;
const BAR_GAP: i32 = 4;
/// 進度條底軌的不透明度，0–255。
const TRACK_ALPHA: f32 = 64.0;
/// 資料過期時整張圖的不透明度倍率。系統匣是把顏色乘 0.55，這裡改乘 alpha：
/// 淺色工作列上把字色調暗反而更顯眼。
const STALE_ALPHA: f32 = 0.55;

/// 淺色工作列上 Normal 的字色。系統匣那組 `[236,236,236]` 是給深色工作列的，
/// 放在淺色工作列上看不到。
const NORMAL_ON_LIGHT: [u8; 3] = [32, 32, 32];

/// 色調換成顏色。只有 Normal 分深淺，其他沿用系統匣的顏色（`tray::icon::state_color`）。
pub fn color(tone: Tone, light: bool) -> [u8; 3] {
    match tone {
        Tone::Normal if light => NORMAL_ON_LIGHT,
        Tone::Normal => state_color(DisplayState::Normal),
        Tone::Low => state_color(DisplayState::Low),
        Tone::Alert => state_color(DisplayState::Exhausted),
        Tone::Muted => state_color(DisplayState::FreeTier),
    }
}

struct Fonts {
    latin: FontRef<'static>,
    cjk: FontRef<'static>,
}

fn fonts() -> Fonts {
    Fonts {
        latin: FontRef::try_from_slice(crate::tray::icon::FONT_DATA).expect("內嵌字型應可解析"),
        cjk: FontRef::try_from_slice(CJK_FONT).expect("內嵌字型應可解析"),
    }
}

/// 一個字元該用哪一套字型、多大。兩套都沒有就是 `None`。
fn pick(fonts: &Fonts, ch: char, px: f32) -> Option<(&FontRef<'static>, PxScale)> {
    if fonts.latin.glyph_id(ch).0 != 0 {
        Some((&fonts.latin, PxScale::from(px)))
    } else if fonts.cjk.glyph_id(ch).0 != 0 {
        Some((&fonts.cjk, PxScale::from(px * CJK_SCALE)))
    } else {
        None
    }
}

fn text_px(dpi: u32) -> f32 {
    TEXT_PX * dpi as f32 / 96.0
}

/// 文字的前進寬度，實體像素。
fn measure(fonts: &Fonts, text: &str, px: f32) -> f32 {
    text.chars()
        .filter_map(|ch| pick(fonts, ch, px).map(|(f, s)| f.as_scaled(s).h_advance(f.glyph_id(ch))))
        .sum()
}

/// widget 的寬度，實體像素。
pub fn width_for(dpi: u32) -> i32 {
    let fonts = fonts();
    measure(&fonts, WIDEST, text_px(dpi)).ceil() as i32 + 2 * scale(PAD_X, dpi)
}

/// 預乘 alpha 的 over。`a` 是 0–1 的覆蓋率乘上不透明度。
fn blend(buf: &mut [u8], w: i32, h: i32, x: i32, y: i32, color: [u8; 3], a: f32) {
    if x < 0 || y < 0 || x >= w || y >= h || a <= 0.0 {
        return;
    }
    let a = a.min(1.0);
    let i = ((y * w + x) * 4) as usize;
    // BGRA
    let src = [color[2], color[1], color[0]];
    for c in 0..3 {
        let s = src[c] as f32 * a;
        buf[i + c] = (s + buf[i + c] as f32 * (1.0 - a)).round() as u8;
    }
    buf[i + 3] = (a * 255.0 + buf[i + 3] as f32 * (1.0 - a)).round() as u8;
}

/// 圓頭長條的覆蓋率：到中線段的距離小於半徑的地方算在裡面，邊緣半個像素做反鋸齒。
#[allow(clippy::too_many_arguments)]
fn capsule(
    buf: &mut [u8],
    w: i32,
    h: i32,
    x0: f32,
    x1: f32,
    top: f32,
    bar_h: f32,
    color: [u8; 3],
    alpha: f32,
) {
    if x1 <= x0 {
        return;
    }
    let r = bar_h / 2.0;
    let cy = top + r;
    // 比直徑還短時中線段縮成一點，畫出來是圓點，不會超出範圍。
    let (s0, s1) = if x1 - x0 < bar_h {
        let m = (x0 + x1) / 2.0;
        (m, m)
    } else {
        (x0 + r, x1 - r)
    };
    let r = r.min((x1 - x0) / 2.0);
    for py in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
        for px in x0.floor() as i32..=x1.ceil() as i32 {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let dx = if fx < s0 {
                s0 - fx
            } else if fx > s1 {
                fx - s1
            } else {
                0.0
            };
            let dy = fy - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let cover = (r - d + 0.5).clamp(0.0, 1.0);
            blend(buf, w, h, px, py, color, cover * alpha);
        }
    }
}

pub fn render(face: &WidgetFace, w: i32, h: i32, dpi: u32, light: bool) -> Vec<u8> {
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    let mut buf = [0, 0, 0, HIT_ALPHA].repeat((w * h) as usize);
    let fonts = fonts();
    let px = text_px(dpi);
    let opacity = if face.stale { STALE_ALPHA } else { 1.0 };
    let rgb = color(face.tone, light);

    let latin = fonts.latin.as_scaled(PxScale::from(px));
    let line = latin.ascent() - latin.descent();
    let bar_h = scale(BAR_H, dpi) as f32;
    let gap = scale(BAR_GAP, dpi) as f32;
    // 沒有進度條時字單獨置中，不留一塊空的位置給它。
    let content = if face.fill.is_some() {
        line + gap + bar_h
    } else {
        line
    };
    let top = ((h as f32 - content) / 2.0).round();
    let baseline = top + latin.ascent();

    let mut pen = ((w as f32 - measure(&fonts, &face.text, px)) / 2.0).round();
    for ch in face.text.chars() {
        let Some((font, s)) = pick(&fonts, ch, px) else {
            continue;
        };
        let id = font.glyph_id(ch);
        if let Some(outlined) =
            font.outline_glyph(id.with_scale_and_position(s, point(pen, baseline)))
        {
            let b = outlined.px_bounds();
            outlined.draw(|gx, gy, cover| {
                let x = b.min.x as i32 + gx as i32;
                let y = b.min.y as i32 + gy as i32;
                blend(&mut buf, w, h, x, y, rgb, cover * opacity);
            });
        }
        pen += font.as_scaled(s).h_advance(id);
    }

    if let Some(fill) = face.fill {
        let x0 = scale(PAD_X, dpi) as f32;
        let x1 = (w - scale(PAD_X, dpi)) as f32;
        let bar_top = top + line + gap;
        capsule(
            &mut buf,
            w,
            h,
            x0,
            x1,
            bar_top,
            bar_h,
            rgb,
            TRACK_ALPHA / 255.0 * opacity,
        );
        let end = x0 + (x1 - x0) * fill.clamp(0.0, 1.0);
        capsule(&mut buf, w, h, x0, end, bar_top, bar_h, rgb, opacity);
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::human_duration;

    fn sample(text: &str, fill: Option<f32>, stale: bool) -> WidgetFace {
        WidgetFace {
            text: text.into(),
            tone: Tone::Normal,
            fill,
            stale,
        }
    }

    /// 有畫東西的像素。背景本身是 `HIT_ALPHA`，不算。
    fn opaque(buf: &[u8]) -> usize {
        buf.chunks(4).filter(|p| p[3] > HIT_ALPHA).count()
    }

    /// face.rs 會產生的每一個字元，至少一套字型要有。缺字的話那個字會直接不見。
    #[test]
    fn every_character_the_face_can_produce_has_a_glyph() {
        let fonts = fonts();
        let mut texts: Vec<String> = vec!["\u{2013}".into(), "!".into()];
        for minutes in (0..=999 * 60 + 59).step_by(7) {
            texts.push(human_duration(minutes));
        }
        for text in texts {
            for ch in text.chars() {
                assert!(
                    pick(&fonts, ch, 15.0).is_some(),
                    "「{ch}」兩套字型都沒有（{text}）"
                );
            }
        }
    }

    /// 固定寬度要塞得下任何一個時間。
    #[test]
    fn the_fixed_width_fits_every_duration() {
        let fonts = fonts();
        for dpi in [96, 120, 144, 192] {
            let room = (width_for(dpi) - 2 * scale(PAD_X, dpi)) as f32;
            for minutes in (0..=999 * 60 + 59).step_by(7) {
                let text = human_duration(minutes);
                let wide = measure(&fonts, &text, text_px(dpi));
                assert!(wide <= room, "{text} 寬 {wide}，只有 {room}（dpi {dpi}）");
            }
        }
    }

    #[test]
    fn the_width_grows_with_dpi() {
        assert!(width_for(144) > width_for(96));
    }

    #[test]
    fn the_buffer_has_one_bgra_pixel_per_cell() {
        let buf = render(&sample("12 小時", Some(0.5), false), 150, 60, 120, false);
        assert_eq!(buf.len(), 150 * 60 * 4);
    }

    /// `UpdateLayeredWindow` 吃的是預乘 alpha：任何色版都不能大於 alpha。
    #[test]
    fn the_pixels_are_premultiplied() {
        let buf = render(
            &sample("23 小時 10 分鐘", Some(0.7), false),
            150,
            60,
            120,
            false,
        );
        for p in buf.chunks(4) {
            assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{p:?}");
        }
    }

    #[test]
    fn a_fuller_bar_covers_more_pixels() {
        let none = opaque(&render(&sample("5 小時", None, false), 150, 60, 120, false));
        let empty = opaque(&render(
            &sample("5 小時", Some(0.0), false),
            150,
            60,
            120,
            false,
        ));
        let full = opaque(&render(
            &sample("5 小時", Some(1.0), false),
            150,
            60,
            120,
            false,
        ));
        assert!(none < empty, "底軌沒畫出來");
        let empty_alpha: u32 = render(&sample("5 小時", Some(0.0), false), 150, 60, 120, false)
            .chunks(4)
            .map(|p| p[3] as u32)
            .sum();
        let full_alpha: u32 = render(&sample("5 小時", Some(1.0), false), 150, 60, 120, false)
            .chunks(4)
            .map(|p| p[3] as u32)
            .sum();
        assert!(full_alpha > empty_alpha, "填滿的沒有比空的多");
        assert!(full >= empty);
    }

    #[test]
    fn stale_data_is_fainter() {
        let max = |buf: Vec<u8>| buf.chunks(4).map(|p| p[3]).max().unwrap();
        let fresh = max(render(
            &sample("5 小時", Some(1.0), false),
            150,
            60,
            120,
            false,
        ));
        let stale = max(render(
            &sample("5 小時", Some(1.0), true),
            150,
            60,
            120,
            false,
        ));
        assert!(stale < fresh, "{stale} 沒有比 {fresh} 淡");
        assert!(stale > 64, "{stale} 淡到看不見");
    }

    /// 每個像素都要點得到。alpha 0 的像素點擊會穿透到工作列，使用者點到字與
    /// 進度條中間的空白時面板不會出來（2026-09-23 實機回報）。背景只有 `HIT_ALPHA`，
    /// 顏色是 0，看不出來。
    #[test]
    fn every_pixel_catches_clicks() {
        let (w, h) = (150, 60);
        for fill in [Some(1.0), None] {
            let buf = render(&sample("5 小時", fill, false), w, h, 120, false);
            assert!(
                buf.chunks(4).all(|p| p[3] >= HIT_ALPHA),
                "{fill:?} 有點不到的像素"
            );
            for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
                let i = ((y * w + x) * 4) as usize;
                assert_eq!(&buf[i..i + 4], &[0, 0, 0, HIT_ALPHA], "({x}, {y}) 看得出來");
            }
        }
    }

    #[test]
    fn a_light_taskbar_gets_dark_text() {
        assert_eq!(color(Tone::Normal, true), NORMAL_ON_LIGHT);
        assert_eq!(
            color(Tone::Normal, false),
            state_color(DisplayState::Normal)
        );
        // 其他色調兩種工作列都看得到，不分深淺。
        assert_eq!(color(Tone::Alert, true), color(Tone::Alert, false));
    }

    #[test]
    fn a_zero_size_does_not_panic() {
        assert!(render(&sample("1 小時", Some(0.5), false), 0, 0, 96, false).is_empty());
    }

    /// 傾印成 PNG 給人看：`cargo test ... -- --ignored dumps_widget`。
    /// 疊在工作列的底色上，不然透明的地方看不出邊界。
    #[test]
    #[ignore]
    fn dumps_widget_for_visual_inspection() {
        let dir = std::env::temp_dir().join("gfnusage-widget");
        std::fs::create_dir_all(&dir).unwrap();
        let dpi = 120;
        let w = width_for(dpi);
        let h = 60;
        type Case = (&'static str, &'static str, Tone, Option<f32>, bool);
        let cases: [Case; 8] = [
            (
                "a-normal",
                "23 小時 10 分鐘",
                Tone::Normal,
                Some(0.23),
                false,
            ),
            ("b-widest", WIDEST, Tone::Normal, Some(0.99), false),
            ("c-low", "45 分鐘", Tone::Low, Some(0.02), false),
            ("d-over", "12 小時", Tone::Alert, Some(0.4), false),
            ("e-exhausted", "!", Tone::Alert, Some(0.0), false),
            ("e2-login", "!", Tone::Alert, None, false),
            ("f-none", "\u{2013}", Tone::Muted, None, false),
            ("g-stale", "23 小時 10 分鐘", Tone::Normal, Some(0.23), true),
        ];
        for (theme, bg, light) in [
            ("dark", [32u8, 32, 32], false),
            ("light", [243, 243, 243], true),
        ] {
            for (name, text, tone, fill, stale) in cases {
                let buf = render(
                    &WidgetFace {
                        text: text.into(),
                        tone,
                        fill,
                        stale,
                    },
                    w,
                    h,
                    dpi,
                    light,
                );
                // 預乘的 over：out = src + bg * (1 - a)。
                let mut rgb = Vec::with_capacity((w * h * 3) as usize);
                for p in buf.chunks(4) {
                    let a = p[3] as f32 / 255.0;
                    for (c, b) in [(p[2], bg[0]), (p[1], bg[1]), (p[0], bg[2])] {
                        rgb.push((c as f32 + b as f32 * (1.0 - a)).round() as u8);
                    }
                }
                let file = std::fs::File::create(dir.join(format!("{theme}-{name}.png"))).unwrap();
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
                enc.set_color(png::ColorType::Rgb);
                enc.set_depth(png::BitDepth::Eight);
                enc.write_header().unwrap().write_image_data(&rgb).unwrap();
            }
        }
        println!("dumped to {}", dir.display());
    }
}
