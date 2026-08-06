<script setup lang="ts">
import { computed, ref, watch } from 'vue'
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

// 已访问过的插件 id(保持 iframe 常驻,切换时状态不丢)
const visitedPlugins = ref<string[]>([])
watch(() => props.activePlugin, (id) => {
  if (id && !['market', 'settings'].includes(id) && !visitedPlugins.value.includes(id)) {
    visitedPlugins.value.push(id)
  }
}, { immediate: true })
</script>

<template>
  <div style="height: 100%; overflow: auto; padding: 0;">
    <div v-if="view === 'home'" style="padding: 24px;">
      <n-h2 style="margin-bottom: 8px;">欢迎使用 PlugKit</n-h2>
      <n-p style="margin-bottom: 24px; color: var(--n-text-color-3);">
        从左侧选择一个工具开始使用;或在左下角打开插件市场安装更多工具。
      </n-p>
    </div>

    <MarketPanel v-else-if="view === 'market'" />
    <SettingsPanel v-else-if="view === 'settings'" />

    <!-- 插件 iframe 常驻:切换用 v-show,保留每个插件状态 -->
    <div v-else>
      <div v-for="pid in visitedPlugins" :key="pid" v-show="activePlugin === pid" style="height: 100%;">
        <PluginPanel :plugin-id="pid" />
      </div>
    </div>
  </div>
</template>
