use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use archon_core::split::{JoinReport, SplitReport, join_volumes, split_archive};
use archon_core::{
    ArchiveInfo, ArchiveListing, CreateOptions, CreateReport, EngineRegistry, ExtractOptions,
    ExtractReport, FormatCapability, OperationContext, TestReport,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct AppState {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: String,
    message: String,
}

impl From<archon_core::ArchonError> for CommandError {
    fn from(value: archon_core::ArchonError) -> Self {
        Self {
            code: value.code().to_string(),
            message: value.to_string(),
        }
    }
}

impl From<String> for CommandError {
    fn from(value: String) -> Self {
        Self {
            code: "internal".into(),
            message: value,
        }
    }
}

fn join_error(error: impl std::fmt::Display) -> CommandError {
    CommandError {
        code: "internal".into(),
        message: format!("Archive worker failed: {error}"),
    }
}

#[tauri::command]
pub async fn format_capabilities() -> Result<Vec<FormatCapability>, CommandError> {
    Ok(EngineRegistry::new().capabilities())
}

#[tauri::command]
pub async fn list_archive(
    path: String,
    password: Option<String>,
) -> Result<ArchiveListing, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        EngineRegistry::new().list(&PathBuf::from(path), password.as_deref())
    })
    .await
    .map_err(join_error)?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn archive_info(
    path: String,
    password: Option<String>,
) -> Result<ArchiveInfo, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        EngineRegistry::new().info(&PathBuf::from(path), password.as_deref())
    })
    .await
    .map_err(join_error)?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_archive(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    options: CreateOptions,
) -> Result<CreateReport, CommandError> {
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .expect("job registry poisoned")
        .insert(job_id.clone(), cancel.clone());

    let context =
        OperationContext::with_cancel(job_id.clone(), cancel).with_reporter(move |event| {
            let _ = app.emit("archon://progress", event);
        });

    let result = tauri::async_runtime::spawn_blocking(move || {
        EngineRegistry::new().create(&options, &context)
    })
    .await;

    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.remove(&job_id);
    }

    result.map_err(join_error)?.map_err(CommandError::from)
}

#[tauri::command]
pub async fn extract_archive(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    options: ExtractOptions,
) -> Result<ExtractReport, CommandError> {
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .expect("job registry poisoned")
        .insert(job_id.clone(), cancel.clone());

    let context =
        OperationContext::with_cancel(job_id.clone(), cancel).with_reporter(move |event| {
            let _ = app.emit("archon://progress", event);
        });

    let result = tauri::async_runtime::spawn_blocking(move || {
        EngineRegistry::new().extract(&options, &context)
    })
    .await;

    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.remove(&job_id);
    }

    result.map_err(join_error)?.map_err(CommandError::from)
}

#[tauri::command]
pub async fn test_archive(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    path: String,
    password: Option<String>,
) -> Result<TestReport, CommandError> {
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .expect("job registry poisoned")
        .insert(job_id.clone(), cancel.clone());

    let context =
        OperationContext::with_cancel(job_id.clone(), cancel).with_reporter(move |event| {
            let _ = app.emit("archon://progress", event);
        });

    let result = tauri::async_runtime::spawn_blocking(move || {
        EngineRegistry::new().test(&PathBuf::from(path), password.as_deref(), &context)
    })
    .await;

    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.remove(&job_id);
    }

    result.map_err(join_error)?.map_err(CommandError::from)
}

#[tauri::command]
pub async fn add_entries(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    archive: String,
    sources: Vec<String>,
) -> Result<CreateReport, CommandError> {
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .expect("job registry poisoned")
        .insert(job_id.clone(), cancel.clone());

    let context =
        OperationContext::with_cancel(job_id.clone(), cancel).with_reporter(move |event| {
            let _ = app.emit("archon://progress", event);
        });

    let sources: Vec<PathBuf> = sources.into_iter().map(PathBuf::from).collect();
    let result = tauri::async_runtime::spawn_blocking(move || {
        EngineRegistry::new().add_entries(&PathBuf::from(archive), &sources, &context)
    })
    .await;

    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.remove(&job_id);
    }

    result.map_err(join_error)?.map_err(CommandError::from)
}

#[tauri::command]
pub async fn remove_entries(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    archive: String,
    entries: Vec<String>,
) -> Result<(), CommandError> {
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .expect("job registry poisoned")
        .insert(job_id.clone(), cancel.clone());

    let context =
        OperationContext::with_cancel(job_id.clone(), cancel).with_reporter(move |event| {
            let _ = app.emit("archon://progress", event);
        });

    let result = tauri::async_runtime::spawn_blocking(move || {
        EngineRegistry::new().remove_entries(&PathBuf::from(archive), &entries, &context)
    })
    .await;

    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.remove(&job_id);
    }

    result.map_err(join_error)?.map_err(CommandError::from)
}

#[tauri::command]
pub async fn split_archive_volumes(
    path: String,
    size_mb: u64,
) -> Result<SplitReport, CommandError> {
    if size_mb == 0 {
        return Err(CommandError {
            code: "configuration".into(),
            message: "--size-mb must be greater than zero".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || {
        split_archive(&PathBuf::from(path), size_mb.saturating_mul(1024 * 1024))
    })
    .await
    .map_err(join_error)?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn join_volumes_command(
    first: String,
    output: String,
) -> Result<JoinReport, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        join_volumes(&PathBuf::from(first), &PathBuf::from(output))
    })
    .await
    .map_err(join_error)?
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) -> bool {
    let Ok(jobs) = state.jobs.lock() else {
        return false;
    };
    let Some(token) = jobs.get(&job_id) else {
        return false;
    };
    token.store(true, std::sync::atomic::Ordering::SeqCst);
    true
}
