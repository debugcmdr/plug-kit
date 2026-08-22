<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import PluginPanel from '../components/PluginPanel.vue'
import MarketPanel from '../components/MarketPanel.vue'
import SettingsPanel from '../components/SettingsPanel.vue'
import TaskPanel from '../components/TaskPanel.vue'
import LogPanel from '../components/LogPanel.vue'
import AppIcon from '../components/AppIcon.vue'

const props = defineProps<{
  activePlugin: string | null
}>()

const emit = defineEmits<{
  'update:activePlugin': [v: string]
}>()

function go(target: string) {
  emit('update:activePlugin', target)
}

const view = computed<'market' | 'settings' | 'plugin' | 'tasks' | 'logs' | 'home'>(() => {
  const id = props.activePlugin
  if (id === 'home') return 'home'
  if (id === 'market') return 'market'
  if (id === 'settings') return 'settings'
  if (id === 'tasks') return 'tasks'
  if (id === 'logs') return 'logs'
  if (id) return 'plugin'
  return 'home'
})

// ---- 插件状态常驻 ----
// 关键:插件面板用 v-show(仅隐藏)而非 v-if/KeepAlive 卸载。
// iframe 一旦从 DOM 分离再插回(KeepAlive 的缓存方式)就会被浏览器重新加载,
// 插件页面内部的 JS 状态(已选文件/表单)全部丢失。
// v-show 只改 display,iframe 保持挂载,切走/切回/换插件都不重载、不丢状态。
const visitedPlugins = ref<string[]>([])
const MAX_KEPT_PLUGINS = 8
function ensureVisited(id: string) {
  if (!id || visitedPlugins.value.includes(id)) return
  visitedPlugins.value.push(id)
  if (visitedPlugins.value.length > MAX_KEPT_PLUGINS) {
    visitedPlugins.value.shift()  // 超过上限,卸载最早访问的插件(其状态随之释放)
  }
}
watch(
  () => props.activePlugin,
  (v) => { if (v && view.value === 'plugin') ensureVisited(v) },
  { immediate: true },
)

const HOME_CARDS = [
  { key: 'market', icon: 'market', color: '#3370ff', title: '插件市场', desc: '浏览并安装多媒体处理工具' },
  { key: 'tasks', icon: 'tasks', color: '#18a058', title: '任务中心', desc: '查看转换进度与剩余时间' },
  { key: 'settings', icon: 'settings', color: '#f0a50b', title: '设置', desc: '缓存管理与关于信息' },
] as const
</script>

<template>
  <div style="height: 100%; display: flex; flex-direction: column; overflow: hidden;">
    <!-- 首页 -->
    <div v-if="view === 'home'" class="home">
      <div class="home-hero">
        <span class="home-logo"><AppIcon name="logo" :size="28" /></span>
        <div>
          <n-h2 style="margin: 0 0 4px;">欢迎使用 PlugKit</n-h2>
          <n-p style="margin: 0; color: var(--n-text-color-3);">多媒体工具市场 · 选择已安装工具或前往市场安装更多</n-p>
        </div>
      </div>

      <div class="home-grid">
        <div v-for="c in HOME_CARDS" :key="c.key" class="home-card" @click="go(c.key)">
          <span class="home-card-icon" :style="{ color: c.color }"><AppIcon :name="c.icon" :size="22" /></span>
          <div class="home-card-title">{{ c.title }}</div>
          <div class="home-card-desc">{{ c.desc }}</div>
          <span class="home-card-arrow"><AppIcon name="arrow" :size="16" /></span>
        </div>
      </div>
    </div>

    <!-- 插件面板：v-show 常驻渲染(iframe 保持挂载,切走/换插件不重载、不丢状态)。
         已访问过的插件各占一个隐藏实例,当前激活的显示,其余 display:none。
         切到首页/市场/设置时整层隐藏但 iframe 不卸载 → 回来状态原样。 -->
    <div v-show="view === 'plugin'" style="flex:1; min-height:0; display:flex; flex-direction:column;">
      <PluginPanel
        v-for="pid in visitedPlugins"
        v-show="props.activePlugin === pid"
        :key="pid"
        :plugin-id="pid"
        style="flex:1; min-height:0;"
      />
    </div>

    <!-- 其他面板：各自独立 v-if,与插件面板互斥显示 -->
    <TaskPanel v-if="view === 'tasks'" style="flex:1; min-height:0; overflow:hidden;" />
    <LogPanel  v-if="view === 'logs'" style="flex:1; min-height:0; overflow:hidden;" />
    <MarketPanel v-if="view === 'market'" style="flex:1; overflow:auto;" />
    <SettingsPanel v-if="view === 'settings'" style="flex:1; overflow:auto;" />
  </div>
</template>

<style scoped>
.home {
  padding: 32px;
  overflow: auto;
  height: 100%;
}
.home-hero {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 28px;
}
.home-logo {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 14px;
  background: var(--n-primary-color-soft);
  color: var(--n-primary-color);
  flex-shrink: 0;
}
.home-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 16px;
  max-width: 760px;
}
.home-card {
  position: relative;
  padding: 20px;
  border-radius: 12px;
  background: var(--n-color-target);
  border: 1px solid var(--n-divider-color);
  cursor: pointer;
  transition: transform 0.15s, box-shadow 0.15s, border-color 0.15s;
}
.home-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.08);
  border-color: var(--n-primary-color);
}
.home-card-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: 10px;
  background: var(--n-color);
  margin-bottom: 12px;
}
.home-card-title {
  font-size: 15px;
  font-weight: 600;
  margin-bottom: 4px;
}
.home-card-desc {
  font-size: 13px;
  color: var(--n-text-color-3);
  line-height: 1.5;
}
.home-card-arrow {
  position: absolute;
  top: 18px;
  right: 18px;
  color: var(--n-text-color-3);
}
</style>
