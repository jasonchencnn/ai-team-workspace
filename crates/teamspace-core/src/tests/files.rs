//! 共享文件（页面 `4:435`）的命令级测试：列出、读取的边界、引用任务的反查。
//!
//! 忽略规则、符号链接这些依赖真实文件系统行为，所以除了走假探测的 [`Fixture`]（命令与校验逻辑），
//! 还有 [`RealFixture`] 用 [`crate::system_probe::LocalProbe`] 跑真实 Git 仓库与普通文件夹。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{Fixture, assert_kind, plain_fixture, templates};
use crate::Teamspace;
use crate::error::{ErrorKind, Result};
use crate::model::{
    CreateTeamInput, FileTaskRef, FileTasksQuery, MarkdownFile, MarkdownList, MarkdownListQuery,
    MarkdownReadQuery, TaskStatus, Team,
};
use crate::probe::MARKDOWN_FILE_LIMIT;
use crate::system_probe::LocalProbe;

// ---------------------------------------------------------------- 真实探测的工作空间

/// 真实探测（[`LocalProbe`]）+ 真实文件系统的工作空间，用来验证「哪些目录值得看」这类真实现。
struct RealFixture {
    /// 只为持有临时目录的所有权，字段本身不用。
    _dir: TempDir,
    ws: PathBuf,
    ts: Teamspace,
    team: Team,
}

fn git_in(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(std::process::Stdio::null())
        .status()
        .unwrap_or_else(|err| panic!("git {args:?} 运行失败：{err}"));
    assert!(status.success(), "git {args:?} 退出码 {status}");
}

/// 建一个团队 + 工作空间；`git` 为真时工作空间是真实 Git 仓库，否则是普通文件夹。
fn real_fixture(name: &str, git: bool) -> RealFixture {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    fs::create_dir_all(&ws).unwrap();
    if git {
        git_in(&ws, &["init", "--quiet"]);
        git_in(&ws, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git_in(&ws, &["config", "user.name", "test"]);
        git_in(&ws, &["config", "user.email", "test@example.invalid"]);
    }
    let ts = Teamspace::open(
        &dir.path().join("teamspace.db"),
        Box::new(LocalProbe::new()),
    )
    .unwrap();
    let team = ts
        .create_team(CreateTeamInput {
            name: name.into(),
            description: String::new(),
            workspace_path: ws.to_string_lossy().into_owned(),
            roles: templates(),
        })
        .unwrap();
    RealFixture {
        _dir: dir,
        ws,
        ts,
        team,
    }
}

fn list(fixture: &RealFixture) -> MarkdownList {
    fixture
        .ts
        .list_workspace_markdown(MarkdownListQuery {
            team_id: fixture.team.id.clone(),
        })
        .unwrap()
}

/// 岗位文件夹（D14，`Teamspace/<岗位>/`）里的 Markdown 也是工作空间内容，
/// 断言「用户放了哪些文件」时先滤掉它们。
fn own_files(files: Vec<String>) -> Vec<String> {
    files
        .into_iter()
        .filter(|path| !path.starts_with("Teamspace/"))
        .collect()
}

fn write(ws: &Path, rel: &str, content: &str) {
    let path = ws.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

// ---------------------------------------------------------------- 列举

#[test]
fn lists_markdown_of_a_git_workspace_and_skips_ignored_and_big_dirs() {
    let fixture = real_fixture("M3 组", true);
    write(&fixture.ws, "README.md", "# 团队\n");
    write(&fixture.ws, "plans/task-breakdown.md", "# 拆解\n");
    write(&fixture.ws, "docs/deep/nested/note.md", "# 深层\n");
    write(&fixture.ws, "素材清单.txt", "不是 Markdown\n");
    write(&fixture.ws, ".gitignore", "ignored.md\nbuild/\n");
    write(&fixture.ws, "ignored.md", "# 被忽略\n");
    write(&fixture.ws, "build/generated.md", "# 构建产物\n");
    write(&fixture.ws, "node_modules/pkg/readme.md", "# 依赖里的\n");
    git_in(&fixture.ws, &["add", "-A"]);
    git_in(&fixture.ws, &["commit", "--quiet", "-m", "seed"]);

    let result = list(&fixture);
    let role_folders = result
        .files
        .iter()
        .any(|path| path == "Teamspace/README.md");
    assert_eq!(
        own_files(result.files),
        [
            "README.md",
            "docs/deep/nested/note.md",
            "plans/task-breakdown.md"
        ],
        ".gitignore 忽略的、build/、node_modules/ 和非 .md 都不出现；路径已排序"
    );
    assert!(
        role_folders,
        "岗位文件夹（D14）里的 Markdown 也是工作空间内容"
    );
    assert!(!result.truncated);
    assert_eq!(result.team_id, fixture.team.id);
}

#[test]
fn lists_markdown_of_a_plain_folder_and_skips_symlinks_outside() {
    let fixture = real_fixture("素材组", false);
    write(&fixture.ws, "notes/outline.md", "# 大纲\n");
    write(&fixture.ws, "target/debug/gen.md", "# 构建产物\n");
    write(&fixture.ws, "分镜.csv", "镜头\n");

    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.md"), "不该被列出\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret.md"), fixture.ws.join("leak.md"))
        .unwrap();

    let result = list(&fixture);
    let expected: Vec<&str> = if cfg!(unix) {
        vec!["notes/outline.md"]
    } else {
        // Windows 上建不了这个链接，普通文件夹仍只应看到工作空间里的 Markdown。
        vec!["leak.md", "notes/outline.md"]
    };
    assert_eq!(
        own_files(result.files),
        expected,
        "工作空间之外的文件不进列表"
    );
    assert!(!result.truncated);
}

#[test]
fn empty_workspace_lists_nothing_but_the_role_folders() {
    let f = plain_fixture();
    let team = f.team("空组");
    let result =
        f.ts.list_workspace_markdown(MarkdownListQuery {
            team_id: team.id.clone(),
        })
        .unwrap();
    assert!(
        own_files(result.files.clone()).is_empty(),
        "用户还没放任何文件：{:?}",
        result.files
    );
    assert!(
        !result.files.is_empty(),
        "建团队会建出岗位文件夹（D14），它们也是工作空间里的 Markdown"
    );
    assert!(!result.truncated, "空工作空间不是被截断");
    assert_eq!(result.team_id, team.id);
}

#[test]
fn listing_marks_truncation_at_the_limit() {
    let f = plain_fixture();
    let team = f.team("大组");
    // 正好到上限不算截断：还有第 5001 个才算「可能还有更多」。
    for index in 0..MARKDOWN_FILE_LIMIT {
        fs::write(f.ws.join(format!("note-{index:05}.md")), "x").unwrap();
    }
    let at_limit =
        f.ts.list_workspace_markdown(MarkdownListQuery {
            team_id: team.id.clone(),
        })
        .unwrap();
    assert_eq!(at_limit.files.len(), MARKDOWN_FILE_LIMIT);
    assert!(at_limit.truncated);

    fs::write(f.ws.join("note-99999.md"), "x").unwrap();
    let over =
        f.ts.list_workspace_markdown(MarkdownListQuery { team_id: team.id })
            .unwrap();
    assert_eq!(
        over.files.len(),
        MARKDOWN_FILE_LIMIT,
        "超出上限时只返回上限条数"
    );
    assert!(over.truncated);
}

#[test]
fn listing_a_missing_team_is_not_found() {
    let f = plain_fixture();
    let msg = assert_kind(
        f.ts.list_workspace_markdown(MarkdownListQuery {
            team_id: "没有这个团队".into(),
        }),
        ErrorKind::NotFound,
    );
    assert!(msg.contains("团队"), "{msg}");
}

// ---------------------------------------------------------------- 读取

#[test]
fn reads_a_markdown_file_with_size_and_time() {
    let f = plain_fixture();
    let team = f.team("读文件组");
    fs::create_dir_all(f.ws.join("plans")).unwrap();
    fs::write(f.ws.join("plans/task.md"), "# 账号系统任务拆解\n").unwrap();

    let file =
        f.ts.read_workspace_markdown(MarkdownReadQuery {
            team_id: team.id.clone(),
            path: "plans/task.md".into(),
        })
        .unwrap();
    assert_eq!(file.content, "# 账号系统任务拆解\n");
    assert_eq!(file.path, "plans/task.md");
    assert_eq!(file.team_id, team.id);
    assert!(!file.truncated);
    assert_eq!(file.size, file.content.len() as u64);
    assert!(
        file.updated_at.ends_with('Z'),
        "时间是 UTC RFC3339：{}",
        file.updated_at
    );
}

#[test]
fn reading_accepts_anchors_and_backslash_separators() {
    let f = plain_fixture();
    let team = f.team("读文件组");
    fs::create_dir_all(f.ws.join("plans")).unwrap();
    fs::write(f.ws.join("plans/task.md"), "# 任务\n").unwrap();

    let file =
        f.ts.read_workspace_markdown(MarkdownReadQuery {
            team_id: team.id,
            path: "plans\\task.md#验收".into(),
        })
        .unwrap();
    assert_eq!(file.path, "plans/task.md");
}

#[test]
fn reading_rejects_paths_outside_the_workspace() {
    let f = plain_fixture();
    let team = f.team("越界组");
    fs::create_dir_all(f.ws.join("plans")).unwrap();
    fs::write(f.ws.join("plans/task.md"), "# 任务\n").unwrap();
    let absolute = f.ws.join("plans/task.md").display().to_string();
    let read = |path: &str| -> Result<MarkdownFile> {
        f.ts.read_workspace_markdown(MarkdownReadQuery {
            team_id: team.id.clone(),
            path: path.into(),
        })
    };

    for path in [
        absolute.as_str(),
        "../outside.md",
        "plans/../../secrets.md",
        "",
    ] {
        assert_kind(read(path), ErrorKind::Invalid);
    }
    // 不是 Markdown 与文件不存在是两件事：类型不对是 invalid，丢失是 not_found。
    assert_kind(read("素材清单.txt"), ErrorKind::Invalid);
    assert_kind(read("plans"), ErrorKind::Invalid);
    assert_kind(read("plans/missing.md"), ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn reading_a_symlink_out_of_the_workspace_is_refused() {
    let f = plain_fixture();
    let team = f.team("越界组");
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.md"), "不该被读到\n").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.md"), f.ws.join("leak.md")).unwrap();

    let msg = assert_kind(
        f.ts.read_workspace_markdown(MarkdownReadQuery {
            team_id: team.id,
            path: "leak.md".into(),
        }),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("工作空间之外"), "{msg}");
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_the_workspace_is_readable() {
    let f = plain_fixture();
    let team = f.team("链接组");
    fs::write(f.ws.join("real.md"), "# 真的在里面的\n").unwrap();
    std::os::unix::fs::symlink(f.ws.join("real.md"), f.ws.join("alias.md")).unwrap();

    let file =
        f.ts.read_workspace_markdown(MarkdownReadQuery {
            team_id: team.id,
            path: "alias.md".into(),
        })
        .unwrap();
    assert_eq!(file.path, "alias.md");
    assert_eq!(file.content, "# 真的在里面的\n");
}

#[test]
fn oversized_files_are_truncated_instead_of_failing() {
    let f = plain_fixture();
    let team = f.team("大文件组");
    let big = "x".repeat(crate::workspace_file::MAX_MARKDOWN_BYTES as usize + 2048);
    fs::write(f.ws.join("big.md"), &big).unwrap();

    let file =
        f.ts.read_workspace_markdown(MarkdownReadQuery {
            team_id: team.id,
            path: "big.md".into(),
        })
        .unwrap();
    assert!(file.truncated);
    assert_eq!(file.size, big.len() as u64, "截断也要报告真实大小");
    assert_eq!(
        file.content.len(),
        crate::workspace_file::MAX_MARKDOWN_BYTES as usize
    );
}

#[test]
fn reading_after_the_workspace_moved_reports_the_directory() {
    let f = plain_fixture();
    let team = f.team("搬迁组");
    fs::write(f.ws.join("task.md"), "# 任务\n").unwrap();
    fs::remove_dir_all(&f.ws).unwrap();

    let msg = assert_kind(
        f.ts.read_workspace_markdown(MarkdownReadQuery {
            team_id: team.id,
            path: "task.md".into(),
        }),
        ErrorKind::Invalid,
    );
    assert!(msg.contains("工作空间目录不存在"), "{msg}");
}

// ---------------------------------------------------------------- 引用此文件的任务

fn referencing_tasks(fixture: &Fixture, team_id: &str, path: &str) -> Result<Vec<FileTaskRef>> {
    fixture.ts.list_tasks_referencing_file(FileTasksQuery {
        team_id: team_id.into(),
        path: path.into(),
    })
}

#[test]
fn lists_the_tasks_referencing_a_file_with_owner_and_status() {
    let f = plain_fixture();
    let team = f.team("引用组");
    let fe = f.role(&team.id, "role_frontend");
    let be = f.role(&team.id, "role_backend");
    f.touch("plans/task.md");

    let fe_task = f
        .try_task(
            &team.id,
            &fe,
            "登录界面",
            TaskStatus::Ready,
            vec!["plans/task.md#验收"],
            vec![],
        )
        .unwrap();
    f.drive(&fe_task.task.id, &[TaskStatus::Doing, TaskStatus::Review]);
    let be_task = f
        .try_task(
            &team.id,
            &be,
            "Auth API",
            TaskStatus::Ready,
            vec!["plans/task.md"],
            vec![],
        )
        .unwrap();
    f.drive(
        &be_task.task.id,
        &[TaskStatus::Doing, TaskStatus::Review, TaskStatus::Done],
    );
    f.try_task(
        &team.id,
        &fe,
        "不相关的任务",
        TaskStatus::Todo,
        vec!["docs/spec.md"],
        vec![],
    )
    .unwrap();

    let refs = referencing_tasks(&f, &team.id, "plans/task.md").unwrap();
    // 状态顺序（待验收 → 已完成）而不是编号顺序，进行中的任务排在前面。
    assert_eq!(
        refs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        [fe_task.task.id.as_str(), be_task.task.id.as_str()]
    );
    assert_eq!(refs[0].key, fe_task.task.key);
    assert_eq!(refs[0].title, "登录界面");
    assert_eq!(refs[0].status, TaskStatus::Review);
    assert_eq!(refs[0].owner.id, fe.id);
    assert_eq!(refs[0].anchor.as_deref(), Some("验收"));
    assert_eq!(refs[1].status, TaskStatus::Done);
    assert_eq!(refs[1].owner.id, be.id);
    assert!(refs[1].anchor.is_none());
}

#[test]
fn one_task_listed_once_even_when_it_references_the_file_twice() {
    let f = plain_fixture();
    let team = f.team("重复引用组");
    let fe = f.role(&team.id, "role_frontend");
    f.touch("plans/task.md");
    f.try_task(
        &team.id,
        &fe,
        "重复引用",
        TaskStatus::Todo,
        vec!["plans/task.md#一", "plans/task.md#二"],
        vec![],
    )
    .unwrap();

    let refs = referencing_tasks(&f, &team.id, "plans/task.md").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].anchor.as_deref(), Some("一"), "锚点取第一条引用");
}

#[test]
fn a_file_without_references_returns_an_empty_list() {
    let f = plain_fixture();
    let team = f.team("无引用组");
    f.touch("plans/task.md");
    f.task(&team.id, &f.role(&team.id, "role_frontend"), "别的任务");

    assert!(
        referencing_tasks(&f, &team.id, "plans/task.md")
            .unwrap()
            .is_empty()
    );
    assert!(
        referencing_tasks(&f, &team.id, "根本不存在.md")
            .unwrap()
            .is_empty()
    );
    assert_kind(referencing_tasks(&f, &team.id, "  "), ErrorKind::Invalid);
    assert_kind(
        f.ts.list_tasks_referencing_file(FileTasksQuery {
            team_id: "没有这个团队".into(),
            path: "plans/task.md".into(),
        }),
        ErrorKind::NotFound,
    );
}

#[test]
fn only_tasks_of_the_same_team_are_listed() {
    let f = plain_fixture();
    let team = f.team("甲组");
    let other = f.team("乙组");
    let fe = f.role(&team.id, "role_frontend");
    f.touch("plans/task.md");
    let fe_task = f
        .try_task(
            &team.id,
            &fe,
            "甲组任务",
            TaskStatus::Ready,
            vec!["plans/task.md"],
            vec![],
        )
        .unwrap();
    // 另一个团队也有同路径的引用，不能串过来。
    let be = f.role(&other.id, "role_backend");
    f.try_task(
        &other.id,
        &be,
        "乙组任务",
        TaskStatus::Todo,
        vec!["plans/task.md"],
        vec![],
    )
    .unwrap();

    let refs = referencing_tasks(&f, &team.id, "plans/task.md").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, fe_task.task.id);
    assert_eq!(refs[0].key, fe_task.task.key);
}

#[test]
fn owner_and_key_come_from_the_backend_not_the_page() {
    let f = plain_fixture();
    let team = f.team("归属组");
    let be = f.role(&team.id, "role_backend");
    f.touch("plans/task.md");
    f.try_task(
        &team.id,
        &be,
        "后端接口",
        TaskStatus::Todo,
        vec!["plans/task.md"],
        vec![],
    )
    .unwrap();

    let refs = referencing_tasks(&f, &team.id, "plans/task.md").unwrap();
    assert_eq!(refs[0].owner.id, be.id);
    assert_eq!(refs[0].owner.name, be.name);
    assert_eq!(refs[0].owner.code, be.code);
    assert!(refs[0].key.starts_with("BE-"));
    assert!(refs[0].updated_at.ends_with('Z'));
}
