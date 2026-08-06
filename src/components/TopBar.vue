<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { tasks, fetchTasks } from '../stores/tasks'
import { useMessage } from 'naive-ui'
import { invoke } from '@tauri-apps/api/core'

const showPopover = ref(false)
const message = useMessage()

onMounted(fetchTasks)

function formatTime(dateStr: string) {
  return new Date(dateStr).toLocaleTimeString()
}

function statusText(status: string) {
  const map: Record<string, string> = {
    pending: '排队中', running: '进行中', paused: '已暂停',
    completed: '已完成', failed: '失败', cancelled: '已取消', interrupted: '已中断'
  }
  return map[status] ?? status
}

async function cancelTask(taskId: string) {
  try {
    await invoke('bridge_message', {
      pluginId: 'taskbar',
      msg: { id: crypto.randomUUID(), command: 'task_cancel', payload: { task_id: taskId } }
    })
    message.success('已取消任务')
    await fetchTasks()
  } catch (e) {
    message.error(`取消失败: ${e}`)
  }
}
</script>

<template>
  <div style="display: flex; justify-content: space-between; align-items: center; padding: 0 16px; height: 48px; border-bottom: 1px solid var(--n-border-color); background: var(--n-color-1);">
    <div style="display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: 15px;">
      <n-icon size="18" color="#3370ff">
        <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 17l6-6-6-6"></path><path d="M12 19h8"></path></svg>
      </n-icon>
      <span>PlugKit</span>
      <span style="color: var(--n-text-color-3); font-weight: 400; font-size: 12px;">工具市场</span>
    </div>

    <!-- 任务中心下拉(右上角) -->
    <n-popover
      v-model:show="showPopover"
      trigger="click"
      placement="bottom-end"
      :width="380"
    >
      <template #trigger>
        <n-badge :dot="tasks.filter(t => t.status === 'running').length > 0" :offset="[-2, 2]">
          <n-button quaternary circle>
            <n-icon size="18">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
            </n-icon>
          </n-button>
        </n-badge>
      </template>

      <div style="width: 100%;">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
          <strong style="font-size: 14px;">任务中心</strong>
          <span style="font-size: 12px; color: var(--n-text-color-3);">
            {{ tasks.filter(t => t.status === 'running').length }} 进行中
          </span>
        </div>
        <n-empty v-if="tasks.length === 0" description="暂无任务" style="padding: 16px 0;" />
        <div v-else style="max-height: 320px; overflow: auto;">
          <div
            v-for="task in tasks.slice().reverse()"
            :key="task.task_id"
            style="display: flex; justify-content: space-between; align-items: center; padding: 8px 4px; border-bottom: 1px solid var(--n-divider-color);"
          >
            <div style="flex: 1; min-width: 0;">
              <div style="font-size: 13px; display: flex; align-items: center; gap: 6px;">
                <span style="font-weight: 500;">{{ task.plugin_id }}</span>
                <n-tag size="tiny" :type="task.status === 'failed' || task.status === 'cancelled' ? 'error' : task.status === 'completed' ? 'success' : 'info'">
                  {{ statusText(task.status) }}
                </n-tag>
              </div>
              <div style="display: flex; align-items: center; gap: 8px; margin-top: 4px;">
                <n-progress
                  v-if="task.status === 'running' || task.status === 'pending'"
                  type="line"
                  :percentage="Math.min(100, Math.round(task.progress.percent))"
                  :show-indicator="true"
                  :height="4"
                  style="flex: 1;"
                />
                <span v-else style="font-size: 12px; color: var(--n-text-color-3);">
                  {{ task.progress.percent.toFixed(0) }}%
                </span>
                <span style="font-size: 11px; color: var(--n-text-color-3); white-space: nowrap;">
                  {{ task.progress.speed || '' }} {{ task.progress.eta ? '· ' + task.progress.eta : '' }}
                </span>
              </div>
              <div style="font-size: 11px; color: var(--n-text-color-4); margin-top: 2px;">{{ formatTime(task.created_at) }}</div>
            </div>
            <n-button
              v-if="task.status === 'running' || task.status === 'pending'"
              size="tiny"
              quaternary
              type="error"
              @click="cancelTask(task.task_id)"
            >
              取消
            </n-button>
          </div>
        </div>
      </div>
    </n-popover>
  </div>
</template>
