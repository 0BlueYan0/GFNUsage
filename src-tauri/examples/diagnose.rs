//! 憑證診斷與修復工具。
//!
//! 用途：釐清「需要重新登入」是哪一種狀況，並在可能的情況下修好。
//!
//!   cargo run --example diagnose --manifest-path src-tauri/Cargo.toml
//!
//! 它會回答三個問題：
//!   1. keychain 裡的 token 和 GFN 客戶端檔案裡的是不是同一顆？
//!   2. 哪一顆還活著？
//!   3. 刷新之後，舊的那顆會不會立刻失效？（這決定我們能不能和 GFN 客戶端共存）
//!
//! token 內容一律遮蔽，只印前後各 4 字元與長度。
//! 最後會把確認可用的那顆寫回 keychain。

use std::sync::Arc;

use gfnusage_lib::auth::refresh::{STARFLEET_BASE, STARFLEET_CLIENT_ID};
use gfnusage_lib::auth::session::{default_shared_storage_path, read_shared_storage, ImportedSession};
use gfnusage_lib::auth::store::{KeyringStore, TokenStore};

const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:client_token";

fn mask(token: &str) -> String {
    if token.len() <= 8 {
        return format!("<{} 字元>", token.len());
    }
    format!("{}…{} ({} 字元)", &token[..4], &token[token.len() - 4..], token.len())
}

struct Outcome {
    status: u16,
    body: String,
    new_client_token: Option<String>,
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
            }
        }
    };

    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let new_client_token = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("client_token")?.as_str().map(str::to_owned));

    Outcome {
        status,
        body: if status == 200 {
            String::new()
        } else {
            text.chars().take(200).collect()
        },
        new_client_token,
    }
}

#[tokio::main]
async fn main() {
    let http = reqwest::Client::builder()
        .user_agent("GFNUsage/0.1")
        .build()
        .expect("HTTP 用戶端");
    let keyring: Arc<dyn TokenStore> = Arc::new(KeyringStore);

    println!("=== 1. 兩邊的憑證 ===");

    let stored = match keyring.load() {
        Ok(Some(session)) => {
            println!("keychain      {}", mask(&session.client_token));
            Some(session)
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

    let from_file = default_shared_storage_path()
        .and_then(|path| match read_shared_storage(&path) {
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

    let mut alive: Option<(String, ImportedSession)> = None;
    let mut rotated_from: Option<ImportedSession> = None;

    for (name, session) in [("keychain", stored.as_ref()), ("GFN 客戶端", from_file.as_ref())] {
        let Some(session) = session else { continue };
        if alive.is_some() {
            println!("{name:<12}  （已經找到可用的，跳過）");
            continue;
        }

        let outcome = try_refresh(&http, session).await;
        match outcome.status {
            200 => {
                let new = outcome.new_client_token.clone().unwrap_or_default();
                let same = new == session.client_token;
                println!(
                    "{name:<12}  可用（HTTP 200）；回傳的 client_token {}",
                    if same { "與送出的相同 → 不輪替".to_string() } else { format!("是新的 {} → 會輪替", mask(&new)) }
                );
                if !same {
                    rotated_from = Some(session.clone());
                }
                alive = Some((
                    name.to_string(),
                    ImportedSession {
                        client_token: if new.is_empty() { session.client_token.clone() } else { new },
                        sub: session.sub.clone(),
                    },
                ));
            }
            code => println!("{name:<12}  失效（HTTP {code}）{}", outcome.body),
        }
    }

    let Some((source, current)) = alive else {
        println!();
        println!("兩顆都失效了。");
        println!("解法：開啟 GeForce NOW 客戶端並確認能正常登入，");
        println!("      讓它寫入新的憑證，然後在面板重新匯入。");
        return;
    };

    println!();
    println!("=== 3. 舊的那顆刷新後會不會立刻失效 ===");

    match rotated_from {
        None => println!("伺服器沒有輪替 client_token，本工具與 GFN 客戶端可以共存。"),
        Some(old) => {
            let outcome = try_refresh(&http, &old).await;
            match outcome.status {
                200 => println!(
                    "舊的那顆刷新後仍可用（HTTP 200）→ 輪替但不立即失效，\n\
                     代表本工具與 GFN 客戶端可以共存。"
                ),
                code => println!(
                    "舊的那顆已失效（HTTP {code}）→ 每次刷新都會讓另一方的複本作廢。\n\
                     本工具與 GFN 客戶端無法共用同一組憑證。"
                ),
            }
        }
    }

    println!();
    println!("=== 4. 寫回 keychain ===");
    match keyring.save(&current) {
        Ok(()) => println!("已把來自「{source}」的可用憑證 {} 寫回。", mask(&current.client_token)),
        Err(e) => println!("寫入失敗：{e}"),
    }
    println!("回到 GFNUsage 面板按「立即更新」即可。");
}
