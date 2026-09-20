//! 憑證診斷與修復工具。
//!
//! 用途：釐清「需要重新登入」是哪一種狀況，並在可能的情況下修好。
//!
//!   cargo run --example diagnose --manifest-path src-tauri/Cargo.toml
//!   cargo run --example diagnose --manifest-path src-tauri/Cargo.toml -- --probe-rotation
//!
//! 它會回答兩個問題（加 `--probe-rotation` 則是三個）：
//!   1. keychain 裡的 token 和 GFN 客戶端檔案裡的是不是同一顆？
//!   2. 哪一顆還活著？
//!   3. （選用）刷新之後，舊的那顆會不會立刻失效？這決定能不能和 GFN 客戶端共存。
//!      預設不跑：它會再鑄一顆 token，而且伺服器若是延遲作廢，這一步鑄出的
//!      新 client_token 會讓第 2 步那顆失效 —— 診斷工具不該自己造成鎖死。
//!
//! 每次成功的 `/token` 都算進「同時有效的 access_token 數量上限」，
//! 所以本工具最多只呼叫兩次（加 `--probe-rotation` 三次），並且把最後一次
//! 鑄出的 client_token 與 id_token 都寫回 keychain，app 下次啟動不必再鑄。
//!
//! token 內容一律遮蔽，只印前後各 4 字元與長度。

use std::sync::Arc;

use gfnusage_lib::auth::refresh::{STARFLEET_BASE, STARFLEET_CLIENT_ID};
use gfnusage_lib::auth::session::{
    default_shared_storage_path, read_shared_storage, ImportedSession,
};
use gfnusage_lib::auth::store::{KeyringStore, StoredSession, TokenStore};
use gfnusage_lib::{http_client, HTTP_TIMEOUT};

const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:client_token";

/// 撞到「同時有效的 access_token 數量上限」。這不代表憑證壞了 ——
/// 憑證是好的，只是暫時換不到新的 token，等既有的過期就會恢復。
const TOO_MANY_TOKENS: &str = "Max allowed simultaneous valid access_token exceeded";

fn mask(token: &str) -> String {
    if token.len() <= 8 {
        return format!("<{} 字元>", token.len());
    }
    format!(
        "{}…{} ({} 字元)",
        &token[..4],
        &token[token.len() - 4..],
        token.len()
    )
}

struct Outcome {
    status: u16,
    body: String,
    new_client_token: Option<String>,
    id_token: Option<String>,
}

async fn try_refresh(http: &reqwest::Client, session: &ImportedSession) -> Outcome {
    let response = http
        .post(format!("{STARFLEET_BASE}/token"))
        .form(&[
            ("grant_type", GRANT_TYPE),
            ("client_token", session.client_token.as_str()),
            ("client_id", STARFLEET_CLIENT_ID),
            ("sub", session.sub.as_str()),
        ])
        .send()
        .await;

    let response = match response {
        Ok(response) => response,
        Err(e) => {
            return Outcome {
                status: 0,
                body: format!("網路錯誤：{e}"),
                new_client_token: None,
                id_token: None,
            }
        }
    };

    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let json = serde_json::from_str::<serde_json::Value>(&text).ok();
    let field = |name: &str| {
        json.as_ref()
            .and_then(|v| v.get(name)?.as_str().map(str::to_owned))
    };

    Outcome {
        status,
        body: if status == 200 {
            String::new()
        } else {
            text.chars().take(200).collect()
        },
        new_client_token: field("client_token"),
        id_token: field("id_token"),
    }
}

/// 一次成功刷新的結果：之後要寫回 keychain 的就是這組。
#[derive(Clone)]
struct Minted {
    session: ImportedSession,
    id_token: Option<String>,
}

#[tokio::main]
async fn main() {
    let probe_rotation = std::env::args().any(|a| a == "--probe-rotation");
    let http = http_client(HTTP_TIMEOUT);
    let keyring: Arc<dyn TokenStore> = Arc::new(KeyringStore);

    println!("=== 1. 兩邊的憑證 ===");

    let stored = match keyring.load() {
        Ok(Some(session)) => {
            println!("keychain      {}", mask(&session.client_token));
            // 統一成匯入用的型別，方便和 GFN 檔案那顆並排比較。
            Some(ImportedSession {
                client_token: session.client_token,
                sub: session.sub,
                client_token_expires_at: session.client_token_expires_at,
            })
        }
        Ok(None) => {
            println!("keychain      （空的，尚未匯入）");
            None
        }
        Err(e) => {
            println!("keychain      讀取失敗：{e}");
            None
        }
    };

    let from_file =
        default_shared_storage_path().and_then(|path| match read_shared_storage(&path) {
            Ok(session) => {
                println!("GFN 客戶端    {}", mask(&session.client_token));
                Some(session)
            }
            Err(e) => {
                println!("GFN 客戶端    讀取失敗：{e}");
                None
            }
        });

    if let (Some(a), Some(b)) = (&stored, &from_file) {
        if a.client_token == b.client_token {
            println!("兩邊相同 —— 我們還沒刷新過，或剛從檔案匯入。");
        } else {
            println!("兩邊不同 —— 曾經刷新過，其中一顆已經是舊的。");
        }
    }

    println!();
    println!("=== 2. 哪一顆還活著 ===");

    let mut alive: Option<(String, Minted)> = None;
    let mut rotated_from: Option<ImportedSession> = None;
    let mut capped: Option<ImportedSession> = None;

    for (name, session) in [
        ("keychain", stored.as_ref()),
        ("GFN 客戶端", from_file.as_ref()),
    ] {
        let Some(session) = session else { continue };
        if alive.is_some() {
            println!("{name:<12}  （已經找到可用的，跳過）");
            continue;
        }

        let outcome = try_refresh(&http, session).await;
        match outcome.status {
            200 => {
                let new = outcome.new_client_token.clone().unwrap_or_default();
                let same = new.is_empty() || new == session.client_token;
                println!(
                    "{name:<12}  可用（HTTP 200）；回傳的 client_token {}",
                    if same {
                        "與送出的相同 → 不輪替".to_string()
                    } else {
                        format!("是新的 {} → 會輪替", mask(&new))
                    }
                );
                if !same {
                    rotated_from = Some(session.clone());
                }
                alive = Some((
                    name.to_string(),
                    Minted {
                        session: ImportedSession {
                            client_token: if same {
                                session.client_token.clone()
                            } else {
                                new
                            },
                            sub: session.sub.clone(),
                            // 輪替不重設效期，沿用原本那個。
                            client_token_expires_at: session.client_token_expires_at,
                        },
                        id_token: outcome.id_token,
                    },
                ));
            }
            code if outcome.body.contains(TOO_MANY_TOKENS) => {
                println!(
                    "{name:<12}  憑證有效，但暫時換不到 token（HTTP {code}）
                     {:<14}NVIDIA 限制同時有效的 access_token 數量，目前已達上限。",
                    ""
                );
                capped = Some(session.clone());
            }
            code => println!("{name:<12}  失效（HTTP {code}）{}", outcome.body),
        }
    }

    let Some((source, mut current)) = alive else {
        println!();
        if let Some(session) = capped {
            println!("憑證是好的，只是撞到 token 數量上限。");
            println!("已把它寫回 keychain；等既有的 token 過期（最多 1 小時）後");
            println!("回到面板按「立即更新」就會恢復，不需要重新登入。");
            if let Err(e) = keyring.save(&StoredSession::from(session)) {
                println!("（寫入失敗：{e}）");
            }
        } else {
            println!("兩顆都失效了。");
            println!("解法：開啟 GeForce NOW 客戶端並確認能正常登入，");
            println!("      讓它寫入新的憑證，然後在面板重新匯入。");
        }
        return;
    };

    if probe_rotation {
        println!();
        println!("=== 3. 舊的那顆刷新後會不會立刻失效 ===");

        match rotated_from {
            None => println!("伺服器沒有輪替 client_token，本工具與 GFN 客戶端可以共存。"),
            Some(old) => {
                let outcome = try_refresh(&http, &old).await;
                match outcome.status {
                    200 => {
                        println!(
                            "舊的那顆刷新後仍可用（HTTP 200）→ 輪替但不立即失效，\n\
                             代表本工具與 GFN 客戶端可以共存。"
                        );
                        // 這一步又鑄了一顆；能確定還活著的是最後鑄出的這顆，寫回的要是它。
                        if let Some(new) = outcome.new_client_token.filter(|t| !t.is_empty()) {
                            current = Minted {
                                session: ImportedSession {
                                    client_token: new,
                                    sub: old.sub.clone(),
                                    // 輪替不重設效期，沿用原本那個。
                                    client_token_expires_at: old.client_token_expires_at,
                                },
                                id_token: outcome.id_token,
                            };
                        }
                    }
                    code => println!(
                        "舊的那顆已失效（HTTP {code}）→ 每次刷新都會讓另一方的複本作廢。\n\
                         本工具與 GFN 客戶端無法共用同一組憑證。"
                    ),
                }
            }
        }
    }

    println!();
    println!(
        "=== {}. 寫回 keychain ===",
        if probe_rotation { 4 } else { 3 }
    );
    match keyring.save(&StoredSession::from(current.session.clone())) {
        Ok(()) => println!(
            "已把來自「{source}」的可用憑證 {} 寫回。",
            mask(&current.session.client_token)
        ),
        Err(e) => println!("寫入失敗：{e}"),
    }
    // 連 id_token 一起寫回，app 下次啟動就不必再鑄一顆。
    match current.id_token.as_deref() {
        Some(id_token) => match keyring.save_id_token(id_token) {
            Ok(()) => println!(
                "id_token（{} 字元）也已寫回，app 啟動時會直接沿用。",
                id_token.len()
            ),
            Err(e) => println!("id_token 寫入失敗（app 啟動時會重新取得）：{e}"),
        },
        None => {
            let _ = keyring.clear_id_token();
        }
    }
    println!("回到 GFNUsage 面板按「立即更新」即可。");
}
