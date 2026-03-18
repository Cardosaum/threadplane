#![expect(
    clippy::redundant_pub_crate,
    reason = "Migration helpers are crate-local bootstrap building blocks."
)]

use snafu::ResultExt as _;
use sqlx::migrate::Migrator;
use std::path::Path;

use crate::{error::DatabaseMigration, prelude::*};

#[cfg(test)]
pub(crate) const INITIAL_MIGRATION_SQL: &str = include_str!("../migrations/0001_initial.sql");
#[cfg(test)]
pub(crate) const PROJECTION_OFFSETS_MIGRATION_SQL: &str =
    include_str!("../migrations/0002_projection_offsets.sql");
#[cfg(test)]
pub(crate) const COMMAND_RECEIPTS_MIGRATION_SQL: &str =
    include_str!("../migrations/0003_command_receipts.sql");
#[cfg(test)]
pub(crate) const PERFORMANCE_INDEXES_MIGRATION_SQL: &str =
    include_str!("../migrations/0004_performance_indexes.sql");
#[cfg(test)]
pub(crate) const WORKSPACE_GOVERNANCE_MIGRATION_SQL: &str =
    include_str!("../migrations/0005_workspace_governance.sql");
#[cfg(test)]
pub(crate) const MEMORIES_MIGRATION_SQL: &str = include_str!("../migrations/0006_memories.sql");
const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

pub(crate) async fn run_migrations(pool: &PgPool) -> ServerResult<()> {
    let migrator = Migrator::new(Path::new(MIGRATIONS_DIR))
        .await
        .context(DatabaseMigration)?;
    migrator.run(pool).await.context(DatabaseMigration)
}
