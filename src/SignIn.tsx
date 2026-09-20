import { useState, type ReactNode } from "react";

/**
 * 登入畫面。
 *
 * `busy` 與 `loggingIn` 是兩件事，刻意分開：
 * `busy` 是匯入、解除連結這種幾秒就結束的動作，全部鎖住沒問題；
 * `loggingIn` 卻長達五分鐘，而且很常以「使用者把分頁關掉」收場。
 * 兩者混用的話，使用者改變主意之後會對著一整排 disabled 的按鈕乾等，
 * 包括他這時最該按的那一個 —— 底下的匯入。
 */
export default function SignIn({
  busy,
  loggingIn,
  error,
  needsLogin,
  banners,
  onLogin,
  onCancelLogin,
  onImportLocal,
  onImportManual,
}: {
  busy: boolean;
  loggingIn: boolean;
  error: string | null;
  needsLogin: boolean;
  banners: ReactNode;
  onLogin: () => void;
  onCancelLogin: () => void;
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
        <p className="note">NVIDIA 不再接受目前的憑證。重新登入一次就好。</p>
      )}

      <p className="note">
        用 NVIDIA 帳號登入。瀏覽器會開起來，登入完成後這個面板會自己回來。
      </p>

      <div className="buttons">
        <button
          className="primary"
          disabled={busy || loggingIn}
          onClick={onLogin}
        >
          {loggingIn ? "等待瀏覽器…" : "登入 NVIDIA 帳號"}
        </button>
        {/* 取消不跟著 `busy` 走：登入卡住時，這是唯一的出口。 */}
        {loggingIn && (
          <button className="link" onClick={onCancelLogin}>
            取消登入
          </button>
        )}
      </div>

      <p className="note">
        或者，從這台電腦已安裝的 GeForce NOW 匯入憑證。沒有安裝的話，
        到有安裝的機器上取出 <code>sharedstorage.json</code> 裡
        <code>starfleetSession.data</code> 的值貼到下面。
      </p>

      {/* 這兩個不看 `loggingIn`：登入卡住時，這條備援路徑正是出口。
          按下去會順手把那次登入收掉（後端的 `link_account`）。 */}
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
