import { describe, expect, it } from "vitest";
import { daysUntil, formatHours, formatResetAt, percentUsed } from "./types";

describe("formatHours", () => {
  it("轉換分鐘為一位小數的小時", () => {
    expect(formatHours(6180)).toBe("103.0");
    expect(formatHours(6900)).toBe("115.0");
    expect(formatHours(720)).toBe("12.0");
    expect(formatHours(0)).toBe("0.0");
  });

  it("保留小數，不讓 90 分鐘看起來像 1 小時", () => {
    expect(formatHours(90)).toBe("1.5");
  });
});

describe("formatResetAt", () => {
  // 重置時點是 UTC，在台灣會落到隔天早上 —— 這是最容易誤解的一點。
  it("把 UTC 的重置時點換算成台北時間", () => {
    expect(formatResetAt("2026-10-15T23:59:59.999Z", "Asia/Taipei")).toBe(
      "10/16 07:59",
    );
  });

  it("同一個時點在 UTC 下是前一天", () => {
    expect(formatResetAt("2026-10-15T23:59:59.999Z", "UTC")).toBe("10/15 23:59");
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

describe("percentUsed", () => {
  it("算出已用百分比", () => {
    expect(percentUsed(720, 6900)).toBe(10);
    expect(percentUsed(6900, 6900)).toBe(100);
  });

  it("總額為零時不會除以零", () => {
    expect(percentUsed(0, 0)).toBe(0);
  });
});
