use serde::{Deserialize, Serialize};

pub type NodeId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Gbk,
    Gb18030,
    Big5,
    Iso8859_1,
    Windows1252,
}

impl Encoding {
    pub fn all() -> Vec<Encoding> {
        vec![
            Encoding::Utf8,
            Encoding::Utf8Bom,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
            Encoding::Gbk,
            Encoding::Gb18030,
            Encoding::Big5,
            Encoding::Iso8859_1,
            Encoding::Windows1252,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8-BOM",
            Encoding::Utf16Le => "UTF-16LE",
            Encoding::Utf16Be => "UTF-16BE",
            Encoding::Gbk => "GBK",
            Encoding::Gb18030 => "GB18030",
            Encoding::Big5 => "BIG5",
            Encoding::Iso8859_1 => "ISO-8859-1",
            Encoding::Windows1252 => "WINDOWS-1252",
        }
    }

    pub fn bom_bytes(&self) -> Option<&'static [u8]> {
        match self {
            Encoding::Utf8Bom => Some(&[0xEF, 0xBB, 0xBF]),
            Encoding::Utf16Le => Some(&[0xFF, 0xFE]),
            Encoding::Utf16Be => Some(&[0xFE, 0xFF]),
            _ => None,
        }
    }

    pub fn to_encoding_rs(&self) -> &'static encoding_rs::Encoding {
        match self {
            Encoding::Utf8 | Encoding::Utf8Bom => encoding_rs::UTF_8,
            Encoding::Utf16Le => encoding_rs::UTF_16LE,
            Encoding::Utf16Be => encoding_rs::UTF_16BE,
            Encoding::Gbk => encoding_rs::GBK,
            Encoding::Gb18030 => encoding_rs::GB18030,
            Encoding::Big5 => encoding_rs::BIG5,
            Encoding::Iso8859_1 => encoding_rs::ISO_8859_2,
            Encoding::Windows1252 => encoding_rs::WINDOWS_1252,
        }
    }

    pub fn from_name(name: &str) -> Option<Encoding> {
        match name {
            "UTF-8" => Some(Encoding::Utf8),
            "UTF-8-BOM" => Some(Encoding::Utf8Bom),
            "UTF-16LE" => Some(Encoding::Utf16Le),
            "UTF-16BE" => Some(Encoding::Utf16Be),
            "GBK" => Some(Encoding::Gbk),
            "GB18030" => Some(Encoding::Gb18030),
            "BIG5" => Some(Encoding::Big5),
            "ISO-8859-1" => Some(Encoding::Iso8859_1),
            "WINDOWS-1252" => Some(Encoding::Windows1252),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Directory,
    TextFile,
    BinaryFile,
    UnknownEncoding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub id: NodeId,
    pub name: String,
    pub path: String,
    pub node_type: NodeType,
    pub encoding: Option<String>,
    pub is_expanded: bool,
    pub is_selected: bool,
    pub is_converting: bool,
    pub conversion_error: Option<String>,
    pub parent_id: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// 扫描时的文件大小（字节），用于转换前一致性校验
    pub file_size: Option<u64>,
    /// 扫描时的修改时间（UNIX 秒级时间戳），用于转换前一致性校验
    pub file_modified: Option<u64>,
}

impl FileNode {
    pub fn is_text_file(&self) -> bool {
        matches!(self.node_type, NodeType::TextFile)
    }
    pub fn is_binary_file(&self) -> bool {
        matches!(self.node_type, NodeType::BinaryFile)
    }
    pub fn is_directory(&self) -> bool {
        matches!(self.node_type, NodeType::Directory)
    }
}
