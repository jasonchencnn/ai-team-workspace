import { Route, Routes } from "react-router";
import { AppShell } from "./shell/AppShell.js";
import { ActivityPage } from "./pages/activity/ActivityPage.js";
import { FilesPage } from "./pages/files/FilesPage.js";
import { HarnessDetailPage } from "./pages/harness-detail/HarnessDetailPage.js";
import { IndexRedirect } from "./pages/index/IndexRedirect.js";
import { MessagesPage } from "./pages/messages/MessagesPage.js";
import { InboxPage } from "./pages/inbox/InboxPage.js";
import { NotificationSettingsPage } from "./pages/inbox/NotificationSettingsPage.js";
import { ReviewPage } from "./pages/review/ReviewPage.js";
import { RoleWorkbenchPage } from "./pages/role-workbench/RoleWorkbenchPage.js";
import { RolesPage } from "./pages/roles/RolesPage.js";
import { SettingsPage } from "./pages/settings/SettingsPage.js";
import { TaskDetailPage } from "./pages/task-detail/TaskDetailPage.js";
import { TaskFormPage } from "./pages/task-form/TaskFormPage.js";
import { TasksPage } from "./pages/tasks/TasksPage.js";
import { TeamCreatePage } from "./pages/team-create/TeamCreatePage.js";
import { TeamHomePage } from "./pages/team-home/TeamHomePage.js";

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<IndexRedirect />} />
        <Route path="teams/new" element={<TeamCreatePage />} />
        <Route path="teams/:teamId" element={<TeamHomePage />} />
        <Route path="teams/:teamId/tasks" element={<TasksPage />} />
        <Route path="teams/:teamId/tasks/new" element={<TaskFormPage />} />
        <Route path="teams/:teamId/tasks/:taskId/edit" element={<TaskFormPage />} />
        <Route path="teams/:teamId/tasks/:taskId" element={<TaskDetailPage />} />
        <Route path="teams/:teamId/tasks/:taskId/review" element={<ReviewPage />} />
        <Route path="teams/:teamId/files" element={<FilesPage />} />
        <Route path="teams/:teamId/roles" element={<RolesPage />} />
        <Route path="teams/:teamId/roles/:roleId/workbench" element={<RoleWorkbenchPage />} />
        <Route path="teams/:teamId/activity" element={<ActivityPage />} />
        {/* 岗位通信（M5-03）：可带 ?thread= 打开指定线程。 */}
        <Route path="teams/:teamId/messages" element={<MessagesPage />} />
        {/* 待我处理（M5-04）：按类别聚合需要 Human 判断的事项。 */}
        <Route path="teams/:teamId/inbox" element={<InboxPage />} />
        {/* 通知订阅（M5-04）：类别总开关与岗位开关。 */}
        <Route path="teams/:teamId/notifications" element={<NotificationSettingsPage />} />
        {/* 设置（M7-04）：数据备份与导出。 */}
        <Route path="teams/:teamId/settings" element={<SettingsPage />} />
        <Route path="teams/:teamId/harnesses/:harnessId" element={<HarnessDetailPage />} />
        <Route path="*" element={<IndexRedirect />} />
      </Route>
    </Routes>
  );
}
