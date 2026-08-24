"""convert 插件 CLI 单元测试(纯逻辑,不依赖真实 ffmpeg)。

覆盖:
- parse_args 各形态解析
- _reserve_path 原子占位(含并发场景)与 resolve_output 同路径安全化
- JPG_Q_MAP 质量映射
- CLI EXTS 白名单与 UI INPUT_EXTS 严格对齐(防漂移)
- UI FORMATS 每个输出值都被 CLI 输出分发覆盖 + codec 白名单
"""
import importlib.machinery
import importlib.util
import os
import re
import threading

import pytest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CLI_PATH = os.path.join(ROOT, "plugins", "convert", "tool", "plugkit-convert")
INDEX_HTML = os.path.join(ROOT, "plugins", "convert", "tool", "index.html")

_loader = importlib.machinery.SourceFileLoader("plugkit_convert_cli", CLI_PATH)
_spec = importlib.util.spec_from_loader(_loader.name, _loader)
cli = importlib.util.module_from_spec(_spec)
_loader.exec_module(cli)

parse_args = cli.parse_args
resolve_output = cli.resolve_output
_reserve_path = cli._reserve_path
JPG_Q_MAP = cli.JPG_Q_MAP
VIDEO_EXTS = cli.VIDEO_EXTS
AUDIO_EXTS = cli.AUDIO_EXTS
IMAGE_EXTS = cli.IMAGE_EXTS


# ---------------------------------------------------------------- UI 解析(仅测试用)
def parse_ui():
    src = open(INDEX_HTML, encoding="utf-8").read()

    def extract_obj(name):
        m = re.search(rf"const {name} = \{{(.*?)\}};", src, re.S)
        return m.group(1) if m else ""

    input_exts = {}
    body = extract_obj("INPUT_EXTS")
    for cat in ("video", "audio", "image"):
        m = re.search(rf"{cat}: \[(.*?)\]", body, re.S)
        input_exts[cat] = re.findall(r"'([^']+)'", m.group(1)) if m else []

    formats = {}
    body = extract_obj("FORMATS")
    for cat in ("video", "audio", "image"):
        m = re.search(rf"{cat}: \[(.*?)\]", body, re.S)
        items = []
        if m:
            for fm in re.finditer(r"\{(.*?)\}", m.group(1), re.S):
                v = re.search(r"v: '([^']+)'", fm.group(1))
                c = re.search(r"codec: '([^']+)'", fm.group(1))
                if v:
                    items.append({"v": v.group(1), "codec": c.group(1) if c else None})
        formats[cat] = items
    return {"input_exts": input_exts, "formats": formats}


# ---------------------------------------------------------------- parse_args
def test_parse_args_standard():
    a = parse_args(["--input", "a.mp4", "--output", "b.mp4", "--quality", "90"])
    assert a == {"input": "a.mp4", "output": "b.mp4", "quality": "90"}


def test_parse_args_equals_form():
    a = parse_args(["--quality=60", "--codec"])
    assert a["quality"] == "60"
    assert a["codec"] is True  # --codec 无值 → True


def test_parse_args_empty_value():
    # manifest 模板未提供参数时占位符被替换为空串(外壳行为),CLI 必须容错
    a = parse_args(["--audio-bitrate", "", "--codec", ""])
    assert a["audio_bitrate"] == ""
    assert a["codec"] == ""


def test_parse_args_ignores_non_flag():
    a = parse_args(["probe", "extra"])
    assert a == {}


# ---------------------------------------------------------------- 路径安全化
def test_reserve_path_creates_and_uniques(tmp_path):
    target = str(tmp_path / "a.mp4")
    p1 = _reserve_path(target)
    p2 = _reserve_path(target)
    assert p1 != p2
    assert os.path.exists(p1)
    assert os.path.exists(p2)
    assert p2.endswith("a (1).mp4")


def test_reserve_path_concurrent(tmp_path):
    """并发窗口内两个任务必须拿到不同路径(O_CREAT|O_EXCL 原子占位)。"""
    target = str(tmp_path / "b.mp4")
    results = []

    def worker():
        results.append(_reserve_path(target))

    threads = [threading.Thread(target=worker) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert len(results) == 4
    assert len(set(results)) == 4  # 无一重复


def test_resolve_output_same_path(tmp_path):
    src = str(tmp_path / "in.mp4")
    open(src, "w").close()
    out = resolve_output(src, src)
    assert out.endswith("_converted.mp4")


def test_resolve_output_existing_gets_suffix(tmp_path):
    out_file = tmp_path / "out.mp4"
    out_file.write_text("x")
    resolved = resolve_output(str(tmp_path / "in.mp4"), str(out_file))
    assert resolved.endswith("out (1).mp4")


# ---------------------------------------------------------------- JPG 质量映射
def test_jpg_q_map():
    assert JPG_Q_MAP == {"90": "2", "75": "5", "60": "8"}


# ---------------------------------------------------------------- 白名单一致性
def test_cli_exts_align_with_ui():
    ui = parse_ui()
    expected = {
        "video": {f".{e}" for e in ui["input_exts"]["video"]},
        "audio": {f".{e}" for e in ui["input_exts"]["audio"]},
        "image": {f".{e}" for e in ui["input_exts"]["image"]},
    }
    assert VIDEO_EXTS == expected["video"], "video 输入白名单与 UI 漂移"
    assert AUDIO_EXTS == expected["audio"], "audio 输入白名单与 UI 漂移"
    assert IMAGE_EXTS == expected["image"], "image 输入白名单与 UI 漂移"


def test_ui_format_values_covered_by_cli():
    """UI 每个输出选项(value=扩展名)都必须被 CLI 输出分发覆盖。"""
    ui = parse_ui()
    video_handled = {".mp4", ".mkv", ".mov", ".webm", ".avi", ".gif", ".m4v", ".ts", ".mpg", ".flv"}
    for f in ui["formats"]["video"]:
        ext = f".{f['v']}"
        # 视频类别:要么走音频提取(AUDIO_EXTS),要么走视频分支
        assert ext in AUDIO_EXTS or ext in video_handled, f"video 输出 {f['v']} 无 CLI 分发"
    for f in ui["formats"]["audio"]:
        assert f".{f['v']}" in AUDIO_EXTS, f"audio 输出 {f['v']} 无 CLI 分发"
    for f in ui["formats"]["image"]:
        assert f".{f['v']}" in IMAGE_EXTS, f"image 输出 {f['v']} 无 CLI 分发"


def test_ui_codec_whitelist():
    ui = parse_ui()
    for cat, items in ui["formats"].items():
        for f in items:
            if f["codec"]:
                assert f["codec"] in ("hevc",), f"{cat}/{f['v']} codec 非白名单: {f['codec']}"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
