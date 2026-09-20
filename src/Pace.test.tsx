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
    render(<Pace pace={report()} usedMinutes={1500} totalMinutes={6000} />);
    expect(screen.getByText(/已用 25 小時/)).toBeTruthy();
    expect(screen.getByText(/期望 33 小時 20 分鐘/)).toBeTruthy();
    expect(screen.getByText(/低於門檻/)).toBeTruthy();
  });

  it("超前消耗時說超前多少", () => {
    render(
      <Pace
        pace={report({ overPaceMinutes: 1000 })}
        usedMinutes={3000}
        totalMinutes={6000}
      />,
    );
    expect(screen.getByText(/超前 16 小時 40 分鐘/)).toBeTruthy();
  });

  it("預估會剩下時報告結轉浪費", () => {
    render(<Pace pace={report()} usedMinutes={1500} totalMinutes={6000} />);
    expect(screen.getByText(/會剩 25 小時/)).toBeTruthy();
    expect(screen.getByText(/10 小時.*浪費掉/)).toBeTruthy();
  });

  it("預估超支時報告超支多少與哪天用完", () => {
    render(
      <Pace
        pace={report({
          overPaceMinutes: 1000,
          projectedUsedMinutes: 9000,
          overshootMinutes: 3000,
          wastedMinutes: 0,
          runsOutAt: "2026-09-21T00:00:00Z",
        })}
        usedMinutes={3000}
        totalMinutes={6000}
      />,
    );
    expect(screen.getByText(/超支 50 小時/)).toBeTruthy();
    expect(screen.getByText(/用完/)).toBeTruthy();
  });

  it("今日額度一律顯示", () => {
    render(<Pace pace={report()} usedMinutes={1500} totalMinutes={6000} />);
    expect(screen.getByText(/今天還能玩 3 小時 45 分鐘/)).toBeTruthy();
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
        totalMinutes={6000}
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
        totalMinutes={6000}
      />,
    );
    expect(screen.getByText(/沒有可遊玩時間了/)).toBeTruthy();
    expect(screen.queryByText(/今天還能玩/)).toBeNull();
  });
  /// 「會剩 25 小時」已經回答了會不會用完，不再補一句。
  it("不會用完時不再補一句", () => {
    render(
      <Pace
        pace={report({ runsOutAt: null, overshootMinutes: -1500 })}
        usedMinutes={1500}
        totalMinutes={6000}
      />,
    );
    expect(screen.getByText(/會剩 25 小時/)).toBeTruthy();
    expect(screen.queryByText(/不會用完/)).toBeNull();
  });
});
