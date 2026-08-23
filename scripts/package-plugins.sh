#!/usr/bin/env bash
# 打包内置插件为可分发的 zip,并同步更新 market/plugins.json + fallback 快照的
# version/sha256/size/binaryUrl(双份清单同一脚本维护,杜绝漂移);密钥存在时自动签名。
# 产物:release/plugins/{id}.zip (用于上传 GitHub Release)
# 用法:./scripts/package-plugins.sh [tag]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# 当前 tag(默认为空,表示未发布)。tag 仅标记发布批次(binaryUrl 的 release 段),
# 插件版本一律以各插件自身 manifest.json 的 version 为唯一事实源——
# 避免「清单版本 = tag 统一覆盖」导致 zip 内 manifest 与清单版本漂移。
TAG="${1:-}"
# 主仓库(市场清单/插件发布目标)。切换组织/仓库只需改这一处,或用环境变量覆盖。
REPO="${PLUGKIT_REPO:-debugcmdr/plug-kit}"

# 打包输出目录:不用 dist/(vite 构建会清空它),用 release/
OUT_DIR="release/plugins"
mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.zip

for id in download convert playlist; do
  echo "=== 打包 $id ==="

  # 版本以插件自身 manifest.json 为准(唯一事实源),zip 命名与清单 version 同源。
  VER="$(python3 -c "import json;print(json.load(open('plugins/$id/manifest.json'))['version'])")"

  # 临时打包目录:manifest.json + tool/
  tmpdir="$(mktemp -d)/$id"
  mkdir -p "$tmpdir/tool"
  cp "plugins/$id/manifest.json" "$tmpdir/manifest.json"
  cp -R "plugins/$id/tool/." "$tmpdir/tool/"
  # 排除 Python 缓存,保证 zip 确定性(sha256 可复现,不受本地 pyc 影响)
  rm -rf "$tmpdir/tool/__pycache__"
  # 确保可执行
  chmod +x "$tmpdir/tool"/plugkit-* 2>/dev/null || true
  # 固定文件时间戳,保证 zip 确定性(sha256 可复现)
  find "$tmpdir" -exec touch -t 202401010000 {} +

  # 生成 zip(结构:manifest.json + tool/),文件名带版本(与插件 manifest 一致)
  (cd "$(dirname "$tmpdir")" && zip -r -X "$ROOT/$OUT_DIR/$id-$VER.zip" "$id" >/dev/null)

  # 计算 sha256
  SHA=$(shasum -a 256 "$OUT_DIR/$id-$VER.zip" | awk '{print $1}')
  SIZE=$(stat -f%z "$OUT_DIR/$id-$VER.zip")
  echo "  $id-$VER.zip: ${SIZE} bytes, sha256=$SHA"

  # 若存在签名私钥,生成 Ed25519 签名写入清单(signature 字段)
  SIG=""
  KEY="${PLUGKIT_SIGN_KEY:-./.signing/plugkit-sign.key}"
  if [ -f "$KEY" ]; then
    SIG="$(./scripts/sign-plugin.sh "$OUT_DIR/$id-$VER.zip" "$KEY" 2>/dev/null || true)"
    if [ -n "$SIG" ]; then echo "  已签名 $id"; fi
  fi

  # 同步更新 fallback 快照 + 市场清单(双份清单由同一脚本维护,杜绝版本漂移)
  python3 - "$id" "$SHA" "$SIZE" "$TAG" "$REPO" "$SIG" <<'PYEOF'
import json, sys
id_, sha, size, tag, repo, sig = sys.argv[1:7]
paths = ['src-tauri/assets/fallback-manifests.json', 'market/plugins.json']
# 展示字段(name/description/category/tags)以插件 manifest.json 为唯一事实源
# 反向同步,避免市场面板与已安装插件信息漂移(verify-manifests 只查技术字段)。
local_manifest = json.load(open(f'plugins/{id_}/manifest.json'))
for path in paths:
    data = json.load(open(path))
    for p in data['plugins']:
        if p['id'] == id_:
            p['sha256'] = sha
            p['size'] = int(size)
            # 版本同以本地 manifest 为准(与 zip 内 manifest 恒一致,杜绝版本漂移)
            ver = local_manifest.get('version', p.get('version', '0.1.0'))
            p['version'] = ver
            p['name'] = local_manifest.get('name', p['name'])
            p['description'] = local_manifest.get('description', p['description'])
            if local_manifest.get('category'):
                p['category'] = local_manifest['category']
            if local_manifest.get('tags'):
                p['tags'] = local_manifest['tags']
            # 插件 zip 放在主仓库 release 资产,按平台命名(当前仅打包当前平台)
            p['binaryUrl'] = f"https://github.com/{repo}/releases/download/{tag}/{id_}-{ver}.zip"
            p['releaseUrl'] = f"https://github.com/{repo}/releases/latest"
            p['homepage'] = f"https://github.com/{repo}"
            if sig:
                p['signature'] = sig
            elif 'signature' in p:
                del p['signature']
    json.dump(data, open(path, 'w'), ensure_ascii=False, indent=2)
    print(f"  updated {path}")
PYEOF
done

echo ""
echo "=== 打包完成 ==="
ls -la "$OUT_DIR/"
echo ""
echo "=== fallback 清单已更新 ==="
cat src-tauri/assets/fallback-manifests.json
