export type DisplayState =
  | "normal"
  | "low"
  | "overPace"
  | "exhausted"
  | "freeTier";

export interface QuotaSnapshot {
  fetchedAt: string;
  tier: string;
  timeCapped: boolean;
  totalMinutes: number;
  remainingMinutes: number;
  usedMinutes: number;
  rolledOverMinutes: number;
  /** 結轉上限，官方政策的 15 小時。不在 API 回應裡，由後端帶下來。 */
  rolloverCapMinutes: number;
  purchasedMinutes: number;
  /** 沒有時數上限的方案沒有「本期」可言，會是 null。 */
  spanStart: string | null;
  spanEnd: string | null;
  gamePlayAllowed: boolean;
  lowThresholdMinutes: number;
  /** 還沒併入配速的基礎狀態。畫面請用 `PanelData.state`。 */
  state: DisplayState;
}

/** 沒有預測時的原因（spec §6.5）。 */
export type PaceNote = "noTimeLeft" | "insufficient" | "collecting";

export interface PaceReport {
  availPastMinutes: number;
  availLeftMinutes: number;
  expectedUsedMinutes: number | null;
  /** 大於 0 代表超前消耗。 */
  overPaceMinutes: number | null;
  burnRate: number | null;
  projectedUsedMinutes: number | null;
  overshootMinutes: number | null;
  runsOutAt: string | null;
  wastedMinutes: number | null;
  todayBudgetMinutes: number | null;
  note: PaceNote | null;
}

export interface WeeklyWindow {
  /** 0 = 週一 … 6 = 週日。 */
  weekdays: number[];
  startMinute: number;
  endMinute: number;
  note: string;
}

export type ExceptionKind = "blocked" | "free";

export interface ScheduleException {
  /** 本地日期 YYYY-MM-DD，含頭含尾。 */
  startDate: string;
  endDate: string;
  kind: ExceptionKind;
  note: string;
}

export interface Schedule {
  weekly: WeeklyWindow[];
  exceptions: ScheduleException[];
}

/** 面板主要數字看哪一邊。進度條跟著它走。 */
export type Metric = "remaining" | "used";

/** 一場遊玩紀錄。時間是 ISO 8601 的 UTC 字串。 */
export interface PlaySession {
  gameTitle: string;
  startedAt: string;
  /** 還在玩的那一場沒有結束時間。 */
  endedAt: string | null;
  minutes: number;
}

export interface PanelData {
  snapshot: QuotaSnapshot | null;
  /** 最近幾場，新的在前。抓不到逐場紀錄時是空的。 */
  recentSessions: PlaySession[];
  pace: PaceReport | null;
  /** 併入配速後的狀態。畫面一律用這個。 */
  state: DisplayState;
  lastError: string | null;
  hasCredentials: boolean;
  /** 憑證被拒絕：後端已暫停輪詢，要重新匯入。 */
  needsLogin: boolean;
  /** `client_token` 的到期時刻。里程碑 1／2 存下的舊憑證沒有，會是 null。 */
  clientTokenExpiresAt: string | null;
  /** 該不該顯示「把系統匣圖示拖出溢位區」的提示。只有 Windows 會是 true。 */
  showTrayHint: boolean;
  /**
   * 有一次 OAuth 登入正在進行。
   *
   * 由後端說了算，不由前端自己記：開瀏覽器一定會讓面板失焦收起來，
   * 本地旗標撐不過那一下。
   */
  loginPending: boolean;
  metric: Metric;
  /** 查到、使用者還沒關掉提示的新版本。沒有就是已經最新。 */
  updateVersion: string | null;
}
