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
</script>

<template>
  <div>
    <n-h2>设置</n-h2>

    <n-tabs type="line" style="margin-top: 24px;">
      <n-tab-pane name="mirrors" tab="下载镜像">
        <n-card>
          <n-form :model="settings" label-placement="left" label-width="120">
            <n-form-item label="磁盘配额 (MB)">
              <n-input-number v-model:value="settings.disk_quota_mb" :min="100" :max="10000" />
            </n-form-item>

            <n-form-item label="下载镜像源">
              <n-space direction="vertical" style="width: 100%;">
                <div
                  v-for="mirror in settings.download_mirrors"
                  :key="mirror.name"
                  style="display: flex; justify-content: space-between; align-items: center; padding: 8px 0;"
                >
                  <span style="flex: 1;">{{ mirror.name }} — {{ mirror.prefix || '(原始地址)' }}</span>
                  <n-tag :type="mirror.enabled ? 'success' : 'default'" size="small">
                    {{ mirror.enabled ? '启用' : '禁用' }}
                  </n-tag>
                </div>
              </n-space>
            </n-form-item>

            <n-form-item>
              <n-button type="primary" @click="saveSettings">保存设置</n-button>
            </n-form-item>
          </n-form>
        </n-card>
      </n-tab-pane>

      <n-tab-pane name="cache" tab="依赖缓存">
        <n-card>
          <n-descriptions v-if="cacheStats" :column="2" label-placement="left">
            <n-descriptions-item label="已用空间">
              {{ cacheStats.total_size_mb.toFixed(2) }} MB
            </n-descriptions-item>
            <n-descriptions-item label="配额限制">
              {{ cacheStats.quota_mb }} MB
            </n-descriptions-item>
            <n-descriptions-item label="缓存条目">
              {{ cacheStats.entry_count }}
            </n-descriptions-item>
          </n-descriptions>

          <n-space style="margin-top: 24px;">
            <n-button type="error" @click="cleanOrphanCache">清理孤儿缓存</n-button>
          </n-space>
        </n-card>
      </n-tab-pane>

      <n-tab-pane name="logs" tab="日志">
        <n-card>
          <n-button @click="message.info('日志功能开发中')">打开日志目录</n-button>
        </n-card>
      </n-tab-pane>
    </n-tabs>
  </div>
</template>
