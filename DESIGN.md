# 编码转换桌面程序 - 设计文档

**名称**: CharShift  
**技术栈**: Rust + Iced (Native GUI)  
**目标平台**: Windows, Linux  
**构建目标**: 单文件可执行程序  
**核心功能**: 目录扫描、编码检测、批量编码转换

---

## 1. 架构设计

采用 **Elm Architecture (Model-View-Update)** 模式：

```
+--------------+     +--------------+     +--------------+
|     View     |---->|   Message    |---->|   Update     |
|  (UI渲染)    |     |   (用户事件)  |     |  (状态更新)   |
+--------------+     +--------------+     +--------------+
                                              |
                                              v
                                        +--------------+
                                        |    Model     |
                                        |   (应用状态)  |
                                        +--------------+
```

**并发模型**:
- UI 线程：运行 Iced 事件循环，保持 60fps 响应
- 后台线程池：通过 `tokio` 执行文件扫描、编码检测、转换任务
- 消息通道：后台任务通过 Iced 的 `Command` 机制异步向 UI 线程回传结果

---

## 2. 核心数据结构

### 2.1 文件节点

```rust
enum NodeType {
    Directory,
    TextFile { encoding: Encoding },      // 文本文件，可转换
    BinaryFile,                          // 二进制文件，仅展示（灰色）
}

struct FileNode {
    id: NodeId,                          // 唯一标识符（自增 u64）
    name: String,                        // 文件名
    path: PathBuf,                       // 完整路径
    node_type: NodeType,
    is_expanded: bool,                   // 目录展开状态
    is_selected: bool,                   // 是否被用户选中
    is_converting: bool,                 // 是否正在转换中
    parent_id: Option<NodeId>,           // 父节点 ID
    children: Vec<NodeId>,              // 子节点 ID 列表
}
```

### 2.2 应用状态

```rust
struct AppState {
    // 数据层
    nodes: HashMap<NodeId, FileNode>,    // 所有节点的存储（扁平化，便于查找）
    root_nodes: Vec<NodeId>,             // 顶层根节点列表
    next_id: NodeId,                     // 自增 ID 生成器

    // 选择层
    selected_count: usize,               // 已选中文本文件数量（缓存，避免遍历）

    // 配置层
    target_encoding: Encoding,           // 用户选择的目标编码
    available_encodings: Vec<Encoding>,  // 下拉框选项列表

    // 运行时状态
    app_status: AppStatus,               // 当前应用状态
    last_error: Option<String>,          // 最近一次错误信息（用于状态栏显示）
    conversion_progress: ConversionProgress,
}

enum AppStatus {
    Idle,                               // 空闲
    Scanning { directory: PathBuf },    // 正在扫描目录
    Detecting { completed: usize, total: usize }, // 编码检测进度
    Converting { completed: usize, total: usize }, // 转换进度
}

struct ConversionProgress {
    total_files: usize,
    completed_files: usize,
    failed_files: usize,
}
```

### 2.3 消息枚举

```rust
enum Message {
    // 用户操作
    OpenDirectory,
    DirectorySelected(PathBuf),
    ToggleExpand(NodeId),
    ToggleSelect(NodeId),
    SelectAll,
    DeselectAll,
    SetTargetEncoding(Encoding),
    StartConversion,

    // 拖放
    FilesDropped(Vec<PathBuf>),

    // 后台任务回调
    ScanCompleted(Vec<FileNode>),
    EncodingDetected { node_id: NodeId, encoding: Option<Encoding> },
    FileConverted { node_id: NodeId, result: Result<(), String> },
    ConversionFinished,

    // 对话框
    DismissError,
    DismissCompletionDialog { converted: usize, failed: usize },

    // 其他
    Tick,                               // 定时器（用于进度动画）
}
```

---

## 3. 模块设计

### 3.1 `app.rs` - 应用主模块

**职责**: Iced Application trait 的实现，消息路由总线。

**关键逻辑**:
- `update()`: 根据 Message 类型分发到对应处理函数
- `view()`: 根据 AppState 渲染完整界面
- `subscription()`: 监听拖放事件、定时器

**状态机规则**:
```
Idle --OpenDirectory--> Scanning --ScanCompleted--> Detecting --全部检测完成--> Idle
                                                                       |
Idle <-------------------------- 用户可交互 ------------------------------+

Idle --StartConversion--> Converting --ConversionFinished--> 显示完成弹窗 --> Idle
```

### 3.2 `file/scanner.rs` - 文件扫描器

**职责**: 递归遍历目录，构建文件树，区分文本/二进制。

**算法**:
1. 使用 `walkdir` 递归遍历，限制最大深度（如 20 层，防循环挂载）
2. 对每个文件读取前 **8KB**（可配置）
3. 使用 `content_inspector` 的 `inspect` 函数判断是否为文本
4. 跳过以下路径：
   - 隐藏文件/目录（以 `.` 开头，可配置）
   - 符号链接（防止循环）
   - 系统目录（如 Windows 的 `System Volume Information`）

**输出**: 扁平化的 `Vec<FileNode>`，由 `app.rs` 负责组装为树形结构。

### 3.3 `file/detector.rs` - 编码检测器

**职责**: 检测单个文件的字符编码。

**检测策略**（按优先级）:
1. **BOM 检测**: 检查文件头是否有 UTF-8/UTF-16LE/UTF-16BE/UTF-32 的 BOM
2. **chardet 分析**: 对无 BOM 的文件，使用 `chardet` 库基于字符频率分析
3. **启发式回退**: 如果 chardet 置信度 < 0.6，尝试按 UTF-8 解码，成功则标记 UTF-8，否则标记为 "Unknown"

**异步执行**: 每个文件在独立的 tokio 任务中检测，通过 `Command::batch` 批量触发。

### 3.4 `file/converter.rs` - 编码转换器

**职责**: 将文件从检测到的编码转换为目标编码。

**转换流程**:
1. 以二进制模式读取整个文件（保留原文件权限和修改时间）
2. 使用 `encoding_rs` 按源编码解码为 `String`
3. 使用 `encoding_rs` 按目标编码重新编码为字节流
4. 根据目标编码决定是否写入 BOM（仅 UTF-8-BOM / UTF-16LE / UTF-16BE / UTF-32）
5. 原子写入：先写入临时文件（同目录，`.tmp` 后缀），成功后 `std::fs::rename` 覆盖原文件
6. 恢复原始文件的修改时间（`filetime` crate）

**并发控制**: 使用 `tokio::sync::Semaphore` 限制最大并发转换数（如 4 个），避免磁盘 I/O 过载。

### 3.5 `ui/tree.rs` - 文件树组件

**职责**: 自定义递归渲染文件树。

**渲染规则**:
- 目录: `&#128193;` 图标 + 目录名，点击展开/折叠
- 文本文件: `&#128196;` 图标 + 文件名 + `[编码名]`，正常颜色，可选中
- 二进制文件: `&#11036;` 图标 + 文件名 + `(二进制)`，灰色，不可交互
- 转换中文件: 文件名右侧显示旋转动画或进度条

**交互**:
- 单选：鼠标点击切换选中状态
- 多选：按住 Shift/Ctrl 实现范围选择（可选，Phase 1 可先做单选）

### 3.6 `ui/toolbar.rs` - 工具栏

**组件**:
- `[打开目录]` 按钮
- `[清空]` 按钮（清空所有文件树）
- `目标编码` 下拉框
- `[开始转换]` 按钮（仅当选中 >0 个文件且处于 Idle 时可用）
- 状态标签：显示当前操作和统计信息

### 3.7 `ui/dialog.rs` - 对话框组件

**完成弹窗**:
```
+-----------------------------+
|  转换完成                    |
+-----------------------------+
|  成功转换: 15 个文件          |
|  失败: 2 个文件              |
|                             |
|  [查看详情]  [确定]           |
+-----------------------------+
```

**错误弹窗**: 显示最近一次致命错误。

---

## 4. 防御性检查与错误处理

### 4.1 文件系统安全

| 风险点 | 检查措施 |
|--------|----------|
| **路径遍历** | 使用 `std::path::Path` 的规范化方法，拒绝包含 `..` 的输入路径；拖放路径需解析为绝对路径后检查是否在允许范围内 |
| **符号链接循环** | `walkdir` 设置 `follow_links(false)`；手动限制最大遍历深度 20 |
| **特殊文件** | 跳过 FIFO、socket、device 文件；仅处理常规文件 (is_file) |
| **文件过大** | 编码检测仅读取前 8KB；转换时如文件 > 100MB 发出警告但仍执行 |
| **权限不足** | 扫描时跳过无权限目录；转换前检查文件是否可写，不可写则标记失败 |

### 4.2 编码处理安全

| 风险点 | 检查措施 |
|--------|----------|
| **检测失败** | 置信度 < 0.6 时标记 "Unknown"，用户仍可手动选择目标编码进行"盲转"（按字节复制）|
| **解码失败** | 使用 `encoding_rs` 的 `Decoder` 的 `decode_without_bom_handling`，遇到非法序列时替换为 `U+FFFD`（&#65533;），避免 panic |
| **BOM 冲突** | 若源文件有 BOM 但目标编码不匹配（如 GBK 不能带 BOM），转换时去除 BOM |
| **空文件** | 空文件直接按目标编码写入（如目标为 UTF-8-BOM 则只写 BOM）|

### 4.3 并发安全

| 风险点 | 检查措施 |
|--------|----------|
| **重复转换** | 转换前检查 `is_converting` 标志，避免用户重复点击 |
| **状态竞争** | 所有状态变更集中在 `update()` 单线程执行；后台任务仅通过 Message 回调修改状态 |
| **文件锁定** | Windows 下文件被其他进程占用时转换会失败，捕获 `io::ErrorKind::PermissionDenied` 并提示用户关闭占用程序 |
| **磁盘空间** | 转换前估算所需空间（原文件大小 × 2，因为临时文件），不足时提前报错 |

### 4.4 数据一致性

| 风险点 | 检查措施 |
|--------|----------|
| **树结构破坏** | 所有节点操作通过 `NodeId` 索引，删除节点时级联删除所有子节点 |
| **选中状态漂移** | 清空文件树时同步重置 `selected_count` |
| **ID 溢出** | 使用 `u64`，实际场景中不可能溢出；如溢出则 panic（属于程序 bug）|

---

## 5. UI 状态管理事项

### 5.1 状态流转矩阵

| 当前状态 | 允许的操作 | 禁止的操作 |
|----------|-----------|-----------|
| **Idle** | 打开目录、拖放文件、选择/取消选择、修改目标编码、开始转换、清空 | — |
| **Scanning** | 无（显示进度） | 所有文件操作按钮禁用 |
| **Detecting** | 无（显示进度） | 所有文件操作按钮禁用 |
| **Converting** | 无（显示进度，文件项显示转换动画） | 所有文件操作按钮禁用，树形控件只读 |

### 5.2 界面更新规则

- **实时编码显示**: 检测完成一个文件，立即更新对应节点编码标签（不等待全部完成）
- **转换进度反馈**: 每完成一个文件，更新该节点的编码标签为新编码，状态栏显示 `3/15 完成`
- **错误实时显示**: 转换失败的文件在树中高亮为红色，鼠标悬停显示错误原因
- **批量选择优化**: `Ctrl+A` 仅选中所有**可见的文本文件**（不包括灰色二进制文件和折叠目录下的文件）

### 5.3 拖放状态

| 拖入内容 | 系统光标 | 处理结果 |
|----------|----------|----------|
| 纯文本文件 | 允许 (copy) | 添加到根节点列表 |
| 目录 | 拒绝 (no-drop) | 不处理（强制用户用"打开目录"按钮）|
| 二进制文件 | 拒绝 (no-drop) | 不处理，状态栏提示"仅接受文本文件"|
| 混合内容 | 拒绝 (no-drop) | 不处理（统一拒绝，避免用户困惑）|

---

## 6. 编码检测与转换策略

### 6.1 支持编码列表

| 编码 | 说明 | BOM 支持 |
|------|------|----------|
| UTF-8 | 无 BOM | 否 |
| UTF-8-BOM | 带 BOM | 是（写入 EF BB BF）|
| UTF-16LE | 小端序 | 是（写入 FF FE）|
| UTF-16BE | 大端序 | 是（写入 FE FF）|
| GBK | 中文国标扩展 | 否 |
| GB18030 | 中文国标完整 | 否 |
| BIG5 | 繁体中文 | 否 |
| ISO-8859-1 | 西欧 | 否 |
| WINDOWS-1252 | Windows 西欧 | 否 |

### 6.2 检测优先级

```
文件头 BOM? --是--> 按 BOM 确定编码
    |
    否
    v
读取 8KB 采样 --> chardet 分析
    |
    v
置信度 >= 0.6? --是--> 采用检测结果
    |
    否
    v
尝试 UTF-8 解码 --成功--> 标记 UTF-8
    |
    失败
    v
标记 Unknown
```

---

## 7. 构建与部署

### 7.1 依赖清单

```toml
[dependencies]
iced = { version = "0.12", features = ["tokio", "debug"] }
tokio = { version = "1", features = ["rt-multi-thread", "sync"] }
walkdir = "2"
content_inspector = "0.5"
chardet = "0.2"
encoding_rs = "0.8"
encoding_rs_io = "0.1"
rfd = "0.14"
filetime = "0.2"
```

### 7.2 离线编译步骤

1. 有网环境执行: `cargo vendor > .cargo/config.toml`
2. 打包 `vendor/` 目录与源码一并分发
3. 离线环境执行: `cargo build --offline --release`

### 7.3 单文件构建

- **Windows**: `cargo build --release`（自动静态链接 MSVC runtime，如需完全无依赖使用 `x86_64-pc-windows-gnu` target）
- **Linux**: `RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --target x86_64-unknown-linux-gnu`

---

## 8. 已知风险与缓解措施

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| chardet 对短文件检测不准 | 编码识别错误，转换后乱码 | 短文件（< 100 字节）优先尝试 UTF-8，并在 UI 中标记低置信度 |
| 大文件转换导致 UI 卡顿 | 用户体验差 | 大文件使用流式转换（`encoding_rs_io`），每处理 1MB 让出一次线程 |
| Windows 路径长度限制 | 长路径文件无法访问 | 启用 Windows 长路径支持（manifest + `\\?\` 前缀）|
| Linux 无 GUI 环境 | 程序崩溃 | 启动前检查 `$DISPLAY`，无显示时优雅退出并提示 |
| 用户转换系统文件 | 系统损坏 | 拒绝转换只读系统目录（如 `C:\Windows`），弹出警告 |

---

## 9. 开发阶段划分

| 阶段 | 内容 | 预计工作量 |
|------|------|-----------|
| **Phase 1** | 基础 UI 框架 + 目录树渲染 + 打开目录 | 2 天 |
| **Phase 2** | 拖放支持 + 文本/二进制检测 + 编码检测显示 | 2 天 |
| **Phase 3** | 批量选择 + 编码转换 + 进度反馈 | 2 天 |
| **Phase 4** | 完成弹窗 + 错误处理 + 主题美化 | 1 天 |
| **Phase 5** | 跨平台测试 + 离线构建验证 + 性能优化 | 2 天 |
