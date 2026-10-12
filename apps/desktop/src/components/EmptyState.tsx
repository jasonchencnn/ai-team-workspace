import type { ReactNode } from "react";

export interface EmptyStateProps {
  title: string;
  description?: string;
  action?: ReactNode;
}

export function EmptyState({ title, description, action }: EmptyStateProps) {
  return (
    <div className="ui-empty">
      <h2 className="ui-empty__title">{title}</h2>
      {description === undefined ? null : <p className="ui-empty__description">{description}</p>}
      {action === undefined ? null : <div className="ui-empty__action">{action}</div>}
    </div>
  );
}
