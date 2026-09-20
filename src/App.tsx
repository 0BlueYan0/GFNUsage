import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import "./App.css";
import Banners from "./Banners";
import { daysUntil, formatHours, formatResetAt, percentUsed } from "./format";
import Pace from "./Pace";
import ScheduleForm from "./Schedule";
import type {
  DisplayState,
  PaceReport,
  PanelData,
  QuotaSnapshot,
  Schedule,
} from "./types";

const STATE_BADGE: Record<DisplayState, string | null> = {
  normal: null,
  low: "時數偏低",
  overPace: "超前消耗",
  exhausted: "已用完",
  freeTier: "免費方案",
};

/** 狀態對應的 CSS 修飾詞，normal 不加。 */
function modifier(base: string, state: DisplayState): string {
  return state === "normal" || state === "freeTier"
    ? base
    : `${base} ${base}--${state}`;
}

function Quota({
  snapshot,
  state,
  pace,
}: {
  snapshot: QuotaSnapshot;
  state: DisplayState;
  pace: PaceReport | null;
}) {
  if (!snapshot.timeCapped) {
    return <p className="note">此方案沒有每月時數上限，不需要盯著用量。</p>;
  }

  const used = percentUsed(snapshot.usedMinutes, snapshot.totalMinutes);
  const days = snapshot.spanEnd ? daysUntil(snapshot.spanEnd) : 0;

  return (
    <>
      <div className="hero">
        <span className={modifier("hero__value", state)}>
          {formatHours(snapshot.remainingMinutes)}
        </span>
        <span className="hero__unit">
          / {formatHours(snapshot.totalMinutes)} 小時
        </span>
      </div>

      <div
        className="meter"
        role="progressbar"
        aria-valuenow={used}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label="已使用的月配額"
      >
        <div
          className={modifier("meter__fill", state)}
          style={{ width: `${used}%` }}
        />
      </div>

      <dl className="facts">
        <div className="facts__row">
          <dt>已使用</dt>
          <dd>
            {formatHours(snapshot.usedMinutes)} 小時（{used}%）
          </dd>
        </div>
        {snapshot.spanEnd && (
          <div className="facts__row">
            <dt>重置</dt>
            <dd title={formatResetAt(snapshot.spanEnd)}>
              {formatResetAt(snapshot.spanEnd)}
              {days > 0 ? `，還有 ${days} 天` : ""}
            </dd>
          </div>
        )}
        {snapshot.rolledOverMinutes > 0 && (
          <div className="facts__row">
            <dt>本期含結轉</dt>
            <dd>{formatHours(snapshot.rolledOverMinutes)} 小時</dd>
          </div>
        )}
        {snapshot.purchasedMinutes > 0 && (
          <div className="facts__row">
            <dt>本期含加購</dt>
            <dd>{formatHours(snapshot.purchasedMinutes)} 小時</dd>
          </div>
        )}
      </dl>

      {pace && (
        <Pace
          pace={pace}
          usedMinutes={snapshot.usedMinutes}
          totalMinutes={snapshot.totalMinutes}
        />
      )}
    </>
  );
}

function SignIn({
  busy,
  error,
  needsLogin,
  banners,
  onLogin,
  onImportLocal,
  onImportManual,
}: {
  busy: boolean;
  error: string | null;
  needsLogin: boolean;
  banners: ReactNode;
  onLogin: () => void;
  onImportLocal: () => void;
  onImportManual: (data: string) => void;
}) {
  const [pasted, setPasted] = useState("");

  // 內容比主畫面長，而 `.panel` 是固定高度又切掉溢出的部分。
  return (
    <div className="panel panel--scroll">
      <header className="panel__header">
        <h1 className="panel__title">
          {needsLogin ? "重新連結 NVIDIA 帳號" : "連結 NVIDIA 帳號"}
        </h1>
      </header>

      {banners}

      {needsLogin && (
        <p className="note">
          NVIDIA 不再接受目前的憑證。重新登入一次就好。
        </p>
      )}

      <p className="note">
        用 NVIDIA 帳號登入。瀏覽器會開起來，登入完成後這個面板會自己回來。
      </p>

      <button className="primary" disabled={busy} onClick={onLogin}>
        {busy ? "等待瀏覽器…" : "登入 NVIDIA 帳號"}
      </button>

      <p className="note">
        或者，從這台電腦已安裝的 GeForce NOW 匯入憑證。沒有安裝的話，
        到有安裝的機器上取出 <code>sharedstorage.json</code> 裡
        <code>starfleetSession.data</code> 的值貼到下面。
      </p>

      <button disabled={busy} onClick={onImportLocal}>
        從本機 GeForce NOW 匯入
      </button>

      <textarea
        rows={4}
        value={pasted}
        spellCheck={false}
        placeholder="或貼上 starfleetSession.data 的值"
        onChange={(e) => setPasted(e.target.value)}
      />

      {error && <p className="alert">{error}</p>}

      <div className="actions">
        <button
          disabled={busy || pasted.trim() === ""}
          onClick={() => onImportManual(pasted.trim())}
        >
          使用貼上的憑證
        </button>
      </div>
    </div>
  );
}

export default function App() {
  const [data, setData] = useState<PanelData | null>(null);
  const [busy, setBusy] = useState(false);
  // 不是 null 就代表正在看設定畫面。開啟時才去讀設定，不必每次開面板都讀。
  const [schedule, setSchedule] = useState<Schedule | null>(null);
  // 匯入之後用它強迫設定表單重新掛載，丟掉已經過期的草稿。
  const [formKey, setFormKey] = useState(0);

  const load = useCallback(async () => {
    setData(await invoke<PanelData>("get_snapshot"));
  }, []);

  // 面板開啟時先顯示快取，再在背景抓一次新的。後端會在「需重新登入」時略過，
  // 不會每開一次面板就鑄一顆 token。
  const refreshInBackground = useCallback(async () => {
    try {
      await invoke("refresh_if_due");
    } catch {
      // 錯誤已由後端寫進 state。
    }
    await load();
  }, [load]);

  useEffect(() => {
    void load();
    const onVisible = () => {
      if (document.visibilityState === "visible") {
        void load();
        void refreshInBackground();
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [load, refreshInBackground]);

  /**
   * 跑一個會動到後端狀態的動作：期間鎖住按鈕，結束後一律重新載入面板資料。
   *
   * 失敗往外丟，由呼叫端決定怎麼講。設定的儲存需要這個 —— 它的錯誤
   * 不會寫進 `last_error`，吞掉就等於整個消失。
   */
  const run = async (task: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await task();
    } finally {
      await load();
      setBusy(false);
    }
  };

  // 匯入、解除連結、立即更新失敗時，後端都會把錯誤寫進 state，
  // `run()` 的 load() 會取回來顯示，所以這裡吞掉就好。
  const runQuietly = (task: () => Promise<unknown>) => {
    void run(task).catch(() => {});
  };

  if (!data) {
    return (
      <div className="panel">
        <p className="note">載入中…</p>
      </div>
    );
  }

  // 橫幅在早期 return 之前組好：首次啟動時面板顯示的是登入畫面而不是
  // 主畫面（那時必然還沒有憑證），提示只掛在主面板上等於白做。
  const banners = (
    <Banners
      clientTokenExpiresAt={data.clientTokenExpiresAt}
      showTrayHint={data.showTrayHint}
      busy={busy}
      onDismissHint={() => runQuietly(() => invoke("dismiss_tray_hint"))}
    />
  );

  // 設定畫面排在憑證判斷之前：時段設定與帳號無關，背景輪詢剛好把憑證
  // 判死時，不該把使用者正在填的一整排時段無聲清掉。
  if (schedule) {
    return (
      <ScheduleForm
        // 匯入是在後端換掉設定，表單手上那份草稿隨即過期。換 key 讓它
        // 整個重新掛載，`useState(value)` 才會吃到新的那一份。
        key={formKey}
        value={schedule}
        busy={busy}
        onClose={() => setSchedule(null)}
        onSave={(next) =>
          // 這裡不吞錯誤：reject 會被表單接住，顯示在儲存鍵上方。
          run(async () => {
            await invoke("set_schedule", { schedule: next });
            setSchedule(null);
          })
        }
        onExport={(next) =>
          run(() => invoke("export_schedule", { schedule: next }))
        }
        onImport={() =>
          run(async () => {
            await invoke("import_schedule");
            setSchedule(await invoke<Schedule>("get_schedule"));
            setFormKey((key) => key + 1);
          })
        }
      />
    );
  }

  if (!data.hasCredentials || data.needsLogin) {
    return (
      <SignIn
        busy={busy}
        error={data.lastError}
        needsLogin={data.needsLogin}
        banners={banners}
        onLogin={() => runQuietly(() => invoke("start_login"))}
        onImportLocal={() => runQuietly(() => invoke("import_from_local_gfn"))}
        onImportManual={(value) =>
          runQuietly(() => invoke("import_manual", { data: value }))
        }
      />
    );
  }

  const snapshot = data.snapshot;
  const badge = snapshot ? STATE_BADGE[data.state] : null;

  return (
    <div className="panel">
      <header className="panel__header">
        <h1 className="panel__title">
          GeForce NOW
          {snapshot && <span className="panel__tier"> · {snapshot.tier}</span>}
        </h1>
        {badge && (
          <span className={modifier("badge", data.state)}>{badge}</span>
        )}
      </header>

      {banners}

      {snapshot ? (
        <Quota snapshot={snapshot} state={data.state} pace={data.pace} />
      ) : (
        <p className="note">還沒有資料，按下方的「立即更新」試試。</p>
      )}

      {data.lastError && <p className="alert">{data.lastError}</p>}

      {snapshot && (
        <p className="note">
          資料時間{" "}
          {new Date(snapshot.fetchedAt).toLocaleTimeString("zh-TW", {
            hour: "2-digit",
            minute: "2-digit",
            hour12: false,
          })}
        </p>
      )}

      <div className="actions">
        <button
          className="primary"
          disabled={busy}
          onClick={() => runQuietly(() => invoke("refresh_now"))}
        >
          {busy ? "更新中…" : "立即更新"}
        </button>
        <button
          className="link"
          disabled={busy}
          onClick={() =>
            // 讀不到設定就不開表單。拿一份空設定進去會讓使用者一按儲存
            // 就把原本的設定清空。
            void invoke<Schedule>("get_schedule")
              .then(setSchedule)
              .catch(() => {})
          }
        >
          設定
        </button>
        <button
          className="link"
          disabled={busy}
          onClick={() => runQuietly(() => invoke("sign_out"))}
        >
          解除連結
        </button>
      </div>
    </div>
  );
}
