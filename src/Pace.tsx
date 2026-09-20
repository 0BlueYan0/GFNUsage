import { formatDuration, formatOverPace, formatResetAt } from "./format";
import type { PaceNote, PaceReport } from "./types";

const NOTE_TEXT: Record<PaceNote, string> = {
  collecting: "資料累積中，可遊玩時間滿 12 小時後才做預測",
  insufficient: "資料不足，本期還沒有可遊玩時間",
  noTimeLeft: "本期已無可遊玩時間",
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
            {pace.runsOutAt ? (
              <span className="pace__runs-out">
                {formatResetAt(pace.runsOutAt)} 用完
              </span>
            ) : (
              // spec §6.5：還沒玩（r = 0）或額度撐得過本期，要把話講白。
              // 「會剩 X」講的是量，沒回答「會不會用完」。
              overshoot <= 0 && (
                <span className="pace__runs-out">以目前速度不會用完</span>
              )
            )}
            {wasted !== null && wasted > 0 && (
              <span className="pace__waste">
                其中約 {formatDuration(wasted)} 超過 15 小時結轉上限，會作廢
              </span>
            )}
          </span>
        </div>
      )}

      {note && <p className="note">{note}</p>}
    </section>
  );
}
