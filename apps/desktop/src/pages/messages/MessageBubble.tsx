import { Button, Tag } from "../../components/index.js";
import type { Message } from "../../bindings/Message.js";
import {
  MESSAGE_STATUS_LABELS,
  MESSAGE_STATUS_TONES,
  canRetry,
  formatMessageTime,
  senderName,
} from "./messagesUi.js";

export interface MessageBubbleProps {
  message: Message;
  /** 岗位 id → 岗位名，用来显示发送方与接收岗位。 */
  roleNames: Map<string, string>;
  /** 重新发送：只有失败与过期才有这个操作，由页面接后端命令。 */
  onRetry?: (message: Message) => void;
  /** 重发请求进行中。 */
  retrying?: boolean;
  /** 显示用的「现在」，决定时间写成「今天 10:31」还是「9/21 10:31」。 */
  now?: Date;
}

/**
 * 一条消息的气泡：发送方、正文、状态标签、回复正文与时间。
 * 失败与过期额外显示原因与「重新发送」——能不能重发看后端返回的状态。
 */
export function MessageBubble({
  message,
  roleNames,
  onRetry,
  retrying = false,
  now = new Date(),
}: MessageBubbleProps) {
  const fromRole = message.sender === "role";
  const statusLabel = MESSAGE_STATUS_LABELS[message.status];
  const statusTone = MESSAGE_STATUS_TONES[message.status];
  const reason = message.failureReason;
  const retryable = canRetry(message.status);
  const classes = [
    "msg-bubble",
    fromRole ? "msg-bubble--in" : "msg-bubble--out",
    message.status === "failed" ? "msg-bubble--failed" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <article className={classes} data-testid="message-bubble" data-status={message.status}>
      <header className="msg-bubble__head">
        <span className="msg-bubble__sender">{senderName(message, roleNames)}</span>
        <Tag tone={statusTone}>{statusLabel}</Tag>
        <time className="msg-bubble__time" dateTime={message.createdAt}>
          {formatMessageTime(message.createdAt, now)}
        </time>
      </header>

      <p className="msg-bubble__body">{message.body}</p>

      {message.replyBody === null || message.replyBody === "" ? null : (
        <div className="msg-bubble__reply" data-testid="message-reply">
          {/* 回复来自接收岗位：内核把回复写回这条消息，所以回复方显示接收岗位的名字。 */}
          <span className="msg-bubble__sender">
            {roleNames.get(message.toRoleId) ?? "岗位"} / 已回复
          </span>
          <p className="msg-bubble__reply-body">{message.replyBody}</p>
          {message.repliedAt === null ? null : (
            <time className="msg-bubble__reply-time" dateTime={message.repliedAt}>
              {formatMessageTime(message.repliedAt, now)}
            </time>
          )}
        </div>
      )}

      {reason === null || reason === "" ? null : (
        <p className="msg-bubble__failure" data-testid="message-failure">
          失败原因：{reason}
        </p>
      )}

      {retryable && onRetry !== undefined ? (
        <div className="msg-bubble__actions">
          <Button
            variant="neutral"
            loading={retrying}
            disabled={retrying}
            onClick={() => {
              onRetry(message);
            }}
          >
            重新发送
          </Button>
        </div>
      ) : null}
    </article>
  );
}
