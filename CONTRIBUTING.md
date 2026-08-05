# 贡献指南

欢迎为 PlugKit 贡献代码或插件!

## 主程序

1. Fork 本仓库,`git clone` 后 `npm install`
2. 本地开发:`npm run tauri dev`
3. Rust 后端在 `src-tauri/src/`,Vue 前端在 `src/`
4. 提交前确保:`cargo test` + `npm run build` 均通过
5. 提交信息用 `P{n}: {描述}` 风格(如 `P2: fix market install`)

### 代码结构

```
src-tauri/src/
├── lib.rs               # Tauri 命令注册 + 应用入口
├── manifest_model.rs    # manifest.json 数据模型
├── manifest_fetcher.rs  # 市场清单拉取 + 镜像降级
├── plugin_manager.rs    # 安装/卸载/解压 + 安全校验
├── dependency_cache.rs  # 共享依赖缓存
├── tool_runner.rs       # CLI 子进程调用 + 流式协议解析
├── task_queue.rs        # 任务队列 + 持久化
├── message_bridge.rs    # iframe ↔ 后端消息路由
├── protocol.rs          # plugkit:// 自定义协议 + bridge 注入
├── security.rs          # Zip-Slip / SHA256 / 路径校验
├── config_mgr.rs        # 设置 + 插件配置
├── logger.rs            # 统一日志
└── bridge_version.rs    # bridge SDK 版本校验
```

## 发布插件

参考 [插件开发指南](docs/插件开发指南.md)。要点:

- 插件 = CLI + HTML,遵循标准 stdout JSON 协议
- 在 manifest 声明共享依赖,禁止自带 ffmpeg/yt-dlp
- 用模板仓库起步,本地验证协议,然后发布到 GitHub Release
- 在 [plug-kit-plugins](https://github.com/plug-kit/plug-kit-plugins) 清单仓库注册

## 测试

- 后端:`cd src-tauri && cargo test`
- 前端:`npm run build`(含 vue-tsc 类型检查)
- 插件协议:`cargo test --test plugin_e2e`(需要 hello 测试插件在 ~/.plugkit/plugins/)
