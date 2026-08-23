<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import AppIcon from './AppIcon.vue'

interface TaskError { plugin: string; task_id: string; code: string; message: string }
interface LogItem {
  raw: string
  ts?: string
  level?: string
  taskError?: TaskError
  expanded: boolean
}

const items = ref<LogItem[]>([])
const loading = ref(false)
const autoScroll = ref(false)  // 默认手动滚动:用户主动查看/筛选日志时不被自动滚走
const filterLevel = ref<string>('all')

const LEVELS = [
  { key: 'all', label: '全部' },
  { key: 'INFO', label: 'INFO' },
  { key: 'WARN', label: 'WARN' },
  { key: 'ERROR', label: 'ERROR' },
] as const

onMounted(fetchLogs)

async function fetchLogs() {
  loading.value = true
  try {
    const lines = await invoke<string[]>('log_read', {
      lines: 500,
      level: filterLevel.value === 'all' ? undefined : filterLevel.value,
    })
    items.value = lines.map(raw => {
      const m = raw.match(/^\[(.*?)\] \[(.*?)\] (.*)$/)
      if (m && m[2] === 'TASK_ERROR') {
        try {
          const d = JSON.parse(m[3]) as TaskError
          return { raw, ts: m[1], level: 'TASK_ERROR', taskError: d, expanded: false }
        } catch { /* 解析失败落到普通行 */ }
      }
      return { raw, ts: m?.[1], level: m?.[2], expanded: false }
    })
    nextTick(() => { if (autoScroll.value) scrollBottom() })
  } catch (e) {
    console.error('Failed to fetch logs:', e)
  } finally {
    loading.value = false
  }
}

function scrollBottom() {
  const el = document.getElementById('log-scroll')
  el?.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
}

function toggle(item: LogItem) { item.expanded = !item.expanded }

// 折叠态摘要:压平换行后截断
function summary(msg: string) {
  const one = msg.replace(/\s+/g, ' ').trim()
  return one.length > 90 ? one.slice(0, 90) + '…' : one
}

function lineClass(item: LogItem) {
  if (item.level === 'TASK_ERROR' || item.level === 'ERROR') return 'log-error'
  if (item.level === 'WARN') return 'log-warn'
  return ''
}
</script>

<template>
  <div style="padding: 20px 24px 12px; display:flex; flex-direction:column; gap:10px; height:100%;">
    <!-- 工具栏 -->
    <div style="display:flex; justify-content:space-between; align-items:center; flex-shrink:0;">
      <div style="display:flex; align-items:center; gap:8px;">
        <span style="display:inline-flex; align-items:center; justify-content:center; width:30px; height:30px; border-radius:9px; background:rgba(51,112,255,.1); color:var(--n-primary-color);"><AppIcon name="logs" :size="16" /></span>
        <n-h3 style="margin:0; font-size:17px;">日志</n-h3>
      </div>
      <n-space size="small">
        <n-button size="small" quaternary @click="fetchLogs">
          <template #icon><AppIcon name="refresh" :size="14" /></template>刷新</n-button>
        <n-tag
          :type="autoScroll ? 'success' : 'default'"
          round size="small"
          style="cursor:pointer; font-size:12px;"
          @click="autoScroll = !autoScroll"
        >
          {{ autoScroll ? '自动滚动' : '手动滚动' }}
        </n-tag>
      </n-space>
    </div>

    <!-- 级别过滤 -->
    <n-radio-group v-model:value="filterLevel" @change="fetchLogs" size="small" style="flex-shrink:0;">
      <n-radio-button v-for="l in LEVELS" :key="l.key" :value="l.key">{{ l.label }}</n-radio-button>
    </n-radio-group>

    <!-- 日志内容区 -->
    <div
      id="log-scroll"
      style="flex:1; min-height:0; overflow:auto; background:var(--n-color); border-radius:8px; padding:10px 12px;
             font-family:Monaco,Menlo,Consolas,monospace; font-size:12px; line-height:1.75;
             color:var(--n-text-color-1);"
    >
      <div v-if="items.length === 0" style="color:var(--n-text-color-3); text-align:center; padding:40px;">
        {{ loading ? '加载中…' : '暂无日志' }}
      </div>
      <template v-for="(item, i) in items">
        <!-- 任务错误:结构化条目,默认折叠;展开看完整错误(比插件 UI 更详细,含错误码) -->
        <div v-if="item.taskError" :key="'e' + i" class="task-error">
          <div class="te-head" @click="toggle(item)" :title="item.expanded ? '收起' : '展开完整错误'">
            <span class="te-chevron">{{ item.expanded ? '▾' : '▸' }}</span>
            <span class="te-code">{{ item.taskError.code }}</span>
            <span class="te-plugin">{{ item.taskError.plugin }}</span>
            <span class="te-summary">{{ summary(item.taskError.message) }}</span>
          </div>
          <div v-if="item.expanded" class="te-detail">
            <div class="te-meta">{{ item.ts }} · task {{ item.taskError.task_id.slice(0, 8) }}</div>
            {{ item.taskError.message }}
          </div>
        </div>
        <!-- 普通行 -->
        <div v-else :key="'l' + i" :class="lineClass(item)">{{ item.raw }}</div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.log-error { color: #f53f3f; }
.log-warn  { color: #e8a838; }

/* 任务错误条目:红色左边条 + 可点击头部,默认折叠 */
.task-error { border-left: 3px solid #f53f3f; margin: 5px 0; background: rgba(245,63,63,.05); border-radius: 4px; }
.te-head { display: flex; align-items: center; gap: 8px; padding: 5px 10px; cursor: pointer; user-select: none; }
.te-head:hover { background: rgba(245,63,63,.09); }
.te-chevron { color: #f53f3f; font-size: 11px; flex-shrink: 0; }
.te-code { color: #f53f3f; font-weight: 600; font-size: 12px; flex-shrink: 0; }
.te-plugin { color: #4e5969; background: #eef0f3; border-radius: 4px; padding: 0 6px; font-size: 11px; flex-shrink: 0; }
.te-summary { color: #4e5969; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.te-detail { padding: 6px 10px 9px 30px; color: #1f2329; white-space: pre-wrap; word-break: break-all; border-top: 1px dashed rgba(245,63,63,.18); }
.te-meta { font-size: 11px; color: #a0a4ab; margin-bottom: 3px; }
</style>
