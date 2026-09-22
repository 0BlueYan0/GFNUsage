import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import Pace from "./Pace";
import type { PaceReport } from "./types";

function report(overrides: Partial<PaceReport> = {}): PaceReport {
  return {
    availPastMinutes: 14400,
    availLeftMinutes: 28800,
    expectedUsedMinutes: 2000,
    overPaceMinutes: -500,
    burnRate: 0.104,
    projectedUsedMinutes: 4500,
    overshootMinutes: -1500,
    runsOutAt: null,
    wastedMinutes: 600,
    todayBudgetMinutes: 225,
    note: null,
    ...overrides,
  };
}

describe("Pace", () => {
  it("顯示已用與期望的對照", () => {
    render(<Pace pace={report()} usedMinutes={1500} />);
    expect(screen.getByText(/已用 25 小時/)).toBeTruthy();
    expect(screen.getByText(/期望 33 小時 20 分鐘/)).toBeTruthy();
    expect(screen.getByText(/低於門檻/)).toBeTruthy();
  });

  it("超前消耗時說超前多少", () => {
    render(<Pace pace={report({ overPaceMinutes: 1000 })} usedMinutes={3000} />);
    expect(screen.getByText(/超前 16 小時 40 分鐘/)).toBeTruthy();
  });

  it("今日額度一律顯示", () => {
    render(<Pace pace={report()} usedMinutes={1500} />);
    expect(screen.getByText(/今天還能玩 3 小時 45 分鐘/)).toBeTruthy();
  });

  /// 預測搬去走勢圖的 tooltip 了（`formatForecast`）。這裡留一條守著，
  /// 免得哪天有人又把同一件事塞回這一區，畫面上就講了兩次。
  it("不講預測，那是走勢圖的事", () => {
    render(
      <Pace
        pace={report({
          projectedUsedMinutes: 9000,
          overshootMinutes: 3000,
          runsOutAt: "2026-09-21T00:00:00Z",
        })}
        usedMinutes={3000}
      />,
    );
    expect(screen.queryByText(/月底約用/)).toBeNull();
    expect(screen.queryByText(/用完/)).toBeNull();
    expect(screen.queryByText(/浪費掉/)).toBeNull();
  });

  it("樣本不足時只說資料累積中", () => {
    render(
      <Pace
        pace={report({
          note: "collecting",
          projectedUsedMinutes: null,
          overshootMinutes: null,
          wastedMinutes: null,
        })}
        usedMinutes={100}
      />,
    );
    expect(screen.getByText(/資料累積中/)).toBeTruthy();
  });

  it("沒有可遊玩時間時不給今日額度", () => {
    render(
      <Pace
        pace={report({
          note: "noTimeLeft",
          availLeftMinutes: 0,
          todayBudgetMinutes: null,
          projectedUsedMinutes: null,
          wastedMinutes: null,
        })}
        usedMinutes={1500}
      />,
    );
    expect(screen.getByText(/沒有可遊玩時間了/)).toBeTruthy();
    expect(screen.queryByText(/今天還能玩/)).toBeNull();
  });
});
