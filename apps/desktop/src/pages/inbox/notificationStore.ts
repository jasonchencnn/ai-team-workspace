// 本次会话收到的应用内通知。通知不落库：只在页面打开期间攒着，按时间倒序展示，可清空。
// 通知按团队产生：通知中心只显示当前团队的通知，未读数也按当前团队算。
import { useSyncExternalStore } from "react";

import { onNotifyEvent } from "../../api/commands.js";
import type { Notification } from "../../bindings/Notification.js";
import { notificationsForTeam, unreadForTeam, type SessionNotification } from "./inboxUi.js";

type Listener = () => void;

const listeners = new Set<Listener>();

/** 会话内收到的通知，最新的在前；带已读标记。 */
let entries: SessionNotification[] = [];
/** 按团队缓存的快照：同一份数据保持同一个引用（React 的 useSyncExternalStore 要求）。 */
const snapshots = new Map<string, Notification[]>();
let stopBackend: (() => void) | null = null;
let starting = false;

function snapshotFor(teamId: string): Notification[] {
  const cached = snapshots.get(teamId);
  if (cached !== undefined) {
    return cached;
  }
  const list = notificationsForTeam(entries, teamId);
  snapshots.set(teamId, list);
  return list;
}

function notifySubscribers(): void {
  snapshots.clear();
  for (const listener of listeners) {
    listener();
  }
}

function push(notification: Notification): void {
  entries = [{ notification, read: false }, ...entries];
  notifySubscribers();
}

function ensureListening(): void {
  if (stopBackend !== null || starting) {
    return;
  }
  starting = true;
  void onNotifyEvent((event) => {
    if (event.type === "notification") {
      push(event.notification);
    }
  })
    .then((unlisten) => {
      starting = false;
      stopBackend = unlisten;
    })
    .catch(() => {
      starting = false;
    });
}

function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  if (listeners.size === 1) {
    ensureListening();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && stopBackend !== null) {
      stopBackend();
      stopBackend = null;
    }
  };
}

/** 订阅当前团队的通知列表。 */
export function useSessionNotifications(teamId: string): Notification[] {
  return useSyncExternalStore(
    subscribe,
    () => snapshotFor(teamId),
    () => snapshotFor(teamId),
  );
}

/** 订阅当前团队的未读数量。 */
export function useUnreadNotifications(teamId: string): number {
  return useSyncExternalStore(
    subscribe,
    () => unreadForTeam(entries, teamId),
    () => unreadForTeam(entries, teamId),
  );
}

/** 打开通知中心：该团队的未读数归零（别的团队不受影响）。 */
export function markNotificationsRead(teamId: string): void {
  if (!entries.some((entry) => !entry.read && entry.notification.teamId === teamId)) {
    return;
  }
  entries = entries.map((entry) =>
    entry.notification.teamId === teamId ? { ...entry, read: true } : entry,
  );
  notifySubscribers();
}

/** 清空当前团队本次会话收到的通知。 */
export function clearNotifications(teamId: string): void {
  const remaining = entries.filter((entry) => entry.notification.teamId !== teamId);
  if (remaining.length === entries.length) {
    return;
  }
  entries = remaining;
  notifySubscribers();
}
