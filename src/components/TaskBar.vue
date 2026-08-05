<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { tasks, fetchTasks } from '../stores/tasks'

const showPanel = ref(false)

onMounted(fetchTasks)

function togglePanel() {
  showPanel.value = !showPanel.value
}

function formatTime(dateStr: string) {
  return new Date(dateStr).toLocaleTimeString()
}
</script>

<template>
  <div>
    <n-layout-footer
      :bordered="true"
      style="padding: 8px 16px; cursor: pointer;"
      @click="togglePanel"
    >
      <n-space align="center">
        <n-badge :dot="tasks.filter(t => t.status === 'running').length > 0">
          <n-icon size="18">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
          </n-icon>
        </n-badge>
        <span style="font-size: 14px;">
          {{ tasks.filter(t => t.status === 'running').length }} 个任务运行中
        </span>
        <n-button text size="small" @click.stop="togglePanel">
          详情
        </n-button>
      </n-space>
    </n-layout-footer>

    <n-drawer v-model:show="showPanel" :width="400" placement="bottom">
      <n-drawer-content title="任务中心">
        <n-empty v-if="tasks.length === 0" description="暂无任务" />
        <n-timeline v-else>
          <n-timeline-item
            v-for="task in tasks.slice().reverse()"
            :key="task.task_id"
            :title="task.plugin_id"
            :content="`${task.status} · ${task.progress.percent.toFixed(1)}%`"
            :time="formatTime(task.created_at)"
          />
        </n-timeline>
      </n-drawer-content>
    </n-drawer>
  </div>
</template>
