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
  <div style="padding: 24px; min-height: 100%; display:flex; flex-direction:column; gap:12px;">
    <div style="display:flex; justify-content:space-between; align-items:center;">
      <n-h2 style="margin:0">日志</n-h2>
      <n-space>
        <n-button size="small" quaternary @click="fetchLogs">刷新</n-button>
        <n-button size="small" quaternary type="error" @click="clearLogs">清空</n-button>
        <n-tag :type="autoScroll ? 'success' : 'default'" round size="small" style="cursor:pointer" @click="autoScroll = !autoScroll">
          {{ autoScroll ? '自动滚动' : '手动滚动' }}
        </n-tag>
      </n-space>
    </div>

    <n-radio-group v-model:value="filterLevel" @change="fetchLogs" size="small">
      <n-radio-button v-for="l in LEVELS" :key="l.key" :value="l.key">{{ l.label }}</n-radio-button>
    </n-radio-group>

    <div id="log-scroll"
      style="flex:1; overflow:auto; background:#1e1e1e; border-radius:8px; padding:12px; font-family:Monaco,Menlo,Consolas,monospace; font-size:12px; line-height:1.8; color:#d4d4d4; min-height:300px; max-height:calc(100vh - 240px);"
    >
      <div v-if="rawLines.length === 0" style="color:#666; text-align:center; padding:40px;">
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
.log-warn  { color: #f0a50b; }
</style>
