"""shared 公共模块单元测试(_ytdlp.py)。

覆盖:
- playlist_args:True(flag 模式)/"1"/"yes"/空/其他 的真值语义(S-1 回归)
- cookies_args:浏览器名/文件路径/空 的参数生成
- diagnose_ytdlp_error:关键错误分类(登录态/412/403/网络/超时等),防分类漂移
"""
import importlib.machinery
import importlib.util
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
YTDLP_PATH = os.path.join(ROOT, "plugins", "shared", "_ytdlp.py")

_loader = importlib.machinery.SourceFileLoader("plugkit_shared_ytdlp", YTDLP_PATH)
_spec = importlib.util.spec_from_loader(_loader.name, _loader)
yt = importlib.util.module_from_spec(_spec)
_loader.exec_module(yt)


# ---------------------------------------------------------------- playlist_args
def test_playlist_args_flag_true_returns_yes():
    """S-1 回归:parse_args 对无值 flag(--playlist)返回 True,True 表达"下载整个列表"
    的用户意图,必须返回 --yes-playlist(此前误当"未提供"导致手动 CLI 调用失效)。"""
    assert yt.playlist_args(True) == ["--yes-playlist"]


def test_playlist_args_string_truthy():
    assert yt.playlist_args("1") == ["--yes-playlist"]
    assert yt.playlist_args("yes") == ["--yes-playlist"]
    assert yt.playlist_args("true") == ["--yes-playlist"]
    assert yt.playlist_args("on") == ["--yes-playlist"]
    assert yt.playlist_args(" TRUE ") == ["--yes-playlist"]  # 大小写+空白容错


def test_playlist_args_falsy():
    assert yt.playlist_args("") == []
    assert yt.playlist_args(None) == []
    assert yt.playlist_args(False) == []
    assert yt.playlist_args("0") == []
    assert yt.playlist_args("no") == []


# ---------------------------------------------------------------- cookies_args
def test_cookies_args_variants():
    assert yt.cookies_args("") == []
    assert yt.cookies_args(None) == []
    assert yt.cookies_args(True) == []  # flag 真值按"未提供"处理
    assert yt.cookies_args("chrome") == ["--cookies-from-browser", "chrome"]
    assert yt.cookies_args("edge") == ["--cookies-from-browser", "edge"]
    # 以 .txt/.cookies 结尾(不存在的路径)按文件透传,yt-dlp 会给出明确文件错误
    assert yt.cookies_args("/x/y/cookies.txt") == ["--cookies", "/x/y/cookies.txt"]


# ---------------------------------------------------------------- 错误诊断分类
def test_diagnose_network_timeout():
    code, _ = yt.diagnose_ytdlp_error([], ["ERROR: [youtube] ...: timed out"], 1)
    assert code == "NETWORK_ERROR"


def test_diagnose_need_login():
    code, _ = yt.diagnose_ytdlp_error([], ["ERROR: Sign in to confirm you're not a bot"], 1)
    assert code == "NEED_LOGIN"


def test_diagnose_rate_limited_412():
    code, _ = yt.diagnose_ytdlp_error([], ["ERROR: HTTP Error 412: Precondition Failed"], 1)
    assert code == "RATE_LIMITED_412"


def test_diagnose_http_403():
    code, _ = yt.diagnose_ytdlp_error([], ["ERROR: HTTP Error 403: Forbidden"], 1)
    assert code == "HTTP_403"


def test_diagnose_unsupported_url():
    code, _ = yt.diagnose_ytdlp_error([], ["ERROR: Unsupported URL: https://x.invalid/v"], 1)
    assert code == "UNSUPPORTED_URL"


def test_diagnose_ffmpeg_error():
    code, _ = yt.diagnose_ytdlp_error([], ["ERROR: Postprocessing: ffmpeg not found"], 1)
    assert code == "FFMPEG_ERROR"


def test_diagnose_unknown_fallback():
    code, msg = yt.diagnose_ytdlp_error([], ["ERROR: something totally unexpected"], 1)
    assert code == "UNKNOWN_ERROR"
    assert "退出码 1" in msg  # 兜底消息带退出码,用户可自诊
