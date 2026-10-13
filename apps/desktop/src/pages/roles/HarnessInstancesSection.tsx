import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";

import { createHarness, listAcpPresets } from "../../api/commands.js";
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
import type { AcpPreset } from "../../bindings/AcpPreset.js";
import type { CreateHarnessInput } from "../../bindings/CreateHarnessInput.js";
import {
  TEST_HARNESS_LABEL,
  agentVersion,
  commandLine,
  formatRelative,
  formatVersion,
  harnessKindLabel,
  healthLabel,
  healthTone,
  isTestHarness,
  splitArgs,
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
                  {harnessKindLabel(instance.preset)}
                  {isTestHarness(instance.preset) ? `（${TEST_HARNESS_LABEL}）` : ""} /{" "}
                  {instance.name}
                </span>
              }
              tone={instance.handshake.status === "error" ? "warning" : "default"}
              className="harness-card__inner"
            >
              <p className="harness-card__meta">
                版本：{formatVersion(agentVersion(instance.handshake))}
              </p>
              <p className="harness-card__meta">
                <Tag tone={healthTone(instance.handshake.status)}>
                  {healthLabel(instance.handshake.status)}
                </Tag>
                <span className="harness-card__checked">
                  最近检查 {formatRelative(instance.handshake.checkedAt, Date.now())}
                </span>
              </p>
              <p className="harness-card__config">
                启动命令：{commandLine(instance.command, instance.args)}
              </p>
              {instance.handshake.status !== "connected" && instance.handshake.detail !== null ? (
                <p className="harness-card__detail">{instance.handshake.detail}</p>
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

/** 「检测本机 ACP Agent」：按需查询，用户点击后才调用。 */
function HarnessDetection() {
  const detection = useQuery({
    queryKey: queryKeys.acpPresets,
    queryFn: listAcpPresets,
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
        检测本机 ACP Agent
      </Button>

      {detection.error === null ? null : <ErrorBanner error={detection.error} />}

      {detection.data === undefined ? null : (
        <ul className="harness-detection__list">
          {detection.data.map((item) => (
            <li key={item.preset} className="harness-detection__item">
              <Tag tone={item.commandFound ? "success" : "warning"}>{item.label}</Tag>
              <span className="harness-detection__text">
                {item.hint ??
                  (item.commandFound ? `已找到 ${item.commandPath ?? ""}` : item.description)}
              </span>
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
  onSubmit: (input: CreateHarnessInput) => void;
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
  const [preset, setPreset] = useState<AcpPreset>("claude_code");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [envProfile, setEnvProfile] = useState("");

  const trimmed = (value: string) => (value.trim() === "" ? null : value.trim());

  return (
    <form
      className="harness-create"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit({
          teamId,
          name: name.trim(),
          preset,
          command: trimmed(command),
          args: args.trim() === "" ? null : splitArgs(args),
          envProfile: trimmed(envProfile),
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
        value={preset}
        onChange={(event) => {
          setPreset(event.target.value as AcpPreset);
        }}
      >
        <option value="claude_code">Claude Code</option>
        <option value="codex">Codex</option>
        <option value="minimax_code">MiniMax Code</option>
        <option value="gemini_cli">Gemini CLI</option>
        <option value="custom">自定义 ACP Agent</option>
        <option value="mock">Mock（测试用）</option>
      </SelectField>
      <InputField
        label="启动命令"
        hint="可留空；留空则使用预设默认命令（自定义 ACP Agent 必填）。"
        value={command}
        onChange={(event) => {
          setCommand(event.target.value);
        }}
      />
      <InputField
        label="参数"
        hint="可留空；空格分隔，留空则使用预设默认参数。"
        value={args}
        onChange={(event) => {
          setArgs(event.target.value);
        }}
      />
      <InputField
        label="环境配置档案"
        hint="可留空；填写时须为绝对路径，只记录路径，不保存密钥。"
        value={envProfile}
        onChange={(event) => {
          setEnvProfile(event.target.value);
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
