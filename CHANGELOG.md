# 变更日志

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),版本语义化 [SemVer](https://semver.org/lang/zh-CN/)。

## [未发布] - 2026-08-24 开发阶段

### 新增

- **convert 插件 v0.3.0**:
  - 视频输出新增「mp4（H.265）」「mkv（H.265）」(libx265,`--codec` 参数,缺编码器明确提示;mp4 带 `hvc1` tag 兼容 Apple)
  - **视频提取音频**:视频类别下可选 mp3/m4a/wav,CLI 按输出扩展名分发零改动
  - 图片输出新增 **avif**(libsvtav1 → libaom 择优,都缺则 ENCODER_MISSING 提示)
  - 输入格式扩充:视频 +m2ts/mts/vob/3g2,音频 +m4b/mka,图片 +psd(ffmpeg 内建解码,零依赖风险)
  - 输出选项命名统一为「格式（说明）」,value 即真实扩展名,编码差异用 data-codec 区分

### 修复

- **convert JPG 质量选项失效**:`-q:v` 值域 1~31(越小越好),UI 传 90/75/60 被 ffmpeg clamp 到最差画质(实测三值输出字节数完全一致)——改为映射 90→q2 / 75→q5 / 60→q8,方向与档位恢复正确
- **convert 图片输入含 svg**:ffmpeg 官方构建无 SVG 解码器,选了必失败——已从输入白名单移除
- **convert 并发输出竞态**:批量 + 保存目录 + 同名文件时,check-then-write 的路径去重存在并发窗口,两任务可能写同一输出——改为 `O_CREAT|O_EXCL` 原子占位,并发下各自拿独立路径
- **convert ogg 输出依赖 libvorbis**:部分构建(含 Homebrew)无 libvorbis 时直接失败——回退 libopus(ogg 容器原生支持;实测 ffmpeg 8.x 原生 vorbis 编码器不可用,不能作为回退)

### 变更

- **convert 质量选项显隐收敛**:仅 jpg/webp(有损且接入质量参数)显示;png 无损、heic/avif/gif/bmp/tiff 未参数化,不再显示误导
- **convert 版本 0.2.0 → 0.3.0**,发布包与市场清单已重新打包同步
- **convert 一致性清理**:probe() 格式列表对齐 UI;删除 detect_category 死代码;webm 输出补编码器检测;has_encoder 进程内缓存(批量不重复探测);mkv(H.265) 补音频比特率
- **convert CLI 单元测试**:`tests/unit/test_convert_cli.py`(12 例:参数解析/原子占位含并发/质量映射/UI-CLI 白名单对齐/输出覆盖/codec 白名单,纯逻辑不依赖 ffmpeg)

### 修复（2026-08-24 项目整体审查 P0/P1）

- **市场清单 binaryUrl 坏 URL**:package-plugins.sh 未传 tag 时拼出 `releases/download//` 双斜杠(新用户市场安装 404)——tag 为空改为指向 `releases/latest/download/`,verify-manifests 增加 binaryUrl 格式门禁(拒绝空 tag 坏 URL)
- **package.json 缺 `@tauri-apps/cli` 声明**:`npm run tauri` 断链——补 devDependencies
- **任务状态机闭环**:invoke 前置校验(manifest 损坏/命令缺失/exe 丢失)失败时任务永久卡 Pending——统一失败处理并置任务 Failed;cancel/pause/resume 增加活跃状态前置检查,杜绝终态任务非法状态迁移
- **卸载状态不一致**:先递减共享依赖 refcount 再删目录,Windows 删失败时依赖已清但插件仍在——改为删目录成功后再递减
- **插件 UI 反馈链路断裂**:沙盒 iframe 无 allow-modals,`alert/confirm` 被静默阻断(playlist 导出/提取提示、download 导入提示不可见)——全部改为页面内 toast
- **插件列表健壮性**:单个插件 manifest 损坏会让整个 `list_plugins` 失败——跳过坏清单并记录日志
- **bridge task_id 归属校验**:插件可传任意 task_id 覆盖他人任务控制——已登记任务校验归属,不匹配拒绝
- **依赖安装警告透出**:共享依赖下载失败/回退不再只进日志,随 InstallResult.warnings 展示给用户
- **取消/超时杀进程树**:仅 kill 主进程会让 yt-dlp 拉起的 ffmpeg 残留为孤儿继续写盘——进程组 SIGTERM→SIGKILL(Windows taskkill /T)
- **json 命令超时**:list-formats/probe 等短命令 5 分钟超时,防网络卡死永久 running(progress 长任务不设限)
- **任务工作目录分离**:`~/.plugkit/work` 与插件安装临时目录 `~/.plugkit/tmp` 分离,安装清理不再误删运行中任务目录;启动时清理两类残留
- **插件输出协议校验**:最后一行必须是 result/error,progress/不可解析输出明确报错(不再静默返回)
- **跨平台修复**:PATH 探测 Windows 改用 `where`;package-plugins/dev-watch 的 `stat` 兼容 Linux(`-c`)与 BSD(`-f`);cookies 文件路径识别支持 Windows 盘符;convert 文件名取 basename 兼容反斜杠
- **playlist 导出 CSV 公式注入**:标题/上传者来自第三方站点,`=+-@` 开头单元格在 Excel 打开按公式解析——写入前加 `'` 前缀转义
- **convert 占位文件残留**:编码器缺失等提前失败时,O_EXCL 占位空文件残留输出目录——失败分支清理 0 字节占位
- **下载结果精确收集**:download/playlist 不再整目录 listdir(混入用户无关文件)——按 destination 记录精确收集,兜底回退目录
- **日志清理降频**:每次写日志全量扫描目录→60s 间隔;manifest 命令集合按 mtime 缓存(高频 invoke 免重复读盘)
- **插件公共共享模块(方案 B, G-6)**:新增 `plugins/shared/` 单点源码(`_protocol.py` 全插件通用 / `_ytdlp.py` download+playlist),打包与同步脚本自动复制进各插件 `tool/`——消除 download/playlist 双份复制导致的逻辑漂移,改一处全同步;verify-manifests 新增共享模块与源码一致性断言;CLI 在开发/测试环境自动回退从 shared 加载(生产用打包副本)
- **发布门禁增强(release.sh)**:① 依赖 sha256 固化检查——ffmpeg/ffprobe 存在空 sha256 平台即拒绝发布(G-8);② 自动执行 verify-manifests;③ GitHub token 改经 stdin 传给 curl(`-H @-`),不再出现于进程命令行(ps 泄露面)
- **依赖缓存有效性校验(B-1)**:缓存命中前校验二进制非空(固化平台再校验 sha256 一致),0 字节/半截损坏缓存自动删除重下——消除"幽灵缓存"导致的难排查运行失败;回退扫描同样过滤 0 字节文件
- **面板操作反馈(B-4)**:TaskPanel「打开输出文件夹」失败弹错误提示;SettingsPanel「打开日志/数据目录」补 try-catch + 失败提示(原静默/unhandled rejection)
- **依赖下载流式化(B-3)**:逐 chunk 写临时文件 + 增量 sha256(内存恒定,不再整包入内存——ffmpeg ~100MB 场景);硬上限 1 GiB(单文件全局,防异常/恶意 URL 写爆磁盘,超限即 abort+删 tmp);插件包下载同步加 100MB 上限(合法 zip <1MB 的百倍余量)
- **启动清理下载残留(要点 2)**:`cleanup_stale_download_tmp` 递归删 `~/.plugkit/cache/deps` 下中断下载遗留的 `download.tmp`
- **测试修复**:install_e2e/install_layout 硬编码旧版本 zip(0.1.0)→ 动态查找任意版本;install_real / market_remote_e2e 真实 GitHub 发布链路测试标记 `#[ignore]`(发布演练时 `cargo test --ignored` 跑,开发期不再误报)

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
