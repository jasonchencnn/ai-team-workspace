import { useEffect, type ReactNode } from "react";

export interface ModalProps {
  /** 弹窗标题。 */
  title: ReactNode;
  /** 标题右侧的区域（状态标签等）。 */
  badge?: ReactNode;
  /** 标题下的一行说明（任务编号、执行次数、Harness）。 */
  description?: ReactNode;
  children?: ReactNode;
  /** 底部操作区，通常放按钮。 */
  actions?: ReactNode;
  onClose: () => void;
  /** 无障碍标签；缺省用标题文本。 */
  ariaLabel?: string;
}

/**
 * 模态弹窗：遮罩 + 居中卡片，标题区可选副标题与标签，底部为操作区。
 * 按 Escape 或点击遮罩关闭。内容过长时卡片内部滚动，不撑破视口。
 */
export function Modal({
  title,
  badge,
  description,
  children,
  actions,
  onClose,
  ariaLabel,
}: ModalProps) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  return (
    <div
      className="ui-modal__overlay"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        className="ui-modal"
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel ?? (typeof title === "string" ? title : undefined)}
      >
        <header className="ui-modal__head">
          <div className="ui-modal__headline">
            <h2 className="ui-modal__title">{title}</h2>
            {badge === undefined ? null : <div className="ui-modal__badge">{badge}</div>}
          </div>
          {description === undefined ? null : (
            <p className="ui-modal__description">{description}</p>
          )}
        </header>
        {children === undefined ? null : <div className="ui-modal__body">{children}</div>}
        {actions === undefined ? null : <div className="ui-modal__actions">{actions}</div>}
      </div>
    </div>
  );
}
