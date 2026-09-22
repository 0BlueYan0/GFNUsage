import { describe, expect, it } from "vitest";
import {
  daysUntil,
  formatCountdown,
  formatDuration,
  formatForecast,
  formatHeroUnit,
  formatOverPace,
  formatLocalDateTime,
  modifier,
  percentOf,
  splitDuration,
} from "./format";
import type { PaceReport } from "./types";

describe("splitDuration", () => {
  it("拆成小時與分鐘", () => {
    expect(splitDuration(6185)).toEqual({ hours: 103, mins: 5 });
    expect(splitDuration(6180)).toEqual({ hours: 103, mins: 0 });
  });

  it("不滿一小時時小時是 0", () => {
    expect(splitDuration(45)).toEqual({ hours: 0, mins: 45 });
  });

  it("負數當成零", () => {
    expect(splitDuration(-5)).toEqual({ hours: 0, mins: 0 });
  });
});

describe("formatLocalDateTime", () => {
  // 重置時點是 UTC，在台灣會落到隔天早上 —— 這是最容易誤解的一點。
  it("把 UTC 的重置時點換算成台北時間", () => {
    expect(formatLocalDateTime("2026-10-15T23:59:59.999Z", "Asia/Taipei")).toBe(
      "10/16 07:59",
    );
  });

  it("同一個時點在 UTC 下是前一天", () => {
    expect(formatLocalDateTime("2026-10-15T23:59:59.999Z", "UTC")).toBe("10/15 23:59");
  });

  // 較新的 CLDR 把 zh-Hant 的日期時間分隔改成窄不斷行空格。肉眼看不出來，
  // 但字串比不相等，而使用者拿到哪一個由他機器上的 ICU 決定。
  it("分隔用半形空格，不用 ICU 給的那個", () => {
    const out = formatLocalDateTime("2026-10-15T23:59:59.999Z", "UTC");
    expect(out).not.toMatch(/[^ -~]/);
  });
});

describe("daysUntil", () => {
  it("計算距離重置的完整天數", () => {
    const now = new Date("2026-09-20T01:28:00Z");
    expect(daysUntil("2026-10-15T23:59:59.999Z", now)).toBe(25);
  });

  it("已經過期時回傳 0 而不是負數", () => {
    const now = new Date("2026-10-20T00:00:00Z");
    expect(daysUntil("2026-10-15T23:59:59.999Z", now)).toBe(0);
  });
});

describe("percentOf", () => {
  it("算出四捨五入的整數百分比", () => {
    expect(percentOf(720, 6900)).toBe(10);
    expect(percentOf(0, 6900)).toBe(0);
    expect(percentOf(6900, 6900)).toBe(100);
  });

  it("總量為零時不除以零", () => {
    expect(percentOf(10, 0)).toBe(0);
  });
});

describe("formatDuration", () => {
  it("不足一小時講分鐘", () => {
    expect(formatDuration(24)).toBe("24 分鐘");
    expect(formatDuration(59.4)).toBe("59 分鐘");
  });

  it("有餘數就把餘數講成分鐘，不用小數點", () => {
    expect(formatDuration(90)).toBe("1 小時 30 分鐘");
    expect(formatDuration(6185)).toBe("103 小時 5 分鐘");
  });

  it("整點不畫蛇添足加 0 分鐘", () => {
    expect(formatDuration(120)).toBe("2 小時");
    expect(formatDuration(6180)).toBe("103 小時");
  });

  it("負數當成零", () => {
    expect(formatDuration(-5)).toBe("0 分鐘");
  });
});

describe("formatOverPace", () => {
  it("超前時直說超前多少", () => {
    expect(formatOverPace(1000)).toBe("超前 16 小時 40 分鐘");
  });

  it("落後時是好消息，說低於門檻", () => {
    expect(formatOverPace(-500)).toBe("低於門檻 8 小時 20 分鐘");
  });
});

describe("formatCountdown", () => {
  const at = (iso: string) => new Date(iso);

  it("一天以上同時給天與小時", () => {
    expect(
      formatCountdown("2026-10-16T07:59:00+08:00", at("2026-09-21T04:59:00+08:00")),
    ).toBe("25 天 3 小時");
  });

  it("整天數時不畫蛇添足加 0 小時", () => {
    expect(
      formatCountdown("2026-10-16T07:59:00+08:00", at("2026-09-21T07:59:00+08:00")),
    ).toBe("25 天");
  });

  /// 這是最重要的一條：舊的寫法在這裡會退化成「還有 0 天」，
  /// 而剩不到一天正是最需要知道確切還有多久的時候。
  it("不到一天就改講小時", () => {
    expect(
      formatCountdown("2026-10-16T07:59:00+08:00", at("2026-10-15T23:59:00+08:00")),
    ).toBe("8 小時");
  });

  it("不到一小時就改講分鐘", () => {
    expect(
      formatCountdown("2026-10-16T07:59:00+08:00", at("2026-10-16T07:19:00+08:00")),
    ).toBe("40 分鐘");
  });

  it("已經過去的時點是 0 分鐘，不給負數", () => {
    expect(
      formatCountdown("2026-10-16T07:59:00+08:00", at("2026-10-17T00:00:00+08:00")),
    ).toBe("0 分鐘");
  });
});

describe("formatHeroUnit", () => {
  /// 大字那一行沒有用 formatDuration，所以「不用小數點的小時」那條規則
  /// 上次沒管到它，留著「5 分」。這幾個斷言就是為了不再發生。
  it("有餘數就把餘數講成分鐘", () => {
    expect(formatHeroUnit(6185)).toBe("小時 5 分鐘");
  });

  it("整點不講零分", () => {
    expect(formatHeroUnit(120)).toBe("小時");
  });

  it("不滿一小時時單位就是分鐘", () => {
    expect(formatHeroUnit(45)).toBe("分鐘");
    expect(formatHeroUnit(0)).toBe("分鐘");
  });

  /// 大字印的是小時、單位那串補餘數，兩段合起來要跟 formatDuration 同一個說法。
  it("跟 formatDuration 說的是同一件事", () => {
    const minutes = 6185;
    const { hours } = splitDuration(minutes);
    expect(`${hours} ${formatHeroUnit(minutes)}`).toBe(formatDuration(minutes));
  });
});

describe("modifier", () => {
  it("normal 與 freeTier 不加修飾詞", () => {
    expect(modifier("trend", "normal")).toBe("trend");
    expect(modifier("trend", "freeTier")).toBe("trend");
  });

  /// 進度條與走勢圖吃同一組，所以類別名要跟 CSS 裡那幾條對得上。
  it("其餘狀態把基底與修飾詞一起給", () => {
    expect(modifier("trend", "overPace")).toBe("trend trend--overPace");
    expect(modifier("meter__fill", "low")).toBe("meter__fill meter__fill--low");
  });
});

describe("formatForecast", () => {
  const report = (overrides: Partial<PaceReport> = {}): PaceReport => ({
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
  });

  it("預估會剩下時報告結轉浪費", () => {
    const lines = formatForecast(report(), 6000)!.split("\n");
    expect(lines[0]).toBe("月底約用 75 小時，會剩 25 小時");
    expect(lines[1]).toBe("10 小時 會浪費掉");
  });

  it("預估超支時報告超支多少與哪天用完", () => {
    const lines = formatForecast(
      report({
        projectedUsedMinutes: 9000,
        overshootMinutes: 3000,
        wastedMinutes: 0,
        runsOutAt: "2026-09-21T00:00:00Z",
      }),
      6000,
    )!.split("\n");
    expect(lines[0]).toBe("月底約用 150 小時，超支 50 小時");
    expect(lines[1]).toMatch(/用完$/);
  });

  /// 「會剩 25 小時」已經回答了會不會用完，不再補一句。
  it("不會用完時不再補一句", () => {
    const text = formatForecast(report({ wastedMinutes: 0 }), 6000);
    expect(text).toBe("月底約用 75 小時，會剩 25 小時");
  });

  /// 樣本還不夠時圖上沒有虛線，tooltip 也就沒有東西可講。
  it("沒有預測時回 null", () => {
    expect(
      formatForecast(
        report({ projectedUsedMinutes: null, overshootMinutes: null }),
        6000,
      ),
    ).toBeNull();
  });
});
