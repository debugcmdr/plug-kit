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
import json, hashlib, pathlib, re, sys, zipfile

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
    # 5. binaryUrl 格式门禁:必须是 github release 合法 URL,禁止空 tag 造成的
    #    `releases/download//` 双斜杠坏 URL(package-plugins.sh 未传 tag 的旧 bug)。
    bu = p.get('binaryUrl', '')
    if not bu.startswith('https://github.com/') or '//download//' in bu or '/download//' in bu:
        print(f"ERROR: {p['id']} binaryUrl 格式非法(可能空 tag): {bu}")
        errors += 1
        print("  提示:开发期 tag 为空时 binaryUrl 应为 releases/latest/download/...,运行 ./scripts/package-plugins.sh 重新生成")

    # 7. 依赖声明完整性门禁(放在 zip 检查之前:无打包产物时也必须校验,防漏跑打包):
    #    CLI 源码里 dep("name") 引用的共享依赖必须声明在 manifest.dependencies
    #    (防 download/playlist 漏声明 ffmpeg 导致安装期不装、运行期才报 FFMPEG 类错误)。
    local_path = pathlib.Path('plugins') / p['id']
    cli_text = ''
    if local_path.is_dir():
        for f in local_path.glob('tool/plugkit-*'):
            if f.is_file():
                cli_text += f.read_text(encoding='utf-8', errors='ignore')
    deps_used = set(re.findall(r'\bdep\("([^"]+)"\)', cli_text))
    local_manifest_path = local_path / 'manifest.json'
    if deps_used and local_manifest_path.exists():
        declared = set((json.load(open(local_manifest_path)).get('dependencies') or {}).keys())
        missing = deps_used - declared
        if missing:
            print(f"ERROR: {p['id']} CLI 引用共享依赖 {sorted(missing)} 但 manifest.dependencies 未声明——"
                  f"请补充后重新运行 ./scripts/package-plugins.sh")
            errors += 1

    # 7b. 隐式依赖启发式门禁:yt-dlp 在合并/提取/转封装时内部经 PATH 调用 ffmpeg,
    #     dep() 字面扫描覆盖不到这类隐式依赖(CLI 无 dep("ffmpeg") 也实际需要)。
    #     规则:声明了 yt-dlp 且 CLI 出现触发参数 → 必须声明 ffmpeg,否则安装期不下载、
    #     运行期 yt-dlp 报 "ffmpeg not found"。(7 门禁的互补兜底,防未来新插件漏声明)
    if local_manifest_path.exists():
        declared = set((json.load(open(local_manifest_path)).get('dependencies') or {}).keys())
        if 'yt-dlp' in declared:
            ffmpeg_triggers = ('--merge-output-format', '--extract-audio', '--audio-format',
                               '--remux-video', '--convert-subs', '"-x"', "'-x'")
            hit = [t for t in ffmpeg_triggers if t in cli_text]
            if hit and 'ffmpeg' not in declared:
                print(f"ERROR: {p['id']} CLI 使用 yt-dlp 的 ffmpeg 触发参数 {hit} 但 manifest.dependencies "
                      f"未声明 ffmpeg——yt-dlp 合并/提取/转封装内部调用 ffmpeg,请声明后重新打包")
                errors += 1

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

        # 6. 公共共享模块一致性(方案 B):zip 内 tool/_*.py 必须与 plugins/shared/ 源码一致。
        #    防"改了 shared 却漏跑打包脚本"导致发布旧逻辑(行为漂移)。
        shared_dir = pathlib.Path('plugins/shared')
        if shared_dir.is_dir():
            for shared_py in sorted(shared_dir.glob('_*.py')):
                in_zip = [n for n in z.namelist() if n.endswith(f"tool/{shared_py.name}")]
                if not in_zip:
                    # 该插件不使用此共享模块(如 convert 不含 _ytdlp),不算错误
                    continue
                src_sha = hashlib.sha256(shared_py.read_bytes()).hexdigest()
                zip_sha = hashlib.sha256(z.read(in_zip[0])).hexdigest()
                if src_sha != zip_sha:
                    print(f"ERROR: {zip_path.name} 内 {shared_py.name} 与 plugins/shared/ 源码不一致——"
                          f"改共享模块后必须重新运行 ./scripts/package-plugins.sh")
                    errors += 1

if errors:
    print(f"ERROR: {errors} 处校验失败")
    sys.exit(1)
print("OK: 清单一致,插件包 sha256 校验通过,本地/zip 内 manifest 版本与清单一致,共享模块与源码一致,依赖声明完整")
PYEOF
