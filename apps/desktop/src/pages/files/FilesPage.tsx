import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Link, useParams, useSearchParams } from "react-router";

import {
  listTaskStatuses,
  listTasksReferencingFile,
  listTeams,
  listWorkspaceMarkdown,
  readWorkspaceMarkdown,
} from "../../api/commands.js";
import { errorMessage } from "../../api/errors.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  EmptyState,
  ErrorBanner,
  PageHeader,
  Spinner,
  Tag,
} from "../../components/index.js";
import type { TaskStatus } from "../../bindings/TaskStatus.js";
import { statusTone } from "../tasks/taskUi.js";
import {
  ancestorsOf,
  buildTree,
  filterTree,
  formatSize,
  formatUpdated,
  openDirectories,
  resolveDocLink,
  type FileTreeNode,
} from "./filesUi.js";
import "./files.css";

/** 当前打开的文件写在地址栏（`?path=`）：从任务详情跳过来、刷新、复制链接都能回到同一个文件。 */
const PATH_PARAM = "path";

export function FilesPage() {
  const { teamId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const selected = params.get(PATH_PARAM) ?? "";
  const [search, setSearch] = useState("");
  const [toggled, setToggled] = useState<string[]>([]);

  const files = useQuery({
    queryKey: queryKeys.markdownFiles(teamId),
    queryFn: () => listWorkspaceMarkdown({ teamId }),
    enabled: teamId !== "",
  });

  // 没指定文件时打开第一个：页面进来就有东西看，而不是一块空白等点击。
  const listed = files.data?.files ?? [];
  const [first] = listed;
  useEffect(() => {
    if (selected !== "" || first === undefined) {
      return;
    }
    const next = new URLSearchParams(params);
    next.set(PATH_PARAM, first);
    // 自动选中的第一个文件不占用历史记录，用户「返回」时不会退回到同一个页面。
    setParams(next, { replace: true });
  }, [first, params, selected, setParams]);

  const doc = useQuery({
    queryKey: queryKeys.markdownFile(teamId, selected),
    queryFn: () => readWorkspaceMarkdown({ teamId, path: selected }),
    enabled: teamId !== "" && selected !== "",
  });
  const refs = useQuery({
    queryKey: queryKeys.fileTaskRefs(teamId, selected),
    queryFn: () => listTasksReferencingFile({ teamId, path: selected }),
    enabled: teamId !== "" && selected !== "",
  });
  const statuses = useQuery({
    queryKey: queryKeys.taskStatuses,
    queryFn: listTaskStatuses,
    staleTime: Infinity,
  });
  const statusLabel = (status: TaskStatus) =>
    statuses.data?.find((info) => info.status === status)?.label ?? status;
  // 团队名只用于列表上方的一句说明，与顶栏共用同一份缓存。
  const teams = useQuery({ queryKey: queryKeys.teams, queryFn: listTeams });
  const teamName = teams.data?.find((team) => team.id === teamId)?.name ?? "工作空间";

  const select = (path: string) => {
    const next = new URLSearchParams(params);
    next.set(PATH_PARAM, path);
    setParams(next);
  };
  const referencing = refs.data ?? [];
  const firstRef = referencing[0];

  const refresh = () => {
    void files.refetch();
    void doc.refetch();
    void refs.refetch();
  };

  const tree = useMemo(() => filterTree(buildTree(listed), search), [listed, search]);
  // 搜索时全部展开；否则展开用户手动点开的目录，以及当前选中文件所在的目录。
  const open = useMemo(
    () =>
      new Set(
        search.trim() === ""
          ? [...new Set([...toggled, ...ancestorsOf(selected)])]
          : openDirectories(tree),
      ),
    [search, toggled, selected, tree],
  );
  const toggle = (path: string) => {
    setToggled((current) =>
      current.includes(path) ? current.filter((value) => value !== path) : [...current, path],
    );
  };

  const header = (
    <PageHeader
      title="共享文件"
      description="工作空间里的 Markdown 文件，只读浏览；改文件请在你自己的编辑器里进行。"
      actions={
        <Button variant="neutral" onClick={refresh}>
          刷新文件列表
        </Button>
      }
    />
  );

  if (files.isPending) {
    return (
      <>
        {header}
        <div className="files-state">
          <Spinner label="正在列出工作空间文件" />
        </div>
      </>
    );
  }
  if (files.isError) {
    return (
      <>
        {header}
        <ErrorBanner
          error={files.error}
          action={
            <Button variant="neutral" onClick={refresh}>
              重试
            </Button>
          }
        />
      </>
    );
  }

  return (
    <>
      {header}
      <div className="files-layout">
        <aside className="files-panel files-panel--tree" aria-label="工作空间 Markdown 文件">
          <input
            className="files-search"
            type="search"
            aria-label="搜索文件名"
            placeholder="搜索文件名"
            value={search}
            onChange={(event) => {
              setSearch(event.target.value);
            }}
          />
          <p className="files-meta">
            {teamName} · {String(listed.length)} 个 Markdown 文件
            {files.data.truncated ? "（已截断，只显示前一部分）" : ""}
          </p>
          {tree.length === 0 ? (
            <p className="files-tree__empty">
              {listed.length === 0 ? "工作空间里还没有 Markdown 文件。" : "没有匹配的文件。"}
            </p>
          ) : (
            <ul className="files-tree">
              {tree.map((node) => (
                <TreeBranch
                  key={node.path}
                  node={node}
                  depth={0}
                  selected={selected}
                  open={open}
                  onSelect={select}
                  onToggle={toggle}
                />
              ))}
            </ul>
          )}
          <p className="files-note">
            只显示工作空间内的 .md 文件；.git、node_modules 和指向工作空间之外的符号链接不显示。
          </p>
        </aside>

        <main className="files-panel files-panel--doc" aria-label="文件内容">
          {selected === "" ? (
            <EmptyState
              title="还没有可看的文件"
              description="在工作空间里放一个 .md 文件，然后点「刷新文件列表」。"
            />
          ) : doc.isPending ? (
            <Spinner label="正在读取文件" />
          ) : doc.isError ? (
            <EmptyState
              title="文件已被移动或删除"
              description={`${selected} 现在读不到：${errorMessage(doc.error)}`}
              action={
                <Button variant="neutral" onClick={refresh}>
                  重试
                </Button>
              }
            />
          ) : (
            <>
              <div className="files-doc__head">
                <code className="files-doc__path">{doc.data.path}</code>
                <Tag tone="neutral">只读</Tag>
                <span className="files-doc__meta">
                  更新于 {formatUpdated(doc.data.updatedAt, new Date())} ·{" "}
                  {formatSize(doc.data.size)}
                  {doc.data.truncated ? "（内容已截断）" : ""}
                </span>
              </div>
              {/* 只渲染 Markdown，不启用原始 HTML（没有 rehype-raw）。 */}
              <article className="files-doc__body">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  components={{
                    a: ({ href, children }) => {
                      const link = resolveDocLink(doc.data.path, href);
                      if (link.kind === "doc") {
                        return (
                          <button
                            type="button"
                            className="files-doc__link"
                            onClick={() => {
                              select(link.path);
                            }}
                          >
                            {children}
                          </button>
                        );
                      }
                      return (
                        <span
                          className="files-doc__link files-doc__link--blocked"
                          title={link.reason}
                        >
                          {children}
                        </span>
                      );
                    },
                  }}
                >
                  {doc.data.content}
                </ReactMarkdown>
              </article>
            </>
          )}
        </main>

        <aside className="files-panel files-panel--refs" aria-label="引用此文件的任务">
          <h2 className="files-panel__title">
            引用此文件的任务
            <span className="files-panel__count">{String(refs.data?.length ?? 0)}</span>
          </h2>
          {selected === "" || refs.isPending ? null : refs.isError ? (
            <ErrorBanner error={refs.error} />
          ) : referencing.length === 0 ? (
            <p className="files-muted">暂无任务引用</p>
          ) : (
            <ul className="files-refs">
              {referencing.map((ref) => (
                <li key={ref.id} className="files-ref">
                  <p className="files-ref__title">
                    <span className="files-ref__key">{ref.key}</span>
                    <span className="files-ref__name">{ref.title}</span>
                    <Tag tone={statusTone[ref.status]}>{statusLabel(ref.status)}</Tag>
                  </p>
                  <p className="files-ref__owner">负责岗位 {ref.owner.name}</p>
                  <Link className="files-ref__open" to={`/teams/${teamId}/tasks/${ref.id}`}>
                    打开任务 →
                  </Link>
                </li>
              ))}
            </ul>
          )}
          <p className="files-note">点任务跳到任务详情。没有任务引用时显示「暂无任务引用」。</p>
          <h2 className="files-panel__title">从任务进入</h2>
          <div className="files-entry">
            <p className="files-entry__label">任务详情 · 引用文件</p>
            {selected === "" ? (
              <p className="files-muted">先在左边选一个文件。</p>
            ) : firstRef === undefined ? (
              <p className="files-entry__path files-entry__path--plain">{selected}</p>
            ) : (
              // 有任务引用时，这就是「从任务进入」的入口；没有任务就只是回显当前文件。
              <Link className="files-entry__path" to={`/teams/${teamId}/tasks/${firstRef.id}`}>
                {doc.data?.path ?? selected} ↗
              </Link>
            )}
            <p className="files-note">
              在任务详情点引用文件，会打开此页并选中该文件；文件已不存在时显示「文件已被移动或删除」。
            </p>
          </div>
        </aside>
      </div>
    </>
  );
}

interface TreeBranchProps {
  node: FileTreeNode;
  depth: number;
  selected: string;
  open: ReadonlySet<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
}

function TreeBranch({ node, depth, selected, open, onSelect, onToggle }: TreeBranchProps) {
  const indent = { paddingInlineStart: `${String(8 + depth * 16)}px` };
  if (!node.isDirectory) {
    const active = node.path === selected;
    return (
      <li>
        <button
          type="button"
          className={active ? "files-tree__item files-tree__item--active" : "files-tree__item"}
          style={indent}
          aria-current={active ? "true" : undefined}
          onClick={() => {
            onSelect(node.path);
          }}
        >
          {node.name}
        </button>
      </li>
    );
  }
  const expanded = open.has(node.path);
  return (
    <li>
      <button
        type="button"
        className="files-tree__item files-tree__item--dir"
        style={indent}
        aria-expanded={expanded}
        onClick={() => {
          onToggle(node.path);
        }}
      >
        <span className="files-tree__chevron" aria-hidden="true">
          {expanded ? "▾" : "▸"}
        </span>
        {node.name}
        <span className="files-tree__count">{String(node.count)}</span>
      </button>
      {expanded ? (
        <ul className="files-tree">
          {node.children.map((child) => (
            <TreeBranch
              key={child.path}
              node={child}
              depth={depth + 1}
              selected={selected}
              open={open}
              onSelect={onSelect}
              onToggle={onToggle}
            />
          ))}
        </ul>
      ) : null}
    </li>
  );
}
