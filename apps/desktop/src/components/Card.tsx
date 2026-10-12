import type { ReactNode } from "react";

export type CardTone = "default" | "info" | "warning" | "success";

export interface CardProps {
  title: ReactNode;
  tone?: CardTone;
  className?: string;
  children?: ReactNode;
}

export function Card({ title, tone = "default", className, children }: CardProps) {
  const classes = ["ui-card", tone === "default" ? null : `ui-card--${tone}`, className]
    .filter(Boolean)
    .join(" ");
  return (
    <article className={classes}>
      <h3 className="ui-card__title">{title}</h3>
      {children === undefined ? null : <div className="ui-card__body">{children}</div>}
    </article>
  );
}
