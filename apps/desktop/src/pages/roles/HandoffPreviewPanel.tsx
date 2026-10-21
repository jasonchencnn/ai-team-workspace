import { useState } from "react";

import { Button, Panel } from "../../components/index.js";
import { handoffEmptyText } from "./rebindUi.js";

export interface HandoffPreviewPanelProps {
  /** 启动包正文；空串表示不会附加交接。 */
  preview: string;
  /** 岗位是否已绑定 Harness 实例。 */
  hasHarness: boolean;
  isPending: boolean;
  error: Error | null;
}

/**
 * 启动包预览：换了 Harness 之后，新环境第一次执行这个岗位时会附上的交接内容。
 *
 * 默认收起——正文通常不短，展开才看；空的时候直接说明「不会附加交接」。
 */
export function HandoffPreviewPanel({
  preview,
  hasHarness,
  isPending,
  error,
}: HandoffPreviewPanelProps) {
  const [open, setOpen] = useState(false);
  const empty = preview.trim() === "";

  return (
    <Panel className="handoff-preview">
      <div className="handoff-preview__head">
        <h2 className="handoff-preview__title">启动包预览</h2>
        <Button
          variant="subtle"
          aria-expanded={open}
          onClick={() => {
            setOpen(!open);
          }}
        >
          {open ? "收起" : "展开"}
        </Button>
      </div>

      {isPending ? <p className="handoff-preview__note">正在生成预览…</p> : null}
      {error === null ? null : (
        <p className="handoff-preview__note">预览读取失败：{error.message}</p>
      )}

      {!isPending && error === null && empty ? (
        <p className="handoff-preview__note">{handoffEmptyText(hasHarness)}</p>
      ) : null}

      {!open ? null : (
        <pre className="handoff-preview__body">
          {empty ? handoffEmptyText(hasHarness) : preview}
        </pre>
      )}
    </Panel>
  );
}
