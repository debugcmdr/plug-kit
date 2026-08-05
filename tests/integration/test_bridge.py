#!/usr/bin/env python3
"""
测试插件 - 用于验证 bridge 链路
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
