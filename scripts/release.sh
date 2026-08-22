#!/usr/bin/env bash
# 创建 GitHub Release 并上传插件 zip。
# 用法:./scripts/release.sh <tag> [release-title]
# 前置:已运行 ./scripts/package-plugins.sh <tag> 生成 zip
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TAG="${1:?用法: ./scripts/release.sh <tag> [title]}"
TITLE="${2:-$TAG}"
# 主仓库,与 package-plugins.sh 保持一致,可用环境变量 PLUGKIT_REPO 覆盖
REPO="${PLUGKIT_REPO:-debugcmdr/plug-kit}"

# 从 osxkeychain 取 GitHub token(不打印)
TOKEN="$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill 2>/dev/null | grep '^password=' | cut -d= -f2-)"
if [ -z "$TOKEN" ]; then echo "错误:无法从 keychain 获取 GitHub 凭据"; exit 1; fi

# 确认 zip 存在
for f in release/plugins/*.zip; do
  [ -e "$f" ] || { echo "错误:没有打包好的 zip,先运行 ./scripts/package-plugins.sh $TAG"; exit 1; }
done

echo "=== 创建 Release $TAG ==="
# 幂等:若 tag 的 release 已存在则复用,否则创建
EXISTING="$(curl -s -H "Authorization: Bearer $TOKEN" "https://api.github.com/repos/$REPO/releases/tags/$TAG")"
RELEASE_ID="$(echo "$EXISTING" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("id",""))' 2>/dev/null || true)"

if [ -z "$RELEASE_ID" ]; then
  RESP="$(curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
    -d "{\"tag_name\":\"$TAG\",\"name\":\"$TITLE\",\"body\":\"PlugKit $TAG — 内置插件包\",\"draft\":false,\"prerelease\":false}" \
    "https://api.github.com/repos/$REPO/releases")"
  RELEASE_ID="$(echo "$RESP" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("id",""))')"
  [ -n "$RELEASE_ID" ] || { echo "创建 Release 失败: $RESP"; exit 1; }
  echo "  Release $RELEASE_ID 已创建"
else
  echo "  Release $RELEASE_ID 已存在,复用"
fi

echo "=== 上传插件 zip ==="
for f in release/plugins/*.zip; do
  NAME="$(basename "$f")"
  echo "  上传 $NAME ..."
  curl -s -X POST -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/zip" \
    --data-binary @"$f" \
    "https://uploads.github.com/repos/$REPO/releases/$RELEASE_ID/assets?name=$NAME" \
    -o /dev/null -w "    HTTP %{http_code}\n"
done

echo ""
echo "=== 完成!==="
echo "查看: https://github.com/$REPO/releases/tag/$TAG"
