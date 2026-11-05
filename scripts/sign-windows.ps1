# FormatForge Windows 代码签名脚本（第一版占位 + 可执行流程）
#
# 用法（有证书）：
#   $env:WINDOWS_CERT_THUMBPRINT = "ABCD..."; ./scripts/sign-windows.ps1
# 或：
#   ./scripts/sign-windows.ps1 -PfxPath C:\certs\formatforge.pfx -PfxPassword "***"
#
# 用法（无证书，只检查流程）：
#   ./scripts/sign-windows.ps1            # 打印占位说明并退出 0
#
# 正确顺序（很重要）：
#   1. 把 LGPL 版 ffmpeg.exe / ffprobe.exe 放到 src-tauri/resources/ffmpeg/
#   2. 先给 sidecar 签名（本脚本 -SidecarsOnly）
#   3. npx tauri build --bundles nsis,msi
#   4. 再给主程序与安装包签名（本脚本 -Bundle）
#
# 未签名 / 签名链不完整的 sidecar 会被 SmartScreen 拦截，也会导致子进程无法启动，
# 用户侧表现为「转换失败」而不是「编码器不可用」。

param(
  [string]$Thumbprint = $env:WINDOWS_CERT_THUMBPRINT,
  [string]$PfxPath = $env:WINDOWS_CERT_PFX,
  [string]$PfxPassword = $env:WINDOWS_CERT_PASSWORD,
  [string]$Timestamp = "http://timestamp.digicert.com",
  [switch]$SidecarsOnly,
  [switch]$Bundle,
  [switch]$IncludeBundle
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Find-SignTool {
  $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $candidates = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match 'x64' } |
    Sort-Object FullName -Descending
  if ($candidates.Count -gt 0) { return $candidates[0].FullName }
  return $null
}

$signTool = Find-SignTool
if (-not $signTool) {
  Write-Warning "未找到 signtool.exe。请安装 Windows SDK（含 Signing Tools）。"
  Write-Host "占位：签名流程已就绪，但当前机器无法执行签名。"
  exit 0
}

$hasCert = ($Thumbprint -and $Thumbprint.Trim().Length -gt 0) -or ($PfxPath -and (Test-Path $PfxPath))
if (-not $hasCert) {
  Write-Warning "未提供代码签名证书（WINDOWS_CERT_THUMBPRINT 或 -PfxPath）。"
  Write-Host ""
  Write-Host "签名占位说明："
  Write-Host "  1) 申请 EV / OV 代码签名证书（推荐 EV，减少 SmartScreen 警告）"
  Write-Host "  2) 设置环境变量 WINDOWS_CERT_THUMBPRINT（证书指纹）或 WINDOWS_CERT_PFX + WINDOWS_CERT_PASSWORD"
  Write-Host "  3) 先运行: ./scripts/sign-windows.ps1 -SidecarsOnly"
  Write-Host "  4) 构建:   npx tauri build --bundles nsis,msi"
  Write-Host "  5) 再运行: ./scripts/sign-windows.ps1 -Bundle"
  Write-Host "未签名产物仍可用于本地开发，但不得作为正式分发包。"
  exit 0
}

$args = @("sign", "/fd", "SHA256", "/tr", $Timestamp, "/td", "SHA256")
if ($Thumbprint -and $Thumbprint.Trim().Length -gt 0) {
  $args += @("/sha1", $Thumbprint.Trim())
} else {
  $args += @("/f", $PfxPath, "/p", $PfxPassword)
}

function Sign-File([string]$file) {
  if (-not (Test-Path $file)) { Write-Host "跳过（不存在）：$file"; return }
  Write-Host "签名：$file"
  & $signTool @args $file
  if ($LASTEXITCODE -ne 0) { throw "签名失败：$file" }
}

$sidecarDir = Join-Path $root "src-tauri\resources\ffmpeg"
Get-ChildItem $sidecarDir -Filter *.exe -ErrorAction SilentlyContinue | ForEach-Object { Sign-File $_.FullName }

if (-not $SidecarsOnly) {
  # tauri build 会把 cargo 产物重命名为 mainBinaryName（Transmutio / Transmutio GPL）；
  # 直接 cargo build 的调试产物仍是包名 formatforge.exe，两者都尝试。
  $exe = Join-Path $root "src-tauri\target\release\Transmutio.exe"
  if (-not (Test-Path $exe)) { $exe = Join-Path $root "src-tauri\target\release\Transmutio GPL.exe" }
  if (-not (Test-Path $exe)) { $exe = Join-Path $root "src-tauri\target\release\formatforge.exe" }
  Sign-File $exe

  if ($Bundle -or $IncludeBundle) {
    $bundleDir = Join-Path $root "src-tauri\target\release\bundle"
    if (Test-Path $bundleDir) {
      Get-ChildItem $bundleDir -Recurse -Include *.exe, *.msi | ForEach-Object { Sign-File $_.FullName }
    }
  }
}

Write-Host "Windows 签名完成。"
