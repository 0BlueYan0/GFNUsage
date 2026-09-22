import type { DisplayState } from "./types";

/**
 * 拆成小時與分鐘，給要分別排版的地方（面板的主要數字）。
 *
 * 不滿一小時時 `hours` 是 0，由呼叫端決定要不要把分鐘當主角。
 */
export function splitDuration(minutes: number): {
  hours: number;
  mins: number;
} {
  const total = Math.max(0, Math.round(minutes));
  return { hours: Math.floor(total / 60), mins: total % 60 };
}

/**
 * UTC 時刻以本地時區顯示，日期與時間都印。
 *
 * 連時間一起印是為了重置時點：API 給的是 UTC，而重置並不落在本地午夜，
 * `2026-10-15T23:59:59Z` 在台灣是 10/16 早上 07:59，只印日期會讓人誤以為
 * 是午夜。遊玩紀錄的起始時間用同一個格式。
 */
export function formatLocalDateTime(iso: string, timeZone?: string): string {
  const formatted = new Date(iso).toLocaleString("zh-TW", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone,
  });
  // 日期與時間之間那個空白，不同 ICU 版本給的不是同一個字元 —— CLDR 後來
  // 把 zh-Hant 改成窄不斷行空格（U+202F）。肉眼一模一樣，比對就不相等，
  // 而使用者的 Windows 給哪一個由他的系統決定。統一成半形空格。
  return formatted.replace(/\p{Zs}/gu, " ");
}

/** 距離某時點還有幾天，無條件捨去。給「要不要示警」這種門檻判斷用。 */
export function daysUntil(iso: string, now: Date = new Date()): number {
  const ms = new Date(iso).getTime() - now.getTime();
  return Math.max(0, Math.floor(ms / 86_400_000));
}

/**
 * 倒數到某個時點，精度隨剩餘長短自己縮小。
 *
 * 只講天數在剩不到一天時會變成「0 天」，而那正是最需要知道確切還有多久的
 * 時候 —— 重置當天到底是還有 8 小時還是 40 分鐘，差很多。所以：
 * 剩一天以上講到小時，剩不到一天講小時，剩不到一小時講分鐘。
 *
 * 底下的計算本來就是以分鐘為單位（`avail()`、配速、預測都是），
 * 這裡只是讓顯示對得起那個精度。
 */
export function formatCountdown(iso: string, now: Date = new Date()): string {
  const minutes = Math.max(
    0,
    Math.floor((new Date(iso).getTime() - now.getTime()) / 60_000),
  );
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);

  if (days > 0) return hours > 0 ? `${days} 天 ${hours} 小時` : `${days} 天`;
  if (hours > 0) return `${hours} 小時`;
  return `${minutes} 分鐘`;
}

/**
 * 佔總量的百分比，四捨五入到整數。
 *
 * 不叫 percentUsed：進度條現在可能畫的是剩餘，名字寫死成「已使用」會騙人。
 */
export function percentOf(part: number, total: number): number {
  if (total <= 0) return 0;
  return Math.round((part / total) * 100);
}

/**
 * 給人看的時間長度。不用小數點的小時 ——「2.5 小時」要讀的人自己在心裡
 * 乘六十，有餘數就直接把它講成分鐘。
 */
export function formatDuration(minutes: number): string {
  const { hours, mins } = splitDuration(minutes);
  if (hours === 0) return `${mins} 分鐘`;
  if (mins === 0) return `${hours} 小時`;
  return `${hours} 小時 ${mins} 分鐘`;
}

/**
 * 大字時數旁邊那一行的單位。
 *
 * 大字只放小時，餘數的分鐘跟在單位後面 ——「103 小時 5 分鐘」整串用 44px 排，
 * 360px 寬的面板放不下。數字與單位因此分成兩個 span，用不了 `formatDuration`。
 *
 * 放這裡而不是留在元件裡，是因為它上次就是這樣被漏掉的：改成「不用小數點的
 * 小時」時，`formatDuration` 改了，這一串還留著「分」。
 */
export function formatHeroUnit(minutes: number): string {
  const { hours, mins } = splitDuration(minutes);
  if (hours === 0) return "分鐘";
  return mins > 0 ? `小時 ${mins} 分鐘` : "小時";
}

/** 配速差距的說法。超前用「超前」，落後用「低於」。 */
export function formatOverPace(overPaceMinutes: number): string {
  const gap = formatDuration(Math.abs(overPaceMinutes));
  return overPaceMinutes > 0 ? `超前 ${gap}` : `低於門檻 ${gap}`;
}

/**
 * 狀態對應的 CSS 修飾詞，normal 與 freeTier 不加。
 *
 * 放在這裡是因為進度條與走勢圖都要用同一組。留在 App.tsx 裡的話
 * Trend.tsx 只能自己再寫一份，兩份遲早會有一份忘了跟著改。
 */
export function modifier(base: string, state: DisplayState): string {
  return state === "normal" || state === "freeTier"
    ? base
    : `${base} ${base}--${state}`;
}
