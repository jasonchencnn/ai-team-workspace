// 单个文件的 diff 卡片：展开时才向后端取补丁，自己渲染增删行着色；截断与二进制给出说明。
import { useQuery } from "@tanstack/react-query";

import { getReviewFilePatch } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import { Button, ErrorBanner, Spinner, Tag } from "../../components/index.js";
import type { ReviewFile } from "../../bindings/ReviewFile.js";
import { formatCount, parseUnifiedDiff } from "./reviewUi.js";

export function DiffRows({ patch }: { patch: string }) {
  const rows = parseUnifiedDiff(patch);
  if (rows.length === 0) {
    return <p className="diff-card__note">这个文件没有可显示的行改动。</p>;
  }
  return (
    <div className="diff-body" role="table" aria-label="diff 内容">
      {rows.map((row, index) =>
        row.kind === "hunk" ? (
          <div key={index} className="diff-row diff-row--hunk" role="row">
            {row.text}
          </div>
        ) : row.kind === "note" ? (
          <div key={index} className="diff-row diff-row--note" role="row">
            {row.text}
          </div>
        ) : (
          <div key={index} className={`diff-row diff-row--${row.kind}`} role="row">
            <span className="diff-row__no" aria-hidden="true">
              {row.oldNo ?? ""}
            </span>
            <span className="diff-row__no" aria-hidden="true">
              {row.newNo ?? ""}
            </span>
            <span className="diff-row__sign" aria-hidden="true">
              {row.kind === "add" ? "+" : row.kind === "del" ? "-" : " "}
            </span>
            <span className="diff-row__text">{row.text}</span>
          </div>
        ),
      )}
    </div>
  );
}

export function DiffCard({
  runId,
  file,
  open,
  onToggle,
  anchorId,
}: {
  runId: string;
  file: ReviewFile;
  open: boolean;
  onToggle: () => void;
  anchorId: string;
}) {
  const patch = useQuery({
    queryKey: queryKeys.reviewPatch(runId, file.path),
    queryFn: () => getReviewFilePatch(runId, file.path),
    enabled: open && file.hasPatch,
    staleTime: Infinity,
  });
  return (
    <div className="diff-card" id={anchorId}>
      <div className="diff-card__head">
        <span className="diff-card__path">{file.path}</span>
        {file.added !== null && file.removed !== null ? (
          <>
            <span className="diff-count diff-count--add">+{formatCount(file.added)}</span>
            <span className="diff-count diff-count--del">−{formatCount(file.removed)}</span>
          </>
        ) : null}
        <Button
          variant="subtle"
          aria-expanded={open}
          aria-label={`${open ? "收起" : "查看"} ${file.path} 的 diff`}
          onClick={onToggle}
        >
          {open ? "收起" : "查看 diff"}
        </Button>
      </div>
      {open ? (
        file.binary ? (
          <div className="diff-card__note-row">
            <Tag tone="neutral">二进制</Tag>
            <span>二进制文件，不显示 diff。</span>
          </div>
        ) : !file.hasPatch ? (
          <p className="diff-card__note">这个文件没有可查看的 diff。</p>
        ) : patch.isPending ? (
          <div className="diff-card__note">
            <Spinner label="正在读取 diff" />
          </div>
        ) : patch.isError ? (
          <ErrorBanner
            error={patch.error}
            action={
              <Button
                variant="neutral"
                onClick={() => {
                  void patch.refetch();
                }}
              >
                重试
              </Button>
            }
          />
        ) : patch.data.binary ? (
          <div className="diff-card__note-row">
            <Tag tone="neutral">二进制</Tag>
            <span>二进制文件，不显示 diff。</span>
          </div>
        ) : (
          <>
            <DiffRows patch={patch.data.patch} />
            {patch.data.truncated ? (
              <div className="diff-card__truncated" role="note">
                <Tag tone="warning">diff 已截断</Tag>
                <span>这个文件改动很大，只显示了前一部分。完整内容请在仓库中用 git 查看。</span>
              </div>
            ) : null}
          </>
        )
      ) : null}
    </div>
  );
}
