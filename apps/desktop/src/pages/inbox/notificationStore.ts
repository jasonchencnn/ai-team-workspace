// 本次会话收到的应用内通知。通知不落库：只在页面打开期间攒着，按时间倒序展示，可清空。
import { useSyncExternalStore } from "react";

import { onNotifyEvent } from "../../api/commands.js";
import type { Notification } from "../../bindings/Notification.js";

type Listener = () => void;

const listeners = new Set<Listener>();

/** 会话内收到的通知，最新的在前。 */
let notifications: Notification[] = [];
/** 未读数量：打开通知中心后归零。 */
let unread = 0;
let stopBackend: (() => void) | null = null;
let starting = false;

function snapshot(): Notification[] {
  return notifications;
}

function notifySubscribers(): void {
  for (const listener of listeners) {
    listener();
  }
}

function push(notification: Notification): void {
  notifications = [notification, ...notifications];
  unread += 1;
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

/** 订阅会话内的通知列表。 */
export function useSessionNotifications(): Notification[] {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** 订阅未读数量。 */
export function useUnreadNotifications(): number {
  return useSyncExternalStore(
    subscribe,
    () => unread,
    () => unread,
  );
}

/** 打开通知中心：未读数归零。 */
export function markNotificationsRead(): void {
  if (unread === 0) {
    return;
  }
  unread = 0;
  notifySubscribers();
}

/** 清空本次会话收到的通知。 */
export function clearNotifications(): void {
  notifications = [];
  unread = 0;
  notifySubscribers();
}
