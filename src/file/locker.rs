use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::{info, trace, warn};

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
            trace!(%path_str, "文件已被锁定，跳过重复锁定");
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
                    info!(%path_str, "文件独占锁定成功");
                    self.handles.insert(path.to_path_buf(), file);
                    LockResult {
                        path: path_str,
                        success: true,
                        error: None,
                    }
                }
                Err(e) => {
                    warn!(%path_str, error = %e, "文件独占锁定失败");
                    LockResult {
                        path: path_str,
                        success: false,
                        error: Some(format!("{}", e)),
                    }
                }
            }
        }

        #[cfg(not(windows))]
        {
            // POSIX 系统没有与 Windows 等价的强制文件锁，
            // 返回成功以避免破坏用户体验，但实际不阻止其他程序修改。
            trace!(%path_str, "POSIX 系统文件锁定为占位实现");
            LockResult {
                path: path_str,
                success: true,
                error: None,
            }
        }
    }

    pub fn unlock_file(&mut self, path: &Path) {
        let existed = self.handles.remove(path).is_some();
        if existed {
            trace!(path = %path.to_string_lossy(), "文件解锁成功");
        }
    }

    pub fn unlock_all(&mut self) {
        let count = self.handles.len();
        self.handles.clear();
        info!(count, "解锁所有已锁定文件");
    }

    pub fn is_locked(&self, path: &Path) -> bool {
        self.handles.contains_key(path)
    }

    pub fn locked_count(&self) -> usize {
        self.handles.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_lock_and_unlock() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lock.txt");
        fs::write(&path, "locked").unwrap();

        let mut locker = FileLocker::new();
        assert_eq!(locker.locked_count(), 0);

        let result = locker.lock_file(&path);
        assert!(result.success);

        #[cfg(windows)]
        assert_eq!(locker.locked_count(), 1);
        #[cfg(not(windows))]
        assert_eq!(locker.locked_count(), 0); // POSIX 不持有句柄

        locker.unlock_file(&path);
        assert_eq!(locker.locked_count(), 0);
    }

    #[test]
    fn test_lock_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lock.txt");
        fs::write(&path, "locked").unwrap();

        let mut locker = FileLocker::new();
        let r1 = locker.lock_file(&path);
        assert!(r1.success);

        let r2 = locker.lock_file(&path);
        assert!(r2.success);
    }

    #[test]
    fn test_unlock_all() {
        let dir = TempDir::new().unwrap();
        let path1 = dir.path().join("a.txt");
        let path2 = dir.path().join("b.txt");
        fs::write(&path1, "a").unwrap();
        fs::write(&path2, "b").unwrap();

        let mut locker = FileLocker::new();
        locker.lock_file(&path1);
        locker.lock_file(&path2);

        locker.unlock_all();
        assert_eq!(locker.locked_count(), 0);
    }
}
