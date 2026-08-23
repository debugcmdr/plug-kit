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

// ---- 卡片视觉:对齐首页(home-card)样式与动画 ----
const CARD_COLORS = ['#3370ff', '#18a058', '#f0a50b', '#722ed1', '#d54941', '#0fc6c2', '#2f54eb', '#f53f3f']
function pluginColor(id: string) {
  let h = 0
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) % 997
  return CARD_COLORS[h % CARD_COLORS.length]
}
</script>

<template>
  <div style="padding: 24px; display: flex; flex-direction: column; gap: 0;">
    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 24px;">
      <div style="display:flex; align-items:center; gap:10px;">
        <span style="display:inline-flex; align-items:center; justify-content:center; width:34px; height:34px; border-radius:10px; background:rgba(51,112,255,.1); color:var(--n-primary-color);"><AppIcon name="market" :size="18" /></span>
        <n-h2 style="margin: 0; font-size:20px;">插件市场</n-h2>
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
          <!-- 卡片视觉与首页 home-card 同源:圆角14 / hover 浮起+阴影+边框变蓝 / 图标胶囊 / 箭头动效 -->
          <div class="mk-card">
            <div class="mk-head">
              <span class="mk-icon" :style="{ color: pluginColor(plugin.id), background: pluginColor(plugin.id) + '1a' }">
                <AppIcon name="tool" :size="22" />
              </span>
              <div class="mk-titles">
                <div class="mk-title">{{ plugin.name }}</div>
                <div class="mk-vers">
                  <span class="mk-ver">v{{ plugin.version }}</span>
                  <span
                    v-if="plugin.is_installed && plugin.installed_version"
                    class="mk-installed"
                    :class="{ upd: hasUpdate(plugin) }"
                  >
                    {{ hasUpdate(plugin) ? '可更新 v' + plugin.installed_version : '已安装 v' + plugin.installed_version }}
                  </span>
                  <span v-else-if="plugin.is_installed" class="mk-installed">已安装</span>
                </div>
              </div>
              <span class="mk-arrow"><AppIcon name="arrow" :size="15" /></span>
            </div>
            <p class="mk-desc">{{ plugin.description }}</p>
            <div class="mk-foot">
              <template v-if="!plugin.is_installed">
                <button class="mk-btn primary" @click="installPlugin(plugin)">安装</button>
              </template>
              <template v-else>
                <button v-if="hasUpdate(plugin)" class="mk-btn primary" @click="installPlugin(plugin)">更新</button>
                <button class="mk-btn danger" @click="confirmUninstall(plugin)">卸载</button>
              </template>
            </div>
          </div>
        </n-gi>
      </n-grid>

      <n-empty v-if="!marketLoading && filteredPlugins().length === 0" description="暂无插件" />
    </n-spin>
  </div>
</template>

<style scoped>
/* 与首页 home-card 同视觉语言 */
.mk-card {
  position: relative;
  padding: 20px 22px;
  border-radius: 14px;
  background: #fff;
  border: 1px solid #e8eaef;
  cursor: default;
  transition: transform 0.18s, box-shadow 0.18s, border-color 0.18s;
  height: 100%;
  display: flex;
  flex-direction: column;
}
.mk-card:hover {
  transform: translateY(-3px);
  box-shadow: 0 8px 24px rgba(31, 35, 41, .08);
  border-color: #b8d0ff;
}
.mk-head { display: flex; align-items: center; gap: 12px; margin-bottom: 12px; }
.mk-icon {
  display: inline-flex; align-items: center; justify-content: center;
  width: 44px; height: 44px; border-radius: 12px;
  flex-shrink: 0;
}
.mk-titles { flex: 1; min-width: 0; }
.mk-title { font-size: 16px; font-weight: 600; color: #1d2129; margin-bottom: 5px; }
.mk-vers { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
.mk-ver {
  font-size: 11px; color: #3370ff; background: rgba(51,112,255,.08);
  padding: 1px 8px; border-radius: 4px; font-weight: 500;
}
.mk-installed { font-size: 11px; color: #18a058; background: rgba(24,160,88,.08); padding: 1px 8px; border-radius: 4px; }
.mk-installed.upd { color: #f0a50b; background: rgba(240,165,11,.1); }
.mk-arrow { color: #c9cdd4; transition: color 0.18s, transform 0.18s; flex-shrink: 0; }
.mk-card:hover .mk-arrow { color: #3370ff; transform: translateX(2px); }
.mk-desc {
  flex: 1;
  margin: 0 0 14px;
  font-size: 13px; color: #86909c; line-height: 1.6;
  display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
}
.mk-foot { display: flex; justify-content: flex-end; gap: 8px; }
.mk-btn {
  border: none; border-radius: 8px; padding: 7px 20px;
  font-size: 13px; cursor: pointer; transition: filter .15s, transform .15s;
}
.mk-btn:hover { filter: brightness(1.08); transform: translateY(-1px); }
.mk-btn.primary { background: #3370ff; color: #fff; }
.mk-btn.danger { background: rgba(213,73,65,.1); color: #d54941; }
.mk-btn.danger:hover { background: rgba(213,73,65,.16); }
</style>
