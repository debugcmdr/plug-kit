<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useMessage } from 'naive-ui'

const props = defineProps<{
  pluginId: string
}>()

const message = useMessage()
const loading = ref(false)
const iframeSrc = ref('')
const bridgeVersion = ref('')
const iframeLoaded = ref(false)

onMounted(async () => {
  loading.value = true
  try {
    message.info(`加载插件: ${props.pluginId}`)
    iframeLoaded.value = true
  } finally {
    loading.value = false
  }
})

function handleMessage(event: MessageEvent) {
  if (event.data?.type === 'multitool:version') {
    bridgeVersion.value = event.data.version
  } else if (event.data?.type === 'multitool:response') {
    console.log('Response from plugin:', event.data)
  }
}

window.addEventListener('message', handleMessage)
</script>

<template>
  <n-spin :show="loading">
    <div style="height: calc(100vh - 200px); border-radius: 8px; overflow: hidden; background: #fff;">
      <iframe
        v-if="iframeLoaded"
        :src="iframeSrc || 'about:blank'"
        style="width: 100%; height: 100%; border: none;"
        sandbox="allow-scripts allow-same-origin allow-forms"
      />
      <div v-else style="display: flex; align-items: center; justify-content: center; height: 300px; flex-direction: column; gap: 16px;">
        <n-icon size="48" style="color: var(--n-text-color-3);">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
        </n-icon>
        <n-p style="color: var(--n-text-color-3);">插件界面加载中...</n-p>
      </div>
    </div>
    <div v-if="bridgeVersion" style="margin-top: 8px; color: var(--n-text-color-3); font-size: 12px;">
      Bridge Version: {{ bridgeVersion }}
    </div>
  </n-spin>
</template>
