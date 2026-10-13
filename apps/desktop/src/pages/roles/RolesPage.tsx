import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useParams, useSearchParams } from "react-router";

import { listHarnesses, listRoles } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import {
  Button,
  EmptyState,
  ErrorBanner,
  PageHeader,
  Panel,
  Spinner,
} from "../../components/index.js";
import { HarnessInstancesSection } from "./HarnessInstancesSection.js";
import { RoleCreateForm } from "./RoleCreateForm.js";
import { RoleDetail } from "./RoleDetail.js";
import { RoleList } from "./RoleList.js";
import { HARNESS_TAB_PARAM } from "./rolesNav.js";
import "./roles.css";

/** 岗位页内的两个视图：岗位列表/详情，以及 Harness 接入。用 `?tab=` 记住当前视图。 */
export function RolesPage() {
  const { teamId } = useParams();

  if (teamId === undefined) {
    return <EmptyState title="未选择团队" description="请先在左侧选择一个团队。" />;
  }

  return <RolesView teamId={teamId} />;
}

function RolesView({ teamId }: { teamId: string }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const showHarnesses = searchParams.get("tab") === HARNESS_TAB_PARAM;
  // 岗位工作台的「换绑 Harness」带着 role 参数跳进来：选中该岗位并直接展开绑定选择。
  const requestedRoleId = searchParams.get("role");

  const [selectedRoleId, setSelectedRoleId] = useState<string | null>(requestedRoleId);
  const [pickingFor, setPickingFor] = useState<string | null>(requestedRoleId);
  const [creating, setCreating] = useState(false);

  const rolesQuery = useQuery({
    queryKey: queryKeys.roles(teamId),
    queryFn: () => listRoles(teamId),
  });
  const harnessesQuery = useQuery({
    queryKey: queryKeys.harnesses(teamId),
    queryFn: () => listHarnesses(teamId),
  });

  const roles = rolesQuery.data ?? [];
  const harnesses = harnessesQuery.data ?? [];
  const selected = roles.find((view) => view.role.id === selectedRoleId) ?? roles[0] ?? null;

  const selectTab = (tab: "roles" | "harness") => {
    if (tab === "harness") {
      setSearchParams({ tab: HARNESS_TAB_PARAM });
      return;
    }
    setSearchParams(requestedRoleId === null ? {} : { role: requestedRoleId });
  };

  return (
    <div className="roles-page">
      <PageHeader
        title="岗位与绑定"
        description="岗位身份长期保留；执行用的 Harness 通过 ACP 接入，可以随时调整。"
      />

      <div className="roles-tabs" role="tablist" aria-label="岗位与 Harness 接入">
        <button
          type="button"
          role="tab"
          className="roles-tabs__tab"
          aria-selected={!showHarnesses}
          onClick={() => {
            selectTab("roles");
          }}
        >
          岗位
        </button>
        <button
          type="button"
          role="tab"
          className="roles-tabs__tab"
          aria-selected={showHarnesses}
          onClick={() => {
            selectTab("harness");
          }}
        >
          Harness 接入
        </button>
      </div>

      {showHarnesses ? (
        <HarnessInstancesSection
          teamId={teamId}
          harnesses={harnesses}
          roles={roles}
          isPending={harnessesQuery.isPending}
          error={harnessesQuery.error}
          onRetry={() => {
            void harnessesQuery.refetch();
          }}
        />
      ) : (
        <div className="roles-layout">
          <div className="roles-column">
            <RoleList
              roles={roles}
              isPending={rolesQuery.isPending}
              error={rolesQuery.error}
              onRetry={() => {
                void rolesQuery.refetch();
              }}
              selectedRoleId={selected?.role.id ?? null}
              onSelect={(roleId) => {
                setSelectedRoleId(roleId);
                setPickingFor(null);
              }}
              onBindRequest={(roleId) => {
                setSelectedRoleId(roleId);
                setPickingFor(roleId);
              }}
              onCreate={() => {
                setCreating(true);
              }}
            />

            {creating ? (
              <RoleCreateForm
                teamId={teamId}
                onCancel={() => {
                  setCreating(false);
                }}
                onCreated={() => {
                  setCreating(false);
                }}
              />
            ) : null}
          </div>

          <div className="roles-column">
            {selected === null ? (
              rolesQuery.isPending ? (
                <Panel>
                  <Spinner label="正在加载岗位" />
                </Panel>
              ) : rolesQuery.error !== null ? (
                <ErrorBanner
                  error={rolesQuery.error}
                  action={
                    <Button
                      variant="neutral"
                      onClick={() => {
                        void rolesQuery.refetch();
                      }}
                    >
                      重试
                    </Button>
                  }
                />
              ) : (
                <Panel>
                  <EmptyState
                    title="还没有岗位"
                    description="先新增岗位，再为它绑定一个执行工具。"
                    action={
                      <Button
                        onClick={() => {
                          setCreating(true);
                        }}
                      >
                        新增岗位
                      </Button>
                    }
                  />
                </Panel>
              )
            ) : (
              <RoleDetail
                key={selected.role.id}
                teamId={teamId}
                view={selected}
                harnesses={harnesses}
                isHarnessPending={harnessesQuery.isPending}
                harnessError={harnessesQuery.error}
                picking={pickingFor === selected.role.id}
                onPickToggle={() => {
                  setPickingFor(pickingFor === selected.role.id ? null : selected.role.id);
                }}
              />
            )}
          </div>
        </div>
      )}
    </div>
  );
}
