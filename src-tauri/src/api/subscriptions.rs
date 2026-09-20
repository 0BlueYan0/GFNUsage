use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{describe_shape, GfnError};

pub const MES_BASE: &str = "https://mes.geforcenow.com";

/// `/v4/subscriptions` 回應中本程式會用到的欄位。
///
/// serde 預設忽略未知欄位 —— NVIDIA 隨時會加東西，加了不能壞。
/// 反過來，配額相關欄位一律允許缺席：免費方案的回應從沒觀察過，
/// 缺了幾個欄位不能讓整次解析失敗。`membershipTier` 是唯一必要欄位。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub membership_tier: String,
    #[serde(default)]
    pub sub_type: String,
    #[serde(default)]
    pub allotted_time_in_minutes: u32,
    #[serde(default)]
    pub rolled_over_time_in_minutes: u32,
    #[serde(default)]
    pub purchased_time_in_minutes: u32,
    #[serde(default)]
    pub total_time_in_minutes: u32,
    #[serde(default)]
    pub remaining_time_in_minutes: u32,
    #[serde(default)]
    pub current_span_start_date_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub current_span_end_date_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub current_subscription_state: SubscriptionState,
    #[serde(default)]
    pub notifications: Notifications,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SubscriptionState {
    pub state: String,
    pub is_game_play_allowed: bool,
}

impl Default for SubscriptionState {
    /// 欄位缺席不代表不能玩。
    fn default() -> Self {
        Self {
            state: String::new(),
            is_game_play_allowed: true,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Notifications {
    pub notify_user_when_time_remaining_in_minutes: u32,
    pub notify_user_on_session_when_remaining_time_in_minutes: u32,
}

/// 讀取訂閱資料。
///
/// 必須使用 id_token（JWT），不是 access_token —— 後者一律得到
/// 401 `{"error":"unauthorized","message":"invalid token"}`。
/// 實測不需要任何 NV-* 標頭。
pub async fn fetch_subscription(
    http: &reqwest::Client,
    base: &str,
    id_token: &str,
) -> Result<Subscription, GfnError> {
    let response = http
        .get(format!("{base}/v4/subscriptions"))
        .bearer_auth(id_token)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| GfnError::Network(e.to_string()))?;

    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(GfnError::RateLimited);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(GfnError::NeedsLogin);
    }
    if !status.is_success() {
        return Err(GfnError::Network(format!("/subscriptions 回應 {status}")));
    }

    // 同 `post_token`：`.json()` 失敗時只說「error decoding response body」，
    // 分不出是這裡壞的還是 `/token` 壞的，也不說少了哪個欄位。
    let body = response
        .text()
        .await
        .map_err(|e| GfnError::Network(e.to_string()))?;

    serde_json::from_str(&body).map_err(|e| {
        GfnError::UnexpectedResponse(format!(
            "/subscriptions 的回應解析失敗：{e}；{}",
            describe_shape(&body)
        ))
    })
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/subscription.json");

    #[test]
    fn parses_fixture_and_ignores_unknown_fields() {
        let sub: Subscription = serde_json::from_str(FIXTURE).unwrap();

        assert_eq!(sub.membership_tier, "ULTIMATE");
        assert_eq!(sub.sub_type, "TIME_CAPPED");
        assert_eq!(sub.total_time_in_minutes, 6900);
        assert_eq!(sub.remaining_time_in_minutes, 6180);
        assert_eq!(sub.rolled_over_time_in_minutes, 900);
        assert!(sub.current_subscription_state.is_game_play_allowed);
        assert_eq!(
            sub.notifications.notify_user_when_time_remaining_in_minutes,
            300
        );
        assert_eq!(
            sub.current_span_end_date_time.unwrap().timestamp(),
            1792108799
        );
    }

    #[tokio::test]
    async fn sends_id_token_as_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v4/subscriptions"))
            .and(header("authorization", "Bearer ID-TOKEN"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(FIXTURE, "application/json"))
            .expect(1)
            .mount(&server)
            .await;

        let sub = fetch_subscription(&reqwest::Client::new(), &server.uri(), "ID-TOKEN")
            .await
            .unwrap();

        assert_eq!(sub.remaining_time_in_minutes, 6180);
    }

    #[tokio::test]
    async fn maps_401_to_needs_login() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v4/subscriptions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "unauthorized",
                "message": "invalid token"
            })))
            .mount(&server)
            .await;

        let result = fetch_subscription(&reqwest::Client::new(), &server.uri(), "BAD").await;
        assert!(matches!(result, Err(GfnError::NeedsLogin)));
    }

    /// 免費方案的回應從沒觀察過。配額相關欄位缺席時要能解析，
    /// 不然免費方案的使用者看到的會是「回應格式非預期」而不是「免費方案」。
    #[test]
    fn parses_a_payload_without_quota_fields() {
        let sub: Subscription =
            serde_json::from_str(r#"{"membershipTier":"FREE","subType":"FREE"}"#).unwrap();

        assert_eq!(sub.sub_type, "FREE");
        assert_eq!(sub.total_time_in_minutes, 0);
        assert_eq!(sub.remaining_time_in_minutes, 0);
        assert!(sub.current_span_end_date_time.is_none());
        assert!(
            sub.current_subscription_state.is_game_play_allowed,
            "欄位缺席不代表不能玩"
        );
    }

    #[test]
    fn still_rejects_a_payload_without_the_tier() {
        assert!(serde_json::from_str::<Subscription>("{}").is_err());
    }

    #[tokio::test]
    async fn maps_429_to_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v4/subscriptions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let result = fetch_subscription(&reqwest::Client::new(), &server.uri(), "TOKEN").await;
        assert!(matches!(result, Err(GfnError::RateLimited)));
    }

    /// 兩個端點的解析失敗以前長得一模一樣，分不出是哪一邊壞的。
    /// 訊息必須說出是 `/subscriptions`，以及對方給了什麼欄位。
    #[tokio::test]
    async fn a_malformed_subscription_names_the_endpoint_and_the_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v4/subscriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "subType": "TIME_CAPPED",
            })))
            .mount(&server)
            .await;

        let problem = fetch_subscription(&reqwest::Client::new(), &server.uri(), "JWT")
            .await
            .unwrap_err()
            .to_string();

        assert!(problem.contains("/subscriptions"), "{problem}");
        assert!(problem.contains("membershipTier"), "{problem}");
        assert!(problem.contains("subType"), "{problem}");
    }
}
