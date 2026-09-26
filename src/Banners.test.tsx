import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Banners from "./Banners";

const DAY = 86_400_000;
/// 多加 90 分鐘：兩個函式都無條件捨去，抓得剛剛好的話，測試自己跑掉的
/// 那幾毫秒就會讓 3 天變成 2 天、1 小時變成 0 小時。90 分鐘離兩個
/// 進位邊界都夠遠。
const inDays = (days: number) =>
  new Date(Date.now() + days * DAY + 90 * 60_000).toISOString();

describe("Banners", () => {
  it("登入剩不到七天到期時提醒", () => {
    render(
      <Banners
        clientTokenExpiresAt={inDays(3)}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.getByText(/3 天 1 小時後到期/)).toBeTruthy();
  });

  it("還很久時什麼都不說", () => {
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

  /// 這是加精度的重點：剩不到一天時，舊的寫法只會說「今天到期」，
  /// 而「還有 5 小時」跟「還有 20 分鐘」該做的事完全不同。
  it("剩不到一天時講小時", () => {
    const inFiveHours = new Date(Date.now() + 5 * 3_600_000 + 60_000);
    render(
      <Banners
        clientTokenExpiresAt={inFiveHours.toISOString()}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.getByText(/5 小時後到期/)).toBeTruthy();
  });

  /// 主面板上唯一能重新登入的地方。那裡的「登出」做的是相反的事。
  it("到期橫幅上的重新登入會呼叫 onLogin", () => {
    const onLogin = vi.fn();
    render(
      <Banners
        clientTokenExpiresAt={inDays(3)}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
        onLogin={onLogin}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "重新登入" }));

    expect(onLogin).toHaveBeenCalled();
  });

  /// 登入畫面用的是同一個元件，但那裡底下就有登入鈕。
  it("沒給 onLogin 就不畫重新登入", () => {
    render(
      <Banners
        clientTokenExpiresAt={inDays(3)}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: "重新登入" })).toBeNull();
  });

  /// 登入途中留著重新登入鈕，再按一次只會多綁一個埠、多開一個分頁。
  /// 換成取消，順便給使用者一條出口。
  it("登入途中換成取消登入", () => {
    const onCancelLogin = vi.fn();
    render(
      <Banners
        clientTokenExpiresAt={inDays(3)}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
        onLogin={vi.fn()}
        onCancelLogin={onCancelLogin}
        loggingIn
      />,
    );

    expect(screen.queryByRole("button", { name: "重新登入" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "取消登入" }));

    expect(onCancelLogin).toHaveBeenCalled();
  });

  it("已經過期就直說過期，不講剩幾分鐘", () => {
    const anHourAgo = new Date(Date.now() - 3_600_000);
    render(
      <Banners
        clientTokenExpiresAt={anHourAgo.toISOString()}
        showTrayHint={false}
        busy={false}
        onDismissHint={vi.fn()}
      />,
    );

    expect(screen.getByText(/登入已過期/)).toBeTruthy();
  });

  it("有新版本時畫一條橫幅", () => {
    render(
      <Banners
        clientTokenExpiresAt={null}
        showTrayHint={false}
        updateVersion="0.1.1"
        busy={false}
        onDismissHint={vi.fn()}
        onInstallUpdate={vi.fn()}
        onDismissUpdate={vi.fn()}
      />,
    );

    expect(screen.getByText("有新版本 0.1.1")).toBeTruthy();
  });

  it("更新與知道了各叫各的", () => {
    const onInstallUpdate = vi.fn();
    const onDismissUpdate = vi.fn();
    render(
      <Banners
        clientTokenExpiresAt={null}
        showTrayHint={false}
        updateVersion="0.1.1"
        busy={false}
        onDismissHint={vi.fn()}
        onInstallUpdate={onInstallUpdate}
        onDismissUpdate={onDismissUpdate}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "更新" }));
    fireEvent.click(screen.getByRole("button", { name: "知道了" }));

    expect(onInstallUpdate).toHaveBeenCalledTimes(1);
    expect(onDismissUpdate).toHaveBeenCalledTimes(1);
  });

  /// 安裝途中「更新」再按是重裝，「知道了」會把正在看的進度收掉。
  it("安裝中換成進度，知道了收掉", () => {
    render(
      <Banners
        clientTokenExpiresAt={null}
        showTrayHint={false}
        updateVersion="0.1.1"
        updateProgress={{ kind: "downloading", value: 42 }}
        busy={false}
        onDismissHint={vi.fn()}
        onInstallUpdate={vi.fn()}
        onDismissUpdate={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "42%" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "更新" })).toBeNull();
    expect(screen.queryByRole("button", { name: "知道了" })).toBeNull();
  });

  /// 登入畫面不談更新，`App` 那邊傳的就是 null。
  it("沒有新版本就不畫", () => {
    render(
      <Banners
        clientTokenExpiresAt={null}
        showTrayHint={false}
        updateVersion={null}
        busy={false}
        onDismissHint={vi.fn()}
        onInstallUpdate={vi.fn()}
      />,
    );

    expect(screen.queryByText(/有新版本/)).toBeNull();
  });
});
