/**
 * 版本、更新、開機自動啟動，獨立一頁。
 *
 * 不併進主面板：那裡是 360×480 的固定尺寸，扣掉配額與配速已經沒有餘裕。
 * 也不併進「設定」——那一頁講的是不可遊玩時段，跟程式本身是兩件事。
 *
 * 和 `Sessions`、`Schedule` 一樣只收 props：`invoke` 一律由 `App` 呼叫。
 */
export default function About({
  version,
  updateVersion,
  installing,
  autostart,
  busy,
  note,
  error,
  onClose,
  onCheckUpdate,
  onInstallUpdate,
  onAutostart,
}: {
  version: string | null;
  updateVersion: string | null;
  installing: boolean;
  /// 讀不到就是 null。那時不畫開關 —— 畫一個空的，使用者一碰就等於替他
  /// 做了決定。
  autostart: boolean | null;
  busy: boolean;
  /// 查完但沒有新版本時的那一句。找到新版本的話按鈕自己會變成
  /// 「更新到 x.y.z」，那就是回饋，不必再寫一句。
  note: string | null;
  error: string | null;
  onClose: () => void;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
  onAutostart: (enabled: boolean) => void;
}) {
  return (
    <div className="panel panel--scroll panel--pinned">
      <header className="panel__header panel__header--sticky">
        <h1 className="panel__title">關於</h1>
        <button className="link" onClick={onClose}>
          返回
        </button>
      </header>

      <dl className="facts">
        <div className="facts__row">
          <dt>版本</dt>
          <dd>{version ?? "—"}</dd>
        </div>
      </dl>

      {installing ? (
        <p className="note">更新安裝中</p>
      ) : (
        <div className="buttons">
          {updateVersion ? (
            <button className="primary" disabled={busy} onClick={onInstallUpdate}>
              更新到 {updateVersion}
            </button>
          ) : (
            <button disabled={busy} onClick={onCheckUpdate}>
              {busy ? "檢查中…" : "檢查更新"}
            </button>
          )}
        </div>
      )}

      {autostart !== null && (
        <label className="toggle">
          <input
            type="checkbox"
            checked={autostart}
            disabled={busy}
            onChange={(event) => onAutostart(event.target.checked)}
          />
          開機時自動啟動
        </label>
      )}

      {note && <p className="note">{note}</p>}

      {error && <p className="alert">{error}</p>}
    </div>
  );
}
