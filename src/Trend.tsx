import { modifier } from "./format";
import type { DailyPoint, DisplayState, Metric } from "./types";

/** 畫布尺寸。面板 360 寬扣掉左右各 16px 的 padding 剩 328。 */
const WIDTH = 328;
const HEIGHT = 64;
/** 上下左右各留 2px，2px 的線才不會被畫布邊緣切掉一半。 */
const INSET = 2;
/** 今天那一點的半徑。 */
const TODAY_RADIUS = 3;

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
    INSET + (index / (daily.length - 1)) * (WIDTH - INSET * 2);

  const y = (used: number) => {
    const shown = metric === "used" ? used : totalMinutes - used;
    const ratio = Math.min(Math.max(shown / totalMinutes, 0), 1);
    return HEIGHT - INSET - ratio * (HEIGHT - INSET * 2);
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
      {/* 「用完」那條高度。看剩餘時它在底部，看已使用時在頂部，同一條
          運算式兩邊都對。沒有它的話線就只是浮在一個空框裡，看不出離用完
          還有多遠。 */}
      <line
        className="trend__limit"
        x1={0}
        x2={WIDTH}
        y1={y(totalMinutes)}
        y2={y(totalMinutes)}
      />
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
