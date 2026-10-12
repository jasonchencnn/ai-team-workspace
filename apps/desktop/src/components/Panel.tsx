import type { HTMLAttributes } from "react";

/** 白色面板：圆角 12、描边、内边距 24。 */
export function Panel({ className, ...rest }: HTMLAttributes<HTMLElement>) {
  return <section {...rest} className={["ui-panel", className].filter(Boolean).join(" ")} />;
}
