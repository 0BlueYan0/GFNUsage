//! 從 GFN 客戶端的視窗標題判斷它在首頁還是在遊戲中。
//!
//! 標題是由各語言的模板算出來的，而模板的形狀不一致：多數歐語系把遊戲名
//! 放在前面（`<遊戲名> on GeForce NOW`），繁中、簡中、日、韓、土則把品牌
//! 放在前面，繁中的是「在 GeForce NOW 上玩 <遊戲名>」。所以不能寫成
//! 「切掉 `GeForce NOW` 之後取左邊」—— 那樣繁中切出來是「在」。
//!
//! 這裡不解析遊戲名字，只要分得出「只有品牌」和「品牌加了別的字」，
//! 所以不必背那張 32 種語言的模板表，也不必引入 regex。

use super::GfnState;

/// 品牌字樣。兩種寫法都出現過，長的排前面才不會先被短的吃掉。
const BRANDS: [&str; 2] = ["geforce now", "geforcenow"];

/// 從剩下的字兩端修掉的符號。模板把這些擺在品牌旁邊，拿掉品牌之後
/// 它們會留在邊上。
const EDGE: [char; 12] = [
    ' ', '\t', '-', '—', '–', '|', ':', '·', ',', '、', '：', '\'',
];

/// 整串剩下的字剛好等於其中一個，就當作沒有遊戲名。
///
/// 這些是模板擺在品牌旁邊的連接詞。比對的是**整串**不是子字串 ——
/// 真的有遊戲叫 `Hob`、`Fe`、`C9`、`幻塔`。
const CONNECTORS: [&str; 24] = [
    "a",
    "bei",
    "en",
    "in",
    "na",
    "no",
    "on",
    "op",
    "pe",
    "su",
    "sur",
    "v",
    "ve",
    "via",
    "w",
    "de",
    "ile",
    "上玩",
    "上的",
    "在",
    "の",
    "в",
    "на",
    "сервісі",
];

/// 這個標題屬於哪一種。
///
/// `None` 代表標題裡沒有品牌字樣，呼叫端該跳過這扇視窗 —— GFN 是 CEF
/// 應用，同一個程序底下還有標題空白的輔助視窗，而遊戲啟動失敗時的
/// `Application Launch failed` 也走這條。
pub fn classify(title: &str) -> Option<GfnState> {
    let title = normalize(title);
    let (start, len) = find_brand(&title)?;
    let end = start + absorb_suffix(&title[start + len..]) + len;

    let mut rest = String::with_capacity(title.len());
    rest.push_str(&title[..start]);
    rest.push_str(&title[end..]);
    let rest = rest.trim_matches(|c| EDGE.contains(&c));

    if rest.is_empty() || CONNECTORS.iter().any(|c| rest.eq_ignore_ascii_case(c)) {
        return Some(GfnState::Lobby);
    }
    Some(GfnState::InGame)
}

/// 統一空白與商標符號。
///
/// 繁中的模板用的是全形空白（U+3000），而遊戲名常帶著 `™`／`®`
/// ——「Wuthering Waves™ on GeForce NOW」是真的會出現的標題。
fn normalize(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_was_space = false;
    for c in title.chars() {
        if c == '®' || c == '™' {
            continue;
        }
        if c.is_whitespace() || c == '\u{3000}' {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
            }
            last_was_space = true;
            continue;
        }
        out.push(c);
        last_was_space = false;
    }
    out.trim_end().to_string()
}

/// 品牌字樣在哪裡，回傳 (起點, 長度)，都是 byte offset。
///
/// 自己掃而不是先 `to_lowercase()` 再 `find()`：土耳其語的 `İ` 小寫之後
/// 會變成兩個 char，byte offset 就對不回原字串了。品牌本身全是 ASCII，
/// 所以逐字元用 `eq_ignore_ascii_case` 比就夠。
fn find_brand(title: &str) -> Option<(usize, usize)> {
    let bytes = title.as_bytes();
    for (i, _) in title.char_indices() {
        for brand in BRANDS {
            let end = i + brand.len();
            if end <= bytes.len() && bytes[i..end].eq_ignore_ascii_case(brand.as_bytes()) {
                return Some((i, brand.len()));
            }
        }
    }
    None
}

/// 品牌後面黏著的字尾有幾個 byte。
///
/// 芬蘭語是 `GeForce NOWssa`、土耳其語 `GeForce NOW'da`、匈牙利語
/// `GeForce NOW-n`。不吃掉的話那幾個字母會被當成遊戲名。
///
/// 只取 ASCII 字母數字。用 `is_alphanumeric()` 的話中日韓的字也算數，
/// 「GeForce NOW 上的 幻塔」少了那個空格就會整串被當成字尾。
fn absorb_suffix(rest: &str) -> usize {
    let mut taken = ascii_run(rest);
    let after = &rest[taken..];
    // 撇號或連字號接著字母，才算是同一個詞的一部分。中間有空格的
    // 「GeForce NOW - 某某」是分隔符號，不是字尾。
    let mut chars = after.chars();
    if let (Some(sep @ ('\'' | '\u{2019}' | '-')), Some(next)) = (chars.next(), chars.next()) {
        if next.is_ascii_alphanumeric() {
            taken += sep.len_utf8();
            taken += ascii_run(&rest[taken..]);
        }
    }
    taken
}

fn ascii_run(s: &str) -> usize {
    s.bytes().take_while(u8::is_ascii_alphanumeric).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 首頁的標題就是品牌本身（GFN 的 i18n 包裡的 `common.gfn`）。
    #[test]
    fn the_bare_brand_is_the_lobby() {
        assert_eq!(classify("GeForce NOW"), Some(GfnState::Lobby));
        assert_eq!(classify("  GeForce NOW  "), Some(GfnState::Lobby));
    }

    /// 遊戲名在前的那一半語系。
    #[test]
    fn a_game_before_the_brand_is_in_game() {
        for title in [
            "Wuthering Waves on GeForce NOW",
            "Forza Horizon 6 en GeForce NOW",
            "Clair Obscur: Expedition 33 sur GeForce NOW",
            "WARDOGS bei GeForce NOW",
        ] {
            assert_eq!(classify(title), Some(GfnState::InGame), "{title}");
        }
    }

    /// 品牌在前的那一半語系。這是「切掉品牌取左邊」會答錯的那一組：
    /// 繁中會切出「在」，簡中切出「GeForce NOW 上的」的左邊也就是空字串。
    #[test]
    fn a_game_after_the_brand_is_also_in_game() {
        for title in [
            "在 GeForce NOW 上玩 Wuthering Waves",
            "GeForce NOW 上的 幻塔",
            "GeForce NOW の Wuthering Waves",
            "GeForce NOW'da Forza Horizon 6",
        ] {
            assert_eq!(classify(title), Some(GfnState::InGame), "{title}");
        }
    }

    /// 商標符號與全形空白都是真的會出現在標題裡的東西。
    #[test]
    fn trademarks_and_wide_spaces_do_not_change_the_answer() {
        assert_eq!(
            classify("Wuthering Waves™ on GeForce NOW"),
            Some(GfnState::InGame)
        );
        assert_eq!(
            classify("在　GeForce NOW 上玩 Wuthering Waves"),
            Some(GfnState::InGame)
        );
        assert_eq!(classify("GeForce　NOW"), Some(GfnState::Lobby));
    }

    /// 黏著語的字尾是品牌的一部分，不是遊戲名。
    #[test]
    fn agglutinative_suffixes_belong_to_the_brand() {
        assert_eq!(classify("GeForce NOWssa"), Some(GfnState::Lobby));
        assert_eq!(classify("GeForce NOW'da"), Some(GfnState::Lobby));
        assert_eq!(classify("GeForce NOW-n"), Some(GfnState::Lobby));
        assert_eq!(classify("GeForceNOW"), Some(GfnState::Lobby));
    }

    /// 沒有品牌字樣的標題不是第四種狀態，是「這扇視窗不是我們要的」。
    /// 遊戲啟動失敗的對話框走這條，CEF 的輔助視窗（空標題）也是。
    #[test]
    fn a_title_without_the_brand_is_not_ours() {
        assert_eq!(classify(""), None);
        assert_eq!(classify("Application Launch failed"), None);
        assert_eq!(classify("Application resource corrupted"), None);
        assert_eq!(classify("Wuthering Waves"), None);
    }

    /// 只剩連接詞代表標題是品牌加一個介系詞，沒有遊戲名。
    #[test]
    fn a_leftover_connector_is_still_the_lobby() {
        assert_eq!(classify("GeForce NOW 上的"), Some(GfnState::Lobby));
        assert_eq!(classify("在 GeForce NOW"), Some(GfnState::Lobby));
    }

    /// 兩個字母的遊戲名不能被當成連接詞掃掉。
    #[test]
    fn short_game_names_survive() {
        assert_eq!(classify("Fe on GeForce NOW"), Some(GfnState::InGame));
        assert_eq!(classify("GeForce NOW 上的 幻塔"), Some(GfnState::InGame));
    }
}
