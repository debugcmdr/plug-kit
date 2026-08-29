# PlugKit 三阶段全面代码审查报告（2026-08-29）

审查范围：src-tauri/src（18 模块）、src（Vue 外壳）、plugins（download/convert/playlist + shared）、scripts（发布门禁）、tests。
分级：**【必须修复】** 功能性 bug / 安全漏洞 / 崩溃风险；**【建议优化】** 健壮性/异常/性能隐患；**【仅美化】** 格式命名风格。
本次实际落盘修复 3 处（标记 ✅ 已修复），其余为未采纳建议（阶段三列出）。

---

## 阶段一：分模块专项审查

### M1 任务中心（task_queue.rs + task_registry.rs）— 评估：优
状态机迁移表（G-5）、锁内原子查重（防 TOCTOU）、7 天保留、崩溃恢复、落盘失败显式日志。质量高。

- 【建议优化】URL 去重未规范化：`create_with_dedup` 按原样比较，`https://a.com` 与 `https://a.com/` 视为不同链接（去重仅针对解析类命令，download 类不去重，影响面小）。
- 【建议优化】`update_status_with_error` 的 `started_at` 在 Running 时覆盖更新（重复 Running 幂等迁移会刷新启动时间，仅展示影响）。

### M2 执行引擎（tool_runner.rs）— 评估：优
进程组 SIGSTOP/SIGCONT、杀进程树（SIGTERM→200ms→SIGKILL）、取消恒检查、并发信号量排队可取消、依赖 PATH 注入、UTF-8 安全占位符替换（B-8/R-7 单测覆盖）。

- ✅【必须修复】json 模式命令超时统一标 `Cancelled`：超时≠用户取消，任务中心显示"已取消"误导。修复：超时/等待异常按 `Failed` 带原因标记，仅 `"Task cancelled"` 标 Cancelled。
- 【建议优化】`lines.iter().any(|l| l.contains("\"type\":\"error\""))` 字符串包含判断脆弱（progress 行 message 含该字样会误判已输出 error 行）。改为解析最后一行类型判断：

```rust
let has_error_line = lines.last().map_or(false, |l| {
    serde_json::from_str::<PluginOutput>(l).map_or(false, |o| matches!(o, PluginOutput::Error(_)))
});
```

### M3 插件管理（plugin_manager.rs + preinstall.rs + security.rs）— 评估：优
原子换装（.old 回滚）、Zip-Slip 双保险、解压炸弹上限（64MB/512MB/1万条目）、签名校验预留、预装桥接版本校验。

- 【建议优化】`extract_zip` 目录条目 strip 失败：zip 内 `convert/` 目录条目 `trim_end_matches('/')` 后为 `"convert"`，`strip_prefix("convert/")` 不匹配 → 残留空目录 `dest/convert`（不影响 manifest.json 定位，官方包无此问题）。修复：目录分支先 strip 再判空：

```rust
if file.name().ends_with('/') {
    let dir_rel = entry_rel.trim_end_matches('/');
    if dir_rel.is_empty() { continue; }
    fs::create_dir_all(dest.join(dir_rel)).map_err(|e| e.to_string())?;
}
```

- 【建议优化】目录判定用 `file.name()`（原始名，恶意包用 `dir\\` 反斜杠目录名会被当文件创建，随后子条目 `create_dir_all` 失败 → 安装中止）。防御性，可改用 `raw_name` 判定。

### M4 依赖管理（dependency_cache.rs + dep_manifest.rs + registry.rs + manifest_fetcher.rs）— 评估：优
流式下载+增量 sha256+1GiB 硬上限、镜像串行降级+会话黑名单（R-18）、引用计数对称（B-1）、PATH 探测参数化防注入（B-4）+30s 缓存、registry 备份轮转。

- 【建议优化】`find_cached_any_version` 与 `find_from_registry` 扫描逻辑重复，可合并为单一 `find_cached(name, platform)`（内部先 registry 后目录兜底）。
- 【建议优化】registry.json 损坏且备份也损坏时静默回退空 registry → 引用关系丢失，已装依赖变"孤儿"（卸载不回收，磁盘占用）。可加一条 `log::warn` 提示。

### M5 桥接层（message_bridge.rs + config_mgr.rs + protocol.rs + lib.rs）— 评估：优
命令白名单缓存（mtime 失效）、task_id 归属校验（G-4）、probe 特快通道、plugkit:// CSP 无条件注入（R-12）+ head 精确匹配（R-13）+ 引号感知 tag 结束（B-5）、traversal 防护。

- ✅【必须修复】插件传入 task_id 时任务不进任务中心（详见阶段二 G-1，跨模块交互问题在此层修复）。
- 【建议优化】protocol.rs traversal guard 漏检 `"a/.."` 形式（`contains("/../")` 不匹配末尾 `..`；实际被 `full.is_file()` 404 兜底，无越权）。补一条：

```rust
if normalized_rel.split('/').any(|c| c == "..") { /* forbidden */ }
```

- 【建议优化】config_mgr `set` 无锁读-改-写：两插件并发 set 不同 key 丢失更新（低频，插件级配置）。

### M6 前端外壳（App.vue + stores/plugins.ts + 组件）— 评估：优
v-show 常驻 iframe（KeepAlive 对 iframe 无效的规避）、消息来源身份校验（e.source ∈ 已挂载 iframe）、任务轮询活跃 3s/空闲 15s 降频、拖拽排序（blur 兜底 + suppressClick 双保险）。

- 【建议优化】`pendingInvokes` 以插件自报 id 为 key：跨插件同 id（fallback `'id-'+Date.now()` 同毫秒或随机碰撞）时后置覆盖，响应串台到后注册插件。key 改为 `\`${sourcePlugin}:${id}\``（relayResponse 同步拼接查询）。
- 【建议优化】TaskPanel `etaSnapshots` Map 无上限清理（任务 7 天保留期内增长可控，低风险）。

### M7 download 插件 — 评估：优
分类下载/播放列表/cookies/文件导入（txt/csv/xlsx）、`_run_ytdlp` 流式进度+8 行诊断尾巴。

- 【建议优化】视频 selector 的 `h = re.sub(r"\D", "", quality)`：note 型 label（"格式 137"）误提取数字 → `bestvideo[height<=137]` 错误 selector；"720p (AV1)" 类提取出 "7201" 也会误导。改为仅当 label 匹配 `^\d{3,4}p$` 才提取，否则回退 best：

```python
m = re.match(r"^(\d{3,4})p$", (quality or "").strip())
if m and int(m.group(1)) > 0:
    h = m.group(1)
    fmt = f"bestvideo[height<={h}]+bestaudio/best[height<={h}]/best"
else:
    fmt = "bestvideo+bestaudio/best"
```

### M8 convert 插件 — 评估：优
原子占位防覆盖（O_EXCL）、编码器+封装器双检测置灰、copy 失败降级转码、质量档位映射（JPG q 值/AVIF/HEIC）、AV1 分支（无 -still-picture 坑）。

- 【建议优化】`resolve_output` 的 `out == inp` 用 `os.path.abspath` 字符串比较：macOS 大小写不敏感/软链路径可能漏判"同文件"。可用 `os.path.samefile` 兜底：

```python
try:
    same = os.path.samefile(inp, out)
except OSError:
    same = False
if same:
    d = os.path.dirname(out)
    base = os.path.splitext(os.path.basename(out))[0]
    ext = os.path.splitext(out)[1]
    return _reserve_path(os.path.join(d, f"{base}_converted{ext}"))
```

### M9 playlist 插件 + shared — 评估：优
flat 提取/批量下载/导出 CSV（BOM + 公式注入防护 + 固定 schema）、`_protocol`/`_ytdlp` 单点源码双路径导入、错误分类表精确（412 优先于 403）。

- 【建议优化】`_ensure_ytdlp_venv` 只校验 venv 存在，不校验已装 yt-dlp 版本（venv 被污染/旧版不重建）。可改为读取 `pip show yt-dlp` 版本比对。
- 【建议优化】`PIP_MIRROR` 硬编码清华源，无环境变量覆盖（境外用户慢）。可 `os.environ.get("PLUGKIT_PIP_MIRROR", PIP_MIRROR)`。

---

## 阶段二：全局综合审查

整体评估：架构一致（零业务外壳、插件=CLI 子进程+iframe、三级报错链路、依赖共享缓存+引用计数），模块边界清晰，安全基线扎实（CSP/zip-slip/进程组/校验和）。核心矛盾集中在**任务生命周期的一致性**。

### G-1 ✅【必须修复】任务中心可见性不一致（download/playlist 下载任务不可见）
- 现状：convert 转换不传 task_id → 后端建任务 → 任务中心可见可取消；download/playlist 下载传自生成 task_id → 后端不建任务 → 任务中心不可见、无法取消，而 UI 文案（download "可在任务中心查看进度"、convert "进度/倒计时在任务中心查看"）承诺可见。bridge.js 注释亦声称"MT.invoke 自动建任务"。
- 根因：插件需自管 task_id 做进度过滤（后端不返回创建的 id），代价是绕开任务中心。
- 修复（最小、交互层调整，未动业务逻辑）：`message_bridge::invoke_cli` 的 `Some(id)` 分支——归属校验后若 id 未登记，用该 id 创建任务记录：

```rust
Some(id) => {
    if let Some(owner) = crate::task_queue::TaskQueue::owner_of(id).await {
        if owner != plugin_id { return Err(...); }
    } else {
        crate::task_queue::TaskQueue::create_with_id(
            plugin_id, command_name, extract_url(&params), id,
        ).await;
    }
    id.to_string()
}
```

`TaskQueue::create_with_id`（锁内 `contains_key` 幂等插入）。任务中心现可查看/取消 download/playlist 下载任务，进度事件 task_id 不变（= 插件 id），前端过滤不受影响。

### G-2 ✅【必须修复】进度字段整体覆盖导致进度回跳 0%
- 现状：`read_stdout_protocol` 把 `percent=None` 转 `0.0`，`update_progress` 整体替换 `TaskProgress`。playlist 批量下载切换集数时报 `progress(0, message=...)` → 任务中心进度从 x% 回跳 0%，speed/eta 被清空。
- 修复：`update_progress` 改为字段级更新（`percent: Option<f64>`，None 保留旧值；speed/eta/message/file_name/output_path 同语义），调用方两处适配。行为变化仅限"插件缺字段时不再覆盖为 0/空"。

### G-3 【建议优化】市场信任链：镜像返回的清单/插件包无签名验证（OFFICIAL_PUBLIC_KEY=None）
原始优先串行（R-18）已缓解篡改风险，但发布前启用 Ed25519 签名仍应推进（release.sh 门禁 3 已警告，属发布待办而非代码缺陷）。

### G-4 【仅美化】`open_permission_settings` 为 async 但无 await（clippy unused_async），可去 async 或加 `#[allow]`。

### G-5 闭环完整性核对（通过）
状态机终态冻结、fail_task 统一处理前置校验失败、取消先杀进程树再标状态、依赖不可用即失败、插件无输出/协议违规明确报错落日志——无悬空 Pending、无孤儿进程、无静默吞错。

---

## 阶段三：回归校验

对照 Demo 业务预期（零业务逻辑外壳、插件市场安装/卸载、任务中心进度/取消/暂停/恢复、下载/转换/播放列表、共享依赖自动获取、三级报错链路），逐项确认：

| Demo 业务预期 | 校验结果 |
|---|---|
| 任务创建/状态流转/7 天清理/崩溃恢复 | ✅ 未变（状态机与持久化逻辑零改动） |
| 取消/暂停/恢复真正作用于子进程 | ✅ 未变（task_registry 与进程组信号零改动） |
| 下载/转换/播放列表提取与进度上报 | ✅ 未变（三插件 CLI 零改动，进度事件形状不变） |
| 插件市场安装/卸载/版本守卫/预装 | ✅ 未变（plugin_manager/preinstall 零改动） |
| 依赖自动获取（PATH→缓存→镜像下载+校验） | ✅ 未变（dependency_cache/dep_manifest 零改动） |
| 前端任务轮询/面板/日志/设置 | ✅ 未变（前端零改动） |

### 本次实际改动清单（3 处，均 Rust 后端）

1. `src-tauri/src/task_queue.rs` — 新增 `create_with_id`；`update_progress` 改字段级更新（签名 `percent: f64` → `Option<f64>`）。
2. `src-tauri/src/tool_runner.rs` — 两处 `update_progress` 调用适配 Option；`killed` 分支区分取消/超时（超时标 `Failed` 带原因）。
3. `src-tauri/src/message_bridge.rs` — `Some(id)` 分支未登记 id 时 `create_with_id` 建任务记录。

### 潜在风险点标记

- **R1（G-1）行为变化**：download/playlist 下载任务此后**进入任务中心**（此前不显示）→ 任务列表更长（7 天保留自动清理）；这是与 UI 文案/Demo 预期对齐的有意变更。边界：插件复用同一 task_id 再提交（uuid 每次新生成，实际不发生）时 `create_with_id` 幂等跳过、ToolRunner 对终态任务的状态迁移被状态机静默忽略——不产生错误状态。
- **R2（G-2）语义变化**：插件 progress 行缺字段时不再覆盖旧值。全插件现有调用均传显式值，无"传 None 表示清空"场景，无回归。
- **R3 展示变化**：json 模式超时任务由"已取消"变为"失败(超时原因)"，语义更准确，任务中心无逻辑依赖"取消"文案。

### 验证证据

- `cargo test --lib`：21 passed（含状态机/占位符/引用计数/校验和/URL 去重规范化回归）。
- `pytest tests/unit/`：44 passed（convert 18 + download 9/playlist/shared 17，含 video_format_selector 3 例）。
- `vue-tsc --noEmit`：0 错误；`verify-manifests.sh`：共享模块与源码一致。
- 未采纳建议（J/K/L/M/N 五项）未落盘，后续如需可逐项推进。

---

## 追加：建议修复执行记录（用户确认后执行，A~J 共 10 项）

| # | 文件 | 改动 | 验证 |
|---|---|---|---|
| A | tool_runner.rs | error 行判断由字符串包含改为解析最后一行类型（防 progress 消息误判） | cargo ✓ |
| B | protocol.rs | traversal guard 逐段拒绝 `..`（覆盖 `a/..` 末尾形式，不再误伤 `..x`） | cargo ✓ |
| C | App.vue | pendingInvokes key 改为 `${pluginId}:${id}`（防跨插件同 id 串台） | vue-tsc ✓ |
| D | plugkit-download + 测试 | 新增 `video_format_selector`，仅接受 `720`/`720p` 形式，note 型 label 回退 best | pytest ✓ |
| E | plugkit-convert | `resolve_output` 增加 `os.path.samefile` 兜底（macOS 大小写不敏感/硬链接同文件） | pytest ✓ |
| F | registry.rs | registry 与备份全损坏时 `log::warn` 显式告警 | cargo ✓ |
| G | _ytdlp.py | `PIP_MIRROR` 支持 `PLUGKIT_PIP_MIRROR` 环境变量覆盖 | pytest ✓ |
| H | plugin_manager.rs | zip 目录条目 strip 后为空则跳过（消除残留空目录）；目录判定改用归一化名（反斜杠目录名不再误判为文件） | cargo ✓ |
| I | _ytdlp.py | 新增 `_venv_matches_pinned`，venv 内 yt-dlp 版本与 pinned 不符自动重装 | pytest ✓ |
| J | task_queue.rs + 测试 | `create_with_dedup` URL 去重规范化（去尾斜杠/大小写不敏感，查询参数保留），任务存储保留原值 | cargo ✓ |

同步动作：`sync-plugins.sh` 已同步 dev 副本；`package-plugins.sh` 已重打包 3 个插件 zip（sha256 更新至 market/plugins.json 与 fallback 快照）；`verify-manifests.sh` 通过。

未执行（用户确认不采纳）：J 之外仍保留的 K/L/M/N 四项（find_cached 合并、config_mgr 锁、etaSnapshots 清理、async 修饰）——维持「不建议修复」结论。
