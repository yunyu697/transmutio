#!/usr/bin/env bash
# FormatForge macOS 签名 + 公证脚本（第一版占位 + 可执行流程）
#
# 必需环境变量（有证书时）：
#   APPLE_SIGN_IDENTITY   "Developer ID Application: Your Name (TEAMID)"
#   APPLE_ID              公证账号
#   APPLE_TEAM_ID         团队 ID
#   APPLE_APP_PASSWORD    专用密码（App Store Connect）
# 可选：
#   NOTARY_PROFILE        已用 xcrun notarytool store-credentials 保存的 keychain profile 名
#
# 正确顺序（很重要）：
#   1. 放置 arm64 原生 ffmpeg/ffprobe（Apple Silicon 不要用 Rosetta 版本）
#      可放 resources/ffmpeg/ffmpeg 与 ffprobe，或用 lipo 合成为 universal binary
#   2. 先签名 sidecar（本脚本会做）
#   3. npx tauri build（生成 .app / .dmg）
#   4. 再签名 .app（本脚本会做，--deep 已废弃，按由内到外逐层签）
#   5. 打包 dmg → notarytool 提交 → stapler 装订
#
# 说明：签名/公证本身不改变 VideoToolbox 能力，但签名链不完整或带 quarantine 的 sidecar
# 会被 Gatekeeper 拒绝执行，表现为「ffmpeg 无法启动」→ 所有编码（含硬编）都不可用。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${APP_BUNDLE:-$ROOT/src-tauri/target/release/bundle/macos/FormatForge.app}"
SIDECAR_DIR="${SIDECAR_DIR:-$ROOT/src-tauri/resources/ffmpeg}"
FFMPEG_ENTITLEMENTS="$ROOT/src-tauri/entitlements/ffmpeg.entitlements.plist"
APP_ENTITLEMENTS="$ROOT/src-tauri/entitlements/app.entitlements.plist"

if [[ -z "${APPLE_SIGN_IDENTITY:-}" ]]; then
  echo "未提供 APPLE_SIGN_IDENTITY，跳过真实签名（占位流程）。"
  echo ""
  echo "签名+公证占位说明："
  echo "  1) 申请 Developer ID Application 证书并安装到钥匙串"
  echo "  2) 导出环境变量 APPLE_SIGN_IDENTITY / APPLE_ID / APPLE_TEAM_ID / APPLE_APP_PASSWORD"
  echo "  3) 先执行本脚本签名 sidecar，再 npx tauri build，再执行本脚本签名 app + 公证"
  echo "  4) 验证：codesign --verify --deep --strict FormatForge.app && spctl -a -vv FormatForge.app"
  echo "未签名 .app 不能在其它机器上正常分发（Gatekeeper 会拦截）。"
  exit 0
fi

sign_binary() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    echo "跳过（不存在）：$file"
    return
  fi
  echo "签名 sidecar：$file"
  codesign --force --options runtime --timestamp \
    --entitlements "$FFMPEG_ENTITLEMENTS" \
    --sign "$APPLE_SIGN_IDENTITY" "$file"
}

# 1) sidecar 先签（arm64 原生或 universal）
for name in ffmpeg ffprobe; do
  sign_binary "$SIDECAR_DIR/$name"
  # Tauri externalBin 约定名也一并处理
  for arch in aarch64-apple-darwin x86_64-apple-darwin; do
    sign_binary "$SIDECAR_DIR/$name-$arch"
    sign_binary "$ROOT/src-tauri/binaries/$name-$arch"
  done
done

if [[ ! -d "$APP_BUNDLE" ]]; then
  echo "未找到 .app：$APP_BUNDLE"
  echo "请先放置 sidecar 并执行 npx tauri build，然后重新运行本脚本。"
  exit 0
fi

# 2) 主程序（Tauri 已签时这里会覆盖为一致的身份；确保 entitlements 生效）
echo "签名主程序：$APP_BUNDLE"
codesign --force --options runtime --timestamp \
  --entitlements "$APP_ENTITLEMENTS" \
  --sign "$APPLE_SIGN_IDENTITY" "$APP_BUNDLE/Contents/MacOS/FormatForge" 2>/dev/null || true
codesign --force --options runtime --timestamp \
  --entitlements "$APP_ENTITLEMENTS" \
  --sign "$APPLE_SIGN_IDENTITY" "$APP_BUNDLE"

# 3) 校验
codesign --verify --deep --strict "$APP_BUNDLE"
echo "codesign 校验通过。"

# 4) 公证（dmg 或 zip）
if [[ -n "${NOTARY_PROFILE:-}" ]]; then
  DMG="${DMG_PATH:-$ROOT/src-tauri/target/release/bundle/dmg/FormatForge_0.1.0_aarch64.dmg}"
  if [[ -f "$DMG" ]]; then
    echo "提交公证：$DMG"
    xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$DMG"
    xcrun stapler validate "$DMG"
    echo "公证与装订完成。"
  else
    echo "未找到 dmg（$DMG）；如需公证请设置 DMG_PATH 或使用 zip。"
  fi
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
  ZIP="${ZIP_PATH:-/tmp/FormatForge-notarize.zip}"
  ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP"
  xcrun notarytool submit "$ZIP" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
  xcrun stapler staple "$APP_BUNDLE"
  echo "公证与装订完成。"
else
  echo "未提供公证凭据（NOTARY_PROFILE 或 APPLE_ID/APPLE_TEAM_ID/APPLE_APP_PASSWORD），跳过公证。"
fi

echo "macOS 签名流程结束。"
