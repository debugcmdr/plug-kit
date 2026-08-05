<script setup lang="ts">
import { ref, onMounted } from 'vue'
import type { Settings } from '../types'
import { useMessage } from 'naive-ui'

const message = useMessage()
const settings = ref<Settings>({
  download_mirrors: [],
  disk_quota_mb: 500
})
const cacheStats = ref<any>(null)

onMounted(async () => {
  await loadSettings()
  await loadCacheStats()
})

async function loadSettings() {
  settings.value = {
    download_mirrors: [
      { name: 'ghproxy', prefix: 'https://ghproxy.com/', enabled: true },
      { name: 'gh-proxy', prefix: 'https://gh-proxy.com/', enabled: true }
    ],
    disk_quota_mb: 500
  }
}

async function loadCacheStats() {
  cacheStats.value = {
    total_size_mb: 0,
    entry_count: 0,
    quota_mb: 500
  }
}

async function saveSettings() {
  message.success('设置已保存')
}

async function cleanCache() {
  message.success('缓存已清理')
  await loadCacheStats()
}
</script>

<template>
  <div>
    <n-h2>设置</n-h2>
    
    <n-tabs type="line" style="margin-top: 24px;">
      <n-tab-pane name="mirrors" tab="下载镜像">
        <n-card>
          <n-form :model="settings" label-placement="left" label-width="100">
            <n-form-item label="磁盘配额 (MB)">
              <n-input-number v-model:value="settings.disk_quota_mb" :min="100" :max="10000" />
            </n-form-item>
            
            <n-form-item label="下载镜像源">
              <n-space direction="vertical" style="width: 100%;">
                <n-space vertical>
                  <div v-for="mirror in settings.download_mirrors" :key="mirror.name" style="display: flex; justify-content: space-between; align-items: center; padding: 8px 0;">
                    <span>{{ mirror.name }} - {{ mirror.prefix }}</span>
                    <n-tag :type="mirror.enabled ? 'success' : 'default'" size="small">
                      {{ mirror.enabled ? '启用' : '禁用' }}
                    </n-tag>
                  </div>
                </n-space>
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
            <n-button type="error" @click="cleanCache">清理孤儿缓存</n-button>
            <n-button @click="cleanCache">清理全部缓存</n-button>
          </n-space>
        </n-card>
      </n-tab-pane>
      
      <n-tab-pane name="logs" tab="日志">
        <n-card>
          <n-button @click="message.info('日志功能开发中')">打开日志目录</n-button>
          <n-button style="margin-left: 12px;" @click="message.info('导出功能开发中')">导出诊断包</n-button>
        </n-card>
      </n-tab-pane>
    </n-tabs>
  </div>
</template>
