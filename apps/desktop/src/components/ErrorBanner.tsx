import type { ReactNode } from "react";
import { errorMessage } from "../api/errors.js";

export interface ErrorBannerProps {
  /** invoke 拒绝的值；后端错误体显示其 message。 */
  error: unknown;
  /** 右侧操作（如重试按钮）。 */
  action?: ReactNode;
}

export function ErrorBanner({ error, action }: ErrorBannerProps) {
  return (
    <div className="ui-error-banner" role="alert">
      <span>{errorMessage(error)}</span>
      {action}
    </div>
  );
}
