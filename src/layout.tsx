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
import type { PanelData } from "./types";

function session(startedAt: string, gameTitle: string, minutes: number) {
  return { gameTitle, startedAt, endedAt: null, minutes };
}

/// 最擠的情況：40 場紀錄、配速三列都在、加購與上期未用完都有值。
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
};

// `invoke` 走 `window.__TAURI_INTERNALS__`，在瀏覽器裡沒有這個物件。
// 先補上再載入 `App` —— import 會被提升，所以用動態 import。
/// `?screen=signin` 看登入畫面，不帶參數看主面板。兩個畫面的容器不一樣
/// （`.panel--scroll` 對 `.panel`），改版面兩個都要看。
const SIGN_IN = new URLSearchParams(location.search).get("screen") === "signin";

(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: async (command: string) => {
    if (command === "get_snapshot") {
      return SIGN_IN
        ? { ...DATA, snapshot: null, pace: null, hasCredentials: false, recentSessions: [] }
        : DATA;
    }
    return null;
  },
  transformCallback: (callback: unknown) => callback,
};

const { default: App } = await import("./App");
createRoot(document.getElementById("root")!).render(<App />);
