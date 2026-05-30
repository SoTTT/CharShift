use std::path::Path;
use walkdir::{WalkDir, DirEntry};
use crate::types::{FileNode, NodeId, NodeType};

// 扫描限制常量
const MAX_DEPTH: usize = 20;         // 最大递归深度
const SAMPLE_SIZE: usize = 512;      // 文本检测采样大小（字节）
const MAX_SCAN_NODES: usize = 50000; // 最大节点数限制

// ========================================
// 目录过滤辅助函数
// ========================================

/// 判断是否为隐藏文件/目录（以点开头的名称）
fn is_hidden(entry: &DirEntry) -> bool {
    entry.file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// 判断是否为系统目录（需要跳过的目录）
fn is_system_dir(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy().to_lowercase();
    matches!(name.as_str(), "system volume information" | "$recycle.bin" | "config.msi" | "windows")
}

// ========================================
// 文本/二进制判断
// ========================================

/// 判断指定路径是否为文本文件
/// 
/// 读取文件前 512 字节，通过 content_inspector 分析内容特征
pub fn is_text_file(path: &Path) -> bool {
    if let Ok(mut file) = std::fs::File::open(path) {
        use std::io::Read;
        let mut buffer = vec![0u8; SAMPLE_SIZE];
        if let Ok(n) = file.read(&mut buffer) {
            buffer.truncate(n);
            return content_inspector::inspect(&buffer).is_text();
        }
    }
    false
}

// ========================================
// 目录扫描主函数
// ========================================

/// 递归扫描目录，返回文件树节点列表
/// 
/// # 参数
/// - `path`: 要扫描的根目录路径
/// - `exclude_binary`: 是否跳过二进制文件
/// 
/// # 扫描规则
/// - 最大深度 20 层
/// - 跳过隐藏文件和系统目录
/// - 不跟随符号链接
/// - 文本检测仅读前 512 字节
/// - 超过 50000 个节点会报错
pub fn scan_directory(path: &Path, exclude_binary: bool) -> Result<Vec<FileNode>, String> {
    let mut nodes = Vec::new();
    let mut next_id: NodeId = 1;

    // 配置 walkdir 遍历器：限制深度、不跟随链接、过滤隐藏项和系统目录
    let walker = WalkDir::new(path)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if is_system_dir(e) {
                return false;
            }
            if e.depth() == 0 {
                return true;
            }
            !is_hidden(e)
        });

    // 创建根节点（id 固定为 0）
    let root_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    
    nodes.push(FileNode {
        id: 0,
        name: root_name,
        path: path.to_string_lossy().to_string(),
        node_type: NodeType::Directory,
        encoding: None,
        is_expanded: true,
        is_selected: false,
        is_converting: false,
        conversion_error: None,
        parent_id: None,
        children: Vec::new(),
        file_size: None,
        file_modified: None,
    });

    // 遍历目录树
    for entry_result in walker {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue, // 跳过无权限访问的条目
        };

        // 跳过根节点（已在上面处理）
        if entry.depth() == 0 {
            continue;
        }

        let entry_path = entry.path().to_path_buf();
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue, // 跳过无法读取元数据的条目
        };

        let parent_path = entry_path.parent().map(|p| p.to_string_lossy().to_string());

        // 根据文件类型决定节点类型
        let node_type = if metadata.is_dir() {
            NodeType::Directory
        } else if metadata.is_file() {
            if is_text_file(&entry_path) {
                NodeType::TextFile
            } else {
                if exclude_binary {
                    continue; // 开启"排除二进制"时跳过二进制文件
                }
                NodeType::BinaryFile
            }
        } else {
            continue; // 跳过特殊文件（FIFO、socket 等）
        };

        let id = next_id;
        next_id += 1;

        // 在已创建的节点中查找父节点，建立父子关系
        let mut parent_id = None;
        if let Some(ref pp) = parent_path {
            if let Some(parent_node) = nodes.iter_mut().find(|n| &n.path == pp) {
                parent_node.children.push(id);
                parent_id = Some(parent_node.id);
            }
        }

        // 记录文件元数据用于后续转换校验
        let file_size = if metadata.is_file() { Some(metadata.len()) } else { None };
        let file_modified = metadata.modified().ok().and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
        });

        let node = FileNode {
            id,
            name,
            path: entry_path.to_string_lossy().to_string(),
            node_type,
            encoding: None,
            is_expanded: false,
            is_selected: false,
            is_converting: false,
            conversion_error: None,
            parent_id,
            children: Vec::new(),
            file_size,
            file_modified,
        };

        nodes.push(node);

        // 节点数超限保护
        if nodes.len() > MAX_SCAN_NODES {
            return Err(format!(
                "目录中包含过多文件（超过 {} 个），请选择子目录进行扫描",
                MAX_SCAN_NODES
            ));
        }
    }

    Ok(nodes)
}
