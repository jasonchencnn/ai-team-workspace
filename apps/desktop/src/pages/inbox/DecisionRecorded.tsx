// 「决策已记录」结果卡片（Figma 8-311）。
import { Button } from "../../components/index.js";
import "./inbox.css";

export interface DecisionRecordedProps {
  /** 决策内容。 */
  text: string;
  /** 关联任务编号；没有时为 null。 */
  taskKey: string | null;
  /** 继续处理的岗位名；没有时为 null。 */
  roleName: string | null;
  /** 是否同时把受阻任务恢复到了 READY。 */
  resumedTask: boolean;
  /** 操作人（当前用户）。 */
  actor: string;
  /** 返回待我处理。 */
  onBack: () => void;
}

/** 决策已记录：成功后的状态卡。 */
export function DecisionRecorded({
  text,
  taskKey,
  roleName,
  resumedTask,
  actor,
  onBack,
}: DecisionRecordedProps) {
  return (
    <section className="decision-recorded" aria-live="polite">
      <h2 className="decision-recorded__title">
        <span aria-hidden="true">✓</span> 决策已记录
      </h2>
      <p className="decision-recorded__text">{text}</p>
      <p className="decision-recorded__link">已写入团队活动记录</p>
      {resumedTask && taskKey !== null ? (
        <>
          <p className="decision-recorded__note">
            {taskKey} 已恢复为待开始，由{roleName === null ? "负责岗位" : roleName}继续处理。
          </p>
          <p className="decision-recorded__note">任务解除阻塞，不代表已经完成。</p>
        </>
      ) : null}
      <p className="decision-recorded__meta">操作人 {actor}</p>
      <Button variant="primary" onClick={onBack}>
        返回待我处理
      </Button>
    </section>
  );
}
