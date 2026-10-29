// 团队自动化设置（D43）：权限自动答复规则、Leader 自治级别与合并闸门、任务动态推送给 Leader。
// 只有你能改：Leader 经控制 socket 修改会被内核拒绝并留痕。
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { getTeamAutomation, updateTeamAutomation } from "../../api/commands.js";
import {
  Button,
  ErrorBanner,
  InputField,
  Panel,
  SelectField,
  Spinner,
  Tag,
  TextareaField,
} from "../../components/index.js";
import { AUTONOMY_LEVELS, toDraft, toInput, type AutomationDraft } from "./automationUi.js";

/** 团队自动化设置区。 */
export function AutomationPanel({ teamId }: { teamId: string }) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: ["team-automation", teamId],
    queryFn: () => getTeamAutomation(teamId),
  });
  const [draft, setDraft] = useState<AutomationDraft | null>(null);
  const mutation = useMutation({
    mutationFn: (next: AutomationDraft) => updateTeamAutomation(toInput(teamId, next)),
    onSuccess: (saved) => {
      queryClient.setQueryData(["team-automation", teamId], saved);
      setDraft(null);
    },
  });

  if (query.isPending) {
    return (
      <Panel className="settings__panel">
        <Spinner />
      </Panel>
    );
  }
  if (query.isError) {
    return (
      <Panel className="settings__panel">
        <ErrorBanner error={query.error} />
      </Panel>
    );
  }
  const current = draft ?? toDraft(query.data);
  const edit = (patch: Partial<AutomationDraft>) => {
    setDraft({ ...current, ...patch });
  };
  const level = AUTONOMY_LEVELS.find((item) => item.value === current.autonomyLevel);

  return (
    <Panel className="settings__panel" data-testid="automation-panel">
      <h2 className="settings__panel-title">团队自动化</h2>
      <p className="settings__panel-lead">
        常用的构建、测试、只读命令由规则自动允许；推送、提权、凭据、改写 Git 历史由规则自动拒绝；
        其余交给 Leader 答复（必须写理由），Leader 拿不准再交给你。下面决定 Leader
        能合并到什么程度。
      </p>

      <section className="settings__block">
        <h3 className="settings__block-title">Leader 自治级别</h3>
        <SelectField
          label="自治级别"
          value={String(current.autonomyLevel)}
          hint={level?.hint ?? ""}
          disabled={mutation.isPending}
          onChange={(event) => {
            edit({ autonomyLevel: Number(event.target.value) });
          }}
        >
          {AUTONOMY_LEVELS.map((item) => (
            <option key={item.value} value={item.value}>
              {item.label}
            </option>
          ))}
        </SelectField>
        <InputField
          label="合并前的检查命令"
          placeholder="例如 scripts/check.sh 或 pnpm run check"
          value={current.checkCommand}
          hint="Leader 合并前，内核把执行分支合进基线、在临时目录里跑这条命令，退出码为 0 才合并。留空表示不检查。"
          disabled={mutation.isPending}
          onChange={(event) => {
            edit({ checkCommand: event.target.value });
          }}
        />
        <TextareaField
          label="受保护路径（每行一项）"
          placeholder={"docs/DECISIONS.md\nAGENTS.md\n.github/"}
          value={current.protectedPaths}
          hint="相对工作空间；以 / 结尾表示整个目录。改动碰到这些路径时 Leader 不能合并，留给你。"
          rows={4}
          disabled={mutation.isPending}
          onChange={(event) => {
            edit({ protectedPaths: event.target.value });
          }}
        />
      </section>

      <section className="settings__block">
        <h3 className="settings__block-title">权限自动答复</h3>
        <TextareaField
          label="额外自动允许的命令前缀（每行一项）"
          placeholder={"make test\nscripts/check.sh"}
          value={current.allowCommands}
          hint="在内置白名单之外再放行；命令里的路径仍须在本次执行目录里。"
          rows={3}
          disabled={mutation.isPending}
          onChange={(event) => {
            edit({ allowCommands: event.target.value });
          }}
        />
        <TextareaField
          label="额外自动拒绝的命令前缀（每行一项）"
          placeholder={"docker\nterraform"}
          value={current.denyCommands}
          rows={3}
          disabled={mutation.isPending}
          onChange={(event) => {
            edit({ denyCommands: event.target.value });
          }}
        />
      </section>

      <section className="settings__block">
        <h3 className="settings__block-title">任务动态推送给 Leader</h3>
        <label className="settings__switch">
          <input
            type="checkbox"
            role="switch"
            checked={current.notifyLeader}
            disabled={mutation.isPending}
            onChange={(event) => {
              edit({ notifyLeader: event.target.checked });
            }}
          />
          <span>任务待验收、执行失败或受阻、权限请求等待答复时，自动通知 Leader 岗位去处理</span>
        </label>
        <p className="settings__panel-lead">
          需要团队有绑定了 Harness 的 Leader 岗位。每次推送都会唤起 Leader 的
          Harness，会消耗它的额度。
        </p>
      </section>

      {mutation.isError ? <ErrorBanner error={mutation.error} /> : null}
      <div className="settings__actions">
        {draft === null ? (
          query.data.updatedAt === null ? (
            <Tag tone="neutral">默认设置</Tag>
          ) : null
        ) : (
          <Button
            variant="neutral"
            disabled={mutation.isPending}
            onClick={() => {
              setDraft(null);
            }}
          >
            放弃修改
          </Button>
        )}
        <Button
          variant="primary"
          loading={mutation.isPending}
          disabled={draft === null}
          onClick={() => {
            mutation.mutate(current);
          }}
        >
          保存
        </Button>
      </div>
    </Panel>
  );
}
