use super::*;

fn create(
    f: &Fixture,
    team_id: &str,
    name: &str,
    executable: Option<&str>,
) -> Result<HarnessInstance> {
    f.ts.create_harness(CreateHarnessInput {
        team_id: team_id.into(),
        name: name.into(),
        kind: HarnessKind::ClaudeCode,
        executable: executable.map(Into::into),
        model: Some("  ".into()),
        profile: Some(" ~/.claude/settings.json ".into()),
    })
}

#[test]
fn create_normalizes_optional_fields_and_starts_unknown() {
    let f = fixture();
    let team = f.team("T");
    let h = create(&f, &team.id, " frontend-local ", Some("")).unwrap();
    assert_eq!(h.name, "frontend-local");
    assert_eq!(h.kind, HarnessKind::ClaudeCode);
    assert!(h.executable.is_none());
    assert!(h.model.is_none());
    assert_eq!(h.profile.as_deref(), Some("~/.claude/settings.json"));
    assert_eq!(h.health.status, HealthStatus::Unknown);
    assert!(h.health.checked_at.is_none());
    assert_eq!(f.ts.list_harnesses(&team.id).unwrap(), vec![h]);
}

#[test]
fn name_is_required_and_unique_per_team() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(create(&f, &team.id, " ", None), ErrorKind::Invalid);
    create(&f, &team.id, "local", None).unwrap();
    assert_kind(create(&f, &team.id, "local", None), ErrorKind::Conflict);
    let other = f.team("U");
    create(&f, &other.id, "local", None).unwrap();
}

#[test]
fn executable_must_be_absolute() {
    let f = fixture();
    let team = f.team("T");
    assert_kind(
        create(&f, &team.id, "a", Some("bin/claude")),
        ErrorKind::Invalid,
    );
    let h = create(&f, &team.id, "a", Some("/opt/homebrew/bin/claude")).unwrap();
    assert_eq!(h.executable.as_deref(), Some("/opt/homebrew/bin/claude"));
}

#[test]
fn check_persists_health_and_update_resets_it() {
    let f = fixture();
    let team = f.team("T");
    let h = create(&f, &team.id, "a", None).unwrap();
    let checked = f.ts.check_harness(&h.id).unwrap();
    assert_eq!(checked.health.status, HealthStatus::Ok);
    assert_eq!(
        checked.health.version.as_deref(),
        Some("2.1.288 (Claude Code)")
    );
    assert!(checked.health.checked_at.is_some());
    assert_eq!(checked.updated_at, h.updated_at);
    assert_eq!(
        f.ts.list_harnesses(&team.id).unwrap()[0].health,
        checked.health
    );
    assert_eq!(f.ts.get_harness(&h.id).unwrap(), checked);

    let updated =
        f.ts.update_harness(UpdateHarnessInput {
            harness_id: h.id.clone(),
            name: "b".into(),
            executable: Some(BROKEN_EXECUTABLE.into()),
            model: Some("opus".into()),
            profile: None,
        })
        .unwrap();
    assert_eq!(updated.name, "b");
    assert_eq!(updated.kind, HarnessKind::ClaudeCode);
    assert_eq!(updated.model.as_deref(), Some("opus"));
    assert!(updated.profile.is_none());
    assert_eq!(updated.health.status, HealthStatus::Unknown);
    assert!(updated.health.checked_at.is_none());

    // 检查失败不是命令错误。
    let failed = f.ts.check_harness(&h.id).unwrap();
    assert_eq!(failed.health.status, HealthStatus::Error);
    assert!(failed.health.detail.is_some());

    let events = f.ts.get_team_overview(&team.id).unwrap().recent_events;
    assert_eq!(events[0].kind, EventKind::HarnessChecked);
    assert!(events[0].summary.contains("异常"), "{}", events[0].summary);
}

#[test]
fn mock_harness_is_always_healthy() {
    let f = fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "mock", HarnessKind::Mock);
    let checked = f.ts.check_harness(&h.id).unwrap();
    assert_eq!(checked.health.status, HealthStatus::Ok);
}

#[test]
fn bound_harness_cannot_be_archived() {
    let f = fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "frontend-local", HarnessKind::ClaudeCode);
    let fe = f.role(&team.id, "role_frontend");
    let qa = f.role(&team.id, "role_qa");
    f.ts.bind_role_harness(&fe.id, &h.id).unwrap();
    f.ts.bind_role_harness(&qa.id, &h.id).unwrap();
    let msg = assert_kind(f.ts.archive_harness(&h.id), ErrorKind::Conflict);
    assert!(msg.contains("前端开发") && msg.contains("QA 测试"), "{msg}");

    f.ts.unbind_role_harness(&fe.id).unwrap();
    f.ts.unbind_role_harness(&qa.id).unwrap();
    let archived = f.ts.archive_harness(&h.id).unwrap();
    assert!(archived.archived_at.is_some());

    // 归档后不可见、不可操作，名称可以复用。
    assert!(f.ts.list_harnesses(&team.id).unwrap().is_empty());
    assert_kind(f.ts.check_harness(&h.id), ErrorKind::NotFound);
    assert_kind(f.ts.get_harness(&h.id), ErrorKind::NotFound);
    assert_kind(f.ts.archive_harness(&h.id), ErrorKind::NotFound);
    f.harness(&team.id, "frontend-local", HarnessKind::Codex);
}

#[test]
fn harness_bound_only_to_archived_role_can_be_archived() {
    let f = fixture();
    let team = f.team("T");
    let h = f.harness(&team.id, "h", HarnessKind::Mock);
    let pm = f.role(&team.id, "role_product");
    f.ts.bind_role_harness(&pm.id, &h.id).unwrap();
    f.ts.archive_role(&pm.id).unwrap();
    f.ts.archive_harness(&h.id).unwrap();
}
