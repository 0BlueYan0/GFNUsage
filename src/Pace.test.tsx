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
  it("今日額度一律顯示", () => {
    render(<Pace pace={report()} />);
    expect(screen.getByText(/今天還能玩 3 小時 45 分鐘/)).toBeTruthy();
  });

  /// 配速變成進度條上的那條線，預測是走勢圖那條虛線，兩邊的字都在各自的
  /// tooltip 裡。這一條守著它們不要又被塞回這一區，畫面上講兩次。
  it("不講配速也不講預測", () => {
    render(
      <Pace
        pace={report({
          overPaceMinutes: 1000,
          projectedUsedMinutes: 9000,
          overshootMinutes: 3000,
          runsOutAt: "2026-09-21T00:00:00Z",
        })}
      />,
    );
    expect(screen.queryByText(/已用/)).toBeNull();
    expect(screen.queryByText(/期望/)).toBeNull();
    expect(screen.queryByText(/超前/)).toBeNull();
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
      />,
    );
    expect(screen.getByText(/沒有可遊玩時間了/)).toBeTruthy();
    expect(screen.queryByText(/今天還能玩/)).toBeNull();
  });
});
