import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import ScheduleForm from "./Schedule";
import type { Schedule } from "./types";

const empty: Schedule = { weekly: [], exceptions: [] };

const workdays: Schedule = {
  weekly: [
    {
      weekdays: [0, 1, 2, 3, 4],
      startMinute: 540,
      endMinute: 1080,
      note: "上班",
    },
  ],
  exceptions: [],
};

describe("ScheduleForm", () => {
  it("沒有任何時段時說明預設行為", () => {
    render(
      <ScheduleForm value={empty} busy={false} onSave={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByText(/還沒有設定/)).toBeTruthy();
  });

  it("列出既有的每週時段", () => {
    render(
      <ScheduleForm value={workdays} busy={false} onSave={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByDisplayValue("09:00")).toBeTruthy();
    expect(screen.getByDisplayValue("18:00")).toBeTruthy();
    expect(screen.getByDisplayValue("上班")).toBeTruthy();
  });

  it("新增時段會給一個可用的預設值", () => {
    render(
      <ScheduleForm value={empty} busy={false} onSave={vi.fn()} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "新增每週時段" }));
    expect(screen.getByDisplayValue("00:00")).toBeTruthy();
    expect(screen.getByDisplayValue("07:00")).toBeTruthy();
  });

  it("刪掉時段就從清單消失", () => {
    render(
      <ScheduleForm value={workdays} busy={false} onSave={vi.fn()} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "刪除這個時段" }));
    expect(screen.queryByDisplayValue("上班")).toBeNull();
  });

  it("沒有選任何星期時擋下存檔", () => {
    const onSave = vi.fn();
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onClose={vi.fn()} />,
    );
    for (const day of ["一", "二", "三", "四", "五"]) {
      fireEvent.click(screen.getByRole("button", { name: `星期${day}` }));
    }
    fireEvent.click(screen.getByRole("button", { name: "儲存" }));
    expect(screen.getByText(/至少要選一個星期/)).toBeTruthy();
    expect(onSave).not.toHaveBeenCalled();
  });

  it("儲存時把時間換算回分鐘數送出", () => {
    const onSave = vi.fn();
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "儲存" }));
    expect(onSave).toHaveBeenCalledWith(workdays);
  });

  it("跨日的時段是合法的", () => {
    const overnight: Schedule = {
      weekly: [{ weekdays: [4], startMinute: 1320, endMinute: 120, note: "夜班" }],
      exceptions: [],
    };
    const onSave = vi.fn();
    render(
      <ScheduleForm value={overnight} busy={false} onSave={onSave} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "儲存" }));
    expect(onSave).toHaveBeenCalledWith(overnight);
  });

  it("結束日期早於開始日期時擋下存檔", () => {
    const backwards: Schedule = {
      weekly: [],
      exceptions: [
        {
          startDate: "2026-10-05",
          endDate: "2026-10-01",
          kind: "blocked",
          note: "",
        },
      ],
    };
    render(
      <ScheduleForm value={backwards} busy={false} onSave={vi.fn()} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "儲存" }));
    expect(screen.getByText(/結束日期早於開始日期/)).toBeTruthy();
  });
});
