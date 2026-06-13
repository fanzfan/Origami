#!/bin/bash
# 创建一个本机自签名「代码签名」证书并加入登录钥匙串信任，
# 供 Finder 扩展签名使用（macOS 不会加载仅 ad-hoc 签名的 Finder 扩展）。
# 幂等：已存在同名身份则直接跳过创建。
set -euo pipefail

CERT_CN="Origami Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning 2>/dev/null | grep -q "$CERT_CN"; then
  echo "✓ 已存在签名身份：$CERT_CN"
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "› 生成自签名代码签名证书…"
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$TMP/key.pem" \
  -out "$TMP/cert.pem" \
  -days 3650 \
  -subj "/CN=$CERT_CN" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -addext "basicConstraints=critical,CA:false" 2>/dev/null

# -legacy + 老式 PBE：macOS 的 security 只能导入旧格式 PKCS12
openssl pkcs12 -export -out "$TMP/bundle.p12" \
  -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -legacy -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 \
  -passout pass:origami 2>/dev/null

echo "› 导入登录钥匙串…"
security import "$TMP/bundle.p12" -k "$KEYCHAIN" -P origami \
  -T /usr/bin/codesign -T /usr/bin/security -A >/dev/null

# 允许 codesign 无提示访问私钥
security set-key-partition-list -S apple-tool:,apple:,codesign: \
  -s -k "" "$KEYCHAIN" >/dev/null 2>&1 || true

echo "› 加入用户信任（代码签名）…"
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$TMP/cert.pem" \
  || echo "  （信任设置可能弹出授权对话框，请确认。）"

echo ""
if security find-identity -v -p codesigning 2>/dev/null | grep -q "$CERT_CN"; then
  echo "✓ 完成。可用签名身份：$CERT_CN"
else
  echo "⚠ 身份未出现在 codesigning 列表，可能需要在弹出的对话框中授权后重试。"
fi
