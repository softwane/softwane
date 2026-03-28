use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct ManagedObservability {
    state: Mutex<ObservabilityState>,
}

#[derive(Default)]
struct ObservabilityState {
    file_path: Option<PathBuf>,
}

#[derive(Serialize)]
struct LogEntry<T> {
    timestamp_ms: u64,
    kind: String,
    payload: T,
}

pub fn log_event<T: Serialize>(app_handle: &AppHandle, kind: &str, payload: T) {
    let managed = app_handle.state::<ManagedObservability>();
    let mut state = managed
        .state
        .lock()
        .expect("observability state lock poisoned");

    let Some(path) = resolve_log_path(app_handle, &mut state) else {
        eprintln!("failed to resolve observability log path");
        return;
    };

    let entry = LogEntry {
        timestamp_ms: unix_time_ms(),
        kind: kind.to_string(),
        payload,
    };

    let Ok(mut line) = serde_json::to_vec(&entry) else {
        eprintln!("failed to encode observability log entry");
        return;
    };
    line.push(b'\n');

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        eprintln!("failed to open observability log file");
        return;
    };

    if file.write_all(&line).is_err() {
        eprintln!("failed to write observability log entry");
    }
}

fn resolve_log_path(app_handle: &AppHandle, state: &mut ObservabilityState) -> Option<PathBuf> {
    if let Some(existing) = &state.file_path {
        return Some(existing.clone());
    }

    let base_dir = app_handle
        .path()
        .app_log_dir()
        .or_else(|_| app_handle.path().app_local_data_dir())
        .ok()?;

    if fs::create_dir_all(&base_dir).is_err() {
        return None;
    }

    let path = base_dir.join("observability.jsonl");
    state.file_path = Some(path.clone());
    Some(path)
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock went backwards")
        .as_millis() as u64
}
