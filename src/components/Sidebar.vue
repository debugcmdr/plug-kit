<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo } from '../types'
import { installedVersion, tasks, fetchTasks } from '../stores/plugins'
import AppIcon from './AppIcon.vue'

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
    :width="224"
    :collapsed-width="0"
    :default-collapsed="false"
    :show-trigger="false"
    :native-scrollbar="false"
    :content-style="{ display: 'flex', flexDirection: 'column', height: '100%' }"
    style="height: 100%; border-right: 1px solid var(--n-divider-color);"
  >
    <!-- 品牌头部(点击回到首页) -->
    <div class="brand" @click="handleSelect('home')" title="返回首页">
      <span class="brand-logo"><AppIcon name="logo" :size="20" /></span>
      <span class="brand-name">PlugKit</span>
    </div>

    <!-- 已安装工具列表 -->
    <div class="scroll">
      <div v-if="tools.length === 0" class="empty">
        暂无已安装工具<br><span>请在插件市场安装</span>
      </div>
      <template v-else>
        <div class="section-label">工具</div>
        <div
          v-for="tool in tools"
          :key="tool.id"
          class="nav-item"
          :class="{ active: activeKey === tool.id }"
          @click="handleSelect(tool.id)"
        >
          <AppIcon name="tool" :size="16" class="nav-icon" />
          <span class="nav-label">{{ tool.label }}</span>
        </div>
      </template>
    </div>

    <!-- 底部导航：固定在底部 -->
    <div class="bottom">
      <div
        class="nav-item"
        :class="{ active: activeKey === 'tasks' }"
        @click="handleSelect('tasks')"
      >
        <AppIcon name="tasks" :size="16" class="nav-icon" />
        <span class="nav-label">任务</span>
        <n-badge v-if="taskCount > 0" :value="taskCount" :max="99" color="#f53f3f" class="nav-badge" />
      </div>
      <div
        class="nav-item"
        :class="{ active: activeKey === 'logs' }"
        @click="handleSelect('logs')"
      >
        <AppIcon name="logs" :size="16" class="nav-icon" />
        <span class="nav-label">日志</span>
      </div>
      <div
        class="nav-item"
        :class="{ active: activeKey === 'market' }"
        @click="handleSelect('market')"
      >
        <AppIcon name="market" :size="16" class="nav-icon" />
        <span class="nav-label">插件市场</span>
      </div>
      <div
        class="nav-item"
        :class="{ active: activeKey === 'settings' }"
        @click="handleSelect('settings')"
      >
        <AppIcon name="settings" :size="16" class="nav-icon" />
        <span class="nav-label">设置</span>
      </div>
    </div>
  </n-layout-sider>
</template>

<style scoped>
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 56px;
  padding: 0 18px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--n-divider-color);
  cursor: pointer;
  user-select: none;
  transition: background-color 0.15s;
}
.brand:hover { background: var(--n-hover-color); }
.brand-logo {
  color: var(--n-primary-color);
  display: flex;
  align-items: center;
}
.brand-name {
  font-size: 16px;
  font-weight: 700;
  letter-spacing: 0.3px;
}

.scroll {
  flex: 1;
  overflow: auto;
  padding: 10px 0;
}
.empty {
  padding: 16px;
  font-size: 13px;
  color: var(--n-text-color-3);
  text-align: center;
  line-height: 1.7;
}
.empty span { font-size: 12px; }

.section-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--n-text-color-3);
  padding: 6px 20px 8px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 2px 10px;
  padding: 9px 12px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 14px;
  color: var(--n-text-color-1);
  transition: background-color 0.15s, color 0.15s;
  position: relative;
}
.nav-item:hover { background: var(--n-hover-color); }
.nav-item.active {
  background: var(--n-primary-color-soft);
  color: var(--n-primary-color);
  font-weight: 600;
  box-shadow: inset 3px 0 0 var(--n-primary-color);
}
.nav-icon { flex-shrink: 0; }
.nav-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.nav-badge { margin-left: auto; }

.bottom {
  flex-shrink: 0;
  border-top: 1px solid var(--n-divider-color);
  padding: 8px 0;
}
</style>
