import { formatDuration, formatOverPace, formatResetAt } from "./format";
import type { PaceNote, PaceReport } from "./types";

const NOTE_TEXT: Record<PaceNote, string> = {
  collecting: "資料累積中",
  insufficient: "還沒有可遊玩時間",
  noTimeLeft: "沒有可遊玩時間了",
};

/**
 * spec §7.3 的配速與預測兩列。
 *
 * 缺哪一項就不畫哪一列：後端已經依 §6.5 決定好哪些推算成立，
 * 前端不要自己補算，也不要顯示「—」佔位。
 */
export default function Pace({
  pace,
  usedMinutes,
  totalMinutes,
}: {
  pace: PaceReport;
  usedMinutes: number;
  totalMinutes: number;
}) {
  const over = pace.overPaceMinutes;
  const projected = pace.projectedUsedMinutes;
  const overshoot = pace.overshootMinutes;
  const wasted = pace.wastedMinutes;
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

      {projected !== null && overshoot !== null && (
        <div className="pace__row">
          <span className="pace__label">預測</span>
          <span className="pace__body">
            月底約用 {formatDuration(projected)}，
            {overshoot > 0
              ? `超支 ${formatDuration(overshoot)}`
              : `會剩 ${formatDuration(totalMinutes - projected)}`}
            {/* 不補「不會用完」那一句。前一句的「會剩 X」已經回答了。
                spec §6.5 的 r = 0 那一列要求補，那條比「會剩 X」早寫。 */}
            {pace.runsOutAt && (
              <span className="pace__runs-out">
                {formatResetAt(pace.runsOutAt)} 用完
              </span>
            )}
            {wasted !== null && wasted > 0 && (
              <span className="pace__waste">
                {formatDuration(wasted)} 會浪費掉
              </span>
            )}
          </span>
        </div>
      )}

      {note && <p className="note">{note}</p>}
    </section>
  );
}
