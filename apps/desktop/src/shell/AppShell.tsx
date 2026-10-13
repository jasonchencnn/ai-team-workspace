import { Outlet } from "react-router";

import { useRunEventInvalidation } from "../pages/task-detail/run/useRunEvents.js";
import { Sidebar } from "./Sidebar.js";
import { TopBar } from "./TopBar.js";
import "./shell.css";

/** 应用外壳：左侧导航 + 顶栏 + 内容区。所有页面渲染在 Outlet 中。 */
export function AppShell() {
  // 执行事件全局订阅一次：任何 run 状态变化都会让任务、看板、概览与岗位工作台刷新。
  useRunEventInvalidation();
  return (
    <div className="shell">
      <Sidebar />
      <div className="main">
        <TopBar />
        <main className="content">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
