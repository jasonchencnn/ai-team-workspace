import { useCallback, useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";

import { useRunEventInvalidation } from "../pages/task-detail/run/useRunEvents.js";
import { Sidebar } from "./Sidebar.js";
import { TopBar } from "./TopBar.js";
import "./shell.css";

/** 应用外壳：左侧导航 + 顶栏 + 内容区。所有页面渲染在 Outlet 中。 */
export function AppShell() {
  // 执行事件全局订阅一次：任何 run 状态变化都会让任务、看板、概览与岗位工作台刷新。
  useRunEventInvalidation();
  // 窄屏（< 768px）侧边栏收成抽屉，由顶栏的菜单按钮开合。
  const [navOpen, setNavOpen] = useState(false);
  const location = useLocation();

  const closeNav = useCallback(() => {
    setNavOpen(false);
  }, []);

  // 换页面就收起抽屉，免得遮住新页面的内容。
  useEffect(() => {
    setNavOpen(false);
  }, [location.pathname]);

  // 抽屉开着时按 Esc 关闭。
  useEffect(() => {
    if (!navOpen) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setNavOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [navOpen]);

  return (
    <div className="shell">
      <Sidebar open={navOpen} onNavigate={closeNav} />
      {navOpen ? (
        <div
          className="drawer-backdrop drawer-backdrop--open"
          onClick={closeNav}
          role="presentation"
        />
      ) : null}
      <div className="main">
        <TopBar
          navOpen={navOpen}
          onToggleNav={() => {
            setNavOpen((value) => !value);
          }}
        />
        <main className="content">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
