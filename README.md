# PlugKit | 工具市场

开源的多媒体工具箱市场 —— 一个外壳,装上你需要的工具。

主程序仅作外壳,无内置业务功能;视频格式转换、音频提取、视频链接下载等**所有能力都以插件形式存在**。插件通过 GitHub 分发,零后端运维。

## ✨ 特性

- 🧩 **插件市场**:按需安装/卸载工具,自由插拔;版本对比自动提示「更新」
- 🔌 **任意语言插件**:标准 CLI stdout JSON 协议,Python / Rust / Go / Node 都能写
- 🔒 **安全边界**:Zip-Slip 防护、SHA256 校验、可选 Ed25519 签名验签、子进程隔离、iframe 沙盒 + 严格 CSP
- 🚀 **零后端**:市场清单是 GitHub 上的 JSON,插件包是 Release 上的 zip;镜像候选池自动降级,失效镜像自动跳过
- 🎨 **统一 UI**:插件界面是 HTML,由主程序注入桥接 SDK 直接调用后端
- 📦 **共享依赖**:ffmpeg / ffprobe / yt-dlp 固定版本统一管理,多插件共用;系统 PATH 优先,缺失时自动下载 + 校验和验证 + 缓存引用计数
- ⚙️ **任务中心**:长任务统一管理,支持暂停/恢复/取消,并发上限 5,历史 7 天自动清理;按时间倒序展示,显示插件名/处理文件名 + 「打开输出文件夹」入口
- 📋 **三级报错**:插件内简要提示 → 任务中心完整错误(原因+建议+错误码+详情) → 日志页按任务可展开的详细条目
- 🐛 **日志页**:外壳与任务错误统一落盘(按日 + 14 天轮转),错误条目默认折叠、点击展开详情

## 🛠 内置插件

| 插件 | 功能 | 依赖 |
|------|------|------|
| [download](https://github.com/debugcmdr/plug-kit) | 视频/音频/图片/字幕下载(播放列表、cookies 登录态、CSV/Excel/txt 批量导入链接) | yt-dlp |
| [playlist](https://github.com/debugcmdr/plug-kit) | 播放列表/合集批量下载(多列表卡片、逐集勾选、导出纯链接或全量信息 CSV) | yt-dlp |
| [convert](https://github.com/debugcmdr/plug-kit) | 视频/图片格式转换(预设 + 清晰度/压缩参数) | ffmpeg |

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

插件 UI 通过主程序注入的 `window.MT` / `window.PlugKit` 调用命令、订阅进度、读写配置、管理任务(submit / cancel / pause / resume)。

## 🚀 发布新版本

内置插件随主仓库分发。发布流程:

```bash
# 1. 打 tag(触发 CI 构建 + 打包插件)
git tag v0.1.1
git push origin v0.1.1

# 2. 打包插件 zip(确定性,sha256 可复现)→ 生成到 release/plugins/,
#    并自动同步 market/plugins.json + fallback 快照
#    (版本/sha256/size/binaryUrl 由脚本写入,以各插件 manifest 为唯一事实源)
./scripts/package-plugins.sh v0.1.1

# 3. 创建 GitHub Release 并上传插件 zip
./scripts/release.sh v0.1.1

# 4. 提交同步后的清单(CI 会用 verify-manifests.sh 再校验一致性)
git add market/plugins.json src-tauri/assets/fallback-manifests.json
git commit -m "chore: sync v0.1.1 plugin manifests"
git push origin main
```

> 注意:GitHub Raw CDN 缓存最长 5 分钟,发布后稍等片刻再验证市场拉取。

### 插件签名(可选但推荐)

1. 生成密钥对:`./scripts/gen-sign-key.sh`(私钥妥善保管,勿提交仓库)
2. 将输出的公钥 base64 填入 `src-tauri/src/security.rs` 的 `OFFICIAL_PUBLIC_KEY`
3. 重新打包,`package-plugins.sh` 会自动为插件 zip 签名并写入清单 `signature` 字段
4. 配置信任锚后,安装**未签名**插件会被拒绝

## 🏗 架构

```
PlugKit 主程序 (Tauri 2 + Vue 3)
├── 市场清单:GitHub Raw market/plugins.json + 内置兜底快照 + 内置镜像候选池自动降级(内存缓存 5min,失效镜像自动跳过)
├── 插件包:GitHub Release zip + SHA256 + Zip-Slip 校验 + 可选 Ed25519 签名验签
├── 插件运行:子进程 + 标准 stdout JSON 协议 + 进度推送 + 每任务独立工作目录
├── 插件 UI:plugkit:// 自定义协议加载 iframe(严格 CSP 注入)+ 桥接 SDK 注入
├── 共享依赖:ffmpeg/ffprobe/yt-dlp 固定版本缓存(系统 PATH 优先,缺失自动下载)+ 引用计数 + 校验和验证
└── 错误链路:插件 CLI error 事件 → 任务中心(task.error)→ 日志页(TASK_ERROR 结构化落盘,可展开)
```

数据目录:`~/.plugkit/`(插件、缓存、配置、任务、日志)

## 📄 License

MIT
