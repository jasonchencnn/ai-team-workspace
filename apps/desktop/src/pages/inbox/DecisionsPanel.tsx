// 任务详情里的决策列表：这条任务上记录过的决策。
import { useQuery } from "@tanstack/react-query";

import { listDecisions } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import { EmptyState, ErrorBanner, Tag } from "../../components/index.js";
import { formatTime } from "../tasks/taskUi.js";
import "./inbox.css";

export interface DecisionsPanelProps {
  teamId: string;
  taskId: string;
}

/** 任务详情页里的「决策」区块。 */
export function DecisionsPanel({ teamId, taskId }: DecisionsPanelProps) {
  const decisions = useQuery({
    queryKey: queryKeys.decisions(teamId, taskId),
    queryFn: () => listDecisions({ teamId, taskId }),
    enabled: teamId !== "" && taskId !== "",
  });

  return (
    <section className="decisions">
      <h2 className="decisions__title">决策</h2>
      {decisions.isError ? <ErrorBanner error={decisions.error} /> : null}
      {decisions.data === undefined ? null : decisions.data.length === 0 ? (
        <EmptyState bare title="还没有决策" description="在「待我处理」里给这条任务记一条决策。" />
      ) : (
        <ul className="decisions__list">
          {decisions.data.map((decision) => (
            <li className="decisions__item" key={decision.id}>
              <div className="decisions__item-head">
                <time dateTime={decision.createdAt}>{formatTime(decision.createdAt)}</time>
                {decision.resumedTask ? <Tag tone="success">已恢复任务</Tag> : null}
                {decision.roleName === null ? null : (
                  <span className="decisions__role">岗位：{decision.roleName}</span>
                )}
              </div>
              <p className="decisions__text">{decision.text}</p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
