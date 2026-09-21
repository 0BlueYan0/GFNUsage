import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import About from "./About";

function props(overrides: Partial<Parameters<typeof About>[0]> = {}) {
  return {
    version: "0.1.0",
    updateVersion: null,
    installing: false,
    autostart: false,
    busy: false,
    error: null,
    onClose: () => {},
    onCheckUpdate: () => {},
    onInstallUpdate: () => {},
    onAutostart: () => {},
    ...overrides,
  };
}

describe("About", () => {
  it("顯示版本號", () => {
    render(<About {...props()} />);

    expect(screen.getByText("0.1.0")).toBeTruthy();
  });

  /// 這一頁是獨立的，沒有返回就出不去。
  it("返回叫得到 onClose", () => {
    const onClose = vi.fn();
    render(<About {...props({ onClose })} />);

    fireEvent.click(screen.getByRole("button", { name: "返回" }));

    expect(onClose).toHaveBeenCalled();
  });

  it("沒有新版本時給的是檢查的按鈕", () => {
    const onCheckUpdate = vi.fn();
    render(<About {...props({ onCheckUpdate })} />);

    fireEvent.click(screen.getByRole("button", { name: "檢查更新" }));

    expect(onCheckUpdate).toHaveBeenCalled();
  });

  it("有新版本時按鈕直接寫版本號", () => {
    const onInstallUpdate = vi.fn();
    render(<About {...props({ updateVersion: "0.1.1", onInstallUpdate })} />);

    expect(screen.queryByRole("button", { name: "檢查更新" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "更新到 0.1.1" }));

    expect(onInstallUpdate).toHaveBeenCalled();
  });

  /// 安裝途中兩顆按鈕都不該在：一顆會重裝，一顆會蓋掉正在進行的事。
  it("安裝中不給按鈕", () => {
    render(<About {...props({ updateVersion: "0.1.1", installing: true })} />);

    expect(screen.getByText("更新安裝中")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /更新到/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "檢查更新" })).toBeNull();
  });

  it("開機啟動的開關把新的值送出去", () => {
    const onAutostart = vi.fn();
    render(<About {...props({ onAutostart })} />);

    fireEvent.click(screen.getByRole("checkbox"));

    expect(onAutostart).toHaveBeenCalledWith(true);
  });

  /// 讀不到現況時畫一個空的開關，等於替使用者做了決定。
  it("讀不到開機啟動就不畫那個開關", () => {
    render(<About {...props({ autostart: null })} />);

    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("錯誤留在這一頁", () => {
    render(<About {...props({ error: "開機自動啟動設不起來" })} />);

    expect(screen.getByText("開機自動啟動設不起來")).toBeTruthy();
  });
});
