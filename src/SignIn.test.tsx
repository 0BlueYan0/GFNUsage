import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import SignIn from "./SignIn";

const props = {
  busy: false,
  loggingIn: false,
  error: null,
  needsLogin: false,
  banners: null,
  onLogin: vi.fn(),
  onCancelLogin: vi.fn(),
  onImportLocal: vi.fn(),
  onImportManual: vi.fn(),
};

describe("SignIn", () => {
  it("沒在登入時不顯示取消", () => {
    render(<SignIn {...props} />);

    expect(screen.queryByRole("button", { name: "取消登入" })).toBeNull();
    expect(screen.getByRole("button", { name: "登入" })).toBeTruthy();
  });

  /// 這是整條修正的重點：使用者關掉瀏覽器分頁之後，得有辦法退出來，
  /// 不必盯著一個沒反應的面板等滿五分鐘。
  it("登入途中可以取消", () => {
    const onCancelLogin = vi.fn();
    render(<SignIn {...props} loggingIn onCancelLogin={onCancelLogin} />);

    fireEvent.click(screen.getByRole("button", { name: "取消登入" }));

    expect(onCancelLogin).toHaveBeenCalled();
  });

  /// 備援路徑就在同一個畫面上。登入卡住時那正是使用者該按的按鈕，
  /// 不能跟著一起被鎖起來。
  it("登入途中匯入那條路仍然按得下去", () => {
    const onImportLocal = vi.fn();
    render(<SignIn {...props} loggingIn onImportLocal={onImportLocal} />);

    fireEvent.click(
      screen.getByRole("button", { name: "從本機 GeForce NOW 匯入" }),
    );

    expect(onImportLocal).toHaveBeenCalled();
  });

  /// 登入途中再按一次登入只會多綁一個埠、多開一個分頁。
  it("登入途中不能再按一次登入", () => {
    render(<SignIn {...props} loggingIn />);

    const login = screen.getByRole("button", { name: "等待瀏覽器…" });
    expect((login as HTMLButtonElement).disabled).toBe(true);
  });

  /// `busy` 是別的動作（匯入、登出）在跑。那時全部鎖住是對的，
  /// 但不該冒出一個取消登入的按鈕 —— 根本沒有登入在跑。
  it("其他動作進行中時全部鎖住，也沒有取消鈕", () => {
    render(<SignIn {...props} busy />);

    expect(screen.queryByRole("button", { name: "取消登入" })).toBeNull();
    const importLocal = screen.getByRole("button", {
      name: "從本機 GeForce NOW 匯入",
    });
    expect((importLocal as HTMLButtonElement).disabled).toBe(true);
  });
});
