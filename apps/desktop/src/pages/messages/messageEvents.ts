// 消息事件的纯逻辑：内核的通知通道（teamspace:notify）里既有应用内通知，也有消息状态变化，
// 这里只判断「这条事件要不要让岗位通信页刷新」。React 侧只负责订阅与失效查询。
import type { NotifyEvent } from "../../bindings/NotifyEvent.js";

/** 一次消息状态变化。 */
export interface MessageStatusChange {
  /** 变了状态的那条消息所属的线程。 */
  threadId: string;
  /** 消息所在团队。 */
  teamId: string;
}

/**
 * 这条事件是不是本团队的消息状态变化。
 * 不是就与岗位通信页无关：通知类事件由通知中心（M5-04）处理，别的团队的消息也不关本页的事。
 */
export function messageStatusChange(
  event: NotifyEvent,
  teamId: string,
): MessageStatusChange | null {
  if (event.type !== "message_status") {
    return null;
  }
  if (event.message.teamId !== teamId) {
    return null;
  }
  return { threadId: event.message.threadId, teamId: event.message.teamId };
}
