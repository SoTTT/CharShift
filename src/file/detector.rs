use crate::types::NodeId;

const SAMPLE_SIZE: usize = 8 * 1024;
const CONFIDENCE_THRESHOLD: f32 = 0.6;

#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub node_id: NodeId,
    pub encoding: Option<String>,
}

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

pub fn detect_encoding(node_id: NodeId, path: &std::path::Path) -> DetectionResult {
    use std::io::Read;

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return DetectionResult {
                node_id,
                encoding: None,
            };
        }
    };

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

    // 1. BOM detection
    if let Some(enc) = detect_bom(&buffer) {
        return DetectionResult {
            node_id,
            encoding: Some(enc),
        };
    }

    if buffer.is_empty() {
        return DetectionResult {
            node_id,
            encoding: Some("UTF-8".to_string()),
        };
    }

    // 2. chardet analysis
    let (charset, confidence, _language) = chardet::detect(&buffer);

    if confidence >= CONFIDENCE_THRESHOLD {
        if let Some(enc) = chardet_to_encoding(&charset) {
            return DetectionResult {
                node_id,
                encoding: Some(enc),
            };
        }
    }

    // 3. Heuristic fallback: try UTF-8
    if std::str::from_utf8(&buffer).is_ok() {
        return DetectionResult {
            node_id,
            encoding: Some("UTF-8".to_string()),
        };
    }

    DetectionResult {
        node_id,
        encoding: None,
    }
}
