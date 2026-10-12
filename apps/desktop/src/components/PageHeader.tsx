import type { ReactNode } from "react";

export interface PageHeaderProps {
  title: string;
  description?: string;
  /** 右侧操作区（按钮等）。 */
  actions?: ReactNode;
}

export function PageHeader({ title, description, actions }: PageHeaderProps) {
  return (
    <header className="ui-page-header">
      <div>
        <h1 className="ui-page-header__title">{title}</h1>
        {description === undefined ? null : (
          <p className="ui-page-header__description">{description}</p>
        )}
      </div>
      {actions === undefined ? null : <div className="ui-page-header__actions">{actions}</div>}
    </header>
  );
}
