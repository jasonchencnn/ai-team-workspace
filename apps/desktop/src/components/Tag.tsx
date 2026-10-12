import type { ReactNode } from "react";

export type TagTone = "neutral" | "info" | "warning" | "success" | "danger";

/** 状态胶囊。颜色始终配合文字，不单靠颜色表达状态。 */
export function Tag({ tone = "neutral", children }: { tone?: TagTone; children: ReactNode }) {
  return <span className={`ui-tag ui-tag--${tone}`}>{children}</span>;
}
