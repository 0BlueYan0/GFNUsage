use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{describe_shape, GfnError};

pub const PAYWALL_BASE: &str = "https://api-prod.nvidia.com";

/// 一場遊玩紀錄。
///
/// `minutes` 是 NVIDIA 從額度扣掉的分鐘數，比起訖差略小（28.3 分鐘的時段記
/// 27），扣的是實際串流時間。本期所有場次加總等於 `/v4/subscriptions` 的
/// `T − R`，所以它可以拿來算今天用了多少（spike 3a）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySession {
    pub game_title: String,
    pub started_at: DateTime<Utc>,
    /// 還在玩的那一場沒有結束時間。
    pub ended_at: Option<DateTime<Utc>>,
    pub minutes: f64,
}

/// 回應有兩個陣列。`aggregatedSessionsByGame` 是同一款連續玩的合併結果，
/// 欄位名也不同（`startDate`／`endDate`），這裡不解析它。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryResponse {
    #[serde(default)]
    session_history: Vec<RawSession>,
}

/// 每個欄位都允許缺席。NVIDIA 隨時會改，改了不能讓整次解析失敗 ——
/// 缺了起始時間的那一筆丟掉就好，其餘照用。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSession {
    #[serde(default)]
    game_title: String,
    #[serde(default)]
    session_start_date: Option<DateTime<Utc>>,
    #[serde(default)]
    session_end_date: Option<DateTime<Utc>>,
    #[serde(default)]
    total_playtime: f64,
}

/// 讀取逐場遊玩紀錄。
///
/// 標頭是小寫的 `idtoken`，不是 `Authorization: Bearer` —— 送後者會得到
/// 400 `idToken is missing`。只有 NVIDIA 帳號頁那顆 client_id
/// （`HdpDyyR1…`）簽出來的 token 收，GFN 客戶端那顆一律 401（spike 3a）。
///
/// `from` 會再濾一次：伺服器端是
/// `vars.req_spanStartDate as DateTime as String {format: "yyyy-MM-dd"}`，
/// 時間被丟掉只留日期，所以它回的是整天。本期起點在當天中午的話，
/// 早上那幾場也會一起回來。
pub async fn fetch_session_history(
    http: &reqwest::Client,
    base: &str,
    id_token: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<PlaySession>, GfnError> {
    let response = http
        .get(format!(
            "{base}/gfn-paywall-api/api/v2/userplaytime/sessionshistory"
        ))
        // `+00:00` 不能進查詢字串，`+` 在那裡解碼成空白，伺服器會收到
        // 「2026-09-15T13:18:59 00:00」並回一個包在 200 裡的錯誤。要用 Z。
        .query(&[
            (
                "spanStartDate",
                from.to_rfc3339_opts(SecondsFormat::Secs, true),
            ),
            ("spanEndDate", to.to_rfc3339_opts(SecondsFormat::Secs, true)),
        ])
        .header("idtoken", id_token)
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
        return Err(GfnError::Network(format!(
            "/userplaytime/sessionshistory 回應 {status}"
        )));
    }

    let body = response
        .text()
        .await
        .map_err(|e| GfnError::Network(e.to_string()))?;

    // 參數錯了它回 HTTP 200 包一個 `errors`，不是 4xx。當成成功會拿到空清單，
    // 然後把「今天沒玩」寫進今日額度。
    if body.contains("\"errors\"") {
        return Err(GfnError::UnexpectedResponse(format!(
            "/userplaytime/sessionshistory 回了 200 但帶著錯誤；{}",
            describe_shape(&body)
        )));
    }

    let parsed: HistoryResponse = serde_json::from_str(&body).map_err(|e| {
        GfnError::UnexpectedResponse(format!(
            "/userplaytime/sessionshistory 的回應解析失敗：{e}；{}",
            describe_shape(&body)
        ))
    })?;

    let mut sessions: Vec<PlaySession> = parsed
        .session_history
        .into_iter()
        .filter_map(|raw| {
            let started_at = raw.session_start_date?;
            (started_at >= from).then_some(PlaySession {
                game_title: raw.game_title,
                started_at,
                ended_at: raw.session_end_date,
                minutes: raw.total_playtime,
            })
        })
        .collect();

    // 回應的順序沒有保證，面板要新的在上。
    sessions.sort_unstable_by_key(|session| std::cmp::Reverse(session.started_at));
    Ok(sessions)
}

/// `[from, to)` 這段時間裡玩掉的分鐘數。
///
/// 跨界線的那一場按重疊比例裁切，不能整筆算給開始的那一天 —— 台灣時間
/// 晚上 11 點玩到凌晨 1 點，只有後一小時算今天的。
///
/// 比例用牆上時鐘算，再乘回 `minutes`。兩者不相等（`minutes` 扣的是實際串流
/// 時間），但差距是個位數百分比，而按比例分攤比整筆歸給某一天準。
/// 還沒結束的那一場用 `to` 當結束。
pub fn minutes_between(sessions: &[PlaySession], from: DateTime<Utc>, to: DateTime<Utc>) -> u32 {
    let total: f64 = sessions
        .iter()
        .map(|session| {
            let ended_at = session.ended_at.unwrap_or(to);
            let span = (ended_at - session.started_at).num_seconds();
            let overlap = (ended_at.min(to) - session.started_at.max(from)).num_seconds();
            if overlap <= 0 {
                return 0.0;
            }
            if span <= 0 {
                return session.minutes;
            }
            session.minutes * (overlap.min(span) as f64 / span as f64)
        })
        .sum();

    total.round().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/session_history.json");

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// 本期起點。fixture 裡 WARDOGS 那場在這之前。
    fn span_start() -> DateTime<Utc> {
        at("2026-09-15T13:18:59Z")
    }

    async fn fetch_fixture() -> Vec<PlaySession> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gfn-paywall-api/api/v2/userplaytime/sessionshistory"))
            .and(header("idtoken", "ID-TOKEN"))
            // 送出去的是帶 Z 的 RFC3339。`+00:00` 會被伺服器當成空白。
            .and(query_param("spanStartDate", "2026-09-15T13:18:59Z"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(FIXTURE, "application/json"))
            .expect(1)
            .mount(&server)
            .await;

        fetch_session_history(
            &reqwest::Client::new(),
            &server.uri(),
            "ID-TOKEN",
            span_start(),
            at("2026-09-21T08:00:00Z"),
        )
        .await
        .unwrap()
    }

    /// 伺服器只認日期，所以本期起點當天更早的場次也會回來。要自己濾掉，
    /// 不然加總會比 `T − R` 多。
    #[tokio::test]
    async fn drops_sessions_that_start_before_the_span() {
        let sessions = fetch_fixture().await;
        assert!(
            !sessions.iter().any(|s| s.game_title == "WARDOGS"),
            "本期起點之前的那場沒被濾掉"
        );
    }

    /// 缺起始時間的那一筆丟掉，其餘照用。整份解析失敗的話，一個欄位改名
    /// 就會讓今日額度整個消失。
    #[tokio::test]
    async fn keeps_the_rest_when_one_row_is_incomplete() {
        let sessions = fetch_fixture().await;
        assert_eq!(sessions.len(), 4);
        assert!(!sessions.iter().any(|s| s.game_title.contains("丟掉")));
    }

    #[tokio::test]
    async fn sorts_newest_first() {
        let sessions = fetch_fixture().await;
        let times: Vec<_> = sessions.iter().map(|s| s.started_at).collect();
        let mut sorted = times.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(times, sorted);
    }

    #[tokio::test]
    async fn reads_the_end_time_and_leaves_it_out_when_still_playing() {
        let sessions = fetch_fixture().await;
        let playing = sessions
            .iter()
            .find(|s| s.game_title.contains("沒有結束時間"))
            .unwrap();
        assert_eq!(playing.ended_at, None);

        let finished = sessions
            .iter()
            .find(|s| s.game_title == "Wuthering Waves")
            .unwrap();
        assert_eq!(finished.ended_at, Some(at("2026-09-21T06:21:03Z")));
    }

    /// 參數錯了它回 200 包一個 errors。當成成功會拿到空清單，
    /// 然後把「今天沒玩」寫進今日額度。
    #[tokio::test]
    async fn a_200_carrying_errors_is_not_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "failure",
                "correlationId": "x",
                "errors": { "errorCode": "ERR-MUL-001", "errorMessage": "Cannot coerce String" }
            })))
            .mount(&server)
            .await;

        let result = fetch_session_history(
            &reqwest::Client::new(),
            &server.uri(),
            "ID-TOKEN",
            span_start(),
            at("2026-09-21T08:00:00Z"),
        )
        .await;

        assert!(matches!(result, Err(GfnError::UnexpectedResponse(_))));
    }

    #[tokio::test]
    async fn maps_401_and_429() {
        for (code, want) in [(401, GfnError::NeedsLogin), (429, GfnError::RateLimited)] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(code))
                .mount(&server)
                .await;

            let result = fetch_session_history(
                &reqwest::Client::new(),
                &server.uri(),
                "ID-TOKEN",
                span_start(),
                at("2026-09-21T08:00:00Z"),
            )
            .await;

            assert_eq!(result.unwrap_err().to_string(), want.to_string(), "{code}");
        }
    }

    fn session(start: &str, end: Option<&str>, minutes: f64) -> PlaySession {
        PlaySession {
            game_title: "測試".into(),
            started_at: at(start),
            ended_at: end.map(at),
            minutes,
        }
    }

    #[test]
    fn sums_whole_sessions_inside_the_window() {
        let sessions = [
            session("2026-09-21T01:00:00Z", Some("2026-09-21T02:00:00Z"), 57.0),
            session("2026-09-21T03:00:00Z", Some("2026-09-21T04:00:00Z"), 58.0),
        ];
        let total = minutes_between(
            &sessions,
            at("2026-09-21T00:00:00Z"),
            at("2026-09-21T08:00:00Z"),
        );
        assert_eq!(total, 115);
    }

    /// 這是整個裁切存在的理由。台灣時間晚上 11 點玩到凌晨 1 點，
    /// 只有後一小時算今天的。
    #[test]
    fn splits_a_session_that_crosses_the_boundary() {
        // UTC+8 的 9/21 00:00 是 UTC 的 9/20 16:00。
        let midnight = at("2026-09-20T16:00:00Z");
        let sessions = [session(
            "2026-09-20T15:00:00Z",
            Some("2026-09-20T17:00:00Z"),
            114.0,
        )];

        let today = minutes_between(&sessions, midnight, at("2026-09-21T08:00:00Z"));
        let yesterday = minutes_between(&sessions, at("2026-09-19T16:00:00Z"), midnight);

        assert_eq!(today, 57);
        assert_eq!(yesterday, 57);
    }

    #[test]
    fn a_session_outside_the_window_counts_for_nothing() {
        let sessions = [session(
            "2026-09-19T01:00:00Z",
            Some("2026-09-19T02:00:00Z"),
            57.0,
        )];
        assert_eq!(
            minutes_between(
                &sessions,
                at("2026-09-21T00:00:00Z"),
                at("2026-09-21T08:00:00Z")
            ),
            0
        );
    }

    /// 還在玩的那一場沒有結束時間，用視窗的結尾當結束。
    #[test]
    fn an_unfinished_session_counts_up_to_now() {
        let sessions = [session("2026-09-21T07:00:00Z", None, 10.0)];
        assert_eq!(
            minutes_between(
                &sessions,
                at("2026-09-21T00:00:00Z"),
                at("2026-09-21T08:00:00Z")
            ),
            10
        );
    }

    #[test]
    fn parses_the_fixture_shape() {
        let parsed: HistoryResponse = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(parsed.session_history.len(), 6);
    }

    /// `aggregatedSessionsByGame` 的欄位名不一樣，不能被誤當成逐場資料。
    #[test]
    fn ignores_the_aggregated_array() {
        let parsed: HistoryResponse = serde_json::from_str(
            r#"{"aggregatedSessionsByGame":[{"gameTitle":"X","startDate":"2026-09-21T00:00:00Z","totalPlaytime":9}]}"#,
        )
        .unwrap();
        assert!(parsed.session_history.is_empty());
    }

    #[test]
    fn utc_timestamps_are_what_they_say() {
        assert_eq!(
            at("2026-09-21T04:24:42Z"),
            Utc.with_ymd_and_hms(2026, 9, 21, 4, 24, 42).unwrap()
        );
    }
}
