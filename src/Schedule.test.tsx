import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

// 自動儲存有 debounce，測試得自己把時間推過去。
beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

/** 推過自動儲存的 debounce，並讓它引發的 promise 跑完。 */
async function autosave() {
  await act(async () => {
    vi.advanceTimersByTime(1000);
  });
}

describe("ScheduleForm", () => {
  /// 打開設定就寫一次檔，等於每看一眼就動一次磁碟。
  it("剛打開不會寫出去", async () => {
    const onSave = vi.fn();
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    await autosave();
    expect(onSave).not.toHaveBeenCalled();
  });

  /// 按「返回」離開只有幾十毫秒，等不到 debounce。
  it("返回會把還沒寫的補上", () => {
    const onSave = vi.fn();
    const onClose = vi.fn();
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={onClose} metric="remaining" onMetric={vi.fn()} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "星期六" }));
    fireEvent.click(screen.getByRole("button", { name: "返回" }));

    expect(onSave).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("沒有任何時段時顯示全天可遊玩", () => {
    render(
      <ScheduleForm value={empty} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    expect(screen.getByText("全天可遊玩", { selector: "p" })).toBeTruthy();
  });

  /// 立刻生效，不等「儲存」—— 那顆鍵管的是下面的時段草稿。
  it("選主要數字會立刻回報", () => {
    const onMetric = vi.fn();
    render(
      <ScheduleForm value={empty} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={onMetric} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "已使用" }));

    expect(onMetric).toHaveBeenCalledWith("used");
  });

  it("目前選的那個看得出來", () => {
    render(
      <ScheduleForm value={empty} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="used" onMetric={vi.fn()} />,
    );

    expect(
      screen.getByRole("button", { name: "已使用", pressed: true }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "剩餘", pressed: false }),
    ).toBeTruthy();
  });

  it("列出既有的每週時段", () => {
    render(
      <ScheduleForm value={workdays} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    expect(screen.getByDisplayValue("09:00")).toBeTruthy();
    expect(screen.getByDisplayValue("18:00")).toBeTruthy();
    expect(screen.getByDisplayValue("上班")).toBeTruthy();
  });

  it("新增時段會給一個可用的預設值", () => {
    render(
      <ScheduleForm value={empty} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "新增每週時段" }));
    expect(screen.getByDisplayValue("00:00")).toBeTruthy();
    expect(screen.getByDisplayValue("07:00")).toBeTruthy();
  });

  it("刪掉時段就從清單消失", () => {
    render(
      <ScheduleForm value={workdays} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "刪除這個時段" }));
    expect(screen.queryByDisplayValue("上班")).toBeNull();
  });

  /// 改到一半必然會經過不合法的狀態。那不是錯誤，是還沒改完。
  it("沒有選任何星期時不寫出去", async () => {
    const onSave = vi.fn();
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    for (const day of ["一", "二", "三", "四"]) {
      fireEvent.click(screen.getByRole("button", { name: `星期${day}` }));
    }
    await autosave();
    onSave.mockClear();

    fireEvent.click(screen.getByRole("button", { name: "星期五" }));
    await autosave();

    expect(screen.getByText(/至少要選一個星期/)).toBeTruthy();
    expect(onSave).not.toHaveBeenCalled();
  });

  it("時間輸入換算回分鐘數才寫出去", async () => {
    const onSave = vi.fn();
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );

    fireEvent.change(screen.getByLabelText("開始時間"), {
      target: { value: "10:30" },
    });
    await autosave();

    expect(onSave).toHaveBeenCalledWith({
      ...workdays,
      weekly: [{ ...workdays.weekly[0], startMinute: 630 }],
    });
  });

  /// 標籤是逐字輸入的欄位。每打一個字寫一次檔，一個詞就是五次磁碟寫入。
  it("連續改動只寫一次", async () => {
    const onSave = vi.fn();
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    const start = screen.getByLabelText("開始時間");

    fireEvent.change(start, { target: { value: "10:00" } });
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    fireEvent.change(start, { target: { value: "10:30" } });
    await autosave();

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith({
      ...workdays,
      weekly: [{ ...workdays.weekly[0], startMinute: 630 }],
    });
  });

  it("跨日的時段是合法的", async () => {
    const overnight: Schedule = {
      weekly: [{ weekdays: [4], startMinute: 1320, endMinute: 120, note: "夜班" }],
      exceptions: [],
    };
    const onSave = vi.fn();
    render(
      <ScheduleForm value={overnight} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "星期六" }));
    await autosave();

    expect(onSave).toHaveBeenCalledWith({
      ...overnight,
      weekly: [{ ...overnight.weekly[0], weekdays: [4, 5] }],
    });
  });

  /// 後端擋下來的東西（寫檔失敗、這裡沒鏡射到的規則）一定要看得見，
  /// 不然改了半天是毫無反應。
  it("後端拒絕時把訊息顯示在表單上", async () => {
    const onSave = vi.fn().mockRejectedValue("寫入設定檔失敗：拒絕存取");
    render(
      <ScheduleForm value={workdays} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "星期六" }));
    await autosave();

    expect(screen.getByText(/寫入設定檔失敗/)).toBeTruthy();
  });

  /// 整天是 `0 → 1440`，用時間輸入框會變成 00:00–00:00，看起來就是壞的。
  it("整天的時段顯示成勾選框而不是兩個 00:00", () => {
    const allDay: Schedule = {
      weekly: [{ weekdays: [5, 6], startMinute: 0, endMinute: 1440, note: "" }],
      exceptions: [],
    };
    render(
      <ScheduleForm value={allDay} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    expect(screen.getByRole("checkbox", { name: "整天" })).toBeTruthy();
    expect(screen.queryByLabelText("開始時間")).toBeNull();
  });

  it("勾起整天後寫出 0 到 1440", async () => {
    const onSave = vi.fn();
    render(
      <ScheduleForm value={empty} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "新增每週時段" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "整天" }));
    await autosave();

    expect(onSave).toHaveBeenLastCalledWith({
      weekly: [
        { weekdays: [0, 1, 2, 3, 4, 5, 6], startMinute: 0, endMinute: 1440, note: "" },
      ],
      exceptions: [],
    });
  });

  /// 清空的日期是空字串，`"2026-09-20" < ""` 擋不住，Rust 端則是連命令都進不去。
  it("日期被清空時不寫出去", async () => {
    const blank: Schedule = {
      weekly: [],
      exceptions: [
        { startDate: "", endDate: "2026-10-01", kind: "blocked", note: "" },
      ],
    };
    const onSave = vi.fn();
    render(
      <ScheduleForm value={blank} busy={false} onSave={onSave} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    await autosave();
    expect(screen.getByText(/日期還沒填完/)).toBeTruthy();
    expect(onSave).not.toHaveBeenCalled();
  });

  it("結束日期早於開始日期時不寫出去", async () => {
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
      <ScheduleForm value={backwards} busy={false} onSave={vi.fn()} onExport={vi.fn()} onImport={vi.fn()} onClose={vi.fn()} metric="remaining" onMetric={vi.fn()} />,
    );
    await autosave();
    expect(screen.getByText(/結束日期早於開始日期/)).toBeTruthy();
  });
});

describe("ScheduleForm 的匯出與匯入", () => {
  it("匯出送出的是螢幕上這一份草稿", () => {
    const onExport = vi.fn();
    render(
      <ScheduleForm
        value={workdays}
        busy={false}
        onSave={vi.fn()}
        onExport={onExport}
        onImport={vi.fn()}
        onClose={vi.fn()} metric="remaining" onMetric={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "匯出" }));

    expect(onExport).toHaveBeenCalledWith(workdays);
  });

  /// 匯出的錯誤不會寫進 last_error，吞掉就整個消失了。
  it("匯出被拒絕時把訊息顯示在表單上", async () => {
    render(
      <ScheduleForm
        value={workdays}
        busy={false}
        onSave={vi.fn()}
        onExport={vi.fn().mockRejectedValue("寫入設定檔失敗：拒絕存取")}
        onImport={vi.fn()}
        onClose={vi.fn()} metric="remaining" onMetric={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "匯出" }));
    await act(async () => {});

    expect(screen.getByText(/寫入設定檔失敗/)).toBeTruthy();
  });

  it("匯入被拒絕時把訊息顯示在表單上", async () => {
    render(
      <ScheduleForm
        value={workdays}
        busy={false}
        onSave={vi.fn()}
        onExport={vi.fn()}
        onImport={vi.fn().mockRejectedValue("設定檔格式錯誤：expected value")}
        onClose={vi.fn()} metric="remaining" onMetric={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "匯入" }));

    await act(async () => {});
    expect(screen.getByText(/設定檔格式錯誤/)).toBeTruthy();
  });
});
