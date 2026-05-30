use std::path::Path;
use walkdir::{WalkDir, DirEntry};
use crate::types::{FileNode, NodeId, NodeType};

const MAX_DEPTH: usize = 20;
const SAMPLE_SIZE: usize = 512;
const MAX_SCAN_NODES: usize = 50000;

fn is_hidden(entry: &DirEntry) -> bool {
    entry.file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

fn is_system_dir(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy().to_lowercase();
    matches!(name.as_str(), "system volume information" | "$recycle.bin" | "config.msi" | "windows")
}

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

pub fn scan_directory(path: &Path, exclude_binary: bool) -> Result<Vec<FileNode>, String> {
    let mut nodes = Vec::new();
    let mut next_id: NodeId = 1;

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

    // Root node
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

    for entry_result in walker {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.depth() == 0 {
            continue;
        }

        let entry_path = entry.path().to_path_buf();
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let parent_path = entry_path.parent().map(|p| p.to_string_lossy().to_string());

        let node_type = if metadata.is_dir() {
            NodeType::Directory
        } else if metadata.is_file() {
            if is_text_file(&entry_path) {
                NodeType::TextFile
            } else {
                if exclude_binary {
                    continue; // skip binary files when exclude_binary is enabled
                }
                NodeType::BinaryFile
            }
        } else {
            continue; // skip special files
        };

        let id = next_id;
        next_id += 1;

        let mut parent_id = None;

        // Find parent
        if let Some(ref pp) = parent_path {
            if let Some(parent_node) = nodes.iter_mut().find(|n| &n.path == pp) {
                parent_node.children.push(id);
                parent_id = Some(parent_node.id);
            }
        }

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

        if nodes.len() > MAX_SCAN_NODES {
            return Err(format!(
                "目录中包含过多文件（超过 {} 个），请选择子目录进行扫描",
                MAX_SCAN_NODES
            ));
        }
    }

    Ok(nodes)
}
