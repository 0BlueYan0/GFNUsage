/**
 * 版面檢查用的入口，不進 app。
 *
 * jsdom 沒有版面引擎，vitest 測得到「畫了五列」但測不到「五列看不看得見」。
 * `.panel` 是 `height: 100vh` 加 `overflow: hidden`，塞不下的東西直接消失，
 * 被切掉的是最底下那排按鈕。這一頁餵一份假的 `PanelData` 給真正的 `App`，
 * 用瀏覽器開到 360×480 就看得到實際結果。
 *
 *   npm run dev   →  http://localhost:1420/layout.html
 */
import { createRoot } from "react-dom/client";
import type { PanelData, Schedule } from "./types";

function session(startedAt: string, gameTitle: string, minutes: number) {
  return { gameTitle, startedAt, endedAt: null, minutes };
}

/// 最擠的情況：40 場紀錄、配速三列都在、加購與上期未用完都有值，
/// 再加上一條「有新版本」的橫幅。
const DATA: PanelData = {
  snapshot: {
    tier: "ULTIMATE",
    timeCapped: true,
    totalMinutes: 6900,
    remainingMinutes: 5909,
    usedMinutes: 991,
    rolledOverMinutes: 900,
    rolloverCapMinutes: 900,
    purchasedMinutes: 120,
    spanStart: "2026-09-15T13:18:59Z",
    spanEnd: "2026-10-15T23:59:59Z",
    fetchedAt: "2026-09-21T08:00:00Z",
    gamePlayAllowed: true,
    lowThresholdMinutes: 300,
    state: "normal",
  },
  pace: {
    availPastMinutes: 5000,
    availLeftMinutes: 20000,
    expectedUsedMinutes: 1380,
    overPaceMinutes: -389,
    burnRate: 0.198,
    projectedUsedMinutes: 4951,
    overshootMinutes: -1949,
    runsOutAt: "2026-10-14T12:00:00Z",
    wastedMinutes: 1049,
    todayBudgetMinutes: 223,
    note: null,
  },
  state: "normal",
  lastError: null,
  hasCredentials: true,
  needsLogin: false,
  clientTokenExpiresAt: null,
  showTrayHint: false,
  loginPending: false,
  metric: "remaining",
  // 驗「玩的遊戲多了」那一頁會不會被切掉，以及標題有沒有釘住。實測一期 38 場。
  recentSessions: Array.from({ length: 40 }, (_, i) =>
    session(
      `2026-09-${String(21 - Math.floor(i / 4)).padStart(2, "0")}T${String(i % 24).padStart(2, "0")}:24:42Z`,
      [
        "Wuthering Waves",
        "Clair Obscur: Expedition 33",
        "Where Winds Meet",
        "Forza Horizon 4",
        "WARDOGS",
      ][i % 5],
      16 + i * 7,
    ),
  ),
  updateVersion: "0.1.1",
};

/// 設定頁最擠的情況：時段多到要捲，才看得出標題有沒有釘住。
const SCHEDULE: Schedule = {
  weekly: Array.from({ length: 6 }, (_, i) => ({
    weekdays: [i % 7, (i + 3) % 7],
    startMinute: 60 * i,
    endMinute: 60 * i + 420,
    note: `時段 ${i + 1}`,
  })),
  exceptions: [
    { startDate: "2026-10-01", endDate: "2026-10-05", kind: "blocked", note: "出差" },
    { startDate: "2026-10-10", endDate: "2026-10-12", kind: "free", note: "連假" },
  ],
};

// `invoke` 走 `window.__TAURI_INTERNALS__`，在瀏覽器裡沒有這個物件。
// 先補上再載入 `App` —— import 會被提升，所以用動態 import。
/// `?screen=` 不帶參數看主面板，`signin` 登入畫面，`about` 關於頁，
/// `settings` 設定頁，`sessions` 最近。容器不一樣（`.panel--scroll` 對
/// `.panel`），改版面每一個都要看。
const SCREEN = new URLSearchParams(location.search).get("screen") ?? "";

(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: async (command: string) => {
    if (command === "get_snapshot") {
      return SCREEN === "signin"
        ? { ...DATA, snapshot: null, pace: null, hasCredentials: false, recentSessions: [] }
        : DATA;
    }
    // 關於頁要的三樣。給有值的版本，不然那一頁空著就看不出版面。
    if (command === "app_version") return "0.1.0";
    if (command === "get_update_status") {
      return { version: "0.1.1", installing: false };
    }
    if (command === "get_autostart") return false;
    if (command === "get_schedule") return SCHEDULE;
    return null;
  },
  transformCallback: (callback: unknown) => callback,
};

/// 從 `.actions` 把那一頁按開。
///
/// `App` 的畫面切換是它自己的 state，沒有外部入口，而為了這一頁在正式程式碼
/// 上開一個 prop 不划算。按鈕要等 `get_snapshot` 回來才畫得出來，所以等到
/// 它出現為止。
function openScreen(label: string) {
  const tick = () => {
    const button = [
      ...document.querySelectorAll<HTMLButtonElement>(".actions button"),
    ].find((candidate) => candidate.textContent?.trim() === label);
    if (button) button.click();
    else requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

const { default: App } = await import("./App");
createRoot(document.getElementById("root")!).render(<App />);

const ENTRY: Record<string, string> = {
  about: "關於",
  settings: "設定",
  sessions: "最近",
};
if (ENTRY[SCREEN]) openScreen(ENTRY[SCREEN]);
