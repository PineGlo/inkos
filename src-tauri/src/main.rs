use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use inkos_core::api::v1::{self, ApiState};
use inkos_core::db::init_db;
use inkos_core::agents::AiOrchestrator;
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
            let ai = AiOrchestrator::new().expect("failed to initialise AI orchestrator");
            app.manage(ApiState { db, ai: Arc::new(ai) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            v1::ping,
            v1::db_status,
            v1::create_note,
            v1::list_notes,
            v1::ai_list_providers,
            v1::ai_get_settings,
            v1::ai_update_settings,
            v1::ai_chat
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
