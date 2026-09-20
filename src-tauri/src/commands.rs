use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::api::subscriptions::fetch_subscription;
use crate::auth::oauth;
use crate::auth::session::{
    decode_session_data, default_shared_storage_path, read_shared_storage, ImportedSession,
};
use crate::error::GfnError;
use crate::pace::schedule::Schedule;
use crate::pace::{self, PaceReport};
use crate::panel;
use crate::quota::{DisplayState, QuotaSnapshot};
use crate::store::{self, SnapshotRow};
use crate::tray;
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelData {
    pub snapshot: Option<QuotaSnapshot>,
    /// 配速與預測。免費方案或本期已結束時為 `None`。
    pub pace: Option<PaceReport>,
    /// 併入配速後的顯示狀態。面板一律看這個，不要看 `snapshot.state`
    /// —— 那個是還沒併入配速的基礎狀態。
    pub state: DisplayState,
    pub last_error: Option<String>,
    pub has_credentials: bool,
    /// 憑證被拒絕。面板應回到匯入畫面；輪詢已暫停。
    pub needs_login: bool,
    /// `client_token` 的到期時刻（spec §4.4）。里程碑 1／2 存下的舊憑證
    /// 沒有這個值，會是 `None` —— 那時不顯示橫幅。
    pub client_token_expires_at: Option<DateTime<Utc>>,
    /// 該不該顯示「把系統匣圖示拖出溢位區」的提示（spec §7.1）。
    pub show_tray_hint: bool,
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, Arc<AppState>>) -> PanelData {
    panel_data(state.inner())
}

pub fn panel_data(state: &AppState) -> PanelData {
    let snapshot = state.snapshot.lock().unwrap().clone();
    let pace = state.pace.lock().unwrap().clone();
    let display = match snapshot.as_ref() {
        Some(snapshot) => pace::display_state(snapshot, pace.as_ref()),
        None => DisplayState::Normal,
    };
    // 只讀一次金鑰儲存區：有沒有憑證、憑證什麼時候到期，是同一個問題的兩面。
    let stored = state.store.load().ok().flatten();

    PanelData {
        snapshot,
        pace,
        state: display,
        last_error: state.display_error(),
        has_credentials: stored.is_some(),
        client_token_expires_at: stored.and_then(|stored| stored.client_token_expires_at),
        needs_login: state.needs_login.load(Ordering::SeqCst),
        // 溢位區是 Windows 才有的東西。在 Rust 這邊判平台，不要讓 JS 去猜
        // —— 前端能拿到的只有 user agent，那是出了名的不可靠。
        show_tray_hint: cfg!(target_os = "windows")
            && !store::load_ui_state(&state.ui_state_path()).tray_hint_dismissed,
    }
}

/// 關掉系統匣溢位區的提示。錯誤走回傳通道。
#[tauri::command]
pub fn dismiss_tray_hint(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    dismiss_hint(state.inner())
}

pub fn dismiss_hint(state: &AppState) -> Result<(), String> {
    let path = state.ui_state_path();
    let mut ui = store::load_ui_state(&path);
    ui.tray_hint_dismissed = true;
    store::save_ui_state(&path, &ui)
}

/// 目前生效的設定。
pub fn read_schedule(state: &AppState) -> Schedule {
    state.schedule.lock().unwrap().clone()
}

/// 寫入設定並更新記憶體快取。內容不合法時原封不動回報錯誤。
pub fn write_schedule(state: &AppState, schedule: Schedule) -> Result<(), String> {
    store::save(&state.schedule_path(), &schedule)?;
    *state.schedule.lock().unwrap() = schedule;
    Ok(())
}

#[tauri::command]
pub fn get_schedule(state: State<'_, Arc<AppState>>) -> Schedule {
    read_schedule(state.inner())
}

#[tauri::command]
pub fn set_schedule(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    schedule: Schedule,
) -> Result<(), String> {
    let state = state.inner();
    write_schedule(state, schedule)?;
    // 設定一改，門檻與預測立刻不同 —— 系統匣不能等到下個週期才更新。
    recompute_pace(state, Utc::now());
    tray::sync(&app, state);
    Ok(())
}

/// 把面板上這一份設定另存成 JSON（spec §8）。
///
/// 收的是前端當下的草稿而不是檔案裡那一份：使用者改了幾行還沒按儲存就按
/// 匯出，匯出的卻是舊的，沒有人猜得到為什麼。內容不合法時 `store::save`
/// 會擋下來，訊息和按儲存時看到的一模一樣。
///
/// 錯誤走**回傳通道**（比照 `set_schedule`），不寫 `last_error` ——
/// 這是使用者當下的操作，訊息該出現在按鈕旁邊，而不是變成黏在主面板上的
/// 一行紅字。使用者取消存檔不是錯誤。
#[tauri::command]
pub async fn export_schedule(app: AppHandle, schedule: Schedule) -> Result<(), String> {
    schedule.validate()?;

    // 檔案視窗會擋住呼叫它的執行緒，不能擋在 async runtime 的工作執行緒上。
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("匯出不可遊玩時段")
            .set_file_name(store::SCHEDULE_FILE)
            .add_filter("JSON", &["json"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("開啟存檔視窗失敗：{e}"))?;

    let Some(picked) = picked else {
        return Ok(());
    };
    let path = picked
        .into_path()
        .map_err(|e| format!("取得存檔路徑失敗：{e}"))?;
    store::save(&path, &schedule)
}

/// 從 JSON 檔讀回設定（spec §8）。取消選檔不是錯誤。
///
/// 錯誤同樣走回傳通道。讀檔、解析、驗證都由 `store::load` 一手包辦，
/// 訊息也已經是可以直接顯示的中文。
#[tauri::command]
pub async fn import_schedule(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let handle = app.clone();

    let picked = tauri::async_runtime::spawn_blocking(move || {
        handle
            .dialog()
            .file()
            .set_title("匯入不可遊玩時段")
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("開啟選檔視窗失敗：{e}"))?;

    let Some(picked) = picked else {
        return Ok(());
    };
    let path = picked
        .into_path()
        .map_err(|e| format!("取得檔案路徑失敗：{e}"))?;

    import_schedule_from(&state, &path)?;
    // 設定一換，門檻與預測立刻不同 —— 系統匣不能等到下個週期才更新。
    recompute_pace(&state, Utc::now());
    tray::sync(&app, &state);
    Ok(())
}

/// 讀一個設定檔並套用。不碰 `AppHandle`，可以直接測。
pub fn import_schedule_from(state: &AppState, path: &std::path::Path) -> Result<(), String> {
    // `store::load` 把「檔案不存在」當成「空設定」（全新安裝沒有設定不是
    // 錯誤）。那個語意對匯入完全不適用：選了一個不存在的檔，結果會是把
    // 使用者現有的設定無聲清光。
    if !path.exists() {
        return Err("選到的檔案不見了".to_string());
    }
    let schedule = store::load(path)?;
    write_schedule(state, schedule)
}

/// 依目前快照與設定重算配速，結果寫回共用狀態。
///
/// 每個輪詢週期都要呼叫，即使這次抓取失敗：`A_past` 隨時間變大，
/// 同一份快照的配速結論過幾小時就不一樣了。
/// 順便從檔案重讀設定，使用者手動改檔案不必重開程式。
pub fn recompute_pace(state: &AppState, now: DateTime<Utc>) {
    // 壞掉的設定檔要講出來，不能靜默沿用上一份：使用者手改壞了卻看到
    // 一切正常，只會以為預測本來就長這樣。訊息存在自己的格子裡，
    // 檔案修好的下一輪就清掉，不必等抓取成功。顯示順序由
    // `AppState::display_error()` 決定：憑證與抓取的問題比設定檔急。
    match store::load(&state.schedule_path()) {
        Ok(fresh) => {
            *state.schedule.lock().unwrap() = fresh;
            *state.settings_error.lock().unwrap() = None;
        }
        Err(message) => *state.settings_error.lock().unwrap() = Some(message),
    }
    let schedule = read_schedule(state);
    let snapshot = state.snapshot.lock().unwrap().clone();
    let report = snapshot
        .as_ref()
        .and_then(|snapshot| pace::compute(snapshot, &schedule, now, pace::machine_tz()));
    *state.pace.lock().unwrap() = report;
}

/// 從本機安裝的 GFN 客戶端匯入憑證。
#[tauri::command]
pub async fn import_from_local_gfn(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner();
    let result = link_local(state).await;
    finish_link(&app, state, result).await
}

/// 手動貼上 `starfleetSession.data` 的原始字串。給 Mac 等沒裝 GFN 的機器用。
#[tauri::command]
pub async fn import_manual(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    data: String,
) -> Result<(), String> {
    let state = state.inner();
    let result = link_manual(state, &data).await;
    finish_link(&app, state, result).await
}

/// 匯入之後：系統匣先回到未取得資料的樣子，成功的話立刻抓一次 ——
/// 使用者不該匯入完還得自己按「立即更新」。抓取的結果只寫進狀態，
/// 不影響匯入本身的回傳值。
async fn finish_link<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
    result: Result<(), String>,
) -> Result<(), String> {
    tray::sync(app, state);
    result?;
    let _ = refresh_into_state(app, state).await;
    Ok(())
}

/// 走 localhost 迴圈 OAuth 登入（spec §4.1 主要路徑）。
///
/// 錯誤走**兩條路**：寫進 `last_error`（面板上看得到），同時往外丟
/// （按鈕旁也看得到）。這條路長達五分鐘，而且開瀏覽器會把面板弄到失焦
/// 收起來 —— 只靠其中一條的話，使用者很可能兩邊都沒看到。
#[tauri::command]
pub async fn start_login(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let state = state.inner();

    let pending = match begin_login(state).await {
        Ok(pending) => pending,
        Err(message) => {
            tray::sync(&app, state);
            return Err(message);
        }
    };

    if let Err(e) = app.opener().open_url(pending.url.clone(), None::<&str>) {
        return Err(record_error(state, format!("開不了瀏覽器：{e}")));
    }

    let result = finish_login(state, pending).await;

    // 開瀏覽器會把面板弄到失焦，失焦處理器就把它收起來了。
    // 不叫回來給使用者看結果，等於整件事無聲無息地結束。
    panel::show_default(&app);
    finish_link(&app, state, result).await
}

/// 登入流程的前半段：綁迴圈埠、產生 PKCE 與 nonce、組出授權網址。
///
/// 分成兩半是因為中間那一步要開瀏覽器（需要 `AppHandle`），
/// 而前後兩半都是可以離線測試的。
#[derive(Debug)]
pub struct Pending {
    pub listener: tokio::net::TcpListener,
    pub url: String,
    pub redirect_uri: String,
    pub verifier: String,
    pub nonce: String,
}

pub async fn begin_login(state: &AppState) -> Result<Pending, String> {
    let prepare = async {
        let (listener, port) = oauth::bind_first_free().await?;
        let redirect_uri = oauth::redirect_uri(port);
        let verifier = oauth::verifier()?;
        // nonce 的需求和 verifier 一模一樣：夠長、不可預測、URL 安全。
        let nonce = oauth::verifier()?;
        let url = oauth::authorize_url(
            &state.auth_base,
            &redirect_uri,
            &oauth::challenge(&verifier),
            &nonce,
        );
        Ok::<_, GfnError>(Pending {
            listener,
            url,
            redirect_uri,
            verifier,
            nonce,
        })
    };
    prepare
        .await
        .map_err(|e| record_error(state, e.to_string()))
}

/// 後半段：等授權碼回來、換成憑證、寫進金鑰儲存區。
pub async fn finish_login(state: &AppState, pending: Pending) -> Result<(), String> {
    let run = async {
        let code = oauth::wait_for_code(pending.listener, oauth::LOGIN_TIMEOUT).await?;
        let (session, id_token) = oauth::exchange(
            &state.http,
            &state.auth_base,
            &code,
            &pending.redirect_uri,
            &pending.verifier,
            &pending.nonce,
            Utc::now(),
        )
        .await?;

        // 換碼已經給了一顆能用的 id_token，連同憑證一起收下 ——
        // 在這裡再刷新一次只是白白多鑄一顆，離同時有效上限更近一步。
        state
            .tokens
            .replace_credentials_with_token(session, Some(&id_token))
            .await?;
        Ok::<_, GfnError>(())
    };

    match run.await {
        Ok(()) => {
            reset_state(state);
            Ok(())
        }
        Err(e) => Err(record_error(state, e.to_string())),
    }
}

/// 清除憑證，回到未登入狀態。
#[tauri::command]
pub async fn sign_out(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let state = state.inner();
    let result = unlink_account(state).await;
    tray::sync(&app, state);
    result
}

/// 使用者主動更新：即使黏在「需重新登入」也試一次，最多鑄一顆 token。
#[tauri::command]
pub async fn refresh_now(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    refresh_into_state(&app, state.inner()).await.map(|_| ())
}

/// 面板開啟時的更新。尊重黏住的「需重新登入」——
/// 不然每開一次面板就鑄一顆 token，等於從前門把 token 上限撞滿。
#[tauri::command]
pub async fn refresh_if_due(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    if !poll_due(&state) {
        return Ok(());
    }
    refresh_into_state(&app, state.inner()).await.map(|_| ())
}

// ---- 以下只動共用狀態、不碰系統匣，可以直接測試 ----

async fn link_local(state: &AppState) -> Result<(), String> {
    let session = default_shared_storage_path()
        .ok_or_else(|| "這個平台沒有 GeForce NOW 客戶端資料，請改用手動貼上".to_string())
        .and_then(|path| read_shared_storage(&path).map_err(|e| e.to_string()));
    match session {
        Ok(session) => link_account(state, session).await,
        Err(message) => Err(record_error(state, message)),
    }
}

pub async fn link_manual(state: &AppState, data: &str) -> Result<(), String> {
    match decode_session_data(data) {
        Ok(session) => link_account(state, session).await,
        Err(e) => Err(record_error(state, e.to_string())),
    }
}

/// 寫入新憑證並清掉所有舊狀態。舊帳號還沒過期的 id_token 由
/// `replace_credentials` 一併丟棄，否則畫面上會顯示錯的人的額度。
pub async fn link_account(state: &AppState, session: ImportedSession) -> Result<(), String> {
    state
        .tokens
        .replace_credentials(session.into())
        .await
        .map_err(|e| record_error(state, e.to_string()))?;
    reset_state(state);
    Ok(())
}

pub async fn unlink_account(state: &AppState) -> Result<(), String> {
    state
        .tokens
        .clear_credentials()
        .await
        .map_err(|e| record_error(state, e.to_string()))?;
    reset_state(state);
    Ok(())
}

/// 輪詢與面板開啟時是否該抓資料。黏在「需重新登入」時不抓：
/// 每次抓都會為了 401 重試再鑄一顆 token。
pub fn poll_due(state: &AppState) -> bool {
    !state.needs_login.load(Ordering::SeqCst)
}

/// 牆鐘比單調時鐘多走這麼多，就當作機器剛從睡眠中醒來。
///
/// 30 秒的心跳加上一般的排程延遲，兩個時鐘不會差到兩分鐘。
pub const WAKE_JUMP: chrono::Duration = chrono::Duration::minutes(2);

/// 機器是不是剛從睡眠中醒來（spec §8 把「喚醒」列為輪詢時機之一）。
///
/// 睡眠期間牆鐘照走，`Instant` 不一定 —— Windows 上兩者的行為不保證一致，
/// 所以看的是「兩個時鐘之間的差」而不是任一邊的絕對值。這樣就不必去碰
/// 平台原生的電源事件，也不必為 Windows 與 macOS 各寫一份。
///
/// 只抓往前跳。牆鐘往回跳是 NTP 校時或使用者改時間，不是喚醒。
pub fn woke_from_sleep(monotonic: std::time::Duration, wall: chrono::Duration) -> bool {
    let monotonic =
        chrono::Duration::from_std(monotonic).unwrap_or_else(|_| chrono::Duration::zero());
    wall - monotonic > WAKE_JUMP
}

/// 把這次抓到的快照記進歷史。剩餘時數與上一筆相同時跳過（spec §8），
/// 否則閒置一整天就會多出 288 列一模一樣的資料。
///
/// 「上一筆」指的是這個行程記憶體裡的上一次，不是檔案的最後一行 ——
/// 所以**重開程式後的第一筆一定會寫**，即使它和檔案最後一行重複。
/// 為了省那一列而在每次啟動時去讀檔案尾巴，不值得。
///
/// 寫不進去不影響抓取本身：歷史是留給未來的趨勢圖用的，現在沒有人讀它，
/// 為了它讓一次成功的抓取變成失敗完全不划算。只記到 stderr。
fn record_history(state: &AppState, snapshot: &QuotaSnapshot) {
    let unchanged = state
        .snapshot
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|previous| previous.remaining_minutes == snapshot.remaining_minutes);
    if unchanged {
        return;
    }

    if let Err(message) =
        store::append_history(&state.history_path(), &SnapshotRow::from_snapshot(snapshot))
    {
        eprintln!("{message}");
    }
}

/// 把錯誤寫進狀態供面板顯示，並原樣回傳方便串接。
fn record_error(state: &AppState, message: String) -> String {
    *state.last_error.lock().unwrap() = Some(message.clone());
    message
}

fn reset_state(state: &AppState) {
    *state.snapshot.lock().unwrap() = None;
    *state.pace.lock().unwrap() = None;
    *state.last_error.lock().unwrap() = None;
    state.needs_login.store(false, Ordering::SeqCst);
}

/// 抓取一次訂閱資料並轉成顯示模型。純粹取資料，不寫任何共用狀態。
///
/// 所有取得資料的路徑都經過 `ensure_token()`，也就是刷新 mutex 的唯一入口，
/// 所以這裡的 401 重試不可能與定時輪詢同時燒掉輪替憑證。
async fn fetch_snapshot(state: &AppState) -> Result<QuotaSnapshot, GfnError> {
    let id_token = state.tokens.ensure_token().await?;

    let subscription = match fetch_subscription(&state.http, &state.mes_base, &id_token).await {
        // 401 代表快取的 token 失效了：丟棄後重試一次。
        Err(GfnError::NeedsLogin) => {
            state.tokens.invalidate().await;
            let fresh = state.tokens.ensure_token().await?;
            fetch_subscription(&state.http, &state.mes_base, &fresh).await?
        }
        other => other?,
    };

    Ok(QuotaSnapshot::from_subscription(&subscription, Utc::now()))
}

/// 抓取並把結果寫進共用狀態。失敗時保留上一次的快照，只記錄錯誤供面板顯示。
///
/// 還沒匯入（`NotLinked`）不是錯誤，不記 —— 全新安裝時輪詢就在跑了，
/// 登入畫面不該一開始就掛著紅字。重試後仍 401 則黏住 `needs_login`。
pub async fn refresh_state(state: &AppState) -> Result<QuotaSnapshot, String> {
    match fetch_snapshot(state).await {
        Ok(snapshot) => {
            // 要比對的「上一筆」就是還沒被覆寫掉的那一份，順序不能顛倒。
            record_history(state, &snapshot);
            *state.snapshot.lock().unwrap() = Some(snapshot.clone());
            *state.last_error.lock().unwrap() = None;
            state.needs_login.store(false, Ordering::SeqCst);
            Ok(snapshot)
        }
        Err(GfnError::NotLinked) => Err(GfnError::NotLinked.to_string()),
        Err(e) => {
            if matches!(e, GfnError::NeedsLogin) {
                state.needs_login.store(true, Ordering::SeqCst);
            }
            Err(record_error(state, e.to_string()))
        }
    }
}

/// 抓取、寫進狀態、同步系統匣。
///
/// 這是唯一的更新入口，系統匣也在這裡更新，不由呼叫端各自負責 ——
/// 面板的「立即更新」原本就是漏了那一步，資料進來了但圖示沒變。
pub async fn refresh_into_state<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
) -> Result<QuotaSnapshot, String> {
    let result = refresh_state(state).await;
    recompute_pace(state, Utc::now());
    tray::sync(app, state);
    result
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::auth::store::{MemoryStore, StoredSession, TokenStore};

    const FIXTURE: &str = include_str!("../tests/fixtures/subscription.json");
    const FAR_FUTURE_JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjk5OTk5OTk5OTl9.sig";
    /// base64( urlencode( {"clientToken":"CT123","user":{"sub":"SUB456"}} ) )
    const SAMPLE_DATA: &str = "JTdCJTIyY2xpZW50VG9rZW4lMjIlM0ElMjJDVDEyMyUyMiUyQyUyMnVzZXIlMjIlM0ElN0IlMjJzdWIlMjIlM0ElMjJTVUI0NTYlMjIlN0QlN0Q=";

    struct Harness {
        server: MockServer,
        store: Arc<MemoryStore>,
        state: Arc<AppState>,
        /// 綁著生命週期，drop 掉就會刪除暫存目錄。
        _settings: tempfile::TempDir,
    }

    async fn harness(linked: bool) -> Harness {
        let server = MockServer::start().await;
        let store = Arc::new(MemoryStore::new());
        if linked {
            store
                .save(&StoredSession {
                    client_token: "CT-OLD".into(),
                    sub: "SUB456".into(),
                    client_token_expires_at: None,
                })
                .unwrap();
        }
        let settings = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::with(
            store.clone(),
            reqwest::Client::new(),
            &server.uri(),
            &server.uri(),
            settings.path().to_path_buf(),
        ));
        Harness {
            server,
            store,
            state,
            _settings: settings,
        }
    }

    async fn mount_token(server: &MockServer, expect: u64) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "AT",
                "token_type": "Bearer",
                "expires_in": 3600,
                "client_token": "CT-NEW",
                "id_token": FAR_FUTURE_JWT,
            })))
            .expect(expect)
            .mount(server)
            .await;
    }

    fn subscriptions_ok() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(FIXTURE, "application/json")
    }

    async fn mount_subscriptions(server: &MockServer, template: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/v4/subscriptions"))
            .respond_with(template)
            .mount(server)
            .await;
    }

    /// 第一次回 `first`，之後都回 `then`。
    async fn mount_subscriptions_sequence(
        server: &MockServer,
        first: ResponseTemplate,
        then: ResponseTemplate,
    ) {
        Mock::given(method("GET"))
            .and(path("/v4/subscriptions"))
            .respond_with(first)
            .up_to_n_times(1)
            .expect(1)
            .mount(server)
            .await;
        mount_subscriptions(server, then).await;
    }

    /// 把 fixture 的 `remainingTimeInMinutes` 換成別的值。
    fn subscriptions_with_remaining(remaining: u32) -> ResponseTemplate {
        let mut body: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        *body
            .pointer_mut("/remainingTimeInMinutes")
            .expect("fixture 應有 remainingTimeInMinutes") = remaining.into();
        ResponseTemplate::new(200).set_body_json(body)
    }

    fn history_lines(state: &AppState) -> Vec<String> {
        match std::fs::read_to_string(state.history_path()) {
            Ok(text) => text.lines().map(str::to_string).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 閒置時每 5 分鐘抓一次，剩餘時數不動。每次都寫的話一天就多出
    /// 288 列一模一樣的資料。
    #[tokio::test]
    async fn an_unchanged_remaining_time_is_not_written_twice() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions(&h.server, subscriptions_ok()).await;

        refresh_state(&h.state).await.unwrap();
        refresh_state(&h.state).await.unwrap();

        assert_eq!(history_lines(&h.state).len(), 1);
    }

    #[tokio::test]
    async fn a_changed_remaining_time_appends_a_row() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions_sequence(
            &h.server,
            subscriptions_ok(),
            subscriptions_with_remaining(6000),
        )
        .await;

        refresh_state(&h.state).await.unwrap();
        refresh_state(&h.state).await.unwrap();

        assert_eq!(history_lines(&h.state).len(), 2);
    }

    /// 抓取失敗不該留下任何一列 —— 歷史是「這一刻的事實」，
    /// 沒抓到就是沒有事實。
    #[tokio::test]
    async fn a_failed_fetch_writes_no_history() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions(&h.server, ResponseTemplate::new(500)).await;

        assert!(refresh_state(&h.state).await.is_err());

        assert!(history_lines(&h.state).is_empty());
    }

    /// 埠全被占用時（GFN 客戶端正在登入）要講人話，而且不能動到既有憑證。
    #[tokio::test]
    async fn a_login_that_cannot_bind_leaves_the_credentials_alone() {
        let h = harness(true).await;
        let mut held = Vec::new();
        for port in crate::auth::oauth::PORTS {
            match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
                Ok(listener) => held.push(listener),
                // 某個埠本來就被別的東西占著，這個測試就失去意義了 ——
                // 與其偽陽性通過，不如直接跳過。
                Err(_) => return,
            }
        }

        let problem = begin_login(&h.state).await.unwrap_err();

        assert!(problem.contains("連接埠"), "{problem}");
        assert_eq!(h.store.load().unwrap().unwrap().client_token, "CT-OLD");
        // 登入失敗要看得見，不能只是按了沒反應。
        assert_eq!(last_error(&h.state), Some(problem));
    }

    /// 授權網址要指向注入的 auth_base，不是寫死的 login.nvidia.com ——
    /// 不然測試與正式環境會打到不同的地方。
    #[tokio::test]
    async fn the_authorize_url_points_at_the_configured_endpoint() {
        let h = harness(false).await;

        let pending = begin_login(&h.state).await.unwrap();

        assert!(pending
            .url
            .starts_with(&format!("{}/authorize?", h.server.uri())));
        // verifier 與 nonce 各自獨立產生，不能是同一個值。
        assert_ne!(pending.verifier, pending.nonce);
    }

    #[test]
    fn a_normal_tick_is_not_a_wake_up() {
        assert!(!woke_from_sleep(
            std::time::Duration::from_secs(30),
            chrono::Duration::seconds(30)
        ));
    }

    /// 排程延遲、時鐘微調、忙碌的機器 —— 這些都會讓兩個時鐘差個幾秒，
    /// 但差不到兩分鐘。
    #[test]
    fn a_small_drift_is_not_a_wake_up() {
        assert!(!woke_from_sleep(
            std::time::Duration::from_secs(30),
            chrono::Duration::seconds(75)
        ));
    }

    /// 闔上筆電八小時：牆鐘走了八小時，單調時鐘幾乎沒動。
    #[test]
    fn a_large_jump_is_a_wake_up() {
        assert!(woke_from_sleep(
            std::time::Duration::from_secs(31),
            chrono::Duration::hours(8)
        ));
    }

    /// 牆鐘往回跳（NTP 校時、使用者改時間）不是喚醒，也不該當成喚醒 ——
    /// 硬要抓一次只是白白多鑄一顆 token。
    #[test]
    fn a_backwards_clock_is_not_a_wake_up() {
        assert!(!woke_from_sleep(
            std::time::Duration::from_secs(30),
            chrono::Duration::seconds(-3600)
        ));
    }

    /// 到期時刻要送到面板去，橫幅才有東西可以算。
    #[tokio::test]
    async fn the_panel_learns_when_the_credential_expires() {
        let h = harness(false).await;
        let expires_at = Utc::now() + chrono::Duration::days(3);
        h.store
            .save(&StoredSession {
                client_token: "CT".into(),
                sub: "SUB".into(),
                client_token_expires_at: Some(expires_at),
            })
            .unwrap();

        assert_eq!(
            panel_data(&h.state).client_token_expires_at,
            Some(expires_at)
        );
    }

    /// 里程碑 1／2 存下的憑證沒有這個欄位。不知道就是不知道，
    /// 不要編一個日期出來。
    #[tokio::test]
    async fn an_older_credential_reports_no_expiry() {
        let h = harness(true).await;

        assert_eq!(panel_data(&h.state).client_token_expires_at, None);
    }

    #[tokio::test]
    async fn dismissing_the_tray_hint_sticks() {
        let h = harness(false).await;
        assert_eq!(panel_data(&h.state).show_tray_hint, cfg!(windows));

        dismiss_hint(&h.state).unwrap();

        assert!(!panel_data(&h.state).show_tray_hint);
    }

    /// 首次啟動的旗標與提示旗標各自獨立：關掉提示不該讓下次啟動又自己彈出來。
    #[tokio::test]
    async fn dismissing_the_hint_keeps_the_first_run_flag() {
        let h = harness(false).await;
        let path = h.state.ui_state_path();
        store::save_ui_state(
            &path,
            &store::UiState {
                first_run_done: true,
                tray_hint_dismissed: false,
            },
        )
        .unwrap();

        dismiss_hint(&h.state).unwrap();

        assert!(store::load_ui_state(&path).first_run_done);
    }

    fn workdays() -> Schedule {
        Schedule {
            weekly: vec![crate::pace::schedule::WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4],
                start_minute: 540,
                end_minute: 1080,
                note: "上班".into(),
            }],
            exceptions: Vec::new(),
        }
    }

    /// 匯出寫出去的檔案，匯入要讀得回同一份設定 ——
    /// 這是「搬到另一台機器」的全部意義。
    #[tokio::test]
    async fn an_exported_schedule_can_be_imported_again() {
        let h = harness(false).await;
        let exported = h.state.settings_dir.join("exported.json");

        store::save(&exported, &workdays()).unwrap();
        import_schedule_from(&h.state, &exported).unwrap();

        assert_eq!(read_schedule(&h.state), workdays());
    }

    /// 匯入壞掉的檔案時，現有設定必須原封不動 ——
    /// 「匯入失敗」和「設定被清空」是兩件差很多的事。
    #[tokio::test]
    async fn a_broken_import_leaves_the_current_schedule_alone() {
        let h = harness(false).await;
        write_schedule(&h.state, workdays()).unwrap();
        let broken = h.state.settings_dir.join("broken.json");
        std::fs::write(&broken, "{ not json").unwrap();

        assert!(import_schedule_from(&h.state, &broken).is_err());

        assert_eq!(read_schedule(&h.state), workdays());
    }

    /// 使用者在檔案視窗開著時把檔案刪了。`store::load` 對「檔案不存在」
    /// 的回應是「空設定」，直接沿用會把使用者的設定無聲清光。
    #[tokio::test]
    async fn importing_a_missing_file_is_an_error_not_an_empty_schedule() {
        let h = harness(false).await;
        write_schedule(&h.state, workdays()).unwrap();

        assert!(import_schedule_from(&h.state, &h.state.settings_dir.join("gone.json")).is_err());

        assert_eq!(read_schedule(&h.state), workdays());
    }

    fn last_error(state: &AppState) -> Option<String> {
        state.last_error.lock().unwrap().clone()
    }

    fn needs_login(state: &AppState) -> bool {
        state.needs_login.load(Ordering::SeqCst)
    }

    /// 全新安裝時輪詢就開始跑了；還沒匯入不是錯誤，登入畫面不該掛著紅字。
    #[tokio::test]
    async fn refresh_without_credentials_records_no_error() {
        let h = harness(false).await;

        assert!(refresh_state(&h.state).await.is_err());

        assert_eq!(last_error(&h.state), None);
        assert!(!needs_login(&h.state));
    }

    /// 貼了壞憑證，面板要看得到為什麼。
    #[tokio::test]
    async fn invalid_pasted_credential_is_recorded_as_last_error() {
        let h = harness(false).await;

        assert!(link_manual(&h.state, "not base64 at all !!!")
            .await
            .is_err());

        assert!(last_error(&h.state).unwrap().contains("憑證格式無效"));
        assert_eq!(h.store.load().unwrap(), None);
    }

    #[tokio::test]
    async fn refresh_retries_once_after_a_401() {
        let h = harness(true).await;
        mount_token(&h.server, 2).await;
        mount_subscriptions_sequence(&h.server, ResponseTemplate::new(401), subscriptions_ok())
            .await;

        let snapshot = refresh_state(&h.state).await.unwrap();

        assert_eq!(snapshot.remaining_minutes, 6180);
        assert_eq!(last_error(&h.state), None);
        assert!(h.state.snapshot.lock().unwrap().is_some());
    }

    /// 持續 401 時只准鑄兩顆（第一次 + 重試），然後黏住、暫停輪詢。
    /// 否則每 5 分鐘一顆，一小時就把「同時有效 token 上限」撞滿。
    #[tokio::test]
    async fn persistent_401_marks_needs_login_and_pauses_polling() {
        let h = harness(true).await;
        mount_token(&h.server, 2).await;
        mount_subscriptions(&h.server, ResponseTemplate::new(401)).await;

        assert!(refresh_state(&h.state).await.is_err());

        assert!(needs_login(&h.state));
        assert!(!poll_due(&h.state));
        assert_eq!(last_error(&h.state).as_deref(), Some("需要重新登入"));
    }

    #[tokio::test]
    async fn a_successful_refresh_clears_needs_login() {
        let h = harness(true).await;
        h.state.needs_login.store(true, Ordering::SeqCst);
        mount_token(&h.server, 1).await;
        mount_subscriptions(&h.server, subscriptions_ok()).await;

        refresh_state(&h.state).await.unwrap();

        assert!(!needs_login(&h.state));
        assert!(poll_due(&h.state));
    }

    #[tokio::test]
    async fn linking_an_account_resets_the_sticky_state() {
        let h = harness(true).await;
        h.state.needs_login.store(true, Ordering::SeqCst);
        *h.state.last_error.lock().unwrap() = Some("需要重新登入".into());

        link_manual(&h.state, SAMPLE_DATA).await.unwrap();

        assert_eq!(h.store.load().unwrap().unwrap().client_token, "CT123");
        assert!(!needs_login(&h.state));
        assert_eq!(last_error(&h.state), None);
        assert!(poll_due(&h.state));
    }

    #[tokio::test]
    async fn unlinking_clears_the_store_and_the_snapshot() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions(&h.server, subscriptions_ok()).await;
        refresh_state(&h.state).await.unwrap();

        unlink_account(&h.state).await.unwrap();

        assert_eq!(h.store.load().unwrap(), None);
        assert!(h.state.snapshot.lock().unwrap().is_none());
        assert_eq!(last_error(&h.state), None);
    }

    /// 網路失敗保留上次的快照，只記錄錯誤（spec §9）。
    #[tokio::test]
    async fn a_network_error_keeps_the_last_snapshot() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions_sequence(&h.server, subscriptions_ok(), ResponseTemplate::new(500))
            .await;

        refresh_state(&h.state).await.unwrap();
        assert!(refresh_state(&h.state).await.is_err());

        assert!(h.state.snapshot.lock().unwrap().is_some());
        assert!(last_error(&h.state).unwrap().contains("網路錯誤"));
        assert!(!needs_login(&h.state));
    }

    #[tokio::test]
    async fn the_schedule_round_trips_through_the_commands() {
        use crate::pace::schedule::{Schedule, WeeklyWindow};

        let h = harness(false).await;
        let schedule = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![0, 1, 2, 3, 4],
                start_minute: 540,
                end_minute: 1080,
                note: "上班".into(),
            }],
            exceptions: Vec::new(),
        };

        write_schedule(&h.state, schedule.clone()).unwrap();
        assert_eq!(read_schedule(&h.state), schedule);
        assert!(h.state.schedule_path().exists());
    }

    #[tokio::test]
    async fn an_invalid_schedule_is_refused_and_the_old_one_survives() {
        use crate::pace::schedule::{Schedule, WeeklyWindow};

        let h = harness(false).await;
        let bad = Schedule {
            weekly: vec![WeeklyWindow {
                weekdays: vec![],
                start_minute: 0,
                end_minute: 60,
                note: String::new(),
            }],
            exceptions: Vec::new(),
        };
        assert!(write_schedule(&h.state, bad).is_err());
        assert_eq!(read_schedule(&h.state), Schedule::default());
    }

    /// 抓取成功後配速要跟著算好，面板不必自己再要一次。
    #[tokio::test]
    async fn refreshing_also_computes_the_pace() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions(&h.server, subscriptions_ok()).await;

        refresh_state(&h.state).await.unwrap();
        recompute_pace(&h.state, Utc::now());

        assert!(h.state.pace.lock().unwrap().is_some());
    }

    /// 抓取失敗時配速照算：A_past 會隨時間變大，狀態可能翻轉。
    #[tokio::test]
    async fn the_pace_is_recomputed_even_without_a_fresh_snapshot() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions(&h.server, subscriptions_ok()).await;
        refresh_state(&h.state).await.unwrap();

        *h.state.pace.lock().unwrap() = None;
        recompute_pace(&h.state, Utc::now());
        assert!(h.state.pace.lock().unwrap().is_some());
    }

    /// 手動改壞設定檔要看得見，不能靜默沿用上一份。
    #[tokio::test]
    async fn a_broken_settings_file_is_reported_to_the_panel() {
        let h = harness(false).await;
        std::fs::write(h.state.schedule_path(), "{ not json").unwrap();

        recompute_pace(&h.state, Utc::now());

        assert!(
            h.state
                .display_error()
                .is_some_and(|e| e.contains("設定檔")),
            "設定檔壞掉要在面板上看得到"
        );
    }

    /// 檔案修好了訊息就要消失，不必等下一次抓取成功。
    #[tokio::test]
    async fn a_repaired_settings_file_clears_the_message() {
        let h = harness(false).await;
        std::fs::write(h.state.schedule_path(), "{ not json").unwrap();
        recompute_pace(&h.state, Utc::now());
        assert!(h.state.display_error().is_some());

        std::fs::write(h.state.schedule_path(), r#"{"weekly":[],"exceptions":[]}"#).unwrap();
        recompute_pace(&h.state, Utc::now());

        assert_eq!(h.state.display_error(), None);
    }

    /// 憑證死了就不抓，本期過不過期都一樣：每抓一次都會為了 401 重試再鑄一顆
    /// token。過期的快照由 `pace::compute()` 回傳 `None` 擋掉，不需要靠輪詢。
    #[tokio::test]
    async fn a_rejected_credential_still_pauses_polling_when_the_span_has_ended() {
        let h = harness(true).await;
        mount_token(&h.server, 1).await;
        mount_subscriptions(&h.server, subscriptions_ok()).await;
        refresh_state(&h.state).await.unwrap();

        h.state.needs_login.store(true, Ordering::SeqCst);
        let mut guard = h.state.snapshot.lock().unwrap();
        guard.as_mut().unwrap().span_end = Some(Utc::now() - chrono::Duration::days(1));
        drop(guard);

        assert!(!poll_due(&h.state));
        recompute_pace(&h.state, Utc::now());
        assert!(
            h.state.pace.lock().unwrap().is_none(),
            "本期已結束就沒有配速可言"
        );
    }

    /// 憑證失效比設定檔急。設定檔的錯誤不能蓋掉「需要重新登入」，
    /// 否則使用者會去修設定檔而不是重新匯入憑證。
    #[tokio::test]
    async fn a_settings_error_does_not_mask_a_credential_error() {
        let h = harness(false).await;
        *h.state.last_error.lock().unwrap() = Some("需要重新登入".into());
        std::fs::write(h.state.schedule_path(), "{ not json").unwrap();

        recompute_pace(&h.state, Utc::now());

        assert_eq!(h.state.display_error().as_deref(), Some("需要重新登入"));
        // 設定檔的問題沒有被丟掉，只是排在後面等憑證修好。
        assert!(h.state.settings_error.lock().unwrap().is_some());
    }
}
