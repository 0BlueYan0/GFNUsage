import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import "./App.css";
import type { DisplayState, PanelData, QuotaSnapshot } from "./types";
import { daysUntil, formatHours, formatResetAt, percentUsed } from "./types";

const STATE_BADGE: Record<DisplayState, string | null> = {
  normal: null,
  low: "時數偏低",
  exhausted: "已用完",
  freeTier: "免費方案",
};

/** 狀態對應的 CSS 修飾詞，normal 不加。 */
function modifier(base: string, state: DisplayState): string {
  return state === "normal" || state === "freeTier"
    ? base
    : `${base} ${base}--${state}`;
}

function Quota({ snapshot }: { snapshot: QuotaSnapshot }) {
  if (!snapshot.timeCapped) {
    return <p className="note">此方案沒有每月時數上限，不需要盯著用量。</p>;
  }

  const used = percentUsed(snapshot.usedMinutes, snapshot.totalMinutes);
  const days = snapshot.spanEnd ? daysUntil(snapshot.spanEnd) : 0;

  return (
    <>
      <div className="hero">
        <span className={modifier("hero__value", snapshot.state)}>
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
          className={modifier("meter__fill", snapshot.state)}
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
    </>
  );
}

function SignIn({
  busy,
  error,
  needsLogin,
  onImportLocal,
  onImportManual,
}: {
  busy: boolean;
  error: string | null;
  needsLogin: boolean;
  onImportLocal: () => void;
  onImportManual: (data: string) => void;
}) {
  const [pasted, setPasted] = useState("");

  return (
    <div className="panel">
      <header className="panel__header">
        <h1 className="panel__title">
          {needsLogin ? "重新連結 NVIDIA 帳號" : "連結 NVIDIA 帳號"}
        </h1>
      </header>

      {needsLogin && (
        <p className="note">
          NVIDIA 不再接受目前的憑證。先開啟 GeForce NOW 確認能正常登入，再重新匯入。
        </p>
      )}

      <p className="note">
        從這台電腦已安裝的 GeForce NOW 匯入憑證。沒有安裝的話，
        到有安裝的機器上取出 <code>sharedstorage.json</code> 裡
        <code>starfleetSession.data</code> 的值貼到下面。
      </p>

      <button className="primary" disabled={busy} onClick={onImportLocal}>
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

  const run = async (task: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await task();
    } catch {
      // 匯入、解除連結、立即更新失敗時，後端都會把錯誤寫進 state，
      // 下面的 load() 會取回來顯示。
    } finally {
      await load();
      setBusy(false);
    }
  };

  if (!data) {
    return (
      <div className="panel">
        <p className="note">載入中…</p>
      </div>
    );
  }

  if (!data.hasCredentials || data.needsLogin) {
    return (
      <SignIn
        busy={busy}
        error={data.lastError}
        needsLogin={data.needsLogin}
        onImportLocal={() => void run(() => invoke("import_from_local_gfn"))}
        onImportManual={(value) =>
          void run(() => invoke("import_manual", { data: value }))
        }
      />
    );
  }

  const snapshot = data.snapshot;
  const badge = snapshot ? STATE_BADGE[snapshot.state] : null;

  return (
    <div className="panel">
      <header className="panel__header">
        <h1 className="panel__title">
          GeForce NOW
          {snapshot && <span className="panel__tier"> · {snapshot.tier}</span>}
        </h1>
        {badge && (
          <span className={modifier("badge", snapshot!.state)}>{badge}</span>
        )}
      </header>

      {snapshot ? (
        <Quota snapshot={snapshot} />
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
          onClick={() => void run(() => invoke("refresh_now"))}
        >
          {busy ? "更新中…" : "立即更新"}
        </button>
        <button
          className="link"
          disabled={busy}
          onClick={() => void run(() => invoke("sign_out"))}
        >
          解除連結
        </button>
      </div>
    </div>
  );
}
