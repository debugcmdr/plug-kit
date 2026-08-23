# 贡献指南

欢迎为 PlugKit 贡献代码或插件!

## 主程序

1. Fork 本仓库,`git clone` 后 `npm install`
2. 本地开发:`npm run tauri dev`(Rust 改动自动重编译并重启应用)
3. Rust 后端在 `src-tauri/src/`,Vue 前端在 `src/`
4. 提交前确保:`cargo test` + `npm run build` + `npm run test:unit` 均通过,并运行
   `./scripts/verify-manifests.sh` 校验市场清单一致性
5. 提交信息用 `P{n}: {描述}` 风格(如 `P2: fix market install`);新增用户可见变更记入
   [CHANGELOG.md](CHANGELOG.md)

### 代码结构

```
src-tauri/src/
├── lib.rs               # Tauri 命令注册 + 应用入口 + 日志读取
├── manifest_model.rs    # manifest.json 数据模型
├── manifest_fetcher.rs  # 市场清单拉取 + 内置镜像候选池降级(失效自动跳过)
├── plugin_manager.rs    # 安装/卸载/解压 + 版本守卫 + 原子替换
├── dependency_cache.rs  # 共享依赖缓存(固定版本 + 引用计数 + 校验和)
├── dep_manifest.rs      # 共享依赖版本表(yt-dlp pinned)
├── tool_runner.rs       # CLI 子进程调用 + 流式协议解析 + 并发上限 + 暂停/恢复 + 任务错误落盘
├── task_queue.rs        # 任务队列 + 持久化 + 7 天历史自动清理(created_at 倒序)
├── task_registry.rs     # 运行任务控制标志(cancel/pause)
├── message_bridge.rs    # iframe ↔ 后端消息路由(命令白名单 + 任务创建闭环)
├── protocol.rs          # plugkit:// 自定义协议 + bridge 注入 + 插件 CSP
├── security.rs          # Zip-Slip / SHA256 / Ed25519 签名验签
├── config_mgr.rs        # 插件级配置(~/.plugkit/configs)
├── preinstall.rs        # 内置插件预装(含 bridgeVersion 校验)
├── registry.rs          # 依赖缓存注册表(引用计数 + 备份恢复)
├── logger.rs            # 统一日志(按日 + 14 天轮转 + TASK_ERROR 结构化)
└── bridge_version.rs    # bridge SDK 版本校验
```

### 脚本

| 脚本 | 用途 |
|------|------|
| `scripts/sync-plugins.sh` | 同步 `plugins/` → `~/.plugkit/plugins/`(开发调试) |
| `scripts/dev-watch.sh` | 监听插件目录变化自动同步(npm run dev:watch) |
| `scripts/package-plugins.sh` | 打包插件 zip + 同步双份清单(版本以插件 manifest 为唯一事实源) |
| `scripts/verify-manifests.sh` | 清单一致性门禁(双份清单 + zip sha256 + manifest 版本对照,CI 用) |
| `scripts/release.sh` | 创建 GitHub Release 并上传插件包 |
| `scripts/gen-sign-key.sh` / `scripts/sign-plugin.sh` | 插件签名密钥生成 / 打包签名 |
| `scripts/clean-plugin-cache.mjs` | 清理插件缓存(按引用计数,孤儿目录) |
| `scripts/gen-app-icon.mjs` | 生成应用图标(sharp 矢量渲染,与 UI logo 同源) |

## 报错链路约定

插件错误按三级展示,保持层级清晰:

1. **插件 UI(最简)**:只显示原因段(`｜` 之前,约 90 字截断),完整错误放 `title` 属性
2. **任务中心(中等)**:显示插件 `error` 事件的完整 `message`
3. **日志页(最详细)**:`tool_runner` 自动落盘 `TASK_ERROR` 结构化行(错误码 + 完整 message + task_id),前端渲染为可展开条目

插件 CLI 的 `error(code, message)` 约定:`message` 建议包含「原因 + 操作建议 + `｜ 详情: <底层报错> (退出码 N)」`,错误码保持稳定(如 `NETWORK_ERROR` / `COOKIES_PERMISSION` / `FFMPEG_ERROR`)。

## 发布插件

参考 [插件开发指南](docs/插件开发指南.md)。要点:

- 插件 = CLI + HTML,遵循标准 stdout JSON 协议
- 在 manifest 声明共享依赖,禁止自带 ffmpeg/yt-dlp
- 用模板仓库起步,本地验证协议,然后发布到 GitHub Release
- 内置插件:在主仓库 [debugcmdr/plug-kit](https://github.com/debugcmdr/plug-kit) 的
  `market/plugins.json` 注册,由 `./scripts/package-plugins.sh` 自动生成条目,勿手工编辑

## 测试

- 后端:`cd src-tauri && cargo test`(注意:集成测试会清理 `~/.plugkit/plugins/`,跑完需重跑 `./scripts/sync-plugins.sh`)
- 前端类型检查 + 构建:`npm run build`(含 vue-tsc)
- 前端单元测试:`npm run test:unit`(vitest,任务映射回归等)
- 市场清单一致性:`./scripts/verify-manifests.sh`
- 插件协议:`cargo test --test tool_runner_e2e`(需先同步插件到 ~/.plugkit/plugins/)
