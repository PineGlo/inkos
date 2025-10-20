use std::path::PathBuf;
use r2d2_sqlite::SqliteConnectionManager;
use r2d2::Pool;
use rusqlite::Connection;
use anyhow::Result;

pub type DbPool = Pool<SqliteConnectionManager>;

pub fn init_db(workspace_dir: PathBuf) -> Result<DbPool> {
    std::fs::create_dir_all(&workspace_dir)?;
    let db_path = workspace_dir.join("inkos.db");
    let mgr = SqliteConnectionManager::file(&db_path);
    let pool = Pool::new(mgr)?;
    {
        let conn = pool.get()?;
        apply_migrations(&conn)?;
    }
    Ok(pool)
}

fn apply_migrations(conn: &Connection) -> Result<()> {
    let sql = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../migrations/0001_init.sql"));
    conn.execute_batch(sql)?;
    Ok(())
}
