import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router";
import { listTeams } from "../api/commands.js";
import { errorMessage } from "../api/errors.js";
import { queryKeys } from "../api/queryKeys.js";

export function TeamSwitcher({ currentTeamId }: { currentTeamId: string | undefined }) {
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const teams = useQuery({ queryKey: queryKeys.teams, queryFn: listTeams });

  const current = teams.data?.find((team) => team.id === currentTeamId);
  const triggerLabel = current?.name ?? (teams.isError ? "团队加载失败" : "选择团队");

  const go = (path: string) => {
    setOpen(false);
    void navigate(path);
  };

  return (
    <div className="team-switcher">
      <button
        type="button"
        className="team-switcher__trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => {
          setOpen((value) => !value);
        }}
      >
        <span>{triggerLabel}</span>
        <span aria-hidden="true">⌄</span>
      </button>
      {open ? (
        <ul className="team-switcher__menu" role="listbox" aria-label="切换团队">
          {teams.isPending ? <li className="team-switcher__status">加载中…</li> : null}
          {teams.isError ? (
            <li className="team-switcher__status" role="alert">
              {errorMessage(teams.error)}
            </li>
          ) : null}
          {teams.data?.map((team) => (
            <li key={team.id} role="option" aria-selected={team.id === currentTeamId}>
              <button
                type="button"
                className={
                  team.id === currentTeamId
                    ? "team-switcher__option team-switcher__option--current"
                    : "team-switcher__option"
                }
                onClick={() => {
                  go(`/teams/${team.id}`);
                }}
              >
                {team.name}
              </button>
            </li>
          ))}
          <li role="option" aria-selected={false}>
            <button
              type="button"
              className="team-switcher__option"
              onClick={() => {
                go("/teams/new");
              }}
            >
              + 创建团队
            </button>
          </li>
        </ul>
      ) : null}
    </div>
  );
}
