# PlugKit 全量模块代码审查报告

- **审查日期**：2026-08-25
- **审查范围**：全部 5 大模块（前端外壳 / Rust 后端 / 插件层 / 脚本与发布 / 测试）
- **审查方法**：阶段一（模块内专项审查，六维度）→ 阶段二（全局综合审查，九维度）
- **问题分级**：【必须修复】功能性 bug / 安全漏洞 / 崩溃；【建议优化】健壮性 / 异常处理 / 性能隐患；【仅美化】格式 / 命名 / 风格
- **状态**：本报告为审查结论 + 修复方案，**修复需经确认后执行**

---

## 一、审查范围

| 模块 | 范围 | 已读文件 |
|---|---|---|
| A. 前端外壳 | `src/` | 13 个（App.vue / MainContent.vue / Sidebar / MarketPanel / TaskPanel / LogPanel / SettingsPanel / PluginPanel / AppIcon / stores / types / main.ts） |
| B. Rust 后端 | `src-tauri/src/` | 19 个 .rs（lib / protocol / message_bridge / task_queue / task_registry / tool_runner / plugin_manager / manifest_model / manifest_fetcher / dep_manifest / dependency_cache / config_mgr / security / logger / bridge_version / registry / preinstall / main） |
| C. 插件层 | `plugins/` | 3 个 CLI（download/convert/playlist）+ 3 个 index.html + 3 个 manifest.json + shared 3 个 py |
| D. 脚本与发布 | `scripts/` + `market/` | 9 个脚本（package-plugins / verify-manifests / release / sync-plugins / dev-watch / sign-plugin / gen-sign-key / clean-plugin-cache / gen-app-icon） |
| E. 测试 | `tests/` | 4 个（test_shared_ytdlp / test_convert_cli / plugins-store.test / test_bridge fixture） |

总体评价：**代码质量高**。经过多轮打磨（错误前置、原子占位、状态机校验、Zip-Slip 防护、CSP 注入、进程组信号、三级报错链路、发布门禁等均有完整实现与注释），未发现大规模结构性问题。问题集中在：**2 处数据一致性缺口（依赖引用计数落盘、日志跨文件排序）、1 处安全纵深缺口（zip 解压无上限）、1 处跨层超时契约冲突（playlist 导出）**，以及若干健壮性/可维护性建议。

---

## 二、阶段一：模块内专项审查

### 模块 A：前端外壳（src/）

【必须修复】无

【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| A-1 | `stores/plugins.ts` mapTask | `progress.percent` 未兜底：后端若返回缺失/异常 progress 结构，TaskPanel 中 `task.progress.percent.toFixed(0)` 会运行时崩溃（模板直接调用） | 归一化为 `Number(r.progress?.percent ?? 0)`，并 clamp 0-100 |
| A-2 | `TaskPanel.vue` handleCancel/handlePause/handleResume | invoke 失败无 try/catch → unhandled rejection 且用户无反馈（如任务恰好结束再点取消） | 包 try/catch，失败 `message.error` |
| A-3 | `TaskPanel.vue` FILTERS | 筛选缺 `paused` / `interrupted` 两项（paused 任务只能靠「全部」查看） | 补充两项（STATUS_LABEL 已支持） |
| A-4 | `MarketPanel.vue` compareVersions | 预发布版本误判：`1.0.0-rc1` 与 `1.0.0` 解析为相等；带 `v` 前缀版本不剥离 → 「可更新」判断可能不准 | 改用 semver 解析（如 `semver` npm 包）或至少剥离 `v` 前缀 |
| A-5 | `MarketPanel.vue` 安装/卸载按钮 | 无 loading 态与防重复点击 → 连点产生重复安装请求（后端有版本守卫兜底，但体验差） | 按钮 `:loading` + 点击后禁用至完成 |
| A-6 | `App.vue` relayResponse | 若后端返回的 `res.id` 与请求 id 不一致（异常场景），原 `pendingInvokes` 条目不删除，靠 10 分钟定时清理兜底 | `delete` 时以实际命中为准；或后端回显恒等于请求 id（已如此，属防御性） |
| A-7 | `App.vue` postMessage | 所有转发 `targetOrigin='*'`（沙盒 iframe origin 为 opaque `null`） | 可收紧为 `'null'`（iframe contentWindow origin），降低消息被其他窗口读取的理论面 |
| A-8 | `App.vue` onUnmounted | 顶层 `window.addEventListener('message')` 未移除（根组件不卸载，风险极低） | 顺手清理，保持对称 |
| A-9 | `TaskPanel.vue` liveEta | `etaSnapshots` Map 终态任务快照不清理（7 天任务保留期内累积，量小） | 终态时 `etaSnapshots.delete(task_id)` |
| A-10 | `TaskPanel.vue` ticker | 每秒 `now.value = Date.now()` 驱动全面板重渲染 | 任务多时可改为仅 ETA 倒计时组件局部刷新（量小，可选） |
| A-11 | `stores/plugins.ts` | 模块级 `marketData` 与 `marketPlugins` ref 双缓存冗余 | 删其一 |
| A-12 | `stores/plugins.ts` | `installPlugin/uninstallPlugin` 后 `fetchMarket(true)` fire-and-forget | `await`（内部已 catch，属整洁性） |
| A-13 | `SettingsPanel.vue` | `VITE_APP_VERSION` 未注入时回退 `'0.1.0'`，可能显示与真实版本不符 | 构建时注入校验，缺失则显示「dev」 |

【仅美化】

| # | 位置 | 问题描述 |
|---|---|---|
| A-14 | `App.vue` | 事件回调用 `any` 类型（`event: any`） |
| A-15 | 各组件 | `console.error` 与 `message` 混用，无统一错误通道（量小，可选） |

---

### 模块 B：Rust 后端（src-tauri/src/）

【必须修复】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| B-1 | `dependency_cache.rs` install_dependency 缓存命中分支（L208-212） | **引用计数丢失**：缓存命中时 `increment_refcount` 只改内存 registry，**未 `registry.save()`**（新增分支有 save）。后果：插件 B 安装复用 A 已下载的依赖后，磁盘 registry 仍只有 `refs=[A]`；卸载 A 时 refcount 减到 0 → `clean_orphans` 误删仍被 B 使用的依赖 → B 下次运行重新下载（或下载失败任务失败）。**与卸载的 decrement 扫描不对称，是真实数据一致性 bug** | 命中分支 `increment_refcount` 后补 `registry.save(&self.registry_path)?`（与新增分支对称） |
| B-2 | `lib.rs` log_read 跨文件聚合（L358-380） | **日志时间顺序错乱**：多文件聚合后整体 `all_lines.reverse()`。单文件场景正确；当最近日志文件行数 < 请求行数（如今天不足 500 行）跨入昨天文件时，昨天的行被整体排到今天行之前 → 日志页展示顺序颠倒，误导排查 | 每文件内部按需倒序后再聚合，或聚合时按「文件时间戳 + 行序」排序；最简修复：`for file` 循环内先取该文件尾部 `n` 行倒序加入结果，达到 `n` 即停 |
| B-3 | `plugin_manager.rs` extract_zip（L319-371） | **解压炸弹（zip bomb）**：单文件大小、总解压大小、条目数均无上限。100MB 插件 zip（sha256 来自无签名的市场清单，镜像可投递）可膨胀为任意大小 → 磁盘填充 DoS。`std::io::copy` 无限制 | 解压时校验：单条目解压字节上限（如 50MB）、累计解压上限（如 500MB）、条目数上限（如 10000），超限即中止并删 tmp 目录 |

【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| B-4 | `dep_manifest.rs` dep_available_on_path（L154） | `sh -c "command -v {name}"` 字符串拼接：依赖名来自插件 manifest，恶意名可注入 shell 命令（插件本身即可执行任意代码，无实际提权，但属纵深防御缺口） | 参数化：`sh -c 'command -v "$1"' _ {name}` 或直接用 `Command::new(name)` 探测 |
| B-5 | `protocol.rs` CSP 注入（L116） | `<head>` 标签属性值含 `>` 时（如 `<head class="a>b">`），`find('>')` 定位到属性内 `>` → meta CSP 注入到属性值中间 → CSP 失效（HTML 解析异常） | 用正则匹配 `<head[^>]*>` 完整标签后注入，或先剥离属性扫描 |
| B-6 | `protocol.rs` CSP img-src | `img-src plugkit: data: https:` 允许任意 https 图片外联 → 不可信插件可经 `<img>` 侧信道向外部发请求，弱化 `connect-src 'none'` 的禁网意图 | 若可接受则文档化该权衡；否则收紧为仅 `plugkit: data:` |
| B-7 | `plugin_manager.rs` uninstall（L221-224） | `let _ = dc.decrement_refcount_for_plugin(...)` 与 `let _ = dc.clean_orphans()` 吞错 → 依赖清理失败静默（孤儿依赖残留磁盘） | 至少 `log::error`，可选透出 warnings |
| B-8 | `tool_runner.rs` substitute_args（L627-636） | 参数值二次替换污染：params 含 `{input: "x{output}", output: "y"}` 时，`{output}` 先被替换后，input 值里的字面 `{output}` 又被 output 值污染 | 占位符替换改为一次遍历（收集所有 `{key}` 后一次性替换，替换值不再参与后续替换） |
| B-9 | `manifest_fetcher.rs` fetch_single_manifest | 市场清单无大小上限（远端可返回超大 JSON 全量读内存） | 检查 Content-Length 或设置读取上限（如 10MB） |
| B-10 | `security.rs` OFFICIAL_PUBLIC_KEY=None | **供应链信任锚缺失**：市场清单（含 sha256）无签名验证，GitHub Raw / 镜像被攻破即可全链路替换插件包。已知 G-8 待办，release.sh 有 WARN 门禁 | 发布前启用签名（gen-sign-key.sh 生成 → 公钥填入 security.rs → 打包自动签名） |
| B-11 | `lib.rs` open_in_folder（L124-130） | 相对路径 `"foo"` 的 parent 为 `""` → `open_path("")` 报错但消息不友好；`~` 单独（无 `/`）不展开 | 补空路径/相对路径的明确报错 |
| B-12 | `lib.rs` read_log_tail | 只回读尾部 256KB，超长单行/超大日志被截断 | 记录已知限制即可（日志页 2000 行上限内基本覆盖） |
| B-13 | `config_mgr.rs` | IO/解析错误全部映射为 `PathOutsideRoot`（错误语义误用） | 引入 `IoError` 变体或改 `anyhow` 风格 |
| B-14 | `task_queue.rs` save() | `queue.save().unwrap_or(())` 落盘失败静默 | 失败时 `log::error`（高频路径仅状态迁移时落盘，代价可接受） |
| B-15 | `tool_runner.rs` substitute_args | `{platform}` 判断逻辑与 `dep_manifest::current_platform()` 重复（3 处 platform 字符串判断） | 复用 `current_platform()` |
| B-16 | `lib.rs` dialog_open_file_inner | `blocking_pick_files` 阻塞 tokio worker 线程（模态设计，可接受） | 记录即可 |

【仅美化】

| # | 位置 | 问题描述 |
|---|---|---|
| B-17 | `plugin_manager.rs` L380 | `validate_extracted_paths` 与 `install_dependencies` 之间无空行 |
| B-18 | `manifest_model.rs` | `impl Manifest` 拆成两块（has_ui / tool_dir）可合并 |
| B-19 | `lib.rs` get_home_dir | 失败返回空字符串（极端场景，调用方拼接出畸形路径） |

---

### 模块 C：插件层（plugins/）

【必须修复】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| C-1 | `playlist/tool/plugkit-playlist` export + `manifest.json` export 命令 | **跨层超时契约冲突**：CLI 内 `subprocess.run(timeout=1800)`（30 分钟，注释「全量提取可长达 30 分钟」），但该命令在 manifest 中声明 `"output": "json"` → ToolRunner 对 json 模式施加 **300s 硬超时**。大列表（>几百集）全量导出必被外壳在 5 分钟杀掉，CLI 的 30 分钟设置形同虚设 | 二选一：① export 命令改 `"output": "progress+json"`（外壳不设超时，符合长任务语义）；② 将 CLI 超时降到 300s 内并明确告知用户限制。推荐 ①，与 download 长任务一致 |

【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| C-2 | `shared/_ytdlp.py` _ensure_service | 首次调用 `pip install yt-dlp`（最长 300s 阻塞）在任务执行路径上：download/playlist 首次解析可能等待 1-2 分钟无反馈 | 首次走后台预热（异步拉起），或解析前先输出「正在准备解析服务…」进度 |
| C-3 | `download/tool/index.html` L453、`playlist/tool/index.html` L418/L463 | `r.output_dir` / `r.format.toUpperCase()` 在 `r` 为 null/undefined 时 TypeError（后端成功但 data 为 null 的边界） | 改 `r && r.output_dir`，`r.format` 判空 |
| C-4 | `download/tool/index.html` L301 | 去重提示用字符串前缀 `'该链接'` 匹配后端文案（跨层字符串耦合，后端文案变更即失效） | 后端错误已带 `[CODE]` 前缀，UI 改解析 code（如 `DUP_EXTRACT`） |
| C-5 | `shared/_ytdlp_service.py` serve_conn | 每连接一线程无上限（单客户端低并发，可接受） | 记录即可 |
| C-6 | `download/tool/plugkit-download` L262 | `"Error" in line or "error" in line` 会把文件名含 error 的行误收进诊断（轻微） | 收窄为行首 `ERROR:` / 常见 yt-dlp 错误前缀 |
| C-7 | `playlist/tool/plugkit-playlist` _unique_path | 导出防覆盖非原子（并发导出同路径可能碰撞，低概率） | 可选复用 convert 的 `_reserve_path` O_EXCL 方案 |
| C-8 | 三个插件 manifest | `dependencies` 的版本约束（`">=2024.01.01"` 等）从未被 Rust 端解析使用（dep_manifest 为 pinned 版本）——**声明的语义与实现不符** | 删约束改存在性声明（`"ffmpeg": ""`），或实现约束解析（后者成本高，推荐前者） |

【仅美化】

| # | 位置 | 问题描述 |
|---|---|---|
| C-9 | `convert/tool/plugkit-convert` L36-37 | 模块顶部双空行 |
| C-10 | download/playlist UI | toast/esc/brief/cookies 状态渲染代码重复（插件独立打包，共享成本高，可接受） |

---

### 模块 D：脚本与发布（scripts/）

【必须修复】无

【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| D-1 | `gen-app-icon.mjs` L21 | 硬编码 `/Users/main/.workbuddy/binaries/node/workspace/node_modules/sharp` 绝对路径 → 换机器/用户即失效 | 优先 `process.env.NODE_PATH` 或项目内依赖，最后回退提示安装 |
| D-2 | `release.sh` L45 | 公钥配置检测用字符串匹配源码（`'OFFICIAL_PUBLIC_KEY:Option<&str>=None;' not in ...`）→ 格式/注释变化即误判 | 解析 `Some(`/`None` 判定，或读取编译产物 |
| D-3 | `release.sh` L84 | `$TAG` 直接拼 JSON body（含引号破坏 JSON） | 用 python3/jq 构造 JSON |
| D-4 | `dev-watch.sh` L29 | `xargs sh -c` 拼接文件名（含引号/特殊字符文件名破坏统计） | 插件文件名可控（.py/.html/json），可接受；如需稳妥改 find -print0 |

【仅美化】

| # | 位置 | 问题描述 |
|---|---|---|
| D-5 | `package-plugins.sh` | tag 参数未校验（verify-manifests 门禁兜底 `//download//` 坏 URL） |

---

### 模块 E：测试（tests/）

【必须修复】无

【建议优化】

| # | 位置 | 问题描述 | 修复建议 |
|---|---|---|---|
| E-1 | Rust 侧（全局） | **无 Rust 集成/单元测试覆盖任务系统与依赖缓存**：`lib.rs` 暴露了 `api` facade（PluginManager / DependencyCache / TaskQueue）但 tests/ 下无 Rust 测试。B-1（引用计数落盘）正是无测试盲区 | 补：`task_queue` 状态机迁移表、`dependency_cache` 引用计数增删对称、`plugin_manager` 安装链路（mock 下载）单测 |
| E-2 | download/playlist CLI | 无单元测试（仅 convert 有 18 例） | 补 `_build_info_result` / `_build_list_result` / `export` CSV 构造的纯函数测试 |
| E-3 | `tests/unit/plugins-store.test.ts` | 仅覆盖 mapTask（轮询/缓存逻辑未测） | 可选补 fetchTasks/market 缓存 TTL 单测 |
| E-4 | `tests/integration/test_bridge.py` | 实为测试插件 fixture 而非测试文件，命名/位置误导 | 移入 `tests/fixtures/` 或文件头注明用途 |

【仅美化】

| # | 位置 | 问题描述 |
|---|---|---|
| E-5 | `tests/integration/test_bridge.py` | 位置与命名（见 E-4） |

---

## 三、阶段二：全局综合审查

**整体评估**：架构分层清晰（外壳零业务逻辑 / 插件 CLI+iframe / 共享 SDK），模块间契约基本一致，错误码三级链路闭环（插件 UI 简要 → 任务中心 → 结构化日志），生命周期管理完整（任务队列 → 进程组信号 → 引用计数 → 孤儿清理 → 启动恢复）。全局层面有 4 项需协调的交互问题与若干一致性建议。

### 全局问题清单

| # | 维度 | 位置 | 问题描述 | 风险等级 | 修复建议 |
|---|---|---|---|---|---|
| G-1 | 模块间调用 | playlist export ↔ tool_runner 超时 | CLI 1800s 超时与外壳 json 模式 300s 超时冲突（同 C-1） | **必须修复** | export 改 progress+json 输出模式 |
| G-2 | 模块间调用 | message_bridge.rs L99 | `probe` 命令名硬编码在外壳（任何插件的 probe 都即时执行不进任务中心）——命令语义耦合外壳 | 建议优化 | 移至 manifest 命令属性（如 `"output": "probe"`），或加注释固化契约 |
| G-3 | 参数传递 | 插件 UI ↔ 后端错误码 | download UI 用字符串前缀 `'该链接'`/`'完全磁盘访问'` 匹配后端文案（同 C-4）；playlist UI 同样匹配 `'完全磁盘访问'` | 建议优化 | 统一改为解析后端 `[CODE]` 前缀判断场景 |
| G-4 | 状态流转 | task_queue ↔ task_registry | cancel/pause/resume 先操作注册表再改状态，存在竞态窗口（进程刚结束时取消 → 已完成任务显示 Cancelled，`can_transition` 拦截后者但无法回滚） | 建议优化 | 语义可接受（用户确实点了取消），补注释即可 |
| G-5 | 错误传播 | plugin_manager uninstall / task_queue save | 多处 `let _ =` 吞错（依赖清理失败、任务落盘失败）静默 | 建议优化 | 统一补 `log::error`（落盘/清理类失败必须可见） |
| G-6 | 全局配置 | `~/.plugkit` 路径散落 | 数据目录/日志目录/缓存目录/tmp socket 路径分散在 7+ 处（Rust 6 处 + Python 2 处），无集中常量 | 建议优化 | Rust 侧建 `paths.rs` 常量模块，shared 建 `_paths.py`；未来便携模式只需改一处 |
| G-7 | 重复逻辑 | tool_runner.rs substitute_args / dep_manifest.rs / plugin_manager.rs | platform 字符串判断重复 3 处 | 建议优化 | 统一复用 `dep_manifest::current_platform()` |
| G-8 | 重复逻辑 | 插件 UI | toast/esc/brief/cookies 渲染在 download/playlist 重复（convert 用 status） | 仅美化 | 可经 plugkit:// 引入公共 UI JS（成本高，接受现状） |
| G-9 | 整体安全 | security.rs / 市场清单 | 供应链信任锚缺失（OFFICIAL_PUBLIC_KEY=None，sha256 来自无签名清单）（同 B-10） | 建议优化（发布前必做） | 发布前启用 Ed25519 签名（脚本已齐备） |
| G-10 | 整体安全 | plugin_manager extract_zip / protocol | zip bomb（B-3）+ CSP 注入边界（B-5）+ img 外联（B-6）为威胁面纵深缺口 | 必须修复（B-3） | 见各条目 |
| G-11 | 整体资源 | ToolRunner 信号量 | 全局 5 并发对所有插件生效（跨插件共享），符合预期；插件 UI 各持 ≤5 解析并发池 → 实际仍受全局 5 约束 | 建议优化 | 记录即可（现状即全局受限） |
| G-12 | 整体资源 | _ytdlp_service | 常驻解析服务内存约 250MB，空闲 120s 退出；download/playlist 共用 socket，多插件不重复拉起 | 通过 | 无需处理 |
| G-13 | 闭环完整性 | 主流程 | 安装→依赖→iframe→invoke→进度→错误→控制→卸载闭环完整；唯一破坏点是 B-1（引用计数不落盘）与 C-1（导出超时） | 必须修复 | 见 B-1 / C-1 |
| G-14 | 闭环完整性 | 启动恢复 | recover() + tmp/work 清理 + download.tmp 清理 + 日志 14 天轮转齐全 | 通过 | 无需处理 |

---

## 四、完整修复方案（提交确认后执行）

### P0 —— 必须修复（4 项，直接影响正确性/安全）

| # | 对应条目 | 修复内容 | 涉及文件 | 工作量 |
|---|---|---|---|---|
| P0-1 | B-1 | 依赖缓存命中分支补 `registry.save()`（引用计数增删对称闭环） | `src-tauri/src/dependency_cache.rs` | 小 |
| P0-2 | B-2 | log_read 跨文件日志顺序修正（每文件内倒序聚合） | `src-tauri/src/lib.rs` | 小 |
| P0-3 | B-3 | zip 解压增加单文件/总量/条目数上限，超限中止并清理 | `src-tauri/src/plugin_manager.rs` | 中 |
| P0-4 | C-1 / G-1 | playlist export 改 progress+json 输出模式（消除跨层超时冲突），同步 CLI 进度输出 | `plugins/playlist/manifest.json`、`plugins/playlist/tool/plugkit-playlist` | 小 |

### P1 —— 建议优化（择期批量执行）

| # | 对应条目 | 修复内容 | 涉及文件 |
|---|---|---|---|
| P1-1 | A-1 / A-2 / A-3 | mapTask progress 兜底；任务控制按钮错误反馈；筛选补 paused/interrupted | `src/stores/plugins.ts`、`src/components/TaskPanel.vue` |
| P1-2 | B-4 | dep_available_on_path 参数化探测（消除 shell 拼接） | `src-tauri/src/dep_manifest.rs` |
| P1-3 | B-5 / B-6 | protocol CSP 注入边界加固；img-src 收紧或文档化 | `src-tauri/src/protocol.rs` |
| P1-4 | B-7 / G-5 | 卸载/落盘失败统一 log::error | `plugin_manager.rs`、`task_queue.rs` |
| P1-5 | B-8 | substitute_args 一次性替换（防参数值二次污染） | `src-tauri/src/tool_runner.rs` |
| P1-6 | G-3 / C-4 | 插件 UI 错误场景判断改解析 [CODE] 前缀 | download/playlist index.html |
| P1-7 | C-3 | 插件 UI 对 `r` 判空 | download/playlist index.html |
| P1-8 | G-6 / G-7 | 路径常量集中（paths.rs）+ platform 判断复用 | Rust 侧 + shared |
| P1-9 | C-8 | manifest dependencies 约束语义澄清（删未解析约束） | 3 个 manifest.json + verify-manifests 门禁同步 |
| P1-10 | D-1 / D-2 / D-3 | 图标脚本路径去硬编码；release 公钥检测与 JSON 构造加固 | scripts |
| P1-11 | E-1 / E-2 | 补 Rust 任务/依赖缓存单测 + download/playlist CLI 纯函数测试 | tests/ |
| P1-12 | C-2 | 解析服务首次 pip install 阻塞提示 | shared/_ytdlp.py |
| P1-13 | B-10 / G-9 | 发布前启用签名（操作项，非代码） | 发布流程 |

### P2 —— 仅美化（随代码修改顺带处理）

A-14/A-15、B-17/B-18/B-19、C-9、D-5、E-4/E-5 等格式与命名问题，修复 P0/P1 时顺带清理，不单独排期。

### 执行顺序建议

1. **P0 先行**：4 项均为小而集中的改动，改完跑 `cargo test` + `npm run test` + `./scripts/verify-manifests.sh` 回归；
2. **P1 分批**：按「后端一致性 → 前端健壮性 → 插件契约 → 脚本/测试」四批推进，每批独立验证；
3. **验证清单**：P0-4 需真实链路验证（导出大列表不被 5 分钟杀掉）；P0-1 需验证「装 A 装 B → 卸 A → B 仍可用且缓存不误删」。

---

## 五、修复执行记录（2026-08-25 执行完毕，全量验证通过）

用户确认后 P0/P1/P2 全部执行。验证：`cargo check` 零警告、`cargo test` 全过（lib 20 + plugin_e2e 2 + tool_runner_e2e 1）、pytest 43 passed（新增 download 8 + playlist 4）、vitest 3、vue-tsc 零错误、verify-manifests 通过、重打包 + dev 副本已同步。

| 项 | 状态 | 修复说明 |
|---|---|---|
| P0-1 (B-1) | ✅ | `dependency_cache.rs` 缓存命中分支补 `registry.save()`；新增 `with_dir()` 测试构造 + 引用计数对称性单测 |
| P0-2 (B-2) | ✅ | `lib.rs` log_read 每文件内倒序聚合，消除跨文件顺序颠倒 |
| P0-3 (B-3) | ✅ | `plugin_manager.rs` extract_zip 单条目 64MB / 累计 512MB / 条目 10000 上限，流式计数 |
| P0-4 (C-1) | ✅ | playlist export 改 progress+json（外壳不设超时）+ CLI 进度上报 |
| P1-1 (A-1/2/3) | ✅ | mapTask percent 兜底+clamp；任务控制按钮错误反馈；FILTERS 补 paused/interrupted |
| P1-2 (B-4) | ✅ | dep_available_on_path 参数化 `command -v "$1"` |
| P1-3 (B-5/6) | ✅ | CSP 注入改 `find_tag_end`（跳过属性引号）；img-src 收紧为 `plugkit: data:` |
| P1-4 (B-7/G-5) | ✅ | 卸载/clean_orphans 失败 log；task_queue 落盘失败 `save_log` 显式记录 |
| P1-5 (B-8) | ✅ | `substitute_arg_once` 一次性替换（消除参数值二次污染）+ 平台串复用 `current_platform()`（G-7）+ 新单测 |
| P1-6 (G-3) | ✅ | dedup 文案改 `[DUP_EXTRACT]` 错误码；UI 错误场景判断改错误码（保留文案兜底） |
| P1-7 (C-3) | ✅ | download/playlist UI 对 `r` 判空 |
| P1-8 (G-6) | ✅ | 新建 `paths.rs` 路径常量集中，替换 9 处散落路径 |
| P1-9 (C-8) | ✅ | 三 manifest `dependencies` 约束改存在性声明（值置空，Rust 端从未解析约束） |
| P1-10 (D-1/2/3) | ✅ | gen-app-icon 去硬编码（SHARP_PATH 回退链）；release 公钥检测正则化；JSON body python 构造 |
| P1-11 (E-1/2) | ✅ | 新增 task_queue 状态机/创建、dependency_cache 引用计数单测；download/playlist CLI 单测 |
| P1-12 (C-2) | ✅ | 首次 venv 创建发 `progress` 提示（约 30s 不误判卡死） |
| P1-13 (B-10) | ⏸️ | 发布前启用签名——操作项，未改代码（release.sh 已有 WARN 门禁） |
| P1-14 (新增) | ✅ | **下载复用 venv pip 版 yt-dlp**：`ytdlp_binary()` 优先 venv（冷启动 22s→0.4s），Windows/venv 缺失回退二进制；`YTDLP_VERSION` pin 与 dep_manifest 对齐（实测 venv 已为 2026.08.19） |
| P2 | ✅ | App.vue 事件类型化；plugin_manager 空行；manifest_model impl 合并；convert 空行；test_bridge 用途注明 |
| 新发现 | ✅ | playlist `_build_list_result` 引用未定义变量 `url`（条目无 url 时 NameError）——已修复（加默认参数）+ 单测覆盖 |

**修正**：报告原 E-1 称「Rust 侧无集成测试」不准确——`src-tauri/tests/` 实际有 plugin_e2e / tool_runner_e2e / market_remote_e2e 三个 e2e；真正缺口是 task_queue / dependency_cache 的单元测试（已补）。

**遗留说明**：
- P1-13（签名启用）为发布前操作项，涉及密钥管理，按你的节奏执行；
- manifest 依赖约束置空后，Rust 端不解析约束的事实与声明一致（存在性声明）；未来若需版本门槛，应在 dep_manifest 层实现（非 manifest 字符串）。

---

## 追加：方向级调整执行记录（2026-08-25）

### 1. 方向定案（归零审查后）
放弃第三方插件生态，工具全部官方维护；**工具级热更新为主**（工具箱红点 + 一键更新），外壳变更才发整包；应用内一键覆盖更新（updater）暂不做，仅「检查更新 + 链接 GitHub」告知。对应 commits：`fb33e8c`（工具箱改名/红点/check_app_update）、`06c9943`（new-plugin 脚手架 + package-plugins 自动发现）。

### 2. 解析服务已删除（commit `eef0071`）
本报告 C-2/C-5/G-12 涉及 `_ytdlp_service` 的条目随本次删除而失效（C-2 的 progress 提示逻辑已迁移至 `_ensure_ytdlp_venv`）。删除依据为 ROI 实测（冷解析直连 1.69s vs 服务 1.57s，仅差 0.1s，网络主导；250MB 常驻换"同 URL 重复解析 0s"边缘场景不划算）。

- 删除 `plugins/shared/_ytdlp_service.py`；`_ytdlp.py` 移除服务客户端，venv 创建迁移 `_ensure_ytdlp_venv`（`ytdlp_binary()` venv 缺失时自动创建 + progress 提示，失败回退二进制）
- 顺带修复隐藏 NameError：`ytdlp_binary()` 回退分支引用 `dep` 但 `_ytdlp.py` 未导入（venv 存在时未暴露）
- download/playlist CLI 移除服务分支，解析/下载统一直连 venv pip 版
- 验证：pytest 43 passed / verify-manifests 通过 / 重打包 + dev 副本同步无残留

*报告完毕。修复执行完成，可进入验收（真实链路验证建议：大列表导出不被 5 分钟杀掉；装 A 装 B → 卸 A → B 缓存不误删）。*
