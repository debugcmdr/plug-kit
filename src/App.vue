<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { GlobalThemeOverrides } from 'naive-ui'
import Sidebar from './components/Sidebar.vue'
import MainContent from './views/MainContent.vue'
import { startMarketPoll, startTaskPoll } from './stores/plugins'

// 统一设计 Token(naive-ui 主题覆盖):主色/圆角/柔和阴影,
// 保证外壳各面板视觉一致(品牌蓝 #3370ff)。
const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#3370ff',
    primaryColorHover: '#4a82ff',
    primaryColorPressed: '#2460e8',
    primaryColorSuppl: '#3370ff',
    borderRadius: '10px',
    borderRadiusSmall: '8px',
    fontFamily: "-apple-system, 'PingFang SC', 'Segoe UI', sans-serif",
  },
  Card: {
    borderRadius: '12px',
    boxShadow: '0 1px 3px rgba(0,0,0,.05)',
    borderColor: '#e5e6eb',
  },
  Button: {
    borderRadiusMedium: '8px',
    borderRadiusSmall: '7px',
  },
}

const activePlugin = ref<string | null>(null)
let unlistenProgress: UnlistenFn | null = null
let unlistenDropped: UnlistenFn | null = null

function selectPlugin(target: string) {
  activePlugin.value = target
}

// 转发 Tauri 进度事件(plugkit:task-progress)给对应插件 iframe,
// 以 plugkit:progress postMessage 类型送达(bridge.js onProgress 监听它)。
// 用 onMounted + await:确保 Tauri IPC 就绪后再注册监听(顶层调用可能时序错乱)。
onMounted(async () => {
  // 转发 Tauri 进度事件给对应插件 iframe(plugkit:progress 供 bridge.js onProgress)
  unlistenProgress = await listen('plugkit:task-progress', (event: any) => {
    const { plugin_id, task_id, data } = event.payload || {}
    if (!plugin_id) return
    const iframe = document.querySelector(
      `iframe[data-plugin-id="${plugin_id}"]`
    ) as HTMLIFrameElement | null
    // 保留 task_id 随进度下发给插件:并发任务(多链接下载)时,
    // 插件可据此过滤归属,否则所有任务进度串台(广播无法区分)。
    iframe?.contentWindow?.postMessage(
      { type: 'plugkit:progress', data: { ...(data || {}), task_id } },
      '*'
    )
  })
  // 系统文件拖入窗口:外壳 emit plugkit:files-dropped,按「当前激活插件」转发到其
  // iframe(plugkit:files-dropped 供 bridge.js onFilesDropped 监听)。
  // 业务过滤(扩展名/分类)由插件侧做,外壳/前端不掺和。
  unlistenDropped = await listen('plugkit:files-dropped', (event: any) => {
    const pid = activePlugin.value
    if (!pid || pid === 'home') return
    const iframe = document.querySelector(
      `iframe[data-plugin-id="${pid}"]`
    ) as HTMLIFrameElement | null
    iframe?.contentWindow?.postMessage(
      { type: 'plugkit:files-dropped', paths: event.payload?.paths || [] },
      '*'
    )
  })
  // 启动即后台预取市场清单 + 每 5 分钟轮询,使"打开市场"时数据已就绪,无需原地等待。
  startMarketPoll()
  // 任务数据全局轮询(侧边栏徽标/任务中心共享一份数据):
  // 此前 startTaskPoll 只在任务面板挂载时启动,从未进过任务面板时
  // 侧边栏徽标不随任务开始/结束实时更新。
  startTaskPoll()
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
  unlistenDropped?.()
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

  // 身份校验:消息必须来自「已挂载插件 iframe」之一(MainContent 用 v-show 常驻,
  // 后台隐藏插件仍在跑 JS——只校验激活插件会让后台插件的合法 invoke 被丢弃)。
  // 插件身份由其 DOM 位置(data-plugin-id)决定,消息来源必须是该 iframe 的
  // contentWindow,外部窗口/其他来源无法伪造。
  const frames = document.querySelectorAll<HTMLIFrameElement>('iframe[data-plugin-id]')
  let sourcePlugin: string | null = null
  for (const f of frames) {
    if (f.contentWindow === e.source) { sourcePlugin = f.dataset.pluginId || null; break }
  }
  if (!sourcePlugin) return

  pendingInvokes.set(id, { pluginId: sourcePlugin, ts: Date.now() })
  invoke('bridge_message', { pluginId: sourcePlugin, msg: { id, command, payload } })
    .then((res: any) => {
      relayResponse(res.id ?? id, res.id ?? id, res.result, sourcePlugin!)
    })
    .catch((err: any) => {
      relayResponse(id, id, { Err: String(err) }, sourcePlugin!)
    })
})
</script>

<template>
  <n-config-provider :theme-overrides="themeOverrides">
    <n-message-provider>
      <n-dialog-provider>
        <div style="height: 100vh; display: flex; flex-direction: column;">
          <div style="flex: 1; display: flex; min-height: 0;">
            <Sidebar @select="selectPlugin" />
            <div style="flex: 1; min-width: 0; overflow: hidden; background: #f5f6f8;">
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

/* 全局细滚动条(外壳与插件页共用观感) */
::-webkit-scrollbar { width: 8px; height: 8px; }
::-webkit-scrollbar-thumb { background: #d0d3d9; border-radius: 4px; }
::-webkit-scrollbar-thumb:hover { background: #b8bcc4; }
::-webkit-scrollbar-track { background: transparent; }
</style>
