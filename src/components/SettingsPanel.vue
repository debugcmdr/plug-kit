<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { CacheStats } from '../types'
import { useMessage } from 'naive-ui'
import { appUpdate, fetchAppVersion } from '../stores/plugins'
import AppIcon from './AppIcon.vue'

const message = useMessage()
const cacheStats = ref<CacheStats | null>(null)

// 应用版本从 Rust 编译注入透出(get_app_version = CARGO_PKG_VERSION),
// 与更新检查的比较基准一致;启动时 checkAppUpdate 已填充 appUpdate(设置图标红点)。
const APP_VERSION = ref('')

async function loadCacheStats() {
  try {
    cacheStats.value = await invoke<CacheStats>('get_cache_stats')
  } catch (e) {
    console.error('Failed to load cache stats:', e)
  }
}

onMounted(async () => {
  loadCacheStats()
  APP_VERSION.value = await fetchAppVersion()
})

// 有外壳新版本时跳转 GitHub Releases(仅告知,不下载覆盖)
async function openReleases() {
  try {
    await invoke('open_url', {
      url: appUpdate.value.url || 'https://github.com/debugcmdr/plug-kit/releases',
    })
  } catch (e) {
    message.error(`打开更新页面失败: ${e}`)
  }
}

async function cleanOrphanCache() {
  try {
    const r = await invoke<any>('clean_orphan_cache')
    message.success(`已清理 ${(r.freed_mb ?? 0).toFixed(2)} MB 孤儿缓存`)
    await loadCacheStats()
  } catch (e) {
    message.error(`清理失败: ${e}`)
  }
}

// 打开日志目录 / 数据目录(失败明确提示,不再静默产生 unhandled rejection)
async function openLogDir() {
  try {
    const home = await invoke<string>('get_home_dir')
    await invoke('open_in_folder', { path: `${home}/.plugkit/logs` })
  } catch (e) {
    message.error(`打开日志目录失败: ${e}`)
  }
}
async function openDataDir() {
  try {
    const home = await invoke<string>('get_home_dir')
    await invoke('open_in_folder', { path: `${home}/.plugkit` })
  } catch (e) {
    message.error(`打开数据目录失败: ${e}`)
  }
}
</script>

<template>
  <div style="padding: 24px; display: flex; flex-direction: column; gap: 20px;">
    <div style="display:flex; align-items:center; gap:10px;">
      <span style="display:inline-flex; align-items:center; justify-content:center; width:34px; height:34px; border-radius:10px; background:rgba(51,112,255,.1); color:var(--n-primary-color);"><AppIcon name="settings" :size="18" /></span>
      <n-h2 style="margin: 0; font-size:20px;">设置</n-h2>
    </div>

    <!-- 依赖缓存 -->
    <n-card title="依赖缓存">
      <n-descriptions v-if="cacheStats" :column="3" label-placement="left" size="small">
        <n-descriptions-item label="已用空间">
          <span>
            {{ cacheStats.total_size_mb.toFixed(1) }} MB
          </span>
        </n-descriptions-item>
        <n-descriptions-item label="缓存条目">{{ cacheStats.entry_count }} 个</n-descriptions-item>
      </n-descriptions>
      <n-space style="margin-top: 16px;">
        <n-button size="small" @click="loadCacheStats">刷新</n-button>
        <n-button size="small" type="error" @click="cleanOrphanCache">清理孤儿缓存</n-button>
      </n-space>
    </n-card>

    <!-- 关于 -->
    <n-card title="关于">
      <n-space direction="vertical" :size="8">
        <div style="display: flex; justify-content: space-between; align-items: center;">
          <span style="color: var(--n-text-color-2);">版本</span>
          <span style="display:flex; align-items:center; gap:8px;">
            <n-tag size="small">{{ APP_VERSION }}</n-tag>
            <!-- 外壳新版本提示:红点已在设置图标;此处给出版本与跳转入口(仅告知) -->
            <template v-if="appUpdate.has_update">
              <n-tag size="small" type="error" round :bordered="false">有新版本 v{{ appUpdate.latest_version }}</n-tag>
              <n-button size="tiny" type="primary" @click="openReleases">查看更新</n-button>
            </template>
          </span>
        </div>
        <div style="display: flex; justify-content: space-between;">
          <span style="color: var(--n-text-color-2);">数据目录</span>
          <span style="font-size: 13px; color: var(--n-text-color-3);">~/.plugkit/</span>
        </div>
        <n-space style="margin-top: 4px;">
          <n-button size="small" quaternary @click="openLogDir">打开日志目录</n-button>
          <n-button size="small" quaternary @click="openDataDir">打开数据目录</n-button>
        </n-space>
      </n-space>
    </n-card>
  </div>
</template>
