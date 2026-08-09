<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import PluginPanel from '../components/PluginPanel.vue'
import MarketPanel from '../components/MarketPanel.vue'
import SettingsPanel from '../components/SettingsPanel.vue'
import TaskPanel from '../components/TaskPanel.vue'
import LogPanel from '../components/LogPanel.vue'

const props = defineProps<{
  activePlugin: string | null
}>()

// 当前视图类型
const view = computed<'market' | 'settings' | 'plugin' | 'tasks' | 'logs' | 'home'>(() => {
  const id = props.activePlugin
  if (id === 'market') return 'market'
  if (id === 'settings') return 'settings'
  if (id === 'tasks') return 'tasks'
  if (id === 'logs') return 'logs'
  if (id) return 'plugin'
  return 'home'
})

// 所有访问过的插件 id —— iframe 全局常驻,切换其他视图也不卸载
const visitedPlugins = ref<string[]>([])
watch(() => props.activePlugin, (id) => {
  if (id && !['market', 'settings', 'tasks', 'logs'].includes(id) && !visitedPlugins.value.includes(id)) {
    visitedPlugins.value.push(id)
  }
}, { immediate: true })
</script>

<template>
  <div style="height: 100%; overflow: auto; padding: 0;">
    <!-- 首页 -->
    <div v-if="view === 'home'" style="padding: 24px;">
      <n-h2 style="margin-bottom: 8px;">欢迎使用 PlugKit</n-h2>
      <n-p style="margin-bottom: 24px; color: var(--n-text-color-3);">
        从左侧选择一个工具开始使用;或在左下角打开插件市场安装更多工具。
      </n-p>
    </div>

    <!-- 插件 iframe 无条件常驻 -->
    <div
      v-for="pid in visitedPlugins"
      :key="pid"
      v-show="view === 'plugin' && activePlugin === pid"
      style="height: 100%;"
    >
      <PluginPanel :plugin-id="pid" />
    </div>

    <!-- 各面板 -->
    <TaskPanel v-if="view === 'tasks'" />
    <LogPanel    v-else-if="view === 'logs'" />
    <MarketPanel v-else-if="view === 'market'" style="position: relative;" />
    <SettingsPanel v-else-if="view === 'settings'" style="position: relative;" />
  </div>
</template>
