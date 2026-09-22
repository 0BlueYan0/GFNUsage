import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import "./App.css";
import Banners from "./Banners";
import SignIn from "./SignIn";
import {
  formatCountdown,
  formatDuration,
  formatHeroUnit,
  formatForecast,
  formatLocalDateTime,
  formatPace,
  modifier,
  percentOf,
  splitDuration,
} from "./format";
import Pace from "./Pace";
import Trend from "./Trend";
import Sessions from "./Sessions";
import About from "./About";
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

  // 配速門檻在進度條上的位置。跟著 metric 走：看已使用時是「該用掉多少」，
  // 看剩餘時是「該剩下多少」，也就是同一條界線的另一邊。
  const expected = pace?.expectedUsedMinutes ?? null;
  const pacePercent =
    expected === null
      ? null
      : percentOf(
          metric === "used" ? expected : snapshot.totalMinutes - expected,
          snapshot.totalMinutes,
        );
  const paceText = pace && formatPace(pace, snapshot.usedMinutes);

  return (
    <>
      <div className="hero">
        <span className={modifier("hero__value", state)}>
          {shown.hours > 0 ? shown.hours : shown.mins}
        </span>
        <span className="hero__unit">
          {formatHeroUnit(value)}
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
        aria-label={
          (metric === "used" ? "已使用的月配額" : "剩餘的月配額") +
          // 讀螢幕的人不會滑過來，而配速那幾句已經不在面板上了。
          (paceText ? `。${paceText.replace(/\n/g, "，")}` : "")
        }
        title={paceText ?? undefined}
      >
        {/* 圓角與裁切在這一層。那條線要凸出去，不能被它裁掉。 */}
        <div className="meter__track">
          <div
            className={modifier("meter__fill", state)}
            style={{ width: `${percent}%` }}
          />
        </div>
        {/* 配速門檻。長條走過它就是超前，這件事本來寫成一列字。
            減 1px 是把 2px 寬的線壓在那個位置上，不是從那裡往右長。 */}
        {pacePercent !== null && (
          <div
            className="meter__pace"
            style={{ left: `calc(${pacePercent}% - 1px)` }}
          />
        )}
      </div>

      <dl className="facts">
        {snapshot.spanEnd && (
          <div className="facts__row">
            <dt>重置</dt>
            {/* 倒數不加條件：舊寫法在剩不到一天時會整句消失，
                而那正是最該顯示的時候。 */}
            <dd title={formatLocalDateTime(snapshot.spanEnd)}>
              {formatLocalDateTime(snapshot.spanEnd)}，還有{" "}
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

      {pace && <Pace pace={pace} />}
    </>
  );
}

/** 後端在面板顯示出來時送的事件。字串要和 `panel::SHOWN_EVENT` 一致。 */
const PANEL_SHOWN = "panel-shown";

interface AboutData {
  version: string | null;
  updateVersion: string | null;
  installing: boolean;
  autostart: boolean | null;
}

/** `update::CheckResult` 的另一半。三種結果在畫面上是三件事。 */
type CheckResult =
  | { kind: "found"; value: string }
  | { kind: "upToDate" }
  | { kind: "failed"; value: string };

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
  // 看不看得到「最近」那一頁。紀錄本身跟著 `PanelData` 一起來，這裡只管畫面。
  const [showSessions, setShowSessions] = useState(false);
  // 關於頁的資料。`null` 代表還沒開過那一頁 —— 版本與開機設定跟額度無關，
  // 沒必要每次開面板都去問。
  const [about, setAbout] = useState<AboutData | null>(null);
  const [aboutError, setAboutError] = useState<string | null>(null);
  // 查完但沒有新版本時要講的那一句。查到的話按鈕自己會變。
  const [aboutNote, setAboutNote] = useState<string | null>(null);
  // 關於頁自己的忙碌旗標，不共用 `busy`。更新檢查會跑到十幾秒（系統 proxy
  // 不通時要等逾時再直連一次），共用的話那段時間主面板的「立即更新」會寫
  // 「更新中…」、五顆按鈕全部按不動 —— 而那裡根本沒有事情在跑。
  const [aboutBusy, setAboutBusy] = useState(false);

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
    // 主要靠這個，不是靠 `visibilitychange`。面板收起來走的是原生視窗的
    // hide，WebView2 的文件一直是 visible，那個事件不會來 —— 沒有這條的話
    // 面板停在啟動當下那一份，系統匣已經有數字了它還寫「沒有資料」。
    // `visibilitychange` 留著：macOS 沒實測過，多一條沒有壞處。
    const shown = listen(PANEL_SHOWN, () => {
      void load();
      void refreshInBackground();
    });
    return () => {
      document.removeEventListener("visibilitychange", onVisible);
      void shown.then((stop) => stop());
    };
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

  /** 問齊關於頁要的三樣。任何一樣讀不到就留 null，那一塊不畫。 */
  const loadAbout = async (): Promise<AboutData> => {
    const [version, update, autostart] = await Promise.all([
      invoke<string>("app_version").catch(() => null),
      invoke<{ version: string | null; installing: boolean }>(
        "get_update_status",
      ).catch(() => null),
      invoke<boolean>("get_autostart").catch(() => null),
    ]);
    return {
      version,
      updateVersion: update?.version ?? null,
      installing: update?.installing ?? false,
      autostart,
    };
  };

  const openAbout = async () => {
    setAboutError(null);
    setAboutNote(null);
    setAbout(await loadAbout());
  };

  /**
   * 關於頁的動作。錯誤留在那一頁，不寫進 `last_error` —— 那一格講的是額度，
   * 混進「開機啟動設不起來」只會讓兩件事都看不懂。
   */
  const runAbout = async (task: () => Promise<unknown>) => {
    setAboutBusy(true);
    setAboutError(null);
    setAboutNote(null);
    try {
      await task();
    } catch (reason) {
      setAboutError(typeof reason === "string" ? reason : String(reason));
    } finally {
      setAbout(await loadAbout());
      setAboutBusy(false);
    }
  };

  /**
   * 手動檢查更新。
   *
   * 後端不再把結果吞掉：查不到和查失敗在畫面上要是兩件事。查到的話什麼都
   * 不寫，底下那顆按鈕會變成「更新到 x.y.z」。
   */
  const checkUpdate = () =>
    runAbout(async () => {
      const result = await invoke<CheckResult>("check_update_now");
      if (result.kind === "failed") throw result.value;
      if (result.kind === "upToDate") setAboutNote("已是最新版本");
    });

  // 匯入、登出、立即更新失敗時，後端都會把錯誤寫進 state，
  // `run()` 的 load() 會取回來顯示，所以這裡吞掉就好。
  const runQuietly = (task: () => Promise<unknown>) => {
    void run(task).catch(() => {});
  };

  /**
   * 登入自己一條路，不走 `run()`。
   *
   * 差別只有一個但很要緊：期間**不**鎖住取消那一顆。登入要等使用者在
   * 登入視窗裡操作，可能五分鐘 —— 取消是那時唯一的出口。
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
      // `withLogin` 為真的那一次就是主面板（登入畫面走 false）。
      // 登入畫面不談更新：那時使用者要做的只有一件事。
      updateVersion={withLogin ? data.updateVersion : null}
      busy={busy}
      loggingIn={starting || data.loginPending}
      onDismissHint={() => runQuietly(() => invoke("dismiss_tray_hint"))}
      onInstallUpdate={() => runQuietly(() => invoke("install_update"))}
      onDismissUpdate={() =>
        runQuietly(() =>
          invoke("dismiss_update", { version: data.updateVersion }),
        )
      }
      onLogin={withLogin ? login : undefined}
      onCancelLogin={
        withLogin ? () => void invoke("cancel_login_command") : undefined
      }
    />
  );

  // 關於頁排在登入判斷之前：版本號與開機啟動跟帳號無關，登入過期時
  // 不該把使用者從這一頁踢走。
  if (about) {
    return (
      <About
        version={about.version}
        updateVersion={about.updateVersion}
        installing={about.installing}
        autostart={about.autostart}
        busy={aboutBusy}
        note={aboutNote}
        error={aboutError}
        onClose={() => {
          setAbout(null);
          setAboutError(null);
          setAboutNote(null);
        }}
        onCheckUpdate={() => void checkUpdate()}
        onInstallUpdate={() => void runAbout(() => invoke("install_update"))}
        onAutostart={(enabled) =>
          void runAbout(() => invoke("set_autostart", { enabled }))
        }
      />
    );
  }

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
      />
    );
  }

  // 排在登入判斷之後：沒有登入就沒有紀錄，那時該看到的是登入畫面。
  if (showSessions) {
    return (
      <Sessions
        sessions={data.recentSessions}
        onClose={() => setShowSessions(false)}
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

      {/* 中間這一段可捲，`.actions` 釘在底下。面板是 360×480 的固定尺寸，
          沒有這層包裝的話，多加任何一區都會把按鈕擠出視窗 —— `.panel` 是
          `overflow: hidden`，擠出去就是不見，使用者沒有出口。 */}
      <div className="panel__body">
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

        {/* 走勢圖排在配速之後。主面板扣掉配額與配速已經沒有餘裕，圖擺在
            上面會把配速那幾列推到捲軸下面。`daily` 是空的就不畫。 */}
        {snapshot && (
          <Trend
            daily={data.daily}
            metric={data.metric}
            totalMinutes={snapshot.totalMinutes}
            state={data.state}
            forecast={
              data.pace && formatForecast(data.pace, snapshot.totalMinutes)
            }
          />
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
      </div>

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
          onClick={() => setShowSessions(true)}
        >
          最近
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
          onClick={() => void openAbout()}
        >
          關於
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
