<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import Sidebar from './components/Sidebar.vue'
import MainContent from './views/MainContent.vue'
import { startMarketPoll } from './stores/plugins'

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
  // 启动即后台预取市场清单 + 每 5 分钟轮询,使"打开市场"时数据已就绪,无需原地等待。
  startMarketPoll()
})

onUnmounted(() => {
  unlistenProgress?.()
})

// Window-level bridge relay: plugin iframes postMessage {type:'plugkit:invoke'}
// here; we forward to the Tauri bridge_message command (pure pass-through, no
// business logic in Vue), then relay the response back to the ORIGINATING iframe.
//
// 竞态修复:响应按"请求发出时"的来源插件投递(而非当前激活插件)。
// 否则插件 A 发起长下载期间切到插件 B,响应会送错 iframe,Promise 悬空至超时。
const pendingInvokes = new Map<string, { pluginId: string; ts: number }>()

// 周期性清理超时(与 bridge.js 的 10 分钟 invoke 超时一致)未响应的条目,
// 防止切换插件/异常后 Map 无限增长。
const INVOKE_TIMEOUT_MS = 10 * 60 * 1000
let invokeCleanupTimer: ReturnType<typeof setInterval> | null = null
invokeCleanupTimer = setInterval(() => {
  const now = Date.now()
  for (const [reqId, rec] of pendingInvokes) {
    if (now - rec.ts > INVOKE_TIMEOUT_MS) pendingInvokes.delete(reqId)
  }
}, 60 * 1000)

onUnmounted(() => {
  unlistenProgress?.()
  if (invokeCleanupTimer) clearInterval(invokeCleanupTimer)
})

function relayResponse(reqId: string, resId: string, result: unknown, fallback: string) {
  const rec = pendingInvokes.get(reqId)
  pendingInvokes.delete(reqId)
  const pluginId = rec?.pluginId ?? fallback
  const iframe = document.querySelector(
    `iframe[data-plugin-id="${pluginId}"]`
  ) as HTMLIFrameElement | null
  iframe?.contentWindow?.postMessage(
    { type: 'plugkit:response', id: resId, result },
    '*'
  )
}

window.addEventListener('message', (e) => {
  if (e.data?.type !== 'plugkit:invoke') return
  const { id, command, payload } = e.data
  const source = activePlugin.value
  if (!source) return

  // 身份校验(S1):消息必须来自「当前激活插件」自己的 iframe。
  // 否则恶意/后台 iframe 可伪造 plugkit:invoke 冒充其他插件调用命令、读写其配置。
  // (响应仍按 pendingInvokes 记录的来源插件投递,不依赖此处校验,后台任务不受影响)
  const frame = document.querySelector(
    `iframe[data-plugin-id="${source}"]`
  ) as HTMLIFrameElement | null
  if (!frame || !frame.contentWindow || e.source !== frame.contentWindow) return

  pendingInvokes.set(id, { pluginId: source, ts: Date.now() })
  invoke('bridge_message', { pluginId: source, msg: { id, command, payload } })
    .then((res: any) => {
      relayResponse(res.id ?? id, res.id ?? id, res.result, source)
    })
    .catch((err: any) => {
      relayResponse(id, id, { Err: String(err) }, source)
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
