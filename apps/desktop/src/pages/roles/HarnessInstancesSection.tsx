import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";

import { checkHarness, createHarness, listAcpPresets } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import type { AcpPreset } from "../../bindings/AcpPreset.js";
import type { AcpPresetInfo } from "../../bindings/AcpPresetInfo.js";
import type { CreateHarnessInput } from "../../bindings/CreateHarnessInput.js";
import type { HarnessInstance } from "../../bindings/HarnessInstance.js";
import type { RoleView } from "../../bindings/RoleView.js";
import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  InputField,
  Panel,
  Spinner,
  Tag,
} from "../../components/index.js";
import {
  TEST_HARNESS_LABEL,
  agentVersion,
  capabilityLabels,
  commandLine,
  formatDateTime,
  healthLabel,
  healthTone,
  isTestHarness,
  presetTagLabel,
  protocolVersionLabel,
  splitArgs,
} from "../harness-detail/harnessFormat.js";

export interface HarnessInstancesSectionProps {
  teamId: string;
  harnesses: HarnessInstance[];
  /** 岗位列表：用于标出每个接入被哪些岗位绑定。 */
  roles: RoleView[];
  isPending: boolean;
  error: Error | null;
  onRetry: () => void;
}

/** 「Harness 接入」区：ACP 说明、接入列表（握手检查、编辑）与两步新建接入。 */
export function HarnessInstancesSection({
  teamId,
  harnesses,
  roles,
  isPending,
  error,
  onRetry,
}: HarnessInstancesSectionProps) {
  const [creating, setCreating] = useState(false);
  const navigate = useNavigate();
  const detailPath = (harnessId: string) => `/teams/${teamId}/harnesses/${harnessId}`;

  return (
    <div className="acp-section">
      <Card
        tone="info"
        title="所有 Harness 通过 ACP（Agent Client Protocol）标准协议接入"
        className="acp-banner"
      >
        <p>
          会话创建、权限请求、异步执行进度都按同一协议进行。平台不保存任何密钥，登录由各工具自己管理。
        </p>
      </Card>

      <Panel className="harness-section">
        <div className="harness-section__head">
          <h2 className="harness-section__title">接入列表</h2>
          <Button
            variant={creating ? "neutral" : "primary"}
            onClick={() => {
              setCreating(!creating);
            }}
          >
            {creating ? "收起新建接入" : "新建接入"}
          </Button>
        </div>

        {isPending ? <Spinner label="正在加载接入列表" /> : null}

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
            bare
            title="本团队还没有接入"
            description="新建接入后即可为岗位绑定执行者；Mock 接入用于本地验证，不消耗额度。"
          />
        ) : null}

        <div className="acp-list">
          {harnesses.map((instance) => (
            <HarnessEntry
              key={instance.id}
              instance={instance}
              boundRoles={roles
                .filter((view) => view.role.harnessId === instance.id)
                .map((view) => view.role.name)}
              onOpenDetail={() => {
                void navigate(detailPath(instance.id));
              }}
            />
          ))}
        </div>
      </Panel>

      {creating ? (
        <HarnessCreateSection
          teamId={teamId}
          onClose={() => {
            setCreating(false);
          }}
        />
      ) : null}
    </div>
  );
}

interface HarnessEntryProps {
  instance: HarnessInstance;
  boundRoles: string[];
  onOpenDetail: () => void;
}

/** 接入列表的一行：名称、预设、握手状态、启动命令、Agent 版本、能力与被绑定岗位。 */
function HarnessEntry({ instance, boundRoles, onOpenDetail }: HarnessEntryProps) {
  const queryClient = useQueryClient();
  const handshake = instance.handshake;
  const protocol = protocolVersionLabel(handshake);
  const capabilities = capabilityLabels(handshake.capabilities);

  const check = useMutation({
    mutationFn: (id: string) => checkHarness(id),
    onSuccess: (updated) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.harnesses(updated.teamId) });
    },
  });

  return (
    <article className="acp-entry">
      <div className="acp-entry__head">
        <span className="acp-entry__name">
          {instance.name}
          {isTestHarness(instance.preset) ? `（${TEST_HARNESS_LABEL}）` : ""}
        </span>
        <Tag tone={isTestHarness(instance.preset) ? "warning" : "info"}>
          {presetTagLabel(instance.preset)}
        </Tag>
        <Tag tone={healthTone(handshake.status)}>{healthLabel(handshake.status)}</Tag>
        {protocol === null ? null : <span className="acp-entry__protocol">{protocol}</span>}
        <span className="acp-entry__actions">
          <Button
            variant="neutral"
            loading={check.isPending && check.variables === instance.id}
            onClick={() => {
              check.mutate(instance.id);
            }}
          >
            握手检查
          </Button>
          <Button variant="neutral" onClick={onOpenDetail}>
            编辑
          </Button>
        </span>
      </div>

      {handshake.status === "unknown" && handshake.detail === null ? null : (
        <p className="acp-entry__detail">{handshake.detail ?? "尚未握手。"}</p>
      )}

      <p className="acp-entry__command">
        <span className="nowrap">启动命令</span>
        <code className="acp-entry__code">{commandLine(instance.command, instance.args)}</code>
      </p>

      <p className="acp-entry__meta">
        <span className="nowrap">Agent 版本</span>
        <span>{agentVersion(handshake) ?? "—"}</span>
        {capabilities.length === 0 ? null : (
          <>
            <span className="nowrap">能力</span>
            {capabilities.map((label) => (
              <Tag key={label} tone="neutral">
                {label}
              </Tag>
            ))}
          </>
        )}
        <span className="nowrap">被绑定</span>
        <span>{boundRoles.length === 0 ? "暂未被任何岗位绑定" : boundRoles.join("、")}</span>
        {handshake.checkedAt === null ? null : (
          <span className="acp-entry__checked">{formatDateTime(handshake.checkedAt)}</span>
        )}
      </p>

      {check.error === null ? null : <ErrorBanner error={check.error} />}
    </article>
  );
}

interface HarnessCreateSectionProps {
  teamId: string;
  onClose: () => void;
}

/**
 * 「新建接入」两步：先选预设（带本机检测结果），再填启动配置。
 * 保存时先 `create_harness` 拿到实例，再 `check_harness` 做一次握手检查。
 */
function HarnessCreateSection({ teamId, onClose }: HarnessCreateSectionProps) {
  const queryClient = useQueryClient();

  const presets = useQuery({ queryKey: queryKeys.acpPresets, queryFn: listAcpPresets });

  const [preset, setPreset] = useState<AcpPreset | null>(null);
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [envProfile, setEnvProfile] = useState("");

  const selected: AcpPresetInfo | null =
    presets.data?.find((info) => info.preset === preset) ?? null;

  const trimmed = (value: string) => (value.trim() === "" ? null : value.trim());

  /** 选中预设时预填默认命令与默认参数；再次点击同一张卡片则取消选择。 */
  const choosePreset = (info: AcpPresetInfo) => {
    if (preset === info.preset) {
      setPreset(null);
      return;
    }
    setPreset(info.preset);
    setCommand(info.defaultCommand ?? "");
    setArgs(info.defaultArgs.join(" "));
    setName(`${info.label}（${info.defaultCommand ?? "自定义"}）`);
  };

  const save = useMutation({
    mutationFn: async (input: CreateHarnessInput) => {
      const created = await createHarness(input);
      return checkHarness(created.id);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.harnesses(teamId) });
      onClose();
    },
  });

  const trimmedName = name.trim();
  const commandRequired = preset === "custom";

  return (
    <Panel className="acp-create">
      <div className="acp-create__head">
        <h2 className="harness-section__title">新建接入</h2>
        <p className="acp-create__steps">两步：选择预设，再确认配置</p>
      </div>

      {presets.isPending ? <Spinner label="正在检测本机可用 Agent" /> : null}
      {presets.error === null ? null : <ErrorBanner error={presets.error} />}

      <div className="acp-create__body">
        <section className="acp-create__presets" aria-labelledby="acp-presets-heading">
          <h3 className="acp-create__step" id="acp-presets-heading">
            第一步 选择预设
          </h3>
          <div className="acp-preset-grid">
            {(presets.data ?? []).map((info) => (
              <button
                key={info.preset}
                type="button"
                className="acp-preset"
                aria-pressed={preset === info.preset}
                onClick={() => {
                  choosePreset(info);
                }}
              >
                <span className="acp-preset__name">{info.label}</span>
                <span className="acp-preset__desc">{info.description}</span>
                <span className="acp-preset__detect">{presetStateLabel(info)}</span>
                {info.hint === null ? null : <span className="acp-preset__hint">{info.hint}</span>}
              </button>
            ))}
          </div>
        </section>

        <form
          className="acp-create__form"
          onSubmit={(event) => {
            event.preventDefault();
            if (preset === null) {
              return;
            }
            save.mutate({
              teamId,
              name: trimmedName,
              preset,
              command: trimmed(command),
              args: args.trim() === "" ? null : splitArgs(args),
              envProfile: trimmed(envProfile),
            });
          }}
        >
          <h3 className="acp-create__step">
            {selected === null ? "第二步 确认配置" : `第二步 确认配置（已选：${selected.label}）`}
          </h3>

          <InputField
            label="实例名称"
            required
            hint="团队内唯一，如 frontend-local。"
            value={name}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
          <InputField
            label="启动命令"
            required={commandRequired}
            hint={
              commandRequired
                ? "自定义 ACP Agent 必填；命令名或绝对路径。"
                : "已按预设预填，可改成绝对路径。"
            }
            value={command}
            onChange={(event) => {
              setCommand(event.target.value);
            }}
          />
          <InputField
            label="参数"
            hint="（可选）空格分隔，如 --verbose"
            value={args}
            onChange={(event) => {
              setArgs(event.target.value);
            }}
          />
          <InputField
            label="环境配置档案（可选）"
            hint="选择或填写配置文件路径；只记录路径，不保存密钥。"
            value={envProfile}
            onChange={(event) => {
              setEnvProfile(event.target.value);
            }}
          />
          <p className="acp-create__note">
            工作目录：执行时由 Teamspace 按任务自动设置为岗位的隔离目录，无需填写。
          </p>

          {save.error === null ? null : <ErrorBanner error={save.error} />}

          <div className="harness-create__actions">
            <Button
              type="submit"
              disabled={
                preset === null || trimmedName === "" || (commandRequired && command.trim() === "")
              }
              loading={save.isPending}
            >
              保存并握手检查
            </Button>
            <Button variant="neutral" onClick={onClose}>
              取消
            </Button>
          </div>
        </form>
      </div>
    </Panel>
  );
}

/** 预设卡片上的本机检测标签：已检测到 / 未检测到 / 内置 / 自由配置。 */
function presetStateLabel(info: AcpPresetInfo): string {
  switch (info.preset) {
    case "mock":
      return "内置";
    case "custom":
      return "自由配置";
    case "claude_code":
    case "codex":
    case "minimax_code":
    case "gemini_cli":
      return info.commandFound ? "本机已检测到" : "未检测到";
  }
}
