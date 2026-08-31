use std::path::PathBuf;

use fpv_ai::ProviderConfig;
use fpv_app::{AppState, ExecuteOutcome};
use fpv_core::{Command, Project};
use tauri::State;

type AppResult<T> = Result<T, String>;

fn app_error<T>(result: anyhow::Result<T>) -> AppResult<T> {
    result.map_err(|error| error.to_string())
}

#[tauri::command]
async fn timeline(state: State<'_, AppState>) -> AppResult<Project> {
    Ok(state.get_timeline_state().await)
}

#[tauri::command]
async fn execute(state: State<'_, AppState>, command: Command) -> AppResult<ExecuteOutcome> {
    app_error(state.execute_command(command).await)
}

#[tauri::command]
async fn undo(state: State<'_, AppState>) -> AppResult<ExecuteOutcome> {
    app_error(state.undo().await)
}

#[tauri::command]
async fn redo(state: State<'_, AppState>) -> AppResult<ExecuteOutcome> {
    app_error(state.redo().await)
}

#[tauri::command]
async fn load_project(state: State<'_, AppState>, path: String) -> AppResult<Project> {
    app_error(state.load_project(&PathBuf::from(path)).await)
}

#[tauri::command]
async fn save_project(state: State<'_, AppState>, path: String) -> AppResult<()> {
    app_error(state.save_project(&PathBuf::from(path)).await)
}

#[tauri::command]
fn probe_media(state: State<'_, AppState>, path: String) -> AppResult<fpv_media::MediaInfo> {
    app_error(state.probe_media(&PathBuf::from(path)))
}

#[tauri::command]
async fn configure_ai(state: State<'_, AppState>, config: ProviderConfig) -> AppResult<()> {
    state.configure_ai(config).await;
    Ok(())
}

#[tauri::command]
async fn test_ai_connection(state: State<'_, AppState>) -> AppResult<()> {
    app_error(state.test_ai_connection().await)
}

#[tauri::command]
async fn chat(state: State<'_, AppState>, prompt: String) -> AppResult<String> {
    app_error(state.chat(&prompt).await)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            timeline,
            execute,
            undo,
            redo,
            load_project,
            save_project,
            probe_media,
            configure_ai,
            test_ai_connection,
            chat
        ])
        .run(tauri::generate_context!())
        .expect("error while running FPV Editor");
}
