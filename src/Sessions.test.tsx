import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Sessions from "./Sessions";
import type { PlaySession } from "./types";

function session(overrides: Partial<PlaySession> = {}): PlaySession {
  return {
    gameTitle: "Wuthering Waves",
    startedAt: "2026-09-21T04:24:42.000Z",
    endedAt: "2026-09-21T06:21:03.000Z",
    minutes: 116,
    ...overrides,
  };
}

describe("Sessions", () => {
  it("一場都沒有時說沒有紀錄，不是空白一頁", () => {
    render(<Sessions onClose={() => {}} sessions={[]} />);

    expect(screen.getByText("沒有紀錄")).toBeTruthy();
    expect(document.querySelectorAll(".sessions__row")).toHaveLength(0);
  });

  /// 這一頁是獨立的，沒有返回就出不去。
  it("返回叫得到 onClose", () => {
    const onClose = vi.fn();
    render(<Sessions onClose={onClose} sessions={[]} />);

    fireEvent.click(screen.getByRole("button", { name: "返回" }));

    expect(onClose).toHaveBeenCalled();
  });

  it("列出遊戲名稱與時長", () => {
    render(<Sessions onClose={() => {}} sessions={[session()]} />);

    expect(screen.getByText("Wuthering Waves")).toBeTruthy();
    expect(screen.getByText("1 小時 56 分鐘")).toBeTruthy();
  });

  /// 時間長度不用小數點的小時，跟面板其他地方同一條規則。
  it("時長沒有小數點", () => {
    render(<Sessions onClose={() => {}} sessions={[session({ minutes: 150 })]} />);

    expect(screen.getByText("2 小時 30 分鐘")).toBeTruthy();
    expect(screen.queryByText(/2\.5/)).toBeNull();
  });

  /// 名稱缺席不能讓那一列消失 —— 玩了多久才是這一區的重點。
  it("沒有遊戲名稱時只留時間與時長", () => {
    render(<Sessions onClose={() => {}} sessions={[session({ gameTitle: "", minutes: 45 })]} />);

    expect(screen.getByText("45 分鐘")).toBeTruthy();
    expect(document.querySelectorAll(".sessions__row")).toHaveLength(1);
  });

  it("幾場就畫幾列", () => {
    render(
      <Sessions
        onClose={() => {}}
        sessions={[
          session({ startedAt: "2026-09-21T04:24:42.000Z" }),
          session({ startedAt: "2026-09-20T04:24:42.000Z" }),
          session({ startedAt: "2026-09-19T04:24:42.000Z" }),
        ]}
      />,
    );

    expect(document.querySelectorAll(".sessions__row")).toHaveLength(3);
  });

  /// 還在玩的那一場沒有結束時間，不能因此整列不畫。
  it("還在玩的那一場照樣列出來", () => {
    render(<Sessions onClose={() => {}} sessions={[session({ endedAt: null, minutes: 10 })]} />);

    expect(screen.getByText("10 分鐘")).toBeTruthy();
  });
});
