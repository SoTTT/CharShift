# CharShift

批量检测并转换文本文件编码的桌面工具。

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)
![Vibe Coding](https://img.shields.io/badge/built%20with-vibe%20coding-purple.svg)

> 这是一个 **Vibe Coding** 项目 —— 由人类描述需求，AI 辅助实现，在对话中自然生长出来的工具。
> 
> 💜 爱来自 Kimi Code

---

## 功能

- **批量编码检测** — 自动识别文件编码（BOM → chardet 频率分析 → UTF-8 启发式）
- **批量编码转换** — 将选中的文件一键转换为目标编码
- **树形文件列表** — 扫描目录后以树形结构展示，支持展开/折叠
- **拖放添加** — 直接将文件或目录拖入窗口即可添加
- **批量选中** — 按住 Shift 点击文件，可像文件管理器一样批量选中范围内的文件
- **文件锁定（Windows）** — 开启后可阻止其他程序修改已添加的文件
- **安全转换** — 转换前自动校验文件是否被外部修改，防止覆盖他人编辑内容
- **原子写入** — 先写入临时文件再重命名覆盖，确保转换结果完整一致

## 支持的编码

| 编码 | 说明 |
|------|------|
| UTF-8 | 无 BOM |
| UTF-8-BOM | 带 BOM |
| UTF-16LE | 小端序 |
| UTF-16BE | 大端序 |
| GBK | 简体中文 |
| GB18030 | 国标扩展 |
| BIG5 | 繁体中文 |
| ISO-8859-1 | 西欧语言 |
| WINDOWS-1252 | Windows 西欧 |

## 安装

从 [Releases](../../releases) 下载对应平台的安装包：

| 版本 | 适用场景 |
|------|---------|
| `charshift-x64-setup.exe` | Win10/11 用户（系统自带 WebView2） |
| `charshift-x64-webview2-setup.exe` | Win7/8/Server（安装时自动下载 WebView2） |
| `charshift-x64-webview2-full-setup.exe` | 内网/无网络环境（自带完整 WebView2 离线包） |
| `.deb` / `.AppImage` | Linux 用户 |
| `.dmg` | macOS 用户 |

## 使用

### 1. 添加文件

- 点击「打开目录」扫描整个文件夹
- 或将文件/目录直接拖入窗口

### 2. 选择要转换的文件

- 单击文本文件可单独选中/取消
- 单击目录的复选框可全选/全不选其子文件
- 按住 **Shift** 点击文件，可选中从上次点击位置到当前位置之间的所有文件
- 被识别为二进制或未知编码的文件不可选

### 3. 选择目标编码并转换

- 在工具栏选择目标编码（默认 UTF-8）
- 点击「开始转换」
- 转换完成后会显示成功/失败统计，失败的文件会标注原因

### 4. 设置

点击右上角齿轮图标打开设置面板：

- **扫描时排除非文本文件** — 开启后自动跳过二进制文件
- **锁定列表中的文件** — 开启后阻止其他程序修改已添加的文件（Windows 有效）

## 构建

### 环境要求

- [Rust](https://rustup.rs/)
- [Tauri CLI](https://tauri.app/start/prerequisites/)（`cargo install tauri-cli`）

### 开发调试

```bash
cargo tauri dev
```

### 生产构建

```bash
# Windows（标准版）
cargo tauri build

# Windows（嵌入 WebView2 Bootstrapper）
cargo tauri build --config ./build-scripts/config/webview2-embed.json

# Windows（嵌入完整 WebView2 离线包）
cargo tauri build --config ./build-scripts/config/webview2-offline.json

# Linux
cargo tauri build --config ./build-scripts/config/linux.json

# macOS
cargo tauri build --config ./build-scripts/config/macos.json
```

### 一键打包（Windows）

```powershell
.\build-scripts\build-windows.ps1
```

输出位置：`dist/windows/`

## 技术栈

- **后端**：Rust + Tauri v2
- **前端**：原生 HTML / CSS / JavaScript（无框架）
- **编码检测**：chardet + BOM 检测 + UTF-8 启发式
- **编码转换**：encoding_rs

## 平台支持

| 平台 | 支持状态 | 备注 |
|------|---------|------|
| Windows 10/11 | ✅ 完整支持 | 推荐 |
| Windows 7 SP1 | ⚠️ 有限支持 | 需安装 WebView2 Runtime，文件锁定可用 |
| Linux | ✅ 支持 | 需 WebKitGTK |
| macOS | ✅ 支持 | — |

## 许可证

MIT
