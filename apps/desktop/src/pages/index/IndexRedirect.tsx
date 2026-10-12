import { useQuery } from "@tanstack/react-query";
import { Navigate } from "react-router";
import { listTeams } from "../../api/commands.js";
import { queryKeys } from "../../api/queryKeys.js";
import { Button, ErrorBanner, Spinner } from "../../components/index.js";

/** `/`：无团队去创建，否则进入第一个团队。 */
export function IndexRedirect() {
  const teams = useQuery({ queryKey: queryKeys.teams, queryFn: listTeams });

  if (teams.isPending) {
    return <Spinner label="正在加载团队" />;
  }
  if (teams.isError) {
    return (
      <ErrorBanner
        error={teams.error}
        action={
          <Button variant="neutral" onClick={() => void teams.refetch()}>
            重试
          </Button>
        }
      />
    );
  }
  const first = teams.data[0];
  if (first === undefined) {
    return <Navigate to="/teams/new" replace />;
  }
  return <Navigate to={`/teams/${first.id}`} replace />;
}
