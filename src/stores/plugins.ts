import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo, InstallResult } from '../types'

export const plugins = ref<PluginInfo[]>([])
export const loading = ref(false)

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
  return r
}

export async function uninstallPlugin(pluginId: string): Promise<InstallResult> {
  const r = await invoke<InstallResult>('uninstall_plugin', { pluginId })
  await fetchMarket()
  return r
}
