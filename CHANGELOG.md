# 变更日志

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),版本语义化 [SemVer](https://semver.org/lang/zh-CN/)。

## [未发布] - 2026-08-23 开发阶段

### 新增

- **应用图标**:蓝渐变圆角底 + 白色插头(sharp 矢量渲染,与 UI logo 完全同源比例)
- **首页 GitHub 卡片**:点击弹确认框,经系统浏览器打开仓库(新增 `open_url` 命令,http/https 白名单)
- **日志页打通**:任务错误结构化落盘 `TASK_ERROR`(错误码 + 完整 message + task_id),前端渲染为可展开条目;日志默认手动滚动
- **插件市场卡片**:对齐首页视觉(圆角/hover 浮起/图标胶囊/箭头动效)
- **任务中心**:按 `created_at` 倒序;显示插件显示名(非 id);中断任务补「应用重启,任务已中断」原因
- **download 插件 v0.3.0**:CSV/Excel/txt 批量导入链接(`import-file` 命令 + UI 按钮)
- **playlist 插件 v0.2.0**:多列表卡片化(连续提取、独立进度、并行下载);导出支持「纯链接 txt / 全量信息 CSV」双模式(含发布时间)
- **cookies 登录态**:浏览器/文件 cookies 支持;Safari 沙盒权限错误 → `COOKIES_PERMISSION` 诊断 + 一键打开「完全磁盘访问」面板(`open_permission_settings` + `MT.system.openPermissionSettings()`)

### 变更

- **报错链路规范化**:插件 UI 简要提示(原因段截断)→ 任务中心完整错误 → 日志页可展开详情;convert 错误补齐「原因+建议+详情+退出码」
- **发布门禁**:`package-plugins.sh` 版本号以插件 manifest 为唯一事实源;`verify-manifests.sh` 补本地/zip 内 manifest 版本对照(防 convert 类漂移再犯)
- **预装校验**:preinstall 复制前校验 bridgeVersion,插件运行时桥接检查移除
- **任务轮询**:上移 App 全局,侧边栏任务徽标实时更新
- **侧边栏**:工具拖拽排序(鼠标跟随 + 平滑动画 + 顺序持久化 localStorage)
- **UI 全面重设计**:统一设计语言(浅灰底 + 白卡片 + 品牌蓝),外壳与插件页面视觉一致

### 修复

- 任务中心无法滚动(外层 overflow 裁切)
- 日志级别过滤失效(时间戳前缀导致 starts_with 永不匹配)
- 拖拽频闪/残留拖拽态(确定性理论坐标 + document 级事件)
- convert 版本漂移(市场/fallback 清单与本地 manifest 不一致)
- 插件错误在插件 UI 与任务中心重复展示(层级错位)

## [0.1.0] - 2026-08-05

### 新增

- Tauri 2 + Vue 3 外壳,插件市场/任务中心/设置/日志面板
- 插件机制:CLI + iframe UI、plugkit:// 协议、桥接 SDK、共享依赖缓存、签名验签
- 内置插件:download(yt-dlp 下载)、convert(ffmpeg 转换)、playlist(播放列表批量下载)
- CI:构建 + 插件发布工作流
