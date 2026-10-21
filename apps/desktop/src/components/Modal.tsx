import { useEffect, useRef, type ReactNode } from "react";

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

/** 弹窗内可聚焦的元素（不含 tabindex=-1 之类）。 */
const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** 取得元素内可聚焦元素中第一个可用的（没有就返回元素本身，便于聚焦容器）。 */
function firstFocusable(root: HTMLElement): HTMLElement | null {
  const nodes = [...root.querySelectorAll<HTMLElement>(FOCUSABLE)];
  return (
    nodes.find((node) => node.offsetParent !== null || node === document.activeElement) ?? null
  );
}

/** 取得元素内可聚焦元素中最后一个可用的。 */
function lastFocusable(root: HTMLElement): HTMLElement | null {
  const nodes = [...root.querySelectorAll<HTMLElement>(FOCUSABLE)];
  const visible = nodes.filter(
    (node) => node.offsetParent !== null || node === document.activeElement,
  );
  return visible.at(-1) ?? null;
}

/**
 * 模态弹窗：遮罩 + 居中卡片，标题区可选副标题与标签，底部为操作区。
 * 按 Escape 或点击遮罩关闭。内容过长时卡片内部滚动，不撑破视口。
 *
 * 无障碍：打开时把焦点移进弹窗，Tab 在弹窗内循环，关闭后焦点回到触发元素。
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
  const dialogRef = useRef<HTMLDivElement>(null);
  // 打开弹窗时记下触发元素，关闭后把焦点还回去。
  const openerRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    openerRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = dialogRef.current;
    if (dialog !== null) {
      // 优先聚焦第一个可交互元素；没有就聚焦弹窗本身，读屏仍能读出标题。
      const target = firstFocusable(dialog) ?? dialog;
      target.focus();
    }
    return () => {
      const opener = openerRef.current;
      if (opener !== null && document.contains(opener)) {
        opener.focus();
      }
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }
      const dialog = dialogRef.current;
      if (dialog === null) {
        return;
      }
      // 焦点在弹窗外时（例如用户点了遮罩），先把它拉回弹窗内。
      if (!dialog.contains(document.activeElement)) {
        event.preventDefault();
        (firstFocusable(dialog) ?? dialog).focus();
        return;
      }
      // Tab 在弹窗内循环，不跑到背后的页面上。
      const first = firstFocusable(dialog);
      const last = lastFocusable(dialog);
      if (first === null || last === null) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
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
        tabIndex={-1}
        ref={dialogRef}
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
