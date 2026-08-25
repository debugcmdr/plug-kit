#!/usr/bin/env python3
"""
测试插件 fixture(非 pytest 测试)——用于手动验证 bridge 链路。
位置在 tests/integration/ 下但无 test_ 函数,pytest 收集时不会执行断言;
它模拟一个输出 progress→result 协议流的插件 CLI,供 bridge 集成调试使用。
(E-4/E-5:原位置/命名易误导为测试文件,特此注明用途。)
"""
import sys
import json
import time
from datetime import datetime

def main():
    args = sys.argv[1:]
    
    if "--version" in args:
        print(json.dumps({
            "type": "result",
            "success": True,
            "data": {"version": "0.1.0", "name": "hello-test"}
        }))
        return
    
    if "--help" in args or "-h" in args:
        print("Usage: hello.py [--version] [--help]")
        return
    
    # Simulate a download task
    print(json.dumps({"type": "progress", "percent": 0, "message": "开始下载..." }))
    
    for i in range(1, 11):
        time.sleep(0.05)
        print(json.dumps({
            "type": "progress",
            "percent": i * 10,
            "speed": f"{i * 2.5}MB/s",
            "eta": f"{(10-i)*5}s",
            "message": f"下载中 {i*10}%"
        }))
    
    print(json.dumps({
        "type": "result",
        "success": True,
        "data": {
            "url": "https://example.com/video.mp4",
            "filename": "test_video.mp4",
            "size": 12345678,
            "message": "下载完成"
        }
    }))

if __name__ == "__main__":
    main()
