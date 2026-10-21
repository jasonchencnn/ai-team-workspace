// 待我处理的数据访问：查询、轮询兜底与通知事件刷新。
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { getInbox, onNotifyEvent } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { Inbox } from "../../bindings/Inbox.js";
import type { NotifyEvent } from "../../bindings/NotifyEvent.js";

/** 轮询兜底间隔（毫秒）。事件通道不可用时靠它保持界面跟上后端。 */
export const INBOX_POLL_INTERVAL_MS = 5000;

/** 待我处理列表。 */
export function useInbox(teamId: string): ReturnType<typeof useQuery<Inbox, Error>> {
  return useQuery({
    queryKey: queryKeys.inbox(teamId),
    queryFn: () => getInbox({ teamId }),
    enabled: teamId !== "",
    refetchInterval: INBOX_POLL_INTERVAL_MS,
  });
}

/**
 * 待处理总数：给侧边栏角标用。
 * 与页面共用同一份缓存，页面已经取过就不用再取一次。
 */
export function useInboxCount(teamId: string): number {
  const query = useInbox(teamId);
  return query.data?.counts.total ?? 0;
}

/**
 * 订阅通知事件：收到与本团队相关的通知就让待我处理重新取一次。
 * 订阅失败（权限被拒、通道不可用）时只记日志，轮询兜底继续工作。
 */
export function useNotifyRefresh(teamId: string): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (teamId === "") {
      return;
    }
    let dispose: (() => void) | undefined;
    let cancelled = false;

    const handle = (event: NotifyEvent) => {
      if (event.type !== "notification" || event.notification.teamId !== teamId) {
        return;
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.inbox(teamId) });
    };

    void onNotifyEvent(handle)
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
          return;
        }
        dispose = unlisten;
      })
      .catch((error: unknown) => {
        console.error("订阅通知事件失败，已改用轮询兜底：", error);
      });

    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [queryClient, teamId]);
}
