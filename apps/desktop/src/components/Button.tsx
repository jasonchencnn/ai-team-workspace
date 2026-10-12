import type { ButtonHTMLAttributes } from "react";
import { Spinner } from "./Spinner.js";

export type ButtonVariant = "primary" | "neutral" | "subtle";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  /** 加载中：显示转圈并禁止点击。 */
  loading?: boolean;
}

export function Button({
  variant = "primary",
  loading = false,
  disabled = false,
  type = "button",
  className,
  children,
  ...rest
}: ButtonProps) {
  const classes = ["ui-button", `ui-button--${variant}`, className].filter(Boolean).join(" ");
  return (
    <button
      {...rest}
      type={type}
      className={classes}
      disabled={disabled || loading}
      aria-busy={loading}
    >
      {loading ? <Spinner /> : null}
      {children}
    </button>
  );
}
