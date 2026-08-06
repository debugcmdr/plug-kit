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
const view = computed<'home' | 'market' | 'settings' | 'plugin'>(() =>
  props.activePlugin === 'market' ? 'market'
  : props.activePlugin === 'settings' ? 'settings'
  : props.activePlugin ? 'plugin' : 'home'
)

function goTo(target: string) {
  emit('update:active-plugin', target)
}
</script>

<template>
  <div style="height: 100%; overflow: auto;">
    <n-layout>
      <n-layout-header style="padding: 16px; border-bottom: 1px solid var(--n-border-color);">
        <n-breadcrumb>
          <n-breadcrumb-item v-if="view === 'home'">首页</n-breadcrumb-item>
          <n-breadcrumb-item v-else-if="view === 'market'">插件市场</n-breadcrumb-item>
          <n-breadcrumb-item v-else-if="view === 'settings'">设置</n-breadcrumb-item>
          <n-breadcrumb-item v-else>{{ activePlugin }}</n-breadcrumb-item>
        </n-breadcrumb>
      </n-layout-header>

      <n-layout-content style="padding: 24px;">
        <div v-if="view === 'home'" style="text-align: center; padding: 48px 0;">
          <n-h1>欢迎使用 PlugKit</n-h1>
          <n-p style="margin-bottom: 32px;">选择一个插件开始使用，或前往插件市场获取更多工具</n-p>
          <n-space>
            <n-button type="primary" size="large" @click="goTo('market')">
              浏览插件市场
            </n-button>
            <n-button size="large" @click="goTo('settings')">
              设置
            </n-button>
          </n-space>
        </div>

        <PluginPanel v-else-if="view === 'plugin'" :plugin-id="activePlugin!" />
        <MarketPanel v-else-if="view === 'market'" />
        <SettingsPanel v-else-if="view === 'settings'" />
      </n-layout-content>
    </n-layout>
  </div>
</template>
