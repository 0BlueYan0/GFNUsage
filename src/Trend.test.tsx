import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import Trend from "./Trend";
import type { DailyPoint } from "./types";

const TOTAL = 6000;

/// 三天實線（0 → 1500 → 3000）接四天虛線，第三天是今天。
function daily(overrides: Partial<DailyPoint>[] = []): DailyPoint[] {
  const base: DailyPoint[] = [
    { date: "2026-09-01", usedMinutes: 0, projectedUsedMinutes: null },
    { date: "2026-09-02", usedMinutes: 1500, projectedUsedMinutes: null },
    { date: "2026-09-03", usedMinutes: 3000, projectedUsedMinutes: 3000 },
    { date: "2026-09-04", usedMinutes: null, projectedUsedMinutes: 3750 },
    { date: "2026-09-05", usedMinutes: null, projectedUsedMinutes: 4500 },
    { date: "2026-09-06", usedMinutes: null, projectedUsedMinutes: 5250 },
  ];
  return base.map((point, index) => ({ ...point, ...overrides[index] }));
}

function draw(points: DailyPoint[], metric: "remaining" | "used" = "remaining") {
  return render(
    <Trend
      daily={points}
      metric={metric}
      totalMinutes={TOTAL}
      state="normal"
    />,
  ).container;
}

/** 折線的第一個座標，拆成 x 與 y。 */
function firstPoint(container: HTMLElement, selector: string) {
  const points = container.querySelector(selector)?.getAttribute("points");
  const [x, y] = points!.split(" ")[0].split(",").map(Number);
  return { x, y };
}

describe("Trend", () => {
  it("實線與虛線都畫", () => {
    const container = draw(daily());
    expect(container.querySelector(".trend__actual")).toBeTruthy();
    expect(container.querySelector(".trend__projected")).toBeTruthy();
  });

  /// 這一點是實線與虛線的交界。本期第一天只有一個實線的點，沒有它那天
  /// 整條線是空的。
  it("今天那一點畫在最後一個實線值上", () => {
    const container = draw(daily());
    const today = container.querySelector(".trend__today");
    const actual = container.querySelector(".trend__actual");
    const drawn = actual!.getAttribute("points")!.split(" ");
    const last = drawn[drawn.length - 1];
    expect(`${today!.getAttribute("cx")},${today!.getAttribute("cy")}`).toBe(
      last,
    );
  });

  /// 切換 metric 只該讓線上下翻，不該讓它變形，所以兩種模式共用 0 到 T
  /// 的範圍。已使用 0 在底部，剩餘 6000 在頂部。
  it("metric 換邊時第一點上下翻", () => {
    const remaining = firstPoint(draw(daily()), ".trend__actual");
    const used = firstPoint(draw(daily(), "used"), ".trend__actual");
    expect(remaining.y).toBe(2);
    expect(used.y).toBe(62);
    expect(remaining.x).toBe(used.x);
  });

  /// 額度用完之後不再往前畫。畫下去會沿著邊緣走成一條平的，看起來像
  /// 後來不玩了。
  it("虛線碰到額度上限就停住", () => {
    const overshooting = daily([
      {},
      {},
      {},
      { projectedUsedMinutes: 6000 },
      { projectedUsedMinutes: 9000 },
      { projectedUsedMinutes: 12000 },
    ]);
    const container = draw(overshooting);
    const points = container
      .querySelector(".trend__projected")!
      .getAttribute("points")!
      .split(" ");
    // 今天那一點加上碰到上限的那一點，後面兩點不畫。
    expect(points).toHaveLength(2);
  });

  it("沒有資料就什麼都不畫", () => {
    expect(draw([]).querySelector(".trend")).toBeNull();
  });

  /// 後端還沒給預測（樣本不夠）時只畫實線。
  it("沒有預測時不畫虛線", () => {
    const container = draw(
      daily().map((point) => ({ ...point, projectedUsedMinutes: null })),
    );
    expect(container.querySelector(".trend__actual")).toBeTruthy();
    expect(container.querySelector(".trend__projected")).toBeNull();
  });

  /// 顏色掛在 svg 上，兩個標記都吃 `currentColor`，所以狀態修飾詞只有一組。
  it("超前消耗時整張圖換色", () => {
    const container = render(
      <Trend
        daily={daily()}
        metric="remaining"
        totalMinutes={TOTAL}
        state="overPace"
      />,
    ).container;
    expect(container.querySelector(".trend--overPace")).toBeTruthy();
  });
});
