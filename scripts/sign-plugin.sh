#!/usr/bin/env bash
# 对插件 zip 做 Ed25519 签名(输出 base64 签名)。
# 用法:
#   ./scripts/sign-plugin.sh <zip> [私钥路径]
#   私钥也可通过环境变量 PLUGKIT_SIGN_KEY 指定;默认 ./.signing/plugkit-sign.key
# 输出:base64 签名(可写入 market/plugins.json 条目的 "signature" 字段)
set -euo pipefail

ZIP="${1:?用法: ./scripts/sign-plugin.sh <zip> [私钥路径]}"
KEY="${2:-${PLUGKIT_SIGN_KEY:-./.signing/plugkit-sign.key}}"

[ -f "$ZIP" ] || { echo "错误:文件不存在: $ZIP"; exit 1; }
[ -f "$KEY" ] || { echo "错误:私钥不存在: $KEY (先运行 ./scripts/gen-sign-key.sh)"; exit 1; }

# OpenSSL 3 使用 -rawin;LibreSSL/macOS 可能不支持,给出提示
if openssl pkeyutl -sign -inkey "$KEY" -rawin -in "$ZIP" 2>/dev/null | base64; then
  exit 0
fi
echo "错误:openssl pkeyutl -rawin 不可用(可能是 LibreSSL/OpenSSL 1.x)。" >&2
echo "建议:安装 OpenSSL 3 (brew install openssl@3) 或改用 minisign 签名并在 manifest 登记签名。" >&2
exit 1
