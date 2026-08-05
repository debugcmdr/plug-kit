import { ref } from 'vue'
import type { PluginInfo } from '../types'

export const plugins = ref<PluginInfo[]>([])

export async function fetchPlugins() {
  console.log('Fetching plugins...')
}

export async function installPlugin(pluginId: string) {
  console.log('Installing plugin:', pluginId)
}

export async function uninstallPlugin(pluginId: string) {
  console.log('Uninstalling plugin:', pluginId)
}
