import { describe, expect, it } from "vitest";
import {
  daysUntil,
  formatDuration,
  formatHours,
  formatOverPace,
  formatResetAt,
  percentUsed,
} from "./format";

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
  it("算出四捨五入的整數百分比", () => {
    expect(percentUsed(720, 6900)).toBe(10);
    expect(percentUsed(0, 6900)).toBe(0);
    expect(percentUsed(6900, 6900)).toBe(100);
  });

  it("總量為零時不除以零", () => {
    expect(percentUsed(10, 0)).toBe(0);
  });
});

describe("formatDuration", () => {
  it("不足一小時講分鐘", () => {
    expect(formatDuration(24)).toBe("24 分鐘");
    expect(formatDuration(59.4)).toBe("59 分鐘");
  });

  it("超過一小時講小時", () => {
    expect(formatDuration(90)).toBe("1.5 小時");
    expect(formatDuration(6180)).toBe("103.0 小時");
  });

  it("負數當成零", () => {
    expect(formatDuration(-5)).toBe("0 分鐘");
  });
});

describe("formatOverPace", () => {
  it("超前時直說超前多少", () => {
    expect(formatOverPace(1000)).toBe("超前 16.7 小時");
  });

  it("落後時是好消息，說低於門檻", () => {
    expect(formatOverPace(-500)).toBe("低於門檻 8.3 小時");
  });
});
