<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { tasks, tasksLoading, fetchTasks, cancelTask, startTaskPoll, stopTaskPoll } from '../stores/plugins'
import type { Task } from '../types'

const props = defineProps<{ pluginId?: string }>()
const statusFilter = ref<string>('all')
const SEARCH_PERSIST = 'task_filter_status'

onMounted(() => {
  statusFilter.value = (localStorage.getItem(SEARCH_PERSIST) as string) || 'all'
  fetchTasks()
  startTaskPoll()
})
onUnmounted(stopTaskPoll)

function setFilter(s: string) {
  statusFilter.value = s
  localStorage.setItem(SEARCH_PERSIST, s)
}

const filteredTasks = computed(() => {
  let list = tasks.value
  if (props.pluginId) list = list.filter(t => t.plugin_id === props.pluginId)
  if (statusFilter.value !== 'all') list = list.filter(t => t.status === statusFilter.value)
  return list
})

const stats = computed(() => {
  const all = tasks.value
  return {
    running: all.filter(t => t.status === 'running').length,
    pending: all.filter(t => t.status === 'pending').length,
    failed: all.filter(t => t.status === 'failed').length,
    total: all.length,
  }
})

const FILTERS = [
  { key: 'all', label: '全部' },
  { key: 'running', label: '运行中' },
  { key: 'pending', label: '等待中' },
  { key: 'failed', label: '失败' },
  { key: 'completed', label: '已完成' },
  { key: 'cancelled', label: '已取消' },
] as const

async function handleCancel(task: Task) {
  if (task.status === 'running' || task.status === 'pending') {
    await cancelTask(task.task_id)
  }
}

function fmtTime(iso: string) {
  const d = new Date(iso)
  return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function statusDot(status: string) {
  const map: Record<string, string> = {
    running: '#18a058', pending: '#f0a50b', completed: '#86909c',
    failed: '#f53f3f', cancelled: '#86909c', paused: '#3370ff', interrupted: '#f53f3f',
  }
  return map[status] ?? '#86909c'
}
</script>

<template>
  <div style="padding: 24px; min-height: 100%;">
    <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:20px;">
      <n-h2 style="margin:0">任务</n-h2>
      <n-button size="small" quaternary @click="fetchTasks">刷新</n-button>
    </div>

    <n-space style="margin-bottom:16px;" wrap>
      <n-tag
        v-for="f in FILTERS" :key="f.key"
        :type="statusFilter === f.key ? 'primary' : 'default'"
        :bordered="false" round
        class="filter-btn"
        @click="setFilter(f.key)"
      >
        {{ f.label }}
        <n-badge
          v-if="['running','pending','failed'].includes(f.key)"
          :value="(stats as Record<string,number>)[f.key]"
          :max="99" :show="(stats as Record<string,number>)[f.key] > 0"
        />
      </n-tag>
    </n-space>

    <n-spin :show="tasksLoading && tasks.length === 0">
      <div v-if="filteredTasks.length === 0" style="text-align:center; padding:40px; color:var(--n-text-color-3);">
        <n-icon size="32" style="margin-bottom:8px;"><svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg></n-icon>
        <div>暂无任务</div>
      </div>

      <div v-for="task in filteredTasks" :key="task.task_id" style="margin-bottom:10px;">
        <n-card size="small" :bordered="false" style="border-radius:8px; background:var(--n-color-target);">
          <template #header>
            <div style="display:flex; justify-content:space-between; align-items:center;">
              <div style="display:flex; align-items:center; gap:8px;">
                <span
                  style="width:8px;height:8px;border-radius:50%;display:inline-block;flex-shrink:0;"
                  :style="{ background: statusDot(task.status) }"
                ></span>
                <span style="font-weight:500;font-size:14px;">{{ task.plugin_id }}</span>
                <n-tag size="tiny" type="info">{{ task.type }}</n-tag>
              </div>
              <n-space>
                <n-tag size="tiny" :style="{ color: statusDot(task.status), borderColor: statusDot(task.status) }" :color="statusDot(task.status)" text-color="#fff">{{ task.status }}</n-tag>
                <n-button v-if="task.status === 'running' || task.status === 'pending'"
                  size="tiny" quaternary type="error" @click="handleCancel(task)">取消</n-button>
              </n-space>
            </div>
          </template>

          <div v-if="task.status === 'running' || task.status === 'pending'" style="margin-top:8px;">
            <n-progress type="line" :percentage="Math.round(task.progress.percent)" :show-indicator="false"
              :height="6" :border-radius="3" />
            <div style="display:flex; justify-content:space-between; margin-top:4px; font-size:12px; color:var(--n-text-color-3);">
              <span>{{ task.progress.percent.toFixed(0) }}%</span>
              <span v-if="task.progress.speed">{{ task.progress.speed }}</span>
              <span v-if="task.progress.eta">剩余 {{ task.progress.eta }}</span>
            </div>
          </div>

          <div style="display:flex; justify-content:space-between; font-size:12px; color:var(--n-text-color-3); margin-top:4px;">
            <span>{{ fmtTime(task.created_at) }}</span>
            <span>{{ task.task_id.slice(0, 8) }}...</span>
          </div>
        </n-card>
      </div>
    </n-spin>
  </div>
</template>

<style scoped>
.filter-btn { cursor: pointer; }
</style>
