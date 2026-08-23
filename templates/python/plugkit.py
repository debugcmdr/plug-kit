#!/usr/bin/env python3
"""plugkit - PlugKit 插件开发辅助库(Python 模板)

封装 stdout JSON 协议,插件作者只需实现业务逻辑。
用法:继承 Plugin 并实现命令处理器,或直接用 @command 装饰器。
"""
import json
import os
import sys


def progress(percent, speed=None, eta=None, message=None, file_name=None, output_path=None):
    """输出一条进度消息。percent 允许 None(无百分比任务)。
    file_name/output_path 可选但建议携带:任务中心据此展示处理文件名
    与「打开输出文件夹」入口。"""
    line = {"type": "progress", "percent": percent}
    if speed is not None:
        line["speed"] = speed
    if eta is not None:
        line["eta"] = eta
    if message is not None:
        line["message"] = message
    if file_name is not None:
        line["file_name"] = file_name
    if output_path is not None:
        line["output_path"] = output_path
    print(json.dumps(line, ensure_ascii=False), flush=True)


def result(data):
    """输出成功结果(任务结束)。"""
    print(json.dumps({"type": "result", "success": True, "data": data}, ensure_ascii=False), flush=True)


def error(code, message):
    """输出错误(任务结束)。"""
    print(json.dumps({"type": "error", "code": code, "message": message}, ensure_ascii=False), flush=True)


def dep(name):
    """获取共享依赖路径(优先 PLUGKIT_* 环境变量,回退系统 PATH)。"""
    key = f"PLUGKIT_{name.upper().replace('-', '_')}"
    if key in os.environ:
        return os.environ[key]
    return name  # fallback: assume on PATH


class Plugin:
    """基类:子类实现 handle(cmd, args) 即可。

    示例:
        class MyPlugin(Plugin):
            def handle(self, cmd, args):
                if cmd == "echo":
                    result({"echo": args.get("text")})
    """

    def handle(self, cmd, args):
        raise NotImplementedError

    def run(self):
        cmd = sys.argv[1] if len(sys.argv) > 1 else None
        args = self._parse_args(sys.argv[2:])

        # 支持 --command k=v 形式
        if not cmd and "--command" in args:
            cmd = args.pop("command")

        try:
            self.handle(cmd, args)
        except Exception as e:
            error("PLUGIN_ERROR", str(e))
            sys.exit(1)

    def _parse_args(self, argv):
        args = {}
        i = 0
        while i < len(argv):
            a = argv[i]
            if a.startswith("--"):
                key = a[2:]
                if i + 1 < len(argv) and not argv[i + 1].startswith("--"):
                    args[key] = argv[i + 1]
                    i += 2
                else:
                    args[key] = True
                    i += 1
            else:
                i += 1
        return args
