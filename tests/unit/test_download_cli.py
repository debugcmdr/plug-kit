"""download 插件 CLI 单元测试(纯逻辑,不依赖真实 yt-dlp)。

覆盖(E-2):
- _build_info_result:视频格式排序/清晰度推断/音频 ext/best_height
- guess_height_from_url:URL 文件名清晰度推断
- looks_like_url:链接粗判
- import_file:txt 链接导入去重
"""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CLI_PATH = os.path.join(ROOT, "plugins", "download", "tool", "plugkit-download")

_loader = importlib.machinery.SourceFileLoader("plugkit_download_cli", CLI_PATH)
_spec = importlib.util.spec_from_loader(_loader.name, _loader)
cli = importlib.util.module_from_spec(_spec)
_loader.exec_module(cli)


def run_and_capture(fn):
    """执行输出单行 result JSON 的函数并解析(CLI 协议: stdout 单行 JSON)。"""
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn()
    return json.loads(buf.getvalue())


def test_build_info_result_formats_sorted():
    meta = {
        "title": "测试视频",
        "formats": [
            {"format_id": "f1", "height": 480, "ext": "mp4", "vcodec": "avc1",
             "acodec": "mp4a", "format_note": "480p"},
            {"format_id": "f2", "height": 1080, "ext": "mp4", "vcodec": "avc1",
             "acodec": "mp4a", "format_note": "1080p"},
            # 站点未提供结构化高度 → 从下载 URL 推断 720
            {"format_id": "f3", "height": None, "ext": "mp4", "vcodec": "avc1",
             "acodec": "none", "url": "https://x/402946-720p.mp4"},
            # 纯音频格式(不进 video_formats,用于 audio_ext)
            {"format_id": "audio1", "height": None, "ext": "m4a",
             "vcodec": "none", "acodec": "mp4a"},
        ],
    }
    data = run_and_capture(lambda: cli._build_info_result(meta))["data"]
    assert data["title"] == "测试视频"
    # 视频格式按清晰度降序;未知高度从 URL 推断 720 参与排序(1080 > 720 > 480)
    assert [f["height"] for f in data["video_formats"]] == [1080, 720, 480]
    assert data["audio_ext"] == "m4a"
    assert data["best_height"] == 1080
    assert data["is_playlist"] is False
    assert data["has_subtitles"] is False


def test_build_info_result_playlist_detection():
    meta = {"title": "L", "playlist_count": 10, "formats": []}
    data = run_and_capture(lambda: cli._build_info_result(meta))["data"]
    assert data["is_playlist"] is True
    assert data["playlist_count"] == 10


def test_guess_height_from_url():
    assert cli.guess_height_from_url("https://x/402946-720p.mp4") == 720
    assert cli.guess_height_from_url("https://x/_1080p/stream") == 1080
    assert cli.guess_height_from_url("https://x/video.mp4") is None
    assert cli.guess_height_from_url("") is None


def test_looks_like_url():
    assert cli.looks_like_url("https://www.bilibili.com/video/BV1xx411c7mD")
    assert cli.looks_like_url("http://a.example/x")
    assert not cli.looks_like_url("plain text")
    assert not cli.looks_like_url("")
    assert not cli.looks_like_url("https://a")  # 长度 < 10 不视为链接


def test_import_file_txt_dedup(tmp_path):
    f = tmp_path / "links.txt"
    f.write_text(
        "https://a.example/v1\nnot a url\nhttps://b.example/v2\nhttps://b.example/v2\n",
        encoding="utf-8",
    )
    data = run_and_capture(lambda: cli.import_file(str(f)))["data"]
    assert data["count"] == 2  # 去重保序
    assert data["links"] == ["https://a.example/v1", "https://b.example/v2"]


def test_import_file_missing(tmp_path):
    data = run_and_capture(lambda: cli.import_file(str(tmp_path / "nope.txt")))
    assert data["type"] == "error"
    assert data["code"] == "FILE_NOT_FOUND"


def test_import_file_unsupported_ext(tmp_path):
    f = tmp_path / "x.md"
    f.write_text("hi", encoding="utf-8")
    data = run_and_capture(lambda: cli.import_file(str(f)))
    assert data["type"] == "error"
    assert data["code"] == "UNSUPPORTED_EXT"
