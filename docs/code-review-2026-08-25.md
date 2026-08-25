# PlugKit 全模块分阶段代码审查报告

- 审查日期：2026-08-25
- 审查范围：全项目（插件 download/convert/playlist/shared、Rust 后端 17 文件、前端外壳 13 文件、发布流水线 8 脚本、测试 3 文件），约 1.1 万行
- 审查方法：分模块逐文件精读，按六维度检查（逻辑漏洞 / 输入输出边界 / 异常捕获 / 安全隐患 / 性能 / 可读性）
- 问题分级：**【必须修复】**（功能性 bug / 安全漏洞 / 崩溃）→ **【建议优化】**（健壮性 / 异常 / 性能）→ **【仅美化】**（格式 / 命名 / 风格）
- 状态：**本报告仅审查与方案，未执行任何修改**；修复待确认后执行

---

## 阶段一：分模块专项审查

### 1. download 插件（plugins/download/）

**模块职责**：万能下载（yt-dlp 驱动），支持信息提取、视频/音频/图片/直播/字幕下载、播放列表、cookies 登录态、txt/csv/xlsx 文件导入链接。

#### 【必须修复】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| D-1 | `tool/index.html` L170-175 | **「清空」按钮只清 `links`/`parseQueue` 数组并隐藏 `#resultSection`，不清空其 DOM 子节点**。旧链接块仍挂在容器内，再次「提取全部」时旧块与新块同时显示；旧块按钮的 `data-idx` 指向新 `links` 数组下标 → **点击旧块的下载按钮会下载到错误的链接** | 清空时执行 `$('resultSection').innerHTML = ''`；并校验 `links[i]` 归属（`startDownload` 中比对 `links.includes(l)`） |

#### 【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| D-2 | `index.html` L233 | 粘贴超过 20 条链接时**静默截断**，用户无感知 | 截断时 toast 提示"仅保留前 20 条，其余已忽略" |
| D-3 | `index.html` L277 / `index.html` L407 | 错误分类依赖 `msg.indexOf('该链接') === 0` / `'完全磁盘访问'` **字符串匹配**，与后端文案强耦合（后端改文案即失效） | 后端错误传播携带结构化错误码（见 G-1），前端按 code 分支 |
| D-4 | `plugkit-download` L296-305 | 直播 `extra` 参数注释声明"可带时长限制 extra=Nm"，**但代码只处理 `from_start`，时长限制未实现** | 删除误导注释，或实现 `--live-from-start` + 时长限制（`extra` 解析 Nm） |
| D-5 | `manifest.json` L10-12 | **dependencies 仅声明 yt-dlp，未声明 ffmpeg**；但视频合并（`--merge-output-format mp4`）与 mp3 提取（`-x`）依赖 ffmpeg → 安装时不保证 ffmpeg，首次使用必报 FFMPEG 类错误 | manifest 补 `"ffmpeg": ">=6.0"`（与 convert 一致；同 playlist，见 G-3） |
| D-6 | `index.html` L401-416 | 下载失败重复点击时**错误 div 反复叠加** | 失败渲染前移除同元素旧错误节点 |
| D-7 | `index.html` L158-162 | `changeDir` 失败静默无反馈 | 与 convert 一致，失败时 toast |

#### 【仅美化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| D-8 | `plugkit-download` L107 | 超时文案写"yt-dlp 在 90s 内未取得元数据"，实际 `timeout=150` | 文案改 150s |
| D-9 | `plugkit-download` L244 | `[download] 100%` 分支实际不可达（100% 行总带 `of xxx`，先命中 L225 正则） | 删除或改为去重上报 |
| D-10 | `index.html` L144-147 | 大文件统一显示 MB（2GB 显示 2048.0 MB） | 自适应 GB 单位 |

---

### 2. convert 插件（plugins/convert/）

**模块职责**：格式转换（ffmpeg 驱动），视频/音频/图片互转、视频音频提取（含 v0.6.0 原样无损复制）、批量、拖放、probe 即时探测。

#### 【必须修复】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| C-1 | `plugkit-convert` L379-380 | **`extract_audio_copy` 对未知音轨编码（dts / truehd / pcm_s16be / pcm_u8 / mp2 等）`AUDIO_CODEC_EXT.get(codec)` 返回 None → `base + "." + None` 抛 TypeError → 顶层 PLUGIN_ERROR 崩溃**。蓝光 remux 的 DTS 音轨是常见输入，选「原音频（无损复制）」必崩 | 映射缺失时按 copy 失败路径降级：先尝试 `-c:a copy` 到 wav（PCM 容器兼容性最广），失败再降级重编码 mp3；并为 dts/truehd 补映射（mka 容器可封装 dts） |
| C-2 | `plugkit-convert` L469-486 | **`convert_image` 所有失败分支（FileNotFoundError / TimeoutExpired / returncode≠0）均不清理 `_reserve_path` 占位文件** → 失败后残留 0 字节/半成品文件，用户误判"已生成"（与 convert_video 各分支的 cleanup 行为不一致） | 三个失败分支统一补 `cleanup_if_empty(output_path)` |

#### 【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| C-3 | `plugkit-convert` L245-261 / L390-404 | copy 失败降级路径：首次失败的 >0 字节半成品文件未清理（cleanup_if_empty 只清 0 字节），降级成功时旧半成品与最终文件并存 | 降级前删除首次失败产生的输出文件（非 0 字节也删，因为即将被降级产物覆盖） |
| C-4 | `index.html` L379-387 | **切换视频/音频/图片分类不清空已选文件列表** → 视频文件可被"音频提取"按钮操作（ffmpeg 能提取但语义混乱），且提示语"仅显示当前类别的文件"与已选列表不符 | 切分类时按新类别过滤或清空已选文件并提示 |
| C-5 | `index.html` L400-431 | 批量任务成功/失败**交替覆盖 status 区**，最后只显示最后一个事件，用户看不到汇总 | 聚合显示（如"成功 X / 失败 Y"），或展示最近一条 + 计数 |
| C-6 | `plugkit-convert` L283-290 | AUDIO_EXTS 含 `m4b/mka`（输入白名单），但 `codec_map` 无对应项 → 输出 `.m4b` 会以 libmp3lame 写入 m4b 容器（ffmpeg 报错或产生坏文件） | codec_map 补 `m4b→aac`、`mka→aac`，或在 main 分发时拒绝 |
| C-7 | `index.html` L446 | 视频转换也传 `quality: $('quality').value`（隐藏下拉默认 90），CLI `convert_video` 不使用该参数 → 无效参数传递 | video 分类不传 quality |
| C-8 | `plugkit-convert` L230-232 | avi/mpg/flv 音频统一 `-c:a aac`：**AVI / MPEG-PS 容器对 AAC 支持不佳**，部分播放器打不开 | avi/mpg 改 `-c:a libmp3lame`（更兼容），flv 保持 AAC |
| C-9 | `plugkit-convert` L513-519 | `probe()` 输出的 video/audio/image 格式列表硬编码，**与 UI FORMATS 无同源断言**（仅有输入白名单断言） | 单测增加 probe 列表 ↔ UI FORMATS 对齐断言 |
| C-10 | `plugkit-convert` L104-109 | `run_with_progress` 错误消息不含输出文件名，多任务并发时用户不知道哪个文件失败 | 消息带 `file_name` |

#### 【仅美化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| C-11 | `plugkit-convert` L134-150 | `_reserve_path` while 无上限（理论上目录存在大量序号文件时循环久） | 加最大尝试次数，超限报错 |

---

### 3. playlist 插件（plugins/playlist/）

**模块职责**：播放列表/频道批量提取与下载、按序号勾选、导出清单（CSV 全量 / TXT 链接）。

#### 【建议优化】（本模块无必须修复项；UI 的 `btnClear` 正确清空了 DOM，与 download 的 D-1 形成对照）

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| P-1 | `plugkit-playlist` L180-183 | `download()` 命令**未注入 `--socket-timeout/--retries/--fragment-retries`**（`base_cmd` 只在 list/export 使用），网络抖动时批量下载失败率高于 download 插件 | 复用 base_cmd 网络参数（与 `_run_ytdlp` 对齐） |
| P-2 | `manifest.json` L10-12 | dependencies 仅 yt-dlp，未声明 ffmpeg（`--merge-output-format mp4` 需要） | 补 `"ffmpeg": ">=6.0"`（见 G-3） |
| P-3 | `index.html` L252 / L310-324 | ① 超长列表截断展示 500 条，但**初始全选包含 500 条之后的条目**（"下载不受影响"）——用户看到 500 条却会下载全部；② **「全不选→全选」只把截断的 500 条加回 items**，表格外条目丢失勾选 | ① 截断时明确提示"已默认勾选全部 N 集"；② 全选基于 `info.entries` 全量而非截断数组 |
| P-4 | `index.html` L392 / L436 | 下载/导出的 url 用 `card.info.playlist_url \|\| $('url').value.trim()` 回退输入框**当前值**——提取 A 后修改输入框为 B，若 A 无 playlist_url 字段则**下错列表** | appendCard 时快照原始 url 存入 card，不再回退输入框 |
| P-5 | `index.html` L404-418 | 下载失败后 `msg.style.color` 与 `fill.style.background` 置红，下次下载开始仅重置 fill 背景，**msg 颜色残留红色**；失败按钮重复 append | 下载开始时重置 msg 颜色；失败渲染前清理旧按钮 |
| P-6 | `plugkit-playlist` L238/L277 | 同名播放列表导出到同一目录**直接覆盖**（无序号保护） | 文件名冲突时追加序号 |
| P-7 | `index.html` L285 | 条目 `<a href="${esc(e.url)}">` 直接渲染 yt-dlp 返回的 URL，未过滤协议（理论 `javascript:` 注入面，沙盒无 allow-popups 缓解但纵深不足） | 仅当 url 以 http(s):// 开头才渲染为链接 |

#### 【仅美化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| P-8 | `plugkit-playlist` L38 | 超时文案"120s" vs 实际 `timeout=180` | 文案改 180s |
| P-9 | `plugkit-playlist` L140 | `[download] 100%` 死分支（同 D-9） | 删除 |

---

### 4. shared 公共模块（plugins/shared/）

**模块职责**：插件公共 SDK —— `_protocol.py`（stdout 协议 / dep / parse_args）、`_ytdlp.py`（cookies / playlist / 错误诊断）。变更影响所有使用方，破坏性变更需同步重打包。

#### 【必须修复】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| S-1 | `_ytdlp.py` L39-44 | **`playlist_args()` 的 `playlist is True` 分支返回 `[]`（不加 `--yes-playlist`）**——但 `parse_args` 对无值 flag（`--playlist`）返回 `True` → 手动 CLI 调用 `--playlist` 时整列表下载失效（UI 走 manifest 占位符传 "1" 不受影响，但协议逻辑自相矛盾） | `playlist is True` 应返回 `["--yes-playlist"]`（True 即用户意图下载整个列表） |

#### 【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| S-2 | `_ytdlp.py` L33-36 | cookies 传不存在的文件路径（如手误）→ `os.path.isfile` false → **按浏览器名透传** → yt-dlp 报"无效浏览器"模糊错误 | 路径含分隔符但不存在时明确报 COOKIES_FILE_NOT_FOUND |
| S-3 | `_protocol.py` L50-67 | `parse_args` 对非 `--` 开头的参数**静默忽略**（拼错参数名无反馈） | 收集未识别 token 并在 debug 日志输出（或严格模式报错） |
| S-4 | `_protocol.py` L37-39 | `error()` 只打印不退出，**调用方必须记得手动 return**，漏掉则进程以 0 退出但协议已输出 error 行（依赖 stdout 行判断，退出码无意义） | 文档明确约定；或提供 `fail(code,msg)` 组合（print + exit 1） |

---

### 5. Rust 后端（src-tauri/src/，17 文件）

**模块职责**：Tauri 2 外壳后端——任务队列/执行器、桥接与协议、插件安装管理、共享依赖管理、配置与安全、日志。

#### 【必须修复】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| R-1 | `plugin_manager.rs` L116-126 / L128-146 | **bridgeVersion 与 minAppVersion 校验用 `if let Err(_) = check_bridge_version(...)`，只捕获 semver 解析错误（Err），对 `Ok(false)`（版本不匹配）放行** → 协议不兼容的插件会被正常安装（对比 `preinstall.rs` L51-55 用的是正确的 `unwrap_or(false)`） | 改为 `if !check_bridge_version(...).unwrap_or(false) { 拒绝 }` |
| R-2 | `tool_runner.rs` L438-448 | **暂停循环 `while pause_flag` 不检查子进程是否已退出**：若任务在暂停请求到达前已自然结束，`suspend_process` 对死进程发信号静默失败，随后**卡在等待恢复的死循环**，任务永不进入终态（用户必须点"恢复/取消"才解锁） | 循环内先 `try_wait()`，进程已退出则跳出（记 paused 状态为已完成） |
| R-3 | `tool_runner.rs` L374-410 + `assets/plugkit-bridge.js` L19-22 | **invoke 层 10 分钟硬超时对 progress 模式长任务（下载/转换 >10 分钟）→ bridge.js reject → 插件 UI 误报"失败"、按钮恢复，用户可重复提交 → 同一文件重复下载**；任务中心显示正常但插件 UI 状态错误（全局问题 G-2，跨文件） | progress 模式不设 invoke 超时（任务生命周期由任务中心管理）；或插件 UI 改为"提交后不 await 完成，仅监听进度" |

#### 【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| R-4 | `dependency_cache.rs` L11-16 | `HTTP_CLIENT` 设 120s **整体请求超时（含 body 读取）**，慢网下载 ffmpeg（~200MB）时 120s 内未完成即失败，镜像降级也救不了（同一客户端） | 分阶段超时：连接超时 + 每 chunk 读超时（或大幅提高总超时） |
| R-5 | `tool_runner.rs` L69 | async 上下文中用 `std::thread::sleep(200ms)` 阻塞 tokio worker | 改 `tokio::time::sleep` |
| R-6 | `tool_runner.rs` L578-632 | `read_stdout_protocol` **累积所有行到 Vec**（长下载数小时 → 数千行驻留内存），实际只用最后一行；`read_stderr` 全量读入但结果丢弃 | stdout 只保留末尾 N 行；stderr 边读边丢（仅防管道阻塞） |
| R-7 | `tool_runner.rs` L656-686 | `strip_unfilled_placeholders` 把**任意 `{标识符}` 当占位符删除**——用户文件路径含 `{1}` 等（如 `报告{2}版.mp4`）会被改写路径 | 传"已提供的参数键集合"，只 strip 未提供键对应的占位符 |
| R-8 | `task_queue.rs` L97-106 + `stores/plugins.ts` L143-147 | 任务列表**全量返回 + 前端 3s 轮询**，任务多时每 3s 全量序列化 | 分页/增量/仅活跃任务；轮询可降频（无活跃任务时 10s+） |
| R-9 | `lib.rs` L300-316 / `message_bridge.rs` L155-202 | `cancel/pause/resume` **不返回任务是否存在/操作是否生效**（任务不存在也返回 `{cancelled:true}`） | 返回 `TaskQueue::cancel()` 的实际结果（bool） |
| R-10 | `tool_runner.rs` L406-410 | **错误传播丢失错误码**：CLI 输出 `error{code,message}`，invoke 只回 `Err(message)` → 前端只能字符串匹配（见 D-3） | 返回结构化错误（`{code, message}` 或自定义错误类型带 code 字段） |
| R-11 | `tool_runner.rs` L386-388 | `result` 行的 `success` 字段未校验（插件输出 `success:false` 也标记 Completed） | `ResultData.success == false` 视为失败 |
| R-12 | `protocol.rs` L85 | **CSP 条件注入**：HTML 内容含 `"plugkit-bridge.js"` 字符串即豁免 CSP 注入 → 插件页面无 CSP 防护（可自由外联网络） | 无条件注入 CSP meta（插件自己引 bridge 时去重即可） |
| R-13 | `protocol.rs` L108 | `<head` 前缀匹配会**误命中 `<header>` 标签** → CSP meta 注入到 `<header>` 内（body 区域），HTML 规范中 meta CSP 仅在 `<head>` 生效 → 防护失效 | 匹配 `<head` 后要求后一字符为空白或 `>`（排除 header） |
| R-14 | `plugin_manager.rs` L23-26 | `install()` **无条件先 `fetch_market_manifests()`**，离线时连本地重装路径（L56-62）都进不去 | 市场拉取失败降级：本地已装插件直接走本地 manifest 重装 |
| R-15 | `plugin_manager.rs` L31-35 | 版本解析失败（`semver::Version::parse` Err）→ 按 `0.0.0` 处理 → `existing >= market` 恒真 → **永远 ALREADY_LATEST，无法重装/升级** | 解析失败按"需重装"放行 |
| R-16 | `plugin_manager.rs` L226-258 | `list()` 会把 `{id}.old` 残留目录（原子换装崩溃残留）**当正常插件列出** | 跳过以 `.old` 结尾的目录 |
| R-17 | `plugin_manager.rs` L301-353 | zip 解压**无总大小/条目数限制**：下载上限 100MB 的压缩包可解压膨胀数 GB（zip 炸弹） | 解压时累计写入字节数 + 条目数上限 |
| R-18 | `manifest_fetcher.rs` L117-176 | 市场清单**多源并行、先到先用**：镜像（公共代理）可能先返回被篡改的清单 → 成为市场权威（sha256 也是清单给的，镜像可全链路伪造）；当前无签名验证（G-18）使该风险敞口 | ① 原始 URL 结果优先（镜像仅作回退）；② 或对镜像响应与原始响应做交叉校验 |
| R-19 | `dep_manifest.rs` L125-141 | `dep_available_on_path` 每次任务 invoke 都跑 `sh/where` 子进程（每任务 2-3 次） | 进程内缓存 PATH 探测结果（TTL 30s） |
| R-20 | `dependency_cache.rs` L53-90 | `clean_orphans()` 无文件锁，与 `install_dependency` 并发时读-改-写覆盖（registry 丢失更新） | 复用 `acquire_lock()` |
| R-21 | `lib.rs` L331-384 | `log_read` 整文件读入内存再 reverse（日志文件大时内存峰值高） | 从文件尾部反向读 N 行 |
| R-22 | `tool_runner.rs` L30-41 | `suspend_process` 对**已退出**进程发信号（配合 R-2 的卡死场景） | 调用前 try_wait 确认存活 |

#### 【仅美化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| R-23 | `dep_manifest.rs` L144-156 与 `plugin_manager.rs` L425-437 | `current_platform()` 重复定义 | 提取到公共模块 |
| R-24 | `registry.rs` L114-120 | `entry_count` 数的是版本数而非平台条目数（语义与 stats 展示"缓存条目"不符） | 遍历 platform 层计数 |
| R-25 | `security.rs` L101-111 | `validate_sha256` 十六进制比较区分大小写 | 统一转小写比较 |

---

### 6. 前端外壳（src/）

**模块职责**：Vue 3 外壳——布局/侧边栏/任务中心/市场/设置/日志、iframe 常驻管理、window 桥接转发（零业务逻辑）。

#### 【必须修复】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| F-1 | `components/Sidebar.vue` L140-157 | **拖拽排序松开后的 click 误导航**：`onPointerUp` 在 `mouseup` 时执行并把 `suppressClick` 重置为 false，浏览器随后派发的 `click` 事件到达 `handleSelect` 时已无法抑制 → **用户拖完排序工具却跳转到该工具页面** | 用"最近一次 mouseup 发生过重排"的时间戳/标志，在 click 处理器中判断（事件顺序 mouseup→click，标志需保留到 click 之后，如 setTimeout 清除） |
| F-2 | 同 R-3 | bridge.js invoke 10 分钟超时对长任务误报失败（跨模块，见 R-3） | 见 R-3 |

#### 【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| F-3 | `components/PluginPanel.vue` L11-13 | iframe src **硬编码 `tool/index.html`**，忽略 manifest `ui.html` 字段（模型定义了但从未使用） | 加载时读 manifest 的 `ui.html`（无则默认 tool/index.html） |
| F-4 | `stores/plugins.ts` L143-147 | 任务 3s 轮询无节流，无任务时也持续轮询 | 无活跃任务时降频至 15s |
| F-5 | `components/LogPanel.vue` | 日志页仅手动刷新 | 提供自动刷新开关（如 5s） |

#### 【仅美化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| F-6 | `components/AppIcon.vue` L13-35 | 未使用的图标（trash/alert/check/stop/download/plus/info） | 清理或标注预留 |
| F-7 | `components/MarketPanel.vue` L35-43 | `compareVersions` 忽略预发布/构建元数据段 | 按需支持（当前内置插件无预发布） |

---

### 7. 发布流水线与测试（scripts/ + tests/）

**模块职责**：打包（package-plugins）、清单同步（fallback+market）、门禁（verify-manifests）、发布（release）、开发同步（sync/dev-watch）、签名工具、单元测试。

#### 【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| T-1 | `scripts/dev-watch.sh` L33 | `md5` 命令在 Linux（md5sum）不存在 → Linux 上指纹计算失败，退化为每次全量同步 | 兼容 `md5`/`md5sum` |
| T-2 | `scripts/gen-sign-key.sh` L12-15 | 生成的私钥未 `chmod 600`（默认受 umask 影响可能 644） | 生成后 chmod 600（.gitignore 已忽略 .signing/，双保险） |
| T-3 | `scripts/release.sh` L45 | 门禁 3 用字符串匹配检测 `OFFICIAL_PUBLIC_KEY` 是否配置（格式变化即误判） | 改由 verify 脚本读 security.rs 结构化判断 |
| T-4 | `tests/unit/test_convert_cli.py` | 单测覆盖良好（18 例），但**缺 C-1 对应的未知音轨编码用例**（dts 崩溃点未被测试捕获） | 补 `extract_audio_copy` 未知编码降级路径测试 |
| T-5 | — | **download / playlist CLI 无单元测试**（`playlist_args` 的 S-1 缺陷、`diagnose_ytdlp_error` 分类均无回归保护） | 为 shared/_ytdlp.py 与 download/playlist 关键路径补单测 |

#### 【仅美化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| T-6 | `tests/integration/test_bridge.py` | 实为模拟插件脚本（无断言），非自动化测试 | 标注用途或改造为 pytest 断言 |

---

## 阶段二：全局综合审查

### 全局评估结论

总体架构健康：**零业务逻辑外壳 + CLI/iframe 插件**的核心约束贯彻到位（桥接身份校验、CSP、沙盒、Zip-Slip 防护、依赖校验、原子写、任务状态闭环等安全与健壮性设计显著优于同类项目）。插件三件套的 UI/CLI 白名单一致性有单测保障。全局层面存在 **4 个跨模块一致性问题**（错误码丢失、长任务超时、ffmpeg 依赖声明缺失、插件间重复逻辑）与若干信任链/资源开销问题，均属可修复范围，**无需架构级改动**。

### 全局问题清单

| # | 维度 | 位置 | 问题描述 | 风险 | 修复建议 |
|---|---|---|---|---|---|
| G-1 | 错误传播 | CLI error → tool_runner → 前端 | **错误码在传播链中丢失**：CLI 输出 16 种语义化 code，日志页完整保留，但任务中心与插件 UI 只拿到 message → 前端被迫字符串匹配（D-3/C-5 的根因） | 中 | tool_runner 返回结构化错误（携带 code）；前端按 code 分支渲染 |
| G-2 | 状态一致性 | bridge.js + tool_runner | **invoke 10 分钟超时 vs progress 任务无超时**：长任务插件 UI 误报失败、可重复提交（R-3/F-2） | 高 | progress 模式取消 invoke 超时；或 UI 改 submit-and-forget |
| G-3 | 依赖声明 | download/playlist manifest | **ffmpeg 依赖未声明**（D-5/P-2）：两插件实际强依赖 ffmpeg（合并/转码/提取），安装期不装、运行期报错，与 convert 的声明不一致 | 高 | 两 manifest 补 `"ffmpeg": ">=6.0"`，verify-manifests 增加"manifest 依赖声明完整性"门禁 |
| G-4 | 参数传递 | UI → bridge → CLI | 占位符空值链依赖各插件 `or 默认值` 自觉兜底（约定而非机制），新插件易漏 | 低 | tool_runner 对"缺参必填"参数（input/output）做前置校验 |
| G-5 | 状态机 | task_queue | **无集中状态机校验**（update_status 任意设置），R-2 暂停死循环即状态不一致实例 | 中 | update_status 增加合法迁移表校验 |
| G-6 | 重复逻辑 | download/_run_ytdlp vs playlist/_run_download | 两个函数 ~80 行几乎相同（进度解析/文件收集/错误收集） | 低 | 下沉到 shared/_ytdlp.py（参数化 suffix/前缀消息） |
| G-7 | 重复逻辑 | 三个插件 index.html | esc/toast/brief/cookies 状态渲染/保存路径逻辑在三份 HTML 大量复制 | 低 | 可选：生成公共 plugin-ui.js 注入（不强制，插件隔离是设计） |
| G-8 | 整体安全 | security.rs OFFICIAL_PUBLIC_KEY + manifest_fetcher | **签名验证未启用（None）**：市场清单经公共镜像先到先用（R-18），无签名时镜像可全链路伪造市场+包；发布门禁 3 已 WARN | 中 | 发布前启用签名（gen-sign-key → 公钥填 security.rs）；R-18 原始源优先 |
| G-9 | 整体安全 | protocol.rs | CSP 条件注入可绕过（R-12）+ `<header>` 误匹配（R-13）→ 插件页面网络防护存在缺口 | 中 | 见 R-12/R-13 |
| G-10 | 资源开销 | 全局 | 8 iframe 常驻 + 3s 全量轮询 + 每秒 ETA 滴答 + 每任务 PATH 子进程探测（R-19） | 低 | 见 R-8/R-19/F-4 |
| G-11 | 性能 | tool_runner | stdout 全量累积 + stderr 全量读入（R-6） | 低 | 见 R-6 |
| G-12 | 闭环完整性 | 主流程 | 提取→选择→下载→进度→完成全链路可达；取消/暂停/超时/依赖缺失/协议违规均有兜底。缺口：download 20 条静默截断（D-2）、playlist 500 条截断勾选歧义（P-3）、convert 跨类别误操作（C-4） | 低 | 见各条目 |
| G-13 | 全局配置 | 环境变量 | 配置散落（PLUGKIT_MANIFEST_URL/MIRRORS/REPO/SIGN_KEY 等）无集中管理，靠文档约定 | 低 | 当前规模可接受；新配置项时考虑集中 env 前缀约定文档 |

---

## 修复方案（提交确认后执行）

### P0 —— 必须修复（功能性 bug / 崩溃 / 安全缺口）

| 优先级 | 编号 | 位置 | 修复内容 |
|---|---|---|---|
| P0-1 | R-1 | plugin_manager.rs | 版本校验改为 `!check_bridge_version(...).unwrap_or(false)` 拒绝不兼容插件 |
| P0-2 | C-1 | plugkit-convert | extract_audio_copy 未知编码降级（先 wav copy 再 mp3），并补单测 |
| P0-3 | R-2 | tool_runner.rs | 暂停循环内先 try_wait 检测进程退出 |
| P0-4 | D-1 | download/index.html | 清空按钮清空 resultSection DOM |
| P0-5 | F-1 | Sidebar.vue | 修复拖拽排序后 click 误导航（suppressClick 保留至 click 后） |
| P0-6 | S-1 | shared/_ytdlp.py | playlist_args True 分支返回 `--yes-playlist` |

### P1 —— 建议优化（健壮性 / 性能 / 一致性）

| 优先级 | 编号 | 位置 | 修复内容 |
|---|---|---|---|
| P1-1 | G-3 / D-5 / P-2 | 两 manifest | 补 ffmpeg 依赖声明 + verify-manifests 门禁 |
| P1-2 | G-2 / R-3 / F-2 | bridge.js + tool_runner | progress 模式 invoke 取消超时（或 UI 改 submit-and-forget） |
| P1-3 | G-1 / R-10 | tool_runner | 结构化错误携带 code，前端按 code 分支 |
| P1-4 | C-2 | plugkit-convert | convert_image 失败分支统一清理占位 |
| P1-5 | R-4 | dependency_cache | 依赖下载改分阶段超时（连接/读块分离） |
| P1-6 | R-12 / R-13 | protocol.rs | CSP 无条件注入 + `<head` 排除 `<header>` |
| P1-7 | R-7 | tool_runner.rs | strip 占位符传入已提供键集合 |
| P1-8 | R-18 | manifest_fetcher | 市场清单原始源优先 |
| P1-9 | R-8 / F-4 | task_queue + stores | 任务列表增量/分页 + 空闲降频轮询 |
| P1-10 | R-6 | tool_runner | stdout 只留尾部 N 行；stderr 边读边丢 |
| P1-11 | R-14 / R-15 / R-16 | plugin_manager | 离线本地重装降级、版本解析失败放行、.old 过滤 |
| P1-12 | C-3 / C-4 / C-5 | convert | 降级路径清半成品、切分类清文件、状态聚合 |
| P1-13 | P-3 / P-4 / P-5 | playlist | 截断勾选语义、url 快照、失败状态重置 |
| P1-14 | D-2 / D-3 / D-4 | download | 截断提示、错误码分支、直播注释/实现对齐 |
| P1-15 | G-5 | task_queue | 状态机合法迁移校验 |
| P1-16 | G-6 | shared/_ytdlp.py | 抽取共用下载执行器 |
| P1-17 | R-9 | lib.rs / message_bridge | 任务控制返回实际结果 |
| P1-18 | R-11 | tool_runner | result.success 校验 |
| P1-19 | R-19 | dep_manifest | PATH 探测缓存 |
| P1-20 | R-20 / R-21 / R-22 | dependency_cache / lib.rs / tool_runner | 孤儿清理加锁、日志尾部读取、暂停前确认存活 |
| P1-21 | T-4 / T-5 | tests | 补 convert 未知编码用例 + download/playlist/shared 单测 |
| P1-22 | F-3 / F-5 | PluginPanel / LogPanel | manifest ui.html 支持、日志自动刷新 |

### P2 —— 仅美化（格式 / 命名 / 风格）

| 优先级 | 编号 | 位置 | 修复内容 |
|---|---|---|---|
| P2-1 | D-8/D-9、P-8/P-9、R-23/R-24/R-25、F-6/F-7、C-11 | 各处 | 文案对齐（150s/180s）、删死分支、current_platform 提取、entry_count 语义、sha256 大小写、图标清理、循环上限 |
| P2-2 | T-1/T-2/T-3、T-6 | scripts/tests | md5 兼容、私钥 chmod 600、门禁结构化检测、测试脚本标注 |

### 建议执行顺序

1. **第一批（P0）**：R-1 → C-1(+单测) → R-2 → D-1 → F-1 → S-1 —— 全部为确定性 bug，改动小、独立，可一次提交
2. **第二批（P1 架构一致性）**：G-3 manifest 依赖 → G-2 长任务超时 → G-1 错误码 → R-12/13 CSP → R-7 占位符 → R-18 信任链
3. **第三批（P1 健壮性/性能）**：C-2/3、R-4/6/8/9/11/14-22、P-3/4/5、D-2/3/4、G-5/6、T-4/5
4. **第四批（P2）**：文案/清理/美化
5. **收尾**：全量回归 —— 单测（convert 18+ 新用例、plugins-store）、dev 链路真实验证（提取→下载→转换→暂停/取消）、`verify-manifests.sh`、重新打包三个插件

> 注：P1-3（错误码传播）与 P1-2（长任务超时）涉及前后端契约变更，需同步改 bridge.js / 三个插件 UI 的错误渲染分支，建议作为独立提交并回归插件白名单测试。
