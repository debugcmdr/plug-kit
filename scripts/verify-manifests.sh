#!/usr/bin/env bash
# 校验市场清单一致性(CI 与发布前使用):
#   1. market/plugins.json 与 src-tauri/assets/fallback-manifests.json 的
#      version/sha256/size/binaryUrl 完全一致(双份清单由 package-plugins.sh 同步维护);
#   2. release/plugins/ 下 zip 的 sha256 与清单声明一致;
#   3. 本地 plugins/<id>/manifest.json 的 version 与清单一致(防「清单版本落后于
#      插件开发版本」的静默漂移,如 convert 已升 0.2.0 而市场仍挂 0.1.0);
#   4. zip 内 manifest.json 的 version 与清单一致(防 tag 统一覆盖造成的版本错位)。
# 用法: ./scripts/verify-manifests.sh [release/plugins 目录,默认 release/plugins]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
PLUGIN_DIR="${1:-release/plugins}"

python3 - "$PLUGIN_DIR" <<'PYEOF'
import json, hashlib, pathlib, sys, zipfile

plugin_dir = sys.argv[1]
paths = ['market/plugins.json', 'src-tauri/assets/fallback-manifests.json']
datas = [json.load(open(p))['plugins'] for p in paths]

if datas[0] != datas[1]:
    print("ERROR: market/plugins.json 与 fallback-manifests.json 内容不一致!")
    ids0 = {p['id'] for p in datas[0]}
    ids1 = {p['id'] for p in datas[1]}
    if ids0 != ids1:
        print(f"  缺失条目(市场缺): {sorted(ids1 - ids0)}")
        print(f"  缺失条目(fallback 缺): {sorted(ids0 - ids1)}")
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

    # 3. 本地插件 manifest version 与清单一致(发布门禁:防清单版本落后)
    local_path = pathlib.Path('plugins') / p['id'] / 'manifest.json'
    if not local_path.exists():
        print(f"ERROR: 本地插件清单缺失 {local_path}")
        errors += 1
    else:
        local_ver = json.load(open(local_path)).get('version')
        if local_ver != p['version']:
            print(f"ERROR: plugins/{p['id']}/manifest.json version={local_ver} != 清单 version={p['version']}")
            print("  提示:先运行 ./scripts/package-plugins.sh 重新打包并同步清单")
            errors += 1

    # 4. zip 内 manifest version 与清单一致(防 tag 覆盖造成 zip 内版本错位)
    with zipfile.ZipFile(zip_path) as z:
        inner = [n for n in z.namelist() if n.endswith('manifest.json')]
        if not inner:
            print(f"ERROR: {zip_path.name} 内无 manifest.json")
            errors += 1
        else:
            inner_ver = json.loads(z.read(inner[0])).get('version')
            if inner_ver != p['version']:
                print(f"ERROR: {zip_path.name} 内 manifest version={inner_ver} != 清单 version={p['version']}")
                errors += 1

if errors:
    print(f"ERROR: {errors} 处校验失败")
    sys.exit(1)
print("OK: 清单一致,插件包 sha256 校验通过,本地/zip 内 manifest 版本与清单一致")
PYEOF
