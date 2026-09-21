//! 從 NVIDIA 帳號頁那個 OAuth 客戶端取得 id_token。
//!
//! 為什麼不沿用 `oauth`（迴圈埠）那一套：兩顆 client_id 各有各的限制，
//! 而且剛好互補（依據見 spike 3a）。
//!
//! | | GFN 客戶端 `ZU7sPN…` | 帳號頁 `HdpDyyR1…` |
//! |---|---|---|
//! | localhost 回呼 | 收 | 400 |
//! | `tk_client`（90 天 client_token） | 給 | `Requested scope is not allowed` |
//! | `mes.geforcenow.com` | 200 | 200 |
//! | `api-prod.nvidia.com`（逐場紀錄） | 401 | 200 |
//!
//! 逐場紀錄只有帳號頁那顆拿得到，而它不收 localhost，所以回呼要在 webview
//! 裡等導向。它也沒有 90 天的 client_token，只有一小時的 id_token，過期靠
//! `prompt=none` 的靜默授權換新的 —— 換不換得到，取決於瀏覽器那邊的 session
//! 還在不在。這是換過來付的代價，不是 bug。

use chrono::{DateTime, Utc};
use percent_encoding::{percent_decode_str, utf8_percent_encode};

use super::jwt;
use super::oauth::QUERY;
use super::refresh::{post_token, AuthCodeResponse, Grant};
use crate::error::GfnError;

/// NVIDIA 帳號頁（`www.nvidia.com/en-us/account/`）的 starfleet client_id。
pub const ACCOUNT_CLIENT_ID: &str = "HdpDyyR1DqQFapN2MBk5kjJgAvu6UTXRDgtwLhQjrH8";

/// 授權完成後導向這裡。**不是**我們的頁面，是 NVIDIA 自己的回呼頁 ——
/// 那是登記在這顆 client_id 底下的唯一一個，換掉就是 400。
/// 我們不需要那一頁做任何事，只要在 webview 看到它被導向、把 query 裡的
/// `code` 取走就好。
pub const ACCOUNT_REDIRECT_URI: &str = "https://www.nvidia.com/auth/";

/// 帳號頁實際送出的 scope。`tk_client` 加不得，加了授權完成後會回
/// `Requested scope is not allowed`。
const SCOPE: &str = "openid consent consent_u email";

/// 授權請求的樣子。
///
/// `silent` 走 `prompt=none`：帶著 webview 存下來的 cookie 就能無感換到新的
/// `code`，不會有畫面。換不到時伺服器回 `error=login_required`，那時才需要
/// 把視窗開給使用者看。
pub fn authorize_url(
    base: &str,
    device_id: &str,
    challenge: &str,
    nonce: &str,
    state: &str,
    silent: bool,
) -> String {
    let q = |value: &str| utf8_percent_encode(value, QUERY).to_string();
    format!(
        "{base}/authorize?response_type=code&device_id={device_id}&scope={scope}\
         &client_id={client_id}&redirect_uri={redirect}&ui_locales=zh-TW\
         &nonce={nonce}&state={state}&prompt={prompt}\
         &code_challenge={challenge}&code_challenge_method=S256",
        device_id = q(device_id),
        scope = q(SCOPE),
        client_id = q(ACCOUNT_CLIENT_ID),
        redirect = q(ACCOUNT_REDIRECT_URI),
        nonce = q(nonce),
        state = q(state),
        prompt = if silent { "none" } else { "select_account" },
        challenge = q(challenge),
    )
}

/// webview 導到某個網址時，那是什麼。
#[derive(Debug, PartialEq)]
pub enum Redirect {
    /// 我們在等的回呼，裡面是授權碼。
    Code(String),
    /// NVIDIA 回報授權失敗。
    Denied(String),
    /// 靜默授權換不到，要把視窗開給使用者。這是 `prompt=none` 的正常結果，
    /// 不是錯誤，不能拿它去點亮「需要重新登入」以外的東西。
    LoginRequired,
    /// 登入流程自己的中間頁。**不是失敗**，要讓它繼續走。
    Elsewhere,
}

/// 判斷 webview 正要去的網址是不是我們的回呼。
///
/// 比對 `state`：webview 裡跑的是 NVIDIA 的整套登入流程，中間會經過好幾個
/// 網域，而使用者也可能在同一個視窗裡按到別的連結。
pub fn parse_redirect(url: &str, expected_state: &str) -> Redirect {
    let (path, query) = match url.split_once('?') {
        Some(split) => split,
        None => return Redirect::Elsewhere,
    };
    // 回呼頁的路徑可能帶或不帶尾斜線，兩種都認。
    if path.trim_end_matches('/') != ACCOUNT_REDIRECT_URI.trim_end_matches('/') {
        return Redirect::Elsewhere;
    }

    let mut code = None;
    let mut error = None;
    let mut description = None;
    let mut state = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        // 先把 `+` 換成空白，再 percent-decode。順序不能顛倒：查詢字串是
        // form 編碼，字面的 `+` 送過來是 `%2B`，先解碼的話它會變成 `+`，
        // 再被下一步換成空白。授權碼裡有 `+` 就是一個看不出原因的 400。
        let value = percent_decode_str(&value.replace('+', " "))
            .decode_utf8_lossy()
            .to_string();
        match key {
            "code" => code = Some(value),
            "error" => error = Some(value),
            "error_description" => description = Some(value),
            "state" => state = Some(value),
            _ => {}
        }
    }

    if state.as_deref() != Some(expected_state) {
        return Redirect::Elsewhere;
    }
    if let Some(error) = error {
        if error == "login_required" || error == "interaction_required" {
            return Redirect::LoginRequired;
        }
        return Redirect::Denied(match description {
            Some(text) => format!("{error}：{text}"),
            None => error,
        });
    }
    match code {
        Some(code) if !code.is_empty() => Redirect::Code(code),
        _ => Redirect::Elsewhere,
    }
}

/// 等這次授權的結論。三個來源：webview 導到了有結論的網址、視窗被關掉、
/// 使用者按了取消。
///
/// 拆出來是為了測得到。取消那條路沒有測的話，症狀是按鈕按下去沒反應而
/// 面板卡在「登入中…」，要等滿逾時才會動 —— 那不該靠手動點擊才發現。
///
/// `cancel` 必須在開視窗之前就訂閱好，不然按取消的人比開始等的人早一步，
/// 訊號就掉了。
pub async fn await_outcome(
    redirects: &mut tokio::sync::mpsc::UnboundedReceiver<Redirect>,
    closed: &tokio::sync::Notify,
    cancel: &mut tokio::sync::watch::Receiver<u64>,
    timeout: std::time::Duration,
    silent: bool,
) -> Result<Redirect, GfnError> {
    tokio::select! {
        received = tokio::time::timeout(timeout, redirects.recv()) => match received {
            Ok(Some(outcome)) => Ok(outcome),
            Ok(None) => Err(GfnError::LoginFailed("登入視窗沒有回應".into())),
            // 靜默逾時就是換不到，要使用者動手，不是故障。
            Err(_) if silent => Err(GfnError::NeedsLogin),
            Err(_) => Err(GfnError::LoginFailed(format!(
                "等了 {} 分鐘還是沒有完成登入",
                timeout.as_secs() / 60
            ))),
        },
        _ = closed.notified() => Err(GfnError::LoginCancelled),
        _ = cancel.changed() => Err(GfnError::LoginCancelled),
    }
}

/// 一顆可以用的 id_token 與它的到期時刻。
///
/// 沒有 client_token —— 這顆 client_id 不給（見模組說明）。
pub struct Token {
    pub id_token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// 用授權碼換 id_token。
///
/// 只有一段。GFN 客戶端那條要再打一次 `GET /client_token` 拿 90 天的憑證
/// （spike 4c），這裡沒有那一步，因為沒有 `tk_client`。
pub async fn exchange_code(
    http: &reqwest::Client,
    auth_base: &str,
    code: &str,
    verifier: &str,
    nonce: &str,
) -> Result<Token, GfnError> {
    let auth = post_token::<AuthCodeResponse>(
        http,
        auth_base,
        Grant::Exchange,
        &[
            ("grant_type", "authorization_code"),
            // 必須和授權請求裡那一個逐字元相同。
            ("redirect_uri", ACCOUNT_REDIRECT_URI),
            ("code", code),
            ("code_verifier", verifier),
        ],
    )
    .await?;

    // 同 `oauth::exchange`：沒有 nonce claim 就不比，不能因此擋下能用的憑證。
    if let Some(returned) = jwt::nonce(&auth.id_token) {
        if returned != nonce {
            return Err(GfnError::LoginFailed("回應的 nonce 與請求不符".into()));
        }
    }
    if auth.id_token.is_empty() {
        return Err(GfnError::LoginFailed("回應裡沒有 id_token".into()));
    }

    Ok(Token {
        expires_at: jwt::expiry(&auth.id_token),
        id_token: auth.id_token,
    })
}

/// 這台機器的識別碼。
///
/// 帳號頁送的是一顆 UUID，不是 GFN 客戶端那個固定的 `gfnclient`。內容沒有被
/// 驗證過，但格式照著給比較保險。存在 `ui-state.json`：它是這台機器的身分，
/// 跟著作息設定匯出到別台機器沒有意義。
pub fn new_device_id() -> Result<String, GfnError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| GfnError::LoginFailed(format!("取不到亂數：{e}")))?;
    // RFC 4122 的版本與變體位元。
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

#[cfg(test)]
mod tests {
    use tokio::sync::watch;

    use super::*;
    use crate::auth::oauth::LOGIN_TIMEOUT;

    const STATE: &str = "SxnS9EADQ9dDOUkjgb_z9A";

    fn url() -> String {
        authorize_url(
            "https://login.nvidia.com",
            "105c3409-aace-4e9b-a3dd-30260a61a188",
            "Kx9FuxNGfFFTtevpHs_8s1sKj2wBvNgVVwGOzPLwJuo",
            "nonce123",
            STATE,
            false,
        )
    }

    /// `tk_client` 混進去，授權完成後整個流程會以
    /// `Requested scope is not allowed` 結束，而且是在使用者按完同意之後才失敗。
    #[test]
    fn the_scope_must_not_ask_for_a_client_token() {
        assert!(!url().contains("tk_client"));
        assert!(url().contains("scope=openid%20consent%20consent_u%20email"));
    }

    /// base64url 的 `-` 與 `_` 不能被編碼，編了 PKCE 會無聲對不上。
    /// 理由與 `oauth::QUERY` 同一條。
    #[test]
    fn base64url_characters_survive_the_encoding() {
        assert!(url().contains("code_challenge=Kx9FuxNGfFFTtevpHs_8s1sKj2wBvNgVVwGOzPLwJuo"));
        assert!(url().contains(&format!("client_id={ACCOUNT_CLIENT_ID}")));
    }

    #[test]
    fn silent_and_interactive_differ_only_in_the_prompt() {
        let interactive = url();
        let silent = authorize_url(
            "https://login.nvidia.com",
            "105c3409-aace-4e9b-a3dd-30260a61a188",
            "Kx9FuxNGfFFTtevpHs_8s1sKj2wBvNgVVwGOzPLwJuo",
            "nonce123",
            STATE,
            true,
        );
        assert!(interactive.contains("prompt=select_account"));
        assert!(silent.contains("prompt=none"));
        assert_eq!(
            interactive.replace("prompt=select_account", "prompt=none"),
            silent
        );
    }

    #[test]
    fn picks_the_code_out_of_the_callback() {
        assert_eq!(
            parse_redirect(
                &format!("https://www.nvidia.com/auth/?code=ABC123&state={STATE}"),
                STATE
            ),
            Redirect::Code("ABC123".into())
        );
    }

    /// 實測回過的那一種。使用者按完同意才失敗，訊息要原樣帶出來。
    #[test]
    fn reports_what_nvidia_refused() {
        let url = format!(
            "https://www.nvidia.com/auth/?error_description=Requested+scope+is+not+allowed\
             &state={STATE}&error=invalid_request"
        );
        assert_eq!(
            parse_redirect(&url, STATE),
            Redirect::Denied("invalid_request：Requested scope is not allowed".into())
        );
    }

    /// `prompt=none` 換不到是常態，不是錯誤。混在一起的話，每次靜默續期
    /// 失敗都會變成一則錯誤訊息。
    #[test]
    fn login_required_is_its_own_outcome() {
        for error in ["login_required", "interaction_required"] {
            assert_eq!(
                parse_redirect(
                    &format!("https://www.nvidia.com/auth/?error={error}&state={STATE}"),
                    STATE
                ),
                Redirect::LoginRequired,
                "{error}"
            );
        }
    }

    /// webview 裡跑的是整套登入流程，中間會經過好幾個網域。把中間頁當成
    /// 失敗，等於登入永遠走不完。
    #[test]
    fn the_pages_along_the_way_are_not_the_callback() {
        for url in [
            "https://login.nvidia.com/authorize?client_id=x&state=y",
            "https://accounts.nvgs.nvidia.com/api/1/oauth/authorize?redirect_uri=z",
            "https://login.nvgs.nvidia.com/v1/login/identifier?client_id=310670214980174257",
            "https://www.nvidia.com/en-us/account/",
        ] {
            assert_eq!(parse_redirect(url, STATE), Redirect::Elsewhere, "{url}");
        }
    }

    /// state 對不上的回呼不是我們這次的。
    #[test]
    fn a_callback_for_someone_elses_login_is_ignored() {
        assert_eq!(
            parse_redirect(
                "https://www.nvidia.com/auth/?code=ABC123&state=somebody-else",
                STATE
            ),
            Redirect::Elsewhere
        );
    }

    #[test]
    fn the_trailing_slash_is_not_load_bearing() {
        assert_eq!(
            parse_redirect(
                &format!("https://www.nvidia.com/auth?code=ABC&state={STATE}"),
                STATE
            ),
            Redirect::Code("ABC".into())
        );
    }

    /// 字面的 `+` 在查詢字串裡是 `%2B`。先 percent-decode 再換 `+` 的話
    /// 它會變成空白，而這串是拿去換 token 的。
    #[test]
    fn a_plus_inside_the_code_survives() {
        assert_eq!(
            parse_redirect(
                &format!("https://www.nvidia.com/auth/?code=A%2BB&state={STATE}"),
                STATE
            ),
            Redirect::Code("A+B".into())
        );
    }

    fn waiters() -> (
        tokio::sync::mpsc::UnboundedSender<Redirect>,
        tokio::sync::mpsc::UnboundedReceiver<Redirect>,
        tokio::sync::Notify,
        tokio::sync::watch::Sender<u64>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (tx, rx, tokio::sync::Notify::new(), watch::Sender::new(0))
    }

    /// 取消按下去就結束，不必等滿逾時。而且訊號比開始等更早到也收得到
    /// —— receiver 在開視窗之前就訂閱好。
    #[tokio::test]
    async fn cancelling_does_not_wait_for_the_timeout() {
        let (_tx, mut rx, closed, canceller) = waiters();
        let mut cancel = canceller.subscribe();
        let started = std::time::Instant::now();

        canceller.send_modify(|generation| *generation += 1);
        let problem = await_outcome(&mut rx, &closed, &mut cancel, LOGIN_TIMEOUT, false)
            .await
            .unwrap_err();

        assert!(matches!(problem, GfnError::LoginCancelled), "{problem:?}");
        assert!(started.elapsed() < LOGIN_TIMEOUT, "{started:?}");
    }

    /// 使用者自己把視窗關掉。通知可能比開始等更早到，所以 `window.rs`
    /// 用 `notify_one` 而不是 `notify_waiters`。
    #[tokio::test]
    async fn closing_the_window_ends_it() {
        let (_tx, mut rx, closed, canceller) = waiters();
        let mut cancel = canceller.subscribe();

        closed.notify_one();
        let problem = await_outcome(&mut rx, &closed, &mut cancel, LOGIN_TIMEOUT, false)
            .await
            .unwrap_err();

        assert!(matches!(problem, GfnError::LoginCancelled), "{problem:?}");
    }

    /// 靜默逾時是「換不到，要使用者動手」。混成錯誤的話，每次續期失敗
    /// 都會在面板上留一行紅字。
    #[tokio::test]
    async fn a_silent_timeout_asks_for_a_login() {
        let (_tx, mut rx, closed, canceller) = waiters();
        let mut cancel = canceller.subscribe();

        let problem = await_outcome(
            &mut rx,
            &closed,
            &mut cancel,
            std::time::Duration::from_millis(10),
            true,
        )
        .await
        .unwrap_err();

        assert!(matches!(problem, GfnError::NeedsLogin), "{problem:?}");
    }

    #[tokio::test]
    async fn a_redirect_with_a_conclusion_wins() {
        let (tx, mut rx, closed, canceller) = waiters();
        let mut cancel = canceller.subscribe();

        tx.send(Redirect::Code("ABC".into())).unwrap();

        assert_eq!(
            await_outcome(&mut rx, &closed, &mut cancel, LOGIN_TIMEOUT, false)
                .await
                .unwrap(),
            Redirect::Code("ABC".into())
        );
    }

    #[test]
    fn device_ids_look_like_uuids_and_differ() {
        let a = new_device_id().unwrap();
        assert_eq!(a.len(), 36);
        assert_eq!(a.matches('-').count(), 4);
        assert_eq!(&a[14..15], "4", "版本位元");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"), "變體位元");
        assert_ne!(a, new_device_id().unwrap());
    }
}
