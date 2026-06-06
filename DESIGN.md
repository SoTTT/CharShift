# CharShift 软件设计说明书

> **版本**：v1.0（Tauri v2 架构）  
> **日期**：2026-05-30  
> **适用范围**：CharShift 桌面端编码批量检测与转换程序

---

## 1. 引言

### 1.1 编写目的

本文档描述 CharShift 桌面应用程序的总体架构、模块划分、接口定义、数据结构与核心算法，为开发、测试及后续维护提供技术依据。

### 1.2 项目背景

CharShift 是一款基于 **Tauri v2** 的跨平台桌面 GUI 程序，用于批量检测文本文件的原始编码，并将其转换为目标编码。程序面向需要处理大量历史文本文件（如 GBK、BIG5、ISO-8859-1 等）的开发者和运维人员。

### 1.3 术语与定义

| 术语 | 说明 |
|------|------|
| Tauri IPC | Tauri 提供的前后端进程间通信机制，前端通过 `invoke` 调用后端命令，后端通过 `Channel` 向前端推送流式数据 |
| BOM | Byte Order Mark，字节顺序标记，用于标识 UTF 系列编码 |
| 原子写入 | 先写入临时文件，再通过文件系统 `rename` 覆盖原文件，确保写入要么完全成功，要么不产生副作用 |
| 独占锁定 | Windows 下以 `share_mode(0)` 打开文件，阻止其他进程读写 |

### 1.4 参考资料

- Tauri v2 官方文档
- `Cargo.toml` / `tauri.conf.json` 项目配置
- 源码目录 `src/` 与 `public/`

---

## 2. 总体设计

### 2.1 系统架构

CharShift 采用 **Tauri 混合架构**：

```
┌─────────────────────────────────────────────┐
│                  前端层                       │
│  ┌─────────┐  ┌─────────┐  ┌─────────────┐  │
│  │ index.html │  │ app.js  │  │ styles.css  │  │
│  │ (DOM结构)  │  │ (逻辑)  │  │ (主题样式)  │  │
│  └─────────┘  └─────────┘  └─────────────┘  │
│         ↕ Tauri IPC（invoke / Channel）       │
├─────────────────────────────────────────────┤
│                  后端层                       │
│  ┌─────────┐  ┌─────────┐  ┌─────────────┐  │
│  │  main.rs │  │ types.rs │  │ file/ 模块  │  │
│  │(命令入口)│  │(共享类型)│  │scanner/     │  │
│  │          │  │          │  │detector/    │  │
│  │          │  │          │  │converter/   │  │
│  │          │  │          │  │locker       │  │
│  └─────────┘  └─────────┘  └─────────────┘  │
└─────────────────────────────────────────────┘
```

- **前端**：原生 HTML5 + CSS3 + ES2020，无虚拟 DOM 框架，无 npm 构建链。Tauri 直接将 `public/` 作为 `frontendDist` 加载。
- **后端**：Rust，基于 `tokio` 异步运行时，通过 `#[tauri::command]` 暴露 IPC 接口。

### 2.2 功能模块结构

```
charshift
├── 前端模块
│   ├── UI 渲染（文件树、工具栏、状态栏、对话框）
│   ├── 状态管理（全局节点 Map、选中状态、展开状态）
│   ├── 交互事件（点击、拖放、Shift 多选、设置）
│   └── IPC 调用封装
│
└── 后端模块
    ├── 命令路由层（main.rs）
    ├── 共享类型层（types.rs）
    └── 文件处理层（file/）
        ├── scanner    — 目录扫描与文本/二进制判定
        ├── detector   — 编码检测（BOM / chardet / 启发式）
        ├── converter  — 编码转换、原子写入、时间恢复
        └── locker     — 文件独占锁定（Windows）
```

### 2.3 运行环境

| 层级 | 环境要求 |
|------|----------|
| 前端 | WebView2（Windows）、WebKit（macOS/Linux），由 Tauri 自动托管 |
| 后端 | Rust 1.70+、Cargo、Tauri CLI v2 |
| 目标平台 | Windows（主要）、Linux、macOS |

### 2.4 设计约束与原则

1. **无前端构建链**：`public/` 下的静态文件直接作为前端资源，不引入 Webpack/Vite/npm。
2. **最小权限**：`tauri.conf.json` 中仅声明 `core:default` 权限；CSP 当前为 `null`（开发阶段）。
3. **防御式文件操作**：扫描时跳过无权限目录；转换前校验文件大小与修改时间；使用原子写入避免数据损坏。
4. **并发可控**：编码检测最大并发 16，文件转换最大并发 4，防止磁盘 IO 竞争和内存压力。

---

## 3. 接口设计

### 3.1 用户接口（UI）

| UI 区域 | 功能说明 |
|---------|----------|
| 标题栏 | 显示应用图标与名称，含「设置」按钮 |
| 工具栏 | 「打开目录」「清空」、目标编码下拉框、「开始转换」 |
| 文件树卡片 | 可展开/折叠的目录树；文本文件显示复选框、文件名、检测编码；二进制文件置灰 |
| 拖放遮罩 | 拖入文件时显示高亮提示区 |
| 状态栏 | 左侧状态点+文字（就绪/扫描中/检测中/转换中/失败），右侧节点统计 |
| 对话框 | 成功/错误提示弹窗 |
| 设置面板 | 「扫描时排除非文本文件」「锁定列表中的文件」两个开关 |

### 3.2 外部接口

#### 3.2.1 操作系统接口

| 接口 | 用途 | 调用方 |
|------|------|--------|
| `rfd::FileDialog::pick_folder()` | 原生目录选择对话框 | `pick_directory` 命令 |
| `std::fs` / `walkdir` | 文件系统遍历与读写 | `scanner`、`converter`、`locker` |
| `filetime` | 恢复文件修改时间 | `converter` |
| Windows `OpenOptionsExt::share_mode(0)` | 独占文件锁定 | `locker` |

#### 3.2.2 第三方库接口

| 库 | 用途 |
|----|------|
| `encoding_rs` | 编码解码与重新编码 |
| `chardet` | 基于字符频率的无 BOM 编码检测 |
| `content_inspector` | 二进制/文本内容判定 |
| `serde` / `serde_json` | 前后端数据序列化 |

### 3.3 内部接口（Tauri IPC 命令）

前端通过 `window.__TAURI__.core.invoke(cmd, payload)` 调用以下命令：

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `pick_directory` | — | `Option<String>` | 打开目录选择对话框 |
| `scan_directory` | `path: String`, `exclude_binary: bool` | `Vec<FileNode>` | 递归扫描目录，返回文件树 |
| `detect_encodings` | `tasks: Vec<DetectTask>` | `Vec<{id, encoding}>` | 同步批量检测编码 |
| `detect_encodings_stream` | `tasks: Vec<DetectTask>`, `onProgress: Channel<DetectProgress>` | `()` | 流式批量检测，实时回传进度 |
| `convert_files` | `tasks: Vec<ConvertTask>`, `targetEncoding: String` | `Vec<{id, success, error}>` | 批量转换文件编码 |
| `check_text_files` | `paths: Vec<String>` | `Vec<FileCheckResult>` | 拖放时快速筛选文本文件 |
| `get_available_encodings` | — | `Vec<String>` | 获取支持的目标编码列表 |
| `lock_files` | `paths: Vec<String>` | `Vec<LockResult>` | 批量独占锁定文件 |
| `unlock_files` | `paths: Vec<String>` | `()` | 批量解锁文件 |
| `unlock_all_files` | — | `()` | 解锁所有已锁定文件 |

---

## 4. 数据设计

### 4.1 核心数据结构

#### 4.1.1 文件树节点（前后端共享）

```rust
pub struct FileNode {
    pub id: NodeId,               // u64，唯一标识
    pub name: String,             // 文件/目录名称
    pub path: String,             // 绝对路径
    pub node_type: NodeType,      // Directory / TextFile / BinaryFile / UnknownEncoding
    pub encoding: Option<String>, // 检测到的编码名称
    pub is_expanded: bool,        // 目录是否展开
    pub is_selected: bool,        // 是否被用户选中
    pub is_converting: bool,      // 是否正在转换（前端状态）
    pub conversion_error: Option<String>, // 转换失败信息
    pub parent_id: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub file_size: Option<u64>,   // 扫描时文件大小（一致性校验用）
    pub file_modified: Option<u64>, // 扫描时修改时间 UNIX 秒（一致性校验用）
}
```

#### 4.1.2 编码枚举

```rust
pub enum Encoding {
    Utf8, Utf8Bom, Utf16Le, Utf16Be,
    Gbk, Gb18030, Big5,
    Iso8859_1, Windows1252,
}
```

每个枚举值提供：`name()`（显示名）、`bom_bytes()`（BOM 字节）、`to_encoding_rs()`（映射到 `encoding_rs` 实例）。

#### 4.1.3 任务结构

```rust
pub struct DetectTask { id: u64, path: String }

pub struct ConvertTask {
    id: u64,
    path: String,
    source_encoding: Option<String>,
    expected_size: Option<u64>,      // 扫描时记录的大小
    expected_modified: Option<u64>,  // 扫描时记录的时间
}

pub struct DetectProgress {
    id: u64,
    encoding: Option<String>,
    completed: usize,
    total: usize,
}
```

### 4.2 数据流

#### 4.2.1 目录扫描与编码检测流

```
用户点击「打开目录」
    ↓
前端 invoke("pick_directory") → 获取路径
    ↓
前端 invoke("scan_directory", {path, exclude_binary})
    ↓
Rust: scanner::scan_directory() → 遍历目录 → 返回 Vec<FileNode>
    ↓
前端合并节点到全局 Map → 筛选 TextFile → 构造 DetectTask[]
    ↓
前端 invoke("detect_encodings_stream", {tasks, onProgress})
    ↓
Rust: 启动 Semaphore(16) 限流的并发任务
    ↓ 逐个文件
Rust: detector::detect_encoding() → 返回 DetectionResult
    ↓
Rust: 通过 Channel 推送 DetectProgress 到前端
    ↓
前端: channel.onmessage → 更新节点 encoding → 局部刷新 DOM
```

#### 4.2.2 文件转换流

```
用户选择目标编码 → 点击「开始转换」
    ↓
前端收集 is_selected 且 node_type == TextFile 的节点
    ↓
前端 invoke("convert_files", {tasks, targetEncoding})
    ↓
Rust: 启动 Semaphore(4) 限流的并发任务
    ↓ 逐个文件
Rust: converter::convert_file()
    ├─ 1. 打开文件并获取 metadata
    ├─ 2. 校验 expected_size / expected_modified（防外部修改）
    ├─ 3. 记录原始修改时间
    ├─ 4. 读取整个文件到内存
    ├─ 5. 按 source_encoding 解码（无 BOM 处理，非法字节替换为 U+FFFD）
    ├─ 6. 按 target_encoding 重新编码
    ├─ 7. 拼接目标 BOM（如需要）
    ├─ 8. 写入随机后缀临时文件
    ├─ 9. rename 临时文件覆盖原文件
    └─ 10. 恢复原始修改时间
    ↓
Rust: 收集 ConversionResult → 返回 JSON 数组
    ↓
前端: 更新每个节点的 conversion_error → 渲染结果 → 弹窗提示成功/失败数
```

---

## 5. 模块详细设计

### 5.1 前端模块（`public/`）

#### 5.1.1 状态管理（app.js）

前端采用**命令式全局状态**，无 Redux/Vuex 等状态管理库：

| 状态变量 | 类型 | 说明 |
|----------|------|------|
| `nodes` | `Map<number, FileNode>` | 所有文件节点的全局存储 |
| `rootNodes` | `number[]` | 顶层节点 ID 列表 |
| `currentPath` | `string` | 当前扫描的根目录路径 |
| `isScanning` / `isConverting` | `boolean` | 全局操作锁，防止并发操作 |
| `lastClickedFileId` | `number \| null` | Shift+批量选中的锚点 |
| `visibleTextFileIds` | `number[]` | 按渲染顺序收集的可见文本文件 ID |

关键算法：

- **Shift 批量选中**：基于 `visibleTextFileIds` 数组下标范围，统一设置选中状态，时间复杂度 O(n)。
- **目录复选框状态**：`getDirCheckboxState()` 递归统计目录下所有文本文件的选中情况，返回 `checked / indeterminate / unchecked / none`。
- **ID 重映射**：`remapScannedNodes()` 在追加新扫描结果时，通过 `offset` 平移所有 ID，避免与已有节点冲突。

#### 5.1.2 渲染策略

- 文件树采用**全量重新渲染**（`renderTree()`），因节点规模通常不超过数千，DOM 操作在可接受范围内。
- 编码检测阶段使用**局部 DOM 更新**（`updateNodeEncoding()`），仅修改对应节点的编码文本，避免频繁整树刷新造成闪烁。

#### 5.1.3 拖放处理

支持双路拖放：

1. **Tauri 原生拖放**（主要）：监听 `tauri://drag-enter/leave/over/drop` 事件。Windows 下 `drag-enter` 有时不触发，以 `drag-over` + 300ms 防抖定时器作为 fallback 显示遮罩。
2. **HTML5 拖放**（降级）：监听 `dragenter/dragleave/dragover/drop`，通过 `dragCounter` 计数器解决子元素反复触发 `dragleave` 的问题。

### 5.2 后端模块（`src/`）

#### 5.2.1 命令路由层（`main.rs`）

- 所有 IPC 命令均为 `async fn`（除 `pick_directory`、`check_text_files`、`get_available_encodings` 等纯同步命令外）。
- 使用 `tauri::generate_handler![]` 宏集中注册命令。
- 全局 `FileLocker` 通过 `.manage()` 注入 Tauri State，供 `lock_files` / `unlock_files` 共享。

#### 5.2.2 共享类型层（`types.rs`）

- `NodeId = u64` 作为节点 ID 类型别名。
- `Encoding` 枚举与前端下拉框保持名称一致（如 `"UTF-8"`、`"GBK"`）。
- `FileNode` 实现了 `Serialize` / `Deserialize`，可直接通过 IPC 前后端传输。

#### 5.2.3 扫描模块（`file/scanner.rs`）

**职责**：递归遍历目录，构建文件树，区分文本/二进制。

| 常量 | 值 | 说明 |
|------|-----|------|
| `MAX_DEPTH` | 20 | 最大递归深度 |
| `SAMPLE_SIZE` | 512 字节 | 文本检测采样大小 |
| `MAX_SCAN_NODES` | 50,000 | 最大节点数上限 |

**过滤规则**：
- 跳过隐藏项（名称以 `.` 开头）。
- 跳过系统目录：`System Volume Information`、`$Recycle.Bin`、`Config.Msi`、`Windows`。
- 不跟随符号链接（`follow_links(false)`）。
- 跳过非常规文件（FIFO、socket、device）。

**文本判定**：读取文件前 512 字节，调用 `content_inspector::inspect(buffer).is_text()`。

**复杂度**：时间复杂度 O(n)，n 为遍历到的文件/目录总数；空间复杂度 O(n)，用于存储 `Vec<FileNode>`。

#### 5.2.4 检测模块（`file/detector.rs`）

**职责**：检测单个文本文件的实际编码。

**检测策略（按优先级）**：

1. **BOM 检测**（最优先，最精确）
   - `EF BB BF` → UTF-8-BOM
   - `FF FE` → UTF-16LE
   - `FE FF` → UTF-16BE
   - `00 00 FE FF` → UTF-32BE（映射为 UTF-16BE）

2. **chardet 频率分析**
   - 读取前 8KB 样本。
   - 调用 `chardet::detect()` 获取 `(charset, confidence, _lang)`。
   - 置信度阈值 `CONFIDENCE_THRESHOLD = 0.6`。
   - chardet 返回的原始 charset 通过 `chardet_to_encoding()` 映射为内部编码名。

3. **UTF-8 启发式回退**
   - 若 chardet 置信度不足，尝试 `std::str::from_utf8(&buffer)`。
   - 解码成功则标记为 UTF-8。

4. **未知编码**
   - 以上均失败，返回 `encoding: None`。

**特殊情况**：空文件默认标记为 UTF-8。

#### 5.2.5 转换模块（`file/converter.rs`）

**职责**：安全地将文件从源编码转换为目标编码。

**核心流程**：

```
打开原文件 → 获取 metadata
    ↓
一致性校验（大小、修改时间）
    ↓
记录原始修改时间（用于最后恢复）
    ↓
读取全部字节 → 按源编码解码 → 按目标编码编码
    ↓
构造输出字节（目标 BOM + 编码后内容）
    ↓
写入临时文件（.tmp-{pid}-{nanos}）
    ↓
rename 覆盖原文件
    ↓
恢复原始修改时间（filetime）
```

**原子性保证**：
- 临时文件使用包含进程 ID 和纳秒时间戳的后缀，避免命名冲突。
- `rename` 失败时自动删除临时文件，不污染原目录。

**一致性校验**：
- `expected_size`：扫描后若文件大小变化，说明文件被外部程序修改，拒绝转换。
- `expected_modified`：扫描后若修改时间变化，拒绝转换。

#### 5.2.6 锁定模块（`file/locker.rs`）

**职责**：在转换前锁定文件，防止转换过程中被外部程序修改。

**实现差异**：
- **Windows**：使用 `std::fs::OpenOptions` 配合 `.share_mode(0)`（即 `FILE_SHARE_NONE`）以独占模式打开文件，持有 `File` 句柄即可阻止其他进程访问。
- **非 Windows（POSIX）**：无与 Windows 等价的强制文件锁，返回成功占位，但不提供实际跨进程保护。

**生命周期**：
- 扫描/拖放添加文件时，若设置中开启「锁定文件」，自动调用 `lock_files`。
- 节点被移除、点击「清空」、程序退出时，调用 `unlock_files` / `unlock_all_files` 释放句柄。
- `FileLocker` 内部以 `HashMap<PathBuf, File>` 持有句柄，重复锁定同一文件返回成功。

---

## 6. 安全性设计

### 6.1 文件系统安全

| 措施 | 说明 |
|------|------|
| 路径来源可控 | 路径仅来自系统原生对话框或用户拖放，不接受不可信网络输入 |
| 符号链接安全 | `walkdir` 设置 `follow_links(false)`，且限制最大深度 20，防止循环遍历 |
| 特殊文件过滤 | 跳过 FIFO、socket、device，仅处理 `is_file()` 与 `is_dir()` |
| 无路径穿越写入 | 转换输出严格覆盖原文件，不根据用户输入构造新路径 |
| 原子写入 | 临时文件 + rename，避免转换中途崩溃导致原文件损坏 |

### 6.2 并发安全

- 转换阶段使用 `tokio::sync::Semaphore` 限制最大并发 4，避免磁盘 IO 竞争导致系统卡顿。
- 检测阶段最大并发 16，平衡速度与资源占用。
- `FileLocker` 通过 `Arc<Mutex<_>>` 共享于所有异步命令中，保证内部 `HashMap` 操作线程安全。

### 6.3 资源限制

| 资源 | 限制 |
|------|------|
| 目录遍历深度 | ≤ 20 层 |
| 扫描节点数 | ≤ 50,000 个 |
| 检测采样大小 | 8 KB（大文件不全部读入内存） |
| 文本判定采样 | 512 字节 |
| 转换内存占用 | 整文件读入内存（超大文件可能产生内存压力，当前版本未做分块处理） |

---

## 7. 错误处理

### 7.1 后端错误传递

所有 Rust 命令统一返回 `Result<T, String>`，错误信息以中文字符串形式传递到前端，前端直接展示给用户。

### 7.2 关键错误场景

| 场景 | 后端行为 | 前端行为 |
|------|----------|----------|
| 目录无权限 | `walkdir` 报错被 `continue` 跳过，不中断扫描 | 无感知，跳过条目不显示 |
| 文件打开失败 | 检测/转换返回 `encoding: None` 或 `Err("读取文件失败")` | 节点显示错误信息或 `(binary)` |
| 文件扫描后被修改 | 转换前一致性校验失败，返回具体错误 | 节点行显示红色错误文本 |
| 转换时文件被占用 | `rename` 失败，返回错误 | 计入失败数，弹窗提示 |
| 节点数超限 | `scan_directory` 返回 `Err` | 弹窗提示用户选择子目录 |

---

## 8. 部署与构建

### 8.1 开发调试

```bash
cargo tauri dev
```

或分离调试：

```bash
cargo build
cargo run
```

### 8.2 生产构建

```bash
cargo build --release
```

Tauri 在 `target/release/` 下生成可执行文件，并自动处理资源打包与图标嵌入。Windows 安装包通过 `nsis` 目标生成。

### 8.3 前端资源

`tauri.conf.json` 中：

```json
{
  "build": {
    "frontendDist": "./public"
  }
}
```

`public/` 目录下的 `index.html`、`app.js`、`styles.css` 直接作为静态资源加载，无需编译步骤。

---

## 9. 附录

### 9.1 支持的编码映射表

| 内部枚举 | 显示名称 | BOM | encoding_rs 映射 |
|----------|----------|-----|------------------|
| `Utf8` | UTF-8 | 无 | `UTF_8` |
| `Utf8Bom` | UTF-8-BOM | `EF BB BF` | `UTF_8` |
| `Utf16Le` | UTF-16LE | `FF FE` | `UTF_16LE` |
| `Utf16Be` | UTF-16BE | `FE FF` | `UTF_16BE` |
| `Gbk` | GBK | 无 | `GBK` |
| `Gb18030` | GB18030 | 无 | `GB18030` |
| `Big5` | BIG5 | 无 | `BIG5` |
| `Iso8859_1` | ISO-8859-1 | 无 | `ISO_8859_2`（⚠️ 当前代码中实际映射为 ISO-8859-2，为已知问题） |
| `Windows1252` | WINDOWS-1252 | 无 | `WINDOWS_1252` |

### 9.2 目录结构

```
├── public/                 # 前端静态资源（Tauri frontendDist）
│   ├── index.html
│   ├── app.js
│   └── styles.css
├── src/                    # Rust 后端源码
│   ├── main.rs             # Tauri 命令定义与入口
│   ├── types.rs            # 共享数据结构
│   └── file/
│       ├── mod.rs
│       ├── scanner.rs      # 目录扫描
│       ├── detector.rs     # 编码检测
│       ├── converter.rs    # 编码转换
│       └── locker.rs       # 文件锁定
├── icons/                  # 应用图标
├── Cargo.toml
├── tauri.conf.json
└── build.rs
```
