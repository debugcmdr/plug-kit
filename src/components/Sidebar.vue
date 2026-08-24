<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { PluginInfo } from '../types'
import { installedVersion, tasks } from '../stores/plugins'
import AppIcon from './AppIcon.vue'

const emit = defineEmits<{
  select: [target: string]
}>()

const installedPlugins = ref<PluginInfo[]>([])
const activeKey = ref<string | null>(null)

// 工具排序持久化(localStorage,纯前端偏好,零后端改动)
const ORDER_KEY = 'plugkit_tool_order'
// 拖拽排序状态(pointer events):dragId=源项,dragging=按住中,suppressClick=重排后抑制误点击
let dragId: string | null = null
let dragging = false
let suppressClick = false
let dragStartY = 0        // 鼠标按下时 clientY
let dragStartTop = 0      // 被拖项按下时自然 top(未 transform)
let lastMove = 0          // mousemove 节流(rAF 级,防高频重排)
let listTop = 0           // 列表第一项自然 top(拖拽开始时缓存,动画静止态)
let rowHeight = 0         // 相邻项间距(含 margin,等高列表 → 确定性坐标)

// 运行中/排队任务计数(徽标):任务数据由 App 全局轮询维护,此处纯派生、实时更新。
const taskCount = computed(
  () => tasks.value.filter(t => t.status === 'running' || t.status === 'pending').length
)

// 平铺工具列表，从后端动态加载（不再硬编码）
const tools = computed(() =>
  installedPlugins.value.map(p => ({ id: p.id, label: p.name }))
)

// 按本地保存的顺序重排已安装插件(新安装的排到末尾)
function applySavedOrder() {
  let saved: string[] = []
  try { saved = JSON.parse(localStorage.getItem(ORDER_KEY) || '[]') } catch { /* 忽略损坏数据 */ }
  if (!saved.length) return
  const byId = new Map(installedPlugins.value.map(p => [p.id, p]))
  const ordered = saved.map(id => byId.get(id)).filter(Boolean) as PluginInfo[]
  const rest = installedPlugins.value.filter(p => !saved.includes(p.id))
  installedPlugins.value = [...ordered, ...rest]
}

async function loadInstalled() {
  try {
    const all = await invoke<PluginInfo[]>('list_plugins')
    installedPlugins.value = all.filter(p => p.is_installed)
    applySavedOrder()
  } catch (e) {
    console.error('Failed to load plugins:', e)
  }
}

// ---- 拖拽排序(鼠标跟随) ----
// HTML5 Drag & Drop 在 Tauri macOS WKWebView 中不生效;改用 mousedown/mousemove/mouseup:
// 按下记录被拖项,移动时被拖项 translateY 跟随鼠标(限在列表可视区内,不飘出),
// 掠过其他项中点即实时让位(TransitionGroup FLIP 平滑过渡),松开(任意位置)保存排序;
// 位移足够小(<4px)视为普通点击,不触发重排。
function itemEl(id: string): HTMLElement | null {
  return document.querySelector(`[data-tool-id="${id}"]`)
}

function onPointerDown(id: string, e: MouseEvent) {
  if (e.button !== 0) return
  const el = itemEl(id)
  if (!el) return
  const els = Array.from(document.querySelectorAll<HTMLElement>('[data-tool-id]'))
  if (els.length < 2) return
  // 缓存确定性坐标:列表第一项 top + 相邻行距。拖拽开始时列表静止,
  // 之后插入点/被拖项位置全部基于这两个常量计算,不依赖动画中间态的实时 rect。
  listTop = els[0].getBoundingClientRect().top
  const rowGap = els[1].getBoundingClientRect().top - els[0].getBoundingClientRect().top
  if (rowGap <= 0) return
  rowHeight = rowGap
  dragId = id
  dragging = true
  suppressClick = false
  dragStartY = e.clientY
  dragStartTop = el.getBoundingClientRect().top
  el.classList.add('drag-moving')  // transition none,transform 由 JS 全权控制
  document.addEventListener('mousemove', onPointerMove)
  document.addEventListener('mouseup', onPointerUp)
  // 兜底:鼠标拖出窗口/应用失焦时结束拖拽,避免 dragging 状态与 transform 残留
  window.addEventListener('blur', onWindowBlur)
}

function onWindowBlur() {
  if (dragging) onPointerUp()
}

function onPointerMove(e: MouseEvent) {
  if (!dragging || !dragId) return
  const now = performance.now()
  if (now - lastMove < 16) return
  lastMove = now

  const arr = [...installedPlugins.value]
  let dragIndex = arr.findIndex(p => p.id === dragId)
  if (dragIndex < 0) return

  // 1) 目标槽位:与各项理论中点(listTop + i*rowHeight + rowHeight/2)比较,
  //    掠过中点即越过该项。理论坐标 → 让位动画播放期间判断依然稳定,不来回跳。
  let to = dragIndex
  const n = arr.length
  for (let i = 0; i < n; i++) {
    if (i === dragIndex) continue
    const mid = listTop + i * rowHeight + rowHeight / 2
    if (e.clientY > mid) to = Math.max(to, i)
    else if (e.clientY < mid) to = Math.min(to, i)
  }
  if (to !== dragIndex) {
    arr.splice(to, 0, arr.splice(dragIndex, 1)[0])
    installedPlugins.value = arr
    dragIndex = to
    suppressClick = true
  }

  // 2) 被拖项跟随鼠标:目标 top = 按下时 top + 鼠标位移,
  //    减去确定性理论位置(listTop + dragIndex*rowHeight),无实时 rect 反馈 → 不抖。
  const el = itemEl(dragId)
  if (el) {
    let targetTop = dragStartTop + (e.clientY - dragStartY)
    const wrap = el.closest('.scroll')
    if (wrap) {
      const wr = wrap.getBoundingClientRect()
      const h = el.offsetHeight
      const minTop = wr.top + 4
      const maxTop = wr.bottom - h - 4
      if (targetTop < minTop) targetTop = minTop
      if (targetTop > maxTop) targetTop = maxTop
    }
    el.style.transform = `translateY(${targetTop - (listTop + dragIndex * rowHeight)}px)`
  }
}

function onPointerUp() {
  if (!dragging) return
  document.removeEventListener('mousemove', onPointerMove)
  document.removeEventListener('mouseup', onPointerUp)
  window.removeEventListener('blur', onWindowBlur)
  const el = dragId ? itemEl(dragId) : null
  if (el) {
    el.classList.remove('drag-moving')
    el.style.transform = ''  // 从鼠标位置平滑落回自然位置(0.2s transform 过渡)
  }
  dragging = false
  dragId = null
  if (suppressClick) {
    // 发生了重排:保存顺序;随后触发的 click 会被 handleSelect 抑制,避免误导航
    suppressClick = false
    localStorage.setItem(ORDER_KEY, JSON.stringify(installedPlugins.value.map(p => p.id)))
  }
}

onMounted(loadInstalled)
onUnmounted(() => {
  // 拖拽中卸载组件时兜底清理,避免 document 事件泄漏
  document.removeEventListener('mousemove', onPointerMove)
  document.removeEventListener('mouseup', onPointerUp)
})
watch(installedVersion, loadInstalled)

function handleSelect(target: string) {
  if (suppressClick) { suppressClick = false; return }  // 拖拽重排触发的 click,忽略
  activeKey.value = target
  emit('select', target)
}
</script>

<template>
  <n-layout-sider
    :width="228"
    :collapsed-width="0"
    :default-collapsed="false"
    :show-trigger="false"
    :native-scrollbar="false"
    :content-style="{ display: 'flex', flexDirection: 'column', height: '100%', background: '#fafbfc' }"
    style="height: 100%; border-right: 1px solid #e5e6eb; background: #fafbfc;"
  >
    <!-- 品牌头部(点击回到首页) -->
    <div class="brand" @click="handleSelect('home')" title="返回首页">
      <span class="brand-logo"><AppIcon name="logo" :size="20" /></span>
      <span class="brand-name">PlugKit</span>
    </div>

    <!-- 已安装工具列表 -->
    <div class="scroll">
      <div v-if="tools.length === 0" class="empty">
        暂无已安装工具<br><span>请在插件市场安装</span>
      </div>
      <template v-else>
        <div class="section-label">工具<span class="sort-tip">拖拽排序</span></div>
        <!-- TransitionGroup:列表重排时对移动元素做 transform 过渡(内置 FLIP),拖拽平滑无跳变 -->
        <TransitionGroup name="tool-list" tag="div">
          <div
            v-for="tool in tools"
            :key="tool.id"
            class="nav-item"
            :data-tool-id="tool.id"
            :class="{ active: activeKey === tool.id }"
            @mousedown="onPointerDown(tool.id, $event)"
            @click="handleSelect(tool.id)"
          >
            <AppIcon name="tool" :size="16" class="nav-icon" />
            <span class="nav-label">{{ tool.label }}</span>
          </div>
        </TransitionGroup>
      </template>
    </div>

    <!-- 底部导航：固定在底部 -->
    <div class="bottom">
      <div
        class="nav-item"
        :class="{ active: activeKey === 'tasks' }"
        @click="handleSelect('tasks')"
      >
        <AppIcon name="tasks" :size="16" class="nav-icon" />
        <span class="nav-label">任务</span>
        <n-badge v-if="taskCount > 0" :value="taskCount" :max="99" color="#f53f3f" class="nav-badge" />
      </div>
      <div
        class="nav-item"
        :class="{ active: activeKey === 'logs' }"
        @click="handleSelect('logs')"
      >
        <AppIcon name="logs" :size="16" class="nav-icon" />
        <span class="nav-label">日志</span>
      </div>
      <div
        class="nav-item"
        :class="{ active: activeKey === 'market' }"
        @click="handleSelect('market')"
      >
        <AppIcon name="market" :size="16" class="nav-icon" />
        <span class="nav-label">插件市场</span>
      </div>
      <div
        class="nav-item"
        :class="{ active: activeKey === 'settings' }"
        @click="handleSelect('settings')"
      >
        <AppIcon name="settings" :size="16" class="nav-icon" />
        <span class="nav-label">设置</span>
      </div>
    </div>
  </n-layout-sider>
</template>

<style scoped>
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 60px;
  padding: 0 18px;
  flex-shrink: 0;
  border-bottom: 1px solid #ececef;
  cursor: pointer;
  user-select: none;
  transition: background-color 0.15s;
}
.brand:hover { background: #f0f1f3; }
.brand-logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 9px;
  background: linear-gradient(135deg, #3370ff 0%, #6b9aff 100%);
  color: #fff;
  box-shadow: 0 2px 6px rgba(51, 112, 255, .3);
}
.brand-name {
  font-size: 16px;
  font-weight: 700;
  letter-spacing: 0.3px;
  color: #1d2129;
}

.scroll {
  flex: 1;
  overflow: auto;
  padding: 12px 10px;
}
.empty {
  padding: 16px;
  font-size: 13px;
  color: #86909c;
  text-align: center;
  line-height: 1.7;
}
.empty span { font-size: 12px; }

.section-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: #b0b3b8;
  padding: 10px 12px 6px;
}
.sort-tip {
  float: right;
  margin-right: 4px;
  font-weight: 400;
  letter-spacing: 0;
  text-transform: none;
  color: #b0b3b8;
  opacity: .8;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 2px 0;
  padding: 9px 12px;
  border-radius: 9px;
  cursor: pointer;
  font-size: 14px;
  color: #4e5969;
  transition: background-color 0.15s, color 0.15s, transform 0.2s ease;
  position: relative;
}
.nav-item:hover { background: #f0f1f3; }
.nav-item.active {
  background: #eef4ff;
  color: #3370ff;
  font-weight: 600;
}
.nav-item.active::before {
  content: '';
  position: absolute;
  left: -10px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 20px;
  border-radius: 2px;
  background: #3370ff;
}
/* 拖拽重排过渡:位置平滑移动(TransitionGroup 内置 FLIP),元素外观保持原样 */
.tool-list-move { transition: transform .25s ease; }
/* 拖拽中的被拖项:关闭过渡动画,位置完全由 JS transform 控制(跟随鼠标),
   并抬升层级保证浮在其他项之上;外观(颜色/字号)不变。 */
.nav-item.drag-moving { transition: none !important; z-index: 5; cursor: grabbing; }
.nav-icon { flex-shrink: 0; }
.nav-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.nav-badge { margin-left: auto; }

.bottom {
  flex-shrink: 0;
  border-top: 1px solid #ececef;
  padding: 8px 10px;
}
</style>
