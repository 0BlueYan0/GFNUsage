export type DisplayState = "normal" | "low" | "exhausted" | "freeTier";

export interface QuotaSnapshot {
  fetchedAt: string;
  tier: string;
  timeCapped: boolean;
  totalMinutes: number;
  remainingMinutes: number;
  usedMinutes: number;
  rolledOverMinutes: number;
  purchasedMinutes: number;
  /** 沒有時數上限的方案沒有「本期」可言，會是 null。 */
  spanStart: string | null;
  spanEnd: string | null;
  gamePlayAllowed: boolean;
  lowThresholdMinutes: number;
  state: DisplayState;
}

export interface PanelData {
  snapshot: QuotaSnapshot | null;
  lastError: string | null;
  hasCredentials: boolean;
  /** 憑證被拒絕：後端已暫停輪詢，要重新匯入。 */
  needsLogin: boolean;
}

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
