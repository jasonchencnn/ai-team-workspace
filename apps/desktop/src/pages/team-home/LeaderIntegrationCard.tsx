// 「接入 Leader」卡片（M4-03）：为外部会话（Claude Code / Codex）生成 MCP 配置并可写入工作空间。
// 三段配置文本由内核生成（leader_integration），这里只负责展示、复制与写入前的确认。
import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { leaderIntegration, writeWorkspaceMcpConfig } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import { Button, ErrorBanner, Modal, Panel, Spinner, Tag } from "../../components/index.js";
import {
  type CopyState,
  COPY_FAILED_HINT,
  LEADER_CAN_DO,
  LEADER_CANNOT_DO,
  canWriteMcpConfig,
  configSections,
  copyLabel,
  gitNotice,
  writeBlockedHint,
  writeConfirmText,
  writeDoneText,
} from "./leaderIntegrationUi.js";
import "./leader-integration.css";

export function LeaderIntegrationCard({ teamId }: { teamId: string }) {
  const [copied, setCopied] = useState<Record<string, CopyState>>({});
  const [confirming, setConfirming] = useState(false);
  const [written, setWritten] = useState<string | null>(null);

  const integration = useQuery({
    queryKey: queryKeys.leaderIntegration(teamId),
    queryFn: () => leaderIntegration(teamId),
    enabled: teamId !== "",
  });

  const write = useMutation({
    mutationFn: () => writeWorkspaceMcpConfig(teamId),
    onSuccess: (data) => {
      setConfirming(false);
      setWritten(writeDoneText(data.path, data.created));
    },
    onError: () => {
      setConfirming(false);
    },
  });

  if (integration.isPending) {
    return (
      <Panel className="li-panel">
        <Spinner label="正在生成接入配置" />
      </Panel>
    );
  }

  if (integration.isError) {
    return (
      <Panel className="li-panel">
        <ErrorBanner
          error={integration.error}
          action={
            <Button
              variant="neutral"
              onClick={() => {
                void integration.refetch();
              }}
            >
              重试
            </Button>
          }
        />
      </Panel>
    );
  }

  const data = integration.data;
  const sections = configSections(data);
  const notice = gitNotice(data.gitWorkspace);
  const writable = canWriteMcpConfig(data);
  const blocked = writeBlockedHint(data);

  const copy = async (key: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied((state) => ({ ...state, [key]: "ok" }));
    } catch {
      setCopied((state) => ({ ...state, [key]: "failed" }));
    }
  };

  return (
    <Panel className="li-panel">
      <div className="li-head">
        <h2 className="li-title">接入 Leader</h2>
        {data.binaryFound ? null : <Tag tone="warning">未构建 teamspace-mcp</Tag>}
      </div>

      <p className="li-note">
        Leader 是你在工作空间目录里打开的 Claude Code 或 Codex。接入后，它通过 teamspace-mcp
        这个本地 MCP 服务操作这个团队；服务只是转发，真正的执行仍由 Teamspace 应用负责。
      </p>

      <div className="li-lists">
        <div className="li-list">
          <h3 className="li-list__title">Leader 能做</h3>
          <ul className="li-list__items">
            {LEADER_CAN_DO.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
        <div className="li-list">
          <h3 className="li-list__title">只能你在应用里做</h3>
          <ul className="li-list__items">
            {LEADER_CANNOT_DO.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
      </div>

      {data.binaryHint === null ? null : <p className="li-warn">{data.binaryHint}</p>}

      <div className="li-sections">
        {sections.map((section) => {
          const state = copied[section.key] ?? "idle";
          return (
            <div className="li-section" key={section.key}>
              <div className="li-section__head">
                <h3 className="li-section__title">{section.title}</h3>
                <Button
                  variant="neutral"
                  onClick={() => {
                    void copy(section.key, section.text);
                  }}
                >
                  {copyLabel(state)}
                </Button>
              </div>
              <p className="li-note">{section.hint}</p>
              <pre className="li-code">{section.text}</pre>
              {state === "failed" ? <p className="li-note">{COPY_FAILED_HINT}</p> : null}
            </div>
          );
        })}
      </div>

      <div className="li-actions">
        <Button
          disabled={!writable}
          title={blocked ?? undefined}
          onClick={() => {
            setConfirming(true);
          }}
        >
          写入工作空间 .mcp.json
        </Button>
        <span className="li-note">工作空间：{data.workspacePath}</span>
      </div>

      {blocked === null ? null : <p className="li-warn">{blocked}</p>}
      {notice === null ? null : <p className="li-note">{notice}</p>}
      {written === null ? null : <p className="li-done">{written}</p>}
      {write.error === null ? null : <ErrorBanner error={write.error} />}

      {confirming ? (
        <Modal
          title="写入工作空间 .mcp.json"
          description={writeConfirmText(data)}
          onClose={() => {
            setConfirming(false);
          }}
          actions={
            <>
              <Button
                variant="neutral"
                onClick={() => {
                  setConfirming(false);
                }}
              >
                取消
              </Button>
              <Button
                loading={write.isPending}
                onClick={() => {
                  write.mutate();
                }}
              >
                确认写入
              </Button>
            </>
          }
        >
          <p className="li-note">
            写入的是上面第二段配置里的 teamspace 一项，不会改动文件里的其他服务。
          </p>
        </Modal>
      ) : null}
    </Panel>
  );
}
