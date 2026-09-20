use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::api::subscriptions::{fetch_subscription, MES_BASE};
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
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, Arc<AppState>>) -> PanelData {
    PanelData {
        snapshot: state.snapshot.lock().unwrap().clone(),
        last_error: state.last_error.lock().unwrap().clone(),
        has_credentials: state.store.load().ok().flatten().is_some(),
    }
}

/// 從本機安裝的 GFN 客戶端匯入憑證。
#[tauri::command]
pub async fn import_from_local_gfn(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let path = default_shared_storage_path()
        .ok_or_else(|| "這個平台沒有 GeForce NOW 客戶端資料，請改用手動貼上".to_string())?;
    let session = read_shared_storage(&path).map_err(|e| e.to_string())?;
    store_credentials(&app, state.inner(), session).await
}

/// 手動貼上 `starfleetSession.data` 的原始字串。給 Mac 等沒裝 GFN 的機器用。
#[tauri::command]
pub async fn import_manual(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    data: String,
) -> Result<(), String> {
    let session = decode_session_data(&data).map_err(|e| e.to_string())?;
    store_credentials(&app, state.inner(), session).await
}

/// 寫入新憑證。
///
/// 一定要一併丟棄快取的 token —— 否則換了帳號之後，舊帳號那顆還沒過期的
/// id_token 會被繼續拿來查詢，畫面上就會顯示錯的人的額度。
async fn store_credentials<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
    session: ImportedSession,
) -> Result<(), String> {
    state.store.save(&session.into()).map_err(|e| e.to_string())?;
    state.tokens.invalidate().await;
    clear_state(app, state);
    Ok(())
}

/// 清除憑證，回到未登入狀態。
///
/// 同樣必須丟棄快取的 token，不然「解除連結」之後按更新還是會成功。
#[tauri::command]
pub async fn sign_out(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.store.clear().map_err(|e| e.to_string())?;
    state.tokens.invalidate().await;
    clear_state(&app, state.inner());
    Ok(())
}

/// 把快照與錯誤一併清空，系統匣也回到未取得資料的樣子。
fn clear_state<R: Runtime>(app: &AppHandle<R>, state: &Arc<AppState>) {
    *state.snapshot.lock().unwrap() = None;
    *state.last_error.lock().unwrap() = None;
    tray::clear(app);
}

/// 抓取一次訂閱資料並轉成顯示模型。純粹取資料，不寫任何共用狀態。
///
/// 所有取得資料的路徑都經過 `ensure_token()`，也就是刷新 mutex 的唯一入口，
/// 所以這裡的 401 重試不可能與定時輪詢同時燒掉輪替憑證。
async fn fetch_snapshot(state: &AppState) -> Result<QuotaSnapshot, GfnError> {
    let id_token = state.tokens.ensure_token().await?;

    let subscription = match fetch_subscription(&state.http, MES_BASE, &id_token).await {
        // 401 代表快取的 token 失效了：丟棄後重試一次。
        Err(GfnError::NeedsLogin) => {
            state.tokens.invalidate().await;
            let fresh = state.tokens.ensure_token().await?;
            fetch_subscription(&state.http, MES_BASE, &fresh).await?
        }
        other => other?,
    };

    Ok(QuotaSnapshot::from_subscription(&subscription, Utc::now()))
}

/// 抓取並把結果寫進共用狀態與系統匣。
///
/// 這是唯一的更新入口，系統匣也在這裡更新，不由呼叫端各自負責 ——
/// 面板的「立即更新」原本就是漏了那一步，資料進來了但圖示沒變。
/// 失敗時保留上一次的快照，只記錄錯誤供面板顯示。
pub async fn refresh_into_state<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
) -> Result<QuotaSnapshot, String> {
    match fetch_snapshot(state).await {
        Ok(snapshot) => {
            *state.snapshot.lock().unwrap() = Some(snapshot.clone());
            *state.last_error.lock().unwrap() = None;
            tray::update(app, &snapshot);
            Ok(snapshot)
        }
        Err(e) => {
            let message = e.to_string();
            *state.last_error.lock().unwrap() = Some(message.clone());
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn refresh_now(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let inner = state.inner().clone();
    refresh_into_state(&app, &inner).await.map(|_| ())
}
