"""PlugKit yt-dlp 共享逻辑模块(_ytdlp)。

由 plugins/shared/ 在打包/同步时复制进 download/playlist 的 tool/ 目录。
包含:cookies 参数解析、播放列表参数、yt-dlp 错误诊断分类(三级报错链路的诊断层)、
yt-dlp 可执行解析(优先共享 venv pip 版,回退外壳注入二进制)。

⚠️ 本模块按"内部 SDK"对待:变更影响所有使用方插件,破坏性变更需同步更新并重新打包
(verify-manifests 会校验打包一致性)。
"""

import os
import subprocess as _subprocess
import sys as _sys

# progress/dep 来自 _protocol(首次创建 venv 报进度、二进制回退取外壳注入路径)。
# 双路径导入与各插件 CLI 一致:生产(打包副本)/开发(shared 源码)均可用,
# 测试加载本模块也不因缺 _protocol 挂掉。
try:
    from _protocol import progress, dep  # noqa: F401
except ImportError:
    import sys as _sys0
    _sys0.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from _protocol import progress, dep  # noqa: F401


# --cookies-from-browser 支持的浏览器名
BROWSER_NAMES = {
    "chrome", "chromium", "edge", "firefox", "brave",
    "vivaldi", "opera", "safari", "whale",
}


def cookies_args(cookies):
    """把 --cookies 参数转为 yt-dlp 参数。

    cookies 取值:
      - 浏览器名(chrome/edge/firefox/brave/...) → --cookies-from-browser
      - 文件路径 → --cookies <file>
      - 空/未提供 → 不加参数
    """
    if not cookies or cookies is True or str(cookies).strip() == "":
        return []
    c = str(cookies).strip()
    if c in BROWSER_NAMES:
        return ["--cookies-from-browser", c]
    if os.path.isfile(c) or c.endswith((".txt", ".cookies")):
        return ["--cookies", c]
    # 未知值按浏览器名透传(yt-dlp 会给出明确错误提示)
    return ["--cookies-from-browser", c]


def playlist_args(playlist):
    """--playlist 为真值(1/yes/true/on 或 flag True)时返回 --yes-playlist,支持下载整个播放列表/频道。

    注意:parse_args 对无值 flag(--playlist)返回 True,True 同样表达"下载整个列表"的
    用户意图,必须返回 --yes-playlist(此前误将 True 当"未提供",手动 CLI 调用失效)。
    """
    if not playlist:
        return []
    if playlist is True:
        return ["--yes-playlist"]
    p = str(playlist).strip().lower()
    return ["--yes-playlist"] if p in ("1", "yes", "true", "on") else []


# 常见 yt-dlp 错误 → (语义化代码, 可读原因 + 原始错误)
# 设计:精确分类,避免把"网络挂起/412 风控"误归为"需要 cookies";
# 末尾附加 yt-dlp 原始错误行(截断),方便用户自诊与排查。
def diagnose_ytdlp_error(err_lines, last_lines, code):
    """根据 yt-dlp stderr/最后行推断失败原因,返回 (code, message)。

    code 枚举: COOKIES_PERMISSION / NEED_LOGIN / JS_RUNTIME_MISSING / DRM_PROTECTED
             / PRIVATE_VIDEO / MEMBERS_ONLY / AGE_RESTRICTED / GEO_BLOCKED
             / RATE_LIMITED_412 / API_BLOCKED / HTTP_403 / HTTP_404
             / NO_FORMATS_AVAILABLE / NETWORK_ERROR / UNSUPPORTED_URL
             / FFMPEG_ERROR / UNKNOWN_ERROR
    """
    raw = " ".join((err_lines or []) + (last_lines or []))
    raw_lower = raw.lower()

    # 抓取 yt-dlp 原始错误最后一行(去 [download] 前缀),供用户自诊。
    # 过滤 yt-dlp 开发者样板文案(please report this issue / Confirm you are on
    # the latest version 等)——对用户无意义,且会掩盖真实错误行。
    _YTDLP_JUNK = ("please report this issue", "confirm you are on the latest version",
                   "filling out the appropriate issue template")
    detail = ""
    for line in reversed(last_lines or []):
        line = line.strip()
        if not line or line.startswith("[download]"):
            continue
        if any(j in line.lower() for j in _YTDLP_JUNK):
            continue
        detail = line[-200:]
        break
    suffix = f" ｜ 详情: {detail}" if detail else ""

    # 0. 浏览器 cookies 读取权限(macOS 沙盒:Safari 需「完全磁盘访问」授权)。
    #    必须放在登录态判断之前——这是 cookies 读取本身失败,比"需要登录"更具体。
    if "operation not permitted" in raw_lower or "errno 1" in raw_lower:
        return ("COOKIES_PERMISSION",
                f"无法读取浏览器 cookies(macOS 沙盒权限限制)。请打开「系统设置 → 隐私与安全性 → 完全磁盘访问权限」,"
                f"勾选 PlugKit(dev 模式为你的终端应用)后重启重试;或改用 Chrome/Edge/Firefox 等无需授权的浏览器。{suffix} (yt-dlp 退出码 {code})")

    # 1. 登录态:仅匹配明确登录指示(避免与 cookie 文件读取失败/网络错误混淆)。
    if any(k in raw_lower for k in [
        "sign in to confirm", "confirm you're not a bot", "confirm you are not a bot",
        "fresh cookies", "logged in", "login required", "use --cookies",
        "use --username", "please log in",
    ]):
        return ("NEED_LOGIN",
                f"需要登录或有效 cookies(平台反爬)。建议:在界面「登录态」选择已登录该站的浏览器,或稍后重试。{suffix} (yt-dlp 退出码 {code})")

    # 2. JS 运行时(deno 未安装)
    if "js runtime" in raw_lower or "javascript runtime" in raw_lower:
        return ("JS_RUNTIME_MISSING",
                f"该站点需要 JS 运行时(deno)才能解析,当前未安装。{suffix} (yt-dlp 退出码 {code})")

    # 3. DRM
    if "drm" in raw_lower or "widevine" in raw_lower or "encrypted and not yet" in raw_lower:
        return ("DRM_PROTECTED",
                f"该视频受 DRM 加密保护,无法下载。{suffix} (yt-dlp 退出码 {code})")

    # 4. 412 风控(优先级高于 403,避免误归为 cookies 提示)
    if "412" in raw_lower or "blocked by server" in raw_lower or "request is blocked" in raw_lower:
        return ("RATE_LIMITED_412",
                f"服务端主动拒绝(412,风控/限流)。通常过会儿可恢复,或换网络/用登录 cookies 后再试。{suffix} (yt-dlp 退出码 {code})")
    # 4.5 B 站 API 业务风控(code:-403 等):虽然 HTTP 200,但 API 业务码拒绝
    if ('"code":-403' in raw_lower or '"code":-101' in raw_lower or '"code":-352' in raw_lower
            or "访问权限不足" in raw):
        return ("API_BLOCKED",
                f"站点 API 业务风控(如 B 站 code:-403/-101 等)。通常意味着 IP 被风控或需要登录 cookies。{suffix} (yt-dlp 退出码 {code})")

    # 5. 私密/会员/年龄(细分)
    if "members only" in raw_lower:
        return ("MEMBERS_ONLY",
                f"该视频为会员专属,需要订阅会员 cookies。{suffix} (yt-dlp 退出码 {code})")
    if "age" in raw_lower and ("restrict" in raw_lower or "limit" in raw_lower):
        return ("AGE_RESTRICTED",
                f"该视频有年龄限制,需要登录相应账号。{suffix} (yt-dlp 退出码 {code})")
    if "this video is private" in raw_lower or "private video" in raw_lower or "video is private" in raw_lower:
        return ("PRIVATE_VIDEO",
                f"该视频为私密/不可访问。{suffix} (yt-dlp 退出码 {code})")

    # 6. 地区限制
    if "geo" in raw_lower or "not available in your country" in raw_lower:
        return ("GEO_BLOCKED",
                f"该视频在你的地区不可用(地区限制)。{suffix} (yt-dlp 退出码 {code})")

    # 7. 无可用格式(直播结束/格式变化)
    if any(k in raw_lower for k in [
        "requested format not available", "no video formats", "no format",
        "unable to download video", "no media could be extracted",
    ]):
        return ("NO_FORMATS_AVAILABLE",
                f"没有可用的视频/音频格式(可能是直播结束、源站格式变更或链接指向图片/字幕而非视频)。{suffix} (yt-dlp 退出码 {code})")

    # 8. 403(与 412 严格区分)
    if "http error 403" in raw_lower or "403: forbidden" in raw_lower:
        return ("HTTP_403",
                f"平台拒绝访问(403,反爬)。建议:用登录 cookies 重试,或换网络。{suffix} (yt-dlp 退出码 {code})")

    # 9. 404
    if "http error 404" in raw_lower or "404: not found" in raw_lower:
        return ("HTTP_404",
                f"资源不存在或链接失效(404)。请检查链接是否有效/已被删除。{suffix} (yt-dlp 退出码 {code})")

    # 10. 网络/超时/SSL/DNS(收紧关键字,不与 cookies 命中冲突)
    if any(k in raw_lower for k in [
        "timed out", "read timed out", "connection aborted", "connection refused",
        "connection reset", "network is unreachable", "ssl:", "certificate verify failed",
        "name resolution", "temporary failure in name resolution", "no route to host",
        "bad gateway", "502",
    ]):
        return ("NETWORK_ERROR",
                f"网络连接失败/超时/SSL/DNS 异常。请检查网络后重试,或换网络/代理。{suffix} (yt-dlp 退出码 {code})")

    # 11. 不支持的链接
    if "unsupported url" in raw_lower:
        return ("UNSUPPORTED_URL",
                f"不支持的链接或站点(yt-dlp 未内置该站解析器)。{suffix} (yt-dlp 退出码 {code})")

    # 12. ffmpeg 后处理
    if "ffmpeg" in raw_lower or "postprocessing" in raw_lower or "merging" in raw_lower:
        return ("FFMPEG_ERROR",
                f"合并/后处理失败(可能需要 ffmpeg 或磁盘空间不足)。{suffix} (yt-dlp 退出码 {code})")

    # 兜底:返回原始错误,用户可自诊
    return ("UNKNOWN_ERROR",
            f"未知错误 (yt-dlp 退出码 {code}){suffix}")


# ---------------------------------------------------------------- yt-dlp 可执行解析
# 下载/解析统一使用共享 venv(pip 装 yt-dlp 包):pip 版冷启动 ~0.4s vs
# PyInstaller 单文件二进制每次解包 ~22s(实测本机 22s/0.4s)。
# venv 缺失(全新机器/Windows)时自动创建(首次 ~30s,带进度提示);失败回退二进制。

# 下载/解析统一使用的 yt-dlp 版本(pinned,与 src-tauri/src/dep_manifest.rs 的
# YTDLP_VERSION 保持同步——两处手工对齐,改版本必须同时改)。
YTDLP_VERSION = "2026.08.19"

VENV_DIR = os.path.expanduser("~/.plugkit/venvs/ytdlp")
VENV_PY = os.path.join(VENV_DIR, "bin", "python")
# 国内默认清华源;境外/自建源可用环境变量 PLUGKIT_PIP_MIRROR 覆盖
PIP_MIRROR = os.environ.get("PLUGKIT_PIP_MIRROR", "https://pypi.tuna.tsinghua.edu.cn/simple")


def _venv_matches_pinned():
    """校验 venv 内已装 yt-dlp 版本与 pinned 版本一致(pip show)。
    版本不符(旧版本残留/被污染)→ 返回 False,由 _ensure_ytdlp_venv 重装。"""
    try:
        proc = _subprocess.run([VENV_PY, "-m", "pip", "show", "yt-dlp"],
                               capture_output=True, text=True, timeout=30)
        if proc.returncode != 0:
            return False
        for line in proc.stdout.splitlines():
            if line.lower().startswith("version:"):
                return line.split(":", 1)[1].strip() == YTDLP_VERSION
        return False
    except Exception:
        return False


def _ensure_ytdlp_venv():
    """确保 venv 可用且 yt-dlp 为 pinned 版本:已就绪 → True;
    venv 缺失则创建 + 安装,版本不符则直接重装(首次 ~30s / 重装 ~10s,
    期间向任务中心报进度,避免用户误判卡死)。失败静默返回 False,由调用方回退。"""
    if os.path.exists(VENV_PY) and _venv_matches_pinned():
        return True
    try:
        progress(0, message="首次使用:正在准备解析环境(约 30 秒)…")
    except Exception:
        pass
    try:
        if not os.path.exists(VENV_PY):
            _subprocess.run([_sys.executable, "-m", "venv", VENV_DIR],
                            capture_output=True, timeout=120)
            if not os.path.exists(VENV_PY):
                return False
        # 版本不符或全新安装:重装 pinned 版本(与 YTDLP_VERSION 对齐,
        # 升级 yt-dlp 后旧 venv 版本不匹配会被自动重装,消除版本漂移)
        _subprocess.run([VENV_PY, "-m", "pip", "install", "-q", "-i", PIP_MIRROR,
                         f"yt-dlp=={YTDLP_VERSION}"],
                        capture_output=True, timeout=300)
    except Exception:
        return False
    return os.path.exists(VENV_PY) and _venv_matches_pinned()


def ytdlp_binary():
    """返回 yt-dlp 可执行路径:优先 venv pip 版(冷启动 0.4s);venv 缺失时尝试
    自动创建(一次性 ~30s);创建失败/Windows 回退外壳注入的缓存二进制
    (dep("yt-dlp"))——功能不回归。"""
    venv_exe = os.path.join(VENV_DIR, "bin", "yt-dlp")
    if os.name == "posix":
        if os.path.isfile(venv_exe):
            return venv_exe
        if _ensure_ytdlp_venv() and os.path.isfile(venv_exe):
            return venv_exe
    return dep("yt-dlp")
