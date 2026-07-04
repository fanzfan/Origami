#!/bin/sh
# 构建 Origami FinderSync 扩展 (.appex)，嵌入已打包的 Origami.app 并重新签名。仅 macOS。
#
# 用法：finder-extension/build.sh <Origami.app 路径> [签名身份]
#   签名身份默认 "Origami Local Signing"（自签名证书）；也可传 Xcode 的临时开发证书名。
#
# 流程：swiftc 编译 -> 组装 .appex 包 -> 签名扩展 -> 重新签名整个 .app（连带封装 PlugIns）。
# 扩展通过 origami:// 深链回主应用，与 Services / Windows 右键流程共用后端。
#
# 注意：tauri-bundler 不支持 app 扩展，故此步骤在 `tauri build` 之后单独执行
#       （见 package.json 的 `build:mac`）。tauri 同时产出的 .dmg 不含扩展，
#       本机验证请直接安装嵌好扩展的 .app。
set -eu

APP="${1:?用法: build.sh <Origami.app 路径> [签名身份]}"
IDENTITY="${2:-Origami Local Signing}"

DIR="$(cd "$(dirname "$0")" && pwd)"
SDK="$(xcrun --show-sdk-path)"
EXT_ID="dev.vela.origami.finder"     # 必须以主应用 bundle id 为前缀
EXT_NAME="OrigamiFinder"

# 版本与主应用保持一致（读 tauri.conf.json，失败则 0.0.0）。
VERSION="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("version","0.0.0"))' \
  "$DIR/../src-tauri/tauri.conf.json" 2>/dev/null || echo 0.0.0)"

APPEX="$APP/Contents/PlugIns/$EXT_NAME.appex"
echo "==> 目标扩展: $APPEX (v$VERSION, 身份: $IDENTITY)"

rm -rf "$APPEX"
mkdir -p "$APPEX/Contents/MacOS"

echo "==> 编译 Swift 扩展"
# app 扩展入口为 _NSExtensionMain（由 Foundation 在运行时提供），无自定义 main()。
swiftc -target arm64-apple-macos11.0 -sdk "$SDK" \
  -framework Cocoa -framework FinderSync \
  -Xlinker -e -Xlinker _NSExtensionMain \
  -emit-executable -O \
  -o "$APPEX/Contents/MacOS/$EXT_NAME" \
  "$DIR/FinderSync.swift"

echo "==> 写入 Info.plist"
cat > "$APPEX/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Origami Finder</string>
  <key>CFBundleExecutable</key><string>$EXT_NAME</string>
  <key>CFBundleIdentifier</key><string>$EXT_ID</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>$EXT_NAME</string>
  <key>CFBundlePackageType</key><string>XPC!</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
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

echo "==> 签名扩展"
codesign --force --sign "$IDENTITY" \
  --entitlements "$DIR/OrigamiFinder.entitlements" \
  --identifier "$EXT_ID" \
  "$APPEX"

echo "==> 重新签名主应用（连带封装 PlugIns）"
codesign --force --sign "$IDENTITY" "$APP"

echo "==> 校验签名"
codesign --verify --deep --strict --verbose=2 "$APP"

cat <<TIP
==> 完成。
本机启用步骤：
  1) 把 $APP 安装到 /Applications 并启动一次（注册扩展）。
  2) 启用扩展：pluginkit -e use -i $EXT_ID
     或在「系统设置 → 通用 → 登录项与扩展 → 扩展 → 访达」里勾选 Origami。
  3) 重启访达：killall Finder
TIP
