import { formatDuration } from "./format";
import type { PaceNote, PaceReport } from "./types";

const NOTE_TEXT: Record<PaceNote, string> = {
  collecting: "資料累積中",
  insufficient: "還沒有可遊玩時間",
  noTimeLeft: "沒有可遊玩時間了",
};

/**
 * spec §7.3 的今日額度。
 *
 * 缺哪一項就不畫哪一列：後端已經依 §6.5 決定好哪些推算成立，
 * 前端不要自己補算，也不要顯示「—」佔位。
 *
 * 配速與預測都不在這裡。配速變成進度條上的那條線（`formatPace`），預測是
 * 走勢圖那條虛線（`formatForecast`），兩邊的字都在各自的 tooltip 裡。
 * 同一件事不在畫面上講兩次。
 */
export default function Pace({ pace }: { pace: PaceReport }) {
  const note = pace.note ? NOTE_TEXT[pace.note] : null;

  return (
    <section className="pace">
      {pace.todayBudgetMinutes !== null && (
        <p className="pace__today">
          今天還能玩 {formatDuration(pace.todayBudgetMinutes)}
        </p>
      )}

      {note && <p className="note">{note}</p>}
    </section>
  );
}
