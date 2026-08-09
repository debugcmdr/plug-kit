<script setup lang="ts">
import { computed } from 'vue'
import PluginPanel from '../components/PluginPanel.vue'
import MarketPanel from '../components/MarketPanel.vue'
import SettingsPanel from '../components/SettingsPanel.vue'
import TaskPanel from '../components/TaskPanel.vue'
import LogPanel from '../components/LogPanel.vue'

const props = defineProps<{
  activePlugin: string | null
}>()

const view = computed<'market' | 'settings' | 'plugin' | 'tasks' | 'logs' | 'home'>(() => {
  const id = props.activePlugin
  if (id === 'market') return 'market'
  if (id === 'settings') return 'settings'
  if (id === 'tasks') return 'tasks'
  if (id === 'logs') return 'logs'
  if (id) return 'plugin'
  return 'home'
})
</script>

<template>
  <div style="height: 100%; display: flex; flex-direction: column; overflow: hidden;">
    <!-- 首页 -->
    <div v-if="view === 'home'" style="padding: 24px;">
      <n-h2 style="margin-bottom: 8px;">欢迎使用 PlugKit</n-h2>
      <n-p style="color: var(--n-text-color-3);">
        从左侧选择一个工具开始使用；或打开插件市场安装更多工具。
      </n-p>
    </div>

    <!-- 单一插件面板：切换插件时 Vue 自动更新 iframe src -->
    <PluginPanel
      v-if="view === 'plugin'"
      :plugin-id="activePlugin!"
      style="flex: 1; min-height: 0;"
    />

    <!-- 其他面板 -->
    <TaskPanel v-else-if="view === 'tasks'" style="flex:1; min-height:0; overflow:hidden;" />
    <LogPanel  v-else-if="view === 'logs'" style="flex:1; min-height:0; overflow:hidden;" />
    <MarketPanel v-else-if="view === 'market'" style="flex:1; overflow:auto;" />
    <SettingsPanel v-else-if="view === 'settings'" style="flex:1; overflow:auto;" />
  </div>
</template>
