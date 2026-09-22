//! 盯著 GeForce NOW 客戶端的視窗，在它的狀態轉換時抓一次額度。
//!
//! 為什麼要這個：額度只在串流時變動，所以定時抓到的多半是同一個數字，而
//! 數字真的變了的那一刻（一場剛玩完）靠定時最久要等滿一個間隔。視窗狀態
//! 指出那一刻，定時因此可以拉長成備援。
//!
//! 只有 Windows 有實作，見 `win32`。

pub mod title;
pub mod win32;

use std::time::Duration;

/// 掃描間隔。參考專案（GeForce-NOW-Rich-Presence）用 10 秒，夠快又不會
/// 在載入畫面翻標題的那幾秒裡誤判。
pub const SWEEP: Duration = Duration::from_secs(10);

/// 連續看到同一個狀態幾次才認定。
///
/// 遊戲啟動時標題會先翻成遊戲名、再被載入畫面翻回去，中間可能只有幾秒。
/// 認定一次就抓一次，所以寧可慢 10 秒也不要在那裡來回抓。
pub const CONFIRM: u8 = 2;

/// 遊戲結束之後補抓的延遲。
///
/// 剛結束的那一場 NVIDIA 後端不一定結算好了，立刻抓可能拿到玩之前的數字。
///
/// 2026-09-23 實測一場：`InGame → Lobby` 確認於 16:19:50Z，這一次補抓在
/// 16:21:51Z 拿到剩餘 5768 → 5664，也就是那場的 104 分鐘。少了補抓，
/// 數字要等到下一個定時週期才會對。
///
/// 兩分鐘夠不夠只有這一個樣本。要再驗就看 `snapshots.json`：它只在剩餘
/// 分鐘數變動時寫列，所以「結束那次」與「補抓那次」是不是兩列不同的值，
/// 一看就知道。
pub const SETTLE: Duration = Duration::from_secs(120);

/// 剛抓過這麼短的時間內，視窗轉換就不再抓。
///
/// 開了 GFN 順手點開面板是很常見的順序：面板自己抓一次，十幾秒後 watcher
/// 確認到「視窗出現」又抓一次。兩次拿到的是同一份數字。
pub const DEBOUNCE: Duration = Duration::from_secs(60);

/// 剛抓過，所以這次視窗轉換不用再抓。
///
/// 只擋「立刻那次」。遊戲結束排的補抓不走這裡 —— 它要的就是晚一點的數字。
pub fn too_soon(last_poll: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last_poll.is_some_and(|last| now.duration_since(last) < DEBOUNCE)
}

/// GFN 客戶端目前在哪個狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GfnState {
    /// 找不到帶品牌字樣的 GFN 視窗。沒開，或是剛被關掉。
    Absent,
    /// 視窗在，標題只有品牌字樣：在首頁、遊戲庫、設定頁那一類地方。
    Lobby,
    /// 視窗在，標題帶著遊戲名：正在串流。
    InGame,
}

/// 一次狀態轉換要抓幾次。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// 不抓。
    None,
    /// 抓一次。
    Once,
    /// 抓一次，`SETTLE` 之後再抓一次。
    Twice,
}

/// 這次轉換要不要抓。
///
/// 「開始玩」刻意不抓：那一刻數字還沒變，而使用者正要去玩，不會看面板。
/// 「沒玩就把 GFN 關掉」也不抓，理由一樣 —— 沒有東西變。
pub fn trigger(from: GfnState, to: GfnState) -> Trigger {
    use GfnState::{Absent, InGame, Lobby};

    match (from, to) {
        // 開 GFN。可能在別台裝置或瀏覽器版玩過，先對一次數字。
        (Absent, Lobby) => Trigger::Once,
        // 這支程式比 GFN 晚啟動，第一眼就已經在遊戲中。同上。
        (Absent, InGame) => Trigger::Once,
        // 開始玩。
        (Lobby, InGame) => Trigger::None,
        // 沒玩就關掉。
        (Lobby, Absent) => Trigger::None,
        // 玩完回到首頁。這是數字變了的那一刻。
        (InGame, Lobby) => Trigger::Twice,
        // 玩完直接關掉 GFN。和上一列同一件事，只是沒經過首頁。
        (InGame, Absent) => Trigger::Twice,
        // 同一個狀態不是轉換，`Probe` 已經擋掉了。
        (Absent, Absent) | (Lobby, Lobby) | (InGame, InGame) => Trigger::None,
    }
}

/// `Probe` 確認到了什麼。
///
/// 分成兩種而不是只回傳轉換，是為了讓「偵測活著」看得出來：程式啟動時
/// GFN 已經開著的話，第一次確認不是轉換，只回傳轉換的話日誌上一片空白，
/// 和「完全沒偵測到」分不出來。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// 第一次確認到的狀態。只是基準，沒有剛剛發生什麼事。
    Baseline(GfnState),
    /// 一次狀態轉換。
    Moved(GfnState, GfnState),
}

/// 把一連串觀察收斂成「確認過的轉換」。
///
/// 參考專案沒有做這件事，標題一翻下一個週期就送出去。對它（Discord 狀態）
/// 代價只是閃一下，對這裡代價是多抓一次。
#[derive(Debug, Default)]
pub struct Probe {
    /// 目前認定的狀態。`None` 代表還沒有基準。
    committed: Option<GfnState>,
    /// 正在累積次數的候選狀態。
    pending: Option<(GfnState, u8)>,
}

impl Probe {
    pub fn new() -> Self {
        Self::default()
    }

    /// 餵一次觀察。
    ///
    /// `seen` 是 `None` 代表這次掃描失敗（不是「沒有視窗」，那是
    /// `GfnState::Absent`）。維持原狀，並且把累積中的次數清掉 ——
    /// 中間斷過就不算連續。
    pub fn observe(&mut self, seen: Option<GfnState>) -> Option<Change> {
        let Some(seen) = seen else {
            self.pending = None;
            return None;
        };

        if self.committed == Some(seen) {
            self.pending = None;
            return None;
        }

        let count = match self.pending {
            Some((pending, count)) if pending == seen => count + 1,
            _ => 1,
        };
        if count < CONFIRM {
            self.pending = Some((seen, count));
            return None;
        }

        self.pending = None;
        // 第一次確認只立基準，不算轉換。程式啟動時 GFN 已經開著的話，
        // 輪詢迴圈的第一圈本來就會抓，這裡再回一個 Absent → Lobby
        // 就變成啟動抓兩次。基準仍然回傳，呼叫端要把它寫進日誌。
        match self.committed.replace(seen) {
            Some(previous) => Some(Change::Moved(previous, seen)),
            None => Some(Change::Baseline(seen)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GfnState::{Absent, InGame, Lobby};
    use super::*;

    #[test]
    fn opening_the_client_fetches_once() {
        assert_eq!(trigger(Absent, Lobby), Trigger::Once);
        assert_eq!(trigger(Absent, InGame), Trigger::Once);
    }

    /// 這兩個是這整件事的理由：一場剛玩完，數字變了。
    #[test]
    fn finishing_a_session_fetches_twice() {
        assert_eq!(trigger(InGame, Lobby), Trigger::Twice);
        assert_eq!(trigger(InGame, Absent), Trigger::Twice);
    }

    #[test]
    fn starting_a_game_or_leaving_without_playing_fetches_nothing() {
        assert_eq!(trigger(Lobby, InGame), Trigger::None);
        assert_eq!(trigger(Lobby, Absent), Trigger::None);
    }

    /// 啟動時 GFN 已經開著。輪詢迴圈的第一圈會抓，這裡不能再抓一次 ——
    /// 所以回的是 `Baseline` 而不是 `Moved`。回傳它而不是 `None`，是為了
    /// 讓日誌看得出偵測活著。
    #[test]
    fn the_first_reading_only_sets_the_baseline() {
        let mut probe = Probe::new();
        assert_eq!(probe.observe(Some(Lobby)), None);
        assert_eq!(probe.observe(Some(Lobby)), Some(Change::Baseline(Lobby)));
        assert_eq!(probe.observe(Some(Lobby)), None);
    }

    #[test]
    fn a_transition_needs_two_consecutive_readings() {
        let mut probe = Probe::new();
        probe.observe(Some(Absent));
        probe.observe(Some(Absent));

        assert_eq!(probe.observe(Some(Lobby)), None);
        assert_eq!(
            probe.observe(Some(Lobby)),
            Some(Change::Moved(Absent, Lobby))
        );
        // 認定之後就不再重複回報。
        assert_eq!(probe.observe(Some(Lobby)), None);
    }

    /// 載入畫面把標題翻一下又翻回來，不該算成玩了一場又結束。
    #[test]
    fn a_single_flicker_is_not_a_transition() {
        let mut probe = Probe::new();
        probe.observe(Some(Lobby));
        probe.observe(Some(Lobby));

        assert_eq!(probe.observe(Some(InGame)), None);
        assert_eq!(probe.observe(Some(Lobby)), None);
        assert_eq!(probe.observe(Some(InGame)), None);
        assert_eq!(probe.observe(Some(Lobby)), None);
    }

    /// 掃描失敗維持原狀，而且把累積中的次數清掉：中間斷過不算連續。
    #[test]
    fn a_failed_sweep_holds_and_breaks_the_run() {
        let mut probe = Probe::new();
        probe.observe(Some(Lobby));
        probe.observe(Some(Lobby));

        assert_eq!(probe.observe(Some(InGame)), None);
        assert_eq!(probe.observe(None), None);
        assert_eq!(probe.observe(Some(InGame)), None);
        assert_eq!(
            probe.observe(Some(InGame)),
            Some(Change::Moved(Lobby, InGame))
        );
    }

    /// 開了 GFN 順手點開面板是常見的順序：面板抓一次，十幾秒後 watcher
    /// 確認到「視窗出現」又要抓。兩次拿到的是同一份數字。
    #[test]
    fn a_transition_right_after_a_fetch_is_skipped() {
        let now = std::time::Instant::now();

        assert!(too_soon(Some(now - Duration::from_secs(20)), now));
        assert!(!too_soon(Some(now - Duration::from_secs(90)), now));
        // 還沒抓過的話沒有理由跳過。
        assert!(!too_soon(None, now));
    }

    /// 走完一場：開 GFN、進遊戲、退出來、關掉。
    #[test]
    fn a_whole_session_yields_three_transitions() {
        let mut probe = Probe::new();
        let mut seen = Vec::new();
        for state in [
            Absent, Absent, // 基準
            Lobby, Lobby, // 開 GFN
            InGame, InGame, // 進遊戲
            Lobby, Lobby, // 退出遊戲
            Absent, Absent, // 關掉 GFN
        ] {
            if let Some(Change::Moved(from, to)) = probe.observe(Some(state)) {
                seen.push(((from, to), trigger(from, to)));
            }
        }

        assert_eq!(
            seen,
            vec![
                ((Absent, Lobby), Trigger::Once),
                ((Lobby, InGame), Trigger::None),
                ((InGame, Lobby), Trigger::Twice),
                ((Lobby, Absent), Trigger::None),
            ]
        );
    }
}
