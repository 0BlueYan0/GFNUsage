import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ProgressButton from "./ProgressButton";

describe("ProgressButton", () => {
  it("下載中顯示百分比，底色填到同一個位置", () => {
    render(<ProgressButton progress={{ kind: "downloading", value: 42 }} />);

    const button = screen.getByRole("button", { name: "42%" });
    expect(button.style.getPropertyValue("--progress")).toBe("42%");
    expect((button as HTMLButtonElement).disabled).toBe(true);
  });

  /// 伺服器沒給檔案大小時算不出百分比，寫 0% 會像是卡住。
  it("不知道檔案大小時不寫百分比", () => {
    render(<ProgressButton progress={{ kind: "downloading", value: null }} />);

    expect(screen.getByRole("button", { name: "下載中…" })).toBeTruthy();
  });

  it("按下去還沒收到進度時是下載中", () => {
    render(<ProgressButton progress={null} />);

    expect(screen.getByRole("button", { name: "下載中…" })).toBeTruthy();
  });

  it("下載完是安裝中，底色填滿", () => {
    render(<ProgressButton progress={{ kind: "installing" }} />);

    const button = screen.getByRole("button", { name: "安裝中…" });
    expect(button.style.getPropertyValue("--progress")).toBe("100%");
  });
});
