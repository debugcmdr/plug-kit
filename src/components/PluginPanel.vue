<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  pluginId: string
}>()

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
})
</script>

<template>
  <div style="display:flex; flex-direction:column; height:100%;">
    <!-- 加载进度条 -->
    <div v-if="loading" style="height:3px; flex-shrink:0; background:var(--n-primary-color);">
      <div style="height:100%; width:40%; background:var(--n-primary-color); animation:plushkit-pulse 1s ease-in-out infinite;"></div>
    </div>

    <!-- 内容区：iframe 自身控制内边距，容器只负责背景/圆角/溢出 -->
    <div style="flex:1; min-height:0; background:#fff; border-radius:0 0 8px 8px; overflow:hidden;">
      <!-- iframe 容器 -->
      <div v-if="!loadError" style="height:100%;">
        <iframe
          :src="iframeSrc"
          :data-plugin-id="props.pluginId"
          data-plugin
          style="width:100%; height:100%; border:none; display:block;"
          sandbox="allow-scripts allow-same-origin allow-forms"
        />
      </div>
      <!-- 错误状态 -->
      <div v-else style="display:flex; align-items:center; justify-content:center; min-height:200px; flex-direction:column; gap:16px; color:var(--n-text-color-3);">
        <n-icon size="48">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
        </n-icon>
        <p>{{ loadError || '插件界面加载中...' }}</p>
      </div>
    </div>
  </div>
</template>

<style>
@keyframes plushkit-pulse {
  0%, 100% { opacity: 0.4; transform: translateX(0); }
  50% { opacity: 1; transform: translateX(120%); }
}
</style>
