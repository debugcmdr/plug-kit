<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import Sidebar from './components/Sidebar.vue'
import MainContent from './views/MainContent.vue'

const activePlugin = ref<string | null>(null)
let unlistenProgress: UnlistenFn | null = null

function selectPlugin(target: string) {
  activePlugin.value = target
}

// 转发 Tauri 进度事件(plugkit:task-progress)给对应插件 iframe,
// 以 plugkit:progress postMessage 类型送达(bridge.js onProgress 监听它)。
// 用 onMounted + await:确保 Tauri IPC 就绪后再注册监听(顶层调用可能时序错乱)。
onMounted(async () => {
  // 转发 Tauri 进度事件给对应插件 iframe(plugkit:progress 供 bridge.js onProgress)
  unlistenProgress = await listen('plugkit:task-progress', (event: any) => {
    const { plugin_id, data } = event.payload || {}
    if (!plugin_id) return
    const iframe = document.querySelector(
      `iframe[data-plugin-id="${plugin_id}"]`
    ) as HTMLIFrameElement | null
    iframe?.contentWindow?.postMessage({ type: 'plugkit:progress', data }, '*')
  })
})

onUnmounted(() => {
  unlistenProgress?.()
})

// Window-level bridge relay: plugin iframes postMessage {type:'plugkit:invoke'}
// here; we forward to the Tauri bridge_message command (pure pass-through, no
// business logic in Vue), then relay the response back to the ORIGINATING iframe
// (matched by data-plugin-id — multiple plugin iframes stay mounted now).
window.addEventListener('message', (e) => {
  if (e.data?.type !== 'plugkit:invoke') return
  const { id, command, payload } = e.data
  const source = activePlugin.value
  if (!source) return

  invoke('bridge_message', { pluginId: source, msg: { id, command, payload } })
    .then((res: any) => {
      const iframe = document.querySelector(
        `iframe[data-plugin-id="${source}"]`
      ) as HTMLIFrameElement | null
      iframe?.contentWindow?.postMessage(
        { type: 'plugkit:response', id: res.id, result: res.result },
        '*'
      )
    })
    .catch((err: any) => {
      const iframe = document.querySelector(
        `iframe[data-plugin-id="${source}"]`
      ) as HTMLIFrameElement | null
      iframe?.contentWindow?.postMessage(
        { type: 'plugkit:response', id, result: { Err: String(err) } },
        '*'
      )
    })
})
</script>

<template>
  <n-config-provider>
    <n-message-provider>
      <n-dialog-provider>
        <div style="height: 100vh; display: flex; flex-direction: column;">
          <div style="flex: 1; display: flex; min-height: 0;">
            <Sidebar @select="selectPlugin" />
            <div style="flex: 1; min-width: 0; overflow: hidden;">
              <MainContent :active-plugin="activePlugin" @update:active-plugin="selectPlugin" />
            </div>
          </div>
        </div>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  height: 100%;
  width: 100%;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', sans-serif;
}
</style>
