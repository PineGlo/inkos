use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::PathBuf;

use crate::agents::config as ai_config;

pub type DbPool = Pool<SqliteConnectionManager>;

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
