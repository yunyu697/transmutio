# 启动首帧采样：用 PrintWindow 直接抓窗口表面，逐帧统计亮度（不受窗口叠放影响）。
#
# 为什么不用录屏：窗口叠放、焦点保护与桌面干扰都会让录屏把别的窗口算进来；
# PrintWindow(hwnd, hdc, PW_RENDERFULLCONTENT) 只取目标窗口自己的绘制结果。
#
# 判据：浅色模式全程不应出现暗帧（y < 150），深色模式不应出现亮帧（y > 150）。
# 由 `npm run verify:real` 调用，也可单独执行：
#   pwsh -File scripts/startup-frame-check.ps1 -Label light -Mode light
param(
  [Parameter(Mandatory = $true)][string]$Label,
  [Parameter(Mandatory = $true)][ValidateSet("light", "dark")][string]$Mode,
  [string]$Exe = "",
  [string]$OutDir = "",
  [string]$ProcessName = "Transmutio",
  [int]$DurationMs = 3000
)

$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "src-tauri\target\release\Transmutio.exe" }
if (-not $OutDir) { $OutDir = Join-Path $root "samples\real-webview" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;using System.Runtime.InteropServices;
public class PWC {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RC7 r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RC7 { public int L,T,R,B; }
}
"@
[PWC]::SetProcessDPIAware() | Out-Null

Get-Process $ProcessName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 700

$sw = [Diagnostics.Stopwatch]::StartNew()
Start-Process -FilePath $Exe | Out-Null

$records = New-Object System.Collections.Generic.List[object]
$shotTaken = 0
$lastHwnd = [IntPtr]::Zero
$bmp = $null
$g = $null

while ($sw.Elapsed.TotalMilliseconds -lt $DurationMs) {
  $p = Get-Process $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $p) { Start-Sleep -Milliseconds 10; continue }
  $p.Refresh()
  $hwnd = $p.MainWindowHandle
  if ($hwnd -eq [IntPtr]::Zero -or -not [PWC]::IsWindowVisible($hwnd)) { Start-Sleep -Milliseconds 10; continue }
  $r = New-Object PWC+RC7
  if (-not [PWC]::GetWindowRect($hwnd, [ref]$r)) { Start-Sleep -Milliseconds 10; continue }
  $w = $r.R - $r.L; $h = $r.B - $r.T
  if ($w -lt 800 -or $h -lt 600) { Start-Sleep -Milliseconds 10; continue }
  if ($hwnd -ne $lastHwnd -or $null -eq $bmp) {
    if ($null -ne $g) { $g.Dispose(); $bmp.Dispose() }
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $lastHwnd = $hwnd
  }
  $hdc = $g.GetHdc()
  $ok = [PWC]::PrintWindow($hwnd, $hdc, 2)
  $g.ReleaseHdc($hdc)
  if (-not $ok) { Start-Sleep -Milliseconds 5; continue }
  $sum = 0.0; $count = 0
  for ($x = 0; $x -lt 24; $x++) {
    for ($y = 0; $y -lt 14; $y++) {
      $px = [int]($w * (0.08 + 0.84 * $x / 23))
      $py = [int]($h * (0.10 + 0.80 * $y / 13))
      $c = $bmp.GetPixel($px, $py)
      $sum += (0.2126 * $c.R + 0.7152 * $c.G + 0.0722 * $c.B)
      $count++
    }
  }
  $center = $bmp.GetPixel([int]($w * 0.5), [int]($h * 0.5))
  $records.Add([pscustomobject]@{
      t   = [math]::Round($sw.Elapsed.TotalMilliseconds)
      y   = [math]::Round($sum / $count, 1)
      rgb = ("{0},{1},{2}" -f $center.R, $center.G, $center.B)
    })
  if ($shotTaken -lt 2 -and $sw.Elapsed.TotalMilliseconds -ge (400 + $shotTaken * 1200)) {
    $file = Join-Path $OutDir ("pw-{0}-{1}ms.png" -f $Label, [math]::Round($sw.Elapsed.TotalMilliseconds))
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    "shot=$file"
    $shotTaken++
  }
  Start-Sleep -Milliseconds 5
}
if ($null -ne $g) { $g.Dispose() }
if ($null -ne $bmp) { $bmp.Dispose() }

"frames=$($records.Count)"
if ($records.Count -eq 0) { "VERDICT=UNKNOWN 没抓到窗口"; exit 1 }
$first = $records[0]
"first t=$($first.t)ms y=$($first.y) center=$($first.rgb)"
$min = ($records | Measure-Object y -Minimum).Minimum
$max = ($records | Measure-Object y -Maximum).Maximum
"y_range=$min..$max"
# 输出前 12 个不同的亮度段
$groups = New-Object System.Collections.Generic.List[object]
foreach ($rec in $records) {
  $last = if ($groups.Count -gt 0) { $groups[$groups.Count - 1] } else { $null }
  if ($last -and [math]::Abs($last.y - $rec.y) -lt 8) { $last.to = $rec.t; $last.n++ } else { $groups.Add([pscustomobject]@{ y = $rec.y; from = $rec.t; to = $rec.t; n = 1 }) }
}
$groups | Select-Object -First 12 | ForEach-Object { "  {0}-{1}ms y={2} x{3}" -f $_.from, $_.to, $_.y, $_.n }

if ($Mode -eq "light") {
  $bad = $records | Where-Object { $_.y -lt 150 }
  if ($bad.Count -eq 0) { "VERDICT=PASS 浅色启动全程无暗帧（$($records.Count) 帧）" }
  else { "VERDICT=FAIL 暗帧 $($bad.Count)/$($records.Count)：$(($bad | Select-Object -First 6 | ForEach-Object { "$($_.t)ms y=$($_.y)" }) -join ', ')" }
} else {
  $bad = $records | Where-Object { $_.y -gt 150 }
  if ($bad.Count -eq 0) { "VERDICT=PASS 深色启动全程无亮帧（$($records.Count) 帧）" }
  else { "VERDICT=FAIL 亮帧 $($bad.Count)/$($records.Count)：$(($bad | Select-Object -First 6 | ForEach-Object { "$($_.t)ms y=$($_.y)" }) -join ', ')" }
}

# 采样完关掉本次启动的实例，避免与下一轮启动撞单实例
Get-Process $ProcessName -ErrorAction SilentlyContinue | Stop-Process -Force
