pub mod avail;
pub mod schedule;

use chrono_tz::Tz;

/// 機器目前的 IANA 時區。
///
/// 取不到或名稱不認得時退回 UTC：時段設定會失準，但不至於讓整個計算崩掉。
/// `chrono::Local` 只給得到位移、給不到時區名稱，而日光節約時間的判斷需要名稱。
pub fn machine_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse::<Tz>().ok())
        .unwrap_or(Tz::UTC)
}
