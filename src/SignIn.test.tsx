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
};

describe("SignIn", () => {
  it("沒在登入時不顯示取消", () => {
    render(<SignIn {...props} />);

    expect(screen.queryByRole("button", { name: "取消登入" })).toBeNull();
    expect(screen.getByRole("button", { name: "登入" })).toBeTruthy();
  });

  /// 使用者把登入視窗關掉之後，得有辦法退出來，不必盯著一個沒反應的面板
  /// 等滿五分鐘。匯入那條備援路徑拆掉之後，這是唯一的出口。
  it("登入途中可以取消", () => {
    const onCancelLogin = vi.fn();
    render(<SignIn {...props} loggingIn onCancelLogin={onCancelLogin} />);

    fireEvent.click(screen.getByRole("button", { name: "取消登入" }));

    expect(onCancelLogin).toHaveBeenCalled();
  });

  /// 登入途中再按一次登入只會多開一個視窗。
  it("登入途中不能再按一次登入", () => {
    render(<SignIn {...props} loggingIn />);

    const login = screen.getByRole("button", { name: "登入中…" });
    expect((login as HTMLButtonElement).disabled).toBe(true);
  });

  /// `busy` 是別的動作（登出）在跑。那時鎖住登入是對的，但不該冒出一個
  /// 取消登入的按鈕 —— 根本沒有登入在跑。
  it("其他動作進行中時登入鎖住，也沒有取消鈕", () => {
    render(<SignIn {...props} busy />);

    expect(screen.queryByRole("button", { name: "取消登入" })).toBeNull();
    const login = screen.getByRole("button", { name: "登入" });
    expect((login as HTMLButtonElement).disabled).toBe(true);
  });

  it("憑證被拒絕時標題改成重新登入", () => {
    render(<SignIn {...props} needsLogin />);

    expect(screen.getByRole("heading", { name: "重新登入" })).toBeTruthy();
  });
});
