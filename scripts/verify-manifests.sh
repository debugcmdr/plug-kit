#!/usr/bin/env bash
# 校验市场清单一致性(CI 与发布前使用):
#   1. market/plugins.json 与 src-tauri/assets/fallback-manifests.json 的
#      version/sha256/size/binaryUrl 完全一致(双份清单由 package-plugins.sh 同步维护);
#   2. release/plugins/ 下 zip 的 sha256 与清单声明一致。
# 用法: ./scripts/verify-manifests.sh [release/plugins 目录,默认 release/plugins]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
PLUGIN_DIR="${1:-release/plugins}"

python3 - "$PLUGIN_DIR" <<'PYEOF'
import json, hashlib, pathlib, sys

plugin_dir = sys.argv[1]
paths = ['market/plugins.json', 'src-tauri/assets/fallback-manifests.json']
datas = [json.load(open(p))['plugins'] for p in paths]

if datas[0] != datas[1]:
    print("ERROR: market/plugins.json 与 fallback-manifests.json 内容不一致!")
    ids0 = {p['id'] for p in datas[0]}
    ids1 = {p['id'] for p in datas[1]}
    for i, (a, b) in enumerate(zip(datas[0], datas[1])):
        for k in ('version', 'sha256', 'size', 'binaryUrl'):
            if a.get(k) != b.get(k):
                print(f"  [{i}] {a['id']} field '{k}': market={a.get(k)} fallback={b.get(k)}")
    print("  提示:先运行 ./scripts/package-plugins.sh <tag> 再提交")
    sys.exit(1)

errors = 0
for p in datas[0]:
    zip_path = pathlib.Path(plugin_dir) / f"{p['id']}-{p['version']}.zip"
    if not zip_path.exists():
        print(f"WARN: 未找到 {zip_path}(本地无打包产物时跳过 sha256 校验)")
        continue
    actual = hashlib.sha256(zip_path.read_bytes()).hexdigest()
    if actual != p['sha256']:
        print(f"ERROR: {zip_path.name} sha256 不匹配: 清单={p['sha256']} 实际={actual}")
        errors += 1

if errors:
    print(f"ERROR: {errors} 个插件包校验失败")
    sys.exit(1)
print("OK: 清单一致,插件包 sha256 校验通过")
PYEOF
