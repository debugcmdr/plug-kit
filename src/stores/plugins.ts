import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo, InstallResult, Task } from '../types'

export const plugins = ref<PluginInfo[]>([])
export const loading = ref(false)

// 已安装插件集合变更信号:自增计数,任何组件 watch 它来刷新已安装列表。
export const installedVersion = ref(0)

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
  return {
    task_id: String(r.task_id ?? ''),
    plugin_id: String(r.plugin_id ?? ''),
    type: String(r.type ?? ''),
    status: String(r.status ?? 'pending') as Task['status'],
    progress: (r.progress as { percent: number; speed?: string; eta?: string; message?: string }) ?? { percent: 0 },
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

export async function cancelTask(taskId: string): Promise<void> {
  await invoke('bridge_message', {
    pluginId: 'system',
    msg: { id: crypto.randomUUID(), command: 'task_cancel', payload: { task_id: taskId } },
  })
  await fetchTasks()
}

export async function pauseTask(taskId: string): Promise<void> {
  await invoke('bridge_message', {
    pluginId: 'system',
    msg: { id: crypto.randomUUID(), command: 'task_pause', payload: { task_id: taskId } },
  })
  await fetchTasks()
}

export async function resumeTask(taskId: string): Promise<void> {
  await invoke('bridge_message', {
    pluginId: 'system',
    msg: { id: crypto.randomUUID(), command: 'task_resume', payload: { task_id: taskId } },
  })
  await fetchTasks()
}

let taskPollTimer: ReturnType<typeof setInterval> | null = null

export function startTaskPoll() {
  if (taskPollTimer) return
  fetchTasks()
  taskPollTimer = setInterval(fetchTasks, 3000)
}

export function stopTaskPoll() {
  if (taskPollTimer) { clearInterval(taskPollTimer); taskPollTimer = null }
}
