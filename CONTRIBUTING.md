# 贡献指南

欢迎为 PlugKit 贡献代码或插件!

## 主程序

1. Fork 本仓库,`git clone` 后 `npm install`
2. 本地开发:`npm run tauri dev`
3. Rust 后端在 `src-tauri/src/`,Vue 前端在 `src/`
4. 提交前确保:`cargo test` + `npm run build` + `npm run test:unit` 均通过,并运行
   `./scripts/verify-manifests.sh` 校验市场清单一致性
5. 提交信息用 `P{n}: {描述}` 风格(如 `P2: fix market install`)

### 代码结构

```
src-tauri/src/
├── lib.rs               # Tauri 命令注册 + 应用入口
├── manifest_model.rs    # manifest.json 数据模型
├── manifest_fetcher.rs  # 市场清单拉取 + 内置镜像候选池降级(失效自动跳过)
├── plugin_manager.rs    # 安装/卸载/解压 + 版本守卫 + 原子替换
├── dependency_cache.rs  # 共享依赖缓存(固定版本 + 引用计数 + 校验和)
├── dep_manifest.rs      # 共享依赖版本表(yt-dlp pinned)
├── tool_runner.rs       # CLI 子进程调用 + 流式协议解析 + 并发上限 + 暂停/恢复
├── task_queue.rs        # 任务队列 + 持久化 + 7 天历史自动清理
├── task_registry.rs     # 运行任务控制标志(cancel/pause)
├── message_bridge.rs    # iframe ↔ 后端消息路由
├── protocol.rs          # plugkit:// 自定义协议 + bridge 注入 + 插件 CSP
├── security.rs          # Zip-Slip / SHA256 / Ed25519 签名验签
├── config_mgr.rs        # 插件级配置(~/.plugkit/configs)
├── preinstall.rs        # 内置插件预装
├── registry.rs          # 依赖缓存注册表(引用计数 + 备份恢复)
├── logger.rs            # 统一日志(按日 + 14 天轮转)
└── bridge_version.rs    # bridge SDK 版本校验
```

## 发布插件

参考 [插件开发指南](docs/插件开发指南.md)。要点:

- 插件 = CLI + HTML,遵循标准 stdout JSON 协议
- 在 manifest 声明共享依赖,禁止自带 ffmpeg/yt-dlp
- 用模板仓库起步,本地验证协议,然后发布到 GitHub Release
- 内置插件:在主仓库 [debugcmdr/plug-kit](https://github.com/debugcmdr/plug-kit) 的
  `market/plugins.json` 注册,由 `./scripts/package-plugins.sh` 自动生成条目,勿手工编辑

## 测试

- 后端:`cd src-tauri && cargo test`
- 前端类型检查 + 构建:`npm run build`(含 vue-tsc)
- 前端单元测试:`npm run test:unit`(vitest,任务映射回归等)
- 市场清单一致性:`./scripts/verify-manifests.sh`
- 插件协议:`cargo test --test tool_runner_e2e`(需先同步插件到 ~/.plugkit/plugins/)
