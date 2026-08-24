<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useMessage } from 'naive-ui'
import { tasks, tasksLoading, fetchTasks, cancelTask, pauseTask, resumeTask, installedVersion } from '../stores/plugins'
import type { Task } from '../types'
import AppIcon from './AppIcon.vue'

const props = defineProps<{ pluginId?: string }>()
const statusFilter = ref<string>('all')
const SEARCH_PERSIST = 'task_filter_status'
const message = useMessage()

// 插件 id → 显示名映射:任务行显示左侧插件名(如「万能下载」而非 download)。
const pluginNames = ref<Record<string, string>>({})
async function loadPluginNames() {
  try {
    const list = await invoke<{ id: string; name: string }[]>('list_plugins')
    pluginNames.value = Object.fromEntries(list.map(p => [p.id, p.name]))
  } catch { /* 失败则回退显示 id */ }
}
function displayName(id: string) { return pluginNames.value[id] || id }
onMounted(() => { loadPluginNames() })
watch(installedVersion, loadPluginNames)

// 倒计时滴答时钟:每秒推进,驱动任务界面里的实时 ETA 倒计时。
const now = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  const stored = localStorage.getItem(SEARCH_PERSIST)
  // 旧版本可能存过已删除的筛选(pending/paused),回退到"全部"。
  statusFilter.value = stored && FILTERS.some(f => f.key === stored) ? stored : 'all'
  // 任务数据由 App 全局轮询维护(startTaskPoll),此处仅确保进入时立即拉一次,
  // 避免等下一个 3s 轮询周期才显示最新数据。
  fetchTasks()
  ticker = setInterval(() => { now.value = Date.now() }, 1000)
})
onUnmounted(() => {
  if (ticker) clearInterval(ticker)
})

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
    paused: all.filter(t => t.status === 'paused').length,
    failed: all.filter(t => t.status === 'failed').length,
    total: all.length,
  }
})

const FILTERS = [
  { key: 'all', label: '全部' },
  { key: 'running', label: '运行中' },
  { key: 'failed', label: '失败' },
  { key: 'completed', label: '已完成' },
  { key: 'cancelled', label: '已取消' },
] as const

// 状态中文名:配合轻量「圆点+文字」展示,替代整块彩色标签。
const STATUS_LABEL: Record<string, string> = {
  running: '运行中', pending: '等待中', paused: '已暂停', completed: '已完成',
  failed: '失败', cancelled: '已取消', interrupted: '中断',
}

async function handleCancel(task: Task) {
  if (task.status === 'running' || task.status === 'pending' || task.status === 'paused') {
    await cancelTask(task.task_id)
  }
}

async function handlePause(task: Task) {
  if (task.status === 'running' || task.status === 'pending') {
    await pauseTask(task.task_id)
  }
}

async function handleResume(task: Task) {
  if (task.status === 'paused') {
    await resumeTask(task.task_id)
  }
}

// 在系统文件管理器中打开任务输出文件所在文件夹(open_in_folder 对目录/文件均适用)。
async function openOutputDir(task: Task) {
  if (!task.output_path) return
  try {
    await invoke('open_in_folder', { path: task.output_path })
  } catch (e) {
    // 用户点击无反馈会误以为功能失效——明确提示失败原因
    console.error('打开输出文件夹失败:', e)
    message.error(`打开输出文件夹失败: ${e}`)
  }
}

function fmtTime(iso: string) {
  const d = new Date(iso)
  return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

// ---- 倒计时:把插件上报的静态 ETA 变成每秒递减的真实倒计时 ----
const TERMINAL = ['completed', 'failed', 'cancelled', 'interrupted']

function parseHms(s: string): number | null {
  const parts = s.split(':').map(Number)
  if (parts.length === 0 || parts.some(isNaN)) return null
  let secs = 0
  for (const p of parts) secs = secs * 60 + p
  return secs
}

function formatHms(secs: number): string {
  secs = Math.max(0, Math.floor(secs))
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = secs % 60
  const pad = (n: number) => String(n).padStart(2, '0')
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`
}

// 每个任务记录最后一次 ETA 快照(原始字符串 + 时间戳),用于滴答递减。
const etaSnapshots = new Map<string, { raw: string; secs: number; ts: number }>()

function liveEta(task: Task): string {
  if (TERMINAL.includes(task.status)) return ''
  const raw = task.progress?.eta
  if (!raw) return ''
  const secs = parseHms(raw)
  if (secs == null) return raw
  // 暂停/等待等非运行态:不倒计时,直接展示估算剩余值。
  if (task.status !== 'running') {
    etaSnapshots.set(task.task_id, { raw, secs, ts: now.value })
    return formatHms(secs)
  }
  const prev = etaSnapshots.get(task.task_id)
  if (!prev || prev.raw !== raw) {
    // ETA 刷新 → 重新对齐起点(基于真实剩余估算)。
    etaSnapshots.set(task.task_id, { raw, secs, ts: now.value })
    return formatHms(secs)
  }
  // 同一 ETA 区间内:按真实流逝时间递减(暂停时已冻结 ts)。
  const elapsed = Math.floor((now.value - prev.ts) / 1000)
  return formatHms(secs - elapsed)
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
      <div style="display:flex; align-items:center; gap:10px;">
        <span style="display:inline-flex; align-items:center; justify-content:center; width:34px; height:34px; border-radius:10px; background:rgba(51,112,255,.1); color:var(--n-primary-color);"><AppIcon name="tasks" :size="18" /></span>
        <n-h2 style="margin:0; font-size:20px;">任务</n-h2>
        <n-tag v-if="stats.total" size="small" round :bordered="false" type="primary">{{ stats.total }}</n-tag>
      </div>
      <n-button size="small" quaternary @click="fetchTasks">
        <template #icon><AppIcon name="refresh" :size="14" /></template>
        刷新
      </n-button>
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
          v-if="['running','failed'].includes(f.key)"
          :value="(stats as Record<string,number>)[f.key]"
          :max="99" :show="(stats as Record<string,number>)[f.key] > 0"
        />
      </n-tag>
    </n-space>

    <n-spin :show="tasksLoading && tasks.length === 0">
      <div v-if="filteredTasks.length === 0" style="text-align:center; padding:40px; color:var(--n-text-color-3);">
        <span style="display:flex; justify-content:center; margin-bottom:8px;"><AppIcon name="tasks" :size="32" :stroke-width="1.5" /></span>
        <div>暂无任务</div>
      </div>

      <!-- 紧凑任务行:圆点+状态文字替代整块彩色标签,单行头部,进度与元信息合并 -->
      <div v-for="task in filteredTasks" :key="task.task_id" class="task-item">
        <div class="task-head">
          <span class="dot" :style="{ background: statusDot(task.status) }"></span>
          <span class="t-name">{{ displayName(task.plugin_id) }}</span>
          <span class="t-type">{{ task.type }}</span>
          <span v-if="task.file_name" class="t-file" :title="task.output_path || task.file_name">{{ task.file_name }}</span>
          <span class="t-status" :style="{ color: statusDot(task.status) }">{{ STATUS_LABEL[task.status] || task.status }}</span>
          <span class="spacer"></span>
          <span class="t-time" :title="task.task_id">{{ fmtTime(task.created_at) }}</span>
          <button
            v-if="task.output_path"
            class="icon-btn" title="打开输出文件夹" @click="openOutputDir(task)"
          ><AppIcon name="folder" :size="13" /></button>
          <button
            v-if="task.status === 'running' || task.status === 'pending'"
            class="icon-btn" title="暂停" @click="handlePause(task)"
          ><AppIcon name="pause" :size="13" /></button>
          <button
            v-if="task.status === 'paused'"
            class="icon-btn success" title="恢复" @click="handleResume(task)"
          ><AppIcon name="play" :size="13" /></button>
          <button
            v-if="task.status === 'running' || task.status === 'pending' || task.status === 'paused'"
            class="icon-btn danger" title="取消" @click="handleCancel(task)"
          ><AppIcon name="close" :size="13" /></button>
        </div>

        <div v-if="task.status === 'running' || task.status === 'pending' || task.status === 'paused'" class="task-prog">
          <n-progress type="line" :percentage="Math.round(task.progress.percent)" :show-indicator="false" :height="4" :border-radius="2" />
          <div class="prog-meta">
            <span>{{ task.progress.percent.toFixed(0) }}%</span>
            <span v-if="task.progress.speed" class="meta-item">速度 {{ task.progress.speed }}</span>
            <span v-if="liveEta(task)" class="meta-item eta">剩余 {{ liveEta(task) }}</span>
            <span v-if="task.progress.message" class="meta-msg">{{ task.progress.message }}</span>
          </div>
        </div>

        <div v-if="(task.status === 'failed' || task.status === 'interrupted') && task.error" class="task-err">❌ {{ task.error }}</div>
      </div>
    </n-spin>
  </div>
</template>

<style scoped>
.filter-btn { cursor: pointer; }

/* ---- 紧凑任务行 ---- */
.task-item {
  background: #fff;
  border: 1px solid #e8eaef;
  border-radius: 10px;
  padding: 12px 16px;
  margin-bottom: 10px;
  transition: box-shadow .15s, border-color .15s;
}
.task-item:hover {
  border-color: #d4dcf0;
  box-shadow: 0 2px 8px rgba(31,35,41,.05);
}
.task-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  line-height: 1.4;
}
.dot {
  width: 7px; height: 7px; border-radius: 50%;
  flex-shrink: 0;
}
.t-name { font-weight: 500; color: var(--n-text-color); }
.t-type {
  font-size: 11px; color: var(--n-text-color-3);
  background: var(--n-color-target);
  border: 1px solid var(--n-divider-color);
  border-radius: 4px;
  padding: 0 6px;
  line-height: 16px;
}
.t-file {
  font-size: 12px; color: var(--n-text-color-2);
  max-width: 240px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  cursor: default;
}
.t-status { font-size: 12px; font-weight: 500; }
.spacer { flex: 1; }
.t-time { font-size: 11px; color: var(--n-text-color-3); font-variant-numeric: tabular-nums; }
.icon-btn {
  display: inline-flex; align-items: center; justify-content: center;
  width: 24px; height: 24px;
  border: none; border-radius: 6px;
  background: transparent; color: var(--n-text-color-2);
  cursor: pointer; flex-shrink: 0;
}
.icon-btn:hover { background: var(--n-hover-color); }
.icon-btn.success { color: #18a058; }
.icon-btn.success:hover { background: rgba(24, 160, 88, .12); }
.icon-btn.danger { color: #d54941; }
.icon-btn.danger:hover { background: rgba(213, 73, 65, .12); }

.task-prog { margin-top: 8px; }
.prog-meta {
  display: flex; align-items: center; gap: 10px;
  margin-top: 4px; font-size: 12px;
  color: var(--n-text-color-3);
  font-variant-numeric: tabular-nums;
}
.meta-item { white-space: nowrap; }
.meta-item.eta { color: #3370ff; font-weight: 500; }
.meta-msg {
  flex: 1; min-width: 0;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  text-align: right;
  color: var(--n-text-color-3);
}
.task-err {
  margin-top: 6px; font-size: 12px; color: #d54941;
  word-break: break-all;
}
</style>
