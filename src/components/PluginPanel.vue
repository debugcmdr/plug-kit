<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  pluginId: string
}>()

const containerRef = ref<HTMLElement | null>(null)
const loading = ref(false)
const loadError = ref('')

const iframeSrc = computed(() =>
  `plugkit://plugin/${props.pluginId}/tool/index.html`
)

async function checkBridge() {
  try {
    const ok = await invoke<boolean>('check_bridge_compat', { required: '>=1.0.0' })
    if (!ok) {
      loadError.value = '此插件需要更新以兼容当前主程序版本'
    }
  } catch (e) {
    loadError.value = `桥接检查失败: ${e}`
  }
}

onMounted(async () => {
  loading.value = true
  try {
    await checkBridge()
  } finally {
    loading.value = false
  }

  // ResizeObserver: 容器尺寸变化时自动调整 iframe 高度
  const el = containerRef.value
  if (!el) return
  const resize = () => {
    const h = el.clientHeight
    const w = el.clientWidth
    console.log(`[PluginPanel:${props.pluginId}] container size: ${w}x${h}`)
    if (h > 0) {
      el.style.height = h + 'px'
    }
  }
  resize() // 立即计算
  const observer = new ResizeObserver(resize)
  observer.observe(el)
  // 窗口 resize 时也更新
  window.addEventListener('resize', resize)
  onBeforeUnmount(() => {
    observer.disconnect()
    window.removeEventListener('resize', resize)
  })
  // 也观察父容器（因为父容器可能才是真正的高度来源）
  const parent = el.parentElement
  if (parent) {
    const parentObserver = new ResizeObserver(() => {
      console.log(`[PluginPanel:${props.pluginId}] parent size: ${parent.clientWidth}x${parent.clientHeight}`)
      resize()
    })
    parentObserver.observe(parent)
    onBeforeUnmount(() => parentObserver.disconnect())
  }
})
</script>

<template>
  <!-- 使用 ref 容器 + ResizeObserver 动态设置高度 -->
  <div ref="containerRef" style="display:flex; flex-direction:column;">
    <!-- 加载状态条 -->
    <div
      v-if="loading"
      style="height:3px; flex-shrink:0; background:var(--n-primary-color); border-radius:2px;"
    >
      <div style="height:100%; width:40%; background:var(--n-primary-color); border-radius:2px; animation:plushkit-pulse 1s ease-in-out infinite;"></div>
    </div>

    <!-- iframe 容器: flex:1 撑满剩余空间 -->
    <div style="flex:1; min-height:0; border-radius:0 0 8px 8px; overflow:hidden; background:#fff;">
      <iframe
        v-if="!loadError"
        :src="iframeSrc"
        :data-plugin-id="props.pluginId"
        data-plugin
        style="width:100%; height:100%; border:none;"
        sandbox="allow-scripts allow-same-origin allow-forms"
      />
      <div
        v-else
        style="display:flex; align-items:center; justify-content:center; height:100%; flex-direction:column; gap:16px; color:var(--n-text-color-3);"
      >
        <n-icon size="48">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
        </n-icon>
        <p>{{ loadError || '插件界面加载中...' }}</p>
      </div>
    </div>
  </div>
  <style>
    @keyframes plushkit-pulse {
      0%, 100% { opacity: 0.4; transform: translateX(0); }
      50% { opacity: 1; transform: translateX(120%); }
    }
  </style>
</template>
