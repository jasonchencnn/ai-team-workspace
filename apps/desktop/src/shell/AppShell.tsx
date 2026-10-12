import { Outlet } from "react-router";
import { Sidebar } from "./Sidebar.js";
import { TopBar } from "./TopBar.js";
import "./shell.css";

/** 应用外壳：左侧导航 + 顶栏 + 内容区。所有页面渲染在 Outlet 中。 */
export function AppShell() {
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
