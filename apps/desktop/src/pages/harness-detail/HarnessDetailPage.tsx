import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate, useParams } from "react-router";

import { archiveHarness, checkHarness, listHarnesses, updateHarness } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  EmptyState,
  ErrorBanner,
  InputField,
  PageHeader,
  Panel,
  SelectField,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { HarnessInstance } from "../../bindings/HarnessInstance.js";
import { harnessListPath } from "../roles/RolesPage.js";
import {
  TEST_HARNESS_LABEL,
  formatDateTime,
  formatVersion,
  harnessKindLabel,
  agentVersion,
  healthLabel,
  healthTone,
  splitArgs,
  isTestHarness,
} from "./harnessFormat.js";
import "./harness-detail.css";

export function HarnessDetailPage() {
  const { teamId, harnessId } = useParams();
  const navigate = useNavigate();

  const queryClient = useQueryClient();
  const harnessesQuery = useQuery({
    queryKey: queryKeys.harnesses(teamId ?? ""),
    queryFn: () => listHarnesses(teamId ?? ""),
    enabled: teamId !== undefined,
  });

  const harness = harnessesQuery.data?.find((item) => item.id === harnessId) ?? null;

  const checkMutation = useMutation({
    mutationFn: (id: string) => checkHarness(id),
    onSuccess: () => {
      if (teamId !== undefined) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.harnesses(teamId) });
      }
    },
  });

  // 「返回」落回岗位页的 Harness 实例视图，与页头文案一致。
  const backToHarnesses = () => {
    if (teamId !== undefined) {
      void navigate(harnessListPath(teamId));
    }
  };

  const backButton = (
    <Button variant="neutral" onClick={backToHarnesses}>
      返回实例列表
    </Button>
  );

  if (teamId === undefined) {
    return <EmptyState title="未选择团队" description="请先在左侧选择一个团队。" />;
  }

  if (harnessesQuery.isPending) {
    return (
      <>
        <PageHeader title="Harness 注册与健康检查" actions={backButton} />
        <Panel>
          <Spinner label="正在加载实例" />
        </Panel>
      </>
    );
  }

  if (harnessesQuery.error !== null) {
    return (
      <>
        <PageHeader title="Harness 注册与健康检查" actions={backButton} />
        <ErrorBanner
          error={harnessesQuery.error}
          action={
            <Button
              variant="neutral"
              onClick={() => {
                void harnessesQuery.refetch();
              }}
            >
              重试
            </Button>
          }
        />
      </>
    );
  }

  if (harness === null) {
    return (
      <>
        <PageHeader title="Harness 注册与健康检查" actions={backButton} />
        <EmptyState
          title="找不到该实例"
          description="它可能已被归档。"
          action={
            <Button variant="neutral" onClick={backToHarnesses}>
              返回实例列表
            </Button>
          }
        />
      </>
    );
  }

  return (
    <HarnessDetailView
      key={harness.id}
      teamId={teamId}
      harness={harness}
      onBack={backToHarnesses}
      onCheck={(id) => {
        checkMutation.mutate(id);
      }}
      isChecking={checkMutation.isPending}
      checkError={checkMutation.error}
    />
  );
}

interface HarnessDetailViewProps {
  teamId: string;
  harness: HarnessInstance;
  onBack: () => void;
  onCheck: (harnessId: string) => void;
  isChecking: boolean;
  checkError: Error | null;
}

function HarnessDetailView({
  teamId,
  harness,
  onBack,
  onCheck,
  isChecking,
  checkError,
}: HarnessDetailViewProps) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(harness.name);
  const [command, setCommand] = useState(harness.command);
  const [args, setArgs] = useState(harness.args.join(" "));
  const [envProfile, setEnvProfile] = useState(harness.envProfile ?? "");

  const trimmed = (value: string) => (value.trim() === "" ? null : value.trim());

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.harnesses(teamId) });
  };

  const saveMutation = useMutation({
    mutationFn: () =>
      updateHarness({
        harnessId: harness.id,
        name: name.trim(),
        command: trimmed(command),
        args: splitArgs(args),
        envProfile: trimmed(envProfile),
      }),
    onSuccess: invalidate,
  });

  const archiveMutation = useMutation({
    mutationFn: () => archiveHarness(harness.id),
    onSuccess: () => {
      invalidate();
      onBack();
    },
  });

  const health = harness.handshake;

  const dirty =
    name !== harness.name ||
    harness.command !== command ||
    harness.args.join(" ") !== args ||
    (harness.envProfile ?? "") !== envProfile;

  return (
    <div className="harness-detail">
      <PageHeader
        title={`${harness.name} · Harness 设置`}
        description="实例可替换，岗位身份与权限保持稳定；敏感凭据只保留安全引用。"
        actions={
          <Button variant="neutral" onClick={onBack}>
            返回实例列表
          </Button>
        }
      />

      <div className="harness-detail__columns">
        <Panel className="harness-detail__connection">
          <h2 className="harness-detail__title">连接设置</h2>

          <InputField
            label="实例名称"
            required
            value={name}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
          <SelectField label="类型" value={harness.preset} disabled hint="创建后不可修改。">
            <option value={harness.preset}>
              {harnessKindLabel(harness.preset)}
              {isTestHarness(harness.preset) ? `（${TEST_HARNESS_LABEL}）` : ""}
            </option>
          </SelectField>
          <InputField
            label="启动命令"
            hint="命令名或绝对路径；留空则使用预设默认命令。"
            value={command}
            onChange={(event) => {
              setCommand(event.target.value);
            }}
          />
          <InputField
            label="参数"
            hint="空格分隔。"
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

          {saveMutation.error === null ? null : <ErrorBanner error={saveMutation.error} />}

          <Button
            disabled={!dirty}
            loading={saveMutation.isPending}
            onClick={() => {
              saveMutation.mutate();
            }}
          >
            保存连接
          </Button>
        </Panel>

        <Panel className="harness-detail__health">
          <h2 className="harness-detail__title">健康检查结果</h2>

          <p className="harness-detail__instance">
            实例 {harness.name} · {harnessKindLabel(harness.preset)}
            {isTestHarness(harness.preset) ? `（${TEST_HARNESS_LABEL}）` : ""}
          </p>

          <div className="harness-detail__health-head">
            <Tag tone={healthTone(health.status)}>{healthLabel(health.status)}</Tag>
            <span className="harness-detail__checked">
              {health.checkedAt === null ? "从未检查" : formatDateTime(health.checkedAt)}
            </span>
          </div>

          {health.detail === null ? null : (
            <p className="harness-detail__detail">{health.detail}</p>
          )}

          <p className="harness-detail__version">版本：{formatVersion(agentVersion(health))}</p>

          {checkError === null ? null : <ErrorBanner error={checkError} />}

          <Button
            variant="neutral"
            loading={isChecking}
            onClick={() => {
              onCheck(harness.id);
            }}
          >
            重新健康检查
          </Button>
        </Panel>
      </div>

      <Panel className="harness-detail__scope">
        <h2 className="harness-detail__title">安全与生效范围</h2>
        <p className="harness-detail__scope-text">
          平台不保存任何密钥，登录由各 CLI 自己管理。保存连接只更新实例配置，不改变岗位和任务历史。
        </p>
      </Panel>

      {archiveMutation.error === null ? null : <ErrorBanner error={archiveMutation.error} />}

      <div className="harness-detail__actions">
        <Button
          variant="neutral"
          loading={archiveMutation.isPending}
          onClick={() => {
            archiveMutation.mutate();
          }}
        >
          归档实例
        </Button>
      </div>
    </div>
  );
}
