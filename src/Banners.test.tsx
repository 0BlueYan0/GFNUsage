import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Banners from "./Banners";

const DAY = 86_400_000;
/// 多加一小時：`daysUntil` 無條件捨去，抓得剛剛好的話，測試自己跑掉的
/// 那幾毫秒就會讓 3 天變成 2 天。
const inDays = (days: number) =>
  new Date(Date.now() + days * DAY + 3_600_000).toISOString();

describe("Banners", () => {
  it("憑證剩不到七天時提醒重新登入", () => {
    render(
      <Banners
        clientTokenExpiresAt={inDays(3)}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.getByText(/再 3 天到期/)).toBeTruthy();
  });

  it("憑證還很久時什麼都不說", () => {
    render(
      <Banners
        clientTokenExpiresAt={inDays(30)}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.queryByText(/到期/)).toBeNull();
  });

  /// 里程碑 1／2 存下的憑證沒有到期時刻。不知道就別講 ——
  /// `daysUntil` 會把 null 以外的東西夾到 0，顯示出來就是「今天到期」。
  it("不知道到期時刻就不顯示橫幅", () => {
    render(
      <Banners
        clientTokenExpiresAt={null}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.queryByText(/到期/)).toBeNull();
  });

  it("按下知道了就把提示收掉", () => {
    const onDismissHint = vi.fn();
    render(
      <Banners
        clientTokenExpiresAt={null}
        showTrayHint
        busy={false}
        onDismissHint={onDismissHint}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "知道了" }));

    expect(onDismissHint).toHaveBeenCalled();
  });

  it("不必顯示提示時連按鈕都沒有", () => {
    render(
      <Banners
        clientTokenExpiresAt={null}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button")).toBeNull();
  });
});
