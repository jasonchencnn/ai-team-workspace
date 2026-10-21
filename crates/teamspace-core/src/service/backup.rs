//! 备份用的数据清单：团队、工作空间与岗位文件夹的登记信息。
//!
//! 只读数据库，不碰文件系统（文件复制在 [`crate::backup`] 里做）。
//! 岗位文件夹相对工作空间的路径存在 `roles.folder`（形如 `Teamspace/前端开发`），
//! 是岗位文件夹同步时登记的；没有登记的岗位说明它还没有文件夹。

use rusqlite::Connection;

use crate::error::Result;

/// 一个岗位的文件夹登记信息。
pub(crate) struct RoleFolderRow {
    pub role_id: String,
    pub role_name: String,
    /// 相对工作空间的路径（`Teamspace/<岗位文件夹>`）。
    pub relative_path: String,
}

/// 一个团队及其岗位文件夹。
pub(crate) struct TeamRow {
    pub team_id: String,
    pub team_name: String,
    pub workspace_path: String,
    pub folders: Vec<RoleFolderRow>,
}

/// 全部团队（含已归档）及其已登记的岗位文件夹。
///
/// 已归档的团队也一并备份：它的岗位文件夹仍在工作空间里，丢掉就再也找不回来。
pub(crate) fn inventory(conn: &Connection) -> Result<Vec<TeamRow>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.workspace_path, r.id, r.name, r.folder
         FROM teams t LEFT JOIN roles r ON r.team_id = t.id
         ORDER BY t.created_at, t.id, r.created_at, r.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut teams: Vec<TeamRow> = Vec::new();
    for row in rows {
        let (team_id, team_name, workspace_path, role_id, role_name, folder) = row?;
        if !teams.iter().any(|team| team.team_id == team_id) {
            teams.push(TeamRow {
                team_id: team_id.clone(),
                team_name: team_name.clone(),
                workspace_path: workspace_path.clone(),
                folders: Vec::new(),
            });
        }
        let Some(team) = teams.iter_mut().find(|team| team.team_id == team_id) else {
            continue;
        };
        let (Some(role_id), Some(role_name), Some(folder)) = (role_id, role_name, folder) else {
            continue;
        };
        team.folders.push(RoleFolderRow {
            role_id,
            role_name,
            relative_path: folder,
        });
    }
    Ok(teams)
}
