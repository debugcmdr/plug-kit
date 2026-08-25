#!/usr/bin/env python3
"""常驻 yt-dlp 解析服务(解析加速方案 1+2)。

背景:yt-dlp 二进制(PyInstaller 单文件)每次冷启动约 12s(Python 解释器 + 1744 个
extractor 加载),单视频/列表解析体验差。本服务用「pip 安装的 yt-dlp 包」常驻,
冷启动只付一次,后续请求走进程内 YoutubeDL API(秒级),并内置元数据缓存。

能力:
- Unix socket 收 JSON 行请求,进程内提取,JSON 行响应(单连接单请求)
- 元数据缓存 URL→结果(TTL 10 分钟):同一链接重复解析秒回
- 空闲 120s 无请求自动退出(释放约 250MB 内存)
- 独立会话(setsid),不被任务取消/超时的进程树清理误杀

协议(JSON 行):
  请求  {"id": 1, "cmd": "extract", "url": "...", "cookies": "", "mode": "info"|"flat"}
  成功  {"id": 1, "ok": true, "data": {yt-dlp extract_info 结果}}
  失败  {"id": 1, "ok": false, "code": "EXTRACT_FAILED", "message": "..."}

平台:仅 Unix(macOS/Linux)启用;Windows 无 Unix socket,客户端自动降级直连。
用法: <venv-python> _ytdlp_service.py <socket_path>
"""
import json
import os
import socket
import sys
import threading
import time

SOCK_PATH = sys.argv[1] if len(sys.argv) > 1 else "/tmp/plugkit-ytdlp.sock"
CACHE_TTL = 600        # 元数据缓存 10 分钟
IDLE_TIMEOUT = 120     # 空闲 120s 自动退出
SOCKET_TIMEOUT = 20    # 与二进制路径一致的网络读超时
RETRIES = 5
RETRY_SLEEP = 2

# 预加载 yt-dlp:冷启动(~10s)只付这一次,之后请求全部复用进程内实例。
import yt_dlp  # noqa: E402

# 错误诊断与直连路径共用(412→RATE_LIMITED_412、超时→NETWORK_ERROR 等,错误码链路一致)。
from _ytdlp import diagnose_ytdlp_error, BROWSER_NAMES  # noqa: E402

_cache = {}  # key -> (data, ts)
_lock = threading.Lock()
_last_activity = time.time()


def _cache_get(key):
    with _lock:
        hit = _cache.get(key)
        if hit and time.time() - hit[1] < CACHE_TTL:
            return hit[0]
        return None


def _cache_put(key, data):
    with _lock:
        _cache[key] = (data, time.time())
        # 简单防膨胀:超过 64 条清最旧(解析场景 URL 量小,防御性上限)
        if len(_cache) > 64:
            oldest = min(_cache, key=lambda k: _cache[k][1])
            _cache.pop(oldest, None)


def extract(url, cookies="", mode="info"):
    """进程内调用 yt-dlp 提取。mode=info 单资源;mode=flat 播放列表扁平(秒级)。"""
    key = f"{mode}|{cookies}|{url}"
    cached = _cache_get(key)
    if cached is not None:
        return {"cached": True, "data": cached}
    opts = {
        "skip_download": True,
        "quiet": True,
        "no_warnings": True,
        "socket_timeout": SOCKET_TIMEOUT,
        "retries": RETRIES,
        "retry_sleep": RETRY_SLEEP,
    }
    if mode == "flat":
        opts["extract_flat"] = True
    if cookies:
        # 与 _ytdlp.cookies_args 语义一致:浏览器名 → --cookies-from-browser;
        # 文件路径(.txt/.cookies 或真实存在) → --cookies <file>。
        c = cookies.strip()
        if c in BROWSER_NAMES:
            opts["cookiesfrombrowser"] = (c, None)
        else:
            opts["cookiefile"] = c
    with yt_dlp.YoutubeDL(opts) as ydl:
        info = ydl.extract_info(url, download=False)
    _cache_put(key, info)
    return {"cached": False, "data": info}


def handle(req):
    global _last_activity
    _last_activity = time.time()
    req_id = req.get("id")
    try:
        if req.get("cmd") == "extract":
            url = req.get("url") or ""
            if not url:
                return {"id": req_id, "ok": False, "code": "MISSING_URL",
                        "message": "缺少 url 参数"}
            try:
                r = extract(url, req.get("cookies") or "", req.get("mode") or "info")
                return {"id": req_id, "ok": True, "cached": r["cached"], "data": r["data"]}
            except Exception as e:
                # 与直连路径同诊断:412→RATE_LIMITED_412、超时→NETWORK_ERROR 等,
                # 错误码链路一致,UI/任务中心可按码识别与引导。
                msg = str(e)[-400:] or "yt-dlp 提取失败"
                code, full_msg = diagnose_ytdlp_error([msg], [msg], 1)
                return {"id": req_id, "ok": False, "code": code, "message": full_msg}
        return {"id": req_id, "ok": False, "code": "BAD_REQUEST",
                "message": f"未知命令: {req.get('cmd')}"}
    except Exception as e:
        return {"id": req_id, "ok": False, "code": "SERVICE_ERROR",
                "message": str(e)[-400:]}


def serve_conn(conn):
    with conn:
        f = conn.makefile("r", encoding="utf-8")
        for line in f:
            line = line.strip()
            if not line:
                break
            try:
                req = json.loads(line)
            except (json.JSONDecodeError, ValueError):
                conn.sendall((json.dumps({"ok": False, "code": "BAD_JSON",
                                          "message": "请求不是合法 JSON"}) + "\n").encode())
                break
            resp = handle(req)
            conn.sendall((json.dumps(resp, ensure_ascii=False) + "\n").encode())
            break  # 单连接单请求,响应后关闭


def idle_watchdog():
    while True:
        time.sleep(10)
        if time.time() - _last_activity > IDLE_TIMEOUT:
            os._exit(0)  # 空闲超时,释放内存


def main():
    try:
        os.unlink(SOCK_PATH)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(SOCK_PATH)
    os.chmod(SOCK_PATH, 0o600)  # 仅当前用户可连
    srv.listen(8)
    threading.Thread(target=idle_watchdog, daemon=True).start()
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=serve_conn, args=(conn,), daemon=True).start()


if __name__ == "__main__":
    main()
