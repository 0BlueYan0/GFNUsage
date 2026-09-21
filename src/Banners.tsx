import { daysUntil, formatCountdown } from "./format";

/** 憑證剩這麼多天以內就開始提醒（spec §4.4）。 */
const WARN_WITHIN_DAYS = 7;

export default function Banners({
  clientTokenExpiresAt,
  showTrayHint,
  updateVersion,
  busy,
  loggingIn,
  onDismissHint,
  onInstallUpdate,
  onDismissUpdate,
  onLogin,
  onCancelLogin,
}: {
  clientTokenExpiresAt: string | null;
  showTrayHint: boolean;
  /// 有新版本可以裝。登入畫面不給，那裡還沒有資格談更新。
  updateVersion?: string | null;
  busy: boolean;
  loggingIn?: boolean;
  onDismissHint: () => void;
  onInstallUpdate?: () => void;
  onDismissUpdate?: () => void;
  /// 給了才畫重新登入鈕。登入畫面不給。
  onLogin?: () => void;
  onCancelLogin?: () => void;
}) {
  // 不知道到期時刻就別講。里程碑 1／2 存下的憑證會是 null，硬要顯示
  // 只會變成「憑證今天到期」這種既嚇人又不準的話。
  const days = clientTokenExpiresAt ? daysUntil(clientTokenExpiresAt) : null;
  const expiring = days !== null && days <= WARN_WITHIN_DAYS;
  // 剩不到一天時「再 0 天到期」等於沒講，所以倒數改用會自己縮小精度的版本。
  const expired =
    clientTokenExpiresAt !== null &&
    new Date(clientTokenExpiresAt).getTime() <= Date.now();

  return (
    <>
      {/* 這一條在 `.panel__body` 裡，而那一層是 overflow-y: auto，
          擠不到底下的 `.actions`。 */}
      {updateVersion && onInstallUpdate && (
        <div className="hint">
          <p>有新版本 {updateVersion}</p>
          <button className="link" disabled={busy} onClick={onInstallUpdate}>
            更新
          </button>
          {onDismissUpdate && (
            <button className="link" disabled={busy} onClick={onDismissUpdate}>
              知道了
            </button>
          )}
        </div>
      )}

      {showTrayHint && (
        <div className="hint">
          <p>
            點工作列的 <code>^</code>，把 GFNUsage 拖出來釘住。
          </p>
          <button className="link" disabled={busy} onClick={onDismissHint}>
            知道了
          </button>
        </div>
      )}

      {expiring && clientTokenExpiresAt && (
        <div className="warn">
          <p>
            {expired
              ? "登入已過期"
              : `登入 ${formatCountdown(clientTokenExpiresAt)}後到期`}
          </p>
          {/* 主面板上唯一能重新登入的地方。那裡的「登出」意思相反，不能兼差。
              登入畫面不給這顆：那個畫面底下就有登入鈕。 */}
          {loggingIn
            ? // 取消不看 `busy`：登入卡住時這是唯一的出口，理由同登入畫面。
              // 登入途中這裡不留重新登入鈕，再按一次只會多綁一個埠。
              onCancelLogin && (
                <button className="link" onClick={onCancelLogin}>
                  取消登入
                </button>
              )
            : onLogin && (
                <button className="link" disabled={busy} onClick={onLogin}>
                  重新登入
                </button>
              )}
        </div>
      )}
    </>
  );
}
