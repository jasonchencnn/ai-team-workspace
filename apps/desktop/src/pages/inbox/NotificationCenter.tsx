// 顶栏铃铛：本次会话收到的应用内通知。通知不落库，按时间倒序，可清空。
import { useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router";

import { Button, EmptyState, Tag } from "../../components/index.js";
import { formatTime } from "../tasks/taskUi.js";
import { notificationTargetPath, NOTIFICATION_CATEGORY_SEVERITY } from "./inboxUi.js";
import {
  clearNotifications,
  markNotificationsRead,
  useSessionNotifications,
  useUnreadNotifications,
} from "./notificationStore.js";
import "./inbox.css";

/** 铃铛图标。 */
function BellIcon() {
  return (
    <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true" focusable="false">
      <path
        d="M10 2.5a4.2 4.2 0 0 0-4.2 4.2v2.6L4.4 12h11.2l-1.4-2.7V6.7A4.2 4.2 0 0 0 10 2.5Zm1.8 11.3a1.8 1.8 0 0 1-3.6 0Z"
        fill="currentColor"
      />
    </svg>
  );
}

/** 通知中心：铃铛 + 通知列表。 */
export function NotificationCenter() {
  const { teamId } = useParams();
  const navigate = useNavigate();
  const notifications = useSessionNotifications();
  const unread = useUnreadNotifications();
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  // 点外面或按 Escape 收起面板。
  useEffect(() => {
    if (!open) {
      return;
    }
    const onPointerDown = (event: MouseEvent) => {
      const node = containerRef.current;
      if (node !== null && event.target instanceof Node && !node.contains(event.target)) {
        setOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next) {
      markNotificationsRead();
    }
  };

  // 通知按团队产生：没有当前团队时不显示铃铛。
  if (teamId === undefined) {
    return null;
  }

  return (
    <div className="notify-center" ref={containerRef}>
      <button
        type="button"
        className="notify-center__bell"
        aria-label={unread > 0 ? `通知中心（${String(unread)} 条未读）` : "通知中心"}
        aria-expanded={open}
        onClick={toggle}
      >
        <BellIcon />
        {unread > 0 ? <span className="notify-center__badge">{unread}</span> : null}
      </button>
      {open ? (
        <div className="notify-center__panel" role="dialog" aria-label="通知中心">
          <header className="notify-center__head">
            <h2 className="notify-center__title">通知</h2>
            <div className="notify-center__head-actions">
              {notifications.length > 0 ? (
                <Button variant="subtle" onClick={clearNotifications}>
                  清空
                </Button>
              ) : null}
              <Button
                variant="subtle"
                onClick={() => {
                  setOpen(false);
                  void navigate(`/teams/${teamId}/notifications`);
                }}
              >
                管理订阅
              </Button>
            </div>
          </header>
          {notifications.length === 0 ? (
            <EmptyState
              bare
              title="还没有通知"
              description="本次会话里收到的通知会出现在这里；通知不落库，关闭应用即清空。"
            />
          ) : (
            <ul className="notify-center__list">
              {notifications.map((notification) => (
                <li key={notification.id}>
                  <button
                    type="button"
                    className="notify-center__item"
                    onClick={() => {
                      setOpen(false);
                      void navigate(notificationTargetPath(teamId, notification));
                    }}
                  >
                    <span className="notify-center__item-head">
                      <Tag
                        tone={
                          NOTIFICATION_CATEGORY_SEVERITY[notification.category] === "high"
                            ? "danger"
                            : NOTIFICATION_CATEGORY_SEVERITY[notification.category] === "important"
                              ? "warning"
                              : "info"
                        }
                      >
                        {notification.categoryLabel}
                      </Tag>
                      <time dateTime={notification.occurredAt}>
                        {formatTime(notification.occurredAt)}
                      </time>
                    </span>
                    <span className="notify-center__item-title">{notification.title}</span>
                    {notification.body ? (
                      <span className="notify-center__item-body">{notification.body}</span>
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : null}
    </div>
  );
}
