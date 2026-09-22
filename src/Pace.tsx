import { formatDuration, formatOverPace } from "./format";
import type { PaceNote, PaceReport } from "./types";

const NOTE_TEXT: Record<PaceNote, string> = {
  collecting: "資料累積中",
  insufficient: "還沒有可遊玩時間",
  noTimeLeft: "沒有可遊玩時間了",
};

/**
 * spec §7.3 的今日額度與配速。
 *
 * 缺哪一項就不畫哪一列：後端已經依 §6.5 決定好哪些推算成立，
 * 前端不要自己補算，也不要顯示「—」佔位。
 *
 * 預測不在這裡。那幾句講的就是走勢圖那條虛線走到哪，所以它們搬進了圖的
 * tooltip（`formatForecast`），同一件事不在畫面上講兩次。
 */
export default function Pace({
  pace,
  usedMinutes,
}: {
  pace: PaceReport;
  usedMinutes: number;
}) {
  const over = pace.overPaceMinutes;
  const note = pace.note ? NOTE_TEXT[pace.note] : null;

  return (
    <section className="pace">
      {pace.todayBudgetMinutes !== null && (
        <p className="pace__today">
          今天還能玩 {formatDuration(pace.todayBudgetMinutes)}
        </p>
      )}

      {over !== null && pace.expectedUsedMinutes !== null && (
        <div className="pace__row">
          <span className="pace__label">配速</span>
          <span className="pace__body">
            已用 {formatDuration(usedMinutes)} vs 期望{" "}
            {formatDuration(pace.expectedUsedMinutes)}
            <span
              className={over > 0 ? "pace__flag pace__flag--over" : "pace__flag"}
            >
              {formatOverPace(over)}
            </span>
          </span>
        </div>
      )}

      {note && <p className="note">{note}</p>}
    </section>
  );
}
