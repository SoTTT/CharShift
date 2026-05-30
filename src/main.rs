#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::sync::Mutex;

mod file;
mod types;

use types::*;

#[derive(Clone, Serialize)]
struct DetectProgress {
    pub id: u64,
    pub encoding: Option<String>,
    pub completed: usize,
    pub total: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DetectTask {
    pub id: u64,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ConvertTask {
    pub id: u64,
    pub path: String,
    pub source_encoding: Option<String>,
    /// 扫描时记录的文件大小，转换前用于一致性校验
    pub expected_size: Option<u64>,
    /// 扫描时记录的修改时间（UNIX 秒级），转换前用于一致性校验
    pub expected_modified: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct FileCheckResult {
    pub path: String,
    pub name: String,
    pub is_text: bool,
}

#[tauri::command]
fn pick_directory() -> Result<Option<String>, String> {
    let path = rfd::FileDialog::new().pick_folder();
    Ok(path.map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
async fn scan_directory(path: String, exclude_binary: bool) -> Result<Vec<FileNode>, String> {
    let path = PathBuf::from(path);
    let nodes = tokio::task::spawn_blocking(move || {
        file::scanner::scan_directory(&path, exclude_binary)
    })
    .await
    .map_err(|e| e.to_string())?;
    nodes
}

#[tauri::command]
async fn detect_encodings(tasks: Vec<DetectTask>) -> Result<Vec<serde_json::Value>, String> {
    let mut results = Vec::new();
    for task in tasks {
        let enc = file::detector::detect_encoding(task.id, &PathBuf::from(&task.path));
        results.push(serde_json::json!({
            "id": enc.node_id,
            "encoding": enc.encoding,
        }));
    }
    Ok(results)
}

#[tauri::command]
async fn detect_encodings_stream(
    tasks: Vec<DetectTask>,
    on_progress: Channel<DetectProgress>,
) -> Result<(), String> {
    let total = tasks.len();
    if total == 0 {
        return Ok(());
    }

    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
    let completed = std::sync::Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for task in tasks {
        let permit = sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let path = task.path.clone();
        let id = task.id;
        let on_progress = on_progress.clone();
        let completed = completed.clone();
        let handle = tokio::spawn(async move {
            let _permit = permit;
            let enc = file::detector::detect_encoding(id, &PathBuf::from(&path));
            let c = completed.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = on_progress.send(DetectProgress {
                id: enc.node_id,
                encoding: enc.encoding,
                completed: c,
                total,
            });
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    Ok(())
}

#[tauri::command]
async fn convert_files(
    tasks: Vec<ConvertTask>,
    target_encoding: String,
) -> Result<Vec<serde_json::Value>, String> {
    let target = parse_encoding(&target_encoding).ok_or("未知编码")?;
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut handles = Vec::new();

    for task in tasks {
        let permit = sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let source = task.source_encoding.clone();
        let path = task.path.clone();
        let id = task.id;
        let target = target.clone();
        let expected_size = task.expected_size;
        let expected_modified = task.expected_modified;
        let handle = tokio::spawn(async move {
            let _permit = permit;
            let result = file::converter::convert_file(
                id,
                &PathBuf::from(&path),
                source,
                target,
                expected_size,
                expected_modified,
            );
            serde_json::json!({
                "id": result.node_id,
                "success": result.result.is_ok(),
                "error": result.result.err(),
            })
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }
    Ok(results)
}

#[tauri::command]
fn check_text_files(paths: Vec<String>) -> Vec<FileCheckResult> {
    let mut results = Vec::new();
    for path_str in paths {
        let path = PathBuf::from(&path_str);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let is_text = path.is_file() && file::scanner::is_text_file(&path);
        results.push(FileCheckResult {
            path: path_str,
            name,
            is_text,
        });
    }
    results
}

#[tauri::command]
fn get_available_encodings() -> Vec<String> {
    Encoding::all()
        .into_iter()
        .map(|e| e.name().to_string())
        .collect()
}

#[tauri::command]
async fn lock_files(
    paths: Vec<String>,
    locker: tauri::State<'_, Arc<Mutex<file::locker::FileLocker>>>,
) -> Result<Vec<file::locker::LockResult>, String> {
    let mut lock = locker.lock().await;
    let results: Vec<_> = paths.iter().map(|p| lock.lock_file(Path::new(p))).collect();
    Ok(results)
}

#[tauri::command]
async fn unlock_files(
    paths: Vec<String>,
    locker: tauri::State<'_, Arc<Mutex<file::locker::FileLocker>>>,
) -> Result<(), String> {
    let mut lock = locker.lock().await;
    for p in paths {
        lock.unlock_file(Path::new(&p));
    }
    Ok(())
}

#[tauri::command]
async fn unlock_all_files(
    locker: tauri::State<'_, Arc<Mutex<file::locker::FileLocker>>>,
) -> Result<(), String> {
    let mut lock = locker.lock().await;
    lock.unlock_all();
    Ok(())
}

fn parse_encoding(name: &str) -> Option<Encoding> {
    Encoding::from_name(name)
}

fn main() {
    let file_locker = Arc::new(Mutex::new(file::locker::FileLocker::new()));

    tauri::Builder::default()
        .manage(file_locker)
        .invoke_handler(tauri::generate_handler![
            pick_directory,
            scan_directory,
            detect_encodings,
            detect_encodings_stream,
            convert_files,
            check_text_files,
            get_available_encodings,
            lock_files,
            unlock_files,
            unlock_all_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
