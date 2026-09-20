/** 分鐘轉為「103.0」這樣的小時數字串，不含單位。 */
export function formatHours(minutes: number): string {
  return (minutes / 60).toFixed(1);
}

/**
 * 重置時點以本地時區顯示。
 *
 * API 給的是 UTC，而且重置並不落在本地午夜：`2026-10-15T23:59:59Z`
 * 在台灣是 10/16 早上 07:59。只印日期會讓人誤以為是午夜，所以連時間一起印。
 */
export function formatResetAt(iso: string, timeZone?: string): string {
  return new Date(iso).toLocaleString("zh-TW", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone,
  });
}

/** 距離重置還有幾天，無條件捨去。 */
export function daysUntil(iso: string, now: Date = new Date()): number {
  const ms = new Date(iso).getTime() - now.getTime();
  return Math.max(0, Math.floor(ms / 86_400_000));
}

/** 已使用的百分比，四捨五入到整數。 */
export function percentUsed(used: number, total: number): number {
  if (total <= 0) return 0;
  return Math.round((used / total) * 100);
}

/**
 * 給人看的時間長度：不足一小時只講分鐘，否則講到小數一位的小時。
 *
 * 「今天還能玩 0.4 小時」不如「今天還能玩 24 分鐘」好懂。
 */
export function formatDuration(minutes: number): string {
  const rounded = Math.max(0, Math.round(minutes));
  if (rounded < 60) return `${rounded} 分鐘`;
  return `${(rounded / 60).toFixed(1)} 小時`;
}

/** 配速差距的說法。超前用「超前」，落後用「低於」。 */
export function formatOverPace(overPaceMinutes: number): string {
  const gap = formatDuration(Math.abs(overPaceMinutes));
  return overPaceMinutes > 0 ? `超前 ${gap}` : `低於門檻 ${gap}`;
}
