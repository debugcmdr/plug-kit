#!/usr/bin/env bash
# 生成新插件骨架(manifest + CLI + UI),直击「降低新增插件工作量」目标(B)。
# 用法: ./scripts/new-plugin.sh <id> [显示名称]
# 产出: plugins/<id>/{manifest.json, tool/plugkit-<id>, tool/index.html}
# 之后: 实现业务 → ./scripts/package-plugins.sh(自动发现,零配置) → 上传 release
set -euo pipefail

ID="${1:-}"
if [ -z "$ID" ]; then
  echo "用法: ./scripts/new-plugin.sh <id> [显示名称]"
  echo "示例: ./scripts/new-plugin.sh compressor 视频压缩"
  exit 1
fi
# id 仅允许小写字母/数字/连字符(插件目录名 = manifest id = 可执行文件名,三者同源)
if [[ ! "$ID" =~ ^[a-z0-9-]+$ ]]; then
  echo "错误: id 仅允许小写字母/数字/连字符(如 download / my-tool)"
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -d "$ROOT/plugins/$ID" ]; then
  echo "错误: plugins/$ID 已存在"
  exit 1
fi
NAME="${2:-$ID}"
mkdir -p "$ROOT/plugins/$ID/tool"

# 用 python 生成(占位符 __ID__/__NAME__ 替换,避免 bash heredoc 的 $ 转义地狱)
python3 - "$ID" "$NAME" "$ROOT/plugins/$ID" <<'PYEOF'
import os
import sys

pid, name, base = sys.argv[1], sys.argv[2], sys.argv[3]

manifest = """{
  "id": "__ID__",
  "name": "__NAME__",
  "version": "0.1.0",
  "description": "__NAME__ 插件(骨架)",
  "author": "plug-kit",
  "category": "media",
  "tags": ["__ID__"],
  "platforms": ["windows-x64", "macos-arm64", "macos-x64", "linux-x64"],
  "dependencies": {},
  "bridgeVersion": ">=1.0.0",
  "entry": {
    "type": "cli",
    "executable": "plugkit-__ID__",
    "toolDir": "tool"
  },
  "commands": {
    "hello": {
      "stdinArgs": ["hello", "--name", "{name}"],
      "output": "json"
    }
  },
  "releaseUrl": "https://github.com/debugcmdr/plug-kit/releases/latest"
}
"""

cli = """#!/usr/bin/env python3
\"\"\"plugkit-__ID__ - __NAME__(骨架)。

新插件协议要点(详见 plugins/shared/_protocol.py):
- stdout 只输出协议行 progress/result/error,其余一律不许 print
- 短命令 manifest output=json(单行 result 结束);长任务 progress+json(流式进度)
- 共享依赖经 dep("name") 获取(外壳注入 PLUGKIT_* 环境变量,缺失回退系统 PATH)
- 打包自动发现:改完运行 ./scripts/package-plugins.sh 即生成 zip + 同步清单
\"\"\"
import os
import sys

try:
    from _protocol import progress, result, error, dep, parse_args  # noqa: F401
except ImportError:
    sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "shared"))
    from _protocol import progress, result, error, dep, parse_args  # noqa: F401


def hello(name):
    # 示例命令:替换为真实业务;长任务用 progress() 流式上报(percent/speed/eta)
    result({"greeting": f"你好, {name}!", "source": "plugkit-__ID__"})


def main():
    argv = sys.argv[1:]
    if not argv:
        result({"usage": "__ID__ <hello> --name NAME"})
        return
    cmd = argv[0]
    args = parse_args(argv[1:])
    try:
        if cmd == "hello":
            hello(args.get("name", "世界"))
        else:
            error("UNKNOWN_CMD", f"未知命令: {cmd}")
    except Exception as e:
        error("PLUGIN_ERROR", str(e))
        sys.exit(1)


if __name__ == "__main__":
    main()
"""

html = """<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>__NAME__</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: -apple-system, 'Segoe UI', 'PingFang SC', sans-serif; background: #f5f6f8; color: #1d2129; padding: 22px; }
    .card { background: #fff; border-radius: 14px; padding: 20px 24px; box-shadow: 0 1px 3px rgba(0,0,0,.05); }
    h2 { font-size: 18px; font-weight: 700; margin-bottom: 4px; }
    .sub { font-size: 12px; color: #86909c; margin-bottom: 16px; }
    .btn { background: #3370ff; color: #fff; border: none; border-radius: 8px; padding: 10px 20px; font-size: 14px; cursor: pointer; }
    .btn:hover { background: #4a82ff; }
    .status { margin-top: 12px; font-size: 13px; color: #646a73; white-space: pre-wrap; word-break: break-all; line-height: 1.6; }
  </style>
</head>
<body>
  <div class="card">
    <h2>__NAME__</h2>
    <div class="sub">插件骨架:改造此页,通过 MT.invoke 调用后端命令。</div>
    <button class="btn" id="btnHello">调用 hello</button>
    <div class="status" id="status">就绪。</div>
  </div>
  <script>
    const MT = window.MT || window.PlugKit;
    const $ = (id) => document.getElementById(id);

    // 外部数据拼 innerHTML 前必须 esc(标题/URL 等不可信)
    function esc(s) {
      return String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
      }[c]));
    }

    // 沙盒 iframe 无 allow-modals,alert/confirm 被静默阻断 → 统一 toast
    let toastEl = null;
    function toast(msg, isErr) {
      if (!toastEl) {
        toastEl = document.createElement('div');
        toastEl.style.cssText = 'position:fixed;left:50%;bottom:28px;transform:translateX(-50%);z-index:999;max-width:80%;padding:10px 18px;border-radius:9px;font-size:13px;line-height:1.5;color:#fff;background:rgba(29,33,41,.92);box-shadow:0 6px 20px rgba(0,0,0,.18);transition:opacity .25s;opacity:0;pointer-events:none;word-break:break-all;';
        document.body.appendChild(toastEl);
      }
      toastEl.textContent = msg;
      toastEl.style.background = isErr ? 'rgba(213,73,65,.94)' : 'rgba(29,33,41,.92)';
      toastEl.style.opacity = '1';
      clearTimeout(toastEl._t);
      toastEl._t = setTimeout(() => { toastEl.style.opacity = '0'; }, 3600);
    }

    $('btnHello').onclick = async () => {
      try {
        // 长任务(progress 模式)必须传 { timeout: 0 } 禁用 invoke 超时
        const r = await MT.invoke('hello', { name: 'PlugKit' });
        $('status').textContent = JSON.stringify(r);
      } catch (e) {
        toast('调用失败: ' + ((e && e.message) || e), true);
      }
    };
  </script>
</body>
</html>
"""

files = [
    ("manifest.json", manifest),
    (os.path.join("tool", f"plugkit-{pid}"), cli),
    (os.path.join("tool", "index.html"), html),
]
for rel, content in files:
    path = os.path.join(base, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(content.replace("__ID__", pid).replace("__NAME__", name))
    # 可执行文件加执行位(与现有插件一致)
    if rel.startswith("tool") and "plugkit-" in rel:
        os.chmod(path, 0o755)
    print(f"  生成 {os.path.relpath(path, os.path.dirname(base))}")
PYEOF

echo ""
echo "=== 插件骨架已生成: plugins/$ID ==="
echo "下一步:"
echo "  1. 实现 tool/plugkit-$ID 的业务逻辑(与 manifest.commands 的命令对齐)"
echo "  2. 定制 tool/index.html 的界面(参考 download/convert 的成熟页面)"
echo "  3. ./scripts/sync-plugins.sh $ID        # 同步 dev 副本(开发热更新)"
echo "  4. ./scripts/package-plugins.sh         # 打包(自动发现,零配置)"
