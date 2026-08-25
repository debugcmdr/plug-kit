"""convert 插件 CLI 单元测试(纯逻辑,不依赖真实 ffmpeg)。

覆盖:
- parse_args 各形态解析
- _reserve_path 原子占位(含并发场景)与 resolve_output 同路径安全化
- 画质档位映射(high/medium/low → jpg q:v / webp quality / avif·heic crf)+ UI 对齐
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
                q = re.search(r"hasQuality:\s*true", fm.group(1))
                if v:
                    items.append({"v": v.group(1), "codec": c.group(1) if c else None,
                                  "has_quality": bool(q)})
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


# ---------------------------------------------------------------- 画质档位映射
def test_quality_maps():
    """画质档位统一为语义值 high/medium/low,各格式各自映射编码器参数
    (jpg -q:v 反向值域、webp -quality 正向、avif/heic -crf 反向,互不通用)。"""
    assert JPG_Q_MAP == {"high": "2", "medium": "5", "low": "8"}
    assert cli.WEBP_Q_MAP == {"high": "90", "medium": "75", "low": "60"}
    assert cli.AVIF_CRF_MAP == {"high": "26", "medium": "30", "low": "36"}
    assert cli.HEIC_CRF_MAP == {"high": "24", "medium": "28", "low": "33"}


def test_quality_maps_have_same_keys():
    """四个映射表档位键一致(UI 三档在全部有损格式均有效)。"""
    keys = set(cli.JPG_Q_MAP)
    for m in (cli.WEBP_Q_MAP, cli.AVIF_CRF_MAP, cli.HEIC_CRF_MAP):
        assert set(m) == keys, f"档位键不一致: {set(m)} != {keys}"


def test_ui_hasquality_aligns_with_cli_maps():
    """UI 标记 hasQuality 的有损格式 ↔ CLI 有对应映射,防「UI 加标记但 CLI 忘映射」
    (曾出现 webp 直接透传语义值 high 的隐患)。png/bmp/gif/tiff 无损语义不得误标。"""
    ui_has_q = {f["v"] for f in parse_ui()["formats"]["image"] if f["has_quality"]}
    cli_maps = {"jpg": cli.JPG_Q_MAP, "webp": cli.WEBP_Q_MAP,
                "avif": cli.AVIF_CRF_MAP, "heic": cli.HEIC_CRF_MAP}
    assert ui_has_q == set(cli_maps), f"UI hasQuality {sorted(ui_has_q)} != CLI 映射 {sorted(cli_maps)}"


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
        # 视频输出格式必须全部由视频转换分支覆盖——音频提取已独立为
        # 「音频输出」选项卡(optAudioExtract),不允许再混入输出格式(防回归)。
        assert ext in video_handled, f"video 输出 {f['v']} 无 CLI 视频分发"
    for f in ui["formats"]["audio"]:
        assert f".{f['v']}" in AUDIO_EXTS, f"audio 输出 {f['v']} 无 CLI 分发"
    for f in ui["formats"]["image"]:
        assert f".{f['v']}" in IMAGE_EXTS, f"image 输出 {f['v']} 无 CLI 分发"


def test_video_formats_no_audio_extract_entries():
    """视频输出格式不得混入音频扩展名(提取音频已独立为第二个输出选项卡)。"""
    ui = parse_ui()
    for f in ui["formats"]["video"]:
        assert f".{f['v']}" not in AUDIO_EXTS, f"video 输出选项混入音频格式 {f['v']}"


def test_extract_audio_options_aligned_with_cli():
    """UI「音频提取」下拉取值必须被 CLI 支持:
    copy=原样无损提取(CLI 识别 .copy 输出,探测源音轨后 -c:a copy);
    mp3/m4a/wav/flac=重编码(走 AUDIO_EXTS 音频转换)。copy 为默认(第一项)。"""
    src = open(INDEX_HTML, encoding="utf-8").read()
    m = re.search(r'id="audioExtract"(.*?)</select>', src, re.S)
    assert m, "index.html 缺少 audioExtract 下拉框"
    values = re.findall(r'value="([^"]*)"', m.group(1))
    assert values == ["copy", "mp3", "m4a", "wav", "flac"], f"audioExtract 选项漂移: {values}"
    for v in values[1:]:
        assert f".{v}" in AUDIO_EXTS, f"audioExtract 取值 {v} 不在 CLI 音频格式集"


def test_extract_audio_copy_mapping():
    """原样提取的音轨编码→扩展名映射:常见音轨都能映射到可无损封装的容器。"""
    for codec, ext in (("aac", "m4a"), ("alac", "m4a"), ("mp3", "mp3"), ("opus", "opus"),
                       ("flac", "flac"), ("vorbis", "ogg"), ("pcm_s16le", "wav"),
                       ("pcm_s24le", "wav"), ("ac3", "ac3"), ("eac3", "eac3"),
                       ("wmav2", "wma"), ("aiff", "aiff")):
        assert cli.AUDIO_CODEC_EXT[codec] == ext, f"映射错误: {codec} → {ext}"


def test_audio_copy_ext_unknown_codec_returns_none():
    """C-1 回归:未知音轨编码(蓝光常见的 dts/truehd 等)必须返回 None 而非崩溃。
    extract_audio_copy 用 `audio_copy_ext(codec) or 'wav'` 降级,绝不允许 None 拼接路径。"""
    assert cli.audio_copy_ext("aac") == "m4a"
    for codec in ("dts", "truehd", "mlp", "pcm_u8", "pcm_s16be", "mp2", "smv", None, ""):
        assert cli.audio_copy_ext(codec) is None, f"未知编码 {codec} 应返回 None"


def test_probe_reports_encoders():
    """probe 必须报告 has_ffmpeg + 编码器字典(UI 据此禁用不可用格式)。"""
    import io
    import json
    import contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        cli.probe()
    data = json.loads(buf.getvalue())
    assert data["type"] == "result" and data["success"] is True
    d = data["data"]
    assert "has_ffmpeg" in d and isinstance(d["has_ffmpeg"], bool)
    assert "encoders" in d and isinstance(d["encoders"], dict)
    for enc in ("libx264", "libx265", "libopus", "libwebp", "libvpx-vp9"):
        assert enc in d["encoders"], f"probe 缺编码器探测: {enc}"
    assert "copy" in d["extractable_audio"], "原样提取(copy)应列入 extractable_audio"
    assert "flac" in d["extractable_audio"], "重编码提取应含 flac(无损,UI 下拉可选项)"


def test_video_split_layout_present():
    """视频模块左右分栏结构必须存在(左=视频转换,右=音频提取,两个独立按钮);
    gif 参数/清空/拖放等新增能力元素必须存在。"""
    src = open(INDEX_HTML, encoding="utf-8").read()
    for token in ('id="convertSplit"', 'id="formatVideo"', 'id="btnConvertVideo"',
                  'id="btnExtractAudio"', 'function convertVideos', 'function extractAudios',
                  'id="gifOpts"', 'id="gifFps"', 'id="gifWidth"', 'id="btnClear"',
                  'MT.onFilesDropped', "MT.invoke('probe')"):
        assert token in src, f"index.html 缺少 {token}"
    # 音频提取输出音频扩展名,由 CLI 按扩展名分发——不再依赖 --extract-audio 参数
    assert "--extract-audio" not in src, "UI 不应再引用 --extract-audio"


def test_build_audio_cmd():
    """音频提取/转换命令构造:必须 -vn(丢弃视频流)+ 目标 codec + 保留元数据。
    wav 不再强制 -ar 降采样(保留源采样率,无损语义);-map_metadata 0 在输出前。"""
    assert cli.build_audio_cmd("in.mp4", "out.mp3") == [
        cli.FFMPEG, "-y", "-i", "in.mp4", "-vn", "-acodec", "libmp3lame",
        "-b:a", "192k", "-map_metadata", "0", "out.mp3"]
    assert cli.build_audio_cmd("in.avi", "out.wav") == [
        cli.FFMPEG, "-y", "-i", "in.avi", "-vn", "-acodec", "pcm_s16le",
        "-map_metadata", "0", "out.wav"]
    assert cli.build_audio_cmd("in.mkv", "out.m4a") == [
        cli.FFMPEG, "-y", "-i", "in.mkv", "-vn", "-acodec", "aac",
        "-b:a", "192k", "-map_metadata", "0", "out.m4a"]
    # 显式码率覆盖默认
    assert cli.build_audio_cmd("in.mov", "out.mp3", "320k") == [
        cli.FFMPEG, "-y", "-i", "in.mov", "-vn", "-acodec", "libmp3lame",
        "-b:a", "320k", "-map_metadata", "0", "out.mp3"]


def test_ui_codec_whitelist():
    ui = parse_ui()
    for cat, items in ui["formats"].items():
        for f in items:
            if f["codec"]:
                assert f["codec"] in ("hevc",), f"{cat}/{f['v']} codec 非白名单: {f['codec']}"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
