import { Route, Routes } from "react-router";
import { AppShell } from "./shell/AppShell.js";
import { ActivityPage } from "./pages/activity/ActivityPage.js";
import { HarnessDetailPage } from "./pages/harness-detail/HarnessDetailPage.js";
import { IndexRedirect } from "./pages/index/IndexRedirect.js";
import { RoleWorkbenchPage } from "./pages/role-workbench/RoleWorkbenchPage.js";
import { RolesPage } from "./pages/roles/RolesPage.js";
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
        <Route path="teams/:teamId/roles" element={<RolesPage />} />
        <Route path="teams/:teamId/roles/:roleId/workbench" element={<RoleWorkbenchPage />} />
        <Route path="teams/:teamId/activity" element={<ActivityPage />} />
        <Route path="teams/:teamId/harnesses/:harnessId" element={<HarnessDetailPage />} />
        <Route path="*" element={<IndexRedirect />} />
      </Route>
    </Routes>
  );
}
