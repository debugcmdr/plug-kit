#!/usr/bin/env bash
# 打包内置插件为可分发的 zip,并同步更新 market/plugins.json + fallback 快照的
# version/sha256/size/binaryUrl(双份清单同一脚本维护,杜绝漂移);密钥存在时自动签名。
# 产物:release/plugins/{id}.zip (用于上传 GitHub Release)
# 用法:./scripts/package-plugins.sh [tag]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# 当前 tag(默认为空,表示未发布)
TAG="${1:-}"
# 版本号:优先 tag,否则 0.1.0
VERSION="${TAG#v}"
[ -z "$VERSION" ] && VERSION="0.1.0"
# 主仓库(市场清单/插件发布目标)。切换组织/仓库只需改这一处,或用环境变量覆盖。
REPO="${PLUGKIT_REPO:-debugcmdr/plug-kit}"

# 打包输出目录:不用 dist/(vite 构建会清空它),用 release/
OUT_DIR="release/plugins"
mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/*.zip

for id in download convert; do
  echo "=== 打包 $id ==="

  # 临时打包目录:manifest.json + tool/
  tmpdir="$(mktemp -d)/$id"
  mkdir -p "$tmpdir/tool"
  cp "plugins/$id/manifest.json" "$tmpdir/manifest.json"
  cp -R "plugins/$id/tool/." "$tmpdir/tool/"
  # 确保可执行
  chmod +x "$tmpdir/tool"/plugkit-* 2>/dev/null || true
  # 固定文件时间戳,保证 zip 确定性(sha256 可复现)
  find "$tmpdir" -exec touch -t 202401010000 {} +

  # 生成 zip(结构:manifest.json + tool/),文件名带版本
  (cd "$(dirname "$tmpdir")" && zip -r -X "$ROOT/$OUT_DIR/$id-$VERSION.zip" "$id" >/dev/null)

  # 计算 sha256
  SHA=$(shasum -a 256 "$OUT_DIR/$id-$VERSION.zip" | awk '{print $1}')
  SIZE=$(stat -f%z "$OUT_DIR/$id-$VERSION.zip")
  echo "  $id-$VERSION.zip: ${SIZE} bytes, sha256=$SHA"

  # 若存在签名私钥,生成 Ed25519 签名写入清单(signature 字段)
  SIG=""
  KEY="${PLUGKIT_SIGN_KEY:-./.signing/plugkit-sign.key}"
  if [ -f "$KEY" ]; then
    SIG="$(./scripts/sign-plugin.sh "$OUT_DIR/$id-$VERSION.zip" "$KEY" 2>/dev/null || true)"
    if [ -n "$SIG" ]; then echo "  已签名 $id"; fi
  fi

  # 同步更新 fallback 快照 + 市场清单(双份清单由同一脚本维护,杜绝版本漂移)
  python3 - "$id" "$SHA" "$SIZE" "$TAG" "$REPO" "$SIG" <<'PYEOF'
import json, sys
id_, sha, size, tag, repo, sig = sys.argv[1:7]
paths = ['src-tauri/assets/fallback-manifests.json', 'market/plugins.json']
for path in paths:
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
