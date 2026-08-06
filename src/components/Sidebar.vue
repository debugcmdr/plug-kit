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

// 顶部单条工具(不套分类):万能下载
const TOP_TOOL = { id: 'download', label: '万能下载', icon: 'download' }

// 分类 → 插件映射(语义分类,插件动态安装)
const CATEGORIES = [
  {
    key: 'convert',
    label: '格式转换',
    icon: 'convert',
    pluginIds: ['convert', 'audio-extract'],
  },
  {
    key: 'subtitle',
    label: '文案字幕',
    icon: 'subtitle',
    pluginIds: ['subtitle'],
  },
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

// 某个分类下已安装的插件
function categoryPlugins(pluginIds: string[]): PluginInfo[] {
  return installedPlugins.value.filter(p => pluginIds.includes(p.id))
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
      <!-- 顶部单条工具:万能下载(不套分类,一条即可) -->
      <div
        v-if="categoryPlugins([TOP_TOOL.id]).length > 0"
        class="top-tool"
        :class="{ active: activeKey === TOP_TOOL.id }"
        @click="handleSelect(TOP_TOOL.id)"
      >
        <n-icon size="15">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
        </n-icon>
        <span style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ TOP_TOOL.label }}</span>
      </div>
      <div v-else class="top-tool cat-empty" @click="handleSelect('market')">
        <n-icon size="13"><svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg></n-icon>
        <span>万能下载 · 去市场安装</span>
      </div>

      <!-- 分类区块 -->
      <div v-for="cat in CATEGORIES" :key="cat.key" style="margin-bottom: 4px; margin-top: 8px;">
        <div
          style="display: flex; align-items: center; gap: 6px; padding: 6px 16px; font-size: 12px; color: var(--n-text-color-3);"
        >
          <n-icon size="14">
            <svg v-if="cat.icon === 'download'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
            <svg v-else-if="cat.icon === 'convert'" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="17 1 21 5 17 9"></polyline><path d="M3 11V9a4 4 0 0 1 4-4h14"></path><polyline points="7 23 3 19 7 15"></polyline><path d="M21 13v2a4 4 0 0 1-4 4H3"></path></svg>
            <svg v-else xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"></path><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"></path></svg>
          </n-icon>
          <span>{{ cat.label }}</span>
        </div>

        <!-- 分类下的插件 -->
        <div
          v-for="plugin in categoryPlugins(cat.pluginIds)"
          :key="plugin.id"
          class="cat-item"
          :class="{ active: activeKey === plugin.id }"
          @click="handleSelect(plugin.id)"
        >
          <span style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ plugin.name }}</span>
        </div>

        <!-- 分类下无已装插件 → 提示去市场 -->
        <div
          v-if="cat.pluginIds.length > 0 && categoryPlugins(cat.pluginIds).length === 0"
          class="cat-empty"
          @click="handleSelect('market')"
        >
          <n-icon size="13"><svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg></n-icon>
          <span>去市场安装</span>
        </div>
        <div v-else-if="cat.pluginIds.length === 0" class="cat-empty" style="cursor: default;">
          <span>暂无工具</span>
        </div>
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

.cat-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 16px 7px 42px;
  font-size: 13px;
  color: var(--n-text-color-1);
  cursor: pointer;
  transition: background-color 0.15s;
}
.cat-item:hover { background: var(--n-hover-color); }
.cat-item.active { background: var(--n-primary-color-soft); color: var(--n-primary-color); font-weight: 500; }

.cat-empty {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 6px 16px 6px 42px;
  font-size: 12px;
  color: var(--n-text-color-3);
  cursor: pointer;
}
.cat-empty:hover { color: var(--n-primary-color); }

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
