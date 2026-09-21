//! 逐場遊玩紀錄端點探測。
//!
//!   cargo run --example playtime --manifest-path src-tauri/Cargo.toml
//!   cargo run --example playtime --manifest-path src-tauri/Cargo.toml -- \
//!       --idtoken-file <路徑>
//!
//! 回答一個問題：有沒有哪個端點吃這支程式的 id_token，並且吐得出每一場的
//! 起訖與時長。答案是沒有，見 spike 文件的 3a。
//!
//! `--idtoken-file` 是後續：拿一顆從瀏覽器帳號頁攔來的 id_token，問 paywall
//! 與 `mes` 各自收不收。收不收決定 webview 登入能不能取代現在的 OAuth。
//!
//! 為什麼要問：`pace` 的 `today_budget_minutes` 要知道「今天已經用掉多少」，
//! 現在是拿本機 `snapshots.json` 的快照差分算的，程式沒開著就答不出來。
//! `/v4/subscriptions` 只給累計的剩餘時數，沒有逐日也沒有逐場。
//!
//! id_token 走 `TokenManager::ensure_token`，剩餘效期低於 5 分鐘才會打
//! `/token`，所以正常情況下這支程式一次 token 都不鑄。同 `diagnose`，
//! 自我限制最多跑兩次。
//!
//! token 不進輸出。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use gfnusage_lib::api::subscriptions::MES_BASE;
use gfnusage_lib::auth::refresh::{TokenManager, STARFLEET_BASE};
use gfnusage_lib::auth::store::{KeyringStore, TokenStore};
use gfnusage_lib::error::describe_shape;
use gfnusage_lib::{http_client, HTTP_TIMEOUT};

/// 那篇部落格文章給的主機。spike §3 記的服務探索端點可能會給出別的，
/// 所以第 1 步先問它。
const PAYWALL_BASE: &str = "https://api-prod.nvidia.com";
/// spike §3：服務探索（各服務真實網址）。
const PCS_BASE: &str = "https://pcs.geforcenow.com";
/// spec §2 與 spike §5 掛著的待辦，從來沒有人打過。
const UDS_BASE: &str = "https://uds.geforcenow.com";

/// 送哪一種標頭。`mes.geforcenow.com` 吃 `Authorization: Bearer`，
/// 那篇文章說 paywall 吃小寫的 `idtoken`。兩種都要試。
#[derive(Clone, Copy)]
enum Auth {
    Bearer,
    IdToken,
    Both,
    None,
}

impl Auth {
    fn label(self) -> &'static str {
        match self {
            Auth::Bearer => "Authorization: Bearer",
            Auth::IdToken => "idtoken",
            Auth::Both => "idtoken + Authorization: Bearer",
            Auth::None => "（不送憑證）",
        }
    }
}

/// id_token 的 claims。
///
/// 端點回「Starfleet idToken was invalid」而同一顆 token 在
/// `mes.geforcenow.com` 拿得到 200 的時候，要分清楚是過期還是對象不符。
/// 只印 `aud`、`iss`、`azp`、`scope` 與 `exp`，不印 token 本身，也不印 `sub`。
fn print_claims(id_token: &str) {
    use base64::Engine;

    let Some(payload) = id_token.split('.').nth(1) else {
        println!("    不是 JWT 格式");
        return;
    };
    let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        println!("    payload 不是 base64url");
        return;
    };
    let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        println!("    payload 不是 JSON");
        return;
    };

    for name in ["aud", "iss", "azp", "scope"] {
        if let Some(value) = claims.get(name) {
            println!("    {name:<6}{value}");
        }
    }
    match claims.get("exp").and_then(|v| v.as_i64()) {
        Some(exp) => {
            let left = exp - Utc::now().timestamp();
            println!(
                "    exp   還有 {} 分 {} 秒{}",
                left / 60,
                left % 60,
                if left <= 0 { "（已過期）" } else { "" }
            );
        }
        None => println!("    exp   沒有這個欄位"),
    }
}

/// 一次 GET，印狀態與欄位名，成功就把 JSON 交回去。
///
/// 失敗時只印 body 的前 200 字。這些端點的錯誤回應沒觀察過，硬套
/// `describe_shape` 會在「回應不是 JSON」時什麼都看不到。
async fn probe(
    http: &reqwest::Client,
    url: &str,
    auth: Auth,
    id_token: &str,
) -> Option<serde_json::Value> {
    println!("--- GET {url}");
    println!("    {}", auth.label());

    let request = http.get(url).header("accept", "application/json");
    let request = match auth {
        Auth::Bearer => request.bearer_auth(id_token),
        Auth::IdToken => request.header("idtoken", id_token),
        Auth::Both => request.header("idtoken", id_token).bearer_auth(id_token),
        Auth::None => request,
    };

    let response = match request.send().await {
        Ok(response) => response,
        Err(e) => {
            println!("    網路錯誤：{e}");
            return None;
        }
    };

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    println!("    HTTP {status}");

    if !status.is_success() {
        println!("    {}", body.chars().take(200).collect::<String>());
        return None;
    }

    println!("    {}", describe_shape(&body));
    match serde_json::from_str(&body) {
        Ok(value) => Some(value),
        Err(e) => {
            println!("    解析失敗：{e}");
            None
        }
    }
}

fn parse_time(value: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    let text = value?.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// 判定條件 A2 與 A3 要的數字。
///
/// 回應裡陣列的欄位名只有一份部落格文章當依據，所以不寫死 `sessionHistory`，
/// 拿物件裡第一個陣列欄位來看。
fn summarise(value: &serde_json::Value, span_start: Option<DateTime<Utc>>, used_minutes: u32) {
    // 回應有兩個陣列。`sessionHistory` 是逐場，`aggregatedSessionsByGame` 是
    // 同一款連續玩的合併結果（欄位名也不同，是 startDate／endDate）。
    // 要算今天用了多少，逐場那個才對。
    let list = value
        .get("sessionHistory")
        .and_then(|v| v.as_array())
        .or_else(|| value.as_array());

    let Some(list) = list else {
        println!("    回應裡沒有陣列，看不到逐場紀錄。");
        return;
    };

    if list.is_empty() {
        println!("    陣列是空的。本期還沒玩過，或這個端點不是這個意思。");
        return;
    }

    println!("    {} 筆：", list.len());

    let mut sum = 0.0_f64;
    let mut earliest: Option<DateTime<Utc>> = None;
    let mut latest: Option<DateTime<Utc>> = None;
    let mut before_span = 0_usize;

    for item in list {
        let start = parse_time(item.get("sessionStartDate"));
        let end = parse_time(item.get("sessionEndDate"));
        let playtime = item.get("totalPlaytime").and_then(|v| v.as_f64());
        let title = item
            .get("gameTitle")
            .and_then(|v| v.as_str())
            .unwrap_or("（沒有 gameTitle）");

        // 起訖差幾分鐘，拿來對照 totalPlaytime 的單位（判定條件 A3）。
        let span_minutes = match (start, end) {
            (Some(a), Some(b)) => format!("{:.1}", (b - a).num_seconds() as f64 / 60.0),
            _ => "?".to_string(),
        };

        println!(
            "      {}  →  {}  totalPlaytime={}  起訖差={} 分鐘  {title}",
            start.map(|t| t.to_rfc3339()).unwrap_or_else(|| "?".into()),
            end.map(|t| t.to_rfc3339()).unwrap_or_else(|| "?".into()),
            playtime
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".into()),
            span_minutes,
        );

        sum += playtime.unwrap_or(0.0);
        if let Some(start) = start {
            earliest = Some(earliest.map_or(start, |e: DateTime<Utc>| e.min(start)));
            if span_start.is_some_and(|s| start < s) {
                before_span += 1;
            }
        }
        if let Some(end) = end {
            latest = Some(latest.map_or(end, |l: DateTime<Utc>| l.max(end)));
        }
    }

    println!();
    println!("    最早 {earliest:?}");
    println!("    最晚 {latest:?}");
    println!("    totalPlaytime 加總 = {sum:.1}");
    println!("    訂閱說的已使用   = {used_minutes} 分鐘");

    // A2 的前提：API 不保證裁到計費期。早於 span_start 的那些要先濾掉再比，
    // 否則加總本來就會比已使用多，會誤判成「數字對不上」。
    if before_span > 0 {
        println!(
            "    ⚠ 有 {before_span} 筆早於本期起點 —— API 不裁到計費期，\
             正式接的時候要自己濾。"
        );
    }
}

/// 拿一顆從瀏覽器帳號頁攔來的 id_token，問兩件事。
///
/// 這是「改用 webview 登入」之前唯一該先知道的事：帳號頁那顆 token 除了
/// paywall 收，`mes.geforcenow.com` 收不收。
///
/// 收 → 一次登入兩邊都有，換掉現在的 OAuth 才成立。
/// 不收 → 換過去等於要登入兩次，比現在差。
///
/// token 從檔案讀，不從命令列參數 —— 參數會進 shell 歷史。檔案內容不印。
async fn try_browser_token(http: &reqwest::Client, path: &str) {
    let token = match std::fs::read_to_string(path) {
        Ok(text) => text.trim().to_string(),
        Err(e) => {
            println!("讀不到 {path}：{e}");
            return;
        }
    };
    if token.is_empty() {
        println!("{path} 是空的。");
        return;
    }
    println!("讀到 {} 字元。claims：", token.len());
    print_claims(&token);

    println!();
    println!("--- 問題二：mes 收不收（決定能不能取代現在的登入）---");
    let mes = probe(
        http,
        &format!("{MES_BASE}/v4/subscriptions"),
        Auth::Bearer,
        &token,
    )
    .await;

    // 本期起點拿來當查詢區間。那篇文章說跨計費期會拿到不完整的結果，
    // 而訂閱回應直接給了真正的起點，不必按日曆月切。
    let (span_start, used_minutes) = match &mes {
        Some(value) => {
            let field = |name: &str| value.get(name).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let total = field("totalTimeInMinutes");
            let remaining = field("remainingTimeInMinutes");
            let span_start = parse_time(value.get("currentSpanStartDateTime"));
            println!(
                "    本期起點 {span_start:?}　已使用 {} 分鐘",
                total - remaining
            );
            (span_start, total.saturating_sub(remaining))
        }
        None => (None, 0),
    };

    println!();
    println!("--- 問題一：paywall 給不給逐場紀錄 ---");
    let now = Utc::now();
    let from = span_start.unwrap_or(now - chrono::Duration::days(30));

    // 伺服器端是 `vars.req_spanStartDate as DateTime as String {format: "yyyy-MM-dd"}`
    // ——錯誤訊息把那行 DataWeave 直接吐出來了。它要的就是日期。
    //
    // 順帶：`to_rfc3339()` 的 `+00:00` 不能直接放進查詢字串，`+` 在那裡解碼成
    // 空白，伺服器收到的是「2026-09-15T13:18:59.675 00:00」。要用 Z。
    let ranges = [
        (
            "yyyy-MM-dd",
            from.format("%Y-%m-%d").to_string(),
            now.format("%Y-%m-%d").to_string(),
        ),
        (
            "RFC3339 帶 Z",
            from.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ),
    ];

    let mut paywall_ok = false;
    for (label, from, to) in ranges {
        println!();
        println!("[{label}]");
        let url = format!(
            "{PAYWALL_BASE}/gfn-paywall-api/api/v2/userplaytime/sessionshistory\
             ?spanStartDate={from}&spanEndDate={to}"
        );
        let Some(value) = probe(http, &url, Auth::IdToken, &token).await else {
            continue;
        };
        // HTTP 200 也可能包著錯誤。只印 errors，不印 user。
        if let Some(errors) = value.get("errors") {
            println!("    errors: {errors}");
            continue;
        }
        paywall_ok = true;

        // 回應有三個欄位，欄位名只有一篇部落格當依據，先把真正的形狀印出來。
        if let Some(map) = value.as_object() {
            for (key, child) in map {
                match child.as_array() {
                    Some(list) => {
                        println!("    {key}: 陣列，{} 筆", list.len());
                        if let Some(first) = list.first() {
                            println!(
                                "      第一筆 {}",
                                serde_json::to_string(first).unwrap_or_default()
                            );
                        }
                    }
                    None => println!("    {key}: {child}"),
                }
            }
        }
        summarise(&value, span_start, used_minutes);
    }

    println!();
    match (paywall_ok, mes.is_some()) {
        (true, true) => println!("兩邊都收。一次 webview 登入可以取代現在的 OAuth。"),
        (true, false) => {
            println!("只有 paywall 收，mes 不收。換過去要登入兩次，比現在差。");
            println!("這種情況就別動現在的 OAuth，webview 只拿來補逐場紀錄。");
        }
        (false, true) => println!("mes 收，paywall 沒給資料。看上面的 errors。"),
        (false, false) => println!("兩邊都不收。token 抓錯了，或已經過期。"),
    }
}

#[tokio::main]
async fn main() {
    let http = http_client(HTTP_TIMEOUT);

    // 從瀏覽器攔來的 token 走這條，不碰金鑰儲存區也不打 /token。
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--idtoken-file") {
        match args.get(i + 1) {
            Some(path) => try_browser_token(&http, path).await,
            None => println!("--idtoken-file 後面要接檔案路徑。"),
        }
        return;
    }

    let keyring: Arc<dyn TokenStore> = Arc::new(KeyringStore);
    let tokens = TokenManager::new(keyring, http.clone(), STARFLEET_BASE.to_string());

    let id_token = match tokens.ensure_token().await {
        Ok(token) => {
            println!("id_token 取得（{} 字元）", token.len());
            token
        }
        Err(e) => {
            println!("拿不到 id_token：{e}");
            println!("先在面板登入，或跑 diagnose。");
            return;
        }
    };
    print_claims(&id_token);

    println!();
    println!("=== 1. 服務探索 ===");
    println!("先問 NVIDIA 自己列的網址，比猜 {PAYWALL_BASE} 可靠。");
    if let Some(urls) = probe(
        &http,
        &format!("{PCS_BASE}/v1/serviceUrls"),
        Auth::None,
        &id_token,
    )
    .await
    {
        println!(
            "{}",
            serde_json::to_string_pretty(&urls).unwrap_or_default()
        );
    }

    println!();
    println!("=== 2. /v4/subscriptions 的原始欄位 ===");
    println!("spike 的欄位表列了 usedTimeInMs，fixture 裡沒有。有的話記下來，");
    println!("它是累計值，跟 T − R 一樣答不出今天用了多少。");

    let subscription = probe(
        &http,
        &format!("{MES_BASE}/v4/subscriptions"),
        Auth::Bearer,
        &id_token,
    )
    .await;

    let (span_start, used_minutes) = match &subscription {
        Some(value) => {
            let field = |name: &str| value.get(name).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let total = field("totalTimeInMinutes");
            let remaining = field("remainingTimeInMinutes");
            let span_start = parse_time(value.get("currentSpanStartDateTime"));
            println!("    本期起點 {span_start:?}");
            println!("    T={total} R={remaining} → 已使用 {}", total - remaining);
            (span_start, total.saturating_sub(remaining))
        }
        None => {
            println!("    訂閱都抓不到，後面的比對沒有基準，先停。");
            return;
        }
    };

    let start_param = span_start
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
    let end_param = Utc::now().to_rfc3339();

    println!();
    println!("=== 3. userplaytime/sessionshistory ===");
    let history_url = format!(
        "{PAYWALL_BASE}/gfn-paywall-api/api/v2/userplaytime/sessionshistory\
         ?spanStartDate={start_param}&spanEndDate={end_param}"
    );
    // 小寫 idtoken 不通就試本專案對 mes 用的那種，再不通就兩個一起送 ——
    // 有些閘道兩個都要。
    let mut sessions = None;
    for auth in [Auth::IdToken, Auth::Bearer, Auth::Both] {
        sessions = probe(&http, &history_url, auth, &id_token).await;
        if sessions.is_some() {
            break;
        }
    }
    if let Some(value) = &sessions {
        summarise(value, span_start, used_minutes);
    }

    println!();
    println!("=== 4. userplaytime/history ===");
    println!("那篇文章說只回溯 6 週且沒有遊戲名稱。本專案不需要名稱。");
    let short_url = format!(
        "{PAYWALL_BASE}/gfn-paywall-api/api/v2/userplaytime/history?memberSince={start_param}"
    );
    let short = match probe(&http, &short_url, Auth::IdToken, &id_token).await {
        Some(value) => Some(value),
        None => probe(&http, &short_url, Auth::Bearer, &id_token).await,
    };
    if let Some(value) = &short {
        summarise(value, span_start, used_minutes);
    }

    println!();
    println!("=== 5. uds session/reports ===");
    println!("spec §2 與 spike §5 的待辦。名字是 reports，客戶端多半是往那裡");
    println!("POST 回報，GET 不一定有東西，所以排最後。");
    let uds = probe(
        &http,
        &format!("{UDS_BASE}/v1/uds/session/reports"),
        Auth::Bearer,
        &id_token,
    )
    .await;
    if let Some(value) = &uds {
        summarise(value, span_start, used_minutes);
    }

    println!();
    println!("=== 判定 ===");
    let any = sessions.is_some() || short.is_some() || uds.is_some();
    if any {
        println!("A1 過：有端點回 200。");
        println!("A2 A3 看上面的加總與「起訖差」那一欄自己判。");
    } else {
        println!("A1 沒過：沒有端點吃這顆 id_token。");
        println!("把上面每個端點回什麼寫進 spike 文件的待辦，程式不改。");
    }
}
