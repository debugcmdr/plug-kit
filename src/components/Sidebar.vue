<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo } from '../types'
import { installedVersion, tasks, fetchTasks } from '../stores/plugins'

const emit = defineEmits<{
  select: [target: string]
}>()

const installedPlugins = ref<PluginInfo[]>([])
const activeKey = ref<string | null>(null)
const taskCount = ref(0)

// 平铺工具列表，从后端动态加载（不再硬编码）
const tools = computed(() =>
  installedPlugins.value.map(p => ({ id: p.id, label: p.name }))
)

async function loadInstalled() {
  try {
    const all = await invoke<PluginInfo[]>('list_plugins')
    installedPlugins.value = all.filter(p => p.is_installed)
    // 统计未完成的任务数
    await fetchTasks()
    taskCount.value = tasks.value.filter(
      t => t.status === 'running' || t.status === 'pending'
    ).length
  } catch (e) {
    console.error('Failed to load plugins:', e)
  }
}

onMounted(loadInstalled)
watch(installedVersion, loadInstalled)

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
      <!-- 动态工具列表 -->
      <div v-if="tools.length === 0" style="padding:16px; font-size:13px; color:var(--n-text-color-3); text-align:center;">
        暂无已安装工具<br><span style="font-size:12px;">请在插件市场安装</span>
      </div>
      <div
        v-for="tool in tools"
        :key="tool.id"
        class="top-tool"
        :class="{ active: activeKey === tool.id }"
        @click="handleSelect(tool.id)"
      >
        <n-icon size="15">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>
        </n-icon>
        <span style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ tool.label }}</span>
      </div>
    </div>

    <!-- 底部导航：固定在底部 -->
    <div style="flex-shrink: 0; border-top: 1px solid var(--n-divider-color); padding: 8px;">
      <div
        class="bottom-item"
        :class="{ active: activeKey === 'tasks' }"
        @click="handleSelect('tasks')"
      >
        <n-icon size="15">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
        </n-icon>
        <span>任务</span>
        <n-badge v-if="taskCount > 0" :value="taskCount" :max="99" color="#f53f3f" style="margin-left:auto;" />
      </div>
      <div
        class="bottom-item"
        :class="{ active: activeKey === 'logs' }"
        @click="handleSelect('logs')"
      >
        <n-icon size="15">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>
        </n-icon>
        <span>日志</span>
      </div>
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
