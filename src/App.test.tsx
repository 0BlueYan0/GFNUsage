import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const listen = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: (...args: unknown[]) => listen(...args) }));

const { default: App } = await import("./App");

/// 最小的一份：有登入、有快照，主面板畫得出來。
function panelData(remaining: number) {
  return {
    snapshot: {
      tier: "ULTIMATE",
      timeCapped: true,
      totalMinutes: 6900,
      remainingMinutes: remaining,
      usedMinutes: 6900 - remaining,
      rolledOverMinutes: 0,
      rolloverCapMinutes: 900,
      purchasedMinutes: 0,
      spanStart: "2026-09-15T13:18:59Z",
      spanEnd: "2026-10-15T23:59:59Z",
      fetchedAt: "2026-09-21T08:00:00Z",
    },
    pace: null,
    state: "normal",
    lastError: null,
    hasCredentials: true,
    needsLogin: false,
    clientTokenExpiresAt: null,
    showTrayHint: false,
    loginPending: false,
    metric: "remaining",
    pollInterval: "30m",
    taskbarWidget: { enabled: false, side: "trayLeft" },
    widgetSupported: false,
    recentSessions: [],
    daily: [],
    updateVersion: null,
  };
}

describe("App", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
    listen.mockResolvedValue(() => {});
  });

  /// 面板收起來走的是原生視窗的 hide，`visibilitychange` 不會來。少了這個
  /// 訂閱，面板就停在啟動當下那一份 —— 系統匣已經有數字了它還寫「沒有資料」。
  it("訂閱面板顯示事件", async () => {
    invoke.mockResolvedValue(panelData(6000));
    render(<App />);

    await waitFor(() => expect(listen).toHaveBeenCalled());
    expect(listen.mock.calls[0][0]).toBe("panel-shown");
  });

  it("面板一顯示就重讀一次資料", async () => {
    invoke.mockResolvedValue(panelData(6000));
    render(<App />);
    await waitFor(() => expect(screen.getByText("100")).toBeTruthy());

    // 後端送事件。第二次回的是不一樣的數字，畫面要跟著換。
    invoke.mockResolvedValue(panelData(3000));
    const fire = listen.mock.calls[0][1] as () => void;
    fire();

    await waitFor(() => expect(screen.getByText("50")).toBeTruthy());
  });

  /// 面板在安裝最後一刻打開：讀進度的 invoke 已送出（後端還在 99%），安裝
  /// 失敗的 null 事件先到，invoke 才回來。回來那份是過期的，不能把 99% 寫回去，
  /// 不然橫幅停在按不下去的「99%」，「更新」與「知道了」都不見。
  it("進度事件之後才回來的狀態讀取不覆蓋事件", async () => {
    let resolveStatus: (status: unknown) => void = () => {};
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_update_status") {
        return new Promise((resolve) => {
          resolveStatus = resolve;
        });
      }
      return Promise.resolve({ ...panelData(6000), updateVersion: "0.4.0" });
    });
    render(<App />);
    await waitFor(() => expect(screen.getByText("有新版本 0.4.0")).toBeTruthy());

    const onProgress = listen.mock.calls.find(
      ([name]) => name === "update-progress",
    )?.[1] as (event: { payload: unknown }) => void;
    onProgress({ payload: null });
    resolveStatus({
      version: "0.4.0",
      installing: false,
      progress: { kind: "downloading", value: 99 },
    });

    // 等 invoke 的續行跑完再看畫面。用 waitFor 的話第一次檢查就過了，那時
    // 續行還沒跑，看到的是事件之前的畫面。
    await act(async () => {});
    expect(screen.getByRole("button", { name: "更新" })).toBeTruthy();
    expect(screen.queryByText("99%")).toBeNull();
  });
});
