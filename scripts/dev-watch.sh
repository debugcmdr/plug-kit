#!/usr/bin/env bash
# 热开发监听脚本:轮询 plugins/ 目录变化,自动同步到 ~/.plugkit/plugins/
#
# 用法:
#   ./scripts/dev-watch.sh           # 监听所有插件
#   ./scripts/dev-watch.sh download  # 只监听指定插件
#
# 与 npm run tauri dev 并行启动(或在另一个终端运行)。

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$HOME/.plugkit/plugins"
POLL_INTERVAL="${POLL_INTERVAL:-1}"  # 轮询间隔(秒)
mkdir -p "$DEST"

SYNCED_HASHES_FILE="$ROOT/.dev/watched-hashes"

# 计算目录的"指纹"(所有文件按路径排序的 inode+mtime 拼接)
dir_hash() {
  local dir="$1"
  if [[ ! -d "$dir" ]]; then
    echo "missing"
    return
  fi
  # macOS/BSD find 不支持 -printf,用 stat 获取 mtime
  (cd "$dir" && find . -type f | sort | xargs -I{} stat -f '%m %N' {}) | md5 | awk '{print $1}'
}

sync_plugin() {
  local id="$1"
  local src="$ROOT/plugins/$id"
  local dst="$DEST/$id"

  if [[ ! -d "$src" ]]; then
    # 插件被删除
    if [[ -d "$dst" ]]; then
      echo "[$(date '+%H:%M:%S')] 插件 $id 已从仓库移除,清理 $dst"
      rm -rf "$dst"
    fi
    return
  fi

  local new_hash
  new_hash=$(dir_hash "$src")
  local old_hash
  old_hash=$(cat "$SYNCED_HASHES_FILE" 2>/dev/null | grep "^${id}=" | cut -d= -f2 || echo "")

  if [[ "$new_hash" != "$old_hash" ]]; then
    echo "[$(date '+%H:%M:%S')] 检测到 $id 变化,同步中..."
    rm -rf "$dst"
    cp -R "$src" "$dst"
    chmod +x "$dst/tool/plugkit-"* 2>/dev/null || true
    # 更新指纹
    sed -i '' "/^${id}=/d" "$SYNCED_HASHES_FILE" 2>/dev/null || true
    echo "${id}=${new_hash}" >> "$SYNCED_HASHES_FILE"
    echo "[$(date '+%H:%M:%S')] $id 同步完成"
  fi
}

# 收集要监听的插件列表
TARGETS=()
if [[ $# -gt 0 ]]; then
  for id in "$@"; do
    TARGETS+=("$id")
  done
else
  for d in "$ROOT"/plugins/*/; do
    [[ -d "$d" ]] && TARGETS+=("$(basename "$d")")
  done
fi

echo "=== PlugKit 热开发监听 ==="
echo "仓库: $ROOT/plugins/"
echo "目标: $DEST"
echo "轮询间隔: ${POLL_INTERVAL}s"
echo "监听: ${TARGETS[*]}"
echo "按 Ctrl+C 停止"
echo ""

# 首次全量同步
for id in "${TARGETS[@]}"; do
  sync_plugin "$id"
done

# 持续轮询
while true; do
  sleep "$POLL_INTERVAL"
  for id in "${TARGETS[@]}"; do
    sync_plugin "$id"
  done
done
