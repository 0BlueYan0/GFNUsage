import { useEffect, useRef, useState } from "react";
import type {
  Metric,
  PollInterval,
  Schedule,
  ScheduleException,
  TaskbarWidget,
  WeeklyWindow,
  WidgetSide,
} from "./types";

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

const MINUTES_PER_DAY = 1440;

/**
 * 自動儲存等這麼久才動手。夠長到一個詞打完只寫一次，短到按「返回」之前
 * 多半已經寫好了 —— 沒寫好的那一份由 `close()` 補上。
 */
const AUTOSAVE_DELAY = 600;

/** 新增時段的預設值：每天 00:00–07:00，也就是睡覺。 */
const DEFAULT_END_MINUTE = 420;

const POLL_INTERVALS: [PollInterval, string][] = [
  ["off", "關閉"],
  ["30m", "30 分鐘"],
  ["1h", "1 小時"],
  ["6h", "6 小時"],
  ["24h", "24 小時"],
];

/**
 * 工作列顯示那一排。「關閉」跟兩邊排在一起，一次點擊就是一個結果。
 *
 * 寫左右，不寫「系統匣」：使用者不一定知道系統匣是哪一塊（2026-09-23）。
 * 標題已經是「工作列顯示」，按鈕裡不再寫一次「工作列」。照工作列上的位置
 * 排，左邊在前。
 */
const WIDGET_CHOICES: ["off" | WidgetSide, string][] = [
  ["off", "關閉"],
  ["taskbarLeft", "左邊"],
  ["trayLeft", "右邊"],
];

/**
 * 分鐘數轉 `<input type="time">` 吃的 HH:MM。
 *
 * 1440 顯示為 24:00 會被瀏覽器拒絕，所以收斂回 00:00。對 `start > 0` 的時段
 * 這樣讀是對的（10:00–00:00 就是玩到午夜），`0 → 1440` 的整天時段則不走
 * 這條路 —— 它由「整天」勾選框表示，見 `isWholeDay()`。
 */
function toTimeValue(minutes: number): string {
  const wrapped = minutes % MINUTES_PER_DAY;
  const h = Math.floor(wrapped / 60);
  const m = wrapped % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

/**
 * 整天的時段在資料上是 `0 → 1440`。
 *
 * 用時間輸入框表示不了：24:00 瀏覽器不收，收斂成 00:00 又會變成
 * `0 → 0`，被兩端的 `validate()` 當成零長度擋掉。所以獨立成一個勾選框。
 */
function isWholeDay(window: WeeklyWindow): boolean {
  return window.startMinute === 0 && window.endMinute === MINUTES_PER_DAY;
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
    // 清空的日期欄位是空字串。`"2026-09-20" < ""` 是 false，靠下面那行擋不住，
    // 送到 Rust 端則是 `NaiveDate` 反序列化失敗 —— 連命令都進不去。
    if (!exception.startDate || !exception.endDate)
      return "例外的日期還沒填完";
    if (exception.endDate < exception.startDate)
      return "例外的結束日期早於開始日期";
  }
  return null;
}

/** Tauri 的 `Result<_, String>` 以字串 reject；其他情況退回字串化。 */
function messageOf(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (reason instanceof Error) return reason.message;
  return String(reason);
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
 * 自動儲存：改完就寫，沒有儲存鍵。草稿還留在自己的 state 裡，因為改到一半
 * 必然會經過不合法的狀態（剛取消最後一個星期、日期還沒填完）—— 那些不寫出去，
 * 等它變合法再寫。
 */
export default function ScheduleForm({
  value,
  busy,
  metric,
  pollInterval,
  taskbarWidget,
  widgetSupported,
  onSave,
  onExport,
  onImport,
  onMetric,
  onPollInterval,
  onTaskbarWidget,
  onClose,
}: {
  value: Schedule;
  busy: boolean;
  metric: Metric;
  pollInterval: PollInterval;
  taskbarWidget: TaskbarWidget;
  widgetSupported: boolean;
  /// 這三個回傳的 promise 被 reject 時，訊息會顯示在表單上。
  onSave: (schedule: Schedule) => Promise<void> | void;
  onExport: (schedule: Schedule) => Promise<void> | void;
  onImport: () => Promise<void> | void;
  onMetric: (metric: Metric) => Promise<void> | void;
  onPollInterval: (interval: PollInterval) => Promise<void> | void;
  onTaskbarWidget: (widget: TaskbarWidget) => Promise<void> | void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<Schedule>(value);
  const [error, setError] = useState<string | null>(null);

  // 最後一次寫出去的那一份。初始值就是檔案裡那一份，所以剛打開設定不會
  // 多寫一次。比對 identity 就夠了：任何一次 `setDraft` 都產生新物件。
  const saved = useRef(value);

  // 父層傳的是行內箭頭函式，每次 render 都是新的。放進相依陣列會讓計時器
  // 每次 render 重置，於是永遠等不到那 600 毫秒。
  const saveRef = useRef(onSave);
  useEffect(() => {
    saveRef.current = onSave;
  });

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

  // 後端也會驗一次，而且擋得到這裡擋不到的東西（寫檔失敗、手改壞的欄位）。
  // 那個錯誤一定要接回來顯示 —— 不然改了半天是毫無反應，使用者只會以為
  // 程式當了，設定其實一個字都沒進去。
  useEffect(() => {
    const problem = validate(draft);
    setError(problem);
    if (problem || draft === saved.current) return;

    // debounce：標籤是逐字輸入的欄位，每打一個字寫一次檔，一個詞就是
    // 五次磁碟寫入加五次 IPC。
    const timer = setTimeout(() => {
      saved.current = draft;
      void Promise.resolve(saveRef.current(draft)).catch((reason) =>
        setError(messageOf(reason)),
      );
    }, AUTOSAVE_DELAY);
    return () => clearTimeout(timer);
  }, [draft]);

  // 按「返回」離開只有幾十毫秒，等不到 debounce。把還沒寫的那一份補寫。
  const close = () => {
    if (draft !== saved.current && !validate(draft)) {
      saved.current = draft;
      void Promise.resolve(saveRef.current(draft)).catch(() => {});
    }
    onClose();
  };

  // 匯出的是螢幕上這一份草稿，不是檔案裡那一份 —— 改到一半按匯出卻拿到
  // 舊設定，沒有人猜得到為什麼。先驗一次，理由同 `save`。
  const exportDraft = () => {
    const problem = validate(draft);
    setError(problem);
    if (problem) return;
    void Promise.resolve(onExport(draft)).catch((reason) =>
      setError(messageOf(reason)),
    );
  };

  const chooseMetric = (next: Metric) => {
    setError(null);
    void Promise.resolve(onMetric(next)).catch((reason) =>
      setError(messageOf(reason)),
    );
  };

  const choosePollInterval = (next: PollInterval) => {
    setError(null);
    void Promise.resolve(onPollInterval(next)).catch((reason) =>
      setError(messageOf(reason)),
    );
  };

  const widgetChoice = taskbarWidget.enabled ? taskbarWidget.side : "off";
  const chooseWidget = (next: "off" | WidgetSide) => {
    setError(null);
    // 關掉時留著原本那一邊，下次打開回到同一邊。
    const widget: TaskbarWidget =
      next === "off"
        ? { ...taskbarWidget, enabled: false }
        : { enabled: true, side: next };
    void Promise.resolve(onTaskbarWidget(widget)).catch((reason) =>
      setError(messageOf(reason)),
    );
  };

  const importFile = () => {
    setError(null);
    void Promise.resolve(onImport()).catch((reason) =>
      setError(messageOf(reason)),
    );
  };

  const empty = draft.weekly.length === 0 && draft.exceptions.length === 0;

  return (
    <div className="panel panel--scroll panel--pinned">
      {/* 和「最近」同一個作法：時段一多這一頁就要捲，捲到底想回上一頁，
          不該先一路捲回頂端。 */}
      <header className="panel__header panel__header--sticky">
        <h1 className="panel__title">設定</h1>
        <button className="link" onClick={close}>
          返回
        </button>
      </header>

      <div className="setting">
        <span id="metric-label">主要數字</span>
        {/* 這個不走底下時段那套 debounce：只有兩個值，點了就該看到結果。 */}
        <div
          className="setting__choice"
          role="group"
          aria-labelledby="metric-label"
        >
          {(["remaining", "used"] as const).map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={metric === option}
              className={metric === option ? "chip chip--on" : "chip"}
              onClick={() => chooseMetric(option)}
            >
              {option === "remaining" ? "剩餘" : "已使用"}
            </button>
          ))}
        </div>
      </div>

      {/* 五個選項排不進「左名稱右選項」那一行，所以名稱自己一行。 */}
      <div className="setting setting--stacked">
        <span id="poll-interval-label">資料更新</span>
        <div
          className="setting__choice"
          role="group"
          aria-labelledby="poll-interval-label"
        >
          {POLL_INTERVALS.map(([option, label]) => (
            <button
              key={option}
              type="button"
              aria-pressed={pollInterval === option}
              className={pollInterval === option ? "chip chip--on" : "chip"}
              onClick={() => choosePollInterval(option)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {widgetSupported && (
        <div className="setting setting--stacked">
          <span id="taskbar-widget-label">工作列顯示</span>
          <div
            className="setting__choice"
            role="group"
            aria-labelledby="taskbar-widget-label"
          >
            {WIDGET_CHOICES.map(([option, label]) => (
              <button
                key={option}
                type="button"
                aria-pressed={widgetChoice === option}
                className={widgetChoice === option ? "chip chip--on" : "chip"}
                onClick={() => chooseWidget(option)}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      )}

      <h2 className="section">不可遊玩時段</h2>

      {empty && <p className="note">全天可遊玩</p>}

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
            <label className="window__allday">
              <input
                type="checkbox"
                checked={isWholeDay(window)}
                onChange={(e) =>
                  patchWindow(
                    index,
                    e.target.checked
                      ? { startMinute: 0, endMinute: MINUTES_PER_DAY }
                      : { startMinute: 0, endMinute: DEFAULT_END_MINUTE },
                  )
                }
              />
              整天
            </label>
            {!isWholeDay(window) && (
              <>
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
                    patchWindow(index, {
                      endMinute: fromTimeValue(e.target.value),
                    })
                  }
                />
                {window.endMinute <= window.startMinute && (
                  <span className="note">跨日</span>
                )}
              </>
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
                endMinute: DEFAULT_END_MINUTE,
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

      <p className="note">例外優先於每週時段</p>

      {error && <p className="alert">{error}</p>}

      <div className="actions">
        <button disabled={busy} onClick={exportDraft}>
          匯出
        </button>
        <button disabled={busy} onClick={importFile}>
          匯入
        </button>
      </div>
    </div>
  );
}
