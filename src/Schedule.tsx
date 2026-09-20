import { useState } from "react";
import type { Schedule, ScheduleException, WeeklyWindow } from "./types";

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

/**
 * 分鐘數轉 `<input type="time">` 吃的 HH:MM。
 *
 * 1440 顯示為 24:00 會被瀏覽器拒絕，所以收斂回 00:00 —— 整天的時段
 * 在資料上是 `0 → 1440`，畫面上就是 00:00 到 00:00。
 */
function toTimeValue(minutes: number): string {
  const wrapped = minutes % 1440;
  const h = Math.floor(wrapped / 60);
  const m = wrapped % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

function fromTimeValue(value: string): number {
  const [h, m] = value.split(":").map(Number);
  return (h || 0) * 60 + (m || 0);
}

/** 與 Rust 端 `Schedule::validate()` 同一套規則，錯誤訊息也一致。 */
function validate(schedule: Schedule): string | null {
  for (const window of schedule.weekly) {
    if (window.weekdays.length === 0) return "每週時段至少要選一個星期";
    if (window.startMinute === window.endMinute)
      return "時段的起訖時間不能相同";
  }
  for (const exception of schedule.exceptions) {
    if (exception.endDate < exception.startDate)
      return "例外的結束日期早於開始日期";
  }
  return null;
}

/**
 * 今天的本地日期 YYYY-MM-DD。
 *
 * 不能用 `toISOString().slice(0, 10)` —— 那是 UTC 日期，台北早上八點以前
 * 會給出昨天，使用者新增例外時預設就錯一天。
 */
function todayISO(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

/**
 * 不可遊玩時段的設定表單。
 *
 * 受控元件：改動只留在自己的 state，按下「儲存」才交給父層寫檔，
 * 使用者改到一半關掉面板不會留下半套設定。
 */
export default function ScheduleForm({
  value,
  busy,
  onSave,
  onClose,
}: {
  value: Schedule;
  busy: boolean;
  onSave: (schedule: Schedule) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<Schedule>(value);
  const [error, setError] = useState<string | null>(null);

  const patchWindow = (index: number, patch: Partial<WeeklyWindow>) =>
    setDraft((d) => ({
      ...d,
      weekly: d.weekly.map((w, i) => (i === index ? { ...w, ...patch } : w)),
    }));

  const patchException = (index: number, patch: Partial<ScheduleException>) =>
    setDraft((d) => ({
      ...d,
      exceptions: d.exceptions.map((e, i) =>
        i === index ? { ...e, ...patch } : e,
      ),
    }));

  const toggleDay = (index: number, day: number) => {
    const current = draft.weekly[index].weekdays;
    patchWindow(index, {
      weekdays: current.includes(day)
        ? current.filter((d) => d !== day)
        : [...current, day].sort((a, b) => a - b),
    });
  };

  const save = () => {
    const problem = validate(draft);
    setError(problem);
    if (!problem) onSave(draft);
  };

  const empty = draft.weekly.length === 0 && draft.exceptions.length === 0;

  return (
    <div className="panel panel--scroll">
      <header className="panel__header">
        <h1 className="panel__title">不可遊玩時段</h1>
        <button className="link" onClick={onClose}>
          返回
        </button>
      </header>

      <p className="note">
        睡覺、上班這些不可能遊玩的時間。設定之後，配速與預測改用「可遊玩時間」
        當分母，週末的正常遊玩就不會被誤判成超支。
      </p>

      {empty && <p className="note">還沒有設定，目前以全天可遊玩計算。</p>}

      {draft.weekly.map((window, index) => (
        <fieldset className="window" key={index}>
          <div className="window__days">
            {WEEKDAYS.map((label, day) => (
              <button
                key={day}
                type="button"
                aria-label={`星期${label}`}
                aria-pressed={window.weekdays.includes(day)}
                className={
                  window.weekdays.includes(day) ? "chip chip--on" : "chip"
                }
                onClick={() => toggleDay(index, day)}
              >
                {label}
              </button>
            ))}
          </div>

          <div className="window__times">
            <input
              type="time"
              aria-label="開始時間"
              value={toTimeValue(window.startMinute)}
              onChange={(e) =>
                patchWindow(index, {
                  startMinute: fromTimeValue(e.target.value),
                })
              }
            />
            <span>–</span>
            <input
              type="time"
              aria-label="結束時間"
              value={toTimeValue(window.endMinute)}
              onChange={(e) =>
                patchWindow(index, { endMinute: fromTimeValue(e.target.value) })
              }
            />
            {window.endMinute <= window.startMinute && (
              <span className="note">跨日</span>
            )}
          </div>

          <div className="window__foot">
            <input
              type="text"
              aria-label="標籤"
              placeholder="標籤，例如上班"
              value={window.note}
              onChange={(e) => patchWindow(index, { note: e.target.value })}
            />
            <button
              className="link"
              aria-label="刪除這個時段"
              onClick={() =>
                setDraft((d) => ({
                  ...d,
                  weekly: d.weekly.filter((_, i) => i !== index),
                }))
              }
            >
              刪除
            </button>
          </div>
        </fieldset>
      ))}

      <button
        onClick={() =>
          setDraft((d) => ({
            ...d,
            weekly: [
              ...d.weekly,
              {
                weekdays: [0, 1, 2, 3, 4, 5, 6],
                startMinute: 0,
                endMinute: 420,
                note: "",
              },
            ],
          }))
        }
      >
        新增每週時段
      </button>

      {draft.exceptions.map((exception, index) => (
        <fieldset className="window" key={`e${index}`}>
          <div className="window__times">
            <input
              type="date"
              aria-label="開始日期"
              value={exception.startDate}
              onChange={(e) =>
                patchException(index, { startDate: e.target.value })
              }
            />
            <span>–</span>
            <input
              type="date"
              aria-label="結束日期"
              value={exception.endDate}
              onChange={(e) =>
                patchException(index, { endDate: e.target.value })
              }
            />
          </div>

          <div className="window__foot">
            <select
              aria-label="例外種類"
              value={exception.kind}
              onChange={(e) =>
                patchException(index, {
                  kind: e.target.value === "free" ? "free" : "blocked",
                })
              }
            >
              <option value="blocked">不可遊玩</option>
              <option value="free">全天可遊玩</option>
            </select>
            <input
              type="text"
              aria-label="例外標籤"
              placeholder="標籤，例如出差"
              value={exception.note}
              onChange={(e) => patchException(index, { note: e.target.value })}
            />
            <button
              className="link"
              aria-label="刪除這個例外"
              onClick={() =>
                setDraft((d) => ({
                  ...d,
                  exceptions: d.exceptions.filter((_, i) => i !== index),
                }))
              }
            >
              刪除
            </button>
          </div>
        </fieldset>
      ))}

      <button
        onClick={() =>
          setDraft((d) => ({
            ...d,
            exceptions: [
              ...d.exceptions,
              {
                startDate: todayISO(),
                endDate: todayISO(),
                kind: "blocked",
                note: "",
              },
            ],
          }))
        }
      >
        新增一次性例外
      </button>

      <p className="note">
        一次性例外優先於每週時段；同一天有兩個例外時，以下面那個為準。
      </p>

      {error && <p className="alert">{error}</p>}

      <div className="actions">
        <button className="primary" disabled={busy} onClick={save}>
          {busy ? "儲存中…" : "儲存"}
        </button>
      </div>
    </div>
  );
}
