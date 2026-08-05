#!/usr/bin/env bash
# 打包三个内置插件为可分发的 zip,并更新 fallback 清单的 sha256。
# 产物:dist/plugins/{id}.zip (用于上传 GitHub Release)
# 用法:./scripts/package-plugins.sh [tag]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# 当前 tag(默认为空,表示未发布)
TAG="${1:-}"
# 版本号:优先 tag,否则 0.1.0
VERSION="${TAG#v}"
[ -z "$VERSION" ] && VERSION="0.1.0"
REPO="debugcmdr/plug-kit"

mkdir -p dist/plugins
rm -f dist/plugins/*.zip

for id in download convert audio-extract; do
  echo "=== 打包 $id ==="

  # 临时打包目录:manifest.json + tool/
  tmpdir="$(mktemp -d)/$id"
  mkdir -p "$tmpdir/tool"
  cp "plugins/$id/manifest.json" "$tmpdir/manifest.json"
  cp -R "plugins/$id/tool/." "$tmpdir/tool/"
  # 确保可执行
  chmod +x "$tmpdir/tool"/plugkit-* 2>/dev/null || true

  # 生成 zip(结构:manifest.json + tool/),文件名带版本
  (cd "$(dirname "$tmpdir")" && zip -r -X "$ROOT/dist/plugins/$id-$VERSION.zip" "$id" >/dev/null)

  # 计算 sha256
  SHA=$(shasum -a 256 "dist/plugins/$id-$VERSION.zip" | awk '{print $1}')
  SIZE=$(stat -f%z "dist/plugins/$id-$VERSION.zip")
  echo "  $id-$VERSION.zip: ${SIZE} bytes, sha256=$SHA"

  # 更新 fallback 清单里的 sha256/size/binaryUrl
  python3 - "$id" "$SHA" "$SIZE" "$TAG" "$REPO" <<'PYEOF'
import json, sys
id_, sha, size, tag, repo = sys.argv[1:6]
path = 'src-tauri/assets/fallback-manifests.json'
data = json.load(open(path))
for p in data['plugins']:
    if p['id'] == id_:
        p['sha256'] = sha
        p['size'] = int(size)
        ver = tag.lstrip('v') if tag else p['version']
        p['version'] = ver
        # 插件 zip 放在主仓库 release 资产,按平台命名(当前仅打包当前平台)
        p['binaryUrl'] = f"https://github.com/{repo}/releases/download/{tag}/{id_}-{ver}.zip"
        p['releaseUrl'] = f"https://github.com/{repo}/releases/latest"
        p['homepage'] = f"https://github.com/{repo}"
json.dump(data, open(path, 'w'), ensure_ascii=False, indent=2)
PYEOF
done

echo ""
echo "=== 打包完成 ==="
ls -la dist/plugins/
echo ""
echo "=== fallback 清单已更新 ==="
cat src-tauri/assets/fallback-manifests.json
