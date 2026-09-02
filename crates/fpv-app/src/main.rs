use std::path::PathBuf;

use fpv_ai::ProviderConfig;
use fpv_app::{AppState, ExecuteOutcome, MediaDiagnostics, MediaImportOutcome, UpdateCheckResult};
use fpv_core::{Command, Project};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

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
async fn load_project(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<Project> {
    let project = app_error(state.load_project(&PathBuf::from(path)).await)?;
    for clip in project.clips.values() {
        app.asset_protocol_scope()
            .allow_file(&clip.source_path)
            .map_err(|error| error.to_string())?;
    }
    Ok(project)
}

#[tauri::command]
async fn new_project(state: State<'_, AppState>, name: String) -> AppResult<Project> {
    Ok(state.new_project(name).await)
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
fn media_diagnostics(state: State<'_, AppState>) -> MediaDiagnostics {
    state.media_diagnostics()
}

#[tauri::command]
async fn import_media(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    target_track_id: Option<fpv_core::TrackId>,
) -> AppResult<MediaImportOutcome> {
    let outcome = app_error(
        state
            .import_media_paths(
                paths.into_iter().map(PathBuf::from).collect(),
                target_track_id,
            )
            .await,
    )?;
    for path in &outcome.imported_paths {
        app.asset_protocol_scope()
            .allow_file(path)
            .map_err(|error| error.to_string())?;
    }
    Ok(outcome)
}

#[tauri::command]
async fn render_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    clip_id: Option<fpv_core::ClipId>,
    start: Option<fpv_core::Timecode>,
) -> AppResult<String> {
    let path = app_error(state.render_preview(clip_id, start).await)?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().into_owned())
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

#[tauri::command]
async fn check_for_updates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<UpdateCheckResult> {
    let current_version = app.package_info().version.to_string();
    app_error(state.check_for_updates(&current_version).await)
}

#[tauri::command]
async fn download_update(
    app: AppHandle,
    state: State<'_, AppState>,
    download_url: String,
    asset_name: String,
) -> AppResult<()> {
    let path = app_error(state.download_update(&download_url, &asset_name).await)?;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            timeline,
            execute,
            undo,
            redo,
            load_project,
            new_project,
            save_project,
            probe_media,
            media_diagnostics,
            import_media,
            render_preview,
            configure_ai,
            test_ai_connection,
            chat,
            check_for_updates,
            download_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running FPV Editor");
}
