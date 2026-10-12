import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";

import { createHarness, detectHarnesses } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  InputField,
  Panel,
  SelectField,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { HarnessInstance } from "../../bindings/HarnessInstance.js";
import type { HarnessKind } from "../../bindings/HarnessKind.js";
import {
  TEST_HARNESS_LABEL,
  formatRelative,
  formatVersion,
  harnessKindLabel,
  healthLabel,
  healthTone,
  isTestHarness,
} from "../harness-detail/harnessFormat.js";

export interface HarnessInstancesSectionProps {
  teamId: string;
  harnesses: HarnessInstance[];
  isPending: boolean;
  error: Error | null;
  onRetry: () => void;
}

/** 「Harness 实例」区：实例卡片、本机 CLI 检测与新建实例。 */
export function HarnessInstancesSection({
  teamId,
  harnesses,
  isPending,
  error,
  onRetry,
}: HarnessInstancesSectionProps) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false);

  const createMutation = useMutation({
    mutationFn: createHarness,
    onSuccess: () => {
      setCreating(false);
      void queryClient.invalidateQueries({ queryKey: queryKeys.harnesses(teamId) });
    },
  });

  const detailPath = (harnessId: string) => `/teams/${teamId}/harnesses/${harnessId}`;

  return (
    <Panel className="harness-section">
      <h2 className="harness-section__title">Harness 实例</h2>
      <p className="harness-section__hint">
        实例属于本团队；岗位只保存可替换的绑定。平台不保存任何密钥，登录由各 CLI 自己管理。
      </p>

      <div className="harness-section__toolbar">
        <HarnessDetection />
        <Button
          variant="neutral"
          onClick={() => {
            setCreating(!creating);
          }}
        >
          {creating ? "收起新建实例" : "新建实例"}
        </Button>
      </div>

      {isPending ? <Spinner label="正在加载实例" /> : null}

      {error === null ? null : (
        <ErrorBanner
          error={error}
          action={
            <Button variant="neutral" onClick={onRetry}>
              重试
            </Button>
          }
        />
      )}

      {!isPending && error === null && harnesses.length === 0 ? (
        <EmptyState
          title="本团队还没有 Harness 实例"
          description="注册一个 Claude Code、Codex 或 Mock 实例后即可为岗位绑定执行者。"
        />
      ) : null}

      <div className="harness-section__grid">
        {harnesses.map((instance) => (
          <div key={instance.id} className="harness-card">
            <Card
              title={
                <span className="harness-card__title">
                  {harnessKindLabel(instance.kind)}
                  {isTestHarness(instance.kind) ? `（${TEST_HARNESS_LABEL}）` : ""} /{" "}
                  {instance.name}
                </span>
              }
              tone={instance.health.status === "error" ? "warning" : "default"}
              className="harness-card__inner"
            >
              <p className="harness-card__meta">版本：{formatVersion(instance.health.version)}</p>
              <p className="harness-card__meta">
                <Tag tone={healthTone(instance.health.status)}>
                  {healthLabel(instance.health.status)}
                </Tag>
                <span className="harness-card__checked">
                  最近检查 {formatRelative(instance.health.checkedAt, Date.now())}
                </span>
              </p>
              <p className="harness-card__config">
                配置：
                {instance.model === null ? "默认模型" : `model ${instance.model}`}
                {instance.profile === null ? "" : ` · profile ${instance.profile}`}
              </p>
              {instance.health.status === "error" && instance.health.detail !== null ? (
                <p className="harness-card__detail">{instance.health.detail}</p>
              ) : null}
              <div className="harness-card__actions">
                <Button
                  variant="neutral"
                  onClick={() => {
                    void navigate(detailPath(instance.id));
                  }}
                >
                  查看详情
                </Button>
              </div>
            </Card>
          </div>
        ))}
      </div>

      {creating ? (
        <HarnessCreateForm
          teamId={teamId}
          isPending={createMutation.isPending}
          error={createMutation.error}
          onSubmit={(input) => {
            createMutation.mutate(input);
          }}
          onCancel={() => {
            setCreating(false);
          }}
        />
      ) : null}
    </Panel>
  );
}

/** 「检测本机 CLI」：按需查询，用户点击后才调用。 */
function HarnessDetection() {
  const detection = useQuery({
    queryKey: queryKeys.harnessDetections,
    queryFn: detectHarnesses,
    enabled: false,
  });

  return (
    <div className="harness-detection">
      <Button
        variant="neutral"
        loading={detection.isFetching}
        onClick={() => {
          void detection.refetch();
        }}
      >
        检测本机 CLI
      </Button>

      {detection.error === null ? null : <ErrorBanner error={detection.error} />}

      {detection.data === undefined ? null : (
        <ul className="harness-detection__list">
          {detection.data.map((item) => (
            <li key={item.kind} className="harness-detection__item">
              <Tag tone={item.found ? "success" : "warning"}>{harnessKindLabel(item.kind)}</Tag>
              {item.found ? (
                <span className="harness-detection__text">
                  已找到 · 版本 {item.version ?? "未知"}
                </span>
              ) : (
                <span className="harness-detection__text">{item.error ?? "未找到可执行文件"}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface HarnessCreateFormProps {
  teamId: string;
  isPending: boolean;
  error: Error | null;
  onSubmit: (input: {
    teamId: string;
    name: string;
    kind: HarnessKind;
    executable: string | null;
    model: string | null;
    profile: string | null;
  }) => void;
  onCancel: () => void;
}

function HarnessCreateForm({
  teamId,
  isPending,
  error,
  onSubmit,
  onCancel,
}: HarnessCreateFormProps) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<HarnessKind>("claude_code");
  const [executable, setExecutable] = useState("");
  const [model, setModel] = useState("");
  const [profile, setProfile] = useState("");

  const trimmed = (value: string) => (value.trim() === "" ? null : value.trim());

  return (
    <form
      className="harness-create"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({
          teamId,
          name: name.trim(),
          kind,
          executable: trimmed(executable),
          model: trimmed(model),
          profile: trimmed(profile),
        });
      }}
    >
      <h3 className="harness-create__title">新建实例</h3>

      <InputField
        label="实例名称"
        required
        hint="团队内唯一，如 frontend-local。"
        value={name}
        onChange={(event) => {
          setName(event.target.value);
        }}
      />
      <SelectField
        label="类型"
        required
        value={kind}
        onChange={(event) => {
          setKind(event.target.value as HarnessKind);
        }}
      >
        <option value="claude_code">Claude Code</option>
        <option value="codex">Codex</option>
        <option value="mock">Mock（测试用）</option>
      </SelectField>
      <InputField
        label="可执行文件路径"
        hint="可留空；留空则使用本机检测到的路径，填写时须为绝对路径。"
        value={executable}
        onChange={(event) => {
          setExecutable(event.target.value);
        }}
      />
      <InputField
        label="模型"
        hint="可留空，原样传给 CLI。"
        value={model}
        onChange={(event) => {
          setModel(event.target.value);
        }}
      />
      <InputField
        label="Profile"
        hint="Claude Code 为 settings 文件路径，Codex 为 profile 名。"
        value={profile}
        onChange={(event) => {
          setProfile(event.target.value);
        }}
      />

      {error === null ? null : <ErrorBanner error={error} />}

      <div className="harness-create__actions">
        <Button type="submit" loading={isPending}>
          注册实例
        </Button>
        <Button variant="neutral" onClick={onCancel}>
          取消
        </Button>
      </div>
    </form>
  );
}
