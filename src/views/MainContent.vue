<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import PluginPanel from '../components/PluginPanel.vue'
import MarketPanel from '../components/MarketPanel.vue'
import SettingsPanel from '../components/SettingsPanel.vue'

const props = defineProps<{
  activePlugin: string | null
}>()

// 当前视图类型:market / settings / plugin / home
const view = computed<'market' | 'settings' | 'plugin' | 'home'>(() =>
  props.activePlugin === 'market' ? 'market'
  : props.activePlugin === 'settings' ? 'settings'
  : props.activePlugin ? 'plugin' : 'home'
)

// 所有访问过的插件 id —— iframe 全局常驻,任何视图下都不卸载
// (market/settings 切换也不丢,因为插件 iframe 始终挂载,只是 v-show 隐藏)
const visitedPlugins = ref<string[]>([])
watch(() => props.activePlugin, (id) => {
  if (id && !['market', 'settings'].includes(id) && !visitedPlugins.value.includes(id)) {
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

    <!-- 插件 iframe 无条件常驻(全局缓存,切换 market/settings 也不丢) -->
    <div
      v-for="pid in visitedPlugins"
      :key="pid"
      v-show="view === 'plugin' && activePlugin === pid"
      style="height: 100%;"
    >
      <PluginPanel :plugin-id="pid" />
    </div>

    <!-- market / settings 覆盖在插件之上 -->
    <MarketPanel v-if="view === 'market'" style="position: relative;" />
    <SettingsPanel v-else-if="view === 'settings'" style="position: relative;" />
  </div>
</template>
