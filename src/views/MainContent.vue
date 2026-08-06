<script setup lang="ts">
import { computed } from 'vue'
import PluginPanel from '../components/PluginPanel.vue'
import MarketPanel from '../components/MarketPanel.vue'
import SettingsPanel from '../components/SettingsPanel.vue'

const props = defineProps<{
  activePlugin: string | null
}>()

const emit = defineEmits<{
  'update:active-plugin': [value: string]
}>()

// 响应式视图:activePlugin 变化时自动切换
const view = computed<'market' | 'settings' | 'plugin' | 'home'>(() =>
  props.activePlugin === 'market' ? 'market'
  : props.activePlugin === 'settings' ? 'settings'
  : props.activePlugin ? 'plugin' : 'home'
)
</script>

<template>
  <div style="height: 100%; overflow: auto; padding: 0;">
    <div v-if="view === 'home'" style="padding: 24px;">
      <n-h2 style="margin-bottom: 8px;">欢迎使用 PlugKit</n-h2>
      <n-p style="margin-bottom: 24px; color: var(--n-text-color-3);">
        从左侧选择一个工具开始使用;或在左下角打开插件市场安装更多工具。
      </n-p>
    </div>

    <PluginPanel v-else-if="view === 'plugin'" :plugin-id="activePlugin!" />
    <MarketPanel v-else-if="view === 'market'" />
    <SettingsPanel v-else-if="view === 'settings'" />
  </div>
</template>
