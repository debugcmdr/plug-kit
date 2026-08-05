<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import Sidebar from './components/Sidebar.vue'
import MainContent from './views/MainContent.vue'
import TaskBar from './components/TaskBar.vue'

const activePlugin = ref<string | null>(null)

function selectPlugin(pluginId: string) {
  activePlugin.value = pluginId
}

// Window-level bridge relay: plugin iframes postMessage {type:'plugkit:invoke'}
// here; we forward to the Tauri bridge_message command (pure pass-through, no
// business logic in Vue), then relay the response back to the originating iframe.
window.addEventListener('message', (e) => {
  if (e.data?.type !== 'plugkit:invoke') return
  const { id, command, payload } = e.data
  const source = activePlugin.value
  if (!source) return

  invoke('bridge_message', { pluginId: source, msg: { id, command, payload } })
    .then((res: any) => {
      const iframe = document.querySelector('iframe[data-plugin]') as HTMLIFrameElement | null
      iframe?.contentWindow?.postMessage(
        { type: 'plugkit:response', id: res.id, result: res.result },
        '*'
      )
    })
    .catch((err: any) => {
      const iframe = document.querySelector('iframe[data-plugin]') as HTMLIFrameElement | null
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
        <n-layout has-sider style="height: 100vh;">
          <Sidebar @select="selectPlugin" />
          <n-layout>
            <n-layout-content style="padding: 16px;">
              <MainContent :active-plugin="activePlugin" />
            </n-layout-content>
            <TaskBar />
          </n-layout>
        </n-layout>
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
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}
</style>
