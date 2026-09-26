import type { CSSProperties } from "react";
import type { UpdateProgress } from "./types";

/**
 * 更新安裝途中，原本那顆更新按鈕換成這一顆：底色填到下載的百分比。
 *
 * 關於頁與主面板的橫幅共用。按不下去：再按一次是重裝。
 */
export default function ProgressButton({
  progress,
  className,
}: {
  /// null 是按下去到第一個進度事件之間，那時還不知道檔案多大。
  progress: UpdateProgress | null;
  className?: string;
}) {
  const percent =
    progress?.kind === "installing" ? 100 : (progress?.value ?? null);
  const label =
    progress?.kind === "installing"
      ? "安裝中…"
      : percent === null
        ? "下載中…"
        : `${percent}%`;
  return (
    <button
      className={["progress", className].filter(Boolean).join(" ")}
      disabled
      style={{ "--progress": `${percent ?? 0}%` } as CSSProperties}
    >
      {label}
    </button>
  );
}
