#requires -Version 5.1
<#
  Code Convertor - Windows 打包脚本
  生成 3 种变体：
    1. 不自带 WebView2（假设系统已安装）
    2. 嵌入 WebView2 Bootstrapper（安装时自动下载，+~2MB）
    3. 嵌入完整 WebView2 离线包（无需联网，+~130MB）
#>
param(
    [switch]$SkipClean
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $ProjectRoot

# 读取版本号
$TomlContent = Get-Content "$ProjectRoot/Cargo.toml" -Raw
if ($TomlContent -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "0.1.0"
}

$OutputDir = "$ProjectRoot/dist/windows"
$BundleDir = "$ProjectRoot/target/release/bundle"

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Code Convertor Windows Build Script" -ForegroundColor Cyan
Write-Host "  Version: $Version" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

if (-not $SkipClean) {
    Write-Host "`n[INFO] Cleaning previous builds..." -ForegroundColor DarkGray
    cargo clean 2>&1 | Out-Null
    if (Test-Path $BundleDir) {
        Remove-Item -Recurse -Force $BundleDir
    }
}

function Build-Variant {
    param(
        [string]$Name,
        [string]$ConfigPath,
        [string]$Suffix
    )

    Write-Host "`n========================================" -ForegroundColor Yellow
    Write-Host "  Building: $Name" -ForegroundColor Yellow
    Write-Host "========================================" -ForegroundColor Yellow

    $BuildArgs = @("tauri", "build")
    if ($ConfigPath) {
        $BuildArgs += "--config"
        $BuildArgs += $ConfigPath
    }

    & cargo @BuildArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Build failed for variant: $Name"
    }

    # 复制产物
    $SrcExe = "$ProjectRoot/target/release/code-convertor.exe"
    $SrcNsis = "$BundleDir/nsis/*.exe"

    if (Test-Path $SrcExe) {
        $DestExe = "$OutputDir/code-convertor-${Version}-x64${Suffix}.exe"
        Copy-Item $SrcExe $DestExe -Force
        $Size = (Get-Item $DestExe).Length / 1MB
        Write-Host "  [OK] Portable: $DestExe ($([math]::Round($Size,1)) MB)" -ForegroundColor Green
    }

    $NsisFiles = Get-ChildItem $SrcNsis -ErrorAction SilentlyContinue
    if ($NsisFiles) {
        $DestNsis = "$OutputDir/code-convertor-${Version}-x64${Suffix}-setup.exe"
        Copy-Item $NsisFiles[0].FullName $DestNsis -Force
        $Size = (Get-Item $DestNsis).Length / 1MB
        Write-Host "  [OK] Installer: $DestNsis ($([math]::Round($Size,1)) MB)" -ForegroundColor Green
    }

    # 清理 bundle 目录以便下次构建
    if (Test-Path $BundleDir) {
        Remove-Item -Recurse -Force $BundleDir
    }
}

# 变体 1: 不自带 WebView2
Build-Variant -Name "No WebView2 bundled" -ConfigPath $null -Suffix ""

# 变体 2: 嵌入 WebView2 Bootstrapper
Build-Variant -Name "WebView2 Bootstrapper" -ConfigPath "$ProjectRoot/build-scripts/config/webview2-embed.json" -Suffix "-webview2"

# 变体 3: 嵌入完整 WebView2 离线包
Build-Variant -Name "WebView2 Offline Installer" -ConfigPath "$ProjectRoot/build-scripts/config/webview2-offline.json" -Suffix "-webview2-full"

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "  All builds completed!" -ForegroundColor Cyan
Write-Host "  Output: $OutputDir" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

Get-ChildItem $OutputDir | ForEach-Object {
    $Size = $_.Length / 1MB
    Write-Host "  $([math]::Round($Size,1)) MB`t$($_.Name)"
}
