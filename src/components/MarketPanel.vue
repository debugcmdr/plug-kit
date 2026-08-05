<script setup lang="ts">
import { ref, onMounted } from 'vue'
import type { PluginInfo } from '../types'
import { useMessage } from 'naive-ui'

const message = useMessage()
const plugins = ref<PluginInfo[]>([])
const loading = ref(false)
const searchQuery = ref('')

onMounted(async () => {
  await fetchPlugins()
})

async function fetchPlugins() {
  loading.value = true
  try {
    // TODO: Fetch from Tauri backend or marketplace API
    plugins.value = []
  } catch (e) {
    message.error('Failed to fetch plugins')
  } finally {
    loading.value = false
  }
}

async function installPlugin(plugin: PluginInfo) {
  message.info(`Installing ${plugin.name}...`)
  // TODO: Call Tauri install command
}

const filteredPlugins = () => {
  if (!searchQuery.value) return plugins.value
  const query = searchQuery.value.toLowerCase()
  return plugins.value.filter(p =>
    p.name.toLowerCase().includes(query) ||
    p.description.toLowerCase().includes(query)
  )
}
</script>

<template>
  <div>
    <n-h2>插件市场</n-h2>
    <n-input
      v-model:value="searchQuery"
      placeholder="搜索插件..."
      clearable
      style="margin-bottom: 24px; max-width: 400px;"
    >
      <template #prefix>
        <n-icon><search /></n-icon>
      </template>
    </n-input>
    
    <n-spin :show="loading">
      <n-grid :cols="24" :x-gap="16" :y-gap="16">
        <n-gi v-for="plugin in filteredPlugins()" :key="plugin.id" span="12">
          <n-card style="height: 100%;">
            <template #header>
              <div style="display: flex; justify-content: space-between; align-items: center;">
                <n-h4 style="margin: 0;">{{ plugin.name }}</n-h4>
                <n-tag size="small">{{ plugin.version }}</n-tag>
              </div>
            </template>
            <p style="color: var(--n-text-color-2); margin-bottom: 16px;">
              {{ plugin.description }}
            </p>
            <template #footer>
              <n-space justify="end">
                <n-button
                  v-if="!plugin.is_installed"
                  type="primary"
                  @click="installPlugin(plugin)"
                >
                  安装
                </n-button>
                <n-button v-else type="default" disabled>
                  已安装
                </n-button>
              </n-space>
            </template>
          </n-card>
        </n-gi>
      </n-grid>
      
      <n-empty v-if="!loading && filteredPlugins().length === 0" description="暂无插件" />
    </n-spin>
  </div>
</template>
