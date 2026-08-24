"""PlugKit yt-dlp 共享逻辑模块(_ytdlp)。

由 plugins/shared/ 在打包/同步时复制进 download/playlist 的 tool/ 目录。
包含:cookies 参数解析、播放列表参数、yt-dlp 错误诊断分类(三级报错链路的诊断层)。

⚠️ 本模块按"内部 SDK"对待:变更影响所有使用方插件,破坏性变更需同步更新并重新打包
(verify-manifests 会校验打包一致性)。
"""

import os


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
    """--playlist 为真值(1/yes/true)时返回 --yes-playlist,支持下载整个播放列表/频道。"""
    if not playlist or playlist is True:
        return []
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
    detail = ""
    for line in reversed(last_lines or []):
        if line and not line.startswith("[download]"):
            detail = line.strip()[-200:]
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
