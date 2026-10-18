use super::*;
use crate::service::{dependency, event};
use TaskStatus::*;

fn dependent(
    fixture: &Fixture,
    team: &Team,
    owner: &Role,
    title: &str,
    upstream: &[&TaskDetail],
) -> TaskDetail {
    fixture
        .try_task(
            &team.id,
            owner,
            title,
            Todo,
            vec![],
            upstream
                .iter()
                .map(|detail| detail.task.id.clone())
                .collect(),
        )
        .unwrap()
}

fn finish(fixture: &Fixture, detail: &TaskDetail) -> TaskDetail {
    if fixture.ts.get_task(&detail.task.id).unwrap().task.status == Todo {
        fixture.move_to(&detail.task.id, Ready).unwrap();
    }
    fixture.drive(&detail.task.id, &[Doing, Review, Done])
}

fn precheck(
    fixture: &Fixture,
    team: &Team,
    task: Option<&TaskDetail>,
    deps: &[&TaskDetail],
) -> Result<TaskDependencyCheck> {
    fixture
        .ts
        .check_task_dependencies(CheckTaskDependenciesInput {
            team_id: team.id.clone(),
            task_id: task.map(|detail| detail.task.id.clone()),
            depends_on: deps.iter().map(|detail| detail.task.id.clone()).collect(),
        })
}

#[test]
fn dependency_precheck_accepts_create_empty_and_duplicate_dependencies_without_writes() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    let before = fixture.ts.get_task(&first.task.id).unwrap();
    for deps in [vec![], vec![&first], vec![&first, &first]] {
        let check = precheck(&fixture, &team, None, &deps).unwrap();
        assert!(check.cycle.is_empty());
        assert!(check.readiness_notice.contains("文件引用可选"));
    }
    assert_eq!(fixture.ts.get_task(&first.task.id).unwrap(), before);
    assert_eq!(fixture.task(&team.id, &owner, "B").task.key, "FE-002");
}

#[test]
fn dependency_precheck_returns_full_cycle_and_save_still_rejects_it() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    let second = dependent(&fixture, &team, &owner, "B", &[&first]);
    let third = dependent(&fixture, &team, &owner, "C", &[&second]);
    let before = fixture.ts.get_task(&first.task.id).unwrap();
    let check = precheck(&fixture, &team, Some(&first), &[&third]).unwrap();
    assert_eq!(check.cycle, ["FE-001", "FE-003", "FE-002", "FE-001"]);
    let error = assert_kind(
        fixture.ts.update_task(UpdateTaskInput {
            task_id: first.task.id.clone(),
            title: first.task.title.clone(),
            description: String::new(),
            owner_role_id: owner.id.clone(),
            file_refs: vec![],
            depends_on: vec![third.task.id.clone()],
        }),
        ErrorKind::Conflict,
    );
    assert_eq!(error, format!("依赖形成环路：{}", check.cycle.join(" → ")));
    assert_eq!(fixture.ts.get_task(&first.task.id).unwrap(), before);
    assert!(
        precheck(&fixture, &team, Some(&first), &[])
            .unwrap()
            .cycle
            .is_empty()
    );
}

#[test]
fn dependency_precheck_detects_self_and_direct_cycles() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    let second = dependent(&fixture, &team, &owner, "B", &[&first]);
    assert_eq!(
        precheck(&fixture, &team, Some(&first), &[&first])
            .unwrap()
            .cycle,
        ["FE-001", "FE-001"]
    );
    assert_eq!(
        precheck(&fixture, &team, Some(&first), &[&second])
            .unwrap()
            .cycle,
        ["FE-001", "FE-002", "FE-001"]
    );
    assert!(
        precheck(&fixture, &team, Some(&second), &[&first])
            .unwrap()
            .cycle
            .is_empty()
    );
    assert!(
        precheck(&fixture, &team, Some(&second), &[])
            .unwrap()
            .cycle
            .is_empty()
    );
}

#[test]
fn dependency_precheck_rejects_foreign_or_missing_tasks_and_invalid_limits() {
    let fixture = fixture();
    let team = fixture.team("T");
    let other = fixture.team("Other");
    let owner = fixture.role(&other.id, "role_frontend");
    let foreign = fixture.task(&other.id, &owner, "foreign");
    assert_kind(
        precheck(&fixture, &team, Some(&foreign), &[]),
        ErrorKind::NotFound,
    );
    assert_kind(
        precheck(&fixture, &team, None, &[&foreign]),
        ErrorKind::NotFound,
    );
    for input in [
        CheckTaskDependenciesInput {
            team_id: "missing".into(),
            task_id: None,
            depends_on: vec![],
        },
        CheckTaskDependenciesInput {
            team_id: team.id.clone(),
            task_id: Some("missing".into()),
            depends_on: vec![],
        },
        CheckTaskDependenciesInput {
            team_id: team.id.clone(),
            task_id: None,
            depends_on: vec!["missing".into()],
        },
    ] {
        assert_kind(
            fixture.ts.check_task_dependencies(input),
            ErrorKind::NotFound,
        );
    }
    assert_kind(
        fixture
            .ts
            .check_task_dependencies(CheckTaskDependenciesInput {
                team_id: team.id,
                task_id: None,
                depends_on: vec![foreign.task.id; 21],
            }),
        ErrorKind::Invalid,
    );
}

#[test]
fn dependency_detail_reports_direct_links_live_status_and_readiness_reasons() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    let second = dependent(&fixture, &team, &owner, "B", &[&first]);
    let third = dependent(&fixture, &team, &owner, "C", &[&second]);
    let second = fixture.ts.get_task(&second.task.id).unwrap();
    assert_eq!(second.dependencies.len(), 1);
    assert_eq!(second.dependencies[0].key, first.task.key);
    assert_eq!(second.dependencies[0].title, "A");
    assert_eq!(second.dependencies[0].status, Todo);
    assert_eq!(second.dependents.len(), 1);
    assert_eq!(second.dependents[0].id, third.task.id);
    assert_eq!(second.readiness, ["依赖任务 FE-001 尚未完成（TODO）"]);
    assert!(second.run_problems.contains(&second.readiness[0]));
    assert!(
        second
            .dependency_notice
            .unwrap()
            .contains("前置完成后会自动变为 READY")
    );
    finish(&fixture, &first);
    let second = fixture.ts.get_task(&second.task.id).unwrap();
    assert_eq!(second.dependencies[0].status, Done);
    assert_eq!(second.task.status, Ready);
    assert!(second.readiness.is_empty());
    assert!(second.dependency_notice.is_none());
    let reopened = fixture.move_to(&first.task.id, Ready).unwrap();
    let second = fixture.ts.get_task(&second.task.id).unwrap();
    assert_eq!(second.task.status, Ready);
    assert_eq!(second.dependencies[0].status, Ready);
    assert!(!second.readiness.is_empty());
    assert!(
        reopened
            .events
            .iter()
            .any(|event| event.summary.contains("未回退"))
    );
}

#[test]
fn dependency_notice_covers_todo_with_completed_predecessors_and_optional_files() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    assert!(first.dependency_notice.is_none());
    finish(&fixture, &first);
    let waiting = dependent(&fixture, &team, &owner, "B", &[&first]);
    assert_eq!(waiting.task.status, Todo);
    assert!(waiting.readiness.is_empty());
    let notice = waiting.dependency_notice.unwrap();
    assert!(notice.contains("前置完成后会自动变为 READY"));
    assert!(notice.contains("文件引用可选"));
}

#[test]
fn completion_advances_only_the_next_link_and_returns_fresh_dependents() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    let second = dependent(&fixture, &team, &owner, "B", &[&first]);
    let third = dependent(&fixture, &team, &owner, "C", &[&second]);

    let completed = finish(&fixture, &first);
    assert_eq!(completed.dependents[0].id, second.task.id);
    assert_eq!(completed.dependents[0].status, Ready);
    let third_before = fixture.ts.get_task(&third.task.id).unwrap();
    assert_eq!(third_before.task.status, Todo);
    assert_eq!(third_before.events.len(), 1);

    finish(&fixture, &second);
    assert_eq!(
        fixture.ts.get_task(&third.task.id).unwrap().task.status,
        Ready
    );
}

#[test]
fn diamond_waits_for_both_middle_tasks() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    let left = dependent(&fixture, &team, &owner, "B", &[&first]);
    let right = dependent(&fixture, &team, &owner, "C", &[&first]);
    let last = dependent(&fixture, &team, &owner, "D", &[&left, &right]);

    finish(&fixture, &first);
    assert_eq!(
        fixture.ts.get_task(&left.task.id).unwrap().task.status,
        Ready
    );
    assert_eq!(
        fixture.ts.get_task(&right.task.id).unwrap().task.status,
        Ready
    );
    finish(&fixture, &left);
    assert_eq!(
        fixture.ts.get_task(&last.task.id).unwrap().task.status,
        Todo
    );
    finish(&fixture, &right);
    assert_eq!(
        fixture.ts.get_task(&last.task.id).unwrap().task.status,
        Ready
    );
}

#[test]
fn partial_completion_and_cancelled_predecessor_do_not_advance() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let completed = fixture.task(&team.id, &owner, "completed");
    let cancelled = fixture.task(&team.id, &owner, "cancelled");
    let waiting = dependent(
        &fixture,
        &team,
        &owner,
        "waiting",
        &[&completed, &cancelled],
    );
    fixture.move_to(&cancelled.task.id, Cancelled).unwrap();
    finish(&fixture, &completed);
    let waiting = fixture.ts.get_task(&waiting.task.id).unwrap();
    assert_eq!(waiting.task.status, Todo);
    assert_eq!(waiting.events.len(), 1);
}

#[test]
fn missing_or_archived_owner_does_not_advance_or_fail_completion() {
    for missing in [true, false] {
        let fixture = fixture();
        let team = fixture.team("T");
        let upstream_owner = fixture.role(&team.id, "role_frontend");
        let owner = fixture.role(&team.id, "role_backend");
        let first = fixture.task(&team.id, &upstream_owner, "first");
        let waiting = dependent(&fixture, &team, &owner, "waiting", &[&first]);
        if missing {
            fixture.raw_sql(&format!(
                "PRAGMA foreign_keys = OFF; UPDATE tasks SET owner_role_id = '' WHERE id = '{}'",
                waiting.task.id,
            ));
        } else {
            fixture.raw_sql(&format!(
                "UPDATE roles SET archived_at = '2026-10-15T00:00:00Z' WHERE id = '{}'",
                owner.id,
            ));
        }
        let completed = finish(&fixture, &first);
        assert_eq!(completed.task.status, Done);
        let conn = rusqlite::Connection::open(fixture.db_path()).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = ?1",
                [&waiting.task.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "todo");
        assert_eq!(event::for_task(&conn, &waiting.task.id).unwrap().len(), 1);
    }
}

#[test]
fn non_todo_dependents_are_never_changed() {
    for status in [Ready, Doing, Blocked, Review, Done, Cancelled] {
        let fixture = fixture();
        let team = fixture.team("T");
        let owner = fixture.role(&team.id, "role_frontend");
        let first = fixture.task(&team.id, &owner, "first");
        let waiting = dependent(&fixture, &team, &owner, "waiting", &[&first]);
        let reason = if status == Blocked {
            "'blocked'"
        } else {
            "NULL"
        };
        fixture.raw_sql(&format!(
            "UPDATE tasks SET status = '{}', blocked_reason = {reason} WHERE id = '{}'",
            crate::store::enum_str(status),
            waiting.task.id,
        ));
        let before = fixture.ts.get_task(&waiting.task.id).unwrap();
        finish(&fixture, &first);
        assert_eq!(
            fixture.ts.get_task(&waiting.task.id).unwrap().task,
            before.task
        );
        assert_eq!(
            fixture.ts.get_task(&waiting.task.id).unwrap().events,
            before.events
        );
    }
}

#[test]
fn missing_reference_does_not_advance_but_other_dependents_do() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    fixture.touch("spec.md");
    let missing = fixture
        .try_task(
            &team.id,
            &owner,
            "missing",
            Todo,
            vec!["spec.md"],
            vec![first.task.id.clone()],
        )
        .unwrap();
    let ready = dependent(&fixture, &team, &owner, "ready", &[&first]);
    fs::remove_file(fixture.ws.join("spec.md")).unwrap();
    finish(&fixture, &first);
    assert_eq!(
        fixture.ts.get_task(&missing.task.id).unwrap().task.status,
        Todo
    );
    assert_eq!(
        fixture.ts.get_task(&ready.task.id).unwrap().task.status,
        Ready
    );
}

#[cfg(unix)]
#[test]
fn reference_symlink_outside_workspace_does_not_advance() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    let outside = fixture.dir.path().join("outside.md");
    fs::write(&outside, "outside").unwrap();
    std::os::unix::fs::symlink(&outside, fixture.ws.join("link.md")).unwrap();
    let waiting = fixture
        .try_task(
            &team.id,
            &owner,
            "waiting",
            Todo,
            vec!["link.md"],
            vec![first.task.id.clone()],
        )
        .unwrap();
    finish(&fixture, &first);
    assert_eq!(
        fixture.ts.get_task(&waiting.task.id).unwrap().task.status,
        Todo
    );
}

#[test]
fn automatic_event_and_role_journal_record_the_trigger_once() {
    let fixture = fixture();
    let team = fixture.team("T");
    let upstream_owner = fixture.role(&team.id, "role_backend");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &upstream_owner, "first");
    let waiting = dependent(&fixture, &team, &owner, "waiting", &[&first]);
    let folder = fixture.ts.get_role_folder(&owner.id, None).unwrap();
    let journal_path =
        Path::new(folder.path.as_ref().unwrap()).join(crate::role_folder::JOURNAL_FILE);
    let before = fs::read_to_string(&journal_path).unwrap();
    finish(&fixture, &first);
    let advanced = fixture.ts.get_task(&waiting.task.id).unwrap();
    let recorded = &advanced.events[0];
    let summary = format!(
        "前置任务 {} 已完成，{} 自动就绪",
        first.task.key, waiting.task.key
    );
    assert_eq!(recorded.actor, Actor::System);
    assert_eq!(recorded.kind, EventKind::TaskStatusChanged);
    assert_eq!(recorded.task_id.as_deref(), Some(waiting.task.id.as_str()));
    assert_eq!(recorded.role_id.as_deref(), Some(owner.id.as_str()));
    assert_eq!(recorded.summary, summary);
    assert_eq!(
        recorded.data,
        serde_json::json!({
            "from": "todo", "to": "ready", "triggerTaskId": first.task.id, "triggerTaskKey": first.task.key,
        })
    );
    let conn = rusqlite::Connection::open(fixture.db_path()).unwrap();
    let files = dependency::file_check_plan(&conn, &first.task.id)
        .unwrap()
        .scan();
    assert!(
        dependency::advance_dependents(&conn, &first.task.id, &files)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        event::for_task(&conn, &waiting.task.id).unwrap().len(),
        advanced.events.len()
    );
    let after = fs::read_to_string(&journal_path).unwrap();
    assert!(after.starts_with(&before));
    assert_eq!(after.matches(&summary).count(), 1);
}

#[test]
fn advance_returns_only_changed_ids_and_is_inert_before_done() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    let waiting = dependent(&fixture, &team, &owner, "waiting", &[&first]);
    let mut conn = crate::store::open(&fixture.db_path()).unwrap();
    let files = dependency::file_check_plan(&conn, &first.task.id)
        .unwrap()
        .scan();
    assert!(
        dependency::advance_dependents(&conn, &first.task.id, &files)
            .unwrap()
            .is_empty()
    );
    let tx = conn.transaction().unwrap();
    tx.execute(
        "UPDATE tasks SET status = 'done' WHERE id = ?1",
        [&first.task.id],
    )
    .unwrap();
    assert_eq!(
        dependency::advance_dependents(&tx, &first.task.id, &files).unwrap(),
        [waiting.task.id]
    );
    assert!(
        dependency::advance_dependents(&tx, &first.task.id, &files)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn completion_uses_precomputed_file_checks_without_rescanning() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    fixture.touch("spec.md");
    let waiting = fixture
        .try_task(
            &team.id,
            &owner,
            "waiting",
            Todo,
            vec!["spec.md"],
            vec![first.task.id.clone()],
        )
        .unwrap();
    let files = fixture.ts.dependent_file_checks(&first.task.id).unwrap();
    fs::remove_file(fixture.ws.join("spec.md")).unwrap();
    let advanced = fixture
        .ts
        .write(|tx| {
            tx.execute(
                "UPDATE tasks SET status = 'done' WHERE id = ?1",
                [&first.task.id],
            )?;
            crate::service::task::on_task_done(tx, &first.task.id, &files)
        })
        .unwrap();
    assert_eq!(advanced.as_slice(), std::slice::from_ref(&waiting.task.id));
    assert_eq!(
        fixture.ts.get_task(&waiting.task.id).unwrap().task.status,
        Ready
    );
}

#[test]
fn stale_dependent_snapshot_rolls_back_completion_and_any_advancement() {
    for change in ["references", "candidate", "workspace"] {
        let fixture = fixture();
        let team = fixture.team("T");
        let owner = fixture.role(&team.id, "role_frontend");
        let first = fixture.task(&team.id, &owner, "first");
        let earlier = dependent(&fixture, &team, &owner, "earlier", &[&first]);
        let waiting = dependent(&fixture, &team, &owner, "waiting", &[&first]);
        let files = fixture.ts.dependent_file_checks(&first.task.id).unwrap();
        match change {
            "references" => {
                fixture
                    .ts
                    .update_task(UpdateTaskInput {
                        task_id: waiting.task.id.clone(),
                        title: waiting.task.title.clone(),
                        description: String::new(),
                        owner_role_id: owner.id.clone(),
                        file_refs: vec![FileRef {
                            path: "new.md".into(),
                            anchor: None,
                        }],
                        depends_on: vec![first.task.id.clone()],
                    })
                    .unwrap();
            }
            "candidate" => {
                dependent(&fixture, &team, &owner, "new", &[&first]);
            }
            "workspace" => {
                fixture
                    .ts
                    .write(|tx| {
                        tx.execute(
                            "UPDATE teams SET workspace_path = ?2 WHERE id = ?1",
                            rusqlite::params![team.id, "changed"],
                        )?;
                        Ok(())
                    })
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert_kind(
            fixture.ts.write(|tx| {
                tx.execute(
                    "UPDATE tasks SET status = 'done' WHERE id = ?1",
                    [&first.task.id],
                )?;
                crate::service::task::on_task_done(tx, &first.task.id, &files)
            }),
            ErrorKind::Conflict,
        );
        let conn = fixture.ts.lock();
        assert_eq!(
            crate::service::task::get(&conn, &first.task.id)
                .unwrap()
                .status,
            first.task.status
        );
        for id in [&earlier.task.id, &waiting.task.id] {
            assert_eq!(crate::service::task::get(&conn, id).unwrap().status, Todo);
            assert_eq!(
                event::for_task(&conn, id).unwrap().len(),
                if change == "references" && id == &waiting.task.id {
                    2
                } else {
                    1
                }
            );
        }
    }
}

#[test]
fn readiness_is_rechecked_after_file_scan() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    let second = fixture.task(&team.id, &owner, "second");
    finish(&fixture, &second);
    let waiting = dependent(&fixture, &team, &owner, "waiting", &[&first, &second]);
    let files = fixture.ts.dependent_file_checks(&first.task.id).unwrap();
    fixture.move_to(&second.task.id, Ready).unwrap();
    let advanced = fixture
        .ts
        .write(|tx| {
            tx.execute(
                "UPDATE tasks SET status = 'done' WHERE id = ?1",
                [&first.task.id],
            )?;
            crate::service::task::on_task_done(tx, &first.task.id, &files)
        })
        .unwrap();
    assert!(advanced.is_empty());
    assert_eq!(
        fixture.ts.get_task(&waiting.task.id).unwrap().task.status,
        Todo
    );
}

#[test]
fn reopening_preserves_ready_and_doing_dependents_and_records_their_keys() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    let ready = dependent(&fixture, &team, &owner, "ready", &[&first]);
    let doing = dependent(&fixture, &team, &owner, "doing", &[&first]);
    let blocked = dependent(&fixture, &team, &owner, "blocked", &[&first]);
    let waiting = fixture
        .try_task(
            &team.id,
            &owner,
            "waiting",
            Todo,
            vec!["missing.md"],
            vec![first.task.id.clone()],
        )
        .unwrap();
    let indirect = dependent(&fixture, &team, &owner, "indirect", &[&ready]);
    finish(&fixture, &first);
    fixture.move_to(&doing.task.id, Doing).unwrap();
    fixture.drive(&blocked.task.id, &[Doing, Blocked]);
    let watched = [&ready, &doing, &blocked, &waiting, &indirect];
    let before: Vec<_> = watched
        .iter()
        .map(|detail| fixture.ts.get_task(&detail.task.id).unwrap())
        .collect();
    let reopened = fixture.move_to(&first.task.id, Ready).unwrap();
    assert_eq!(reopened.events[0].actor, Actor::Human);
    assert!(
        reopened.events[0]
            .summary
            .contains("下游 2 个任务已在进行，未回退")
    );
    assert_eq!(
        reopened.events[0].data["affectedDependentKeys"],
        serde_json::json!([ready.task.key, doing.task.key])
    );
    assert_eq!(reopened.events[0].data["from"], "done");
    assert_eq!(reopened.events[0].data["to"], "ready");
    for previous in before {
        let after = fixture.ts.get_task(&previous.task.id).unwrap();
        assert_eq!(after.task, previous.task);
        assert_eq!(after.events, previous.events);
    }
}

#[test]
fn reopening_without_active_dependents_records_an_empty_list() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    finish(&fixture, &first);
    let reopened = fixture.move_to(&first.task.id, Ready).unwrap();
    assert_eq!(
        reopened.events[0].data["affectedDependentKeys"],
        serde_json::json!([])
    );
    assert!(!reopened.events[0].summary.contains("未回退"));
}

#[test]
fn dependent_event_failure_rolls_back_completion_and_all_advances() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "first");
    let second = dependent(&fixture, &team, &owner, "second", &[&first]);
    let third = dependent(&fixture, &team, &owner, "third", &[&first]);
    fixture.drive(&first.task.id, &[Ready, Doing, Review]);
    let before = fixture.ts.get_task(&first.task.id).unwrap();
    fixture.raw_sql(&format!(
        "CREATE TRIGGER reject_auto_ready BEFORE INSERT ON events
         WHEN NEW.task_id = '{}' AND NEW.actor = 'system'
         BEGIN SELECT RAISE(ABORT, 'test event failure'); END;",
        third.task.id,
    ));
    assert!(fixture.move_to(&first.task.id, Done).is_err());
    assert_eq!(fixture.ts.get_task(&first.task.id).unwrap(), before);
    for waiting in [&second, &third] {
        let after = fixture.ts.get_task(&waiting.task.id).unwrap();
        assert_eq!(after.task, waiting.task);
        assert_eq!(after.events, waiting.events);
    }
    fixture.raw_sql("DROP TRIGGER reject_auto_ready");
    fixture.move_to(&first.task.id, Done).unwrap();
    assert_eq!(
        fixture.ts.get_task(&second.task.id).unwrap().task.status,
        Ready
    );
    assert_eq!(
        fixture.ts.get_task(&third.task.id).unwrap().task.status,
        Ready
    );
}

#[test]
fn create_and_edit_reject_reachable_existing_cycles_with_the_full_path() {
    let fixture = fixture();
    let team = fixture.team("T");
    let owner = fixture.role(&team.id, "role_frontend");
    let first = fixture.task(&team.id, &owner, "A");
    let second = dependent(&fixture, &team, &owner, "B", &[&first]);
    let root = dependent(&fixture, &team, &owner, "root", &[&second]);
    let editing = fixture.task(&team.id, &owner, "editing");
    fixture.raw_sql(&format!(
        "INSERT INTO task_dependencies (task_id, depends_on_id, position) VALUES ('{}', '{}', 0)",
        first.task.id, second.task.id,
    ));
    let created = fixture.try_task(
        &team.id,
        &owner,
        "new",
        Todo,
        vec![],
        vec![root.task.id.clone()],
    );
    let message = assert_kind(created, ErrorKind::Conflict);
    assert_eq!(message, "依赖形成环路：FE-002 → FE-001 → FE-002");
    let message = assert_kind(
        fixture.ts.update_task(UpdateTaskInput {
            task_id: editing.task.id.clone(),
            title: "edit".into(),
            description: String::new(),
            owner_role_id: owner.id.clone(),
            file_refs: vec![],
            depends_on: vec![root.task.id.clone()],
        }),
        ErrorKind::Conflict,
    );
    assert_eq!(message, "依赖形成环路：FE-002 → FE-001 → FE-002");
    assert_eq!(fixture.ts.get_task(&editing.task.id).unwrap(), editing);
    assert_eq!(fixture.task(&team.id, &owner, "next").task.key, "FE-005");
}
