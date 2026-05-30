use crate::types::NodeId;

// 编码检测常量
const SAMPLE_SIZE: usize = 8 * 1024;     // 采样大小：读取文件前 8KB
const CONFIDENCE_THRESHOLD: f32 = 0.6;   // chardet 置信度阈值

// ========================================
// 检测结果结构
// ========================================

/// 单个文件的编码检测结果
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub node_id: NodeId,
    /// 检测到的编码名称，如 "UTF-8"；检测失败则为 None
    pub encoding: Option<String>,
}

// ========================================
// BOM 检测
// ========================================

/// 检查数据头是否有已知的 BOM（Byte Order Mark）
/// 
/// 支持的 BOM：UTF-8-BOM、UTF-16LE、UTF-16BE、UTF-32BE
fn detect_bom(data: &[u8]) -> Option<String> {
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Some("UTF-8-BOM".to_string())
    } else if data.starts_with(&[0xFF, 0xFE]) {
        Some("UTF-16LE".to_string())
    } else if data.starts_with(&[0xFE, 0xFF]) {
        Some("UTF-16BE".to_string())
    } else if data.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        Some("UTF-16BE".to_string())
    } else {
        None
    }
}

// ========================================
// chardet 结果映射
// ========================================

/// 将 chardet 库返回的 charset 名称映射为内部编码名称
fn chardet_to_encoding(charset: &str) -> Option<String> {
    let name = charset.to_lowercase();
    match name.as_str() {
        "utf-8" => Some("UTF-8".to_string()),
        "gb2312" | "gbk" => Some("GBK".to_string()),
        "gb18030" => Some("GB18030".to_string()),
        "big5" => Some("BIG5".to_string()),
        "iso-8859-1" => Some("ISO-8859-1".to_string()),
        "windows-1252" => Some("WINDOWS-1252".to_string()),
        "utf-16le" => Some("UTF-16LE".to_string()),
        "utf-16be" => Some("UTF-16BE".to_string()),
        _ => None,
    }
}

// ========================================
// 编码检测主函数
// ========================================

/// 检测单个文件的编码
/// 
/// 检测策略（按优先级）：
/// 1. BOM 检测 — 检查文件头是否有已知 BOM
/// 2. chardet 频率分析 — 读取前 8KB 进行字符频率统计
/// 3. UTF-8 启发式 — 若 chardet 置信度不足，尝试按 UTF-8 解码
/// 
/// # 参数
/// - `node_id`: 文件节点 ID（用于返回结果关联）
/// - `path`: 文件路径
pub fn detect_encoding(node_id: NodeId, path: &std::path::Path) -> DetectionResult {
    use std::io::Read;

    // 打开文件，失败则返回 None
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return DetectionResult {
                node_id,
                encoding: None,
            };
        }
    };

    // 读取前 8KB 作为分析样本
    let mut buffer = vec![0u8; SAMPLE_SIZE];
    let n = match file.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => {
            return DetectionResult {
                node_id,
                encoding: None,
            };
        }
    };
    buffer.truncate(n);

    // 策略 1: BOM 检测（最快最准确）
    if let Some(enc) = detect_bom(&buffer) {
        return DetectionResult {
            node_id,
            encoding: Some(enc),
        };
    }

    // 空文件默认当作 UTF-8
    if buffer.is_empty() {
        return DetectionResult {
            node_id,
            encoding: Some("UTF-8".to_string()),
        };
    }

    // 策略 2: chardet 频率分析
    let (charset, confidence, _language) = chardet::detect(&buffer);

    if confidence >= CONFIDENCE_THRESHOLD {
        if let Some(enc) = chardet_to_encoding(&charset) {
            return DetectionResult {
                node_id,
                encoding: Some(enc),
            };
        }
    }

    // 策略 3: 启发式回退 — 尝试按 UTF-8 解码，成功则标记 UTF-8
    if std::str::from_utf8(&buffer).is_ok() {
        return DetectionResult {
            node_id,
            encoding: Some("UTF-8".to_string()),
        };
    }

    // 所有策略均失败
    DetectionResult {
        node_id,
        encoding: None,
    }
}
