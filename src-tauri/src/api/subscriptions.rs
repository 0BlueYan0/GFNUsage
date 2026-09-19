use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::GfnError;

pub const MES_BASE: &str = "https://mes.geforcenow.com";

/// `/v4/subscriptions` 回應中本程式會用到的欄位。
/// serde 預設忽略未知欄位 —— NVIDIA 隨時會加東西，加了不能壞。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub membership_tier: String,
    pub sub_type: String,
    pub allotted_time_in_minutes: u32,
    pub rolled_over_time_in_minutes: u32,
    pub purchased_time_in_minutes: u32,
    pub total_time_in_minutes: u32,
    pub remaining_time_in_minutes: u32,
    pub current_span_start_date_time: DateTime<Utc>,
    pub current_span_end_date_time: DateTime<Utc>,
    pub current_subscription_state: SubscriptionState,
    pub notifications: Notifications,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionState {
    pub state: String,
    pub is_game_play_allowed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
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
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(GfnError::NeedsLogin);
    }
    if !status.is_success() {
        return Err(GfnError::Network(format!("/subscriptions 回應 {status}")));
    }

    response
        .json()
        .await
        .map_err(|e| GfnError::UnexpectedResponse(e.to_string()))
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
        assert_eq!(sub.notifications.notify_user_when_time_remaining_in_minutes, 300);
        assert_eq!(sub.current_span_end_date_time.timestamp(), 1792108799);
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
}
