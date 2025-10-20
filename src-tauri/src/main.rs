use std::path::PathBuf;
use tauri::Manager;
use inkos_core::api::v1::{self, ApiState};
use inkos_core::db::init_db;
use directories::ProjectDirs;

fn workspace_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("com", "InkOS", "InkOS") {
        proj.data_dir().to_path_buf()
    } else {
        std::env::temp_dir().join("InkOS")
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let db = init_db(workspace_dir()).expect("failed to init db");
            app.manage(ApiState { db });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            v1::ping,
            v1::db_status,
            v1::create_note,
            v1::list_notes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
