<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo } from '../types'
import { installedVersion } from '../stores/plugins'

const emit = defineEmits<{
  select: [target: string]
}>()

const installedPlugins = ref<PluginInfo[]>([])
const activeKey = ref<string | null>(null)

// 平铺工具列表(一级一行,不套分类;三个选项在插件右侧界面里)
const TOOLS = [
  { id: 'download', label: '万能下载', icon: 'download' },
  { id: 'convert', label: '格式转换', icon: 'convert' },
]

async function loadInstalled() {
  try {
    const all = await invoke<PluginInfo[]>('list_plugins')
    installedPlugins.value = all.filter(p => p.is_installed)
  } catch (e) {
    console.error('Failed to load plugins:', e)
  }
}

onMounted(loadInstalled)
watch(installedVersion, loadInstalled)

// 该工具是否已安装
function isInstalled(id: string): boolean {
  return installedPlugins.value.some(p => p.id === id)
}

function handleSelect(target: string) {
  activeKey.value = target
  emit('select', target)
}
</script>

<template>
  <n-layout-sider
    :width="220"
    :collapsed-width="0"
    :default-collapsed="false"
    :show-trigger="false"
    :native-scrollbar="false"
    style="height: 100%; display: flex; flex-direction: column;"
  >
    <div style="flex: 1; overflow: auto; padding: 8px 0;">
      <!-- 平铺工具列表(一级一行) -->
      <div
        v-for="tool in TOOLS"
        :key="tool.id"
        class="top-tool"
        :class="{ active: activeKey === tool.id }"
        @click="handleSelect(tool.id)"
      >
        <n-icon size="15">
          <svg v-if="tool.icon === 'download'" xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
          <svg v-else xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="17 1 21 5 17 9"></polyline><path d="M3 11V9a4 4 0 0 1 4-4h14"></path><polyline points="7 23 3 19 7 15"></polyline><path d="M21 13v2a4 4 0 0 1-4 4H3"></path></svg>
        </n-icon>
        <span style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ tool.label }}</span>
        <span v-if="!isInstalled(tool.id)" class="not-installed">未安装</span>
      </div>
    </div>

    <!-- 左下角:插件市场 + 设置 -->
    <div style="border-top: 1px solid var(--n-divider-color); padding: 8px;">
      <div
        class="bottom-item"
        :class="{ active: activeKey === 'market' }"
        @click="handleSelect('market')"
      >
        <n-icon size="15">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 9l1.5-5h15L21 9"></path><path d="M3 9a3 3 0 0 0 6 0 3 3 0 0 0 6 0 3 3 0 0 0 6 0"></path><path d="M21 9v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V9"></path><path d="M9 21v-6h6v6"></path></svg>
        </n-icon>
        <span>插件市场</span>
      </div>
      <div
        class="bottom-item"
        :class="{ active: activeKey === 'settings' }"
        @click="handleSelect('settings')"
      >
        <n-icon size="15">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path></svg>
        </n-icon>
        <span>设置</span>
      </div>
    </div>
  </n-layout-sider>
</template>

<style scoped>
.top-tool {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 16px;
  margin: 4px 8px 0;
  border-radius: 6px;
  font-size: 14px;
  font-weight: 500;
  color: var(--n-text-color-1);
  cursor: pointer;
  transition: background-color 0.15s;
}
.top-tool:hover { background: var(--n-hover-color); }
.top-tool.active { background: var(--n-primary-color-soft); color: var(--n-primary-color); }

.not-installed {
  font-size: 11px;
  color: var(--n-text-color-3);
  border: 1px solid var(--n-border-color);
  border-radius: 4px;
  padding: 1px 6px;
  white-space: nowrap;
}

.bottom-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 13px;
  color: var(--n-text-color-1);
  cursor: pointer;
  transition: background-color 0.15s;
}
.bottom-item:hover { background: var(--n-hover-color); }
.bottom-item.active { background: var(--n-primary-color-soft); color: var(--n-primary-color); }
</style>
