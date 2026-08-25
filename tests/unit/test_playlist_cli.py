"""playlist 插件 CLI 单元测试(纯逻辑,不依赖真实 yt-dlp)。

覆盖(E-2):
- _build_list_result:条目过滤(嵌套列表/None)、原始下标保留、无 url 条目兜底
  (回归:此前引用未定义变量 url,条目无 url 时 NameError)
- _unique_path:导出防覆盖加序号
"""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CLI_PATH = os.path.join(ROOT, "plugins", "playlist", "tool", "plugkit-playlist")

_loader = importlib.machinery.SourceFileLoader("plugkit_playlist_cli", CLI_PATH)
_spec = importlib.util.spec_from_loader(_loader.name, _loader)
cli = importlib.util.module_from_spec(_spec)
_loader.exec_module(cli)


def run_and_capture(fn):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn()
    return json.loads(buf.getvalue())


def test_build_list_result_filters_and_index():
    meta = {
        "title": "我的列表",
        "entries": [
            {"_type": "url", "id": "v1", "title": "第一集", "duration": 60,
             "url": "https://a/v1"},
            {"_type": "playlist", "title": "嵌套列表"},  # 嵌套播放列表跳过
            None,                                          # 空条目跳过
            {"id": "v2", "title": "第二集", "duration": 120},  # 无 url → 兜底
        ],
    }
    data = run_and_capture(lambda: cli._build_list_result(meta))["data"]
    assert data["count"] == 2
    assert data["playlist_title"] == "我的列表"
    assert data["entries"][0]["index"] == 1
    assert data["entries"][1]["index"] == 4  # 保留原始下标(供 --playlist-items 使用)


def test_build_list_result_missing_url_no_crash():
    """回归:条目缺 url/webpage_url 时不得 NameError(原实现引用未定义变量 url)。"""
    meta = {"entries": [{"id": "v1", "title": "无链接条目"}]}
    data = run_and_capture(lambda: cli._build_list_result(meta))["data"]
    assert data["count"] == 1
    assert data["entries"][0]["url"] == ""  # 兜底空串,UI 不会渲染可点击链接


def test_build_list_result_empty():
    data = run_and_capture(lambda: cli._build_list_result({}))["data"]
    assert data["count"] == 0
    assert data["entries"] == []


def test_unique_path_avoids_overwrite(tmp_path):
    existing = tmp_path / "x.txt"
    existing.write_text("old", encoding="utf-8")
    assert cli._unique_path(str(existing)) == str(tmp_path / "x (1).txt")
    assert cli._unique_path(str(tmp_path / "fresh.txt")) == str(tmp_path / "fresh.txt")
