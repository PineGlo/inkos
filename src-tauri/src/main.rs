use directories::ProjectDirs;
use inkos_core::agents::AiOrchestrator;
use inkos_core::api::v1::{self, ApiState};
use inkos_core::db::init_db;
use inkos_core::model_manager::ModelManager;
use inkos_core::summarizer::Summarizer;
use inkos_core::workers::JobScheduler;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

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
            let orchestrator =
                Arc::new(AiOrchestrator::new().expect("failed to initialise AI orchestrator"));
            let model_manager = ModelManager::new(db.clone(), Arc::clone(&orchestrator));
            let summarizer = Summarizer::new(db.clone(), Arc::clone(&model_manager));
            let scheduler = JobScheduler::new(db.clone(), Arc::clone(&summarizer));
            if let Err(err) = scheduler.ensure_nightly_digest_schedule_blocking() {
                eprintln!("failed to prime nightly digest schedule: {err}");
            }
            app.manage(ApiState {
                db,
                model_manager,
                summarizer,
                scheduler,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            v1::ping,
            v1::db_status,
            v1::create_note,
            v1::list_notes,
            v1::list_logbook_entries,
            v1::list_timeline_events,
            v1::list_ai_events,
            v1::run_daily_digest,
            v1::ai_list_providers,
            v1::ai_list_models,
            v1::ai_get_settings,
            v1::ai_update_settings,
            v1::ai_chat,
            v1::chat_create_conversation,
            v1::chat_list_conversations,
            v1::chat_get_messages,
            v1::chat_append_and_maybe_rollover,
            v1::ai_rollover_chat,
            v1::ai_set_model,
            v1::ai_summarize,
            v1::ai_get_summary
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
