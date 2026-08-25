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

# ===== 发布门禁 1:依赖 sha256 固化检查 =====
# 发布前必须把 ffmpeg/ffprobe 各平台 sha256 固化到 dep_manifest.rs
# (未固化 = 用户端无完整性校验下载并执行,供应链风险;见审查报告 G-8)。
# 若存在空 sha256 的平台,拒绝发布。
python3 - <<'PYEOF'
import sys
src = open('src-tauri/src/dep_manifest.rs', encoding='utf-8').read()
pending = []
for asset in ['ffmpeg-linux-x64', 'ffmpeg-darwin-x64', 'ffmpeg-win32-x64',
              'ffprobe-linux-x64', 'ffprobe-darwin-x64', 'ffprobe-win32-x64']:
    for line in src.splitlines():
        if f'"{asset}"' in line and 'String::new()' in line:
            pending.append(asset)
if pending:
    print(f"ERROR: 以下平台 ffmpeg/ffprobe sha256 未固化,拒绝发布: {sorted(pending)}")
    print("  提示:在对应平台下载资产后运行 `shasum -a 256 <file>` 固化到 dep_manifest.rs 对应分支,再执行 release")
    sys.exit(1)
print("OK: 各平台 ffmpeg/ffprobe sha256 已固化")
PYEOF
# ===== 发布门禁 2:清单一致性 =====
# 双清单一致性 + zip sha256 + binaryUrl 格式 + 共享模块一致性
./scripts/verify-manifests.sh

# ===== 发布门禁 3:签名状态检查(B-5) =====
# OFFICIAL_PUBLIC_KEY 已配置 → 强制所有插件包已签名(清单 signature 字段),否则拒绝发布
# (与 install 期"配置公钥后未签名拒绝安装"保持一致性);
# 未配置 → 仅警告不阻断(签名是可选项,用户可故意不启用)。
python3 - <<'PYEOF'
import json, re, sys
sec = open('src-tauri/src/security.rs', encoding='utf-8').read()
# 正则解析常量赋值(D-2):原字符串匹配对格式/注释变化敏感(如缩进、`Option < &str>`),
# 误判会导致「实际已配置公钥却被跳过签名门禁」。
m = re.search(r'OFFICIAL_PUBLIC_KEY\s*:\s*Option\s*<\s*&str\s*>\s*=\s*(Some\([^)]*\)|None)', sec)
configured = bool(m and m.group(1).startswith('Some'))
market = json.load(open('market/plugins.json'))['plugins']
unsigned = [p['id'] for p in market if not p.get('signature')]
if configured:
    if unsigned:
        print(f"ERROR: 已启用官方签名公钥,但以下插件包未签名,拒绝发布: {unsigned}")
        print("  提示:运行 ./scripts/sign-plugin.sh 对 zip 签名并重新执行 package-plugins.sh 同步清单")
        sys.exit(1)
    print("OK: 签名已启用,所有插件包均已签名")
else:
    print("WARN: 官方签名公钥未配置(OFFICIAL_PUBLIC_KEY = None)——发布后用户端仅 sha256 完整性校验,")
    print("      无真实性校验(市场清单被攻破后可被替换为恶意包)。建议发布前启用签名:")
    print("      ./scripts/gen-sign-key.sh 生成密钥 → 公钥填入 security.rs → 重新打包(签名自动)")
PYEOF

# 从 osxkeychain 取 GitHub token(不打印)
TOKEN="$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill 2>/dev/null | grep '^password=' | cut -d= -f2-)"
if [ -z "$TOKEN" ]; then echo "错误:无法从 keychain 获取 GitHub 凭据"; exit 1; fi

# ===== 鉴权传参(安全):token 经 stdin 传给 curl(-H @-),不出现于进程命令行 =====
# 原实现 `-H "Authorization: Bearer $TOKEN"` 会暴露在 ps 中(本机共享/日志记录时泄露)。
# 用法: gh_auth <header1> <header2> ... | curl -H @- ...
gh_auth() {
  for h in "$@"; do printf '%s\n' "$h"; done
  printf 'Authorization: Bearer %s\n' "$TOKEN"
}

# 确认 zip 存在
for f in release/plugins/*.zip; do
  [ -e "$f" ] || { echo "错误:没有打包好的 zip,先运行 ./scripts/package-plugins.sh $TAG"; exit 1; }
done

echo "=== 创建 Release $TAG ==="
# 幂等:若 tag 的 release 已存在则复用,否则创建
EXISTING="$(gh_auth | curl -s -H @- "https://api.github.com/repos/$REPO/releases/tags/$TAG")"
RELEASE_ID="$(echo "$EXISTING" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("id",""))' 2>/dev/null || true)"

if [ -z "$RELEASE_ID" ]; then
  # JSON body 用 python3 构造(D-3):TAG/TITLE 含引号或特殊字符时,直接拼字符串
  # 会破坏 JSON 导致创建失败(发布者输入,不可信)。
  BODY="$(python3 -c "import json,sys; print(json.dumps({'tag_name':sys.argv[1],'name':sys.argv[2],'body':'PlugKit %s — 内置插件包' % sys.argv[1],'draft':False,'prerelease':False}))" "$TAG" "$TITLE")"
  RESP="$(gh_auth 'Content-Type: application/json' | curl -s -X POST -H @- \
    -d "$BODY" \
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
  # -H @- 读 stdin 的 header,--data-binary @file 读文件 body,两者互不冲突
  gh_auth 'Content-Type: application/zip' | curl -s -X POST -H @- \
    --data-binary @"$f" \
    "https://uploads.github.com/repos/$REPO/releases/$RELEASE_ID/assets?name=$NAME" \
    -o /dev/null -w "    HTTP %{http_code}\n"
done

echo ""
echo "=== 完成!==="
echo "查看: https://github.com/$REPO/releases/tag/$TAG"
