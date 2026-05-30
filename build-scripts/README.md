# 打包脚本说明

## 目录结构

```
build-scripts/
├── build-windows.ps1          # Windows 本地打包脚本
├── config/
│   ├── webview2-embed.json    # 嵌入 WebView2 Bootstrapper
│   ├── webview2-offline.json  # 嵌入完整 WebView2 离线包
│   ├── linux.json             # Linux 打包配置
│   └── macos.json             # macOS 打包配置
└── README.md                  # 本文件
```

## Windows 变体说明

| 变体 | 配置 | 说明 | 体积增量 |
|------|------|------|----------|
| **标准版** | 无 | 不自带 WebView2，假设系统已安装 | — |
| **Bootstrapper 版** | `webview2-embed.json` | 嵌入 WebView2 引导程序，安装时自动联网下载 | ~2 MB |
| **离线完整版** | `webview2-offline.json` | 嵌入完整 WebView2 离线安装包，无需联网 | ~130 MB |

### 使用场景建议

- **标准版** → 分发给 Win10/11 用户（系统自带 WebView2）
- **Bootstrapper 版** → 分发给 Win7/8/Server 用户（有网络环境）
- **离线完整版** → 内网环境或无法联网的电脑

## 本地打包

### Windows（当前机器）

```powershell
# 以管理员身份运行 PowerShell，进入项目根目录
.\build-scripts\build-windows.ps1

# 或跳过清理（保留之前的编译缓存，更快）
.\build-scripts\build-windows.ps1 -SkipClean
```

输出位置：`dist/windows/`

### Linux（需要 WSL 或 Linux 虚拟机）

```bash
# 安装依赖
sudo apt-get update
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf

# 构建
cargo tauri build --release --config ./build-scripts/config/linux.json

# 产物在 target/release/bundle/{deb,appimage}/
```

### macOS（需要 Mac 或 GitHub Actions）

```bash
cargo tauri build --release --config ./build-scripts/config/macos.json

# 产物在 target/release/bundle/{dmg,app}/
```

## GitHub Actions 自动打包

项目已配置 `.github/workflows/release.yml`，支持以下触发方式：

### 1. 推送版本标签自动触发

```bash
git tag v0.1.0
git push origin v0.1.0
```

### 2. 手动触发

进入 GitHub 仓库 → Actions → Release → Run workflow

### 产物下载

Actions 运行完成后，在页面底部可以下载各平台的构建产物：
- `windows-builds` — Windows 三种变体
- `linux-builds` — Linux `.deb` + `.AppImage`
- `macos-builds` — macOS `.dmg` + `.app`
