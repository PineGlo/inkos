//! Database bootstrap utilities for the embedded SQLite store.
//!
//! The functions here are responsible for creating the workspace database,
//! applying SQL migrations, and seeding default AI provider records.

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::PathBuf;

use crate::agents::config as ai_config;

/// Shared connection pool type for the SQLite database.
pub type DbPool = Pool<SqliteConnectionManager>;

/// Initialise the workspace database inside the supplied directory.
///
/// This helper ensures the directory exists, opens an `r2d2` pool, runs all
/// migrations, and seeds the AI provider tables with sensible defaults. The
/// resulting pool can then be injected into the Tauri state container.
pub fn init_db(workspace_dir: PathBuf) -> Result<DbPool> {
    std::fs::create_dir_all(&workspace_dir)?;
    let db_path = workspace_dir.join("inkos.db");
    let mgr = SqliteConnectionManager::file(&db_path);
    let pool = Pool::new(mgr)?;
    {
        let conn = pool.get()?;
        apply_migrations(&conn)?;
        ai_config::seed_defaults(&conn)?;
    }
    Ok(pool)
}

/// Apply all embedded SQL migrations in order.
fn apply_migrations(conn: &Connection) -> Result<()> {
    let migrations: &[(&str, &str)] = &[
        (
            "0001_init.sql",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../migrations/0001_init.sql"
            )),
        ),
        (
            "0002_ai_settings.sql",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../migrations/0002_ai_settings.sql"
            )),
        ),
        (
            "0003_logbook_timeline.sql",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../migrations/0003_logbook_timeline.sql"
            )),
        ),
    ];

    for (name, sql) in migrations {
        conn.execute_batch(sql)
            .with_context(|| format!("failed to apply migration {name}"))?;
    }
    Ok(())
}
