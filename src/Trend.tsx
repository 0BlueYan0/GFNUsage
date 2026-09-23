import { modifier } from "./format";
import type { DailyPoint, DisplayState, Metric } from "./types";

/** 畫布尺寸。面板 360 寬扣掉左右各 16px 的 padding 剩 328。 */
const WIDTH = 328;
/**
 * 高度要和 `App.css` 的 `.trend` 一樣。兩邊不同的話 svg 會等比例縮小，
 * 線就碰不到左右兩邊。64 的時候主面板有更新橫幅時會多出 1px，捲軸因此
 * 出現，內容跟著縮 15px。
 */
const HEIGHT = 60;
/** 左右留 2px，2px 的線才不會被畫布邊緣切掉一半。 */
const INSET_X = 2;
/** 上下留 5px，線與今天那一點才不會貼著邊。 */
const INSET_Y = 5;
/** 今天那一點的半徑。 */
const TODAY_RADIUS = 3;
/** 百分比離它標的那一點多遠。 */
const LABEL_ABOVE = -8;
const LABEL_BELOW = 15;
/** 這麼靠邊的點，字要改成靠邊對齊，不然會超出畫布。 */
const EDGE = 24;

/** 落在畫布上的一個點。 */
type Mark = { index: number; used: number };

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
  forecast,
}: {
  daily: DailyPoint[];
  metric: Metric;
  totalMinutes: number;
  state: DisplayState;
  /** 虛線走到哪的那幾句，滑過去才出現。原本是面板上「預測」那一列。 */
  forecast: string | null;
}) {
  // 沒有資料就什麼都不畫，不畫空框。後端已經決定哪些情況吐空陣列
  // （免費方案、本期已結束、還沒抓到逐場紀錄），這裡不再判一次。
  // 沿用 `Pace` 那條「缺哪一項就不畫哪一列」。
  if (daily.length < 2 || totalMinutes <= 0) return null;

  const x = (index: number) =>
    INSET_X + (index / (daily.length - 1)) * (WIDTH - INSET_X * 2);

  const ratio = (used: number) => {
    const shown = metric === "used" ? used : totalMinutes - used;
    return Math.min(Math.max(shown / totalMinutes, 0), 1);
  };

  const y = (used: number) =>
    HEIGHT - INSET_Y - ratio(used) * (HEIGHT - INSET_Y * 2);

  const series = (pick: (point: DailyPoint) => number | null) => {
    const drawn: Mark[] = [];
    for (const [index, point] of daily.entries()) {
      const used = pick(point);
      if (used === null) continue;
      drawn.push({ index, used });
      // 額度用完之後不再往前畫。畫下去會沿著邊緣走成一條平的，看起來像
      // 「後來不玩了」，而實際上是那天之後就沒得玩了。
      if (used >= totalMinutes) break;
    }
    return drawn;
  };

  const path = (drawn: Mark[]) =>
    drawn.map((mark) => `${x(mark.index).toFixed(1)},${y(mark.used).toFixed(1)}`).join(" ");

  const actual = series((point) => point.usedMinutes);
  const projected = series((point) => point.projectedUsedMinutes);

  // 今天是最後一個有實線值的點。標出來有兩個作用：實線與虛線的交界看得見，
  // 而本期第一天只有一個實線的點，沒有這一點的話那天整條線是空的。
  const today: Mark | null = actual[actual.length - 1] ?? null;
  const end: Mark | null = projected[projected.length - 1] ?? null;

  /**
   * 一個點上的百分比。
   *
   * 標在點的哪一邊看它在上半部還是下半部：線從今天往期末走，往下走時今天
   * 在上半部、期末在下半部，兩個字各自落在線的空邊，不會壓到線。往上走時
   * 兩邊同時反過來，同一條規則還是對的。
   */
  const value = (mark: Mark, key: string) => {
    const px = x(mark.index);
    const py = y(mark.used);
    return (
      <text
        key={key}
        className="trend__value"
        x={px}
        y={py + (py > HEIGHT / 2 ? LABEL_ABOVE : LABEL_BELOW)}
        textAnchor={px < EDGE ? "start" : px > WIDTH - EDGE ? "end" : "middle"}
      >
        {Math.round(ratio(mark.used) * 100)}%
      </text>
    );
  };

  return (
    <svg
      className={modifier("trend", state)}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      role="img"
      // 讀螢幕的人不會滑過來，所以那幾句要進得了名稱。`aria-label` 在
      // 無障礙樹裡蓋過 `<title>`，只留 `<title>` 的話他們就只聽得到圖名。
      aria-label={
        forecast ? `本期用量走勢。${forecast.replace(/\n/g, "，")}` : "本期用量走勢"
      }
    >
      {/* SVG 的 tooltip 來自子元素 `<title>`，不是 `title` 屬性。 */}
      {forecast && <title>{forecast}</title>}
      {/* 透明的整塊當滑鼠目標。SVG 的空白處沒有圖形元素接得到指標事件，
          少了它就得剛好滑到那條 2px 的線上才叫得出 tooltip。
          `fill: none` 收不到事件，要 `transparent`。 */}
      <rect className="trend__hit" x={0} y={0} width={WIDTH} height={HEIGHT} />
      {/* 「用完」那條高度。看剩餘時它在底部，看已使用時在頂部，同一條
          運算式兩邊都對。兩個百分比講的是線走到哪，這條講的是走到哪裡
          就沒得玩了。 */}
      <line
        className="trend__limit"
        x1={0}
        x2={WIDTH}
        y1={y(totalMinutes)}
        y2={y(totalMinutes)}
      />
      {projected.length > 1 && (
        <polyline className="trend__projected" points={path(projected)} />
      )}
      {actual.length > 1 && (
        <polyline className="trend__actual" points={path(actual)} />
      )}
      {today && (
        <circle
          className="trend__today"
          cx={x(today.index).toFixed(1)}
          cy={y(today.used).toFixed(1)}
          r={TODAY_RADIUS}
        />
      )}
      {today && value(today, "today")}
      {/* 預測只剩今天那一點時（速度是 0，或還沒有預測）不重複標一次。 */}
      {end && end.index !== today?.index && value(end, "end")}
    </svg>
  );
}
