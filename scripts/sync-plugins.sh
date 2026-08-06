#!/usr/bin/env bash
# 开发工作流同步脚本:把仓库 plugins/ 下的插件同步到 ~/.plugkit/plugins/
# 用途:tauri dev 模式读取 ~/.plugkit/plugins,改仓库插件后运行此脚本即生效。
# 用法:./scripts/sync-plugins.sh [plugin_id]  (省略则同步全部)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$HOME/.plugkit/plugins"
mkdir -p "$DEST"

sync_one() {
  local id="$1"
  if [ ! -d "$ROOT/plugins/$id" ]; then
    echo "跳过:$id (仓库无此目录)"
    return
  fi
  rm -rf "$DEST/$id"
  cp -R "$ROOT/plugins/$id" "$DEST/"
  # 确保可执行
  chmod +x "$DEST/$id/tool"/plugkit-* 2>/dev/null || true
  echo "已同步:$id"
}

if [ $# -gt 0 ]; then
  sync_one "$1"
else
  for d in "$ROOT"/plugins/*/; do
    sync_one "$(basename "$d")"
  done
fi

echo "完成 → $DEST"
