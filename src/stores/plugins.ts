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

export async function fetchMarket() {
  loading.value = true
  try {
    plugins.value = await invoke<PluginInfo[]>('market_list')
  } finally {
    loading.value = false
  }
}

export async function installPlugin(pluginId: string): Promise<InstallResult> {
  const r = await invoke<InstallResult>('install_plugin', { pluginId })
  await fetchMarket()
  if (r.success) {
    installedVersion.value++
  }
  return r
}

export async function uninstallPlugin(pluginId: string): Promise<InstallResult> {
  const r = await invoke<InstallResult>('uninstall_plugin', { pluginId })
  await fetchMarket()
  if (r.success) {
    installedVersion.value++
  }
  return r
}

// ---- 任务 ----
export const tasks = ref<Task[]>([])
export const tasksLoading = ref(false)

export async function fetchTasks(): Promise<Task[]> {
  tasksLoading.value = true
  try {
    const raw = await invoke<Record<string, unknown>[]>('list_tasks')
    tasks.value = raw.map(r => ({
      task_id: String(r.task_id ?? ''),
      plugin_id: String(r.plugin_id ?? ''),
      type: String(r.type ?? ''),
      status: String(r.status ?? 'pending') as Task['status'],
      progress: (r.progress as { percent: number; speed?: string; eta?: string }) ?? { percent: 0 },
      created_at: String(r.created_at ?? ''),
      started_at: r.started_at ? String(r.started_at) : undefined,
      completed_at: r.completed_at ? String(r.completed_at) : undefined,
    }))
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

let taskPollTimer: ReturnType<typeof setInterval> | null = null

export function startTaskPoll() {
  if (taskPollTimer) return
  fetchTasks()
  taskPollTimer = setInterval(fetchTasks, 3000)
}

export function stopTaskPoll() {
  if (taskPollTimer) { clearInterval(taskPollTimer); taskPollTimer = null }
}
