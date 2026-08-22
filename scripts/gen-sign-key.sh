#!/usr/bin/env bash
# 生成插件签名密钥对(Ed25519)。
# 用法: ./scripts/gen-sign-key.sh [输出目录,默认 ./.signing]
# 产物:
#   ./.signing/plugkit-sign.key     私钥(务必妥善保管,勿提交仓库!)
#   ./.signing/plugkit-sign.pub.b64 公钥(base64,填入 src-tauri/src/security.rs 的 OFFICIAL_PUBLIC_KEY)
set -euo pipefail

OUT_DIR="${1:-./.signing}"
mkdir -p "$OUT_DIR"

openssl genpkey -algorithm ED25519 -out "$OUT_DIR/plugkit-sign.key" 2>/dev/null
# 公钥 DER 去掉 30 长度头等,取最后 32 字节(原始 Ed25519 公钥),base64 编码
openssl pkey -in "$OUT_DIR/plugkit-sign.key" -pubout -outform DER 2>/dev/null \
  | tail -c 32 | base64 > "$OUT_DIR/plugkit-sign.pub.b64"

echo "=== 生成完成 ==="
echo "私钥: $OUT_DIR/plugkit-sign.key  (请备份并妥善保管)"
echo "公钥: $OUT_DIR/plugkit-sign.pub.b64"
echo ""
echo "把以下公钥填入 src-tauri/src/security.rs 的 OFFICIAL_PUBLIC_KEY:"
cat "$OUT_DIR/plugkit-sign.pub.b64"
