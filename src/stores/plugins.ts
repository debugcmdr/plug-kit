import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo, InstallResult } from '../types'

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
