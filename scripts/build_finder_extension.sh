#!/bin/bash
# 编译 Finder Sync 扩展并嵌入已打包的 Origami.app，使「用 Origami 压缩」
# 直接出现在 Finder 右键的一级菜单。
#
# 用法：先 `npm run tauri build`，再运行本脚本（可选传入 .app 路径）。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/src-tauri/target/release/bundle/macos/Origami.app}"

if [ ! -d "$APP" ]; then
  echo "找不到 $APP" >&2
  echo "请先运行：npm run tauri build" >&2
  exit 1
fi

HOSTID="$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$APP/Contents/Info.plist")"
MOD="OrigamiFinderSync"
EXTID="$HOSTID.finder"
APPEX="$APP/Contents/PlugIns/$MOD.appex"

echo "› 宿主应用: $HOSTID"
echo "› 扩展包:   $APPEX"

rm -rf "$APPEX"
mkdir -p "$APPEX/Contents/MacOS"

# 1) 编译 Swift 扩展（arm64）
echo "› 编译扩展…"
xcrun swiftc -O -module-name "$MOD" \
  -target arm64-apple-macos11 \
  -framework Cocoa -framework FinderSync \
  "$ROOT/finder-extension/FinderSync.swift" \
  -o "$APPEX/Contents/MacOS/$MOD"

# 2) 写 Info.plist
cat > "$APPEX/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Origami 压缩</string>
  <key>CFBundleExecutable</key><string>$MOD</string>
  <key>CFBundleIdentifier</key><string>$EXTID</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>$MOD</string>
  <key>CFBundlePackageType</key><string>XPC!</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSExtension</key>
  <dict>
    <key>NSExtensionPointIdentifier</key><string>com.apple.FinderSync</string>
    <key>NSExtensionPrincipalClass</key><string>OrigamiFinderSyncExt</string>
  </dict>
</dict>
</plist>
PLIST

# 3) 代码签名（优先用本机自签名身份；macOS 不加载 ad-hoc 签名的 Finder 扩展）
SIGN_ID="${ORIGAMI_SIGN_ID:-Origami Local Signing}"
if security find-identity -v -p codesigning 2>/dev/null | grep -q "$SIGN_ID"; then
  echo "› 用签名身份「$SIGN_ID」签名…"
else
  echo "⚠ 未找到签名身份「$SIGN_ID」，回退 ad-hoc（Finder 扩展将无法加载）。" >&2
  echo "  先运行 scripts/setup_local_signing.sh 创建本机签名证书。" >&2
  SIGN_ID="-"
fi
codesign --force --sign "$SIGN_ID" --timestamp=none "$APPEX"
codesign --force --deep --sign "$SIGN_ID" --timestamp=none "$APP"

echo ""
echo "✓ 完成。后续步骤："
echo "  1. 把 Origami.app 移动到 /应用程序 并至少运行一次（注册扩展）。"
echo "  2. 打开 系统设置 › 通用 › 登录项与扩展 › 访达扩展，启用 Origami。"
echo "  3. 在 Finder 中右键文件/文件夹，即可看到一级菜单「用 Origami 压缩 ▸」。"
