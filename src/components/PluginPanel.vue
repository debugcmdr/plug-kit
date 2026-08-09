<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  pluginId: string
}>()

const loading = ref(false)
const loadError = ref('')

// Load the plugin's iframe UI from ~/.plugkit/plugins/{id}/tool/index.html via
// the custom plugkit:// protocol (which also injects the bridge SDK).
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
  <n-spin :show="loading">
    <div style="height: 100%; border-radius: 8px; overflow: hidden; background: #fff;">
      <iframe
        v-if="!loadError"
        :src="iframeSrc"
        :data-plugin-id="props.pluginId"
        data-plugin
        style="width: 100%; height: 100%; border: none;"
        sandbox="allow-scripts allow-same-origin allow-forms"
      />
      <div v-else style="display: flex; align-items: center; justify-content: center; height: 300px; flex-direction: column; gap: 16px;">
        <n-icon size="48" style="color: var(--n-text-color-3);">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
        </n-icon>
        <n-p style="color: var(--n-text-color-3);">{{ loadError || '插件界面加载中...' }}</n-p>
      </div>
    </div>
  </n-spin>
</template>
