import { daysUntil, formatCountdown } from "./format";

/** 憑證剩這麼多天以內就開始提醒（spec §4.4）。 */
const WARN_WITHIN_DAYS = 7;

export default function Banners({
  clientTokenExpiresAt,
  showTrayHint,
  busy,
  onDismissHint,
}: {
  clientTokenExpiresAt: string | null;
  showTrayHint: boolean;
  busy: boolean;
  onDismissHint: () => void;
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
      {showTrayHint && (
        <div className="hint">
          <p>
            Windows 11 預設把新圖示收進系統匣的溢位區。點工作列的{" "}
            <code>^</code> 把 GFNUsage 拖出來釘住，剩餘時數就一眼看得到。
          </p>
          <button className="link" disabled={busy} onClick={onDismissHint}>
            知道了
          </button>
        </div>
      )}

      {expiring && clientTokenExpiresAt && (
        <p className="warn">
          {expired
            ? "憑證已過期，要重新登入。"
            : `憑證再 ${formatCountdown(clientTokenExpiresAt)}到期，到期後要重新登入。現在登入一次就會再延 90 天。`}
        </p>
      )}
    </>
  );
}
