import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import "./App.css";
import Banners from "./Banners";
import SignIn from "./SignIn";
import {
  formatCountdown,
  formatDuration,
  formatResetAt,
  percentOf,
  splitDuration,
} from "./format";
import Pace from "./Pace";
import ScheduleForm from "./Schedule";
import type {
  DisplayState,
  Metric,
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
  metric,
}: {
  snapshot: QuotaSnapshot;
  state: DisplayState;
  pace: PaceReport | null;
  metric: Metric;
}) {
  if (!snapshot.timeCapped) {
    return <p className="note">沒有月時數上限</p>;
  }

  // 大字與進度條看同一個數字。各看各的話，數字往下掉而長條往上長。
  const value =
    metric === "used" ? snapshot.usedMinutes : snapshot.remainingMinutes;
  const percent = percentOf(value, snapshot.totalMinutes);
  // 主要數字只放小時，餘數的分鐘跟在小字那一行。「103 小時 5 分鐘」整串
  // 用 44px 排，360px 寬的面板放不下。
  const shown = splitDuration(value);

  return (
    <>
      <div className="hero">
        <span className={modifier("hero__value", state)}>
          {shown.hours > 0 ? shown.hours : shown.mins}
        </span>
        <span className="hero__unit">
          {shown.hours > 0 ? "小時" : "分鐘"}
          {shown.hours > 0 && shown.mins > 0 && ` ${shown.mins} 分`}
          {" / "}
          {formatDuration(snapshot.totalMinutes)}
        </span>
      </div>

      <div
        className="meter"
        role="progressbar"
        aria-valuenow={percent}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={metric === "used" ? "已使用的月配額" : "剩餘的月配額"}
      >
        <div
          className={modifier("meter__fill", state)}
          style={{ width: `${percent}%` }}
        />
      </div>

      <dl className="facts">
        {snapshot.spanEnd && (
          <div className="facts__row">
            <dt>重置</dt>
            {/* 倒數不加條件：舊寫法在剩不到一天時會整句消失，
                而那正是最該顯示的時候。 */}
            <dd title={formatResetAt(snapshot.spanEnd)}>
              {formatResetAt(snapshot.spanEnd)}，還有{" "}
              {formatCountdown(snapshot.spanEnd)}
            </dd>
          </div>
        )}
        {snapshot.rolledOverMinutes > 0 && (
          <div className="facts__row">
            <dt>上期未用完</dt>
            {/* 帶上限：15 / 15 看得出撞到天花板了，8 / 15 看得出還有空間。 */}
            <dd>
              {formatDuration(snapshot.rolledOverMinutes)} /{" "}
              {formatDuration(snapshot.rolloverCapMinutes)}
            </dd>
          </div>
        )}
        {snapshot.purchasedMinutes > 0 && (
          <div className="facts__row">
            <dt>加購</dt>
            <dd>{formatDuration(snapshot.purchasedMinutes)}</dd>
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

export default function App() {
  const [data, setData] = useState<PanelData | null>(null);
  const [busy, setBusy] = useState(false);
  // 不是 null 就代表正在看設定畫面。開啟時才去讀設定，不必每次開面板都讀。
  const [schedule, setSchedule] = useState<Schedule | null>(null);
  // 匯入之後用它強迫設定表單重新掛載，丟掉已經過期的草稿。
  const [formKey, setFormKey] = useState(0);
  // 按下登入到後端確認之間的樂觀旗標，純粹為了當場就有反應。
  // 「到底有沒有登入在跑」以後端的 `loginPending` 為準 —— 開瀏覽器會把
  // 面板收起來，本地旗標撐不過那一下。
  const [starting, setStarting] = useState(false);

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
   * 失敗往外丟，由呼叫端決定怎麼講。設定的匯出與匯入需要這個 —— 它們的
   * 錯誤不會寫進 `last_error`，吞掉就等於整個消失。
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

  // 匯入、登出、立即更新失敗時，後端都會把錯誤寫進 state，
  // `run()` 的 load() 會取回來顯示，所以這裡吞掉就好。
  const runQuietly = (task: () => Promise<unknown>) => {
    void run(task).catch(() => {});
  };

  /**
   * 登入自己一條路，不走 `run()`。
   *
   * 差別只有一個但很要緊：期間**不**鎖住其他按鈕。登入要等使用者在
   * 瀏覽器裡操作，可能五分鐘，也可能他關掉分頁就再也不回來了 ——
   * 那時底下的匯入正是出口，不能跟著一起被鎖住。
   */
  const login = () => {
    setStarting(true);
    // 立刻回讀一次，把「登入進行中」換成後端那個說了算的版本。
    void load();
    void invoke("start_login")
      // 成功、失敗、取消，錯誤都已經由後端決定要不要寫進 `last_error`。
      .catch(() => {})
      .finally(() => {
        setStarting(false);
        void load();
      });
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
  //
  // 參數決定到期橫幅要不要附一顆重新登入鈕。登入畫面底下本來就有登入鈕，
  // 再附一顆是同一個動作出現兩次。
  const banners = (withLogin: boolean) => (
    <Banners
      clientTokenExpiresAt={data.clientTokenExpiresAt}
      showTrayHint={data.showTrayHint}
      busy={busy}
      loggingIn={starting || data.loginPending}
      onDismissHint={() => runQuietly(() => invoke("dismiss_tray_hint"))}
      onLogin={withLogin ? login : undefined}
      onCancelLogin={
        withLogin ? () => void invoke("cancel_login_command") : undefined
      }
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
        metric={data.metric}
        onMetric={(next) => run(() => invoke("set_metric", { metric: next }))}
        // 自動儲存，所以不走 `run()`：它會把按鈕鎖起來，打字時一路閃。
        // 也不關掉表單 —— 存檔不再是離開的動作了。
        // 不吞錯誤：reject 會被表單接住，顯示在匯出入鍵上方。
        onSave={async (next) => {
          await invoke("set_schedule", { schedule: next });
          await load();
        }}
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
        loggingIn={starting || data.loginPending}
        error={data.lastError}
        needsLogin={data.needsLogin}
        banners={banners(false)}
        onLogin={login}
        onCancelLogin={() => void invoke("cancel_login_command")}
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

      {banners(true)}

      {snapshot ? (
        <Quota
          snapshot={snapshot}
          state={data.state}
          pace={data.pace}
          metric={data.metric}
        />
      ) : (
        <p className="note">沒有資料</p>
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
            // 讀不到設定就不開表單。拿一份空設定進去，使用者一動它就
            // 自動存了出去，原本的設定當場清空。
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
          登出
        </button>
      </div>
    </div>
  );
}
