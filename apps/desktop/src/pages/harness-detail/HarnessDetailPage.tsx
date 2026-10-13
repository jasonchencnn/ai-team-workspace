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
import { harnessListPath } from "../roles/rolesNav.js";
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
} from "./harnessFormat.js";
import "./harness-detail.css";

/** 载入中、载入失败与找不到实例时共用同一个页头，避免三种状态各写一份。 */
const PAGE_TITLE = "Harness 接入";

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

  // 「返回」落回岗位页的 Harness 接入页签，与页头文案一致。
  const backToHarnesses = () => {
    if (teamId !== undefined) {
      void navigate(harnessListPath(teamId));
    }
  };

  const backButton = (
    <Button variant="neutral" onClick={backToHarnesses}>
      返回接入列表
    </Button>
  );

  if (teamId === undefined) {
    return <EmptyState title="未选择团队" description="请先在左侧选择一个团队。" />;
  }

  if (harnessesQuery.isPending) {
    return (
      <>
        <PageHeader title={PAGE_TITLE} actions={backButton} />
        <Panel>
          <Spinner label="正在加载接入" />
        </Panel>
      </>
    );
  }

  if (harnessesQuery.error !== null) {
    return (
      <>
        <PageHeader title={PAGE_TITLE} actions={backButton} />
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
        <PageHeader title={PAGE_TITLE} actions={backButton} />
        <EmptyState
          title="找不到该接入"
          description="它可能已被归档。"
          action={
            <Button variant="neutral" onClick={backToHarnesses}>
              返回接入列表
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
  const protocol = protocolVersionLabel(health);
  const capabilities = capabilityLabels(health.capabilities);
  const agent = health.agent;

  const dirty =
    name !== harness.name ||
    harness.command !== command ||
    harness.args.join(" ") !== args ||
    (harness.envProfile ?? "") !== envProfile;

  return (
    <div className="harness-detail">
      <PageHeader
        title={`${harness.name} · ACP 接入设置`}
        description="接入可替换，岗位身份与权限保持稳定；敏感凭据只保留安全引用。"
        actions={
          <Button variant="neutral" onClick={onBack}>
            返回接入列表
          </Button>
        }
      />

      <div className="harness-detail__columns">
        <Panel className="harness-detail__connection">
          <h2 className="harness-detail__title">启动配置</h2>

          <InputField
            label="实例名称"
            required
            value={name}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
          <SelectField label="预设" value={harness.preset} disabled hint="创建后不可修改。">
            <option value={harness.preset}>
              {presetTagLabel(harness.preset)}
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
            保存启动配置
          </Button>
        </Panel>

        <Panel className="harness-detail__health">
          <h2 className="harness-detail__title">握手结果</h2>

          <p className="harness-detail__instance">
            实例 {harness.name} · {presetTagLabel(harness.preset)}
            {isTestHarness(harness.preset) ? `（${TEST_HARNESS_LABEL}）` : ""}
          </p>

          <div className="harness-detail__health-head">
            <Tag tone={healthTone(health.status)}>{healthLabel(health.status)}</Tag>
            <span className="harness-detail__checked">
              {health.checkedAt === null ? "从未检查" : formatDateTime(health.checkedAt)}
            </span>
          </div>

          <dl className="harness-detail__facts">
            <div className="harness-detail__fact">
              <dt className="harness-detail__key">协议版本</dt>
              <dd className="harness-detail__value">{protocol ?? "—"}</dd>
            </div>
            <div className="harness-detail__fact">
              <dt className="harness-detail__key">Agent 名称</dt>
              <dd className="harness-detail__value">{agent?.name ?? "—"}</dd>
            </div>
            <div className="harness-detail__fact">
              <dt className="harness-detail__key">Agent 版本</dt>
              <dd className="harness-detail__value">{agentVersion(health) ?? "—"}</dd>
            </div>
            <div className="harness-detail__fact">
              <dt className="harness-detail__key">登录方式</dt>
              <dd className="harness-detail__value">
                {health.authMethods.length === 0 ? "—" : health.authMethods.join("、")}
              </dd>
            </div>
            <div className="harness-detail__fact">
              <dt className="harness-detail__key">能力</dt>
              <dd className="harness-detail__value harness-detail__capabilities">
                {capabilities.length === 0 ? "—" : capabilities.join("、")}
              </dd>
            </div>
          </dl>

          {health.detail === null ? null : (
            <p className="harness-detail__detail">{health.detail}</p>
          )}

          <p className="harness-detail__version">
            启动命令：{commandLine(harness.command, harness.args)}
          </p>

          {checkError === null ? null : <ErrorBanner error={checkError} />}

          <Button
            variant="neutral"
            loading={isChecking}
            onClick={() => {
              onCheck(harness.id);
            }}
          >
            握手检查
          </Button>
        </Panel>
      </div>

      <Panel className="harness-detail__scope">
        <h2 className="harness-detail__title">安全与生效范围</h2>
        <p className="harness-detail__scope-text">
          平台不保存任何密钥，登录由各 Agent
          自己管理。保存启动配置只更新接入配置，握手状态会重置为「未检查」，不改变岗位和任务历史。
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
          归档接入
        </Button>
      </div>
    </div>
  );
}
