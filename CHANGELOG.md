# 变更日志

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),版本语义化 [SemVer](https://semver.org/lang/zh-CN/)。

## [未发布] - 2026-08-29 三阶段审查修复

### 修复

- **插件自管理 task_id 任务进入任务中心**:download/playlist 下载任务此前因前端传自生成 task_id 而绕过任务中心(不可见、无法取消),与 UI「可在任务中心查看进度」文案矛盾——`invoke_cli` 对未登记的 task_id 用该 id 建任务记录(`TaskQueue::create_with_id`,幂等),任务中心现可查看/取消下载任务
- **任务中心进度回跳 0%**:插件 progress 行缺字段时整体覆盖 `TaskProgress`(playlist 切换集数时 percent=None→0.0)→ `update_progress` 改字段级更新(None 保留旧值)
- **json 命令超时误标「已取消」**:超时/等待异常按 `Failed` 带原因标记,仅用户取消标 `Cancelled`

### 健壮性与安全加固(审查建议 A~J)

- **插件 error 行判定改解析判断**(tool_runner):字符串包含匹配可能被 progress 消息里的 `"type":"error"` 字样误判
- **plugkit:// traversal guard 补漏**(protocol):逐段拒绝 `..`(覆盖 `a/..` 末尾形式,不再误伤 `..x` 文件名)
- **invoke 响应路由防串台**(App.vue):pendingInvokes key 加插件前缀,消除跨插件同 id 碰撞时响应串台
- **download 视频清晰度 selector 收紧**:仅接受纯数字/数字+p,note 型 label("格式 137"/"720p (AV1)")回退最佳画质(新增单测)
- **convert 输出同文件防护**:`resolve_output` 增加 `os.path.samefile` 兜底(macOS 大小写不敏感/硬链接场景)
- **依赖 registry 损坏显式告警**:主文件与备份全损坏时 log::warn(引用关系丢失可排查)
- **zip 解压目录条目处理**:顶层目录 strip 后为空则跳过(消除残留空目录);目录判定用归一化名(反斜杠目录名不再误判)
- **yt-dlp venv 版本校验**:venv 内版本与 pinned 不符自动重装(消除版本漂移);`PIP_MIRROR` 支持 `PLUGKIT_PIP_MIRROR` 覆盖
- **URL 去重规范化**:`create_with_dedup` 去尾斜杠/大小写不敏感比较(查询参数保留;任务存储保留原值,新增单测)

## [未发布] - 2026-08-25 开发阶段

### 新增

- **工具箱更新红点 + 应用更新检查**:侧边栏「工具箱」按已装工具是否有新版亮红点(启动拉清单);新增 `check_app_update`(查 GitHub Releases latest vs 当前版本,**仅告知**,网络/限流静默)+ 设置页「有新版本 vX」提示 + 查看更新链接(GitHub Releases)
- **插件脚手架 `scripts/new-plugin.sh`**:一键生成插件骨架(manifest + CLI + HTML,协议要点/双路径导入/esc/toast 就绪),`package-plugins.sh` 改自动发现 `plugins/*/manifest.json`(消除硬编码,新插件零配置进打包)
- **Rust 单元测试**:task_queue 状态机迁移表 + 任务创建、dependency_cache 引用计数增删对称性(回归 B-1 落盘)
- **download/playlist CLI 单元测试**:`test_download_cli.py`(8 例)/`test_playlist_cli.py`(4 例),覆盖信息构造/列表构造/URL 识别/CSV 转义

### 变更

- **下载/解析统一使用 venv pip 版 yt-dlp**(`ytdlp_binary()`):冷启动 22s → 0.4s(PyInstaller 单文件每次解包是根因);venv 缺失自动创建(pinned 2026.08.19,与 dep_manifest 对齐),失败回退缓存二进制;解析服务删除后 `_ensure_ytdlp_venv` 承担 venv 创建
- **侧边栏「插件市场」→「工具箱」**:方向定案——放弃第三方插件生态,工具全部官方维护,工具级热更新为主、外壳变更才发整包
- **路径常量集中 `paths.rs`**:替换 9 处散落 `~/.plugkit` 路径(数据/日志/缓存/工作/临时/任务/插件/配置)
- **设置页版本号 Rust 透出**(`get_app_version`,弃 `VITE_APP_VERSION`),与更新比较同基准
- **错误码化去重提示**:后端 dedup 消息加 `[DUP_EXTRACT]` 前缀,插件 UI 按错误码判断(弃字符串匹配);macOS cookies 权限错误 `openPermissionSettings` 一键直达
- **manifest 依赖约束改存在性声明**:原 `>=6.0` 等版本约束从未被解析,清空约束使声明与实现一致(未来需版本门槛在 dep_manifest 层实现)

### 移除

- **常驻解析服务**(`_ytdlp_service.py` + `_ytdlp.py` 服务客户端):ROI 实测——无服务直连 pip 版冷解析 1.69s vs 服务 1.57s 仅差 0.1s(网络主导),250MB 常驻内存换"同 URL 重复解析 0s"边缘场景不划算;download/playlist 解析/下载统一直连 venv

### 修复

- **依赖引用计数落盘(B-1)**:缓存命中分支 `increment_refcount` 后补 `registry.save()`——装 A 装 B 共用依赖后卸 A,不再因内存 registry 未落盘误删 B 仍在用的依赖
- **日志跨文件顺序颠倒(B-2)**:多文件聚合后整体 reverse → 当天日志不足 500 行跨入昨天文件时顺序错乱——改为每文件内倒序聚合
- **zip 解压炸弹防护(B-3)**:单文件 64MB / 累计 512MB / 条目 1 万上限,防恶意包膨胀填满磁盘
- **playlist export 超时契约(C-1)**:CLI 1800s 超时 vs 外壳 json 模式 300s 硬超时冲突——export 改 progress+json(外壳不设超时),大列表全量导出不再 5 分钟被杀
- **playlist `_build_list_result` NameError**:条目缺 url 时引用未定义变量——加默认参数 + 单测覆盖
- **ytdlp_binary 隐藏 NameError**:回退分支引用 `dep` 但 `_ytdlp.py` 未导入(venv 存在时未暴露)——补导入
- **CSP 注入边界(B-5)**:HTML head 属性值含 `>` 时注入失效——`find_tag_end` 跳过引号段;img-src 收紧
- **shell 拼接注入面(B-4)**:`dep_available_on_path` 的 `sh -c` 拼接改参数化
- **substitute_args 一次性替换(B-8)**:占位符替换改为单遍 `substitute_arg_once`,消除参数值含占位符时的二次污染
- **前端健壮性**:mapTask progress 兜底、任务控制失败提示、任务筛选补 paused/interrupted、App.vue 事件类型化

## [未发布] - 2026-08-24 开发阶段

### 新增

- **convert 插件 v0.6.0**:
  - 视频模块左右分栏重构(用户驱动):左「格式转换」右「音频提取」两个独立操作,点哪个执行哪个;音频/图片分类保持单栏
  - **音频提取「原音频(无损复制)」**(默认):ffprobe 探测源音轨编码 → 映射容器 → `-c:a copy` 零转码零损耗;支持 aac/mp3/opus/flac/vorbis/pcm/ac3/eac3/wmav2/alac/aiff;失败降级重编码 mp3
  - **GIF 参数化**:帧率(5/10/15)与宽度(320/480/720)可调,scale 用 -2 保偶
  - **probe 即时执行**:进页面探测依赖与编码器能力(不进任务中心),ffmpeg 缺失前置提示、缺失编码器对应格式置灰
  - **文件拖放添加**:系统拖入文件 → 外壳 → 激活插件(MT.onFilesDropped),按分类扩展名过滤
  - **文件大小展示**:MT.fs.stat 读取已选文件体积
- **convert 插件 v0.5.0**:
  - 视频模块左右分栏雏形:格式转换 + 音频输出独立按钮(本版本由 v0.6.0 文案收敛)

### 修复

- **convert wav 强制降采样 44.1kHz**:无损格式却损失采样率——移除 `-ar`,保留源采样率
- **convert webm 防护不对称**:只查 libvpx-vp9 不查 libopus——双编码器任一缺失都明确提示
- **convert ts/m4v `-c:v copy` 必败**:源编码与容器不兼容时 ffmpeg 裸报错——copy 失败自动降级 libx264 转码
- **convert amr/wma 无编码器提示**:补 libopencore_amrnb/wmav2 has_encoder 检查(与其余分支对称)
- **元数据丢失**:转换不保留标题/日期等——视频/音频命令统一加 `-map_metadata 0`
- **convert 依赖缺失反馈滞后**:ffmpeg 缺失要等点转换才报错——probe 前置提示
- **plugkit:// 协议缓存**:协议响应无 Cache-Control,WebView 缓存插件 HTML 导致「同步了但界面不变」——统一加 `no-store`

### 变更

- **convert 版本 0.3.0 → 0.6.0**,音频提取重编码选项标注「重编码」、原音频为默认
- **convert avi/mpg/flv 统一补 `-preset medium`**(与其余分支行为一致)
- **convert CLI 单元测试 12 → 18 例**:原样提取映射/probe 编码器/GIF 参数/元数据/新 UI 结构断言
- **外壳桥接新增**:`stat_file`(文件大小)、`plugkit:files-dropped` 拖放事件链路、`MT.onFilesDropped`/`MT.fs.stat`(桥接 SDK v1.0.0 兼容追加)

### 新增（convert v0.3.0）

- **convert 插件 v0.3.0**:
  - 视频输出新增「mp4（H.265）」「mkv（H.265）」(libx265,`--codec` 参数,缺编码器明确提示;mp4 带 `hvc1` tag 兼容 Apple)
  - **视频提取音频**:视频类别下可选 mp3/m4a/wav,CLI 按输出扩展名分发零改动
  - 图片输出新增 **avif**(libsvtav1 → libaom 择优,都缺则 ENCODER_MISSING 提示)
  - 输入格式扩充:视频 +m2ts/mts/vob/3g2,音频 +m4b/mka,图片 +psd(ffmpeg 内建解码,零依赖风险)
  - 输出选项命名统一为「格式（说明）」,value 即真实扩展名,编码差异用 data-codec 区分

### 修复（convert v0.3.0）

- **convert JPG 质量选项失效**:`-q:v` 值域 1~31(越小越好),UI 传 90/75/60 被 ffmpeg clamp 到最差画质(实测三值输出字节数完全一致)——改为映射 90→q2 / 75→q5 / 60→q8,方向与档位恢复正确
- **convert 图片输入含 svg**:ffmpeg 官方构建无 SVG 解码器,选了必失败——已从输入白名单移除
- **convert 并发输出竞态**:批量 + 保存目录 + 同名文件时,check-then-write 的路径去重存在并发窗口,两任务可能写同一输出——改为 `O_CREAT|O_EXCL` 原子占位,并发下各自拿独立路径
- **convert ogg 输出依赖 libvorbis**:部分构建(含 Homebrew)无 libvorbis 时直接失败——回退 libopus(ogg 容器原生支持;实测 ffmpeg 8.x 原生 vorbis 编码器不可用,不能作为回退)

### 变更（convert v0.3.0）

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
