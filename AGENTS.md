# CharShift — 编码转换桌面程序

> 本文件供 AI 编码助手阅读。以下信息均基于项目实际文件内容，不做假设。

---

## 项目概览

**CharShift** 是一款基于 **Tauri v2** 的桌面端 GUI 程序，用于批量检测文本文件编码并将其转换为目标编码。

- **技术栈**：Rust（后端）+ 原生 HTML/CSS/JS（前端）
- **构建系统**：Cargo + Tauri（无需前端构建工具链）
- **目标平台**：Windows（主要）、Linux、macOS（由 Tauri 支持）
- **界面语言**：中文

> **注意**：`DESIGN.md` 已更新为 Tauri v2 架构说明，与当前代码一致。

---

## 项目结构

```
├── public/                 # 前端静态资源（直接作为 Tauri frontendDist）
│   ├── index.html          # 主界面 HTML 结构
│   ├── app.js              # 前端逻辑：状态管理、树形渲染、拖放、调用 Rust 命令
│   └── styles.css          # 全局样式，使用 CSS 变量定义主题色
├── src/                    # Rust 后端源码
│   ├── main.rs             # Tauri 命令定义、应用入口
│   ├── types.rs            # 前后端共享的数据结构（Encoding、FileNode、NodeType）
│   └── file/               # 文件处理核心模块
│       ├── mod.rs
│       ├── scanner.rs      # 目录扫描、文本/二进制区分
│       ├── detector.rs     # 编码检测（BOM → chardet → UTF-8 启发式）
│       ├── converter.rs    # 编码转换、原子写入、保留修改时间
│       └── locker.rs       # 文件独占锁定（Windows share_mode(0)）
├── icons/                  # 应用图标（.ico 等）
├── gen/schemas/            # Tauri 生成的 schema 文件
├── Cargo.toml              # Rust 依赖与包配置
├── tauri.conf.json         # Tauri 应用配置（窗口、权限、构建）
└── build.rs                # Tauri 编译脚本
```

---

## 技术栈与关键依赖

### Rust 后端（`Cargo.toml`）

| 依赖 | 用途 |
|------|------|
| `tauri = "2"` | 桌面应用框架，提供 IPC、窗口管理 |
| `tokio = "1"` | 异步运行时（`rt-multi-thread`、`sync`） |
| `serde` / `serde_json` | 前后端数据序列化 |
| `walkdir = "2"` | 递归目录遍历 |
| `content_inspector = "0.2.4"` | 基于内容判断文本/二进制 |
| `chardet = "0.2"` | 无 BOM 时的编码频率分析 |
| `encoding_rs` / `encoding_rs_io` | 编码解码与转换 |
| `filetime = "0.2"` | 恢复文件修改时间 |
| `rfd = "0.14"` | 原生文件对话框（选目录） |
| `tracing = "0.1"` | 结构化日志记录 |
| `tracing-subscriber = "0.3"` | 日志输出与过滤（支持 `RUST_LOG` 环境变量） |

### 前端

- **纯原生技术**：无 React/Vue，无 npm，无 bundler
- 直接通过 `window.__TAURI__.core.invoke()` 调用 Rust 命令
- 通过 `window.__TAURI__.event.listen()` 监听 Tauri 拖放事件
- 设置项使用 `localStorage` 持久化

---

## 构建与运行

### 开发调试

```bash
cargo tauri dev
```

或直接：

```bash
cargo build
cargo run
```

> 前端文件位于 `public/`，Tauri 会直接将其作为静态资源加载，无需额外编译步骤。

### 生产构建

```bash
cargo build --release
```

Tauri 会在 `target/release/` 下生成可执行文件，并自动处理资源打包与图标嵌入。

---

## 核心架构

### 前后端通信（Tauri IPC）

Rust 端暴露以下 `#[tauri::command]` 命令：

| 命令 | 说明 |
|------|------|
| `pick_directory` | 打开原生目录选择对话框 |
| `scan_directory` | 递归扫描目录，返回 `Vec<FileNode>` |
| `detect_encodings` | 批量检测编码（同步返回） |
| `detect_encodings_stream` | 批量检测编码，通过 `Channel` 流式回传进度 |
| `convert_files` | 批量转换编码，最大并发数限制为 4 |
| `check_text_files` | 检查给定路径列表是否为文本文件（用于拖放） |
| `get_available_encodings` | 获取支持的目标编码列表 |
| `lock_files` | 批量独占锁定文件（Windows 强制锁，POSIX 占位） |
| `unlock_files` | 批量解锁指定路径的文件 |
| `unlock_all_files` | 解锁所有已锁定的文件 |

### 编码检测策略（`src/file/detector.rs`）

1. **BOM 检测**：检查文件头是否有 UTF-8/UTF-16LE/UTF-16BE/UTF-32 BOM
2. **chardet 分析**：读取前 8KB，基于字符频率分析
3. **启发式回退**：若 chardet 置信度 `< 0.6`，尝试按 UTF-8 解码，成功则标记 UTF-8，否则标记为 `UnknownEncoding`

### 编码转换流程（`src/file/converter.rs`）

1. 以二进制模式读取整个文件
2. 使用 `encoding_rs` 按源编码解码（跳过 BOM 处理，非法字节替换为 `U+FFFD`）
3. 使用 `encoding_rs` 按目标编码重新编码
4. 根据目标编码写入 BOM（仅 UTF-8-BOM、UTF-16LE、UTF-16BE）
5. **原子写入**：先写 `.tmp` 临时文件，成功后 `rename` 覆盖原文件
6. 使用 `filetime` 恢复原始修改时间

### 扫描规则（`src/file/scanner.rs`）

- 最大遍历深度：**20 层**
- 最大节点数：**50,000**
- 跳过隐藏文件/目录（以 `.` 开头）
- 跳过系统目录（如 `System Volume Information`、`$Recycle.Bin`、`Windows`）
- 不跟随符号链接
- 文本检测仅读取前 **512 字节**

---

## 支持的编码

Rust 后端实际支持的编码：

- UTF-8
- UTF-8-BOM
- UTF-16LE
- UTF-16BE
- GBK
- GB18030
- BIG5
- ISO-8859-1（底层 `encoding_rs` 无独立实现，实际以 `WINDOWS_1252` 处理，两者完全兼容）
- WINDOWS-1252

> 前端下拉框选项与后端 `Encoding` 枚举已保持同步。

---

## 代码规范与约定

### 语言与注释
- 代码注释、UI 文本、错误提示均以**中文**为主
- 变量命名采用 Rust 常规风格（snake_case）

### 前端代码风格
- 使用原生 DOM API，无虚拟 DOM 框架
- 状态直接存储在全局变量（`Map`、`Array`）中
- 事件委托处理树节点点击

### Rust 代码风格
- 使用 `tokio::sync::Semaphore` 控制并发（检测最大 16，转换最大 4）
- 错误通过 `Result<T, String>` 向前端传递
- 文件操作采用防御式编程（跳过无权限目录、原子写入等）

---

## 测试策略

**当前状态：项目中未包含任何自动化测试。**

如需添加测试，建议方向：

- **Rust 单元测试**：在 `src/file/` 各模块中添加 `#[cfg(test)]`
  - `scanner.rs`：测试隐藏文件过滤、系统目录跳过、文本/二进制判断
  - `detector.rs`：测试 BOM 识别、各编码样本检测、置信度边界
  - `converter.rs`：测试转换前后内容一致性、BOM 写入、元数据保留
- **集成测试**：使用 Tauri 的测试工具或临时目录进行端到端文件转换验证
- **前端测试**：由于前端为原生 JS，可引入轻量级测试框架（如 Vitest）或保持手工验证

---

## 安全与风险注意事项

| 风险点 | 现有措施 |
|--------|----------|
| 路径遍历 | 路径来自系统对话框或拖放，未做额外规范化检查；拖放路径建议解析为绝对路径后校验 |
| 符号链接循环 | `walkdir` 设置 `follow_links(false)`，且限制最大深度 20 |
| 特殊文件 | 跳过非常规文件（FIFO、socket、device），仅处理 `is_file()` / `is_dir()` |
| 文件过大 | 编码检测仅读前 8KB；转换时整文件读入内存，超大文件可能导致内存压力 |
| 并发安全 | 转换前检查 `is_converting` 标志；后台任务仅通过 Message/Channel 回调修改前端状态 |
| 文件锁定 | Windows 下若文件被占用，`rename` 会失败，错误会捕获并提示用户 |
| CSP | `tauri.conf.json` 中 `csp: null`，如需上线生产环境建议根据实际前端资源收紧策略 |
| 权限 | 扫描时跳过无权限目录；转换前若文件不可写则会在写入阶段失败并报告 |

---

## 已知问题与待改进项

1. **配置持久化不完整**：`localStorage` 仅保存了 `excludeBinary`（排除二进制）和 `lockFiles`（锁定文件）两项开关，默认目标编码、窗口尺寸等其余状态未持久化。
2. **无测试覆盖**：核心文件处理逻辑（扫描、检测、转换、锁定）缺乏任何自动化测试。

---

## 日志系统

后端已引入 `tracing` + `tracing-subscriber`，通过环境变量 `RUST_LOG` 控制日志级别。

| 日志级别 | 适用场景 |
|----------|----------|
| `error` | 文件打开失败、写入失败、rename 失败等硬错误 |
| `warn` | 一致性校验失败、编码映射失败、锁定失败、恢复修改时间失败 |
| `info` | 命令入口/出口、扫描完成、转换完成、批量操作统计 |
| `trace` | 跳过隐藏文件/系统目录、BOM 检测、临时文件路径、解码/编码过程 |

启用方式（开发调试）：
```bash
RUST_LOG=info cargo tauri dev
RUST_LOG=trace cargo run
```

---

## 快速参考：修改 checklist

- 修改 Rust 数据结构（如 `FileNode`、`Encoding`）→ 需同步检查 `types.rs` 与 `app.js` 中的序列化/反序列化
- 新增 Tauri 命令 → 在 `main.rs` 的 `generate_handler![]` 中注册，并在 `app.js` 中通过 `invoke()` 调用
- 新增前端交互组件 → 直接修改 `index.html` + `styles.css` + `app.js`，无需构建步骤
- 调整窗口属性 → 修改 `tauri.conf.json`
- 添加新编码支持 → 同步修改 `types.rs` 的 `Encoding` 枚举 与 `public/index.html` 的下拉框
