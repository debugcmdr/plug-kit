<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useMessage, useDialog } from 'naive-ui'
import AppIcon from './AppIcon.vue'
import {
  marketPlugins,
  marketLoading,
  fetchMarket,
  refreshMarket,
  installPlugin as installFromStore,
  uninstallPlugin as uninstallFromStore,
} from '../stores/plugins'

const message = useMessage()
const dialog = useDialog()
const searchQuery = ref('')
const manualRefreshLoading = ref(false)

onMounted(() => {
  // 全局轮询由 App 启动统一管理(离开本视图不再停掉),此处仅确保进入时有数据
  // (命中 5 分钟缓存则秒回,否则向后端拉取一次)。
  fetchMarket()
})

async function manualRefresh() {
  manualRefreshLoading.value = true
  try {
    await refreshMarket()
  } finally {
    manualRefreshLoading.value = false
  }
}

/** 简单 semver 比较:返回 >0 表示 a>b。用于判断插件是否有更新。 */
function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(n => parseInt(n, 10) || 0)
  const pb = b.split('.').map(n => parseInt(n, 10) || 0)
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0)
    if (d !== 0) return d
  }
  return 0
}

function hasUpdate(plugin: typeof marketPlugins.value[0]): boolean {
  return !!plugin.is_installed && !!plugin.installed_version &&
    compareVersions(plugin.installed_version, plugin.version) < 0
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
      <div style="display:flex; align-items:center; gap:10px;">
        <span style="color:var(--n-primary-color); display:flex;"><AppIcon name="market" :size="20" /></span>
        <n-h2 style="margin: 0;">插件市场</n-h2>
      </div>
      <n-button size="small" :loading="manualRefreshLoading" @click="manualRefresh">
        <template #icon><AppIcon name="refresh" :size="14" /></template>
        刷新
      </n-button>
    </div>
    <n-input
      v-model:value="searchQuery"
      placeholder="搜索插件..."
      clearable
      style="margin-bottom: 24px; max-width: 400px;"
    >
      <template #prefix><AppIcon name="search" :size="15" /></template>
    </n-input>

    <n-spin :show="marketLoading && marketPlugins.length === 0">
      <n-grid :cols="24" :x-gap="16" :y-gap="16">
        <n-gi v-for="plugin in filteredPlugins()" :key="plugin.id" span="12">
          <n-card style="height: 100%;">
            <template #header>
              <div style="display: flex; justify-content: space-between; align-items: center;">
                <n-h4 style="margin: 0; display:flex; align-items:center; gap:8px;">
                  <AppIcon name="tool" :size="15" style="color:var(--n-text-color-3);" />{{ plugin.name }}
                </n-h4>
                <n-space :size="4">
                  <n-tag size="small" type="info">{{ plugin.version }}</n-tag>
                  <n-tag
                    v-if="plugin.is_installed && plugin.installed_version"
                    size="small"
                    :type="hasUpdate(plugin) ? 'warning' : 'success'"
                  >
                    {{ hasUpdate(plugin) ? '可更新 v' + plugin.installed_version : '已安装 v' + plugin.installed_version }}
                  </n-tag>
                  <n-tag v-else-if="plugin.is_installed" size="small" type="success">已安装</n-tag>
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
                <template v-else>
                  <n-button
                    v-if="hasUpdate(plugin)"
                    type="primary"
                    @click="installPlugin(plugin)"
                  >
                    更新
                  </n-button>
                  <n-button
                    type="error"
                    quaternary
                    @click="confirmUninstall(plugin)"
                  >
                    卸载
                  </n-button>
                </template>
              </n-space>
            </template>
          </n-card>
        </n-gi>
      </n-grid>

      <n-empty v-if="!marketLoading && filteredPlugins().length === 0" description="暂无插件" />
    </n-spin>
  </div>
</template>
