import { useState } from "react";

import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  InputField,
  SelectField,
  TextareaField,
} from "../../components/index.js";
import type { HarnessInstance } from "../../bindings/HarnessInstance.js";
import type { RoleView } from "../../bindings/RoleView.js";
import { RoleFolderSection } from "./roleFolder.js";
import { useRoleMutations } from "./useRoleMutations.js";
import {
  TEST_HARNESS_LABEL,
  formatDate,
  harnessKindLabel,
  healthLabel,
  isTestHarness,
} from "../harness-detail/harnessFormat.js";

export interface RoleDetailProps {
  teamId: string;
  view: RoleView;
  harnesses: HarnessInstance[];
  isHarnessPending: boolean;
  harnessError: Error | null;
  picking: boolean;
  onPickToggle: () => void;
}

/** 右侧选中岗位详情：固定身份、职责与工作约定、当前绑定与更换、岗位文件夹与工作日志。 */
export function RoleDetail({
  teamId,
  view,
  harnesses,
  isHarnessPending,
  harnessError,
  picking,
  onPickToggle,
}: RoleDetailProps) {
  const role = view.role;
  const harness = view.harness;
  const { updateRole, archiveRole, bindRoleHarness, unbindRoleHarness } = useRoleMutations(teamId);

  const [name, setName] = useState(role.name);
  const [taskPrefix, setTaskPrefix] = useState(role.taskPrefix);
  const [description, setDescription] = useState(role.description);
  const [guidelines, setGuidelines] = useState(role.guidelines);
  const [pendingHarnessId, setPendingHarnessId] = useState("");

  const dirty =
    name !== role.name ||
    taskPrefix !== role.taskPrefix ||
    description !== role.description ||
    guidelines !== role.guidelines;

  const pendingHarness = harnesses.find((instance) => instance.id === pendingHarnessId);
  const pendingIsTestHarness = pendingHarness !== undefined && isTestHarness(pendingHarness.kind);

  return (
    <article className="role-detail">
      <header className="role-detail__header">
        <h2 className="role-detail__title">{role.name}</h2>
        <p className="role-detail__meta">
          岗位标识 {role.code}（自动生成） · 创建于 {formatDate(role.createdAt)}
        </p>
      </header>

      {harness === null ? (
        <p className="role-detail__unbound">未绑定 Harness · 暂不派工</p>
      ) : (
        <Card title={`当前绑定 ${harness.name}`} tone="info" className="role-detail__binding">
          <p className="role-detail__binding-meta">
            <span>{harnessKindLabel(harness.kind)}</span>
            <span aria-hidden="true"> · </span>
            <span className={`role-detail__health role-detail__health--${harness.health.status}`}>
              {healthLabel(harness.health.status)}
            </span>
          </p>
          {harness.health.detail === null ? null : (
            <p className="role-detail__binding-detail">{harness.health.detail}</p>
          )}
        </Card>
      )}

      <div className="role-detail__fields">
        <InputField
          label="岗位名称"
          required
          value={name}
          onChange={(event) => {
            setName(event.target.value);
          }}
        />
        <InputField
          label="编号前缀"
          required
          hint="2–4 位大写字母或数字，以字母开头；修改不影响已有任务编号。"
          value={taskPrefix}
          onChange={(event) => {
            setTaskPrefix(event.target.value);
          }}
        />
        <TextareaField
          label="职责"
          value={description}
          onChange={(event) => {
            setDescription(event.target.value);
          }}
        />
        <TextareaField
          label="工作约定"
          value={guidelines}
          onChange={(event) => {
            setGuidelines(event.target.value);
          }}
        />
      </div>

      {updateRole.error === null ? null : <ErrorBanner error={updateRole.error} />}

      <div className="role-detail__actions">
        <Button
          disabled={!dirty}
          loading={updateRole.isPending}
          onClick={() => {
            updateRole.mutate({
              roleId: role.id,
              name,
              taskPrefix,
              description,
              guidelines,
              isLeader: role.isLeader,
            });
          }}
        >
          保存岗位说明
        </Button>
        <Button
          variant="neutral"
          loading={archiveRole.isPending}
          onClick={() => {
            archiveRole.mutate(role.id);
          }}
        >
          归档岗位
        </Button>
      </div>

      {archiveRole.error === null ? null : <ErrorBanner error={archiveRole.error} />}

      <section className="role-detail__binding-actions">
        <Button
          variant={harness === null ? "primary" : "neutral"}
          onClick={onPickToggle}
          disabled={isHarnessPending && !picking}
        >
          {harness === null ? "绑定 Harness" : "更换 Harness"}
        </Button>
        {harness === null ? null : (
          <Button
            variant="neutral"
            loading={unbindRoleHarness.isPending}
            onClick={() => {
              unbindRoleHarness.mutate(role.id);
            }}
          >
            解绑
          </Button>
        )}
      </section>

      {unbindRoleHarness.error === null ? null : <ErrorBanner error={unbindRoleHarness.error} />}
      {bindRoleHarness.error === null ? null : <ErrorBanner error={bindRoleHarness.error} />}

      {picking ? (
        <div className="role-detail__picker">
          {isHarnessPending ? <span>正在加载实例…</span> : null}

          {!isHarnessPending && harnessError !== null ? <ErrorBanner error={harnessError} /> : null}

          {!isHarnessPending && harnessError === null && harnesses.length === 0 ? (
            <EmptyState
              title="本团队还没有 Harness 实例"
              description="请先在「Harness 实例」页签注册一个实例。"
            />
          ) : null}

          {!isHarnessPending && harnessError === null && harnesses.length > 0 ? (
            <>
              <SelectField
                label="选择本团队的 Harness 实例"
                value={pendingHarnessId}
                onChange={(event) => {
                  setPendingHarnessId(event.target.value);
                }}
              >
                <option value="">请选择实例</option>
                {harnesses.map((instance) => (
                  <option key={instance.id} value={instance.id}>
                    {harnessKindLabel(instance.kind)}
                    {isTestHarness(instance.kind) ? `（${TEST_HARNESS_LABEL}）` : ""} /{" "}
                    {instance.name}（{healthLabel(instance.health.status)}）
                  </option>
                ))}
              </SelectField>
              {pendingIsTestHarness ? (
                <p className="role-detail__test-hint">
                  这是测试用实例：仅用于验证流程，不要绑定正式岗位。
                </p>
              ) : null}
              <Button
                disabled={pendingHarnessId === ""}
                loading={bindRoleHarness.isPending}
                onClick={() => {
                  bindRoleHarness.mutate({
                    roleId: role.id,
                    harnessId: pendingHarnessId,
                  });
                }}
              >
                确认绑定
              </Button>
            </>
          ) : null}
        </div>
      ) : null}

      <RoleFolderSection teamId={teamId} view={view} />

      <p className="role-detail__hint">
        岗位身份与绑定分离：更换 Harness 不会改变岗位、任务编号或历史记录。
      </p>
      <p className="role-detail__counts">
        执行中任务 {view.doingTaskCount} 项 · 未结束任务 {view.openTaskCount} 项
      </p>
    </article>
  );
}
