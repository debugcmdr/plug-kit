<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo, InstallResult } from '../types'
import { useMessage, useDialog } from 'naive-ui'

const message = useMessage()
const dialog = useDialog()
const plugins = ref<PluginInfo[]>([])
const loading = ref(false)
const searchQuery = ref('')

onMounted(fetchPlugins)

async function fetchPlugins() {
  loading.value = true
  try {
    plugins.value = await invoke<PluginInfo[]>('market_list')
  } catch (e) {
    message.error(`拉取插件市场失败: ${e}`)
  } finally {
    loading.value = false
  }
}

async function installPlugin(plugin: PluginInfo) {
  message.info(`正在安装 ${plugin.name}...`)
  try {
    const r = await invoke<InstallResult>('install_plugin', { pluginId: plugin.id })
    if (r.success) {
      message.success(r.message)
    } else {
      message.error(r.message)
    }
    await fetchPlugins()
  } catch (e) {
    message.error(`安装失败: ${e}`)
  }
}

function confirmUninstall(plugin: PluginInfo) {
  dialog.warning({
    title: '卸载插件',
    content: `确定要卸载「${plugin.name}」吗?`,
    positiveText: '卸载',
    negativeText: '取消',
    onPositiveClick: async () => {
      try {
        const r = await invoke<InstallResult>('uninstall_plugin', { pluginId: plugin.id })
        r.success ? message.success(r.message) : message.error(r.message)
        await fetchPlugins()
      } catch (e) {
        message.error(`卸载失败: ${e}`)
      }
    }
  })
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
    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 24px;">
      <n-h2 style="margin: 0;">插件市场</n-h2>
      <n-button size="small" @click="fetchPlugins" :loading="loading">刷新</n-button>
    </div>
    <n-input
      v-model:value="searchQuery"
      placeholder="搜索插件..."
      clearable
      style="margin-bottom: 24px; max-width: 400px;"
    />

    <n-spin :show="loading">
      <n-grid :cols="24" :x-gap="16" :y-gap="16">
        <n-gi v-for="plugin in filteredPlugins()" :key="plugin.id" span="12">
          <n-card style="height: 100%;">
            <template #header>
              <div style="display: flex; justify-content: space-between; align-items: center;">
                <n-h4 style="margin: 0;">{{ plugin.name }}</n-h4>
                <n-space :size="4">
                  <n-tag size="small" type="info">{{ plugin.version }}</n-tag>
                  <n-tag v-if="plugin.is_installed" size="small" type="success">已安装</n-tag>
                </n-space>
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
                  :loading="loading"
                  @click="installPlugin(plugin)"
                >
                  安装
                </n-button>
                <n-button
                  v-else
                  type="error"
                  quaternary
                  @click="confirmUninstall(plugin)"
                >
                  卸载
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
