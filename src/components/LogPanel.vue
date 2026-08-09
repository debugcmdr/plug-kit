<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const rawLines = ref<string[]>([])
const loading = ref(false)
const autoScroll = ref(true)
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
    rawLines.value = await invoke<string[]>('log_read', {
      lines: 500,
      level: filterLevel.value === 'all' ? undefined : filterLevel.value,
    })
    nextTick(() => {
      if (autoScroll.value) scrollBottom()
    })
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

function clearLogs() {
  rawLines.value = []
}

function lineClass(line: string) {
  if (line.includes('[ERROR]')) return 'log-error'
  if (line.includes('[WARN]'))  return 'log-warn'
  return ''
}
</script>

<template>
  <div style="padding: 20px 24px 12px; display:flex; flex-direction:column; gap:10px; height:100%;">
    <!-- 工具栏 -->
    <div style="display:flex; justify-content:space-between; align-items:center; flex-shrink:0;">
      <n-h3 style="margin:0; font-size:16px;">日志</n-h3>
      <n-space size="small">
        <n-button size="small" quaternary @click="fetchLogs">刷新</n-button>
        <n-button size="small" quaternary type="error" @click="clearLogs">清空</n-button>
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
      style="flex:1; min-height:0; overflow:auto; background:var(--n-color); border-radius:8px; padding:12px 14px;
             font-family:Monaco,Menlo,Consolas,monospace; font-size:12px; line-height:1.75;
             color:var(--n-text-color-1);"
    >
      <div v-if="rawLines.length === 0" style="color:var(--n-text-color-3); text-align:center; padding:40px;">
        暂无日志
      </div>
      <div v-for="(line, i) in rawLines" :key="i" :class="lineClass(line)">
        {{ line }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.log-error { color: #f53f3f; }
.log-warn  { color: #e8a838; }
</style>
