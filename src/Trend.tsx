import { modifier } from "./format";
import type { DailyPoint, DisplayState, Metric } from "./types";

/** 畫布尺寸。面板 360 寬扣掉左右各 16px 的 padding 剩 328。 */
const WIDTH = 328;
const HEIGHT = 72;
/** 右邊留 2px，2px 的線才不會被畫布邊緣切掉一半。 */
const INSET_X = 2;
/** 上下各留 8px 放刻度的字，置中對齊時字才不會被切到。 */
const INSET_Y = 8;
/** 左邊讓給刻度的字。「100%」在 11px 下約 28px 寬，再加一點間距。 */
const GUTTER = 34;
/** 今天那一點的半徑。 */
const TODAY_RADIUS = 3;
/** 縱軸刻度，由上而下。 */
const TICKS = [1, 0.5, 0];

/**
 * 本期用量走勢。
 *
 * Y 軸跟著 `metric` 翻：看剩餘就往下走，看已使用就往上長。兩種模式共用
 * 同一個 0 到 T 的範圍，不自動縮放 —— 縮放的話切換 metric 會讓線變形，
 * 看起來像換了一份資料。
 *
 * 實線是已經發生的，虛線是用真實燃燒率往後推的，兩條在今天交在同一個值。
 * 分辨兩條靠虛實與今天那一點的左右，不靠顏色：深色模式下 `--accent` 與
 * `--fg-muted` 的明度幾乎相同（0.713 對 0.714），色盲模式下 tritan 的
 * 色差只有 5.3，顏色本身分不出來。
 *
 * 實線的顏色跟著狀態走，與進度條同一組修飾詞。這張圖就是進度條的時間軸，
 * 進度條紅了而線還是綠的會互相矛盾。
 */
export default function Trend({
  daily,
  metric,
  totalMinutes,
  state,
}: {
  daily: DailyPoint[];
  metric: Metric;
  totalMinutes: number;
  state: DisplayState;
}) {
  // 沒有資料就什麼都不畫，不畫空框。後端已經決定哪些情況吐空陣列
  // （免費方案、本期已結束、還沒抓到逐場紀錄），這裡不再判一次。
  // 沿用 `Pace` 那條「缺哪一項就不畫哪一列」。
  if (daily.length < 2 || totalMinutes <= 0) return null;

  const x = (index: number) =>
    GUTTER + (index / (daily.length - 1)) * (WIDTH - GUTTER - INSET_X);

  const at = (ratio: number) =>
    HEIGHT - INSET_Y - ratio * (HEIGHT - INSET_Y * 2);

  const y = (used: number) => {
    const shown = metric === "used" ? used : totalMinutes - used;
    return at(Math.min(Math.max(shown / totalMinutes, 0), 1));
  };

  const line = (pick: (point: DailyPoint) => number | null) => {
    const parts: string[] = [];
    for (const [index, point] of daily.entries()) {
      const used = pick(point);
      if (used === null) continue;
      parts.push(`${x(index).toFixed(1)},${y(used).toFixed(1)}`);
      // 額度用完之後不再往前畫。畫下去會沿著邊緣走成一條平的，看起來像
      // 「後來不玩了」，而實際上是那天之後就沒得玩了。
      if (used >= totalMinutes) break;
    }
    return parts.join(" ");
  };

  const actual = line((point) => point.usedMinutes);
  const projected = line((point) => point.projectedUsedMinutes);

  // 今天是最後一個有實線值的點。標出來有兩個作用：實線與虛線的交界看得見，
  // 而本期第一天只有一個實線的點，沒有這一點的話那天整條線是空的。
  const todayIndex = daily.reduce(
    (found, point, index) => (point.usedMinutes === null ? found : index),
    -1,
  );
  const today = daily[todayIndex]?.usedMinutes ?? null;

  return (
    <svg
      className={modifier("trend", state)}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      role="img"
      aria-label="本期用量走勢"
    >
      {/* 刻度是佔月額度的百分比，跟著 metric 走：看剩餘時 0% 是用完，
          看已使用時 100% 是用完。分母就是大字旁邊那個「/ 115 小時」。
          沒有刻度的話線只是浮在一個空框裡，看得出往下走，看不出走到哪。 */}
      {TICKS.map((ratio) => (
        <g key={ratio}>
          <line
            className="trend__grid"
            x1={GUTTER}
            x2={WIDTH - INSET_X}
            y1={at(ratio)}
            y2={at(ratio)}
          />
          <text
            className="trend__tick"
            x={GUTTER - 6}
            y={at(ratio)}
            textAnchor="end"
            dominantBaseline="middle"
          >
            {Math.round(ratio * 100)}%
          </text>
        </g>
      ))}
      {projected && <polyline className="trend__projected" points={projected} />}
      {actual && <polyline className="trend__actual" points={actual} />}
      {today !== null && (
        <circle
          className="trend__today"
          cx={x(todayIndex).toFixed(1)}
          cy={y(today).toFixed(1)}
          r={TODAY_RADIUS}
        />
      )}
    </svg>
  );
}
