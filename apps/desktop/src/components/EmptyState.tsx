import type { ReactNode } from "react";

export interface EmptyStateProps {
  title: string;
  description?: string;
  action?: ReactNode;
  /** 放进 Panel 等已有容器时使用：去掉自身的虚线框、内边距和背景，避免双层边框。 */
  bare?: boolean;
}

export function EmptyState({ title, description, action, bare = false }: EmptyStateProps) {
  return (
    <div className={bare ? "ui-empty ui-empty--bare" : "ui-empty"}>
      <h2 className="ui-empty__title">{title}</h2>
      {description === undefined ? null : <p className="ui-empty__description">{description}</p>}
      {action === undefined ? null : <div className="ui-empty__action">{action}</div>}
    </div>
  );
}
