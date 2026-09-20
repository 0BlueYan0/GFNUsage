use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::api::subscriptions::fetch_subscription;
use crate::auth::session::{
    decode_session_data, default_shared_storage_path, read_shared_storage, ImportedSession,
};
use crate::error::GfnError;
use crate::quota::QuotaSnapshot;
use crate::tray;
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelData {
    pub snapshot: Option<QuotaSnapshot>,
    pub last_error: Option<String>,
    pub has_credentials: bool,
    /// 憑證被拒絕。面板應回到匯入畫面；輪詢已暫停。
    pub needs_login: bool,
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, Arc<AppState>>) -> PanelData {
    PanelData {
        snapshot: state.snapshot.lock().unwrap().clone(),
        last_error: state.last_error.lock().unwrap().clone(),
        has_credentials: state.store.load().ok().flatten().is_some(),
        needs_login: state.needs_login.load(Ordering::SeqCst),
    }
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
pub async fn refresh_if_due(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
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

/// 把錯誤寫進狀態供面板顯示，並原樣回傳方便串接。
fn record_error(state: &AppState, message: String) -> String {
    *state.last_error.lock().unwrap() = Some(message.clone());
    message
}

fn reset_state(state: &AppState) {
    *state.snapshot.lock().unwrap() = None;
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
    }

    async fn harness(linked: bool) -> Harness {
        let server = MockServer::start().await;
        let store = Arc::new(MemoryStore::new());
        if linked {
            store
                .save(&StoredSession {
                    client_token: "CT-OLD".into(),
                    sub: "SUB456".into(),
                })
                .unwrap();
        }
        let state = Arc::new(AppState::with(
            store.clone(),
            reqwest::Client::new(),
            &server.uri(),
            &server.uri(),
        ));
        Harness {
            server,
            store,
            state,
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

        assert!(link_manual(&h.state, "not base64 at all !!!").await.is_err());

        assert!(last_error(&h.state).unwrap().contains("憑證格式無效"));
        assert_eq!(h.store.load().unwrap(), None);
    }

    #[tokio::test]
    async fn refresh_retries_once_after_a_401() {
        let h = harness(true).await;
        mount_token(&h.server, 2).await;
        mount_subscriptions_sequence(&h.server, ResponseTemplate::new(401), subscriptions_ok()).await;

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
        mount_subscriptions_sequence(&h.server, subscriptions_ok(), ResponseTemplate::new(500)).await;

        refresh_state(&h.state).await.unwrap();
        assert!(refresh_state(&h.state).await.is_err());

        assert!(h.state.snapshot.lock().unwrap().is_some());
        assert!(last_error(&h.state).unwrap().contains("網路錯誤"));
        assert!(!needs_login(&h.state));
    }
}
