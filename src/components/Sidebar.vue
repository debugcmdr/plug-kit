<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo } from '../types'
import { installedVersion } from '../stores/plugins'

const emit = defineEmits<{
  select: [pluginId: string]
}>()

const plugins = ref<PluginInfo[]>([])
const searchQuery = ref('')
const activeKey = ref<string | null>(null)

async function loadInstalled() {
  try {
    // Installed-only view for the sidebar navigation.
    const all = await invoke<PluginInfo[]>('list_plugins')
    plugins.value = all.filter(p => p.is_installed)
  } catch (e) {
    console.error('Failed to load plugins:', e)
  }
}

onMounted(loadInstalled)

// 安装/卸载插件后自动刷新侧边栏
watch(installedVersion, loadInstalled)

const filteredPlugins = () => {
  if (!searchQuery.value) return plugins.value
  const query = searchQuery.value.toLowerCase()
  return plugins.value.filter(p =>
    p.name.toLowerCase().includes(query) ||
    p.description.toLowerCase().includes(query)
  )
}

function handleSelect(pluginId: string) {
  activeKey.value = pluginId
  emit('select', pluginId)
}
</script>

<template>
  <n-layout-sider
    :width="240"
    :collapsed-width="60"
    collapse-mode="width"
    :default-collapsed="false"
    show-trigger
    style="height: 100%;"
  >
    <div style="padding: 16px;">
      <n-h3 strong style="margin-bottom: 16px;">PlugKit | 工具市场</n-h3>
      <n-input
        v-model:value="searchQuery"
        placeholder="搜索插件..."
        clearable
        style="margin-bottom: 16px;"
      >
        <template #prefix>
          <n-icon><Search /></n-icon>
        </template>
      </n-input>
    </div>

    <n-menu
      :value="activeKey"
      :options="filteredPlugins().map(p => ({
        key: p.id,
        label: p.name
      }))"
      @update:value="handleSelect"
      style="padding: 0 8px;"
    />

    <div style="padding: 16px;">
      <n-button block type="primary" @click="handleSelect('market')">
        插件市场
      </n-button>
      <n-button block style="margin-top: 8px;" @click="handleSelect('settings')">
        设置
      </n-button>
    </div>
  </n-layout-sider>
</template>
