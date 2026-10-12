#![forbid(unsafe_code)]

//! SQLx-backed persistence migrations for the local SQLite database.

/// SQLite ports for fixed internal audit consumption and projection replay.
pub mod audit_consumer;
mod expiry_hash;
/// SQLite ports for the internal atomic Message expiry command.
pub mod message_expiry;
/// Shared installation identity checks against current persisted Role IDs.
pub mod owner_identity;

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;

use sqlx_core::error::BoxDynError;
use sqlx_core::migrate::{Migration, MigrationSource, MigrationType, Migrator};
use sqlx_sqlite::SqlitePool;

const TEAM_ROLE_WORKSPACE_SQL: &str = include_str!("../migrations/0001_team_role_workspace.sql");
const HARNESS_ADAPTER_BINDING_SQL: &str =
    include_str!("../migrations/0002_harness_adapter_binding.sql");
const TASK_DEPENDENCY_SQL: &str = include_str!("../migrations/0003_task_dependency.sql");
const RUN_LEASE_REVIEW_SQL: &str = include_str!("../migrations/0004_run_lease_review.sql");
const ARTIFACT_REFERENCE_SQL: &str = include_str!("../migrations/0005_artifact_reference.sql");
const EVENT_OUTBOX_SQL: &str = include_str!("../migrations/0006_event_outbox.sql");
const THREAD_MESSAGE_DELIVERY_SQL: &str =
    include_str!("../migrations/0007_thread_message_delivery.sql");
const NOTIFICATION_ATTENTION_SQL: &str =
    include_str!("../migrations/0008_notification_attention.sql");
const PERMISSION_APPROVAL_SQL: &str = include_str!("../migrations/0009_permission_approval.sql");
const BINDING_TEAM_CONSISTENCY_SQL: &str =
    include_str!("../migrations/0010_binding_team_consistency.sql");
const TASK_DEPENDENCY_CONSISTENCY_SQL: &str =
    include_str!("../migrations/0011_task_dependency_consistency.sql");
const RUN_LEASE_CONSISTENCY_SQL: &str =
    include_str!("../migrations/0012_run_lease_consistency.sql");
const ARTIFACT_REFERENCE_CONSISTENCY_SQL: &str =
    include_str!("../migrations/0013_artifact_reference_consistency.sql");
const EVENT_OUTBOX_CONSISTENCY_SQL: &str =
    include_str!("../migrations/0014_event_outbox_consistency.sql");
const EVENT_ACTOR_OUTBOX_INTEGRITY_SQL: &str =
    include_str!("../migrations/0015_event_actor_outbox_integrity.sql");
const EVENT_DEDUP_STATE_IMMUTABILITY_SQL: &str =
    include_str!("../migrations/0016_event_dedup_state_immutability.sql");
const MESSAGE_DELIVERY_INTEGRITY_SQL: &str =
    include_str!("../migrations/0017_message_delivery_integrity.sql");
const MESSAGE_DELIVERY_INSERT_STATE_GUARD_SQL: &str =
    include_str!("../migrations/0018_message_delivery_insert_state_guard.sql");
const NOTIFICATION_ATTENTION_INTEGRITY_SQL: &str =
    include_str!("../migrations/0019_notification_attention_integrity.sql");
const PERMISSION_APPROVAL_INTEGRITY_SQL: &str =
    include_str!("../migrations/0020_permission_approval_integrity.sql");
const APPROVAL_POLICY_LINKAGE_SQL: &str =
    include_str!("../migrations/0021_approval_policy_linkage.sql");
const COMMAND_IDEMPOTENCY_SQL: &str = include_str!("../migrations/0022_command_idempotency.sql");

const NOTIFICATION_EXECUTION_SQL: &str =
    include_str!("../migrations/0023_notification_execution_integrity.sql");

const OUTBOX_RETRY_DEFAULT_SQL: &str = include_str!("../migrations/0024_outbox_retry_default.sql");

const MESSAGE_ROUTING_TEAM_INTEGRITY_SQL: &str =
    include_str!("../migrations/0025_message_routing_team_integrity.sql");

const CORE_HISTORY_RETENTION_SQL: &str =
    include_str!("../migrations/0026_core_history_retention.sql");

const RUN_TRANSITION_INTEGRITY_SQL: &str =
    include_str!("../migrations/0027_run_transition_integrity.sql");

const RETAINED_ENTITY_INTEGRITY_SQL: &str =
    include_str!("../migrations/0028_retained_entity_integrity.sql");

const RUN_TERMINAL_INTEGRITY_SQL: &str =
    include_str!("../migrations/0029_run_terminal_integrity.sql");

const MESSAGE_ROUTE_OVERRIDES_SQL: &str =
    include_str!("../migrations/0030_message_route_overrides.sql");

const MESSAGE_ROUTE_ATTEMPT_BINDING_SQL: &str =
    include_str!("../migrations/0033_message_route_attempt_binding.sql");

const NOTIFICATION_DEDUPE_RETENTION_SQL: &str =
    include_str!("../migrations/0034_notification_dedupe_retention.sql");

const ATTENTION_SOURCE_REFERENCE_SQL: &str =
    include_str!("../migrations/0035_attention_source_reference_integrity.sql");

#[derive(Debug)]
struct EmbeddedMigrations;

impl MigrationSource<'static> for EmbeddedMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Migration>, BoxDynError>> + Send + 'static>> {
        Box::pin(async {
            Ok(vec![
                Migration::new(
                    1,
                    Cow::Borrowed("team role workspace"),
                    MigrationType::Simple,
                    Cow::Borrowed(TEAM_ROLE_WORKSPACE_SQL),
                    false,
                ),
                Migration::new(
                    2,
                    Cow::Borrowed("harness adapter binding"),
                    MigrationType::Simple,
                    Cow::Borrowed(HARNESS_ADAPTER_BINDING_SQL),
                    false,
                ),
                Migration::new(
                    3,
                    Cow::Borrowed("task dependency"),
                    MigrationType::Simple,
                    Cow::Borrowed(TASK_DEPENDENCY_SQL),
                    false,
                ),
                Migration::new(
                    4,
                    Cow::Borrowed("run lease review"),
                    MigrationType::Simple,
                    Cow::Borrowed(RUN_LEASE_REVIEW_SQL),
                    false,
                ),
                Migration::new(
                    5,
                    Cow::Borrowed("artifact reference"),
                    MigrationType::Simple,
                    Cow::Borrowed(ARTIFACT_REFERENCE_SQL),
                    false,
                ),
                Migration::new(
                    6,
                    Cow::Borrowed("event outbox"),
                    MigrationType::Simple,
                    Cow::Borrowed(EVENT_OUTBOX_SQL),
                    false,
                ),
                Migration::new(
                    7,
                    Cow::Borrowed("thread message delivery"),
                    MigrationType::Simple,
                    Cow::Borrowed(THREAD_MESSAGE_DELIVERY_SQL),
                    false,
                ),
                Migration::new(
                    8,
                    Cow::Borrowed("notification attention"),
                    MigrationType::Simple,
                    Cow::Borrowed(NOTIFICATION_ATTENTION_SQL),
                    false,
                ),
                Migration::new(
                    9,
                    Cow::Borrowed("permission approval"),
                    MigrationType::Simple,
                    Cow::Borrowed(PERMISSION_APPROVAL_SQL),
                    false,
                ),
                Migration::new(
                    10,
                    Cow::Borrowed("binding team consistency"),
                    MigrationType::Simple,
                    Cow::Borrowed(BINDING_TEAM_CONSISTENCY_SQL),
                    false,
                ),
                Migration::new(
                    11,
                    Cow::Borrowed("task dependency consistency"),
                    MigrationType::Simple,
                    Cow::Borrowed(TASK_DEPENDENCY_CONSISTENCY_SQL),
                    false,
                ),
                Migration::new(
                    12,
                    Cow::Borrowed("run lease consistency"),
                    MigrationType::Simple,
                    Cow::Borrowed(RUN_LEASE_CONSISTENCY_SQL),
                    false,
                ),
                Migration::new(
                    13,
                    Cow::Borrowed("artifact reference consistency"),
                    MigrationType::Simple,
                    Cow::Borrowed(ARTIFACT_REFERENCE_CONSISTENCY_SQL),
                    false,
                ),
                Migration::new(
                    14,
                    Cow::Borrowed("event outbox consistency"),
                    MigrationType::Simple,
                    Cow::Borrowed(EVENT_OUTBOX_CONSISTENCY_SQL),
                    false,
                ),
                Migration::new(
                    15,
                    Cow::Borrowed("event actor outbox integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(EVENT_ACTOR_OUTBOX_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    16,
                    Cow::Borrowed("event dedup state immutability"),
                    MigrationType::Simple,
                    Cow::Borrowed(EVENT_DEDUP_STATE_IMMUTABILITY_SQL),
                    false,
                ),
                Migration::new(
                    17,
                    Cow::Borrowed("message delivery integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(MESSAGE_DELIVERY_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    18,
                    Cow::Borrowed("message delivery insert state guard"),
                    MigrationType::Simple,
                    Cow::Borrowed(MESSAGE_DELIVERY_INSERT_STATE_GUARD_SQL),
                    false,
                ),
                Migration::new(
                    19,
                    Cow::Borrowed("notification attention integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(NOTIFICATION_ATTENTION_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    20,
                    Cow::Borrowed("permission approval integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(PERMISSION_APPROVAL_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    21,
                    Cow::Borrowed("approval policy linkage"),
                    MigrationType::Simple,
                    Cow::Borrowed(APPROVAL_POLICY_LINKAGE_SQL),
                    false,
                ),
                Migration::new(
                    22,
                    Cow::Borrowed("command idempotency"),
                    MigrationType::Simple,
                    Cow::Borrowed(COMMAND_IDEMPOTENCY_SQL),
                    false,
                ),
                Migration::new(
                    23,
                    Cow::Borrowed("notification execution integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(NOTIFICATION_EXECUTION_SQL),
                    false,
                ),
                Migration::new(
                    24,
                    Cow::Borrowed("outbox retry default"),
                    MigrationType::Simple,
                    Cow::Borrowed(OUTBOX_RETRY_DEFAULT_SQL),
                    false,
                ),
                Migration::new(
                    25,
                    Cow::Borrowed("message routing team integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(MESSAGE_ROUTING_TEAM_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    26,
                    Cow::Borrowed("core history retention"),
                    MigrationType::Simple,
                    Cow::Borrowed(CORE_HISTORY_RETENTION_SQL),
                    false,
                ),
                Migration::new(
                    27,
                    Cow::Borrowed("run transition integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(RUN_TRANSITION_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    28,
                    Cow::Borrowed("retained entity integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(RETAINED_ENTITY_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    29,
                    Cow::Borrowed("run terminal integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(RUN_TERMINAL_INTEGRITY_SQL),
                    false,
                ),
                Migration::new(
                    30,
                    Cow::Borrowed("message route overrides"),
                    MigrationType::Simple,
                    Cow::Borrowed(MESSAGE_ROUTE_OVERRIDES_SQL),
                    false,
                ),
                Migration::new(
                    31,
                    Cow::Borrowed("command failure details"),
                    MigrationType::Simple,
                    Cow::Borrowed(include_str!(
                        "../migrations/0031_command_failure_details.sql"
                    )),
                    false,
                ),
                Migration::new(
                    32,
                    Cow::Borrowed("audit expiry projection"),
                    MigrationType::Simple,
                    Cow::Borrowed(include_str!(
                        "../migrations/0032_audit_expiry_projection.sql"
                    )),
                    false,
                ),
                Migration::new(
                    33,
                    Cow::Borrowed("message route attempt binding"),
                    MigrationType::Simple,
                    Cow::Borrowed(MESSAGE_ROUTE_ATTEMPT_BINDING_SQL),
                    false,
                ),
                Migration::new(
                    34,
                    Cow::Borrowed("notification dedupe retention"),
                    MigrationType::Simple,
                    Cow::Borrowed(NOTIFICATION_DEDUPE_RETENTION_SQL),
                    false,
                ),
                Migration::new(
                    35,
                    Cow::Borrowed("attention source reference integrity"),
                    MigrationType::Simple,
                    Cow::Borrowed(ATTENTION_SOURCE_REFERENCE_SQL),
                    false,
                ),
            ])
        })
    }
}

/// Applies every forward migration known to this crate to the supplied pool.
///
/// Callers must create SQLite connections with `foreign_keys` enabled, as that
/// setting is connection-local in SQLite.
pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx_core::migrate::MigrateError> {
    Migrator::new(EmbeddedMigrations).await?.run(pool).await
}

#[cfg(test)]
mod route_migration_registration_tests {
    use super::{EmbeddedMigrations, MigrationSource};

    #[tokio::test]
    async fn route_binding_upgrade_is_registered_as_one_transaction() {
        let rows = EmbeddedMigrations
            .resolve()
            .await
            .expect("embedded migrations");
        let route = rows
            .iter()
            .find(|row| row.version == 33)
            .expect("registered 0033");
        assert!(
            !route.no_tx,
            "HC01_REGISTERED_MIGRATION_MUST_BE_TRANSACTIONAL"
        );
        assert_eq!(
            route.sql.as_ref(),
            include_str!("../migrations/0033_message_route_attempt_binding.sql")
        );
        assert_eq!(
            rows.iter().map(|row| row.version).collect::<Vec<_>>(),
            (1..=35).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn notification_dedupe_upgrade_is_registered_as_one_transaction() {
        let rows = EmbeddedMigrations
            .resolve()
            .await
            .expect("embedded migrations");
        let migration = rows
            .iter()
            .find(|row| row.version == 34)
            .expect("registered 0034");
        assert!(
            !migration.no_tx,
            "NOTIFICATION_DEDUPE_MIGRATION_MUST_BE_TRANSACTIONAL"
        );
        assert_eq!(
            migration.sql.as_ref(),
            include_str!("../migrations/0034_notification_dedupe_retention.sql")
        );
    }

    #[tokio::test]
    async fn attention_source_upgrade_is_registered_as_one_transaction() {
        let rows = EmbeddedMigrations
            .resolve()
            .await
            .expect("embedded migrations");
        let migration = rows
            .iter()
            .find(|row| row.version == 35)
            .expect("registered 0035");
        assert!(
            !migration.no_tx,
            "ATTENTION_SOURCE_MIGRATION_MUST_BE_TRANSACTIONAL"
        );
        assert_eq!(
            migration.sql.as_ref(),
            include_str!("../migrations/0035_attention_source_reference_integrity.sql")
        );
    }
}
