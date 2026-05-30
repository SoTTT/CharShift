// 在非调试构建时隐藏控制台窗口（仅 Windows）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::sync::Mutex;

// 引入文件处理模块和共享类型模块
mod file;
mod types;

use types::*;

// ========================================
// 前后端通信用的数据结构
// ========================================

/// 编码检测进度消息，通过 Channel 流式推送给前端
#[derive(Clone, Serialize)]
struct DetectProgress {
    pub id: u64,
    pub encoding: Option<String>,
    pub completed: usize,
    pub total: usize,
}

/// 单个编码检测任务
#[derive(Serialize, Deserialize, Clone)]
pub struct DetectTask {
    pub id: u64,
    pub path: String,
}

/// 单个文件转换任务，携带扫描时的元数据用于一致性校验
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

/// 拖放文件检测结果
#[derive(Serialize, Deserialize)]
pub struct FileCheckResult {
    pub path: String,
    pub name: String,
    pub is_text: bool,
}

// ========================================
// Tauri IPC 命令（前端通过 invoke 调用）
// ========================================

/// 打开原生目录选择对话框，返回用户选择的目录路径
#[tauri::command]
fn pick_directory() -> Result<Option<String>, String> {
    let path = rfd::FileDialog::new().pick_folder();
    Ok(path.map(|p| p.to_string_lossy().to_string()))
}

/// 递归扫描指定目录，返回文件树节点列表
/// 
/// 在阻塞线程池中执行，避免阻塞 async runtime
#[tauri::command]
async fn scan_directory(path: String, exclude_binary: bool) -> Result<Vec<FileNode>, String> {
    let path = PathBuf::from(path);
    // 目录扫描是 IO 密集型阻塞操作，放到 spawn_blocking 中执行
    let nodes = tokio::task::spawn_blocking(move || {
        file::scanner::scan_directory(&path, exclude_binary)
    })
    .await
    .map_err(|e| e.to_string())?;
    nodes
}

/// 批量同步检测编码（一次性返回全部结果）
/// 
/// 适用于文件数量较少的场景
#[tauri::command]
async fn detect_encodings(tasks: Vec<DetectTask>) -> Result<Vec<serde_json::Value>, String> {
    let mut results = Vec::new();
    // 逐个顺序检测，适合少量文件
    for task in tasks {
        let enc = file::detector::detect_encoding(task.id, &PathBuf::from(&task.path));
        results.push(serde_json::json!({
            "id": enc.node_id,
            "encoding": enc.encoding,
        }));
    }
    Ok(results)
}

/// 批量流式检测编码（通过 Channel 实时推送进度）
/// 
/// 最大并发 16 个，适合大量文件场景，前端可实时更新进度条
#[tauri::command]
async fn detect_encodings_stream(
    tasks: Vec<DetectTask>,
    on_progress: Channel<DetectProgress>,
) -> Result<(), String> {
    let total = tasks.len();
    if total == 0 {
        return Ok(());
    }

    // 使用信号量限制并发数，避免同时打开过多文件
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
    // 原子计数器，跟踪已完成数量
    let completed = std::sync::Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    // 为每个检测任务创建一个异步任务
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
            let _permit = permit; // 持有 permit 直到任务结束
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

    // 等待所有检测任务完成
    for h in handles {
        let _ = h.await;
    }

    Ok(())
}

/// 批量转换文件编码
/// 
/// 最大并发 4 个，每个任务独立执行：校验 → 读取 → 解码 → 重新编码 → 原子写入
#[tauri::command]
async fn convert_files(
    tasks: Vec<ConvertTask>,
    target_encoding: String,
) -> Result<Vec<serde_json::Value>, String> {
    // 将目标编码字符串解析为 Encoding 枚举
    let target = parse_encoding(&target_encoding).ok_or("未知编码")?;
    // 使用信号量限制并发数为 4，避免磁盘 IO 竞争
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut handles = Vec::new();

    // 为每个转换任务创建一个异步任务
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
            // 调用 converter 执行实际的编码转换
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

    // 收集所有转换结果
    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }
    Ok(results)
}

/// 检查给定路径列表是否为文本文件（用于拖放时的快速筛选）
#[tauri::command]
fn check_text_files(paths: Vec<String>) -> Vec<FileCheckResult> {
    let mut results = Vec::new();
    for path_str in paths {
        let path = PathBuf::from(&path_str);
        // 提取文件名（不含路径）
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        // 判断是否为普通文件且为文本内容
        let is_text = path.is_file() && file::scanner::is_text_file(&path);
        results.push(FileCheckResult {
            path: path_str,
            name,
            is_text,
        });
    }
    results
}

/// 获取所有支持的目标编码名称列表（供前端下拉框使用）
#[tauri::command]
fn get_available_encodings() -> Vec<String> {
    Encoding::all()
        .into_iter()
        .map(|e| e.name().to_string())
        .collect()
}

/// 批量锁定文件（Windows 独占模式，其他平台占位）
#[tauri::command]
async fn lock_files(
    paths: Vec<String>,
    locker: tauri::State<'_, Arc<Mutex<file::locker::FileLocker>>>,
) -> Result<Vec<file::locker::LockResult>, String> {
    let mut lock = locker.lock().await;
    let results: Vec<_> = paths.iter().map(|p| lock.lock_file(Path::new(p))).collect();
    Ok(results)
}

/// 批量解锁文件
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

/// 解锁所有已锁定的文件
#[tauri::command]
async fn unlock_all_files(
    locker: tauri::State<'_, Arc<Mutex<file::locker::FileLocker>>>,
) -> Result<(), String> {
    let mut lock = locker.lock().await;
    lock.unlock_all();
    Ok(())
}

// ========================================
// 辅助函数
// ========================================

/// 将编码名称字符串解析为 Encoding 枚举
fn parse_encoding(name: &str) -> Option<Encoding> {
    Encoding::from_name(name)
}

// ========================================
// 应用入口
// ========================================

fn main() {
    // 创建全局文件锁管理器，通过 Tauri State 共享给所有命令
    let file_locker = Arc::new(Mutex::new(file::locker::FileLocker::new()));

    tauri::Builder::default()
        .manage(file_locker)
        // 注册所有 IPC 命令，前端通过 invoke 调用
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
