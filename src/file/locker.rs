use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct LockResult {
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
}

pub struct FileLocker {
    handles: HashMap<PathBuf, File>,
}

impl FileLocker {
    pub fn new() -> Self {
        Self {
            handles: HashMap::new(),
        }
    }

    /// 以独占模式打开文件，阻止其他进程读写。
    /// Windows 下使用 share_mode(0)（FILE_SHARE_NONE）实现强制锁；
    /// 非 Windows 平台目前仅返回成功，不提供跨进程强制保护。
    pub fn lock_file(&mut self, path: &Path) -> LockResult {
        let path_str = path.to_string_lossy().to_string();

        // 已锁定则直接返回成功
        if self.handles.contains_key(path) {
            return LockResult {
                path: path_str,
                success: true,
                error: None,
            };
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            let result = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0) // FILE_SHARE_NONE
                .open(path);

            match result {
                Ok(file) => {
                    self.handles.insert(path.to_path_buf(), file);
                    LockResult {
                        path: path_str,
                        success: true,
                        error: None,
                    }
                }
                Err(e) => LockResult {
                    path: path_str,
                    success: false,
                    error: Some(format!("{}", e)),
                },
            }
        }

        #[cfg(not(windows))]
        {
            // POSIX 系统没有与 Windows 等价的强制文件锁，
            // 返回成功以避免破坏用户体验，但实际不阻止其他程序修改。
            LockResult {
                path: path_str,
                success: true,
                error: None,
            }
        }
    }

    pub fn unlock_file(&mut self, path: &Path) {
        self.handles.remove(path);
    }

    pub fn unlock_all(&mut self) {
        self.handles.clear();
    }

    pub fn is_locked(&self, path: &Path) -> bool {
        self.handles.contains_key(path)
    }

    pub fn locked_count(&self) -> usize {
        self.handles.len()
    }
}
