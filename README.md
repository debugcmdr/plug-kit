# PlugKit | 工具市场

开源的多媒体工具箱市场 —— 一个外壳,装上你需要的工具。

主程序仅作外壳,无内置业务功能;视频格式转换、音频提取、视频链接下载等**所有能力都以插件形式存在**。插件通过 GitHub 分发,零后端运维。

## ✨ 特性

- 🧩 **插件市场**:按需安装/卸载工具,自由插拔
- 🔌 **任意语言插件**:标准 CLI stdout JSON 协议,Python / Rust / Go / Node 都能写
- 🔒 **安全边界**:Zip-Slip 防护、SHA256 校验、子进程隔离、iframe 沙盒
- 🚀 **零后端**:市场清单是 GitHub 上的 JSON,插件包是 Release 上的 zip
- 🎨 **统一 UI**:插件界面是 HTML,由主程序注入桥接 SDK 直接调用后端
- 📦 **共享依赖**:ffmpeg / yt-dlp 统一管理,多插件共用,自动缓存

## 🛠 内置插件

| 插件 | 功能 | 依赖 |
|------|------|------|
| [download](https://github.com/plug-kit/plugkit-download) | 视频链接下载(YouTube / Bilibili 等) | yt-dlp |
| [convert](https://github.com/plug-kit/plugkit-convert) | 视频格式转换、压缩 | ffmpeg |

## 📦 安装

### 从源码运行

```bash
# 前置:Node.js 18+、Rust 1.78+、Tauri CLI
npm install
npm run tauri dev
```

### 打包发布

```bash
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/`。

## 🧩 开发一个插件

参考 [插件开发指南](docs/插件开发指南.md) 与 [Python 模板](templates/python/plugkit.py)。

一个插件 = **一个 CLI 可执行文件 + 一个 HTML 界面**:

```python
# 你的插件 tool/plugkit-my-tool
from plugkit import progress, result, error

def handle(cmd, args):
    if cmd == "convert":
        progress(0, message="开始转换")
        # ... 调用 ffmpeg,解析进度 ...
        result({"output": "file.mp4"})

handle(sys.argv[1], parse_args(sys.argv[2:]))
```

插件 UI 通过主程序注入的 `window.MT` / `window.PlugKit` 调用命令、订阅进度、读写配置。

## 🚀 发布新版本

内置插件随主仓库分发。发布流程:

```bash
# 1. 打 tag(触发 CI 构建 + 打包插件)
git tag v0.1.1
git push origin v0.1.1

# 2. 打包插件 zip(确定性,sha256 可复现)→ 生成到 release/plugins/
./scripts/package-plugins.sh v0.1.1

# 3. 创建 GitHub Release 并上传插件 zip
./scripts/release.sh v0.1.1

# 4. 确认 market/plugins.json 的 sha256 与 Release 实际一致后提交推送
git add market/plugins.json src-tauri/assets/fallback-manifests.json
git commit -m "chore: sync v0.1.1 plugin manifests"
git push origin main
```

> 注意:GitHub Raw CDN 缓存最长 5 分钟,发布后稍等片刻再验证市场拉取。

## 🏗 架构

```
PlugKit 主程序 (Tauri 2 + Vue 3)
├── 市场清单:GitHub Raw market/plugins.json + 内置兜底快照 + 多镜像降级
├── 插件包:GitHub Release zip + SHA256 + Zip-Slip 校验
├── 插件运行:子进程 + 标准 stdout JSON 协议 + 进度推送
├── 插件 UI:plugkit:// 自定义协议加载 iframe + 桥接 SDK 注入
└── 共享依赖:ffmpeg/yt-dlp 缓存 + 引用计数 + 环境变量注入
```

数据目录:`~/.plugkit/`(插件、缓存、配置、任务、日志)

## 📄 License

MIT
