import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo, InstallResult, Task } from '../types'

export const plugins = ref<PluginInfo[]>([])
export const loading = ref(false)

// 已安装插件集合变更信号:自增计数,任何组件 watch 它来刷新已安装列表。
export const installedVersion = ref(0)

/** 简单 semver 比较:返回 >0 表示 a>b(供工具箱/市场判断「可更新」)。
 * 注:不处理预发布(v1.0.0-rc1 视为等于 1.0.0),官方工具版本均为纯数字,够用。 */
export function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(n => parseInt(n, 10) || 0)
  const pb = b.split('.').map(n => parseInt(n, 10) || 0)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0)
    if (d !== 0) return d
  }
  return 0
}

// 已安装插件有可用更新的数量(工具箱红点):市场数据刷新(启动/轮询/安装卸载)后自动更新。
export const marketUpdateCount = computed(
  () => marketPlugins.value.filter(p =>
    p.is_installed && !!p.installed_version &&
    compareVersions(p.installed_version, p.version) < 0
  ).length
)

// ---- 应用(外壳)更新检查:仅告知「有新版本」,不下载覆盖(一键更新后续再说) ----
export interface AppUpdateInfo {
  has_update: boolean
  latest_version?: string
  current_version?: string
  url?: string
}
export const appUpdate = ref<AppUpdateInfo>({ has_update: false })

/** 检查外壳新版本(启动调用一次)。仅告知;网络失败/限流静默不打扰。 */
export async function checkAppUpdate(): Promise<void> {
  try {
    const r = await invoke<AppUpdateInfo>('check_app_update')
    if (r) appUpdate.value = r
  } catch { /* 静默 */ }
}

/** 应用版本号(编译注入,与 Rust CARGO_PKG_VERSION 一致;设置页「关于」显示)。 */
export async function fetchAppVersion(): Promise<string> {
  try {
    return await invoke<string>('get_app_version')
  } catch {
    return ''
  }
}

export async function fetchPlugins() {
  loading.value = true
  try {
    plugins.value = await invoke<PluginInfo[]>('list_plugins')
  } finally {
    loading.value = false
  }
}

// ---- 市场数据缓存 ----
// 启动后只请求一次，之后用缓存；每 5 分钟后台刷新一次。
let marketData: PluginInfo[] = []
let marketLastFetched = 0
const MARKET_CACHE_TTL_MS = 5 * 60 * 1000  // 5 分钟
let marketPollTimer: ReturnType<typeof setInterval> | null = null

export const marketPlugins = ref<PluginInfo[]>([])
export const marketLoading = ref(false)

export async function fetchMarket(force = false): Promise<PluginInfo[]> {
  const now = Date.now()
  if (!force && marketLastFetched > 0 && (now - marketLastFetched) < MARKET_CACHE_TTL_MS) {
    return marketPlugins.value
  }
  marketLoading.value = true
  try {
    marketData = await invoke<PluginInfo[]>('market_list')
    marketPlugins.value = marketData
    marketLastFetched = Date.now()
  } catch (e) {
    console.error('Failed to fetch market:', e)
  } finally {
    marketLoading.value = false
  }
  return marketPlugins.value
}

/** 强制刷新:先清后端清单缓存(market_refresh),再重新拉取。 */
export async function refreshMarket(): Promise<void> {
  try {
    await invoke('market_refresh')
  } catch (e) {
    console.error('Failed to refresh market cache:', e)
  }
  await fetchMarket(true)
}

export function startMarketPoll() {
  if (marketPollTimer) return
  fetchMarket()  // 首次立即加载
  marketPollTimer = setInterval(() => fetchMarket(), MARKET_CACHE_TTL_MS)
}

export function stopMarketPoll() {
  if (marketPollTimer) { clearInterval(marketPollTimer); marketPollTimer = null }
}

export async function installPlugin(pluginId: string): Promise<InstallResult> {
  const r = await invoke<InstallResult>('install_plugin', { pluginId })
  if (r.success) {
    installedVersion.value++
    fetchMarket(true)  // 安装后强制刷新市场数据
  }
  return r
}

export async function uninstallPlugin(pluginId: string): Promise<InstallResult> {
  const r = await invoke<InstallResult>('uninstall_plugin', { pluginId })
  if (r.success) {
    installedVersion.value++
    fetchMarket(true)  // 卸载后强制刷新市场数据
  }
  return r
}

// ---- 任务 ----
export const tasks = ref<Task[]>([])
export const tasksLoading = ref(false)

/**
 * 把后端 list_tasks 返回的原始 JSON 映射为前端 Task 类型。
 * 抽成纯函数以便单元测试回归(如 type_ -> type 字段映射)。
 */
export function mapTask(r: Record<string, unknown>): Task {
  const rawProgress = (r.progress ?? {}) as { percent?: unknown; speed?: unknown; eta?: unknown; message?: unknown }
  // percent 兜底 + clamp 0-100(A-1):后端异常/缺失时 TaskPanel 的
  // `task.progress.percent.toFixed(0)` 不再运行时崩溃(此前可能 TypeError)。
  const rawPercent = Number(rawProgress.percent ?? 0)
  const percent = Number.isFinite(rawPercent) ? Math.min(100, Math.max(0, rawPercent)) : 0
  return {
    task_id: String(r.task_id ?? ''),
    plugin_id: String(r.plugin_id ?? ''),
    type: String(r.type ?? ''),
    status: String(r.status ?? 'pending') as Task['status'],
    progress: {
      percent,
      speed: typeof rawProgress.speed === 'string' ? rawProgress.speed : undefined,
      eta: typeof rawProgress.eta === 'string' ? rawProgress.eta : undefined,
      message: typeof rawProgress.message === 'string' ? rawProgress.message : undefined,
    },
    error: r.error ? String(r.error) : undefined,
    file_name: r.file_name ? String(r.file_name) : undefined,
    output_path: r.output_path ? String(r.output_path) : undefined,
    url: r.url ? String(r.url) : undefined,
    created_at: String(r.created_at ?? ''),
    started_at: r.started_at ? String(r.started_at) : undefined,
    completed_at: r.completed_at ? String(r.completed_at) : undefined,
  }
}

export async function fetchTasks(): Promise<Task[]> {
  tasksLoading.value = true
  try {
    const raw = await invoke<Record<string, unknown>[]>('list_tasks')
    tasks.value = raw.map(mapTask)
  } catch (e) {
    console.error('Failed to fetch tasks:', e)
  } finally {
    tasksLoading.value = false
  }
  return tasks.value
}

// 任务控制:直接调用外壳的独立 command(不再伪装成 pluginId='system' 的
// bridge 消息——任务中心是外壳功能,走自己的通道,语义干净)。
export async function cancelTask(taskId: string): Promise<void> {
  await invoke('cancel_task', { taskId })
  await fetchTasks()
}

export async function pauseTask(taskId: string): Promise<void> {
  await invoke('pause_task', { taskId })
  await fetchTasks()
}

export async function resumeTask(taskId: string): Promise<void> {
  await invoke('resume_task', { taskId })
  await fetchTasks()
}

let taskPollTimer: ReturnType<typeof setTimeout> | null = null

// 轮询间隔:有活跃任务(运行/排队/暂停)3s 高频刷新;空闲时降频到 15s(F-4),
// 避免"任务面板从没打开过"也永远 3s 全量拉取后端任务列表。
const TASK_POLL_ACTIVE_MS = 3000
const TASK_POLL_IDLE_MS = 15000

export function startTaskPoll() {
  if (taskPollTimer) return
  const poll = () => {
    fetchTasks().then(() => {
      const hasActive = tasks.value.some(t =>
        t.status === 'running' || t.status === 'pending' || t.status === 'paused'
      )
      taskPollTimer = setTimeout(poll, hasActive ? TASK_POLL_ACTIVE_MS : TASK_POLL_IDLE_MS)
    })
  }
  poll()
}

export function stopTaskPoll() {
  if (taskPollTimer) { clearTimeout(taskPollTimer); taskPollTimer = null }
}
