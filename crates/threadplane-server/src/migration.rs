#![expect(
    clippy::redundant_pub_crate,
    reason = "Migration helpers are crate-local bootstrap building blocks."
)]

use sqlx::{migrate::Migrator, PgPool};
use snafu::ResultExt as _;
use std::path::Path;

use crate::error::{DatabaseMigration, ServerResult};

#[cfg(test)]
pub(crate) const INITIAL_MIGRATION_SQL: &str = include_str!("../migrations/0001_initial.sql");
#[cfg(test)]
pub(crate) const PROJECTION_OFFSETS_MIGRATION_SQL: &str =
    include_str!("../migrations/0002_projection_offsets.sql");
const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

pub(crate) async fn run_migrations(pool: &PgPool) -> ServerResult<()> {
    let migrator = Migrator::new(Path::new(MIGRATIONS_DIR))
        .await
        .context(DatabaseMigration)?;
    migrator.run(pool).await.context(DatabaseMigration)
}
