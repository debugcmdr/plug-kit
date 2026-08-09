<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useMessage, useDialog } from 'naive-ui'
import {
  marketPlugins,
  marketLoading,
  fetchMarket,
  startMarketPoll,
  stopMarketPoll,
  installPlugin as installFromStore,
  uninstallPlugin as uninstallFromStore,
} from '../stores/plugins'

const message = useMessage()
const dialog = useDialog()
const searchQuery = ref('')
const manualRefreshLoading = ref(false)

onMounted(() => {
  startMarketPoll()
})
onUnmounted(stopMarketPoll)

async function manualRefresh() {
  manualRefreshLoading.value = true
  try {
    await fetchMarket(true)
  } finally {
    manualRefreshLoading.value = false
  }
}

async function installPlugin(plugin: typeof marketPlugins.value[0]) {
  message.info(`正在安装 ${plugin.name}...`)
  try {
    const r = await installFromStore(plugin.id)
    r.success ? message.success(r.message) : message.error(r.message)
  } catch (e) {
    message.error(`安装失败: ${e}`)
  }
}

function confirmUninstall(plugin: typeof marketPlugins.value[0]) {
  dialog.warning({
    title: '卸载插件',
    content: `确定要卸载「${plugin.name}」吗?`,
    positiveText: '卸载',
    negativeText: '取消',
    onPositiveClick: async () => {
      try {
        const r = await uninstallFromStore(plugin.id)
        r.success ? message.success(r.message) : message.error(r.message)
      } catch (e) {
        message.error(`卸载失败: ${e}`)
      }
    }
  })
}

const filteredPlugins = () => {
  if (!searchQuery.value) return marketPlugins.value
  const query = searchQuery.value.toLowerCase()
  return marketPlugins.value.filter(p =>
    p.name.toLowerCase().includes(query) ||
    p.description.toLowerCase().includes(query)
  )
}
</script>

<template>
  <div style="padding: 24px; display: flex; flex-direction: column; gap: 0;">
    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 24px;">
      <n-h2 style="margin: 0;">插件市场</n-h2>
      <n-button size="small" :loading="manualRefreshLoading" @click="manualRefresh">刷新</n-button>
    </div>
    <n-input
      v-model:value="searchQuery"
      placeholder="搜索插件..."
      clearable
      style="margin-bottom: 24px; max-width: 400px;"
    />

    <n-spin :show="marketLoading && marketPlugins.length === 0">
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

      <n-empty v-if="!marketLoading && filteredPlugins().length === 0" description="暂无插件" />
    </n-spin>
  </div>
</template>
