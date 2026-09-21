import type { ReactNode } from "react";

/**
 * 登入畫面。
 *
 * `busy` 與 `loggingIn` 是兩件事，刻意分開：`busy` 是登出這種幾秒就結束的
 * 動作，鎖住沒問題。`loggingIn` 卻長達五分鐘，而且很常以「使用者把登入視窗
 * 關掉」收場，所以取消那一顆不跟著它走 —— 登入卡住時那是唯一的出口。
 */
export default function SignIn({
  busy,
  loggingIn,
  error,
  needsLogin,
  banners,
  onLogin,
  onCancelLogin,
}: {
  busy: boolean;
  loggingIn: boolean;
  error: string | null;
  needsLogin: boolean;
  banners: ReactNode;
  onLogin: () => void;
  onCancelLogin: () => void;
}) {
  return (
    <div className="panel panel--scroll">
      <header className="panel__header">
        <h1 className="panel__title">
          {needsLogin ? "重新登入" : "登入 NVIDIA 帳號"}
        </h1>
      </header>

      {banners}

      <div className="buttons">
        <button
          className="primary"
          disabled={busy || loggingIn}
          onClick={onLogin}
        >
          {loggingIn ? "登入中…" : "登入"}
        </button>
        {loggingIn && (
          <button className="link" onClick={onCancelLogin}>
            取消登入
          </button>
        )}
      </div>

      {error && <p className="alert">{error}</p>}
    </div>
  );
}
