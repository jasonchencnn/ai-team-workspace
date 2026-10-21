import { useState } from "react";

import type { HarnessInstance } from "../../bindings/HarnessInstance.js";
import type { RebindMode } from "../../bindings/RebindMode.js";
import type { Role } from "../../bindings/Role.js";
import type { RoleView } from "../../bindings/RoleView.js";
import {
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  Modal,
  SelectField,
} from "../../components/index.js";
import {
  TEST_HARNESS_LABEL,
  harnessKindLabel,
  healthLabel,
  isTestHarness,
} from "../harness-detail/harnessFormat.js";
import { DEFAULT_REBIND_MODE, REBIND_MODE_OPTIONS, rebindCandidates } from "./rebindUi.js";
import type { ActiveRunEntry } from "./useRebindData.js";

export interface RebindHarnessDialogProps {
  role: Role;
  view: RoleView;
  harnesses: HarnessInstance[];
  /** 该岗位进行中的执行；空数组表示没有在跑的活。 */
  activeRuns: ActiveRunEntry[];
  isRunsPending: boolean;
  onClose: () => void;
  onConfirm: (harnessId: string, mode: RebindMode) => void;
  isSubmitting: boolean;
  error: Error | null;
}

/**
 * 更换 Harness 的流程弹窗（Figma 6:403）。
 *
 * 岗位身份、权限、任务与历史都不动，只换执行用的 Harness 实例。
 * 岗位有进行中的执行时必须选处理方式——这一步由 Human 拍板，
 * 界面只给选项与后果，不替用户做决定。
 */
export function RebindHarnessDialog({
  role,
  view,
  harnesses,
  activeRuns,
  isRunsPending,
  onClose,
  onConfirm,
  isSubmitting,
  error,
}: RebindHarnessDialogProps) {
  const [targetId, setTargetId] = useState("");
  const [mode, setMode] = useState<RebindMode>(DEFAULT_REBIND_MODE);

  const current = view.harness;
  const candidates = rebindCandidates(harnesses, current?.id ?? null);
  const target = candidates.find((instance) => instance.id === targetId);
  const hasActiveRuns = activeRuns.length > 0;
  const selectedOption =
    REBIND_MODE_OPTIONS.find((option) => option.mode === mode) ?? REBIND_MODE_OPTIONS[0];

  return (
    <Modal
      title="更换 Harness"
      description={`${role.name} / ${role.code}`}
      ariaLabel="更换 Harness"
      onClose={onClose}
      actions={
        <>
          <Button variant="neutral" onClick={onClose}>
            取消
          </Button>
          <Button
            disabled={target === undefined}
            loading={isSubmitting}
            onClick={() => {
              if (target === undefined) {
                return;
              }
              onConfirm(target.id, mode);
            }}
          >
            确认更换
          </Button>
        </>
      }
    >
      <div className="rebind">
        <p className="rebind__note">岗位身份、权限、任务与历史继续保留。</p>

        {candidates.length === 0 ? (
          <EmptyState
            title="没有可更换的实例"
            description="本团队只有当前绑定的这一个实例，请先在「Harness 接入」页签注册新的实例。"
          />
        ) : (
          <>
            <SelectField
              label="选择新的 Harness 实例"
              value={targetId}
              onChange={(event) => {
                setTargetId(event.target.value);
              }}
            >
              <option value="">请选择实例</option>
              {candidates.map((instance) => (
                <option key={instance.id} value={instance.id}>
                  {harnessKindLabel(instance.preset)}
                  {isTestHarness(instance.preset) ? `（${TEST_HARNESS_LABEL}）` : ""} /{" "}
                  {instance.name}（{healthLabel(instance.handshake.status)}）
                </option>
              ))}
            </SelectField>

            {target !== undefined ? (
              <Card title={`${current?.name ?? "未绑定"} → ${target.name}`} tone="info">
                <p className="rebind__transition-meta">
                  当前 {current?.name ?? "未绑定"}
                  {current === null ? "" : ` · ${healthLabel(current.handshake.status)}`}
                </p>
                <p className="rebind__transition-meta">
                  接着 {harnessKindLabel(target.preset)} · {target.name} ·{" "}
                  {healthLabel(target.handshake.status)}
                </p>
                {isTestHarness(target.preset) ? (
                  <p className="rebind__test-hint">
                    这是测试用实例：仅用于验证流程，不要绑定正式岗位。
                  </p>
                ) : null}
              </Card>
            ) : null}
          </>
        )}

        {isRunsPending ? <p className="rebind__note">正在确认该岗位是否有进行中的执行…</p> : null}

        {!isRunsPending && hasActiveRuns ? (
          <>
            <h3 className="rebind__section-title">有 {String(activeRuns.length)} 个任务正在执行</h3>
            <ul className="rebind__runs">
              {activeRuns.map((entry) => (
                <li key={entry.run.id} className="rebind__run">
                  <span className="rebind__run-key">{entry.taskKey}</span>
                  <span className="rebind__run-title">{entry.taskTitle}</span>
                  <span className="rebind__run-attempt">第 {String(entry.run.attempt)} 次执行</span>
                  <span className="rebind__run-status">{entry.run.statusLabel}</span>
                </li>
              ))}
            </ul>
            <p className="rebind__note">请选择当前执行的处理方式。</p>

            <fieldset className="rebind__modes">
              <legend className="rebind__modes-legend">换绑方式</legend>
              {REBIND_MODE_OPTIONS.map((option) => (
                <label key={option.mode} className="rebind__mode">
                  <input
                    type="radio"
                    name="rebind-mode"
                    className="rebind__mode-input"
                    value={option.mode}
                    checked={mode === option.mode}
                    onChange={() => {
                      setMode(option.mode);
                    }}
                  />
                  <span className="rebind__mode-body">
                    <span className="rebind__mode-label">
                      {option.label}
                      {option.recommended ? "（推荐）" : ""}
                    </span>
                    <span className="rebind__mode-consequence">{option.consequence}</span>
                  </span>
                </label>
              ))}
            </fieldset>
          </>
        ) : null}

        {!isRunsPending && !hasActiveRuns ? (
          <p className="rebind__note">该岗位当前没有进行中的执行，确认后立即换绑。</p>
        ) : null}

        {hasActiveRuns ? (
          <p className="rebind__policy">{selectedOption?.consequence ?? ""}</p>
        ) : null}

        {error === null ? null : <ErrorBanner error={error} />}
      </div>
    </Modal>
  );
}
