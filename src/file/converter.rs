use crate::types::{Encoding, NodeId};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tracing::{error, info, trace, warn};

#[derive(Debug, Clone)]
pub struct ConversionResult {
    pub node_id: NodeId,
    pub result: Result<(), String>,
}

/// 生成一个几乎不可能冲突的临时文件名后缀。
/// 格式: `.tmp-{pid}-{nanos}`
fn make_temp_extension() -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("tmp-{pid}-{nanos}")
}

#[tracing::instrument]
pub fn convert_file(
    node_id: NodeId,
    path: &Path,
    source_encoding: Option<String>,
    target_encoding: Encoding,
    expected_size: Option<u64>,
    expected_modified: Option<u64>,
) -> ConversionResult {
    info!(?path, ?source_encoding, ?target_encoding, "开始转换文件编码");
    // 1. 以同一个 File 句柄打开文件，保证 metadata 和 read 指向同一实体
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            error!(?path, error = %e, "打开文件失败");
            return ConversionResult {
                node_id,
                result: Err(format!("读取文件失败: {}", e)),
            };
        }
    };

    // 2. 先取 metadata（和打开的是同一个 inode / 文件对象）
    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(e) => {
            return ConversionResult {
                node_id,
                result: Err(format!("获取文件元数据失败: {}", e)),
            };
        }
    };

    // 3. 校验文件大小是否一致
    if let Some(expected) = expected_size {
        let actual = metadata.len();
        if actual != expected {
            warn!(?path, expected, actual, "文件大小与扫描时不一致，跳过转换");
            return ConversionResult {
                node_id,
                result: Err(format!(
                    "文件在扫描后被修改（大小不一致: 扫描时 {} 字节，当前 {} 字节），已跳过转换",
                    expected, actual
                )),
            };
        }
    }

    // 4. 校验修改时间是否一致
    if let Some(expected) = expected_modified {
        let actual_modified = match metadata.modified() {
            Ok(t) => t,
            Err(e) => {
                error!(?path, error = %e, "获取文件修改时间失败");
                return ConversionResult {
                    node_id,
                    result: Err(format!("获取文件修改时间失败: {}", e)),
                };
            }
        };
        let actual_secs = actual_modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if actual_secs != expected {
            warn!(?path, expected, actual_secs, "文件修改时间与扫描时不一致，跳过转换");
            return ConversionResult {
                node_id,
                result: Err(format!(
                    "文件在扫描后被外部程序修改（修改时间已变化），已跳过转换"
                )),
            };
        }
    }

    // 5. 记录原始修改时间（用于转换后恢复）
    let modified = match metadata.modified() {
        Ok(t) => t,
        Err(_) => std::time::SystemTime::now(),
    };

    // 6. 读取整个文件内容
    let mut bytes = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytes) {
        error!(?path, error = %e, "读取文件内容失败");
        return ConversionResult {
            node_id,
            result: Err(format!("读取文件内容失败: {}", e)),
        };
    }
    trace!(?path, bytes = bytes.len(), "文件读取完成");

    // 7. 解码
    let source_enc = source_encoding
        .as_deref()
        .and_then(Encoding::from_name)
        .map(|e| e.to_encoding_rs())
        .unwrap_or(encoding_rs::UTF_8);
    trace!(?path, source_enc = source_enc.name(), "开始解码");
    let (cow, had_errors) = source_enc.decode_without_bom_handling(&bytes);
    if had_errors {
        warn!(?path, source_enc = source_enc.name(), "解码过程中遇到非法字节，已替换为 U+FFFD");
    }
    let text = cow.into_owned();

    // 8. 重新编码
    let target_enc = target_encoding.to_encoding_rs();
    trace!(?path, target_enc = target_enc.name(), "开始重新编码");
    let (encoded, _, had_errors) = target_enc.encode(&text);
    if had_errors {
        warn!(?path, target_enc = target_enc.name(), "编码过程中遇到不可映射字符，已替换");
    }

    // 9. 构造输出字节
    let mut output = Vec::new();
    if let Some(bom) = target_encoding.bom_bytes() {
        output.extend_from_slice(bom);
        trace!(?path, bom_len = bom.len(), "添加目标编码 BOM");
    }
    output.extend_from_slice(&encoded);

    // 10. 原子写入：使用随机后缀的临时文件，避免与已有 .tmp 冲突
    let tmp_path = path.with_extension(make_temp_extension());
    trace!(?path, ?tmp_path, "准备原子写入临时文件");
    if let Err(e) = fs::File::create(&tmp_path).and_then(|mut f| f.write_all(&output)) {
        error!(?path, ?tmp_path, error = %e, "写入临时文件失败");
        return ConversionResult {
            node_id,
            result: Err(format!("写入临时文件失败: {}", e)),
        };
    }

    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        error!(?path, ?tmp_path, error = %e, "rename 覆盖原文件失败");
        return ConversionResult {
            node_id,
            result: Err(format!("替换原文件失败: {}", e)),
        };
    }
    trace!(?path, "临时文件已成功覆盖原文件");

    // 11. 恢复原始修改时间
    let ft = filetime::FileTime::from_system_time(modified);
    if let Err(e) = filetime::set_file_mtime(path, ft) {
        warn!(?path, error = %e, "恢复文件修改时间失败");
    } else {
        trace!(?path, "已恢复原始修改时间");
    }

    info!(?path, "文件编码转换成功");
    ConversionResult {
        node_id,
        result: Ok(()),
    }
}
