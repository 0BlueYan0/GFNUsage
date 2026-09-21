import { formatDuration, formatLocalDateTime } from "./format";
import type { PlaySession } from "./types";

/**
 * 最近的遊玩紀錄，獨立一頁。
 *
 * 不併進主面板：主面板是 360×480 的固定尺寸，扣掉配額與配速之後已經沒有餘裕，
 * 而這份清單的長度跟著玩了幾款走，塞進去就是把底下那排按鈕擠出視窗。
 * 走 `.panel--scroll`，清單多長都捲得到。
 */
export default function Sessions({
  sessions,
  onClose,
}: {
  sessions: PlaySession[];
  onClose: () => void;
}) {
  return (
    <div className="panel panel--scroll panel--pinned">
      {/* 清單長到要捲時，標題與返回不能跟著捲走 —— 捲到底想回上一頁，
          不該先一路捲回頂端。 */}
      <header className="panel__header panel__header--sticky">
        <h1 className="panel__title">最近</h1>
        <button className="link" onClick={onClose}>
          返回
        </button>
      </header>

      {sessions.length === 0 ? (
        <p className="note">沒有紀錄</p>
      ) : (
        <ul className="sessions__list">
          {sessions.map((session) => (
            <li className="sessions__row" key={session.startedAt}>
              {/* 遊戲名稱可能缺席。缺了就讓時間遞補，不要填 gameId。 */}
              {session.gameTitle && (
                <span className="sessions__game">{session.gameTitle}</span>
              )}
              <span className="sessions__when">
                {formatLocalDateTime(session.startedAt)}
              </span>
              <span className="sessions__length">
                {formatDuration(session.minutes)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
