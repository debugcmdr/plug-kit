"""PlugKit 插件公共协议模块(_protocol)。

由 plugins/shared/ 在打包/同步时复制进各插件 tool/ 目录(单点源码,自动分发)。
包含所有插件通用的:stdout 协议输出(progress/result/error)、环境变量依赖
解析(dep)、命令行参数解析(parse_args)。

⚠️ 本模块按"内部 SDK"对待:函数签名/行为变更影响所有使用方插件,
破坏性变更需同步更新所有使用方并重新打包(verify-manifests 会校验打包一致性)。
"""

import json
import os


def progress(percent, speed=None, eta=None, message=None, file_name=None, output_path=None):
    """输出进度行(progress+json 模式)。percent 允许 None;必须 flush=True,否则主程序读不到。"""
    line = {"type": "progress", "percent": percent}
    if speed:
        line["speed"] = speed
    if eta:
        line["eta"] = eta
    if message:
        line["message"] = message
    # 任务中心据此展示文件名与「打开输出文件夹」按钮
    if file_name:
        line["file_name"] = file_name
    if output_path:
        line["output_path"] = output_path
    print(json.dumps(line, ensure_ascii=False), flush=True)


def result(data):
    """输出任务成功结果行。任务结束最后一行必须是 result 或 error(主程序校验)。"""
    print(json.dumps({"type": "result", "success": True, "data": data}, ensure_ascii=False), flush=True)


def error(code, message):
    """输出任务错误行(错误码 + message;主程序按三级展示:插件 UI 简要/任务中心/日志页)。"""
    print(json.dumps({"type": "error", "code": code, "message": message}, ensure_ascii=False), flush=True)


def dep(name):
    """共享依赖真实路径:主程序经 PLUGKIT_{NAME} 环境变量注入,缺失回退系统 PATH。"""
    key = f"PLUGKIT_{name.upper().replace('-', '_')}"
    if key in os.environ:
        return os.environ[key]
    return name


def parse_args(argv):
    """命令行参数解析:--key value / --key=value / 无值 flag → True。
    键连字符转下划线(output-dir → output_dir);占位符未提供时可能为空串,须容错。"""
    args = {}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a.startswith("--"):
            key = a[2:].split("=")[0].replace("-", "_")
            if "=" in a:
                args[key] = a.split("=", 1)[1]
            elif i + 1 < len(argv) and not argv[i + 1].startswith("--"):
                args[key] = argv[i + 1]
                i += 1
            else:
                args[key] = True
        i += 1
    return args
