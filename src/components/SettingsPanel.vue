<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Settings, CacheStats } from '../types'
import { useMessage } from 'naive-ui'

const message = useMessage()
const settings = ref<Settings>({
  download_mirrors: [],
  disk_quota_mb: 500
})
const cacheStats = ref<CacheStats | null>(null)

// 应用版本从 tauri.conf.json 编译注入（通过环境变量）
const APP_VERSION = import.meta.env.VITE_APP_VERSION || '0.1.0'

async function loadSettings() {
  try {
    settings.value = await invoke<Settings>('get_settings')
  } catch (e) {
    console.error('Failed to load settings:', e)
  }
}

async function loadCacheStats() {
  try {
    cacheStats.value = await invoke<CacheStats>('get_cache_stats')
  } catch (e) {
    console.error('Failed to load cache stats:', e)
  }
}

onMounted(async () => {
  await Promise.all([loadSettings(), loadCacheStats()])
})

async function saveSettings() {
  try {
    await invoke('set_settings', { settings: JSON.parse(JSON.stringify(settings.value)) })
    message.success('设置已保存')
  } catch (e) {
    message.error(`保存失败: ${e}`)
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

// 打开日志目录 / 数据目录
async function openLogDir() {
  const home = await invoke<string>('get_home_dir')
  await invoke('open_in_folder', { path: `${home}/.plugkit/logs` })
}
async function openDataDir() {
  const home = await invoke<string>('get_home_dir')
  await invoke('open_in_folder', { path: `${home}/.plugkit` })
}
</script>

<template>
  <div style="padding: 24px; display: flex; flex-direction: column; gap: 20px;">
    <n-h2 style="margin: 0;">设置</n-h2>

    <!-- 下载镜像 -->
    <n-card title="下载镜像">
      <n-form :model="settings" label-placement="left" label-width="100">
        <n-form-item label="磁盘配额">
          <n-input-number v-model:value="settings.disk_quota_mb" :min="100" :max="10000" />
          <span style="margin-left: 8px; color: var(--n-text-color-3); font-size: 13px;">MB</span>
        </n-form-item>

        <n-form-item label="镜像源列表">
          <div style="width: 100%;">
            <div
              v-for="mirror in settings.download_mirrors"
              :key="mirror.name"
              style="display: flex; align-items: center; gap: 12px; padding: 8px 0; border-bottom: 1px solid var(--n-border-color);"
            >
              <span style="width: 60px; flex-shrink: 0; font-size: 13px; color: var(--n-text-color-2);">{{ mirror.name }}</span>
              <n-input
                v-model:value="mirror.prefix"
                placeholder="镜像前缀，留空=原始地址"
                size="small"
                style="flex: 1;"
              />
              <n-switch
                :value="mirror.enabled"
                size="small"
                @update:value="(v: boolean) => { mirror.enabled = v }"
              />
              <span style="font-size: 12px; color: var(--n-text-color-3); width: 36px; text-align: center;">
                {{ mirror.enabled ? '启用' : '禁用' }}
              </span>
            </div>
          </div>
        </n-form-item>

        <n-form-item>
          <n-button type="primary" @click="saveSettings">保存设置</n-button>
        </n-form-item>
      </n-form>
    </n-card>

    <!-- 依赖缓存 -->
    <n-card title="依赖缓存">
      <n-descriptions v-if="cacheStats" :column="3" label-placement="left" size="small">
        <n-descriptions-item label="已用空间">
          <span :style="{ color: cacheStats.total_size_mb > cacheStats.quota_mb * 0.8 ? '#f53f3f' : 'inherit' }">
            {{ cacheStats.total_size_mb.toFixed(1) }} MB
          </span>
        </n-descriptions-item>
        <n-descriptions-item label="配额限制">{{ cacheStats.quota_mb }} MB</n-descriptions-item>
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
        <div style="display: flex; justify-content: space-between;">
          <span style="color: var(--n-text-color-2);">版本</span>
          <n-tag size="small">{{ APP_VERSION }}</n-tag>
        </div>
        <div style="display: flex; justify-content: space-between;">
          <span style="color: var(--n-text-color-2);">协议</span>
          <span style="font-size: 13px;">stdout JSON (每行一个对象)</span>
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

    <!-- 热开发 -->
    <n-card title="热开发">
      <n-space direction="vertical" :size="8">
        <p style="font-size: 13px; color: var(--n-text-color-2); margin: 0;">
          开发者模式：修改插件文件后自动同步到运行时目录，无需重启应用。
        </p>
        <n-space>
          <n-tag type="success" size="small">插件 UI/CLI</n-tag>
          <span style="font-size: 13px; color: var(--n-text-color-3);">1s 轮询自动同步</span>
        </n-space>
        <n-space>
          <n-tag type="info" size="small">Vue 前端</n-tag>
          <span style="font-size: 13px; color: var(--n-text-color-3);">Vite HMR 即时刷新</span>
        </n-space>
        <n-space>
          <n-tag type="warning" size="small">Rust 后端</n-tag>
          <span style="font-size: 13px; color: var(--n-text-color-3);">自动重编译（约 10-20s）</span>
        </n-space>
      </n-space>
    </n-card>
  </div>
</template>
