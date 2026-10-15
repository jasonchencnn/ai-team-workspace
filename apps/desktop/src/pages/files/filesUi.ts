// 共享文件页的纯逻辑：目录树、搜索筛选、时间与大小排版、Markdown 内链解析。
// 这里不含任何后端调用，页面只负责把这些结构渲染成画板上的三栏。
// 工作空间里到底有哪些文件由内核决定（`list_workspace_markdown`），页面不猜。

export interface FileTreeNode {
  /** 显示名：文件名或目录名。 */
  name: string;
  /** 完整相对路径：文件是自身，目录是不带结尾斜杠的前缀。 */
  path: string;
  isDirectory: boolean;
  /** 本目录下的 Markdown 文件数（含子目录）。 */
  count: number;
  children: FileTreeNode[];
}

/** 按路径把文件列表组织成树：根层文件在前，目录在后，各自按名称排序。 */
export function buildTree(files: readonly string[]): FileTreeNode[] {
  const roots: FileTreeNode[] = [];
  const directories = new Map<string, FileTreeNode>();
  for (const file of [...files].sort()) {
    const segments = file.split("/").filter((segment) => segment !== "");
    let siblings = roots;
    let prefix = "";
    segments.forEach((segment, index) => {
      prefix = prefix === "" ? segment : `${prefix}/${segment}`;
      if (index === segments.length - 1) {
        siblings.push({
          name: segment,
          path: file,
          isDirectory: false,
          count: 1,
          children: [],
        });
        return;
      }
      let directory = directories.get(prefix);
      if (directory === undefined) {
        directory = {
          name: segment,
          path: prefix,
          isDirectory: true,
          count: 0,
          children: [],
        };
        directories.set(prefix, directory);
        siblings.push(directory);
      }
      directory.count += 1;
      siblings = directory.children;
    });
  }
  return sortTree(roots);
}

function sortTree(nodes: FileTreeNode[]): FileTreeNode[] {
  return [...nodes]
    .sort((a, b) =>
      a.isDirectory === b.isDirectory ? a.name.localeCompare(b.name) : a.isDirectory ? 1 : -1,
    )
    .map((node) => ({ ...node, children: sortTree(node.children) }));
}

/**
 * 按文件名（不区分大小写）筛选树：命中的文件留下，命中的目录留下并只保留命中的子树。
 * 目录本身名字命中时整棵子树都留下——用户找的是那个目录里的东西。
 */
export function filterTree(nodes: readonly FileTreeNode[], query: string): FileTreeNode[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") {
    return [...nodes];
  }
  const kept: FileTreeNode[] = [];
  for (const node of nodes) {
    if (node.isDirectory) {
      const children = filterTree(node.children, needle);
      const self = node.name.toLowerCase().includes(needle);
      if (self) {
        kept.push(node);
      } else if (children.length > 0) {
        kept.push({ ...node, children });
      }
    } else if (node.name.toLowerCase().includes(needle)) {
      kept.push(node);
    }
  }
  return kept;
}

/** 搜索时展开全部结果目录：筛出来的内容藏在收起目录里就等于没筛。 */
export function openDirectories(nodes: readonly FileTreeNode[]): string[] {
  return nodes
    .filter((node) => node.isDirectory)
    .flatMap((node) => [node.path, ...openDirectories(node.children)]);
}

/** 一个文件所在的所有目录（从根到直接父级）。选中文件时用来保证它的目录是展开的。 */
export function ancestorsOf(path: string): string[] {
  const segments = path.split("/").slice(0, -1);
  return segments.map((_, index) => segments.slice(0, index + 1).join("/"));
}

/** 文件大小：画板上显示成「3.2 KB」。 */
export function formatSize(bytes: number): string {
  if (bytes < 1024) {
    return `${String(bytes)} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** 最后修改时间：本地时区，今天与昨天用「今天 / 昨天」，更早用「月-日 时:分」。 */
export function formatUpdated(iso: string, now: Date): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  const clock = date.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  if (isSameLocalDay(date, now)) {
    return `今天 ${clock}`;
  }
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (isSameLocalDay(date, yesterday)) {
    return `昨天 ${clock}`;
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function isSameLocalDay(left: Date, right: Date): boolean {
  return (
    left.getFullYear() === right.getFullYear() &&
    left.getMonth() === right.getMonth() &&
    left.getDate() === right.getDate()
  );
}

/** 是不是工作空间里可以打开的 Markdown 引用（任务引用的跳转入口用）。 */
export function isMarkdownRef(path: string): boolean {
  return path.toLowerCase().endsWith(".md");
}

export type DocLink =
  /** 工作空间内的 Markdown：点了在本页打开。 */
  | { kind: "doc"; path: string }
  /** 不能在应用内打开：外链、目录、绝对路径或非 Markdown 文件。 */
  | { kind: "blocked"; reason: string };

const EXTERNAL = /^(https?:|mailto:|tel:|ftp:)/i;

/**
 * 解析 Markdown 正文里的链接相对目标。
 *
 * 只放行工作空间内的 `.md` 相对链接（本页切换），其余一律禁用：应用没有打开系统浏览器或外部
 * 程序的权限（见 `src-tauri/capabilities`），点一个「看起来能点」的链接却什么都不发生更糟。
 */
export function resolveDocLink(currentPath: string, href: string | undefined): DocLink {
  if (href === undefined || href.trim() === "") {
    return { kind: "blocked", reason: "空链接" };
  }
  const raw = href.trim();
  if (EXTERNAL.test(raw)) {
    return { kind: "blocked", reason: "外部链接，应用内不打开" };
  }
  if (raw.startsWith("#")) {
    // 同一文件内的锚点：保持在当前文件，不做跳转。
    return { kind: "doc", path: currentPath };
  }
  if (raw.startsWith("/") || /^[a-z]:/i.test(raw)) {
    return { kind: "blocked", reason: "工作空间之外的路径" };
  }
  const [target = ""] = raw.split("#");
  if (target === "") {
    return { kind: "blocked", reason: "空链接" };
  }
  const resolved = joinRelative(currentPath, target);
  if (resolved === null) {
    return { kind: "blocked", reason: "链接指向工作空间之外" };
  }
  if (!isMarkdownRef(resolved)) {
    return { kind: "blocked", reason: "不是 Markdown 文件，应用内不打开" };
  }
  return { kind: "doc", path: resolved };
}

/** 相对当前文件所在目录拼出目标路径；上到根目录之外时为 null。 */
function joinRelative(currentPath: string, target: string): string | null {
  const base = currentPath.split("/").slice(0, -1);
  const out: string[] = [];
  for (const part of [...base, ...target.split("/")]) {
    if (part === "" || part === ".") {
      continue;
    }
    if (part === "..") {
      if (out.length === 0) {
        return null;
      }
      out.pop();
      continue;
    }
    out.push(part);
  }
  return out.join("/");
}
